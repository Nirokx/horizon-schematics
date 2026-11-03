//! Bedrock Edition's `.mcstructure`: little-endian NBT, blocks in two layers
//! (the second holds the water a block is logged in), z fastest, then y,
//! then x. Bedrock's block names and states are translated to Java's where
//! they differ; what has no translation keeps its name.

use crate::nbt::{Compound, Result, Tag, err};
use crate::schematic::{BlockState, Entity, Format, Region, Schematic};
use std::collections::HashMap;

pub fn is_mcstructure(root: &Tag) -> bool {
    root.get("structure").is_some() && root.get("size").is_some() && root.get("format_version").is_some()
}

pub fn read(root: &Tag, name: &str) -> Result<Schematic> {
    let Some(c) = root.as_compound() else {
        return err("not a compound");
    };
    let Some(size) = c.get("size").and_then(Tag::as_ints).filter(|s| s.len() == 3) else {
        return err("a structure without a size");
    };
    let size = [size[0], size[1], size[2]];
    let volume: u64 = size.iter().map(|&v| v.max(0) as u64).product();
    if volume == 0 {
        return err("an empty structure");
    }
    if volume > crate::MAX_VOLUME {
        return err(format!("the structure is too large ({volume} blocks)"));
    }
    let Some(structure) = c.compound("structure") else {
        return err("no structure");
    };
    let layers = structure.list("block_indices").unwrap_or(&[]);
    let layer = |i: usize| layers.get(i).and_then(Tag::as_ints).unwrap_or_default();
    let (main, water) = (layer(0), layer(1));
    let palette_tag = structure.compound("palette").and_then(|p| p.compound("default"));
    let palette: Vec<BlockState> = palette_tag
        .and_then(|p| p.list("block_palette"))
        .unwrap_or(&[])
        .iter()
        .map(|t| {
            let c = t.as_compound();
            let name = c.and_then(|c| c.str("name")).unwrap_or("minecraft:air");
            let empty = Compound::new();
            let states = c.and_then(|c| c.compound("states")).unwrap_or(&empty);
            translate(name, states)
        })
        .collect();
    let waterish: Vec<bool> = palette.iter().map(|s| s.short() == "water").collect();
    let mut region = Region::new("Main", [0; 3], size);
    let mut cache: HashMap<(u32, bool), u32> = HashMap::new();
    let [sx, sy, sz] = size;
    for x in 0..sx {
        for y in 0..sy {
            for z in 0..sz {
                let i = ((x * sy + y) * sz + z) as usize;
                let Some(&p) = main.get(i) else { continue };
                if p < 0 || p as usize >= palette.len() {
                    // -1: structure void.
                    continue;
                }
                let logged = water.get(i).is_some_and(|&w| w >= 0 && waterish.get(w as usize).copied().unwrap_or(false));
                let id = *cache.entry((p as u32, logged)).or_insert_with(|| {
                    let mut state = palette[p as usize].clone();
                    if logged && state.short() != "water" {
                        state.set("waterlogged", "true");
                    }
                    region.intern(state)
                });
                let at = region.index(x, y, z).unwrap();
                region.blocks[at] = id;
            }
        }
    }
    if let Some(data) = palette_tag.and_then(|p| p.compound("block_position_data")) {
        for (k, v) in data.iter() {
            let Ok(i) = k.parse::<i64>() else { continue };
            let Some(be) = v.get("block_entity_data").and_then(Tag::as_compound) else { continue };
            let (yz, z) = (i / i64::from(sz), i % i64::from(sz));
            let (x, y) = (yz / i64::from(sy), yz % i64::from(sy));
            let mut tag = be.clone();
            for k in ["x", "y", "z", "isMovable"] {
                tag.remove(k);
            }
            if let Some(id) = tag.str("id") {
                let id = bedrock_block_entity(id);
                tag.remove("id");
                tag.0.insert(0, ("id".into(), Tag::String(id)));
            }
            region.block_entities.insert([x as i32, y as i32, z as i32], tag);
        }
    }
    let origin = structure_origin(c);
    for e in structure.list("entities").unwrap_or(&[]) {
        let Some(e) = e.as_compound() else { continue };
        let Some(p) = e.get("Pos").and_then(Tag::as_doubles).filter(|p| p.len() == 3) else { continue };
        let mut nbt = Compound::new();
        if let Some(id) = e.str("identifier") {
            nbt.insert("id", Tag::String(id.to_owned()));
        }
        let pos = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        region.entities.push(Entity { pos, nbt });
    }
    region.compact();
    Ok(Schematic::new(name, Format::Bedrock, vec![region]))
}

