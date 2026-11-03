//! Numeric block ids and data values (Minecraft 1.12 and before, as MCEdit,
//! WorldEdit 6 and Schematica wrote them) as the flattened block states of
//! 1.13 on.

use crate::schematic::BlockState;

/// 1.12's block names by numeric id (empty where no block had the id).
pub const NAMES: [&str; 256] = [
    "air", "stone", "grass", "dirt", "cobblestone", "planks", "sapling", "bedrock", "flowing_water", "water",
    "flowing_lava", "lava", "sand", "gravel", "gold_ore", "iron_ore", "coal_ore", "log", "leaves", "sponge",
    "glass", "lapis_ore", "lapis_block", "dispenser", "sandstone", "noteblock", "bed", "golden_rail", "detector_rail", "sticky_piston",
    "web", "tallgrass", "deadbush", "piston", "piston_head", "wool", "piston_extension", "yellow_flower", "red_flower", "brown_mushroom",
    "red_mushroom", "gold_block", "iron_block", "double_stone_slab", "stone_slab", "brick_block", "tnt", "bookshelf", "mossy_cobblestone", "obsidian",
    "torch", "fire", "mob_spawner", "oak_stairs", "chest", "redstone_wire", "diamond_ore", "diamond_block", "crafting_table", "wheat",
    "farmland", "furnace", "lit_furnace", "standing_sign", "wooden_door", "ladder", "rail", "stone_stairs", "wall_sign", "lever",
    "stone_pressure_plate", "iron_door", "wooden_pressure_plate", "redstone_ore", "lit_redstone_ore", "unlit_redstone_torch", "redstone_torch", "stone_button", "snow_layer", "ice",
    "snow", "cactus", "clay", "reeds", "jukebox", "fence", "pumpkin", "netherrack", "soul_sand", "glowstone",
    "portal", "lit_pumpkin", "cake", "unpowered_repeater", "powered_repeater", "stained_glass", "trapdoor", "monster_egg", "stonebrick", "brown_mushroom_block",
    "red_mushroom_block", "iron_bars", "glass_pane", "melon_block", "pumpkin_stem", "melon_stem", "vine", "fence_gate", "brick_stairs", "stone_brick_stairs",
    "mycelium", "waterlily", "nether_brick", "nether_brick_fence", "nether_brick_stairs", "nether_wart", "enchanting_table", "brewing_stand", "cauldron", "end_portal",
    "end_portal_frame", "end_stone", "dragon_egg", "redstone_lamp", "lit_redstone_lamp", "double_wooden_slab", "wooden_slab", "cocoa", "sandstone_stairs", "emerald_ore",
    "ender_chest", "tripwire_hook", "tripwire", "emerald_block", "spruce_stairs", "birch_stairs", "jungle_stairs", "command_block", "beacon", "cobblestone_wall",
    "flower_pot", "carrots", "potatoes", "wooden_button", "skull", "anvil", "trapped_chest", "light_weighted_pressure_plate", "heavy_weighted_pressure_plate", "unpowered_comparator",
    "powered_comparator", "daylight_detector", "redstone_block", "quartz_ore", "hopper", "quartz_block", "quartz_stairs", "activator_rail", "dropper", "stained_hardened_clay",
    "stained_glass_pane", "leaves2", "log2", "acacia_stairs", "dark_oak_stairs", "slime", "barrier", "iron_trapdoor", "prismarine", "sea_lantern",
    "hay_block", "carpet", "hardened_clay", "coal_block", "packed_ice", "double_plant", "standing_banner", "wall_banner", "daylight_detector_inverted", "red_sandstone",
    "red_sandstone_stairs", "double_stone_slab2", "stone_slab2", "spruce_fence_gate", "birch_fence_gate", "jungle_fence_gate", "dark_oak_fence_gate", "acacia_fence_gate", "spruce_fence", "birch_fence",
    "jungle_fence", "dark_oak_fence", "acacia_fence", "spruce_door", "birch_door", "jungle_door", "acacia_door", "dark_oak_door", "end_rod", "chorus_plant",
    "chorus_flower", "purpur_block", "purpur_pillar", "purpur_stairs", "purpur_double_slab", "purpur_slab", "end_bricks", "beetroots", "grass_path", "end_gateway",
    "repeating_command_block", "chain_command_block", "frosted_ice", "magma", "nether_wart_block", "red_nether_brick", "bone_block", "structure_void", "observer", "white_shulker_box",
    "orange_shulker_box", "magenta_shulker_box", "light_blue_shulker_box", "yellow_shulker_box", "lime_shulker_box", "pink_shulker_box", "gray_shulker_box", "silver_shulker_box", "cyan_shulker_box", "purple_shulker_box",
    "blue_shulker_box", "brown_shulker_box", "green_shulker_box", "red_shulker_box", "black_shulker_box", "white_glazed_terracotta", "orange_glazed_terracotta", "magenta_glazed_terracotta", "light_blue_glazed_terracotta", "yellow_glazed_terracotta",
    "lime_glazed_terracotta", "pink_glazed_terracotta", "gray_glazed_terracotta", "silver_glazed_terracotta", "cyan_glazed_terracotta", "purple_glazed_terracotta", "blue_glazed_terracotta", "brown_glazed_terracotta", "green_glazed_terracotta", "red_glazed_terracotta",
    "black_glazed_terracotta", "concrete", "concrete_powder", "", "", "structure_block",
];

