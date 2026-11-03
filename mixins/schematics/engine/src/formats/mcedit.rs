//! MCEdit's `.schematic` (also WorldEdit 6's and Schematica's): numeric block
//! ids in `Blocks` (with the high bits in `AddBlocks`) and data values in
//! `Data`, y slowest, then z, then x. Schematica's `SchematicaMapping` and
//! MCEdit-Unified's `BlockIDs` name the ids when they are not vanilla's.

use super::legacy;
use crate::nbt::{Compound, Result, Tag, err};
use crate::schematic::{BlockState, Entity, Format, Region, Schematic};
use std::collections::HashMap;

pub fn is_mcedit(root: &Tag) -> bool {
    matches!(root.get("Blocks"), Some(Tag::ByteArray(_))) && root.get("Width").is_some()
}

fn bytes<'a>(c: &'a Compound, key: &str) -> &'a [i8] {
    match c.get(key) {
        Some(Tag::ByteArray(b)) => b,
        _ => &[],
    }
}

pub fn read(root: &Tag, name: &str) -> Result<Schematic> {
    let Some(c) = root.as_compound() else {
        return err("not a compound");
    };
    if c.str("Materials").is_some_and(|m| m != "Alpha") {
        return err(format!("{} schematics (pre-Alpha or Pocket Edition) are not supported", c.str("Materials").unwrap_or("")));
    }
    let dim = |k: &str| c.get(k).and_then(Tag::as_i32).map_or(0, |v| v as u16 as i32);
    let size = [dim("Width"), dim("Height"), dim("Length")];
    let volume: u64 = size.iter().map(|&v| v as u64).product();
    if volume == 0 {
        return err("an empty schematic");
    }
    if volume > crate::MAX_VOLUME {
        return err(format!("the schematic is too large ({volume} blocks)"));
    }
    let volume = volume as usize;
    let blocks = bytes(c, "Blocks");
    let data = bytes(c, "Data");
    let add = bytes(c, "AddBlocks");
    // Schematica's own "Add" holds whole high bytes rather than nibbles.
    let add_bytes = bytes(c, "Add");
    if blocks.len() < volume {
        return err(format!("Blocks holds {} of {volume} blocks", blocks.len()));
    }
    let id_at = |i: usize| -> u16 {
        let low = blocks[i] as u8 as u16;
        let high = if !add.is_empty() {
            let b = add.get(i >> 1).copied().unwrap_or(0) as u8;
            u16::from(if i & 1 == 0 { b >> 4 } else { b & 15 })
        } else {
            u16::from(add_bytes.get(i).copied().unwrap_or(0) as u8)
        };
        high << 8 | low
    };
    let meta_at = |i: usize| data.get(i).copied().unwrap_or(0) as u8 & 15;
    // File ids by name, when the file names them: mapped onto vanilla's.
    let mut remap: HashMap<u16, Option<u16>> = HashMap::new();
    let mut named: HashMap<u16, String> = HashMap::new();
    for key in ["SchematicaMapping", "BlockIDs"] {
        let Some(mapping) = c.compound(key) else { continue };
        for (k, v) in mapping.iter() {
            // Schematica: name -> id; MCEdit-Unified: "id" -> name.
            let (id, block) = match (v.as_i32(), v.as_str()) {
                (Some(id), _) => (id as u16, k.to_owned()),
                (None, Some(name)) => match k.parse::<u16>() {
                    Ok(id) => (id, name.to_owned()),
                    Err(_) => continue,
                },
                _ => continue,
            };
            remap.insert(id, legacy::id_of(&block));
            named.insert(id, block);
        }
    }
    let vanilla = |id: u16| -> Option<u16> {
        if remap.is_empty() {
            return (id < 256).then_some(id);
        }
        remap.get(&id).copied().flatten()
    };
    let [sx, sy, sz] = size;
    let index = |x: i32, y: i32, z: i32| ((y * sz + z) * sx + x) as usize;
    let mut region = Region::new("Main", [0; 3], size);
    let mut cache: HashMap<(u16, u8), u32> = HashMap::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let i = index(x, y, z);
                let raw = id_at(i);
                let meta = meta_at(i);
                let Some(id) = vanilla(raw) else {
                    // A modded block: keep its name when the file gives one.
                    if let Some(n) = named.get(&raw) {
                        let state = BlockState::new(n);
                        region.blocks[i] = region.intern(state);
                    }
                    continue;
                };
                // Doors and tall plants keep half of their state in each half.
                let state = if legacy::is_door(id) || id == 175 {
                    let top = meta & 8 != 0;
                    let other = if top { y - 1 } else { y + 1 };
                    let other_meta = (0..sy)
                        .contains(&other)
                        .then(|| index(x, other, z))
                        .filter(|&j| vanilla(id_at(j)) == Some(id))
                        .map(meta_at);
                    if id == 175 {
                        let lower = if top { other_meta.unwrap_or(0) } else { meta };
                        legacy::double_plant(lower, top)
                    } else {
                        let (lower, upper) = if top { (other_meta.unwrap_or(0), meta) } else { (meta, other_meta.unwrap_or(0)) };
                        let name = legacy::state(id, 0).name;
                        legacy::door(name.strip_prefix("minecraft:").unwrap_or(&name), lower, upper, top)
                    }
                } else {
                    let key = (id, meta);
                    if let Some(&p) = cache.get(&key) {
                        region.blocks[i] = p;
                        continue;
                    }
                    let p = region.intern(legacy::state(id, meta));
                    cache.insert(key, p);
                    region.blocks[i] = p;
                    continue;
                };
                region.blocks[i] = region.intern(state);
            }
        }
    }
    for t in c.list("TileEntities").unwrap_or(&[]) {
        let Some(be) = t.as_compound() else { continue };
        let (Some(x), Some(y), Some(z)) = (be.i32("x"), be.i32("y"), be.i32("z")) else { continue };
        let mut tag = be.clone();
        for k in ["x", "y", "z"] {
            tag.remove(k);
        }
        if let Some(id) = tag.str("id") {
            let id = upgrade_block_entity_id(id);
            tag.insert("id", Tag::String(id));
        }
        region.block_entities.insert([x, y, z], tag);
    }
    for e in c.list("Entities").unwrap_or(&[]) {
        let Some(e) = e.as_compound() else { continue };
        let Some(p) = e.get("Pos").and_then(Tag::as_doubles).filter(|p| p.len() == 3) else { continue };
        region.entities.push(Entity { pos: [p[0], p[1], p[2]], nbt: e.clone() });
    }
    region.compact();
    Ok(Schematic::new(name, Format::McEdit, vec![region]))
}

