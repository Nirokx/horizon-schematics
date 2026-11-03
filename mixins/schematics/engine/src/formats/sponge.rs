//! The Sponge schematic format (`.schem`, WorldEdit 7 and FAWE): versions 1
//! and 2 (a root named `Schematic`) and 3 (a `Schematic` compound inside
//! the root). Blocks are palette indices as varints, x fastest, then z, then y.

use crate::nbt::{Compound, Result, Tag, compound, err};
use crate::schematic::{BlockState, Entity, Format, Region, Schematic};
use std::collections::BTreeMap;

pub fn varints(bytes: &[i8], count: usize) -> Result<Vec<u32>> {
    let mut out = Vec::with_capacity(count);
    let mut value = 0u32;
    let mut shift = 0;
    for &b in bytes {
        let b = b as u8;
        value |= u32::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            out.push(value);
            if out.len() == count {
                break;
            }
            value = 0;
            shift = 0;
        } else {
            shift += 7;
            if shift > 28 {
                return err("a varint in BlockData is too long");
            }
        }
    }
    if out.len() < count {
        return err(format!("BlockData holds {} of {count} blocks", out.len()));
    }
    Ok(out)
}

pub fn write_varints(values: &[u32]) -> Vec<i8> {
    let mut out = Vec::with_capacity(values.len());
    for &v in values {
        let mut v = v;
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b as i8);
                break;
            }
            out.push((b | 0x80) as i8);
        }
    }
    out
}

fn dimension(c: &Compound, key: &str) -> i32 {
    // Shorts that are unsigned in the spec.
    match c.get(key) {
        Some(Tag::Short(v)) => i32::from(*v as u16),
        Some(t) => t.as_i32().unwrap_or(0),
        None => 0,
    }
}

/// The `Schematic` compound of a file, whichever version wrote it.
pub fn body<'a>(root_name: &str, root: &'a Tag) -> Option<&'a Compound> {
    let c = root.as_compound()?;
    if c.contains("Width") && (root_name == "Schematic" || c.contains("Palette") || c.contains("Blocks")) {
        return Some(c);
    }
    let inner = c.compound("Schematic")?;
    inner.contains("Width").then_some(inner)
}

pub fn is_sponge(root_name: &str, root: &Tag) -> bool {
    body(root_name, root).is_some_and(|c| c.contains("Palette") || c.contains("Blocks") || c.i32("Version").is_some_and(|v| v >= 1))
        && !root.get("Blocks").is_some_and(|b| matches!(b, Tag::ByteArray(_)))
}

pub fn read(root_name: &str, root: &Tag, name: &str) -> Result<Schematic> {
    let Some(c) = body(root_name, root) else {
        return err("not a Sponge schematic");
    };
    let version = c.i32("Version").unwrap_or(1);
    let size = [dimension(c, "Width"), dimension(c, "Height"), dimension(c, "Length")];
    let volume: u64 = size.iter().map(|&v| v.max(0) as u64).product();
    if volume == 0 {
        return err("an empty schematic");
    }
    if volume > crate::MAX_VOLUME {
        return err(format!("the schematic is too large ({volume} blocks)"));
    }
    let offset = c.get("Offset").and_then(Tag::as_ints).filter(|o| o.len() == 3).unwrap_or(vec![0, 0, 0]);
    // Version 3 keeps the palette, data and block entities in `Blocks`.
    let blocks = if version >= 3 { c.compound("Blocks") } else { Some(c) };
    let Some(blocks) = blocks else {
        return err("a Sponge v3 schematic without Blocks");
    };
    let (palette_key, data_key) = if version >= 3 { ("Palette", "Data") } else { ("Palette", "BlockData") };
    let mut region = Region::new("Main", [0; 3], size);
    let mut by_index: BTreeMap<u32, u32> = BTreeMap::new();
    match blocks.compound(palette_key) {
        Some(palette) => {
            for (text, idx) in palette.iter() {
                let Some(i) = idx.as_i32() else { continue };
                let id = region.intern(BlockState::parse(text));
                by_index.insert(i as u32, id);
            }
        }
        None => return err("a Sponge schematic without a palette"),
    }
    let data = match blocks.get(data_key) {
        Some(Tag::ByteArray(b)) => b.as_slice(),
        _ => return err("a Sponge schematic without block data"),
    };
    let raw = varints(data, volume as usize)?;
    for (dst, v) in region.blocks.iter_mut().zip(raw) {
        *dst = by_index.get(&v).copied().unwrap_or(0);
    }
    let entity_list = if version == 1 { "TileEntities" } else { "BlockEntities" };
    for t in blocks.list(entity_list).or_else(|| c.list("TileEntities")).unwrap_or(&[]) {
        let Some(be) = t.as_compound() else { continue };
        let Some(p) = be.get("Pos").and_then(Tag::as_ints).filter(|p| p.len() == 3) else { continue };
        let mut tag = match be.compound("Data") {
            // Version 3 nests the block entity's own tag in `Data`.
            Some(d) => d.clone(),
            None => {
                let mut tag = be.clone();
                tag.remove("Pos");
                tag
            }
        };
        if let Some(id) = be.str("Id") {
            tag.remove("Id");
            tag.remove("id");
            tag.0.insert(0, ("id".into(), Tag::String(id.to_owned())));
        }
        tag.remove("x");
        tag.remove("y");
        tag.remove("z");
        region.block_entities.insert([p[0], p[1], p[2]], tag);
    }
    for t in c.list("Entities").unwrap_or(&[]) {
        let Some(e) = t.as_compound() else { continue };
        let Some(p) = e.get("Pos").and_then(Tag::as_doubles).filter(|p| p.len() == 3) else { continue };
        let mut nbt = match e.compound("Data") {
            Some(d) => d.clone(),
            None => e.clone(),
        };
        if let Some(id) = e.str("Id") {
            nbt.remove("Id");
            nbt.remove("id");
            nbt.0.insert(0, ("id".into(), Tag::String(id.to_owned())));
        }
        // Positions are from the schematic's corner, or (some writers) from
        // its origin, `Offset` away from the corner.
        let mut pos = [p[0], p[1], p[2]];
        let inside = |q: &[f64; 3]| (0..3).all(|a| q[a] >= -1.0 && q[a] <= f64::from(size[a]) + 1.0);
        let shifted = [p[0] - f64::from(offset[0]), p[1] - f64::from(offset[1]), p[2] - f64::from(offset[2])];
        if !inside(&pos) && inside(&shifted) {
            pos = shifted;
        }
        region.entities.push(Entity { pos, nbt });
    }
    region.compact();
    let mut s = Schematic::new(name, Format::Sponge(version as u8), vec![region]);
    s.data_version = c.i32("DataVersion").unwrap_or(0);
    if let Some(meta) = c.compound("Metadata") {
        if let Some(n) = meta.str("Name").filter(|n| !n.is_empty()) {
            s.name = n.to_owned();
        }
        s.author = meta.str("Author").unwrap_or_default().to_owned();
    }
    Ok(s)
}