/// The id of a 1.12 block name (`minecraft:stone` or `stone`).
pub fn id_of(name: &str) -> Option<u16> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    if name.is_empty() {
        return None;
    }
    NAMES.iter().position(|&n| n == name).map(|i| i as u16)
}

pub const COLORS: [&str; 16] = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue",
    "brown", "green", "red", "black",
];
const WOODS: [&str; 6] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak"];
/// Facing by data value 0..6: dispensers, pistons, observers, end rods, shulker boxes.
const FACING6: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
/// `EnumFacing.getHorizontal`: beds, gates, repeaters, glazed terracotta.
const HORIZONTAL: [&str; 4] = ["south", "west", "north", "east"];

fn s(name: &str) -> BlockState {
    BlockState::new(name)
}

fn facing6(name: &str, meta: u8) -> BlockState {
    s(name).with("facing", FACING6[(meta & 7).min(5) as usize])
}

/// Chests, furnaces, ladders, wall signs: 2 north, 3 south, 4 west, 5 east.
fn facing2(name: &str, meta: u8) -> BlockState {
    let f = match meta & 7 {
        3 => "south",
        4 => "west",
        5 => "east",
        _ => "north",
    };
    s(name).with("facing", f)
}

fn horizontal(name: &str, meta: u8) -> BlockState {
    s(name).with("facing", HORIZONTAL[(meta & 3) as usize])
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn stairs(name: &str, meta: u8) -> BlockState {
    let f = ["east", "west", "south", "north"][(meta & 3) as usize];
    s(name)
        .with("facing", f)
        .with("half", if meta & 4 != 0 { "top" } else { "bottom" })
        .with("shape", "straight")
}

fn axis(name: &str, meta: u8) -> BlockState {
    s(name).with("axis", ["y", "x", "z", "y"][(meta >> 2 & 3) as usize])
}

fn log(wood: &str, meta: u8) -> BlockState {
    if meta >> 2 & 3 == 3 {
        s(&format!("{wood}_wood")).with("axis", "y")
    } else {
        axis(&format!("{wood}_log"), meta)
    }
}

fn slab(name: &str, meta: u8, double: bool) -> BlockState {
    let kind = if double {
        "double"
    } else if meta & 8 != 0 {
        "top"
    } else {
        "bottom"
    };
    s(name).with("type", kind)
}

const STONE_SLABS: [&str; 8] = [
    "smooth_stone_slab", "sandstone_slab", "petrified_oak_slab", "cobblestone_slab", "brick_slab", "stone_brick_slab",
    "nether_brick_slab", "quartz_slab",
];

fn torch(lit_name: &str, wall_name: &str, meta: u8, lit: Option<bool>) -> BlockState {
    let mut state = match meta & 7 {
        1 => s(wall_name).with("facing", "east"),
        2 => s(wall_name).with("facing", "west"),
        3 => s(wall_name).with("facing", "south"),
        4 => s(wall_name).with("facing", "north"),
        _ => s(lit_name),
    };
    if let Some(lit) = lit {
        state.set("lit", bool_str(lit));
    }
    state
}

fn button(name: &str, meta: u8) -> BlockState {
    let (face, facing) = match meta & 7 {
        0 => ("ceiling", "north"),
        1 => ("wall", "east"),
        2 => ("wall", "west"),
        3 => ("wall", "south"),
        4 => ("wall", "north"),
        _ => ("floor", "north"),
    };
    s(name).with("face", face).with("facing", facing).with("powered", bool_str(meta & 8 != 0))
}

fn rail(name: &str, meta: u8, powered: bool) -> BlockState {
    const SHAPES: [&str; 10] = [
        "north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south", "south_east",
        "south_west", "north_west", "north_east",
    ];
    let mut state = if powered {
        s(name).with("shape", SHAPES[(meta & 7).min(5) as usize]).with("powered", bool_str(meta & 8 != 0))
    } else {
        s(name).with("shape", SHAPES[(meta & 15).min(9) as usize])
    };
    state.set("waterlogged", "false");
    state
}

/// A door's lower half (`facing`, `open`) and upper half (`hinge`, `powered`)
/// data values, as the two blocks keep them.
pub fn door(name: &str, lower: u8, upper: u8, top: bool) -> BlockState {
    let facing = ["east", "south", "west", "north"][(lower & 3) as usize];
    s(name)
        .with("facing", facing)
        .with("open", bool_str(lower & 4 != 0))
        .with("hinge", if upper & 1 != 0 { "right" } else { "left" })
        .with("powered", bool_str(upper & 2 != 0))
        .with("half", if top { "upper" } else { "lower" })
}

const DOUBLE_PLANTS: [&str; 6] = ["sunflower", "lilac", "tall_grass", "large_fern", "rose_bush", "peony"];

/// A tall plant from its lower half's data value.
pub fn double_plant(lower: u8, top: bool) -> BlockState {
    s(DOUBLE_PLANTS[(lower & 7).min(5) as usize]).with("half", if top { "upper" } else { "lower" })
}

fn mushroom_block(name: &str, meta: u8) -> BlockState {
    // Which faces show the cap: north, east, south, west, up, down.
    let faces: [bool; 6] = match meta {
        1 => [true, false, false, true, true, false],
        2 => [true, false, false, false, true, false],
        3 => [true, true, false, false, true, false],
        4 => [false, false, false, true, true, false],
        5 => [false, false, false, false, true, false],
        6 => [false, true, false, false, true, false],
        7 => [false, false, true, true, true, false],
        8 => [false, false, true, false, true, false],
        9 => [false, true, true, false, true, false],
        10 => return stem([true, true, true, true, false, false]),
        14 => [true; 6],
        15 => return stem([true; 6]),
        _ => [false; 6],
    };
    with_faces(s(name), faces)
}

fn stem(faces: [bool; 6]) -> BlockState {
    with_faces(s("mushroom_stem"), faces)
}

fn with_faces(mut state: BlockState, faces: [bool; 6]) -> BlockState {
    for (key, on) in ["north", "east", "south", "west", "up", "down"].into_iter().zip(faces) {
        state.set(key, bool_str(on));
    }
    state
}

fn level(name: &str, meta: u8) -> BlockState {
    s(name).with("level", &(meta & 15).to_string())
}

fn age(name: &str, meta: u8, max: u8) -> BlockState {
    s(name).with("age", &(meta.min(max)).to_string())
}

/// The state of block `id` with data value `meta`.
pub fn state(id: u16, meta: u8) -> BlockState {
    let m = meta & 15;
    let color = COLORS[m as usize];
    let wood = WOODS[(m & 7).min(5) as usize];
    match id {
        0 => BlockState::air(),
        1 => s(["stone", "granite", "polished_granite", "diorite", "polished_diorite", "andesite", "polished_andesite"]
            [(m as usize).min(6)]),
        2 => s("grass_block").with("snowy", "false"),
        3 => match m {
            1 => s("coarse_dirt"),
            2 => s("podzol").with("snowy", "false"),
            _ => s("dirt"),
        },
        4 => s("cobblestone"),
        5 => s(&format!("{wood}_planks")),
        6 => s(&format!("{wood}_sapling")).with("stage", if m & 8 != 0 { "1" } else { "0" }),
        7 => s("bedrock"),
        8 | 9 => level("water", m),
        10 | 11 => level("lava", m),
        12 => s(if m == 1 { "red_sand" } else { "sand" }),
        13 => s("gravel"),
        14 => s("gold_ore"),
        15 => s("iron_ore"),
        16 => s("coal_ore"),
        17 => log(WOODS[(m & 3) as usize], m),
        18 | 161 => {
            let kind = if id == 18 { WOODS[(m & 3) as usize] } else { WOODS[4 + (m & 1) as usize] };
            s(&format!("{kind}_leaves")).with("persistent", bool_str(m & 4 != 0)).with("distance", "7")
        }
        19 => s(if m == 1 { "wet_sponge" } else { "sponge" }),
        20 => s("glass"),
        21 => s("lapis_ore"),
        22 => s("lapis_block"),
        23 => facing6("dispenser", m).with("triggered", bool_str(m & 8 != 0)),
        24 => s(["sandstone", "chiseled_sandstone", "cut_sandstone"][(m as usize).min(2)]),
        25 => s("note_block"),
        26 => horizontal("red_bed", m)
            .with("part", if m & 8 != 0 { "head" } else { "foot" })
            .with("occupied", "false"),
        27 => rail("powered_rail", m, true),
        28 => rail("detector_rail", m, true),
        29 => facing6("sticky_piston", m).with("extended", bool_str(m & 8 != 0)),
        30 => s("cobweb"),
        31 => s(["dead_bush", "short_grass", "fern"][(m as usize).min(2)]),
        32 => s("dead_bush"),
        33 => facing6("piston", m).with("extended", bool_str(m & 8 != 0)),
        34 => facing6("piston_head", m)
            .with("type", if m & 8 != 0 { "sticky" } else { "normal" })
            .with("short", "false"),
        35 => s(&format!("{color}_wool")),
        36 => facing6("moving_piston", m).with("type", if m & 8 != 0 { "sticky" } else { "normal" }),
        37 => s("dandelion"),
        38 => s([
            "poppy", "blue_orchid", "allium", "azure_bluet", "red_tulip", "orange_tulip", "white_tulip", "pink_tulip",
            "oxeye_daisy",
        ][(m as usize).min(8)]),
        39 => s("brown_mushroom"),
        40 => s("red_mushroom"),
        41 => s("gold_block"),
        42 => s("iron_block"),
        43 => match m {
            8 => s("smooth_stone"),
            9 => s("smooth_sandstone"),
            15 => s("smooth_quartz"),
            _ => slab(STONE_SLABS[(m & 7) as usize], m, true),
        },
        44 => slab(STONE_SLABS[(m & 7) as usize], m, false),
        45 => s("bricks"),
        46 => s("tnt").with("unstable", "false"),
        47 => s("bookshelf"),
        48 => s("mossy_cobblestone"),
        49 => s("obsidian"),
        50 => torch("torch", "wall_torch", m, None),
        51 => age("fire", m, 15),
        52 => s("spawner"),
        53 => stairs("oak_stairs", m),
        54 => facing2("chest", m).with("type", "single"),
        55 => s("redstone_wire").with("power", &m.to_string()),
        56 => s("diamond_ore"),
        57 => s("diamond_block"),
        58 => s("crafting_table"),
        59 => age("wheat", m, 7),
        60 => s("farmland").with("moisture", &m.min(7).to_string()),
        61 | 62 => facing2("furnace", m).with("lit", bool_str(id == 62)),
        63 => s("oak_sign").with("rotation", &m.to_string()),
        64 => door("oak_door", if m & 8 != 0 { 0 } else { m }, if m & 8 != 0 { m } else { 0 }, m & 8 != 0),
        65 => facing2("ladder", m),
        66 => rail("rail", m, false),
        67 => stairs("cobblestone_stairs", m),
        68 => facing2("oak_wall_sign", m),
        69 => {
            let (face, facing) = match m & 7 {
                0 => ("ceiling", "west"),
                1 => ("wall", "east"),
                2 => ("wall", "west"),
                3 => ("wall", "south"),
                4 => ("wall", "north"),
                5 => ("floor", "north"),
                6 => ("floor", "west"),
                _ => ("ceiling", "north"),
            };
            s("lever").with("face", face).with("facing", facing).with("powered", bool_str(m & 8 != 0))
        }
        70 => s("stone_pressure_plate").with("powered", bool_str(m & 1 != 0)),
        71 => door("iron_door", if m & 8 != 0 { 0 } else { m }, if m & 8 != 0 { m } else { 0 }, m & 8 != 0),
        72 => s("oak_pressure_plate").with("powered", bool_str(m & 1 != 0)),
        73 | 74 => s("redstone_ore").with("lit", bool_str(id == 74)),
        75 | 76 => torch("redstone_torch", "redstone_wall_torch", m, Some(id == 76)),
        77 => button("stone_button", m),
        78 => s("snow").with("layers", &((m & 7) + 1).to_string()),
        79 => s("ice"),
        80 => s("snow_block"),
        81 => age("cactus", m, 15),
        82 => s("clay"),
        83 => age("sugar_cane", m, 15),
        84 => s("jukebox").with("has_record", "false"),
        85 => s("oak_fence"),
        86 => horizontal("carved_pumpkin", m),
        87 => s("netherrack"),
        88 => s("soul_sand"),
        89 => s("glowstone"),
        90 => s("nether_portal").with("axis", if m == 2 { "z" } else { "x" }),
        91 => horizontal("jack_o_lantern", m),
        92 => s("cake").with("bites", &m.min(6).to_string()),
        93 | 94 => horizontal("repeater", m)
            .with("delay", &((m >> 2) + 1).to_string())
            .with("powered", bool_str(id == 94))
            .with("locked", "false"),
        95 => s(&format!("{color}_stained_glass")),
        96 | 167 => {
            let facing = ["north", "south", "west", "east"][(m & 3) as usize];
            s(if id == 96 { "oak_trapdoor" } else { "iron_trapdoor" })
                .with("facing", facing)
                .with("open", bool_str(m & 4 != 0))
                .with("half", if m & 8 != 0 { "top" } else { "bottom" })
                .with("powered", "false")
                .with("waterlogged", "false")
        }
        97 => s([
            "infested_stone", "infested_cobblestone", "infested_stone_bricks", "infested_mossy_stone_bricks",
            "infested_cracked_stone_bricks", "infested_chiseled_stone_bricks",
        ][(m as usize).min(5)]),
        98 => s(["stone_bricks", "mossy_stone_bricks", "cracked_stone_bricks", "chiseled_stone_bricks"][(m as usize).min(3)]),
        99 => mushroom_block("brown_mushroom_block", m),
        100 => mushroom_block("red_mushroom_block", m),
        101 => s("iron_bars"),
        102 => s("glass_pane"),
        103 => s("melon"),
        104 => age("pumpkin_stem", m, 7),
        105 => age("melon_stem", m, 7),
        106 => s("vine")
            .with("south", bool_str(m & 1 != 0))
            .with("west", bool_str(m & 2 != 0))
            .with("north", bool_str(m & 4 != 0))
            .with("east", bool_str(m & 8 != 0))
            .with("up", "false"),
        107 => gate("oak_fence_gate", m),
        108 => stairs("brick_stairs", m),
        109 => stairs("stone_brick_stairs", m),
        110 => s("mycelium").with("snowy", "false"),
        111 => s("lily_pad"),
        112 => s("nether_bricks"),
        113 => s("nether_brick_fence"),
        114 => stairs("nether_brick_stairs", m),
        115 => age("nether_wart", m, 3),
        116 => s("enchanting_table"),
        117 => s("brewing_stand"),
        118 => match m & 3 {
            0 => s("cauldron"),
            n => level("water_cauldron", n),
        },
        119 => s("end_portal"),
        120 => horizontal("end_portal_frame", m).with("eye", bool_str(m & 4 != 0)),
        121 => s("end_stone"),
        122 => s("dragon_egg"),
        123 | 124 => s("redstone_lamp").with("lit", bool_str(id == 124)),
        125 => slab(&format!("{wood}_slab"), m, true),
        126 => slab(&format!("{wood}_slab"), m, false),
        127 => horizontal("cocoa", m).with("age", &(m >> 2).min(2).to_string()),
        128 => stairs("sandstone_stairs", m),
        129 => s("emerald_ore"),
        130 => facing2("ender_chest", m),
        131 => horizontal("tripwire_hook", m)
            .with("attached", bool_str(m & 4 != 0))
            .with("powered", bool_str(m & 8 != 0)),
        132 => s("tripwire")
            .with("powered", bool_str(m & 1 != 0))
            .with("attached", bool_str(m & 4 != 0))
            .with("disarmed", bool_str(m & 8 != 0)),
        133 => s("emerald_block"),
        134 => stairs("spruce_stairs", m),
        135 => stairs("birch_stairs", m),
        136 => stairs("jungle_stairs", m),
        137 | 210 | 211 => {
            let name = ["command_block", "repeating_command_block", "chain_command_block"][match id {
                137 => 0,
                210 => 1,
                _ => 2,
            }];
            facing6(name, m).with("conditional", bool_str(m & 8 != 0))
        }
        138 => s("beacon"),
        139 => s(if m == 1 { "mossy_cobblestone_wall" } else { "cobblestone_wall" }),
        140 => s("flower_pot"),
        141 => age("carrots", m, 7),
        142 => age("potatoes", m, 7),
        143 => button("oak_button", m),
        144 => match m & 7 {
            1 | 0 => s("skeleton_skull").with("rotation", "0"),
            _ => facing2("skeleton_wall_skull", m),
        },
        145 => {
            let name = ["anvil", "chipped_anvil", "damaged_anvil"][((m >> 2) as usize).min(2)];
            horizontal(name, m)
        }
        146 => facing2("trapped_chest", m).with("type", "single"),
        147 => s("light_weighted_pressure_plate").with("power", &m.to_string()),
        148 => s("heavy_weighted_pressure_plate").with("power", &m.to_string()),
        149 | 150 => horizontal("comparator", m)
            .with("mode", if m & 4 != 0 { "subtract" } else { "compare" })
            .with("powered", bool_str(m & 8 != 0 || id == 150)),
        151 | 178 => s("daylight_detector").with("inverted", bool_str(id == 178)).with("power", &m.to_string()),
        152 => s("redstone_block"),
        153 => s("nether_quartz_ore"),
        154 => {
            let facing = match m & 7 {
                2 => "north",
                3 => "south",
                4 => "west",
                5 => "east",
                _ => "down",
            };
            s("hopper").with("facing", facing).with("enabled", bool_str(m & 8 == 0))
        }
        155 => match m {
            1 => s("chiseled_quartz_block"),
            2 => s("quartz_pillar").with("axis", "y"),
            3 => s("quartz_pillar").with("axis", "x"),
            4 => s("quartz_pillar").with("axis", "z"),
            _ => s("quartz_block"),
        },
        156 => stairs("quartz_stairs", m),
        157 => rail("activator_rail", m, true),
        158 => facing6("dropper", m).with("triggered", bool_str(m & 8 != 0)),
        159 => s(&format!("{color}_terracotta")),
        160 => s(&format!("{color}_stained_glass_pane")),
        162 => log(WOODS[4 + (m & 1) as usize], m),
        163 => stairs("acacia_stairs", m),
        164 => stairs("dark_oak_stairs", m),
        165 => s("slime_block"),
        166 => s("barrier"),
        168 => s(["prismarine", "prismarine_bricks", "dark_prismarine"][(m as usize).min(2)]),
        169 => s("sea_lantern"),
        170 => axis("hay_block", m),
        171 => s(&format!("{color}_carpet")),
        172 => s("terracotta"),
        173 => s("coal_block"),
        174 => s("packed_ice"),
        175 => double_plant(if m & 8 != 0 { 0 } else { m }, m & 8 != 0),
        176 => s("white_banner").with("rotation", &m.to_string()),
        177 => facing2("white_wall_banner", m),
        179 => s(["red_sandstone", "chiseled_red_sandstone", "cut_red_sandstone"][(m as usize).min(2)]),
        180 => stairs("red_sandstone_stairs", m),
        181 => {
            if m == 8 {
                s("smooth_red_sandstone")
            } else {
                slab("red_sandstone_slab", m, true)
            }
        }
        182 => slab("red_sandstone_slab", m, false),
        183 => gate("spruce_fence_gate", m),
        184 => gate("birch_fence_gate", m),
        185 => gate("jungle_fence_gate", m),
        186 => gate("dark_oak_fence_gate", m),
        187 => gate("acacia_fence_gate", m),
        188 => s("spruce_fence"),
        189 => s("birch_fence"),
        190 => s("jungle_fence"),
        191 => s("dark_oak_fence"),
        192 => s("acacia_fence"),
        193..=197 => {
            let name = ["spruce_door", "birch_door", "jungle_door", "acacia_door", "dark_oak_door"][(id - 193) as usize];
            door(name, if m & 8 != 0 { 0 } else { m }, if m & 8 != 0 { m } else { 0 }, m & 8 != 0)
        }
        198 => facing6("end_rod", m),
        199 => s("chorus_plant"),
        200 => age("chorus_flower", m, 5),
        201 => s("purpur_block"),
        202 => axis("purpur_pillar", m),
        203 => stairs("purpur_stairs", m),
        204 => slab("purpur_slab", m, true),
        205 => slab("purpur_slab", m, false),
        206 => s("end_stone_bricks"),
        207 => age("beetroots", m, 3),
        208 => s("dirt_path"),
        209 => s("end_gateway"),
        212 => age("frosted_ice", m, 3),
        213 => s("magma_block"),
        214 => s("nether_wart_block"),
        215 => s("red_nether_bricks"),
        216 => axis("bone_block", m),
        217 => s("structure_void"),
        218 => facing6("observer", m).with("powered", bool_str(m & 8 != 0)),
        219..=234 => facing6(&format!("{}_shulker_box", COLORS[(id - 219) as usize]), m),
        235..=250 => horizontal(&format!("{}_glazed_terracotta", COLORS[(id - 235) as usize]), m),
        251 => s(&format!("{color}_concrete")),
        252 => s(&format!("{color}_concrete_powder")),
        255 => s("structure_block").with("mode", ["save", "load", "corner", "data"][(m as usize).min(3)]),
        _ => BlockState::air(),
    }
}

fn gate(name: &str, m: u8) -> BlockState {
    horizontal(name, m)
        .with("open", bool_str(m & 4 != 0))
        .with("powered", bool_str(m & 8 != 0))
        .with("in_wall", "false")
}

pub fn is_door(id: u16) -> bool {
    matches!(id, 64 | 71 | 193..=197)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_blocks() {
        assert_eq!(state(1, 3).short(), "diorite");
        assert_eq!(state(35, 14).short(), "red_wool");
        assert_eq!(state(17, 4 | 2).to_string(), "minecraft:birch_log[axis=x]");
        assert_eq!(state(53, 7).to_string(), "minecraft:oak_stairs[facing=north,half=top,shape=straight]");
        assert_eq!(state(44, 8 | 3).to_string(), "minecraft:cobblestone_slab[type=top]");
        assert_eq!(state(50, 5).short(), "torch");
        assert_eq!(state(50, 2).to_string(), "minecraft:wall_torch[facing=west]");
        assert_eq!(state(235 + 8, 0).short(), "light_gray_glazed_terracotta");
        assert_eq!(state(54, 5).get("facing"), Some("east"));
        assert_eq!(id_of("minecraft:stained_hardened_clay"), Some(159));
        assert_eq!(id_of(""), None);
        assert_eq!(NAMES[255], "structure_block");
    }

    #[test]
    fn doors_join_their_halves() {
        let d = door("oak_door", 1 | 4, 8 | 1, true);
        assert_eq!(d.to_string(), "minecraft:oak_door[facing=south,half=upper,hinge=right,open=true,powered=false]");
    }
}
