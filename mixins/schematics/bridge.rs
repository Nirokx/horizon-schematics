// Schematics for Horizon's game side, added to the game as
// `game::schematics_bridge`: the engine's block states as the game's, the
// ghost meshes of the placements near the player (with what the world
// already holds checked block by block), pasting and saving on the single
// player server thread (or through commands on a server), and the material
// list. Only this module knows Horizon's types; the engine crate has the
// formats, the placements and the menus.

use crate::game::Game;
use horizon_assets::model::Direction;
use horizon_client::mesher::Vertex;
use horizon_client_render::schematics_ghost::{self as ghost, FLAT, Mesh};
use horizon_world::registry::{BlockRegistry, BlockStateId};
use schematics_engine::formats::{self, SaveFormat};
use schematics_engine::nbt::{Compound, Tag};
use schematics_engine::schematic::{BlockState, Format, Region, Schematic};
use schematics_engine::state::{self as st, Effect, Here, Placement, State, Verify};
use schematics_engine::{materials, nbt, snbt, ui};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: &str = "schematics";
pub const NAME: &str = "Schematics";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str = "Schematics in the world: M opens the menu to load .litematic, .schem, .schematic, .nbt, \
    .mcstructure and .bp files, place, turn and mirror them, see what is missing or wrong, paste them and save areas.";
pub const KEY_MENU: &str = "menu";
pub const KEY_LAYER_UP: &str = "layer_up";
pub const KEY_LAYER_DOWN: &str = "layer_down";
pub const DEFAULT_MENU: &str = "key.keyboard.m";
pub const DEFAULT_LAYER_UP: &str = "key.keyboard.page.up";
pub const DEFAULT_LAYER_DOWN: &str = "key.keyboard.page.down";
/// The tool: corners of the area selection, and Ctrl + wheel to nudge.
pub const TOOL: &str = "minecraft:stick";

/// `Level.setBlock` flags of a paste: clients told, no shape updates, so
/// every state lands as the schematic has it.
const PASTE_FLAGS: i32 = 2 | 16;
/// Blocks pasted per server tick at most, and the time they may take.
const PASTE_BLOCKS: usize = 16384;
const PASTE_TIME: Duration = Duration::from_millis(25);
/// Commands sent per client tick when pasting on a server.
const COMMANDS_PER_TICK: usize = 16;
/// The meshing budget of a frame.
const MESH_TIME: Duration = Duration::from_millis(4);
/// The most blocks saved at once.
const SAVE_MAX: u64 = 16 * 1024 * 1024;

// Colours of the overlays, 0xRRGGBB, with their alpha.
const WRONG_BLOCK: (u32, u8) = (0xFF3030, 0x78);
const WRONG_STATE: (u32, u8) = (0xFFA020, 0x78);
const EXTRA: (u32, u8) = (0xFF40FF, 0x60);
const UNKNOWN: (u32, u8) = (0x9030FF, 0x90);
const WATER: u32 = 0x3F76E4;
const LAVA: u32 = 0xFF6A00;

/// The engine's state and the game's caches of it, on the window's thread.
pub(super) struct Mod {
    pub s: State,
    started: bool,
    resolved: HashMap<u64, Resolved>,
    sections: HashMap<(u64, [i32; 3]), Section>,
    models: HashMap<BlockStateId, Arc<[Face]>>,
    /// The sections in range, nearest first, and what they were worked out from.
    wanted: Vec<(u64, [i32; 3])>,
    wanted_sig: u64,
    /// Some wanted section isn't built yet.
    stale: bool,
    /// The world's revision and chunk count last seen, and the built
    /// sections still to check against it (from `check` on).
    world_sig: u64,
    checks_left: usize,
    check: usize,
    verify_dirty: bool,
    lines_sig: u64,
    needed: HashMap<u64, Vec<Vec<u32>>>,
    materials_at: Option<Instant>,
    commands: Option<CommandPaste>,
    pending: Vec<Effect>,
    open: bool,
    close: bool,
    data_dir: PathBuf,
}

struct Resolved {
    turn_rev: u64,
    /// The transformed palette of each region, and the game's state for each.
    states: Vec<Vec<BlockState>>,
    ids: Vec<Vec<Option<BlockStateId>>>,
}

struct Section {
    rev: u64,
    sig: u64,
    verify: Verify,
    /// Correct blocks by region and palette index, for the material list.
    correct: HashMap<(usize, u32), u32>,
}

thread_local! {
    static MOD: RefCell<Option<Mod>> = const { RefCell::new(None) };
}

pub(super) fn with<R>(f: impl FnOnce(&mut Mod) -> R) -> R {
    MOD.with(|m| {
        let mut m = m.borrow_mut();
        let m = m.get_or_insert_with(|| Mod {
            s: State::new(Game::game_dir().join("schematics")),
            started: false,
            resolved: HashMap::new(),
            sections: HashMap::new(),
            models: HashMap::new(),
            wanted: Vec::new(),
            wanted_sig: 0,
            stale: false,
            world_sig: 0,
            checks_left: 0,
            check: 0,
            verify_dirty: false,
            lines_sig: 0,
            needed: HashMap::new(),
            materials_at: None,
            commands: None,
            pending: Vec::new(),
            open: false,
            close: false,
            data_dir: PathBuf::new(),
        });
        f(m)
    })
}

/// Work for the single player server thread, and what it says back.
enum ServerJob {
    Paste(ServerPaste),
    Save(SaveJob),
}

struct ServerPaste {
    dimension: String,
    palette: Vec<BlockState>,
    ids: Option<Vec<Option<BlockStateId>>>,
    blocks: Vec<([i32; 3], u32)>,
    block_entities: Vec<([i32; 3], String)>,
    entities: Vec<String>,
    at: usize,
}

struct SaveJob {
    dimension: String,
    lo: [i32; 3],
    hi: [i32; 3],
    name: String,
    format: SaveFormat,
    dir: PathBuf,
    author: String,
}

static SERVER: Mutex<Option<ServerJob>> = Mutex::new(None);
static OUTBOX: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
static PROGRESS: Mutex<Option<(usize, usize)>> = Mutex::new(None);

fn tell(text: String, error: bool) {
    OUTBOX.lock().unwrap_or_else(|e| e.into_inner()).push((text, error));
}

/// Pasting on a server: commands, a few each tick.
struct CommandPaste {
    commands: Vec<String>,
    at: usize,
}

fn here(game: &Game<'_>) -> Option<Here> {
    let p = game.native_player()?;
    Some(Here { pos: [p.x, p.y, p.z], yaw: p.yaw, pitch: p.pitch, dimension: p.dimension.to_owned() })
}