/// A Sponge v3 schematic of all regions merged, as the root compound to write
/// under an empty name.
pub fn write(s: &Schematic, data_version: i32) -> Tag {
    let merged = crate::formats::merged(s);
    let mut palette = Compound::new();
    for (i, state) in merged.palette.iter().enumerate() {
        palette.insert(state.to_string(), Tag::Int(i as i32));
    }
    let block_entities = merged
        .block_entities
        .iter()
        .map(|(p, tag)| {
            let mut data = tag.clone();
            let id = data.remove("id").and_then(|t| t.as_str().map(str::to_owned)).unwrap_or_default();
            compound([
                ("Pos", Tag::IntArray(p.to_vec())),
                ("Id", Tag::String(id)),
                ("Data", Tag::Compound(data)),
            ])
        })
        .collect();
    let entities = merged
        .entities
        .iter()
        .map(|e| {
            let mut data = e.nbt.clone();
            let id = data.remove("id").and_then(|t| t.as_str().map(str::to_owned)).unwrap_or_default();
            data.remove("Pos");
            compound([
                ("Pos", Tag::List(e.pos.iter().map(|&v| Tag::Double(v)).collect())),
                ("Id", Tag::String(id)),
                ("Data", Tag::Compound(data)),
            ])
        })
        .collect();
    compound([(
        "Schematic",
        compound([
            ("Version", Tag::Int(3)),
            ("DataVersion", Tag::Int(data_version)),
            ("Metadata", compound([("Name", Tag::String(s.name.clone())), ("Author", Tag::String(s.author.clone()))])),
            ("Width", Tag::Short(merged.size[0] as u16 as i16)),
            ("Height", Tag::Short(merged.size[1] as u16 as i16)),
            ("Length", Tag::Short(merged.size[2] as u16 as i16)),
            ("Offset", Tag::IntArray(vec![0, 0, 0])),
            (
                "Blocks",
                compound([
                    ("Palette", Tag::Compound(palette)),
                    ("Data", Tag::ByteArray(write_varints(&merged.blocks))),
                    ("BlockEntities", Tag::List(block_entities)),
                ]),
            ),
            ("Entities", Tag::List(entities)),
        ]),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip() {
        let v = vec![0, 1, 127, 128, 300, 70000];
        assert_eq!(varints(&write_varints(&v), v.len()).unwrap(), v);
        assert!(varints(&write_varints(&v), v.len() + 1).is_err());
    }

    fn v2() -> Tag {
        compound([
            ("Version", Tag::Int(2)),
            ("DataVersion", Tag::Int(3465)),
            ("Width", Tag::Short(2)),
            ("Height", Tag::Short(1)),
            ("Length", Tag::Short(2)),
            ("PaletteMax", Tag::Int(3)),
            (
                "Palette",
                compound([
                    ("minecraft:air", Tag::Int(0)),
                    ("minecraft:oak_stairs[facing=west,half=top,shape=straight,waterlogged=false]", Tag::Int(1)),
                    ("minecraft:chest[facing=north,type=single,waterlogged=false]", Tag::Int(2)),
                ]),
            ),
            ("BlockData", Tag::ByteArray(vec![1, 0, 0, 2])),
            (
                "BlockEntities",
                Tag::List(vec![compound([
                    ("Pos", Tag::IntArray(vec![1, 0, 1])),
                    ("Id", Tag::String("minecraft:chest".into())),
                    ("Items", Tag::List(vec![])),
                ])]),
            ),
        ])
    }

    #[test]
    fn reads_version_2() {
        let s = read("Schematic", &v2(), "f").unwrap();
        assert_eq!(s.format, Format::Sponge(2));
        let r = &s.regions[0];
        assert_eq!(r.state(0, 0, 0).get("facing"), Some("west"));
        assert_eq!(r.state(1, 0, 1).short(), "chest");
        assert_eq!(r.block_entities[&[1, 0, 1]].str("id"), Some("minecraft:chest"));
    }

    #[test]
    fn v3_round_trips() {
        let s = read("Schematic", &v2(), "f").unwrap();
        let root = write(&s, 4671);
        assert!(is_sponge("", &root));
        let back = read("", &root, "f").unwrap();
        assert_eq!(back.format, Format::Sponge(3));
        assert_eq!(back.regions[0].blocks, s.regions[0].blocks);
        assert_eq!(back.regions[0].block_entities, s.regions[0].block_entities);
    }
}