fn structure_origin(c: &Compound) -> [f64; 3] {
    c.get("structure_world_origin")
        .and_then(Tag::as_ints)
        .filter(|o| o.len() == 3)
        .map_or([0.0; 3], |o| [o[0].into(), o[1].into(), o[2].into()])
}

fn bedrock_block_entity(id: &str) -> String {
    let java = match id {
        "Chest" => "chest",
        "Furnace" => "furnace",
        "BlastFurnace" => "blast_furnace",
        "Smoker" => "smoker",
        "Barrel" => "barrel",
        "Sign" => "sign",
        "HangingSign" => "hanging_sign",
        "Hopper" => "hopper",
        "Dispenser" => "dispenser",
        "Dropper" => "dropper",
        "ShulkerBox" => "shulker_box",
        "Banner" => "banner",
        "Bed" => "bed",
        "Skull" => "skull",
        "FlowerPot" => "flower_pot",
        "BrewingStand" => "brewing_stand",
        "EnderChest" => "ender_chest",
        "Beacon" => "beacon",
        "MobSpawner" => "mob_spawner",
        "Lectern" => "lectern",
        "Campfire" => "campfire",
        "Bell" => "bell",
        "Jukebox" => "jukebox",
        "CommandBlock" => "command_block",
        other => return super::namespaced_id(&other.to_ascii_lowercase()),
    };
    format!("minecraft:{java}")
}