/// The world or server the placements belong to (as waypoints names them).
fn world_key(game: &Game<'_>, current: Option<&str>) -> Option<String> {
    game.native_player()?;
    if let (None, Some(remote)) = (&game.integrated, &game.remote) {
        return Some(format!("server-{}", remote.session.address.trim().to_lowercase()));
    }
    if let Some(folder) = game.world_icon.as_ref().and_then(|icon| icon.path.parent()?.file_name()) {
        return Some(format!("world-{}", folder.to_string_lossy()));
    }
    let current = current.filter(|w| w.starts_with("world-")).map(str::to_owned);
    let named = game.integrated.as_ref().map(|i| format!("world-{}", i.world_name));
    current.or(named).or_else(|| Some("world".into()))
}

fn file_name(key: &str) -> String {
    key.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// The game's state for an engine state: its name (or the name it has
/// since been given), with each property the block has; a value it can't
/// take keeps the default. None for a block the game doesn't know.
fn resolve(registry: &BlockRegistry, s: &BlockState) -> Option<BlockStateId> {
    let default = registry.default_state(&s.name).or_else(|| registry.default_state(materials::renamed(&s.name)?))?;
    let mut state = registry.state(default)?.clone();
    for (k, v) in &s.props {
        let Some(slot) = state.properties.get_mut(k.as_str()) else { continue };
        let old = std::mem::replace(slot, v.clone());
        if registry.id(&state).is_none()
            && let Some(slot) = state.properties.get_mut(k.as_str())
        {
            *slot = old;
        }
    }
    registry.id(&state).or(Some(default))
}

/// The engine's state for a game state.
fn engine_state(registry: &BlockRegistry, id: BlockStateId) -> BlockState {
    let Some(s) = registry.state(id) else { return BlockState::air() };
    let mut out = BlockState::new(&s.name);
    for (k, v) in s.properties.iter() {
        out.set(k, v);
    }
    out
}

/// A block entity's tag (without id and position) as the engine's, with
/// its id added back.
fn block_entity(game: &Game<'_>, pos: [i32; 3]) -> Option<Compound> {
    let tag = game.block_entity_tag(pos)?;
    let bytes = horizon_world::nbt::write_network(&tag, horizon_world::nbt::Limits::saved()).ok()?;
    let Tag::Compound(mut c) = nbt::read_unnamed(&bytes).ok()? else { return None };
    let id = game.res.facts().get(game.world.block(pos[0], pos[1], pos[2])).name.to_string();
    c.0.insert(0, ("id".into(), Tag::String(be_id(&id))));
    Some(c)
}

/// The block entity type of a block (`minecraft:oak_sign` is a `sign`).
fn be_id(block: &str) -> String {
    let short = block.strip_prefix("minecraft:").unwrap_or(block);
    let kind = if short.ends_with("_hanging_sign") || short.ends_with("_wall_hanging_sign") {
        "hanging_sign"
    } else if short.ends_with("_sign") {
        "sign"
    } else if short.ends_with("_banner") {
        "banner"
    } else if short.ends_with("_bed") {
        "bed"
    } else if short.ends_with("shulker_box") {
        "shulker_box"
    } else if short.ends_with("_skull") || short.ends_with("_head") {
        "skull"
    } else if short.starts_with("potted_") || short == "flower_pot" {
        return block.to_owned();
    } else if short == "trapped_chest" {
        "trapped_chest"
    } else if short.ends_with("command_block") {
        "command_block"
    } else {
        short
    };
    format!("minecraft:{kind}")
}

// ---------------------------------------------------------------------------
// Once a frame on the window.

pub(super) fn frame(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    let on = game.native_on(ID);
    let dir = game.native_data_dir(ID);
    with(|m| {
        if !m.started {
            m.started = true;
            m.data_dir = dir.clone();
            if let Ok(text) = std::fs::read_to_string(dir.join("settings.txt")) {
                m.s.settings = st::Settings::from_text(&text);
            }
            let _ = std::fs::create_dir_all(&m.s.root);
        }
        let key = world_key(game, m.s.world.as_deref());
        if key != m.s.world {
            let file = key.as_ref().map(|k| dir.join("placements").join(format!("{}.txt", file_name(k))));
            m.s.switch_world(key, file);
            m.sections.clear();
            m.wanted.clear();
            m.wanted_sig = 0;
            m.resolved.clear();
            m.needed.clear();
            m.commands = None;
            ghost::clear();
        }
        m.s.poll_loads();
        for (text, error) in std::mem::take(&mut *OUTBOX.lock().unwrap_or_else(|e| e.into_inner())) {
            if error {
                m.s.fail(text);
            } else {
                m.s.say(text);
            }
        }
        m.s.paste_progress = *PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = &m.commands {
            m.s.paste_progress = Some((c.at, c.commands.len()));
        }
        for msg in m.s.take_messages() {
            game.native_chat(&format!("[Schematics] {}", msg.text));
        }
        let fx = std::mem::take(&mut m.pending);
        run(game, m, fx);
        if std::mem::take(&mut m.close) {
            game.native_mod_screen_close(ID);
        }
        if std::mem::take(&mut m.open) && game.native_mod_screen_open(ID) && m.s.ui.tab == ui::Tab::Load {
            m.s.list_dir();
        }
        let Some(here) = here(game).filter(|_| on) else {
            ghost::clear();
            m.sections.clear();
            m.wanted_sig = 0;
            return;
        };
        let game = &*game;
        update_ghosts(game, m, &here);
        update_lines(m, &here);
        let menu_open = game.native_mod_screen() == Some(ID);
        let due = m.materials_at.is_none_or(|t| t.elapsed() > Duration::from_millis(750));
        if menu_open && matches!(m.s.ui.tab, ui::Tab::Materials) && due {
            m.materials_at = Some(Instant::now());
            materials(game, m);
        }
    });
}

/// What the engine asked for.
fn run(game: &mut Game<'_>, m: &mut Mod, fx: Vec<Effect>) {
    for e in fx {
        match e {
            Effect::Open => m.open = true,
            Effect::Close => m.close = true,
            Effect::SaveSettings => {
                let _ = std::fs::create_dir_all(&m.data_dir);
                let _ = std::fs::write(m.data_dir.join("settings.txt"), m.s.settings.to_text());
            }
            Effect::CancelPaste => {
                let mut job = SERVER.lock().unwrap_or_else(|e| e.into_inner());
                if matches!(*job, Some(ServerJob::Paste(_))) {
                    *job = None;
                }
                *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = None;
                m.commands = None;
                m.s.say("Paste stopped");
            }
            Effect::Paste(id) => paste(game, m, id),
            Effect::Save { name, format } => save(game, m, name, format),
        }
    }
}

fn paste(game: &mut Game<'_>, m: &mut Mod, id: u64) {
    let Some(p) = m.s.placement(id) else { return };
    let busy = SERVER.lock().unwrap_or_else(|e| e.into_inner()).is_some() || m.commands.is_some();
    if busy {
        m.s.fail("A paste or a save is still running (/schematics cancel stops it)");
        return;
    }
    let settings = &m.s.settings;
    let plan = st::paste_plan(p, &settings.layers, settings.paste_air, settings.paste_entities);
    let total = plan.blocks.len();
    let dimension = p.dimension.clone();
    let summons: Vec<String> = plan
        .entities
        .iter()
        .filter_map(|(pos, nbt)| {
            let mut nbt = nbt.clone();
            let id = nbt.remove("id")?.as_str()?.to_owned();
            Some(format!(
                "execute in {dimension} run summon {id} {:.3} {:.3} {:.3} {}",
                pos[0],
                pos[1],
                pos[2],
                snbt::to_snbt(&Tag::Compound(nbt))
            ))
        })
        .collect();
    if game.integrated.is_some() {
        let block_entities = plan
            .block_entities
            .iter()
            .map(|(pos, c)| {
                let mut c = c.clone();
                c.remove("id");
                (*pos, snbt::to_snbt(&Tag::Compound(c)))
            })
            .collect();
        *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = Some((0, total));
        *SERVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(ServerJob::Paste(ServerPaste {
            dimension,
            palette: plan.palette,
            ids: None,
            blocks: plan.blocks,
            block_entities,
            entities: summons,
            at: 0,
        }));
        m.s.say(format!("Pasting {total} blocks of {}", p.name));
    } else if game.remote.is_some() {
        let bes: HashMap<[i32; 3], &Compound> = plan.block_entities.iter().map(|(p, c)| (*p, c)).collect();
        let mut commands: Vec<String> = plan
            .blocks
            .iter()
            .map(|(pos, i)| {
                let state = plan.palette[*i as usize].to_string();
                let nbt = bes.get(pos).map(|c| {
                    let mut c = (*c).clone();
                    c.remove("id");
                    snbt::to_snbt(&Tag::Compound(c))
                });
                format!("setblock {} {} {} {state}{}", pos[0], pos[1], pos[2], nbt.unwrap_or_default())
            })
            .collect();
        commands.extend(summons);
        m.s.say(format!(
            "Pasting {total} blocks of {} with commands (you need to be allowed to use /setblock)",
            p.name
        ));
        m.commands = Some(CommandPaste { commands, at: 0 });
    } else {
        m.s.fail("Join a world to paste");
    }
}

fn save(game: &mut Game<'_>, m: &mut Mod, name: String, format: SaveFormat) {
    let Some((lo, hi)) = m.s.selection.bounds() else { return };
    let volume: u64 = (0..3).map(|a| (hi[a] - lo[a] + 1) as u64).product();
    if volume > SAVE_MAX {
        m.s.fail(format!("The area is too large to save ({volume} blocks, at most {SAVE_MAX})"));
        return;
    }
    let author = game.options.player_name();
    let job = SaveJob {
        dimension: m.s.selection.dimension.clone(),
        lo,
        hi,
        name,
        format,
        dir: m.s.root.clone(),
        author,
    };
    if game.integrated.is_some() {
        let mut server = SERVER.lock().unwrap_or_else(|e| e.into_inner());
        if server.is_some() {
            m.s.fail("A paste or a save is still running");
            return;
        }
        *server = Some(ServerJob::Save(job));
        m.s.say("Saving...");
    } else {
        // On a server: what this client sees of the area.
        let (text, error) = write_selection(game, &job);
        if error {
            m.s.fail(text);
        } else {
            m.s.say(text);
            m.s.list_dir();
        }
    }
}

/// Reads the job's area from `game`'s world and writes the file.
fn write_selection(game: &Game<'_>, job: &SaveJob) -> (String, bool) {
    let registry = &game.pack.assets.registry;
    let size = [job.hi[0] - job.lo[0] + 1, job.hi[1] - job.lo[1] + 1, job.hi[2] - job.lo[2] + 1];
    let mut region = Region::new(&job.name, [0; 3], size);
    let mut cache: HashMap<BlockStateId, u32> = HashMap::new();
    let facts = game.res.facts();
    for y in 0..size[1] {
        for z in 0..size[2] {
            for x in 0..size[0] {
                let w = [job.lo[0] + x, job.lo[1] + y, job.lo[2] + z];
                let id = game.world.block(w[0], w[1], w[2]);
                if facts.is_air(id) {
                    continue;
                }
                let p = match cache.get(&id) {
                    Some(&p) => p,
                    None => {
                        let p = region.intern(engine_state(registry, id));
                        cache.insert(id, p);
                        p
                    }
                };
                if let Some(i) = region.index(x, y, z) {
                    region.blocks[i] = p;
                }
                if facts.get(id).block_entity
                    && let Some(be) = block_entity(game, w)
                {
                    region.block_entities.insert([x, y, z], be);
                }
            }
        }
    }
    let mut s = Schematic::new(&job.name, Format::Litematic, vec![region]);
    s.author = job.author.clone();
    let blocks = s.block_count();
    match formats::save(&job.dir, &job.name, &s, job.format, horizon_world::DATA_VERSION, now_millis()) {
        Ok(path) => (
            format!("Saved {blocks} blocks as {}", path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
            false,
        ),
        Err(e) => (format!("Couldn't save: {e}"), true),
    }
}

// ---------------------------------------------------------------------------
// Ghost meshes.

/// A hash step (FxHash's): signatures hash a section's every block.
fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95)
}

