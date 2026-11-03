//! The mod's menu (M) and its HUD lines, laid out in GUI pixels as draws and
//! clickable rectangles: the game's side only draws them and reports clicks.

use crate::formats::SaveFormat;
use crate::state::{Effect, Entry, Here, LayerMode, State};
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const LINE: f32 = 10.0;
const ROW: f32 = 14.0;
const PAD: f32 = 6.0;
const BTN_H: f32 = 14.0;

const INK: u32 = 0xFFFFFF;
const MUTED: u32 = 0xAAAAAA;
const GOOD: u32 = 0x55FF55;
const BAD: u32 = 0xFF5555;
const WARN: u32 = 0xFFAA00;
const ACCENT: u32 = 0x7FC8FF;
const SHADE: u32 = 0xA000_0000;
const PANEL: u32 = 0xE010_1018;
const PANEL_EDGE: u32 = 0xFF50_5064;
const BUTTON: u32 = 0xFF34_3442;
const BUTTON_HOVER: u32 = 0xFF4E_4E66;
const BUTTON_ON: u32 = 0xFF3C_6E46;
const ROW_BG: u32 = 0x3000_0000;
const ROW_HOVER: u32 = 0x40FF_FFFF;
const FIELD: u32 = 0xFF00_0000;
const FIELD_EDGE: u32 = 0xFF70_7070;
const FIELD_FOCUS: u32 = 0xFFFF_FFFF;
const HUD_BG: u32 = 0x9000_0000;

/// What a hook draws, in GUI pixels: a filled `[x, y, w, h]` in ARGB, a line
/// of text at its top left in RGB (with or without a shadow), or an item's
/// icon at a top left, so many pixels square.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Fill([f32; 4], u32),
    Text(String, [f32; 2], u32, bool),
    Item(String, [f32; 2], f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Load,
    Placements,
    Placement,
    Materials,
    Selection,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Load, Tab::Placements, Tab::Placement, Tab::Materials, Tab::Selection, Tab::Settings];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Load => "Load",
            Tab::Placements => "Placements",
            Tab::Placement => "Placement",
            Tab::Materials => "Materials",
            Tab::Selection => "Save area",
            Tab::Settings => "Settings",
        }
    }
}

/// A click's meaning.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Tab(Tab),
    Up,
    Refresh,
    Enter(PathBuf),
    Load(PathBuf),
    Select(u64),
    Toggle(u64),
    Remove(u64),
    MoveHere,
    Nudge(usize, i32),
    Rotate(bool),
    Mirror,
    Paste,
    CancelPaste,
    MissingOnly,
    Corner(usize),
    ClearSelection,
    FocusName,
    Save(SaveFormat),
    Opacity(i32),
    ShowWrong,
    ShowExtra,
    Range(i32),
    PasteAir,
    PasteEntities,
    LayerMode,
    LayerY(i32),
    LayerHere,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub tab: Tab,
    pub scroll: f32,
    /// The folder browsed, relative to the schematics folder.
    pub browse: PathBuf,
    pub entries: Vec<Entry>,
    pub listed: bool,
    pub confirm_remove: Option<u64>,
    pub name: String,
    pub name_focus: bool,
    pub missing_only: bool,
    /// How far the list the last frame drew scrolls.
    max_scroll: Cell<usize>,
    flash: Option<(String, bool, Instant)>,
}

impl Default for Menu {
    fn default() -> Self {
        Self {
            tab: Tab::Load,
            scroll: 0.0,
            browse: PathBuf::new(),
            entries: Vec::new(),
            listed: false,
            confirm_remove: None,
            name: String::new(),
            name_focus: false,
            missing_only: false,
            max_scroll: Cell::new(0),
            flash: None,
        }
    }
}

impl Menu {
    /// A line shown for a few seconds (menu and HUD).
    pub fn flash(&mut self, text: &str, error: bool) {
        self.flash = Some((text.to_owned(), error, Instant::now()));
    }
    pub fn flashed(&self) -> Option<(&str, bool)> {
        let (t, e, at) = self.flash.as_ref()?;
        (at.elapsed() < Duration::from_secs(5)).then_some((t.as_str(), *e))
    }
}

/// The widgets a frame laid out, for its clicks.
pub type Widgets = Vec<([f32; 4], Action)>;

struct Ui<'a> {
    draws: Vec<Draw>,
    widgets: Widgets,
    mouse: [f32; 2],
    measure: &'a dyn Fn(&str) -> f32,
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