/// 1.11+ block entity ids from the old CamelCase ones.
fn upgrade_block_entity_id(id: &str) -> String {
    let new = match id {
        "Chest" => "chest",
        "Furnace" => "furnace",
        "Sign" => "sign",
        "MobSpawner" => "mob_spawner",
        "Trap" => "dispenser",
        "Dropper" => "dropper",
        "Music" => "noteblock",
        "RecordPlayer" => "jukebox",
        "Piston" => "piston",
        "Cauldron" => "brewing_stand",
        "EnchantTable" => "enchanting_table",
        "Airportal" => "end_portal",
        "Beacon" => "beacon",
        "Skull" => "skull",
        "DLDetector" => "daylight_detector",
        "Hopper" => "hopper",
        "Comparator" => "comparator",
        "FlowerPot" => "flower_pot",
        "Banner" => "banner",
        "Control" => "command_block",
        "EnderChest" => "ender_chest",
        "EndGateway" => "end_gateway",
        "Structure" => "structure_block",
        other => return super::namespaced_id(other),
    };
    format!("minecraft:{new}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::compound;

    fn file(blocks: Vec<i8>, data: Vec<i8>, size: [i16; 3]) -> Tag {
        compound([
            ("Width", Tag::Short(size[0])),
            ("Height", Tag::Short(size[1])),
            ("Length", Tag::Short(size[2])),
            ("Materials", Tag::String("Alpha".into())),
            ("Blocks", Tag::ByteArray(blocks)),
            ("Data", Tag::ByteArray(data)),
        ])
    }

    #[test]
    fn reads_ids_and_data_values() {
        // 2 wide, 1 high, 1 long: orange wool, then a birch plank.
        let s = read(&file(vec![35, 5], vec![1, 2], [2, 1, 1]), "w").unwrap();
        let r = &s.regions[0];
        assert_eq!(r.state(0, 0, 0).short(), "orange_wool");
        assert_eq!(r.state(1, 0, 0).short(), "birch_planks");
    }

    #[test]
    fn joins_door_halves() {
        // A 1x2x1 spruce door: lower facing north (3), open; upper hinge right.
        let s = read(&file(vec![193u8 as i8, 193u8 as i8], vec![3 | 4, 8 | 1], [1, 2, 1]), "d").unwrap();
        let r = &s.regions[0];
        let lower = r.state(0, 0, 0);
        let upper = r.state(0, 1, 0);
        assert_eq!(lower.get("facing"), Some("north"));
        assert_eq!(lower.get("hinge"), Some("right"));
        assert_eq!(upper.get("half"), Some("upper"));
        assert_eq!(upper.get("open"), Some("true"));
    }

    #[test]
    fn schematica_mapping_renames_ids() {
        let mut f = file(vec![1, 2], vec![0, 0], [2, 1, 1]);
        if let Tag::Compound(c) = &mut f {
            c.insert(
                "SchematicaMapping",
                compound([("minecraft:gold_block", Tag::Short(1)), ("mymod:thing", Tag::Short(2))]),
            );
        }
        let s = read(&f, "m").unwrap();
        assert_eq!(s.regions[0].state(0, 0, 0).short(), "gold_block");
        assert_eq!(s.regions[0].state(1, 0, 0).name, "mymod:thing");
    }
}
