//! Vanilla structure files (`.nbt`, the structure block's): a size, a
//! palette (or several, for random variants: the first is used) and a list of
//! blocks with their positions. Positions not listed are structure void.

use crate::nbt::{Compound, Result, Tag, compound, err};
use crate::schematic::{BlockState, Entity, Format, Region, Schematic};

pub fn is_structure(root: &Tag) -> bool {
    root.get("size").is_some() && root.get("blocks").is_some() && (root.get("palette").is_some() || root.get("palettes").is_some())
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
    let palette = match c.list("palette") {
        Some(p) => p,
        None => c.list("palettes").and_then(|ps| ps.first()).and_then(Tag::as_list).unwrap_or(&[]),
    };
    let mut region = Region::new("Main", [0; 3], size);
    let map: Vec<u32> = palette
        .iter()
        .map(|t| region.intern(BlockState::from_nbt(t).unwrap_or_else(BlockState::air)))
        .collect();
    for b in c.list("blocks").unwrap_or(&[]) {
        let Some(b) = b.as_compound() else { continue };
        let Some(p) = b.get("pos").and_then(Tag::as_ints).filter(|p| p.len() == 3) else { continue };
        let Some(&id) = b.i32("state").and_then(|s| map.get(s as usize)) else { continue };
        if let Some(i) = region.index(p[0], p[1], p[2]) {
            region.blocks[i] = id;
        }
        if let Some(nbt) = b.compound("nbt") {
            region.block_entities.insert([p[0], p[1], p[2]], nbt.clone());
        }
    }
    for e in c.list("entities").unwrap_or(&[]) {
        let Some(e) = e.as_compound() else { continue };
        let Some(p) = e.get("pos").and_then(Tag::as_doubles).filter(|p| p.len() == 3) else { continue };
        let nbt = e.compound("nbt").cloned().unwrap_or_default();
        region.entities.push(Entity { pos: [p[0], p[1], p[2]], nbt });
    }
    region.compact();
    let mut s = Schematic::new(name, Format::Structure, vec![region]);
    s.data_version = c.i32("DataVersion").unwrap_or(0);
    if let Some(author) = c.str("author") {
        s.author = author.to_owned();
    }
    Ok(s)
}

/// A structure file of all regions merged (air included, so it pastes over).
pub fn write(s: &Schematic, data_version: i32) -> Tag {
    let merged = crate::formats::merged(s);
    let [sx, sy, sz] = merged.size;
    let mut blocks = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let state = merged.blocks[((y * sz + z) * sx + x) as usize];
                let mut b = Compound::new();
                b.insert("pos", Tag::List(vec![Tag::Int(x), Tag::Int(y), Tag::Int(z)]));
                b.insert("state", Tag::Int(state as i32));
                if let Some(nbt) = merged.block_entities.get(&[x, y, z]) {
                    b.insert("nbt", Tag::Compound(nbt.clone()));
                }
                blocks.push(Tag::Compound(b));
            }
        }
    }
    let entities = merged
        .entities
        .iter()
        .map(|e| {
            let block = e.pos.map(|v| Tag::Int(v.floor() as i32));
            compound([
                ("pos", Tag::List(e.pos.iter().map(|&v| Tag::Double(v)).collect())),
                ("blockPos", Tag::List(block.to_vec())),
                ("nbt", Tag::Compound(e.nbt.clone())),
            ])
        })
        .collect();
    compound([
        ("DataVersion", Tag::Int(data_version)),
        ("size", Tag::List(vec![Tag::Int(sx), Tag::Int(sy), Tag::Int(sz)])),
        ("palette", Tag::List(merged.palette.iter().map(BlockState::to_nbt).collect())),
        ("blocks", Tag::List(blocks)),
        ("entities", Tag::List(entities)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_write() {
        let mut r = Region::new("Main", [0; 3], [2, 2, 1]);
        r.set(1, 1, 0, BlockState::new("glass"));
        let s = Schematic::new("s", Format::Structure, vec![r]);
        let tag = write(&s, 1);
        assert!(is_structure(&tag));
        let back = read(&tag, "s").unwrap();
        assert_eq!(back.regions[0].state(1, 1, 0).short(), "glass");
        assert_eq!(back.regions[0].block_count(), 1);
    }
}