impl Ui<'_> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.draws.push(Draw::Fill(r, argb));
    }
    fn text(&mut self, s: &str, at: [f32; 2], rgb: u32) {
        self.draws.push(Draw::Text(s.to_owned(), at, rgb, true));
    }
    fn width(&self, s: &str) -> f32 {
        (self.measure)(s)
    }
    /// Text cut to `w` pixels with an ellipsis.
    fn fit(&self, s: &str, w: f32) -> String {
        if self.width(s) <= w {
            return s.to_owned();
        }
        let mut out: String = s.to_owned();
        while !out.is_empty() && self.width(&format!("{out}...")) > w {
            out.pop();
        }
        format!("{out}...")
    }
    fn button(&mut self, r: [f32; 4], label: &str, action: Action, on: bool) {
        let hover = inside(r, self.mouse);
        let bg = if on {
            BUTTON_ON
        } else if hover {
            BUTTON_HOVER
        } else {
            BUTTON
        };
        self.fill(r, bg);
        let label = self.fit(label, r[2] - 4.0);
        let w = self.width(&label);
        self.text(&label, [r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - 8.0) / 2.0], INK);
        self.widgets.push((r, action));
    }
    /// Buttons side by side filling `w`, from `x`.
    fn buttons(&mut self, x: f32, y: f32, w: f32, items: Vec<(String, Action, bool)>) {
        let n = items.len().max(1) as f32;
        let gap = 3.0;
        let bw = (w - gap * (n - 1.0)) / n;
        for (i, (label, action, on)) in items.into_iter().enumerate() {
            self.button([x + i as f32 * (bw + gap), y, bw, BTN_H], &label, action, on);
        }
    }
}

