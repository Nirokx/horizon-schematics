//! The mod's state: placements of loaded schematics, the area selection,
//! render layers and settings, what the menus and commands do to them, and
//! what they ask of the game ([`Effect`]).

use crate::formats::{self, SaveFormat};
use crate::nbt::Compound;
use crate::schematic::{BlockState, Region, Schematic};
use crate::transform::{Mirror, Rotation, Transform};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

/// A schematic placed in the world.
#[derive(Clone, Debug)]
pub struct Placement {
    pub id: u64,
    pub name: String,
    pub file: PathBuf,
    pub schematic: Arc<Schematic>,
    /// Its blocks other than air (counted once: a large schematic holds millions).
    pub blocks: usize,
    /// Where the schematic's corner 0 0 0 is.
    pub origin: [i32; 3],
    pub transform: Transform,
    pub visible: bool,
    pub dimension: String,
    /// Bumped by every change (moving, turning), and by turning alone.
    pub rev: u64,
    pub turn_rev: u64,
}

impl Placement {
    /// A schematic position as a world position.
    pub fn world(&self, p: [i32; 3]) -> [i32; 3] {
        let t = self.transform.apply(p);
        [self.origin[0] + t[0], self.origin[1] + t[1], self.origin[2] + t[2]]
    }
    /// A world position as a schematic position.
    pub fn local(&self, w: [i32; 3]) -> [i32; 3] {
        self.transform.invert([w[0] - self.origin[0], w[1] - self.origin[1], w[2] - self.origin[2]])
    }
    /// A region's box in the world (inclusive corners).
    pub fn region_box(&self, r: &Region) -> ([i32; 3], [i32; 3]) {
        let a = self.world(r.pos);
        let b = self.world([r.pos[0] + r.size[0] - 1, r.pos[1] + r.size[1] - 1, r.pos[2] + r.size[2] - 1]);
        (
            [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
            [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
        )
    }
    /// The box of all regions in the world.
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        for r in &self.schematic.regions {
            let (a, b) = self.region_box(r);
            for i in 0..3 {
                lo[i] = lo[i].min(a[i]);
                hi[i] = hi[i].max(b[i]);
            }
        }
        (lo, hi)
    }
    /// The region and palette index of the schematic block at world `w`.
    pub fn at(&self, w: [i32; 3]) -> Option<(usize, u32)> {
        let p = self.local(w);
        for (i, r) in self.schematic.regions.iter().enumerate() {
            if let Some(at) = r.index(p[0] - r.pos[0], p[1] - r.pos[1], p[2] - r.pos[2]) {
                return Some((i, r.blocks[at]));
            }
        }
        None
    }
    fn touched(&mut self, turned: bool) {
        self.rev += 1;
        if turned {
            self.turn_rev += 1;
        }
    }
    pub fn move_by(&mut self, d: [i32; 3]) {
        for a in 0..3 {
            self.origin[a] += d[a];
        }
        self.touched(false);
    }
    pub fn move_to(&mut self, origin: [i32; 3]) {
        self.origin = origin;
        self.touched(false);
    }
    /// Turns the placement about its box's centre, so it stays where it is.
    pub fn rotate(&mut self, cw: bool) {
        let (lo, hi) = self.bounds();
        self.transform.rotation = if cw { self.transform.rotation.cw() } else { self.transform.rotation.ccw() };
        self.recentre(lo, hi);
    }
    pub fn mirror(&mut self) {
        let (lo, hi) = self.bounds();
        self.transform.mirror = self.transform.mirror.next();
        self.recentre(lo, hi);
    }
    fn recentre(&mut self, lo: [i32; 3], hi: [i32; 3]) {
        let (nlo, nhi) = self.bounds();
        for a in [0, 2] {
            // Keep the lowest corner where the box's centre allows.
            let was = lo[a] + hi[a];
            let now = nlo[a] + nhi[a];
            self.origin[a] += (was - now).div_euclid(2);
        }
        self.touched(true);
    }
    /// The transformed palette of each region.
    pub fn palettes(&self) -> Vec<Vec<BlockState>> {
        self.schematic.regions.iter().map(|r| r.palette.iter().map(|s| self.transform.state(s)).collect()).collect()
    }
}

/// Whether property `key` of `block` is one the world works out from the
/// block's neighbours, so a block placed by hand differs from the
/// schematic's there without being wrong: a leaf's distance from a log, a
/// wire's power, scaffolding's distance and bottom.
pub fn derived_property(block: &str, key: &str) -> bool {
    let block = block.strip_prefix("minecraft:").unwrap_or(block);
    match key {
        "distance" => block.ends_with("_leaves") || block == "scaffolding",
        "bottom" => block == "scaffolding",
        "power" => block == "redstone_wire",
        _ => false,
    }
}

/// Which layers of the placements show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LayerMode {
    #[default]
    All,
    Single,
    Below,
    Above,
}