fn state_str(t: &Tag) -> String {
    match t {
        Tag::String(s) => s.clone(),
        other => other.as_i64().map(|v| v.to_string()).unwrap_or_default(),
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

const FACING6: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// A Bedrock block state as Java's.
pub fn translate(name: &str, states: &Compound) -> BlockState {
    let get = |k: &str| states.get(k).map(state_str);
    let short = name.strip_prefix("minecraft:").unwrap_or(name);
    let color = get("color").map(|c| if c == "silver" { "light_gray".to_owned() } else { c });
    let wood = get("wood_type").map(|w| if w == "dark_oak" { w } else { w.replace("darkoak", "dark_oak") });

    // Names that changed, or that carry their variant in a state.
    let renamed: String = match short {
        "grass" => "grass_block".into(),
        "tallgrass" => match get("tall_grass_type").as_deref() {
            Some("fern") => "fern".into(),
            _ => "short_grass".into(),
        },
        "snow_layer" => "snow".into(),
        "snow" => "snow_block".into(),
        "lit_pumpkin" => "jack_o_lantern".into(),
        "pumpkin" => "carved_pumpkin".into(),
        "waterlily" => "lily_pad".into(),
        "web" => "cobweb".into(),
        "brick_block" => "bricks".into(),
        "stonebrick" => match get("stone_brick_type").as_deref() {
            Some("mossy") => "mossy_stone_bricks".into(),
            Some("cracked") => "cracked_stone_bricks".into(),
            Some("chiseled") => "chiseled_stone_bricks".into(),
            _ => "stone_bricks".into(),
        },
        "quartz_ore" => "nether_quartz_ore".into(),
        "magma" => "magma_block".into(),
        "slime" => "slime_block".into(),
        "hardened_clay" => "terracotta".into(),
        "stained_hardened_clay" => format!("{}_terracotta", color.as_deref().unwrap_or("white")),
        "mob_spawner" => "spawner".into(),
        "noteblock" => "note_block".into(),
        "golden_rail" => "powered_rail".into(),
        "unlit_redstone_torch" | "redstone_torch" => "redstone_torch".into(),
        "lit_redstone_lamp" => "redstone_lamp".into(),
        "lit_furnace" => "furnace".into(),
        "lit_smoker" => "smoker".into(),
        "lit_blast_furnace" => "blast_furnace".into(),
        "lit_redstone_ore" => "redstone_ore".into(),
        "lit_deepslate_redstone_ore" => "deepslate_redstone_ore".into(),
        "powered_repeater" | "unpowered_repeater" => "repeater".into(),
        "powered_comparator" | "unpowered_comparator" => "comparator".into(),
        "daylight_detector_inverted" => "daylight_detector".into(),
        "standing_sign" => "oak_sign".into(),
        "wall_sign" => "oak_wall_sign".into(),
        "darkoak_standing_sign" => "dark_oak_sign".into(),
        "darkoak_wall_sign" => "dark_oak_wall_sign".into(),
        "standing_banner" => "white_banner".into(),
        "wall_banner" => "white_wall_banner".into(),
        "wooden_door" => "oak_door".into(),
        "wooden_button" => "oak_button".into(),
        "wooden_pressure_plate" => "oak_pressure_plate".into(),
        "trapdoor" => "oak_trapdoor".into(),
        "fence_gate" => "oak_fence_gate".into(),
        "portal" => "nether_portal".into(),
        "invisible_bedrock" => "barrier".into(),
        "grass_path" => "dirt_path".into(),
        "melon_block" => "melon".into(),
        "reeds" => "sugar_cane".into(),
        "undyed_shulker_box" => "shulker_box".into(),
        "chain" => "iron_chain".into(),
        "frame" | "glow_frame" | "info_update" | "info_update2" | "reserved6" | "camera" | "border_block"
        | "allow" | "deny" => "air".into(),
        "wool" | "carpet" | "concrete" | "concrete_powder" | "stained_glass" | "stained_glass_pane" | "shulker_box" => {
            format!("{}_{short}", color.as_deref().unwrap_or("white"))
        }
        "planks" | "fence" => format!("{}_{short}", wood.as_deref().unwrap_or("oak")),
        "log" => format!("{}_log", get("old_log_type").as_deref().unwrap_or("oak")),
        "log2" => format!("{}_log", get("new_log_type").as_deref().unwrap_or("acacia")),
        "leaves" => format!("{}_leaves", get("old_leaf_type").as_deref().unwrap_or("oak")),
        "leaves2" => format!("{}_leaves", get("new_leaf_type").as_deref().unwrap_or("acacia")),
        "sapling" => format!("{}_sapling", get("sapling_type").as_deref().unwrap_or("oak")),
        "stone" => match get("stone_type").as_deref() {
            Some("granite_smooth") => "polished_granite".into(),
            Some("diorite_smooth") => "polished_diorite".into(),
            Some("andesite_smooth") => "polished_andesite".into(),
            Some(t @ ("granite" | "diorite" | "andesite")) => t.into(),
            _ => "stone".into(),
        },
        "red_flower" => match get("flower_type").as_deref() {
            Some("orchid") => "blue_orchid".into(),
            Some("houstonia") => "azure_bluet".into(),
            Some("tulip_red") => "red_tulip".into(),
            Some("tulip_orange") => "orange_tulip".into(),
            Some("tulip_white") => "white_tulip".into(),
            Some("tulip_pink") => "pink_tulip".into(),
            Some("oxeye") => "oxeye_daisy".into(),
            Some("cornflower") => "cornflower".into(),
            Some("lily_of_the_valley") => "lily_of_the_valley".into(),
            Some("allium") => "allium".into(),
            _ => "poppy".into(),
        },
        "yellow_flower" => "dandelion".into(),
        "double_plant" => match get("double_plant_type").as_deref() {
            Some("syringa") => "lilac".into(),
            Some("grass") => "tall_grass".into(),
            Some("fern") => "large_fern".into(),
            Some("rose") => "rose_bush".into(),
            Some("paeonia") => "peony".into(),
            _ => "sunflower".into(),
        },
        other if other.ends_with("_double_slab") => format!("{}_slab", other.trim_end_matches("_double_slab")),
        other if other.ends_with("_standing_sign") => format!("{}_sign", other.trim_end_matches("_standing_sign")),
        other => other.into(),
    };
    let mut s = BlockState::new(&format!("minecraft:{renamed}"));
    if !name.starts_with("minecraft:") && name.contains(':') {
        s = BlockState::new(name);
    }
    let java = s.short().to_owned();

    match short {
        "lit_furnace" | "lit_smoker" | "lit_blast_furnace" | "lit_redstone_lamp" | "lit_redstone_ore"
        | "lit_deepslate_redstone_ore" | "redstone_torch" => s.set("lit", "true"),
        "furnace" | "smoker" | "blast_furnace" | "redstone_lamp" | "redstone_ore" | "unlit_redstone_torch" => {
            s.set("lit", "false")
        }
        "powered_repeater" | "powered_comparator" => s.set("powered", "true"),
        "daylight_detector_inverted" => s.set("inverted", "true"),
        _ => {}
    }
    if short.ends_with("_double_slab") {
        s.set("type", "double");
    }

    for (key, value) in states.iter() {
        let v = state_str(value);
        let n = value.as_i32().unwrap_or(0);
        match key {
            "pillar_axis" => s.set("axis", &v),
            "minecraft:cardinal_direction" | "minecraft:facing_direction" | "minecraft:block_face" => s.set("facing", &v),
            "facing_direction" => {
                let f = FACING6[(n.clamp(0, 5)) as usize];
                if java.ends_with("_button") {
                    let (face, facing) = match n {
                        0 => ("ceiling", "north"),
                        1 => ("floor", "north"),
                        _ => ("wall", f),
                    };
                    s.set("face", face);
                    s.set("facing", facing);
                } else {
                    s.set("facing", f);
                }
            }
            "direction" => {
                let f = if java.ends_with("_door") {
                    ["east", "south", "west", "north"][(n & 3) as usize]
                } else if java.ends_with("_trapdoor") {
                    ["east", "west", "south", "north"][(n & 3) as usize]
                } else {
                    ["south", "west", "north", "east"][(n & 3) as usize]
                };
                s.set("facing", f);
            }
            "weirdo_direction" => s.set("facing", ["east", "west", "south", "north"][(n & 3) as usize]),
            "upside_down_bit" => s.set("half", if n != 0 { "top" } else { "bottom" }),
            "minecraft:vertical_half" => {
                if !short.ends_with("_double_slab") {
                    s.set("type", &v)
                }
            }
            "top_slot_bit" => s.set("type", if n != 0 { "top" } else { "bottom" }),
            "open_bit" => s.set("open", bool_str(n != 0)),
            "upper_block_bit" => s.set("half", if n != 0 { "upper" } else { "lower" }),
            "door_hinge_bit" => s.set("hinge", if n != 0 { "right" } else { "left" }),
            "powered_bit" | "button_pressed_bit" | "rail_data_bit" | "output_lit_bit" => s.set("powered", bool_str(n != 0)),
            "persistent_bit" => s.set("persistent", bool_str(n != 0)),
            "growth" => s.set("age", &v),
            "rail_direction" => {
                const SHAPES: [&str; 10] = [
                    "north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south",
                    "south_east", "south_west", "north_west", "north_east",
                ];
                s.set("shape", SHAPES[(n.clamp(0, 9)) as usize]);
            }
            "redstone_signal" => s.set("power", &v),
            "head_piece_bit" => s.set("part", if n != 0 { "head" } else { "foot" }),
            "occupied_bit" => s.set("occupied", bool_str(n != 0)),
            "candles" => s.set("candles", &(n + 1).to_string()),
            "cluster_count" => s.set("pickles", &(n + 1).to_string()),
            "height" if java == "snow" => s.set("layers", &(n + 1).to_string()),
            "liquid_depth" => s.set("level", &v),
            "bite_counter" => s.set("bites", &v),
            "torch_facing_direction" => {
                if v != "top" && v != "unknown" {
                    let wall = java.replace("torch", "wall_torch");
                    let lit = s.get("lit").map(str::to_owned);
                    s = BlockState::new(&wall).with("facing", &v);
                    if let Some(lit) = lit {
                        s.set("lit", &lit);
                    }
                }
            }
            "ground_sign_direction" => s.set("rotation", &v),
            "repeater_delay" => s.set("delay", &(n + 1).to_string()),
            "output_subtract_bit" => s.set("mode", if n != 0 { "subtract" } else { "compare" }),
            "triggered_bit" => s.set("triggered", bool_str(n != 0)),
            "conditional_bit" => s.set("conditional", bool_str(n != 0)),
            "toggle_bit" => s.set("enabled", bool_str(n == 0)),
            "in_wall_bit" => s.set("in_wall", bool_str(n != 0)),
            "attached_bit" => s.set("attached", bool_str(n != 0)),
            "disarmed_bit" => s.set("disarmed", bool_str(n != 0)),
            "end_portal_eye_bit" => s.set("eye", bool_str(n != 0)),
            "age" | "lit" | "hanging" | "facing" | "axis" | "half" | "type" | "level" | "power" | "rotation" => {
                s.set(key, &v)
            }
            "wall_post_bit" => s.set("up", bool_str(n != 0)),
            k if k.starts_with("wall_connection_type_") => {
                let side = k.trim_start_matches("wall_connection_type_");
                s.set(side, match v.as_str() {
                    "short" => "low",
                    "tall" => "tall",
                    _ => "none",
                });
            }
            "vine_direction_bits" => {
                for (bit, side) in [(1, "south"), (2, "west"), (4, "north"), (8, "east")] {
                    s.set(side, bool_str(n & bit != 0));
                }
            }
            "multi_face_direction_bits" => {
                for (bit, side) in [(1, "down"), (2, "up"), (4, "south"), (8, "west"), (16, "north"), (32, "east")] {
                    s.set(side, bool_str(n & bit != 0));
                }
            }
            "lever_direction" => {
                let (face, facing) = match v.as_str() {
                    "down_east_west" => ("ceiling", "west"),
                    "down_north_south" => ("ceiling", "north"),
                    "up_east_west" => ("floor", "west"),
                    "up_north_south" => ("floor", "north"),
                    side => ("wall", side),
                };
                s.set("face", face);
                s.set("facing", facing);
            }
            "attachment" if java == "bell" => s.set(
                "attachment",
                match v.as_str() {
                    "standing" => "floor",
                    "hanging" => "ceiling",
                    "side" => "single_wall",
                    _ => "double_wall",
                },
            ),
            _ => {}
        }
    }
    if java.ends_with("_stairs") && s.get("shape").is_none() {
        s.set("shape", "straight");
    }
    if (java.ends_with("_leaves")) && s.get("distance").is_none() {
        s.set("distance", "7");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::compound;

    fn states(pairs: Vec<(&str, Tag)>) -> Compound {
        Compound(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    #[test]
    fn translates_common_states() {
        let stairs = translate(
            "minecraft:oak_stairs",
            &states(vec![("weirdo_direction", Tag::Int(3)), ("upside_down_bit", Tag::Byte(1))]),
        );
        assert_eq!(stairs.to_string(), "minecraft:oak_stairs[facing=north,half=top,shape=straight]");
        let log = translate("minecraft:oak_log", &states(vec![("pillar_axis", Tag::String("x".into()))]));
        assert_eq!(log.to_string(), "minecraft:oak_log[axis=x]");
        let wool = translate("minecraft:wool", &states(vec![("color", Tag::String("silver".into()))]));
        assert_eq!(wool.short(), "light_gray_wool");
        let slab = translate("minecraft:stone_brick_double_slab", &Compound::new());
        assert_eq!(slab.to_string(), "minecraft:stone_brick_slab[type=double]");
        let torch = translate("minecraft:torch", &states(vec![("torch_facing_direction", Tag::String("west".into()))]));
        assert_eq!(torch.to_string(), "minecraft:wall_torch[facing=west]");
    }

    #[test]
    fn reads_z_fastest_with_water_layer() {
        let palette = Tag::List(vec![
            compound([("name", Tag::String("minecraft:stone".into())), ("states", compound([]))]),
            compound([("name", Tag::String("minecraft:water".into())), ("states", compound([("liquid_depth", Tag::Int(0))]))]),
            compound([("name", Tag::String("minecraft:oak_fence".into())), ("states", compound([]))]),
        ]);
        let root = compound([
            ("format_version", Tag::Int(1)),
            ("size", Tag::List(vec![Tag::Int(1), Tag::Int(1), Tag::Int(2)])),
            (
                "structure",
                compound([
                    (
                        "block_indices",
                        Tag::List(vec![
                            Tag::List(vec![Tag::Int(0), Tag::Int(2)]),
                            Tag::List(vec![Tag::Int(-1), Tag::Int(1)]),
                        ]),
                    ),
                    ("entities", Tag::List(vec![])),
                    ("palette", compound([("default", compound([("block_palette", palette)]))])),
                ]),
            ),
        ]);
        assert!(is_mcstructure(&root));
        let s = read(&root, "b").unwrap();
        let r = &s.regions[0];
        assert_eq!(r.state(0, 0, 0).short(), "stone");
        assert_eq!(r.state(0, 0, 1).to_string(), "minecraft:oak_fence[waterlogged=true]");
    }
}