/// A count with thousands separated (`109,559`).
pub fn num(n: impl Into<u64>) -> String {
    let digits = n.into().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

/// An item id as a name (`minecraft:oak_log` as "Oak Log").
pub fn pretty(item: &str) -> String {
    let short = item.split_once(':').map_or(item, |(_, s)| s);
    short
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn size_label(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// The panel's box for a window of `size`.
pub fn panel(size: [f32; 2]) -> [f32; 4] {
    let w = (size[0] - 16.0).min(420.0).max(200.0);
    let h = (size[1] - 16.0).min(270.0).max(150.0);
    [((size[0] - w) / 2.0).floor(), ((size[1] - h) / 2.0).floor(), w, h]
}

/// Draws the menu, and the widgets it laid out.
pub fn screen(s: &State, size: [f32; 2], mouse: [f32; 2], here: Option<&Here>, measure: &dyn Fn(&str) -> f32) -> (Vec<Draw>, Widgets) {
    let mut ui = Ui { draws: Vec::new(), widgets: Vec::new(), mouse, measure };
    ui.fill([0.0, 0.0, size[0], size[1]], SHADE);
    let [px, py, pw, ph] = panel(size);
    ui.fill([px - 1.0, py - 1.0, pw + 2.0, ph + 2.0], PANEL_EDGE);
    ui.fill([px, py, pw, ph], PANEL);
    ui.text("Schematics", [px + PAD, py + PAD], ACCENT);
    if let Some((done, total)) = s.paste_progress {
        let t = format!("Pasting {done}/{total}");
        let w = ui.width(&t);
        ui.text(&t, [px + pw - PAD - w, py + PAD], WARN);
    }
    let tabs_y = py + PAD + LINE + 4.0;
    let tabs: Vec<(String, Action, bool)> =
        Tab::ALL.iter().map(|&t| (t.label().to_owned(), Action::Tab(t), t == s.ui.tab)).collect();
    ui.buttons(px + PAD, tabs_y, pw - 2.0 * PAD, tabs);
    let body = [px + PAD, tabs_y + BTN_H + 6.0, pw - 2.0 * PAD, ph - (tabs_y + BTN_H + 6.0 - py) - PAD - LINE - 2.0];
    match s.ui.tab {
        Tab::Load => load_tab(&mut ui, s, body),
        Tab::Placements => placements_tab(&mut ui, s, body),
        Tab::Placement => placement_tab(&mut ui, s, body, here),
        Tab::Materials => materials_tab(&mut ui, s, body),
        Tab::Selection => selection_tab(&mut ui, s, body),
        Tab::Settings => settings_tab(&mut ui, s, body),
    }
    let foot_y = py + ph - PAD - LINE + 2.0;
    if let Some((text, error)) = s.ui.flashed() {
        let t = ui.fit(text, pw - 2.0 * PAD);
        ui.text(&t, [px + PAD, foot_y], if error { BAD } else { GOOD });
    } else if !s.loading.is_empty() {
        ui.text("Loading...", [px + PAD, foot_y], MUTED);
    }
    (ui.draws, ui.widgets)
}

/// Rows of a scrolled list: the first row shown and how many fit (and
/// how far it scrolls, kept for the wheel).
fn rows(menu: &Menu, count: usize, height: f32) -> (usize, usize) {
    let fit = (height / ROW).floor().max(1.0) as usize;
    let max = count.saturating_sub(fit);
    menu.max_scroll.set(max);
    ((menu.scroll.max(0.0) as usize).min(max), fit)
}

fn load_tab(ui: &mut Ui, s: &State, [x, y, w, h]: [f32; 4]) {
    let folder = if s.ui.browse.as_os_str().is_empty() {
        "schematics/".to_owned()
    } else {
        format!("schematics/{}/", s.ui.browse.to_string_lossy())
    };
    let bw = 54.0;
    let label = ui.fit(&folder, w - 2.0 * bw - 12.0);
    ui.text(&label, [x, y + 3.0], MUTED);
    ui.button([x + w - 2.0 * bw - 3.0, y, bw, BTN_H], "Up", Action::Up, false);
    ui.button([x + w - bw, y, bw, BTN_H], "Refresh", Action::Refresh, false);
    let list_y = y + BTN_H + 4.0;
    let list_h = h - BTN_H - 4.0;
    if s.ui.entries.is_empty() {
        ui.text("No schematics here. Put files in this folder:", [x, list_y + 2.0], MUTED);
        let path = ui.fit(&s.root.join(&s.ui.browse).to_string_lossy(), w);
        ui.text(&path, [x, list_y + 2.0 + LINE + 2.0], INK);
        ui.text(".litematic .schem .schematic .nbt .mcstructure .bp", [x, list_y + 2.0 + 2.0 * (LINE + 2.0)], MUTED);
        return;
    }
    let (first, fit) = rows(&s.ui, s.ui.entries.len(), list_h);
    for (i, e) in s.ui.entries.iter().enumerate().skip(first).take(fit) {
        let ry = list_y + (i - first) as f32 * ROW;
        let r = [x, ry, w, ROW - 1.0];
        ui.fill(r, if inside(r, ui.mouse) { ROW_HOVER } else { ROW_BG });
        let (name, info, color) = if e.dir {
            (format!("{}/", e.name), "folder".to_owned(), ACCENT)
        } else {
            let ext = e.name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
            (e.name.clone(), format!(".{ext}  {}", size_label(e.size)), INK)
        };
        let iw = ui.width(&info);
        let name = ui.fit(&name, w - iw - 12.0);
        ui.text(&name, [x + 3.0, ry + 3.0], color);
        ui.text(&info, [x + w - iw - 3.0, ry + 3.0], MUTED);
        let action = if e.dir { Action::Enter(e.path.clone()) } else { Action::Load(e.path.clone()) };
        ui.widgets.push((r, action));
    }
    scrollbar(ui, [x, list_y, w, list_h], first, fit, s.ui.entries.len());
}

fn scrollbar(ui: &mut Ui, [x, y, w, h]: [f32; 4], first: usize, fit: usize, count: usize) {
    if count <= fit {
        return;
    }
    let bar_h = (h * fit as f32 / count as f32).max(8.0);
    let top = y + (h - bar_h) * first as f32 / (count - fit) as f32;
    ui.fill([x + w + 1.0, top, 2.0, bar_h], 0xFF80_8090);
}

fn placements_tab(ui: &mut Ui, s: &State, [x, y, w, h]: [f32; 4]) {
    if s.placements.is_empty() {
        ui.text("No placements yet: load a schematic from the Load tab.", [x, y + 2.0], MUTED);
        return;
    }
    let (first, fit) = rows(&s.ui, s.placements.len(), h);
    for (i, p) in s.placements.iter().enumerate().skip(first).take(fit) {
        let ry = y + (i - first) as f32 * ROW;
        let selected = Some(p.id) == s.selected;
        ui.fill([x, ry, w, ROW - 1.0], if selected { 0x5030_6090 } else { ROW_BG });
        // Wide enough for the longest label, so none is cut.
        let bw = ["Selected", "Select", "Show", "Hide", "Remove", "Sure?"].iter().map(|l| ui.width(l)).fold(0.0, f32::max) + 10.0;
        let buttons_w = 3.0 * bw + 6.0;
        let o = p.origin;
        let info = format!("{} {} {}", o[0], o[1], o[2]);
        let iw = ui.width(&info);
        let name = ui.fit(&p.name, w - buttons_w - iw - 16.0);
        ui.text(&name, [x + 3.0, ry + 3.0], if p.visible { INK } else { MUTED });
        ui.text(&info, [x + w - buttons_w - iw - 6.0, ry + 3.0], MUTED);
        let bx = x + w - buttons_w;
        ui.button([bx, ry, bw, ROW - 1.0], if selected { "Selected" } else { "Select" }, Action::Select(p.id), selected);
        ui.button([bx + bw + 3.0, ry, bw, ROW - 1.0], if p.visible { "Hide" } else { "Show" }, Action::Toggle(p.id), false);
        let sure = s.ui.confirm_remove == Some(p.id);
        ui.button([bx + 2.0 * (bw + 3.0), ry, bw, ROW - 1.0], if sure { "Sure?" } else { "Remove" }, Action::Remove(p.id), sure);
    }
    scrollbar(ui, [x, y, w, h], first, fit, s.placements.len());
}

fn placement_tab(ui: &mut Ui, s: &State, [x, y, w, _h]: [f32; 4], here: Option<&Here>) {
    let Some(p) = s.selected() else {
        ui.text("No placement is selected.", [x, y + 2.0], MUTED);
        return;
    };
    let size = p.schematic.size();
    let o = p.origin;
    let mut ly = y;
    let mut line = |ui: &mut Ui, label: &str, value: String, color: u32| {
        ui.text(label, [x, ly], MUTED);
        let v = ui.fit(&value, w - 70.0);
        ui.text(&v, [x + 70.0, ly], color);
        ly += LINE + 1.0;
    };
    line(ui, "Name", p.name.clone(), INK);
    let file = p.file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    line(ui, "File", format!("{file}  ({})", p.schematic.format.label()), INK);
    line(
        ui,
        "Size",
        format!("{} x {} x {}, {} blocks, {} regions", size[0], size[1], size[2], p.blocks, p.schematic.regions.len()),
        INK,
    );
    line(ui, "Origin", format!("{} {} {}", o[0], o[1], o[2]), INK);
    line(ui, "Rotation", format!("{} deg, mirror {}", p.transform.rotation.degrees(), p.transform.mirror.label()), INK);
    let v = s.verify.get(&p.id).copied().unwrap_or_default();
    let checked = v.correct + v.missing + v.wrong_block + v.wrong_state;
    if checked == 0 {
        line(ui, "In world", "not in the loaded world yet".into(), MUTED);
    } else if v.missing + v.wrong_block + v.wrong_state + v.extra == 0 {
        line(ui, "In world", format!("all {} blocks correct", num(v.correct)), GOOD);
    } else {
        let percent = u64::from(v.correct) * 100 / u64::from(checked.max(1));
        line(ui, "In world", format!("{} of {} correct ({percent}%)", num(v.correct), num(checked)), INK);
        let mut todo = vec![format!("{} missing", num(v.missing)), format!("{} wrong", num(v.wrong_block))];
        if v.wrong_state > 0 {
            todo.push(format!("{} wrong state", num(v.wrong_state)));
        }
        todo.push(format!("{} extra", num(v.extra)));
        line(ui, "To fix", todo.join(", "), WARN);
    }
    ly += 3.0;
    // The origin: to the player, or a block along an axis.
    let label_w = 70.0;
    ui.text("Move", [x, ly + 3.0], MUTED);
    let mut nudges = vec![("To me".to_owned(), Action::MoveHere, false)];
    for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
        nudges.push((format!("{name} -1"), Action::Nudge(axis, -1), false));
        nudges.push((format!("{name} +1"), Action::Nudge(axis, 1), false));
    }
    ui.buttons(x + label_w, ly, w - label_w, nudges);
    ly += BTN_H + 3.0;
    ui.text("Turn", [x, ly + 3.0], MUTED);
    ui.buttons(
        x + label_w,
        ly,
        w - label_w,
        vec![
            ("Left".into(), Action::Rotate(false), false),
            ("Right".into(), Action::Rotate(true), false),
            (format!("Mirror: {}", short_mirror(p.transform.mirror)), Action::Mirror, p.transform.mirror != crate::transform::Mirror::None),
            ((if p.visible { "Hide" } else { "Show" }).into(), Action::Toggle(p.id), !p.visible),
        ],
    );
    ly += BTN_H + 3.0;
    let pasting = s.paste_progress.is_some();
    ui.text("Build", [x, ly + 3.0], MUTED);
    ui.buttons(
        x + label_w,
        ly,
        w - label_w,
        vec![
            if pasting { ("Stop pasting".into(), Action::CancelPaste, true) } else { ("Paste".into(), Action::Paste, false) },
            ("Material list".into(), Action::Tab(Tab::Materials), false),
            (
                (if s.ui.confirm_remove == Some(p.id) { "Sure?" } else { "Remove" }).into(),
                Action::Remove(p.id),
                s.ui.confirm_remove == Some(p.id),
            ),
        ],
    );
    ly += BTN_H + 4.0;
    let hint = if here.is_some() { "Ctrl + wheel with a stick nudges it the way you look." } else { "" };
    ui.text(hint, [x, ly], MUTED);
}

fn short_mirror(m: crate::transform::Mirror) -> &'static str {
    match m {
        crate::transform::Mirror::None => "none",
        crate::transform::Mirror::LeftRight => "Z",
        crate::transform::Mirror::FrontBack => "X",
    }
}

fn materials_tab(ui: &mut Ui, s: &State, [x, y, w, h]: [f32; 4]) {
    let Some(p) = s.selected() else {
        ui.text("No placement is selected.", [x, y + 2.0], MUTED);
        return;
    };
    let title = ui.fit(&format!("Materials for {}", p.name), w - 110.0);
    ui.text(&title, [x, y + 3.0], INK);
    let label = if s.ui.missing_only { "Missing only" } else { "All blocks" };
    ui.button([x + w - 100.0, y, 100.0, BTN_H], label, Action::MissingOnly, s.ui.missing_only);
    let head_y = y + BTN_H + 3.0;
    let list: Vec<_> = s.materials.iter().filter(|m| !s.ui.missing_only || m.missing > 0).collect();
    // Number columns, right-aligned, as wide as their widest entry.
    let heads = ["Total", "Missing", "Stacks", "Have"];
    let cell = |m: &crate::state::MaterialRow, i: usize| match i {
        0 => num(m.needed),
        1 => num(m.missing),
        2 => num(m.missing.div_ceil(64)),
        _ => num(m.have),
    };
    let mut widths = heads.map(|h| ui.width(h));
    for m in &list {
        for (i, w) in widths.iter_mut().enumerate() {
            *w = w.max(ui.width(&cell(m, i)));
        }
    }
    let gap = 10.0;
    let mut right = [0.0f32; 4];
    let mut edge = x + w - 4.0;
    for i in (0..4).rev() {
        right[i] = edge;
        edge -= widths[i] + gap;
    }
    let name_w = edge - (x + 20.0);
    ui.text("Item", [x + 20.0, head_y], MUTED);
    for (i, t) in heads.iter().enumerate() {
        ui.text(t, [right[i] - ui.width(t), head_y], MUTED);
    }
    let rows_y = head_y + LINE + 2.0;
    if list.is_empty() {
        let t = if s.materials.is_empty() { "Counting..." } else { "Nothing is missing in the loaded area." };
        ui.text(t, [x, rows_y + 2.0], MUTED);
        return;
    }
    let (first, fit) = rows(&s.ui, list.len(), h - (rows_y - y));
    for (i, m) in list.iter().enumerate().skip(first).take(fit) {
        let ry = rows_y + (i - first) as f32 * ROW;
        ui.fill([x, ry, w, ROW - 1.0], ROW_BG);
        ui.draws.push(Draw::Item(m.item.clone(), [x + 2.0, ry], 12.0));
        let name = ui.fit(&pretty(&m.item), name_w);
        ui.text(&name, [x + 20.0, ry + 3.0], INK);
        let enough = m.have >= m.missing;
        let colors = [INK, if m.missing == 0 { GOOD } else { WARN }, MUTED, if enough { GOOD } else { BAD }];
        for i in 0..4 {
            let t = cell(m, i);
            ui.text(&t, [right[i] - ui.width(&t), ry + 3.0], colors[i]);
        }
    }
    scrollbar(ui, [x, rows_y, w, h - (rows_y - y)], first, fit, list.len());
}

fn selection_tab(ui: &mut Ui, s: &State, [x, y, w, _h]: [f32; 4]) {
    let mut ly = y;
    ui.text("Hold a stick: left click a block for corner 1, right click for corner 2.", [x, ly], MUTED);
    ly += LINE + 4.0;
    for i in 0..2 {
        let label = match s.selection.corners[i] {
            Some(c) => format!("Corner {}: {} {} {}", i + 1, c[0], c[1], c[2]),
            None => format!("Corner {}: not set", i + 1),
        };
        ui.text(&label, [x, ly + 3.0], if s.selection.corners[i].is_some() { INK } else { MUTED });
        ui.button([x + w - 90.0, ly, 90.0, BTN_H], "Set to my feet", Action::Corner(i), false);
        ly += BTN_H + 3.0;
    }
    let size = match s.selection.size() {
        Some(sz) => format!("Size: {} x {} x {} ({} blocks)", sz[0], sz[1], sz[2], sz.iter().map(|&v| v as u64).product::<u64>()),
        None => "Size: set both corners".into(),
    };
    ui.text(&size, [x, ly + 2.0], INK);
    ly += LINE + 6.0;
    ui.text("Name", [x, ly + 3.0], MUTED);
    let field = [x + 40.0, ly, w - 40.0, BTN_H];
    ui.fill([field[0] - 1.0, field[1] - 1.0, field[2] + 2.0, field[3] + 2.0], if s.ui.name_focus { FIELD_FOCUS } else { FIELD_EDGE });
    ui.fill(field, FIELD);
    let caret = if s.ui.name_focus { "_" } else { "" };
    let shown = if s.ui.name.is_empty() && !s.ui.name_focus { "click to type a name".to_owned() } else { format!("{}{caret}", s.ui.name) };
    let shown = ui.fit(&shown, field[2] - 6.0);
    ui.text(&shown, [field[0] + 3.0, field[1] + 3.0], if s.ui.name.is_empty() && !s.ui.name_focus { MUTED } else { INK });
    ui.widgets.push((field, Action::FocusName));
    ly += BTN_H + 5.0;
    let ready = s.selection.bounds().is_some();
    ui.buttons(
        x,
        ly,
        w,
        vec![
            ("Save .litematic".into(), Action::Save(SaveFormat::Litematic), false),
            ("Save .schem".into(), Action::Save(SaveFormat::Sponge), false),
            ("Save .nbt".into(), Action::Save(SaveFormat::Structure), false),
            ("Clear".into(), Action::ClearSelection, false),
        ],
    );
    ly += BTN_H + 4.0;
    if !ready {
        ui.text("Set both corners to save.", [x, ly], MUTED);
    } else {
        ui.text("Saved into the schematics folder.", [x, ly], MUTED);
    }
}

fn settings_tab(ui: &mut Ui, s: &State, [x, y, w, _h]: [f32; 4]) {
    let st = &s.settings;
    let mut ly = y;
    let label_w = 120.0;
    let mut row = |ui: &mut Ui, label: &str, items: Vec<(String, Action, bool)>| {
        ui.text(label, [x, ly + 3.0], MUTED);
        ui.buttons(x + label_w, ly, w - label_w, items);
        ly += BTN_H + 3.0;
    };
    row(
        ui,
        "Ghost opacity",
        vec![
            ("-".into(), Action::Opacity(-10), false),
            (format!("{}%", st.opacity), Action::Opacity(0), false),
            ("+".into(), Action::Opacity(10), false),
        ],
    );
    row(
        ui,
        "Layers",
        vec![
            (st.layers.mode.label().into(), Action::LayerMode, st.layers.mode != LayerMode::All),
            ("Y-".into(), Action::LayerY(-1), false),
            (if st.layers.mode == LayerMode::All { "y -".into() } else { format!("y {}", st.layers.y) }, Action::LayerHere, false),
            ("Y+".into(), Action::LayerY(1), false),
        ],
    );
    row(ui, "Wrong blocks", vec![(on_off(st.show_wrong).into(), Action::ShowWrong, st.show_wrong)]);
    row(ui, "Extra blocks", vec![(on_off(st.show_extra).into(), Action::ShowExtra, st.show_extra)]);
    row(
        ui,
        "Render range",
        vec![
            ("-".into(), Action::Range(-1), false),
            (format!("{} chunks", st.range), Action::Range(0), false),
            ("+".into(), Action::Range(1), false),
        ],
    );
    row(ui, "Paste air", vec![(on_off(st.paste_air).into(), Action::PasteAir, st.paste_air)]);
    row(ui, "Paste entities", vec![(on_off(st.paste_entities).into(), Action::PasteEntities, st.paste_entities)]);
    ly += 2.0;
    ui.text("Keys: M menu, Page Up / Down layers (Options > Mods > Schematics).", [x, ly], MUTED);
}

/// The widget under the pointer.
pub fn hit(widgets: &Widgets, at: [f32; 2]) -> Option<Action> {
    widgets.iter().rev().find(|(r, _)| inside(*r, at)).map(|(_, a)| a.clone())
}

/// A click on the menu.
pub fn act(s: &mut State, action: Action, here: Option<&Here>) -> Vec<Effect> {
    let mut fx = Vec::new();
    if !matches!(action, Action::Remove(_)) {
        s.ui.confirm_remove = None;
    }
    if !matches!(action, Action::FocusName) {
        s.ui.name_focus = false;
    }
    match action {
        Action::Tab(t) => {
            if s.ui.tab != t {
                s.ui.scroll = 0.0;
            }
            s.ui.tab = t;
            if t == Tab::Load {
                s.list_dir();
            }
        }
        Action::Up => {
            s.ui.browse.pop();
            s.ui.scroll = 0.0;
            s.list_dir();
        }
        Action::Refresh => s.list_dir(),
        Action::Enter(path) => {
            if let Ok(rel) = path.strip_prefix(&s.root) {
                s.ui.browse = rel.to_path_buf();
                s.ui.scroll = 0.0;
                s.list_dir();
            }
        }
        Action::Load(path) => match here {
            Some(h) => s.load(path, h.block(), &h.dimension),
            None => s.fail("Join a world to place a schematic"),
        },
        Action::Select(id) => {
            s.selected = Some(id);
            s.materials.clear();
        }
        Action::Toggle(id) => {
            if let Some(p) = s.placements.iter_mut().find(|p| p.id == id) {
                p.visible = !p.visible;
                p.rev += 1;
            }
            s.save_world();
        }
        Action::Remove(id) => {
            if s.ui.confirm_remove == Some(id) {
                s.ui.confirm_remove = None;
                s.remove(id);
            } else {
                s.ui.confirm_remove = Some(id);
            }
        }
        Action::MoveHere => {
            if let (Some(h), Some(p)) = (here, s.selected_mut()) {
                p.move_to(h.block());
            }
            s.save_world();
        }
        Action::Nudge(axis, by) => {
            if let Some(p) = s.selected_mut() {
                let mut d = [0; 3];
                d[axis.min(2)] = by;
                p.move_by(d);
            }
            s.save_world();
        }
        Action::Rotate(cw) => {
            if let Some(p) = s.selected_mut() {
                p.rotate(cw);
            }
            s.save_world();
        }
        Action::Mirror => {
            if let Some(p) = s.selected_mut() {
                p.mirror();
            }
            s.save_world();
        }
        Action::Paste => {
            if let Some(id) = s.selected {
                fx.push(Effect::Paste(id));
            }
        }
        Action::CancelPaste => fx.push(Effect::CancelPaste),
        Action::MissingOnly => {
            s.ui.missing_only = !s.ui.missing_only;
            s.ui.scroll = 0.0;
        }
        Action::Corner(i) => {
            if let Some(h) = here {
                s.set_corner(i, h.block(), &h.dimension);
            }
        }
        Action::ClearSelection => s.selection.corners = [None, None],
        Action::FocusName => s.ui.name_focus = true,
        Action::Save(format) => {
            let name = s.ui.name.trim().to_owned();
            if s.selection.bounds().is_none() {
                s.fail("Set both corners first");
            } else if name.is_empty() {
                s.ui.name_focus = true;
                s.fail("Type a name first");
            } else {
                fx.push(Effect::Save { name, format });
            }
        }
        Action::Opacity(by) => {
            s.settings.opacity = (s.settings.opacity as i32 + by).clamp(10, 100) as u32;
            s.render_rev += 1;
            fx.push(Effect::SaveSettings);
        }
        Action::ShowWrong => {
            s.settings.show_wrong = !s.settings.show_wrong;
            s.render_rev += 1;
            fx.push(Effect::SaveSettings);
        }
        Action::ShowExtra => {
            s.settings.show_extra = !s.settings.show_extra;
            s.render_rev += 1;
            fx.push(Effect::SaveSettings);
        }
        Action::Range(by) => {
            s.settings.range = (s.settings.range as i32 + by).clamp(2, 32) as u32;
            fx.push(Effect::SaveSettings);
        }
        Action::PasteAir => {
            s.settings.paste_air = !s.settings.paste_air;
            fx.push(Effect::SaveSettings);
        }
        Action::PasteEntities => {
            s.settings.paste_entities = !s.settings.paste_entities;
            fx.push(Effect::SaveSettings);
        }
        Action::LayerMode => {
            let mut l = s.settings.layers;
            l.mode = l.mode.next();
            if l.mode == LayerMode::Single
                && let Some(h) = here
            {
                l.y = h.block()[1];
            }
            s.set_layers(l);
            fx.push(Effect::SaveSettings);
        }
        Action::LayerY(by) => {
            let mut l = s.settings.layers;
            if l.mode == LayerMode::All {
                // From all layers, the buttons start at the player's.
                l.mode = LayerMode::Single;
                l.y = here.map_or(l.y, |h| h.block()[1]);
            } else {
                l.y += by;
            }
            s.set_layers(l);
            fx.push(Effect::SaveSettings);
        }
        Action::LayerHere => {
            if let Some(h) = here {
                let mut l = s.settings.layers;
                if l.mode == LayerMode::All {
                    l.mode = LayerMode::Single;
                }
                l.y = h.block()[1];
                s.set_layers(l);
                fx.push(Effect::SaveSettings);
            }
        }
    }
    fx
}

/// The wheel over the menu.
pub fn scroll(s: &mut State, by: f64) {
    let max = match s.ui.tab {
        Tab::Load | Tab::Placements | Tab::Materials => s.ui.max_scroll.get(),
        _ => 0,
    } as f32;
    s.ui.scroll = (s.ui.scroll.floor() - (by.signum() * by.abs().ceil()) as f32 * 2.0).clamp(0.0, max);
}

/// A key on the menu: typing into the name field; true when the key was the
/// field's.
pub fn key(s: &mut State, key: &str) -> (bool, Vec<Effect>) {
    if !s.ui.name_focus {
        return (false, Vec::new());
    }
    match key {
        "key.keyboard.backspace" => {
            s.ui.name.pop();
        }
        "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
            let fx = act(s, Action::Save(SaveFormat::Litematic), None);
            return (true, fx);
        }
        "key.keyboard.escape" => s.ui.name_focus = false,
        _ => {}
    }
    (true, Vec::new())
}