impl LayerMode {
    pub fn next(self) -> Self {
        match self {
            LayerMode::All => LayerMode::Single,
            LayerMode::Single => LayerMode::Below,
            LayerMode::Below => LayerMode::Above,
            LayerMode::Above => LayerMode::All,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            LayerMode::All => "All",
            LayerMode::Single => "Single layer",
            LayerMode::Below => "All below",
            LayerMode::Above => "All above",
        }
    }
    fn key(self) -> &'static str {
        match self {
            LayerMode::All => "all",
            LayerMode::Single => "single",
            LayerMode::Below => "below",
            LayerMode::Above => "above",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "single" => LayerMode::Single,
            "below" => LayerMode::Below,
            "above" => LayerMode::Above,
            _ => LayerMode::All,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Layers {
    pub mode: LayerMode,
    pub y: i32,
}

impl Layers {
    pub fn shows(&self, y: i32) -> bool {
        match self.mode {
            LayerMode::All => true,
            LayerMode::Single => y == self.y,
            LayerMode::Below => y <= self.y,
            LayerMode::Above => y >= self.y,
        }
    }
    /// The heights shown, as an inclusive range.
    pub fn range(&self) -> (i32, i32) {
        match self.mode {
            LayerMode::All => (i32::MIN, i32::MAX),
            LayerMode::Single => (self.y, self.y),
            LayerMode::Below => (i32::MIN, self.y),
            LayerMode::Above => (self.y, i32::MAX),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The ghost blocks' opacity, 10 to 100 (percent).
    pub opacity: u32,
    pub show_wrong: bool,
    pub show_extra: bool,
    /// How far placements draw, in 16-block sections.
    pub range: u32,
    pub paste_air: bool,
    pub paste_entities: bool,
    pub layers: Layers,
}

impl Default for Settings {
    fn default() -> Self {
        Self { opacity: 60, show_wrong: true, show_extra: true, range: 8, paste_air: false, paste_entities: true, layers: Layers::default() }
    }
}

impl Settings {
    pub fn to_text(&self) -> String {
        format!(
            "opacity={}\nshow_wrong={}\nshow_extra={}\nrange={}\npaste_air={}\npaste_entities={}\nlayer_mode={}\nlayer_y={}\n",
            self.opacity,
            self.show_wrong,
            self.show_extra,
            self.range,
            self.paste_air,
            self.paste_entities,
            self.layers.mode.key(),
            self.layers.y
        )
    }
    pub fn from_text(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "true";
            match k {
                "opacity" => s.opacity = v.parse().unwrap_or(60).clamp(10, 100),
                "show_wrong" => s.show_wrong = b,
                "show_extra" => s.show_extra = b,
                "range" => s.range = v.parse().unwrap_or(8).clamp(2, 32),
                "paste_air" => s.paste_air = b,
                "paste_entities" => s.paste_entities = b,
                "layer_mode" => s.layers.mode = LayerMode::parse(v),
                "layer_y" => s.layers.y = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        s
    }
}

/// The area selection: two corners, in one dimension.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub corners: [Option<[i32; 3]>; 2],
    pub dimension: String,
}

impl Selection {
    pub fn bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let (Some(a), Some(b)) = (self.corners[0], self.corners[1]) else { return None };
        Some(([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]))
    }
    pub fn size(&self) -> Option<[i32; 3]> {
        let (lo, hi) = self.bounds()?;
        Some([hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1])
    }
}

/// Where the player is, as the hooks read it.
#[derive(Clone, Debug, PartialEq)]
pub struct Here {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub dimension: String,
}

impl Here {
    pub fn block(&self) -> [i32; 3] {
        self.pos.map(|v| v.floor() as i32)
    }
    /// The axis the player looks along most, as a unit step.
    pub fn look_step(&self) -> [i32; 3] {
        let (yaw, pitch) = (f64::from(self.yaw).to_radians(), f64::from(self.pitch).to_radians());
        let d = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
        let a = (0..3).max_by(|&i, &j| d[i].abs().total_cmp(&d[j].abs())).unwrap_or(0);
        let mut step = [0; 3];
        step[a] = if d[a] < 0.0 { -1 } else { 1 };
        step
    }
}

/// One row of the material list.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialRow {
    pub item: String,
    pub needed: u32,
    /// Placed wrong or not at all, in the loaded part of the world.
    pub missing: u32,
    /// In the player's inventory.
    pub have: u32,
}

/// What the loaded part of the world holds of a placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verify {
    pub correct: u32,
    pub missing: u32,
    pub wrong_block: u32,
    pub wrong_state: u32,
    pub extra: u32,
}

impl Verify {
    pub fn add(&mut self, o: &Verify) {
        self.correct += o.correct;
        self.missing += o.missing;
        self.wrong_block += o.wrong_block;
        self.wrong_state += o.wrong_state;
        self.extra += o.extra;
    }
}

/// What the game is asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Paste placement `id` into the world.
    Paste(u64),
    /// Read the selection from the world and save it.
    Save { name: String, format: SaveFormat },
    /// Stop a paste in progress.
    CancelPaste,
    /// The settings changed: keep them.
    SaveSettings,
    /// Close the menu.
    Close,
    /// Open the menu on its current tab.
    Open,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// A file or folder of the schematics folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    pub size: u64,
}