fn section_key(id: u64, s: [i32; 3]) -> u64 {
    s.iter().fold(mix(0, id), |h, &v| mix(h, u64::from(v as u32)))
}

/// Set when the game reloaded its resources: block models and the atlas
/// may have changed.
static RELOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn resources_reloaded() {
    RELOADED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// A block model's face, baked once per state: the side that hides it, and
/// its vertices from the block's corner.
struct Face {
    cull: Option<Direction>,
    vertices: [Vertex; 4],
}

/// The faces of `state`'s model (its first variant), textured from the atlas.
fn bake(game: &Game<'_>, state: BlockStateId) -> Arc<[Face]> {
    let Ok(models) = game.pack.render.model_catalog.collect_seeded(state, 0) else {
        return Arc::from([]);
    };
    let grass = game.pack.item_renderer.grass_color(0.5, 1.0) as u32;
    let tables = &game.pack.context.tables;
    let mut faces = Vec::new();
    for (_, model) in &models {
        for face in &model.faces {
            let Some(s) = game.pack.layer_names.get(&face.layer).and_then(|n| game.gui.atlas.sprites.get(n)) else {
                continue;
            };
            let shade: u32 = match face.quad.facing {
                Direction::Up => 255,
                Direction::Down => 160,
                Direction::North | Direction::South => 220,
                Direction::West | Direction::East => 190,
            };
            let mut color = 0xFF00_0000 | shade << 16 | shade << 8 | shade;
            if let Some(t) = tables.default_tint(state, face.quad.tint_index, grass) {
                color = horizon_client::hud_layout::multiply(color, horizon_client::mobs::render::abgr(t));
            }
            let vertex = |k: usize| {
                let uv = face.quad.uv[k];
                Vertex {
                    normal: [0; 4],
                    pos: face.quad.positions[k],
                    uv: [s.u0 + uv[0] * (s.u1 - s.u0), s.v0 + uv[1] * (s.v1 - s.v0)],
                    layer: 0,
                    color,
                    light: 0,
                    glint_uv: [0.0, 0.0],
                }
            };
            faces.push(Face { cull: face.quad.cull, vertices: [vertex(0), vertex(1), vertex(2), vertex(3)] });
        }
    }
    faces.into()
}

/// The world's blocks seen from one section: its own chunk read directly,
/// the others (a neighbour across the edge) through the world.
struct Blocks<'a, 'g> {
    game: &'a Game<'g>,
    cx: i32,
    cz: i32,
    chunk: Option<&'a horizon_client::chunk::Chunk>,
}

impl<'a, 'g> Blocks<'a, 'g> {
    fn new(game: &'a Game<'g>, sec: [i32; 3]) -> Self {
        let chunk = game.world.chunks.get(&(sec[0], sec[2])).map(|c| &**c);
        Self { game, cx: sec[0], cz: sec[2], chunk }
    }
    fn at(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        if x >> 4 == self.cx && z >> 4 == self.cz {
            return match self.chunk {
                Some(c) => c.block(x & 15, y, z & 15),
                None => self.game.world.air,
            };
        }
        self.game.world.block(x, y, z)
    }
}

fn update_ghosts(game: &Game<'_>, m: &mut Mod, here: &Here) {
    let start = Instant::now();
    ghost::set_opacity(m.s.settings.opacity as f32 / 100.0);
    if RELOADED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        m.models.clear();
        m.sections.clear();
        m.wanted_sig = 0;
    }
    let registry = &game.pack.assets.registry;
    // Each placement's palettes as the game's states, once per turn.
    m.resolved.retain(|id, _| m.s.placements.iter().any(|p| p.id == *id));
    for p in &m.s.placements {
        if m.resolved.get(&p.id).is_some_and(|r| r.turn_rev == p.turn_rev) {
            continue;
        }
        let states = p.palettes();
        let ids = states.iter().map(|ps| ps.iter().map(|s| if s.is_air() { None } else { resolve(registry, s) }).collect()).collect();
        m.resolved.insert(p.id, Resolved { turn_rev: p.turn_rev, states, ids });
    }

    // The sections in range, nearest first: worked out again only when the
    // player changes section or a placement or setting changes.
    let range = m.s.settings.range as i32;
    let ps = here.block().map(|v| v >> 4);
    let mut sig = ps.iter().fold(mix(u64::from(m.s.settings.range), m.s.render_rev), |h, &v| mix(h, u64::from(v as u32)));
    sig = here.dimension.bytes().fold(sig, |h, b| mix(h, u64::from(b)));
    for p in &m.s.placements {
        sig = mix(mix(mix(sig, p.id), p.rev), u64::from(p.visible));
    }
    if sig != m.wanted_sig {
        m.wanted_sig = sig;
        let (ly0, ly1) = m.s.settings.layers.range();
        let mut wanted: Vec<(i64, u64, [i32; 3])> = Vec::new();
        for p in m.s.placements.iter().filter(|p| p.visible && p.dimension == here.dimension) {
            let (lo, hi) = p.bounds();
            let (ylo, yhi) = (lo[1].max(ly0), hi[1].min(ly1));
            if ylo > yhi {
                continue;
            }
            for sx in (lo[0] >> 4).max(ps[0] - range)..=(hi[0] >> 4).min(ps[0] + range) {
                for sz in (lo[2] >> 4).max(ps[2] - range)..=(hi[2] >> 4).min(ps[2] + range) {
                    for sy in (ylo >> 4).max(ps[1] - range)..=(yhi >> 4).min(ps[1] + range) {
                        let d = [sx - ps[0], sy - ps[1], sz - ps[2]].iter().map(|&v| i64::from(v) * i64::from(v)).sum();
                        wanted.push((d, p.id, [sx, sy, sz]));
                    }
                }
            }
        }
        wanted.sort_unstable_by_key(|w| w.0);
        wanted.truncate(6000);
        m.wanted = wanted.into_iter().map(|w| (w.1, w.2)).collect();
        let want: HashSet<(u64, [i32; 3])> = m.wanted.iter().copied().collect();
        m.sections.retain(|k, _| want.contains(k));
        let keys: HashSet<u64> = want.iter().map(|&(id, s)| section_key(id, s)).collect();
        ghost::retain(|k| keys.contains(&k));
        m.stale = true;
        m.verify_dirty = true;
    }

    let Mod { s, resolved, models, sections, wanted, .. } = m;
    let settings = &s.settings;
    let rev_of = |p: &Placement| mix(p.rev, s.render_rev);
    let mut built = false;
    // Sections new or changed: built nearest first, within the budget.
    if m.stale {
        let mut done = true;
        for &(id, sec) in wanted.iter() {
            let Some(p) = s.placements.iter().find(|p| p.id == id) else { continue };
            let rev = rev_of(p);
            if sections.get(&(id, sec)).is_some_and(|x| x.rev == rev) {
                continue;
            }
            if start.elapsed() > MESH_TIME {
                done = false;
                break;
            }
            let Some(r) = resolved.get(&id) else { continue };
            let (section, mesh) = build(game, p, r, settings, models, sec, rev);
            ghost::set_mesh(section_key(id, sec), next_rev(), mesh.map(Arc::new));
            sections.insert((id, sec), section);
            built = true;
        }
        m.stale = !done;
    }
    // After the world changed (a block anywhere, a chunk in or out), every
    // built section is checked against it again, a few a frame.
    let world_sig = mix(game.world.revision, game.world.chunks.len() as u64);
    if world_sig != m.world_sig {
        m.world_sig = world_sig;
        m.checks_left = wanted.len();
        m.check = 0;
    }
    while m.checks_left > 0 && !wanted.is_empty() && start.elapsed() < MESH_TIME * 2 {
        let (id, sec) = wanted[m.check % wanted.len()];
        m.check += 1;
        m.checks_left -= 1;
        let Some(p) = s.placements.iter().find(|p| p.id == id) else { continue };
        let Some(old) = sections.get(&(id, sec)) else { continue };
        if signature(game, p, settings, sec) == old.sig {
            continue;
        }
        let rev = old.rev;
        let Some(r) = resolved.get(&id) else { continue };
        let (section, mesh) = build(game, p, r, settings, models, sec, rev);
        ghost::set_mesh(section_key(id, sec), next_rev(), mesh.map(Arc::new));
        sections.insert((id, sec), section);
        built = true;
    }
    // What the loaded world holds of each placement.
    if built || m.verify_dirty {
        m.verify_dirty = false;
        let mut verify: HashMap<u64, Verify> = HashMap::new();
        for ((id, _), sec) in sections.iter() {
            verify.entry(*id).or_default().add(&sec.verify);
        }
        s.verify = verify;
    }
}