pub fn typed(s: &mut State, c: char) {
    if s.ui.name_focus && !c.is_control() && s.ui.name.chars().count() < 48 {
        s.ui.name.push(c);
    }
}

/// The HUD lines while playing: the selected placement and what is left.
pub fn hud(s: &State) -> Vec<(String, u32)> {
    let mut lines = Vec::new();
    if let Some((done, total)) = s.paste_progress {
        lines.push((format!("Schematics: pasting {done}/{total}"), WARN));
    }
    if let Some(p) = s.selected().filter(|p| p.visible) {
        let v = s.verify.get(&p.id).copied().unwrap_or_default();
        let todo = v.missing + v.wrong_block + v.wrong_state;
        let state = if v.correct + todo == 0 {
            String::new()
        } else if todo + v.extra == 0 {
            "  done".into()
        } else {
            format!("  {} missing, {} wrong, {} extra", num(v.missing), num(v.wrong_block + v.wrong_state), num(v.extra))
        };
        lines.push((format!("{}{}", p.name, state), if todo + v.extra == 0 && v.correct > 0 { GOOD } else { INK }));
        if s.settings.layers.mode != LayerMode::All {
            lines.push((format!("Layer: {} (y {})", s.settings.layers.mode.label(), s.settings.layers.y), MUTED));
        }
    }
    if let Some((text, error)) = s.ui.flashed() {
        lines.push((text.to_owned(), if error { BAD } else { GOOD }));
    }
    lines
}

