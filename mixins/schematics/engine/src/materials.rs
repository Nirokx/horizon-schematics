//! What a block takes to place, for the material list: how many of its item
//! a state counts as (0 for the second half of a door, a bed's head...), and
//! names older files use for blocks that have since been renamed.

use crate::schematic::BlockState;

/// How many items placing `state` takes; 0 when the block comes with
/// another (the upper half of a door) or is not placed by hand.
pub fn item_count(state: &BlockState) -> u32 {
    let name = state.short();
    if state.is_air() {
        return 0;
    }
    match name {
        "piston_head" | "moving_piston" | "nether_portal" | "end_portal" | "end_gateway" | "fire" | "soul_fire"
        | "frosted_ice" | "bubble_column" | "tripwire" => return 0,
        _ => {}
    }
    if state.get("half") == Some("upper") || state.get("part") == Some("head") {
        return 0;
    }
    if name.ends_with("_slab") && state.get("type") == Some("double") {
        return 2;
    }
    for key in ["candles", "pickles", "eggs", "layers", "flower_amount", "segment_amount"] {
        if let Some(n) = state.get(key).and_then(|v| v.parse::<u32>().ok()) {
            return n.max(1);
        }
    }
    if matches!(name, "glow_lichen" | "sculk_vein" | "resin_clump") {
        let faces = ["north", "east", "south", "west", "up", "down"].iter().filter(|f| state.get(f) == Some("true")).count();
        return faces.max(1) as u32;
    }
    1
}

/// The item a block is placed with, where it isn't the block's own item
/// (water as a bucket), else None.
pub fn item_override(state: &BlockState) -> Option<&'static str> {
    Some(match state.short() {
        "water" | "bubble_column" => "minecraft:water_bucket",
        "lava" => "minecraft:lava_bucket",
        "powder_snow" => "minecraft:powder_snow_bucket",
        "redstone_wire" => "minecraft:redstone",
        "tripwire" => "minecraft:string",
        "wheat" => "minecraft:wheat_seeds",
        "carrots" => "minecraft:carrot",
        "potatoes" => "minecraft:potato",
        "beetroots" => "minecraft:beetroot_seeds",
        "pumpkin_stem" | "attached_pumpkin_stem" => "minecraft:pumpkin_seeds",
        "melon_stem" | "attached_melon_stem" => "minecraft:melon_seeds",
        "cocoa" => "minecraft:cocoa_beans",
        "sweet_berry_bush" => "minecraft:sweet_berries",
        "cave_vines" | "cave_vines_plant" => "minecraft:glow_berries",
        "kelp_plant" => "minecraft:kelp",
        "bamboo_sapling" => "minecraft:bamboo",
        "torchflower_crop" => "minecraft:torchflower_seeds",
        "pitcher_crop" => "minecraft:pitcher_pod",
        "farmland" | "dirt_path" => "minecraft:dirt",
        _ => return None,
    })
}

/// A newer name for a block an older version named otherwise, for files
/// saved before the rename.
pub fn renamed(name: &str) -> Option<&'static str> {
    Some(match name {
        "minecraft:grass" => "minecraft:short_grass",
        "minecraft:grass_path" => "minecraft:dirt_path",
        "minecraft:chain" => "minecraft:iron_chain",
        "minecraft:sign" => "minecraft:oak_sign",
        "minecraft:wall_sign" => "minecraft:oak_wall_sign",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_items_per_state() {
        assert_eq!(item_count(&BlockState::new("oak_door").with("half", "upper")), 0);
        assert_eq!(item_count(&BlockState::new("oak_door").with("half", "lower")), 1);
        assert_eq!(item_count(&BlockState::new("stone_slab").with("type", "double")), 2);
        assert_eq!(item_count(&BlockState::new("candle").with("candles", "3")), 3);
        assert_eq!(item_count(&BlockState::new("red_bed").with("part", "head")), 0);
        assert_eq!(item_count(&BlockState::air()), 0);
        assert_eq!(item_override(&BlockState::new("water")), Some("minecraft:water_bucket"));
    }
}