fn next_rev() -> u64 {
    static REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    REV.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The part of section `sec` the placement and the layers cover.
fn section_box(p: &Placement, settings: &st::Settings, sec: [i32; 3]) -> Option<([i32; 3], [i32; 3])> {
    let (lo, hi) = p.bounds();
    let (ly0, ly1) = settings.layers.range();
    let mut a = [0; 3];
    let mut b = [0; 3];
    for i in 0..3 {
        a[i] = (sec[i] * 16).max(lo[i]);
        b[i] = (sec[i] * 16 + 15).min(hi[i]);
    }
    a[1] = a[1].max(ly0);
    b[1] = b[1].min(ly1);
    (a[0] <= b[0] && a[1] <= b[1] && a[2] <= b[2]).then_some((a, b))
}

/// What the world holds in the section's box: it changed when this does.
/// The same hash `build` makes while it reads the blocks.
fn signature(game: &Game<'_>, p: &Placement, settings: &st::Settings, sec: [i32; 3]) -> u64 {
    let blocks = Blocks::new(game, sec);
    let mut h = u64::from(blocks.chunk.is_some());
    if let Some((a, b)) = section_box(p, settings, sec) {
        for y in a[1]..=b[1] {
            for z in a[2]..=b[2] {
                for x in a[0]..=b[0] {
                    h = mix(h, u64::from(blocks.at(x, y, z).0));
                }
            }
        }
    }
    h
}

fn abgr(rgb: u32, a: u8) -> u32 {
    u32::from(a) << 24 | (rgb & 0xFF) << 16 | (rgb >> 8 & 0xFF) << 8 | (rgb >> 16 & 0xFF)
}

fn flat(pos: [f32; 3], color: u32) -> Vertex {
    Vertex { pos, uv: [0.0, 0.0], layer: FLAT, color, light: 0, normal: [0; 4], glint_uv: [0.0, 0.0] }
}

/// A box from `a` to `b` as six flat quads.
fn cuboid(out: &mut Vec<Vertex>, a: [f32; 3], b: [f32; 3], color: u32) {
    let [x0, y0, z0] = a;
    let [x1, y1, z1] = b;
    let faces = [
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
    ];
    for face in faces {
        for p in face {
            out.push(flat(p, color));
        }
    }
}

fn offset(d: Direction) -> [i32; 3] {
    match d {
        Direction::Down => [0, -1, 0],
        Direction::Up => [0, 1, 0],
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
    }
}

/// Whether two states of one block differ only in properties the world
/// works out by itself (a leaf's distance, a wire's power).
fn same_but_derived(registry: &BlockRegistry, a: BlockStateId, b: BlockStateId) -> bool {
    let (Some(sa), Some(sb)) = (registry.state(a), registry.state(b)) else { return false };
    sa.name == sb.name
        && sa.properties.len() == sb.properties.len()
        && sa
            .properties
            .iter()
            .zip(sb.properties.iter())
            .all(|((k, v), (k2, w))| k == k2 && (v == w || st::derived_property(&sa.name, k)))
}

/// What a block of a placement is, against the world.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Not in the placement (or its layers), or as the schematic has it.
    None,
    /// Missing: drawn as a ghost of the state.
    Ghost(BlockStateId),
    WrongBlock,
    WrongState,
    Extra,
    /// A block this game doesn't know.
    Unknown,
}