/// The HUD box: its lines on a dark background, from a top left.
pub fn hud_draws(lines: &[(String, u32)], at: [f32; 2], measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
    let mut out = Vec::new();
    if lines.is_empty() {
        return out;
    }
    let w = lines.iter().map(|(t, _)| measure(t)).fold(0.0, f32::max) + 6.0;
    let h = lines.len() as f32 * LINE + 4.0;
    out.push(Draw::Fill([at[0], at[1], w, h], HUD_BG));
    for (i, (t, c)) in lines.iter().enumerate() {
        out.push(Draw::Text(t.clone(), [at[0] + 3.0, at[1] + 2.0 + i as f32 * LINE], *c, true));
    }
    out
}

/// The HUD box's size for `lines`.
pub fn hud_size(lines: &[(String, u32)], measure: &dyn Fn(&str) -> f32) -> [f32; 2] {
    let w = lines.iter().map(|(t, _)| measure(t)).fold(0.0, f32::max) + 6.0;
    [w, lines.len() as f32 * LINE + 4.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measure(t: &str) -> f32 {
        t.chars().count() as f32 * 6.0
    }

    #[test]
    fn every_tab_lays_out_and_clicks_land() {
        let dir = std::env::temp_dir().join(format!("schematics-ui-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sub"));
        let _ = std::fs::write(dir.join("a.schem"), b"x");
        let mut s = State::new(dir.clone());
        s.list_dir();
        assert_eq!(s.ui.entries.len(), 2);
        assert!(s.ui.entries[0].dir);
        let here = Here { pos: [0.5, 70.0, 0.5], yaw: 0.0, pitch: 0.0, dimension: "minecraft:overworld".into() };
        for tab in Tab::ALL {
            s.ui.tab = tab;
            let (draws, widgets) = screen(&s, [480.0, 270.0], [0.0, 0.0], Some(&here), &measure);
            assert!(!draws.is_empty());
            assert!(widgets.len() >= 6, "{tab:?}");
        }
        s.ui.tab = Tab::Load;
        let (_, widgets) = screen(&s, [480.0, 270.0], [0.0, 0.0], Some(&here), &measure);
        let folder = widgets.iter().find(|(_, a)| matches!(a, Action::Enter(_))).unwrap();
        let at = [folder.0[0] + 1.0, folder.0[1] + 1.0];
        let action = hit(&widgets, at).unwrap();
        act(&mut s, action, Some(&here));
        assert_eq!(s.ui.browse, PathBuf::from("sub"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_items() {
        assert_eq!(pretty("minecraft:oak_log"), "Oak Log");
        assert_eq!(num(0u32), "0");
        assert_eq!(num(109_559u32), "109,559");
        assert_eq!(num(1_234_567u64), "1,234,567");
    }
}