/// A schematic being read on its own thread.
pub struct Loading {
    pub file: PathBuf,
    pub origin: [i32; 3],
    pub dimension: String,
    pub restore: Option<(Transform, bool, String)>,
    rx: Receiver<Result<Schematic, String>>,
}

pub struct State {
    pub root: PathBuf,
    pub placements: Vec<Placement>,
    pub selected: Option<u64>,
    pub selection: Selection,
    pub settings: Settings,
    /// Bumped whenever what draws changes (layers, opacity, overlays).
    pub render_rev: u64,
    pub verify: HashMap<u64, Verify>,
    pub materials: Vec<MaterialRow>,
    pub materials_for: Option<u64>,
    pub ui: crate::ui::Menu,
    pub loading: Vec<Loading>,
    /// The world or server whose placements these are, and where they are kept.
    pub world: Option<String>,
    pub world_file: Option<PathBuf>,
    pub paste_progress: Option<(usize, usize)>,
    next_id: u64,
    messages: Vec<Message>,
}

impl State {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            placements: Vec::new(),
            selected: None,
            selection: Selection::default(),
            settings: Settings::default(),
            render_rev: 0,
            verify: HashMap::new(),
            materials: Vec::new(),
            materials_for: None,
            ui: crate::ui::Menu::default(),
            loading: Vec::new(),
            world: None,
            world_file: None,
            paste_progress: None,
            next_id: 1,
            messages: Vec::new(),
        }
    }

    /// Messages for chat since the last call.
    pub fn take_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.messages)
    }
    pub fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.ui.flash(&text, false);
        self.messages.push(Message { text, error: false });
    }
    pub fn fail(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.ui.flash(&text, true);
        self.messages.push(Message { text, error: true });
    }

    pub fn selected(&self) -> Option<&Placement> {
        self.placements.iter().find(|p| Some(p.id) == self.selected)
    }
    pub fn selected_mut(&mut self) -> Option<&mut Placement> {
        let id = self.selected?;
        self.placements.iter_mut().find(|p| p.id == id)
    }
    pub fn placement(&self, id: u64) -> Option<&Placement> {
        self.placements.iter().find(|p| p.id == id)
    }

    /// Starts reading `file` on its own thread; it is placed at `origin`
    /// once read.
    pub fn load(&mut self, file: PathBuf, origin: [i32; 3], dimension: &str) {
        self.load_with(file, origin, dimension, None);
    }
    fn load_with(&mut self, file: PathBuf, origin: [i32; 3], dimension: &str, restore: Option<(Transform, bool, String)>) {
        let (tx, rx) = channel();
        let path = file.clone();
        let spawned = std::thread::Builder::new().name("schematics-load".into()).spawn(move || {
            let _ = tx.send(formats::load(&path).map_err(|e| e.to_string()));
        });
        if spawned.is_err() {
            self.fail("Couldn't start reading the schematic");
            return;
        }
        if restore.is_none() {
            self.ui.flash(&format!("Loading {}...", formats::stem(&file)), false);
        }
        self.loading.push(Loading { file, origin, dimension: dimension.to_owned(), restore, rx });
    }

    /// Places the schematics read since the last call.
    pub fn poll_loads(&mut self) {
        let mut done = Vec::new();
        self.loading.retain(|l| match l.rx.try_recv() {
            Ok(r) => {
                done.push((l.file.clone(), l.origin, l.dimension.clone(), l.restore.clone(), r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for (file, origin, dimension, restore, result) in done {
            match result {
                Ok(s) => {
                    let id = self.next_id;
                    self.next_id += 1;
                    let restored = restore.is_some();
                    let (transform, visible, name) = restore.unwrap_or((Transform::default(), true, s.name.clone()));
                    let blocks = s.block_count();
                    let size = s.size();
                    let format = s.format.label();
                    self.placements.push(Placement {
                        id,
                        name,
                        file: file.clone(),
                        schematic: Arc::new(s),
                        blocks,
                        origin,
                        transform,
                        visible,
                        dimension,
                        rev: 1,
                        turn_rev: 1,
                    });
                    if !restored {
                        self.selected = Some(id);
                        self.say(format!(
                            "Placed {} ({format}, {}x{}x{}, {blocks} blocks) at {} {} {}",
                            formats::stem(&file),
                            size[0],
                            size[1],
                            size[2],
                            origin[0],
                            origin[1],
                            origin[2]
                        ));
                        self.ui.tab = crate::ui::Tab::Placement;
                        self.save_world();
                    } else if self.selected.is_none() {
                        self.selected = Some(id);
                    }
                }
                Err(e) => self.fail(format!("Couldn't load {}: {e}", formats::stem(&file))),
            }
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.placements.retain(|p| p.id != id);
        self.verify.remove(&id);
        if self.selected == Some(id) {
            self.selected = self.placements.last().map(|p| p.id);
        }
        self.save_world();
    }

    /// Lists the folder the menu browses.
    pub fn list_dir(&mut self) {
        let dir = self.root.join(&self.ui.browse);
        let _ = std::fs::create_dir_all(&self.root);
        let mut entries: Vec<Entry> = Vec::new();
        if let Ok(read) = std::fs::read_dir(&dir) {
            for e in read.flatten() {
                let path = e.path();
                let Ok(meta) = e.metadata() else { continue };
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if meta.is_dir() {
                    entries.push(Entry { name, path, dir: true, size: 0 });
                } else if formats::is_schematic(&path) {
                    entries.push(Entry { name, path, dir: false, size: meta.len() });
                }
            }
        }
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        self.ui.entries = entries;
        self.ui.listed = true;
    }

    /// Placements are kept per world: forget these and read that world's.
    pub fn switch_world(&mut self, world: Option<String>, file: Option<PathBuf>) {
        if self.world == world {
            return;
        }
        self.placements.clear();
        self.loading.clear();
        self.verify.clear();
        self.selected = None;
        self.selection = Selection::default();
        self.materials.clear();
        self.materials_for = None;
        self.world = world;
        self.world_file = file.clone();
        let Some(file) = file else { return };
        let Ok(text) = std::fs::read_to_string(&file) else { return };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 9 {
                continue;
            }
            let path = PathBuf::from(f[0]);
            let path = if path.is_absolute() { path } else { self.root.join(path) };
            let (Ok(x), Ok(y), Ok(z)) = (f[2].parse(), f[3].parse(), f[4].parse()) else { continue };
            let rotation = Rotation::parse(f[5]).unwrap_or_default();
            let mirror = Mirror::parse(f[6]).unwrap_or_default();
            let restore = (Transform::new(rotation, mirror), f[7] == "true", f[1].to_owned());
            self.load_with(path, [x, y, z], f[8], Some(restore));
        }
    }

    /// Keeps this world's placements.
    pub fn save_world(&mut self) {
        let Some(file) = &self.world_file else { return };
        let mut text = String::from("schematics-placements\t1\n");
        for p in &self.placements {
            let path = p.file.strip_prefix(&self.root).unwrap_or(&p.file);
            let clean = |s: &str| s.replace(['\t', '\n'], " ");
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                path.to_string_lossy(),
                clean(&p.name),
                p.origin[0],
                p.origin[1],
                p.origin[2],
                p.transform.rotation.name(),
                p.transform.mirror.name(),
                p.visible,
                p.dimension
            ));
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, text);
    }

    pub fn set_layers(&mut self, layers: Layers) {
        self.settings.layers = layers;
        self.render_rev += 1;
    }
    /// Moves the layer up or down (Page Up / Page Down).
    pub fn step_layer(&mut self, by: i32, here: Option<&Here>) -> Vec<Effect> {
        let mut layers = self.settings.layers;
        if layers.mode == LayerMode::All {
            layers.mode = LayerMode::Single;
            layers.y = here.map_or(0, |h| h.block()[1]);
        } else {
            layers.y += by;
        }
        self.set_layers(layers);
        self.ui.flash(&format!("Layer: {} (y {})", layers.mode.label(), layers.y), false);
        vec![Effect::SaveSettings]
    }

    /// Nudges the selected placement one block along the look (Ctrl and the
    /// wheel, with the tool).
    pub fn nudge(&mut self, here: &Here, forward: bool) {
        let mut step = here.look_step();
        if !forward {
            step = step.map(|v| -v);
        }
        let Some(p) = self.selected_mut() else { return };
        p.move_by(step);
        let o = p.origin;
        self.ui.flash(&format!("Origin {} {} {}", o[0], o[1], o[2]), false);
        self.save_world();
    }

    /// A corner of the area selection at `pos`.
    pub fn set_corner(&mut self, corner: usize, pos: [i32; 3], dimension: &str) {
        if self.selection.dimension != dimension {
            self.selection = Selection { corners: [None, None], dimension: dimension.to_owned() };
        }
        self.selection.corners[corner.min(1)] = Some(pos);
        let size = self.selection.size().map(|s| format!(" ({}x{}x{})", s[0], s[1], s[2])).unwrap_or_default();
        self.ui.flash(&format!("Corner {}: {} {} {}{size}", corner + 1, pos[0], pos[1], pos[2]), false);
    }

    /// A `/schematics` command (without the name). Answers go to chat.
    pub fn command(&mut self, args: &str, here: Option<&Here>) -> Vec<Effect> {
        let mut words = args.split_whitespace();
        let sub = words.next().unwrap_or("help").to_ascii_lowercase();
        let rest: Vec<&str> = words.collect();
        let mut fx = Vec::new();
        match sub.as_str() {
            "menu" | "open" => fx.push(Effect::Open),
            "list" => {
                self.ui.browse = PathBuf::new();
                self.list_dir();
                let files: Vec<String> = self.ui.entries.iter().filter(|e| !e.dir).map(|e| e.name.clone()).collect();
                if files.is_empty() {
                    self.say(format!("No schematics in {}", self.root.display()));
                } else {
                    self.say(format!("Schematics: {}", files.join(", ")));
                }
            }
            "load" => {
                let name = rest.join(" ");
                if name.is_empty() {
                    self.fail("Usage: /schematics load <file>");
                } else if let Some(h) = here {
                    match self.find_file(&name) {
                        Some(path) => self.load(path, h.block(), &h.dimension),
                        None => self.fail(format!("No schematic named {name} in {}", self.root.display())),
                    }
                }
            }
            "placements" => {
                let lines: Vec<String> = self
                    .placements
                    .iter()
                    .map(|p| {
                        let mark = if Some(p.id) == self.selected { "*" } else { "" };
                        format!("{mark}{} at {} {} {}", p.name, p.origin[0], p.origin[1], p.origin[2])
                    })
                    .collect();
                if lines.is_empty() {
                    self.say("No placements");
                } else {
                    self.say(lines.join("; "));
                }
            }
            "move" | "rotate" | "mirror" | "remove" | "paste" | "hide" | "show" => {
                if self.selected().is_none() {
                    self.fail("No placement is selected");
                    return fx;
                }
                match sub.as_str() {
                    "move" => {
                        let nums: Vec<i32> = rest.iter().filter_map(|w| w.parse().ok()).collect();
                        let target = match (nums.len(), here) {
                            (3, _) => Some([nums[0], nums[1], nums[2]]),
                            (0, Some(h)) => Some(h.block()),
                            _ => None,
                        };
                        match target {
                            Some(t) => {
                                if let Some(p) = self.selected_mut() {
                                    p.move_to(t);
                                }
                                self.say(format!("Moved to {} {} {}", t[0], t[1], t[2]));
                                self.save_world();
                            }
                            None => self.fail("Usage: /schematics move [x y z]"),
                        }
                    }
                    "rotate" => {
                        let ccw = rest.first().is_some_and(|w| w.starts_with("ccw") || *w == "left");
                        if let Some(p) = self.selected_mut() {
                            p.rotate(!ccw);
                        }
                        self.save_world();
                    }
                    "mirror" => {
                        if let Some(p) = self.selected_mut() {
                            p.mirror();
                        }
                        self.save_world();
                    }
                    "remove" => {
                        if let Some(id) = self.selected {
                            self.remove(id);
                        }
                    }
                    "paste" => fx.push(Effect::Paste(self.selected.unwrap_or(0))),
                    "hide" | "show" => {
                        let on = sub == "show";
                        if let Some(p) = self.selected_mut() {
                            p.visible = on;
                            p.touched(false);
                        }
                        self.save_world();
                    }
                    _ => {}
                }
            }
            "cancel" => fx.push(Effect::CancelPaste),
            "pos1" | "pos2" => {
                if let Some(h) = here {
                    self.set_corner(if sub == "pos1" { 0 } else { 1 }, h.block(), &h.dimension);
                    let pos = h.block();
                    self.say(format!("Corner {} at {} {} {}", if sub == "pos1" { 1 } else { 2 }, pos[0], pos[1], pos[2]));
                }
            }
            "save" => {
                let (format, name_words): (SaveFormat, Vec<&str>) = match rest.last().and_then(|w| SaveFormat::parse(w)) {
                    Some(f) if rest.len() > 1 => (f, rest[..rest.len() - 1].to_vec()),
                    _ => (SaveFormat::Litematic, rest.clone()),
                };
                let name = name_words.join(" ");
                if name.is_empty() {
                    self.fail("Usage: /schematics save <name> [litematic|schem|nbt]");
                } else if self.selection.bounds().is_none() {
                    self.fail("Select an area first: /schematics pos1 and pos2, or a stick's left and right clicks");
                } else {
                    fx.push(Effect::Save { name, format });
                }
            }
            "layer" => {
                let mode = rest.first().copied().unwrap_or("");
                let mut layers = self.settings.layers;
                layers.mode = match mode {
                    "single" => LayerMode::Single,
                    "below" => LayerMode::Below,
                    "above" => LayerMode::Above,
                    _ => LayerMode::All,
                };
                layers.y = rest.get(1).and_then(|y| y.parse().ok()).or(here.map(|h| h.block()[1])).unwrap_or(layers.y);
                self.set_layers(layers);
                self.say(format!("Layers: {} (y {})", layers.mode.label(), layers.y));
                fx.push(Effect::SaveSettings);
            }
            _ => self.say(
                "/schematics: menu, list, load <file>, placements, move [x y z], rotate [cw|ccw], mirror, show, hide, remove, \
                 paste, cancel, pos1, pos2, save <name> [litematic|schem|nbt], layer <all|single|below|above> [y]",
            ),
        }
        fx
    }

    /// A schematic file by name (with or without extension, in a subfolder).
    pub fn find_file(&self, name: &str) -> Option<PathBuf> {
        let direct = self.root.join(name);
        if direct.is_file() && formats::is_schematic(&direct) {
            return Some(direct);
        }
        let want = name.to_lowercase();
        let mut found = None;
        walk(&self.root, 0, &mut |path| {
            let file = path.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
            let stem = formats::stem(path).to_lowercase();
            if found.is_none() && (file == want || stem == want) {
                found = Some(path.to_path_buf());
            }
        });
        found
    }

    /// The selection's blocks as a schematic, from `read` (a world block
    /// state and its block entity at a position).
    pub fn selection_schematic(
        &self,
        name: &str,
        author: &str,
        mut read: impl FnMut([i32; 3]) -> (BlockState, Option<Compound>),
    ) -> Option<Schematic> {
        let (lo, _) = self.selection.bounds()?;
        let size = self.selection.size()?;
        let mut region = Region::new(name, [0; 3], size);
        for y in 0..size[1] {
            for z in 0..size[2] {
                for x in 0..size[0] {
                    let (state, be) = read([lo[0] + x, lo[1] + y, lo[2] + z]);
                    if state.is_air() {
                        continue;
                    }
                    region.set(x, y, z, state);
                    if let Some(be) = be {
                        region.block_entities.insert([x, y, z], be);
                    }
                }
            }
        }
        let mut s = Schematic::new(name, crate::schematic::Format::Litematic, vec![region]);
        s.author = author.to_owned();
        Some(s)
    }
}

fn walk(dir: &Path, depth: u32, f: &mut dyn FnMut(&Path)) {
    if depth > 6 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for e in read.flatten() {
        let path = e.path();
        if path.is_dir() {
            walk(&path, depth + 1, f);
        } else if formats::is_schematic(&path) {
            f(&path);
        }
    }
}

/// What pasting a placement puts where: the transformed palette, the blocks
/// (lowest layer first), block entities and entities, in world positions.
pub struct PastePlan {
    pub palette: Vec<BlockState>,
    pub blocks: Vec<([i32; 3], u32)>,
    pub block_entities: Vec<([i32; 3], Compound)>,
    pub entities: Vec<([f64; 3], Compound)>,
}

pub fn paste_plan(p: &Placement, layers: &Layers, air: bool, entities: bool) -> PastePlan {
    let mut palette: Vec<BlockState> = Vec::new();
    let mut index: HashMap<BlockState, u32> = HashMap::new();
    let mut blocks = Vec::new();
    let mut block_entities = Vec::new();
    let mut ents = Vec::new();
    for r in &p.schematic.regions {
        let map: Vec<u32> = r
            .palette
            .iter()
            .map(|s| {
                let t = p.transform.state(s);
                *index.entry(t.clone()).or_insert_with(|| {
                    palette.push(t);
                    (palette.len() - 1) as u32
                })
            })
            .collect();
        let air_ids: Vec<bool> = r.palette.iter().map(BlockState::is_air).collect();
        for y in 0..r.size[1] {
            for z in 0..r.size[2] {
                for x in 0..r.size[0] {
                    let v = r.at(x, y, z) as usize;
                    if air_ids[v] && !air {
                        continue;
                    }
                    let w = p.world([r.pos[0] + x, r.pos[1] + y, r.pos[2] + z]);
                    if !layers.shows(w[1]) {
                        continue;
                    }
                    blocks.push((w, map[v]));
                    if let Some(be) = r.block_entities.get(&[x, y, z]) {
                        block_entities.push((w, be.clone()));
                    }
                }
            }
        }
        if entities {
            for e in &r.entities {
                let local = [e.pos[0] + f64::from(r.pos[0]), e.pos[1] + f64::from(r.pos[1]), e.pos[2] + f64::from(r.pos[2])];
                let t = p.transform.apply_f(local);
                let w = [t[0] + f64::from(p.origin[0]), t[1] + f64::from(p.origin[1]), t[2] + f64::from(p.origin[2])];
                if !layers.shows(w[1].floor() as i32) {
                    continue;
                }
                let mut nbt = e.nbt.clone();
                for k in ["UUID", "UUIDMost", "UUIDLeast", "Pos", "Motion"] {
                    nbt.remove(k);
                }
                if let Some(crate::nbt::Tag::List(rot)) = nbt.get_mut("Rotation")
                    && let Some(crate::nbt::Tag::Float(yaw)) = rot.first_mut()
                {
                    *yaw = p.transform.yaw(*yaw);
                }
                ents.push((w, nbt));
            }
        }
    }
    blocks.sort_by_key(|(w, _)| w[1]);
    PastePlan { palette, blocks, block_entities, entities: ents }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schematic::Format;

    fn placement(size: [i32; 3]) -> Placement {
        let mut r = Region::new("r", [0; 3], size);
        r.set(0, 0, 0, BlockState::new("stone"));
        Placement {
            id: 1,
            name: "p".into(),
            file: PathBuf::from("p.litematic"),
            schematic: Arc::new(Schematic::new("p", Format::Litematic, vec![r])),
            blocks: 1,
            origin: [10, 64, 10],
            transform: Transform::default(),
            visible: true,
            dimension: "minecraft:overworld".into(),
            rev: 1,
            turn_rev: 1,
        }
    }

    #[test]
    fn turning_keeps_the_box_in_place() {
        let mut p = placement([5, 2, 3]);
        let (lo, hi) = p.bounds();
        assert_eq!((lo, hi), ([10, 64, 10], [14, 65, 12]));
        for _ in 0..4 {
            p.rotate(true);
            let (a, b) = p.bounds();
            let centre = |l: [i32; 3], h: [i32; 3]| [(l[0] + h[0]) as f32 / 2.0, (l[2] + h[2]) as f32 / 2.0];
            let (c0, c1) = (centre(lo, hi), centre(a, b));
            assert!((c0[0] - c1[0]).abs() <= 1.0 && (c0[1] - c1[1]).abs() <= 1.0);
        }
        assert_eq!(p.bounds(), (lo, hi));
        assert_eq!(p.at([10, 64, 10]), Some((0, 1)));
    }

    #[test]
    fn world_and_local_are_inverse() {
        let mut p = placement([4, 4, 4]);
        p.rotate(true);
        p.mirror();
        for w in [[10, 64, 10], [12, 65, 9], [7, 66, 13]] {
            assert_eq!(p.world(p.local(w)), w);
        }
    }

    #[test]
    fn derived_properties() {
        assert!(derived_property("minecraft:oak_leaves", "distance"));
        assert!(derived_property("minecraft:redstone_wire", "power"));
        assert!(!derived_property("minecraft:oak_stairs", "facing"));
    }

    #[test]
    fn settings_round_trip() {
        let mut s = Settings::default();
        s.opacity = 40;
        s.layers = Layers { mode: LayerMode::Below, y: -12 };
        s.paste_air = true;
        assert_eq!(Settings::from_text(&s.to_text()), s);
    }

    #[test]
    fn layers_and_paste_plans() {
        let p = placement([2, 3, 1]);
        let plan = paste_plan(&p, &Layers::default(), false, true);
        assert_eq!(plan.blocks.len(), 1);
        let plan = paste_plan(&p, &Layers::default(), true, true);
        assert_eq!(plan.blocks.len(), 6);
        let only = Layers { mode: LayerMode::Single, y: 65 };
        assert_eq!(paste_plan(&p, &only, true, true).blocks.len(), 2);
    }

    #[test]
    fn looks_along_the_strongest_axis() {
        let h = |yaw, pitch| Here { pos: [0.0; 3], yaw, pitch, dimension: String::new() };
        assert_eq!(h(0.0, 0.0).look_step(), [0, 0, 1]);
        assert_eq!(h(90.0, 0.0).look_step(), [-1, 0, 0]);
        assert_eq!(h(0.0, 80.0).look_step(), [0, -1, 0]);
    }

    #[test]
    fn commands_need_a_selection_to_save() {
        let mut s = State::new(std::env::temp_dir());
        let here = Here { pos: [1.5, 64.0, 2.5], yaw: 0.0, pitch: 0.0, dimension: "minecraft:overworld".into() };
        assert!(s.command("save house", Some(&here)).is_empty());
        s.command("pos1", Some(&here));
        s.set_corner(1, [4, 66, 5], "minecraft:overworld");
        assert_eq!(s.command("save house schem", Some(&here)), vec![Effect::Save { name: "house".into(), format: SaveFormat::Sponge }]);
        assert_eq!(s.selection.size(), Some([4, 3, 4]));
    }
}