/// The overlay colour of a kind drawn as a box.
fn overlay_color(kind: Kind) -> Option<(u32, u8)> {
    match kind {
        Kind::WrongBlock => Some(WRONG_BLOCK),
        Kind::WrongState => Some(WRONG_STATE),
        Kind::Extra => Some(EXTRA),
        Kind::Unknown => Some(UNKNOWN),
        _ => None,
    }
}

/// The faces of the unit cube, by `Direction` order (down, up, north,
/// south, west, east).
const CUBE_FACES: [[[f32; 3]; 4]; 6] = [
    [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
    [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
    [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
    [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
];

/// Meshes a placement's section: ghosts of the blocks missing from the
/// world, overlays on wrong and extra blocks; and what it found. Every
/// block is classified first, so faces between two ghosts, or two overlays
/// of a kind, are left out.
fn build(
    game: &Game<'_>,
    p: &Placement,
    r: &Resolved,
    settings: &st::Settings,
    models: &mut HashMap<BlockStateId, Arc<[Face]>>,
    sec: [i32; 3],
    rev: u64,
) -> (Section, Option<Mesh>) {
    let origin = sec.map(|v| v * 16);
    let blocks = Blocks::new(game, sec);
    let loaded = blocks.chunk.is_some();
    let mut section = Section { rev, sig: u64::from(loaded), verify: Verify::default(), correct: HashMap::new() };
    let Some((a, b)) = section_box(p, settings, sec) else {
        return (section, None);
    };
    let facts = game.res.facts();
    let registry = &game.pack.assets.registry;
    let air = game.world.air;
    // What the block at `w` is, with the world's block there.
    let classify = |w: [i32; 3], world: BlockStateId| -> (Kind, Option<(usize, u32)>) {
        if !settings.layers.shows(w[1]) {
            return (Kind::None, None);
        }
        let Some((ri, pi)) = p.at(w) else { return (Kind::None, None) };
        let Some(want) = r.ids[ri][pi as usize] else {
            if !r.states[ri][pi as usize].is_air() {
                return (Kind::Unknown, None);
            }
            let extra = loaded && world != air && !facts.is_air(world) && !facts.get(world).name.ends_with("air");
            return (if extra { Kind::Extra } else { Kind::None }, None);
        };
        if loaded && (world == want || (!facts.is_air(world) && same_but_derived(registry, world, want))) {
            return (Kind::None, Some((ri, pi)));
        }
        if !loaded || facts.is_air(world) {
            return (Kind::Ghost(want), None);
        }
        let same = facts.get(world).name == facts.get(want).name;
        (if same { Kind::WrongState } else { Kind::WrongBlock }, None)
    };

    // First the section's blocks: their kinds, the counts and the world's hash.
    let index = |x: i32, y: i32, z: i32| ((y - origin[1]) * 256 + (z - origin[2]) * 16 + (x - origin[0])) as usize;
    let mut kinds = vec![Kind::None; 4096];
    let mut sig = section.sig;
    for y in a[1]..=b[1] {
        for z in a[2]..=b[2] {
            for x in a[0]..=b[0] {
                let world = blocks.at(x, y, z);
                sig = mix(sig, u64::from(world.0));
                let (kind, correct) = classify([x, y, z], world);
                kinds[index(x, y, z)] = kind;
                match kind {
                    Kind::None => {
                        if let Some(at) = correct {
                            section.verify.correct += 1;
                            *section.correct.entry(at).or_default() += 1;
                        }
                    }
                    Kind::Ghost(_) => {
                        if loaded {
                            section.verify.missing += 1;
                        }
                    }
                    Kind::WrongBlock => section.verify.wrong_block += 1,
                    Kind::WrongState => section.verify.wrong_state += 1,
                    Kind::Extra => section.verify.extra += 1,
                    Kind::Unknown => section.verify.missing += 1,
                }
            }
        }
    }
    section.sig = sig;
    // A neighbour's kind: from the section's own, else worked out.
    let kind_at = |n: [i32; 3]| -> Kind {
        if (0..3).all(|i| n[i] >> 4 == sec[i]) {
            kinds[index(n[0], n[1], n[2])]
        } else {
            classify(n, blocks.at(n[0], n[1], n[2])).0
        }
    };
    let shown = |kind: Kind| match kind {
        Kind::WrongBlock | Kind::WrongState => settings.show_wrong,
        Kind::Extra => settings.show_extra,
        _ => true,
    };

    // Then the geometry.
    let alpha = (settings.opacity.clamp(10, 100) * 255 / 100) as u8;
    let mut out: Vec<Vertex> = Vec::new();
    for y in a[1]..=b[1] {
        for z in a[2]..=b[2] {
            for x in a[0]..=b[0] {
                let kind = kinds[index(x, y, z)];
                let local = [(x - origin[0]) as f32, (y - origin[1]) as f32, (z - origin[2]) as f32];
                if let Kind::Ghost(want) = kind {
                    let faces = models.entry(want).or_insert_with(|| bake(game, want)).clone();
                    if faces.is_empty() {
                        fallback(&mut out, facts.get(want), local, alpha);
                        continue;
                    }
                    for face in faces.iter() {
                        if let Some(cull) = face.cull {
                            let d = offset(cull);
                            let n = [x + d[0], y + d[1], z + d[2]];
                            if facts.get(blocks.at(n[0], n[1], n[2])).solid_render {
                                continue;
                            }
                            if let Kind::Ghost(ng) = kind_at(n)
                                && (ng == want || facts.get(ng).solid_render)
                            {
                                continue;
                            }
                        }
                        for v in &face.vertices {
                            let mut v = *v;
                            v.pos = [v.pos[0] + local[0], v.pos[1] + local[1], v.pos[2] + local[2]];
                            out.push(v);
                        }
                    }
                    continue;
                }
                let Some((rgb, a)) = overlay_color(kind).filter(|_| shown(kind)) else { continue };
                let color = abgr(rgb, a);
                // A little larger than the block, so it shows on its faces.
                let e = 0.004;
                for (i, d) in Direction::ALL.into_iter().enumerate() {
                    let o = offset(d);
                    if kind_at([x + o[0], y + o[1], z + o[2]]) == kind {
                        continue;
                    }
                    for c in CUBE_FACES[i] {
                        let pos = [0, 1, 2].map(|k| local[k] + if c[k] > 0.5 { 1.0 + e } else { -e });
                        out.push(flat(pos, color));
                    }
                }
            }
        }
    }
    let mesh = (!out.is_empty()).then_some(Mesh { origin, vertices: out });
    (section, mesh)
}

/// A block without a model (fluids, and the blocks drawn as entities:
/// chests, signs, beds) as flat boxes of its shape and map colour.
fn fallback(out: &mut Vec<Vertex>, f: &horizon_items::blocks::StateFacts, local: [f32; 3], alpha: u8) {
    let name = f.name.as_ref();
    if name.ends_with("water") || name == "minecraft:bubble_column" {
        cuboid(out, local, [local[0] + 1.0, local[1] + 0.875, local[2] + 1.0], abgr(WATER, alpha.min(0xA0)));
        return;
    }
    if name.ends_with("lava") {
        cuboid(out, local, [local[0] + 1.0, local[1] + 0.875, local[2] + 1.0], abgr(LAVA, alpha));
        return;
    }
    let rgb = horizon_items::blocks::MAP_COLORS.get(usize::from(f.map_color)).copied().filter(|&c| c != 0).unwrap_or(0x8F7748);
    if f.outline.is_empty() {
        cuboid(out, local, [local[0] + 1.0, local[1] + 1.0, local[2] + 1.0], abgr(rgb, alpha));
    }
    for bx in f.outline.iter() {
        let lo = [local[0] + bx[0] as f32, local[1] + bx[1] as f32, local[2] + bx[2] as f32];
        let hi = [local[0] + bx[3] as f32, local[1] + bx[4] as f32, local[2] + bx[5] as f32];
        cuboid(out, lo, hi, abgr(rgb, alpha));
    }
}

/// The boxes drawn over everything: placement bounds and the area selection.
fn update_lines(m: &mut Mod, here: &Here) {
    let origin = here.block().map(|v| v.div_euclid(256) * 256);
    let mut boxes: Vec<([i32; 3], [i32; 3], u32, f32)> = Vec::new();
    for p in m.s.placements.iter().filter(|p| p.visible && p.dimension == here.dimension) {
        let (lo, hi) = p.bounds();
        let selected = Some(p.id) == m.s.selected;
        boxes.push((lo, hi, if selected { abgr(0x60B0FF, 0xE0) } else { abgr(0xB0B0B0, 0x90) }, 0.04));
    }
    let sel = &m.s.selection;
    if sel.dimension == here.dimension {
        if let Some((lo, hi)) = sel.bounds() {
            boxes.push((lo, hi, abgr(0xFFFFFF, 0xF0), 0.05));
        }
        for (i, c) in sel.corners.iter().enumerate() {
            if let Some(c) = c {
                boxes.push((*c, *c, if i == 0 { abgr(0xFF5050, 0xF0) } else { abgr(0x5080FF, 0xF0) }, 0.06));
            }
        }
    }
    let mut sig = origin.iter().fold(1, |h, &v| mix(h, u64::from(v as u32)));
    for (lo, hi, color, _) in &boxes {
        for v in lo.iter().chain(hi) {
            sig = mix(sig, u64::from(*v as u32));
        }
        sig = mix(sig, u64::from(*color));
    }
    if sig == m.lines_sig {
        return;
    }
    m.lines_sig = sig;
    let mut out = Vec::new();
    for (lo, hi, color, t) in boxes {
        let a = [(lo[0] - origin[0]) as f32, (lo[1] - origin[1]) as f32, (lo[2] - origin[2]) as f32];
        let b = [(hi[0] - origin[0] + 1) as f32, (hi[1] - origin[1] + 1) as f32, (hi[2] - origin[2] + 1) as f32];
        edges(&mut out, a, b, t, color);
    }
    ghost::set_lines(sig, Some(Arc::new(Mesh { origin, vertices: out })));
}

/// The twelve edges of a box, as thin boxes `t` thick.
fn edges(out: &mut Vec<Vertex>, a: [f32; 3], b: [f32; 3], t: f32, color: u32) {
    let h = t / 2.0;
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for cu in [a[u], b[u]] {
            for cv in [a[v], b[v]] {
                let mut lo = [0.0; 3];
                let mut hi = [0.0; 3];
                lo[axis] = a[axis] - h;
                hi[axis] = b[axis] + h;
                lo[u] = cu - h;
                hi[u] = cu + h;
                lo[v] = cv - h;
                hi[v] = cv + h;
                cuboid(out, lo, hi, color);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The material list.

fn materials(game: &Game<'_>, m: &mut Mod) {
    let Some(p) = m.s.selected() else {
        m.s.materials.clear();
        return;
    };
    let id = p.id;
    let needed = m.needed.entry(id).or_insert_with(|| {
        p.schematic
            .regions
            .iter()
            .map(|r| {
                let mut n = vec![0u32; r.palette.len()];
                for &b in &r.blocks {
                    n[b as usize] += 1;
                }
                n
            })
            .collect()
    });
    let Some(r) = m.resolved.get(&id) else { return };
    let mut correct: HashMap<(usize, u32), u32> = HashMap::new();
    for ((pid, _), s) in &m.sections {
        if *pid == id {
            for (k, v) in &s.correct {
                *correct.entry(*k).or_default() += v;
            }
        }
    }
    let facts = game.res.facts();
    let mut rows: HashMap<String, (u32, u32)> = HashMap::new();
    for (ri, counts) in needed.iter().enumerate() {
        for (pi, &n) in counts.iter().enumerate() {
            let state = &r.states[ri][pi];
            if n == 0 || state.is_air() {
                continue;
            }
            let per = materials::item_count(state);
            if per == 0 {
                continue;
            }
            let item = match materials::item_override(state) {
                Some(i) => i.to_owned(),
                None => match r.ids[ri][pi] {
                    Some(sid) => {
                        let item = facts.get(sid).item.to_string();
                        if item.is_empty() || item == "minecraft:air" { state.name.clone() } else { item }
                    }
                    None => state.name.clone(),
                },
            };
            let done = correct.get(&(ri, pi as u32)).copied().unwrap_or(0);
            let row = rows.entry(item).or_default();
            row.0 += n * per;
            row.1 += done.min(n) * per;
        }
    }
    let have: HashMap<String, u32> = game.native_carried_items().into_iter().collect();
    let mut list: Vec<st::MaterialRow> = rows
        .into_iter()
        .map(|(item, (needed, done))| st::MaterialRow {
            have: have.get(&item).copied().unwrap_or(0),
            missing: needed - done.min(needed),
            needed,
            item,
        })
        .collect();
    list.sort_by(|a, b| b.missing.cmp(&a.missing).then(b.needed.cmp(&a.needed)).then(a.item.cmp(&b.item)));
    m.s.materials = list;
    m.s.materials_for = Some(id);
}

// ---------------------------------------------------------------------------
// Ticks.

/// The window's tick: commands of a paste on a server.
pub(super) fn client_tick(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    let batch: Vec<String> = with(|m| {
        let Some(c) = m.commands.as_mut() else { return Vec::new() };
        let end = (c.at + COMMANDS_PER_TICK).min(c.commands.len());
        let batch = c.commands[c.at..end].to_vec();
        c.at = end;
        if c.at >= c.commands.len() {
            let n = c.commands.len();
            m.commands = None;
            m.s.say(format!("Sent all {n} paste commands"));
        }
        batch
    });
    if game.remote.is_none() {
        return;
    }
    for command in batch {
        game.send_command(&command);
    }
}

/// The single player server's level tick: a paste or a save, for the level
/// it is in.
pub(super) fn server_tick(game: &mut Game<'_>, dimension: &str) {
    if game.native_window() || game.serving.is_none() {
        return;
    }
    let mut guard = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    let Some(job) = guard.as_mut() else { return };
    match job {
        ServerJob::Save(save) => {
            if save.dimension != dimension {
                return;
            }
            let (text, error) = write_selection(game, save);
            tell(text, error);
            *guard = None;
        }
        ServerJob::Paste(paste) => {
            if paste.dimension != dimension {
                return;
            }
            let start = Instant::now();
            let registry = &game.pack.assets.registry;
            let ids = paste.ids.get_or_insert_with(|| paste.palette.iter().map(|s| resolve(registry, s)).collect()).clone();
            let total = paste.blocks.len();
            let end = (paste.at + PASTE_BLOCKS).min(total);
            while paste.at < end {
                let (pos, i) = paste.blocks[paste.at];
                if let Some(id) = ids[i as usize] {
                    game.native_set_block(dimension, pos, id, PASTE_FLAGS);
                }
                paste.at += 1;
                if start.elapsed() > PASTE_TIME {
                    break;
                }
            }
            *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = Some((paste.at, total));
            if paste.at < total {
                return;
            }
            for (pos, nbt) in std::mem::take(&mut paste.block_entities) {
                game.load_block_entity_nbt(pos, &nbt);
            }
            let summons = std::mem::take(&mut paste.entities);
            let entities = summons.len();
            for command in summons {
                game.native_run_command(&command);
            }
            let extra = if entities > 0 { format!(" and {entities} entities") } else { String::new() };
            tell(format!("Pasted {total} blocks{extra}"), false);
            *PROGRESS.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *guard = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Input and drawing, for the hooks.

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_on(ID) || !game.native_playing() {
        return;
    }
    let menu = game.native_mod_key(ID, KEY_MENU, DEFAULT_MENU);
    let up = game.native_mod_key(ID, KEY_LAYER_UP, DEFAULT_LAYER_UP);
    let down = game.native_mod_key(ID, KEY_LAYER_DOWN, DEFAULT_LAYER_DOWN);
    let here = here(game);
    if key == menu {
        let open = game.native_mod_screen_open(ID);
        with(|m| {
            if open && (m.s.ui.tab == ui::Tab::Load || !m.s.ui.listed) {
                m.s.list_dir();
            }
        });
    } else if key == up || key == down {
        with(|m| {
            let fx = m.s.step_layer(if key == up { 1 } else { -1 }, here.as_ref());
            m.pending.extend(fx);
        });
    }
}

/// The tool's clicks while playing: the selection's corners.
pub(super) fn mouse(game: &mut Game<'_>, button: i32, pressed: bool, canceled: &mut bool) {
    if !pressed || (button != 0 && button != 1) || !game.native_on(ID) || !game.native_playing() {
        return;
    }
    if game.native_held().is_none_or(|h| h.0 != TOOL) {
        return;
    }
    let Some((_, hit)) = game.native_view_hit() else { return };
    let Some(p) = game.native_player() else { return };
    let pos = [hit.block_x, hit.block_y, hit.block_z];
    with(|m| m.s.set_corner(button as usize, pos, p.dimension));
    *canceled = true;
}

/// Ctrl + wheel with the tool: nudges the selected placement.
pub(super) fn scrolling(game: &mut Game<'_>, accumulated: [i32; 2], modifiers: u32, canceled: &mut bool) {
    if modifiers & 2 == 0 || accumulated[1] == 0 || !game.native_on(ID) || !game.native_playing() {
        return;
    }
    if game.native_held().is_none_or(|h| h.0 != TOOL) {
        return;
    }
    let Some(here) = here(game) else { return };
    with(|m| {
        if m.s.selected().is_some() {
            m.s.nudge(&here, accumulated[1] > 0);
            *canceled = true;
        }
    });
}

pub(super) fn command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    let mut words = command.trim().splitn(2, ' ');
    let name = words.next().unwrap_or("");
    if name != "schematics" && name != "sch" {
        return;
    }
    *ran = true;
    let args = words.next().unwrap_or("").to_owned();
    let here = here(game);
    with(|m| {
        let fx = m.s.command(&args, here.as_ref());
        m.pending.extend(fx);
    });
}

pub(super) fn commands(commands: &mut Vec<&'static str>) {
    commands.push("schematics");
    commands.push("sch");
}

pub(super) fn hud(game: &mut Game<'_>, batch: &mut horizon_client::hud::HudBatch, corners: &mut horizon_client::extras_hud::Corners) {
    if !game.native_on(ID) || game.native_player().is_none() || game.native_mod_screen() == Some(ID) {
        return;
    }
    use horizon_client::extras::Corner;
    if !game.native_corner_shown(Corner::TopLeft) {
        return;
    }
    let game = &*game;
    let measure = |t: &str| game.native_text_width(t);
    let lines = with(|m| ui::hud(&m.s));
    if lines.is_empty() {
        return;
    }
    let [w, h] = ui::hud_size(&lines, &measure);
    let (x, y) = corners.place(Corner::TopLeft, w, h);
    for d in ui::hud_draws(&lines, [x, y], &measure) {
        draw(game, batch, d);
    }
}

fn draw(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, d: ui::Draw) {
    match d {
        ui::Draw::Fill(rect, argb) => game.native_fill(batch, rect, argb),
        ui::Draw::Text(text, at, rgb, shadow) => game.native_text(batch, &text, at, rgb, shadow),
        ui::Draw::Item(item, at, size) => {
            game.native_item(batch, &item, at, size);
        }
    }
}

pub(super) fn screen(game: &Game<'_>, batch: &mut horizon_client::hud::HudBatch, mouse: [f32; 2]) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    let here = here(game);
    let size = [batch.width, batch.height];
    let measure = |t: &str| game.native_text_width(t);
    let (draws, _) = with(|m| ui::screen(&m.s, size, mouse, here.as_ref(), &measure));
    for d in draws {
        draw(game, batch, d);
    }
}

pub(super) fn click(game: &Game<'_>, size: [f32; 2], mouse: [f32; 2], button: i32, canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    *canceled = true;
    if button != 0 {
        return;
    }
    let here = here(game);
    let measure = |t: &str| game.native_text_width(t);
    with(|m| {
        let (_, widgets) = ui::screen(&m.s, size, mouse, here.as_ref(), &measure);
        match ui::hit(&widgets, mouse) {
            Some(action) => {
                let fx = ui::act(&mut m.s, action, here.as_ref());
                m.pending.extend(fx);
            }
            None => m.s.ui.name_focus = false,
        }
    });
}

pub(super) fn scroll(game: &Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    *canceled = true;
    with(|m| ui::scroll(&mut m.s, scroll[1]));
}

pub(super) fn screen_key(game: &Game<'_>, key: &str, canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    let menu = game.native_mod_key(ID, KEY_MENU, DEFAULT_MENU);
    with(|m| {
        let (taken, fx) = ui::key(&mut m.s, key);
        m.pending.extend(fx);
        if taken {
            *canceled = true;
        } else if key == menu {
            *canceled = true;
            m.close = true;
        }
    });
}

pub(super) fn typed(game: &Game<'_>, c: char, canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    with(|m| {
        if m.s.ui.name_focus {
            *canceled = true;
            ui::typed(&mut m.s, c);
        }
    });
}
