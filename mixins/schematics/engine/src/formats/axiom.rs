//! Axiom's blueprints (`.bp`): a magic number, a header tag (name, author),
//! a PNG thumbnail and gzip NBT of 16x16x16 sections, each a paletted
//! container as chunks keep them (values do not span longs).

use crate::nbt::{Endian, Result, Tag, err, read_file, read_root};
use crate::schematic::{BlockState, Format, Region, Schematic};
use std::collections::BTreeMap;

pub const MAGIC: u32 = 0x0AE5_BB36;

pub fn is_blueprint(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == MAGIC
}

fn chunk(bytes: &[u8], at: &mut usize) -> Result<std::ops::Range<usize>> {
    if bytes.len() < *at + 4 {
        return err("the blueprint ends early");
    }
    let n = u32::from_be_bytes(bytes[*at..*at + 4].try_into().unwrap()) as usize;
    *at += 4;
    if bytes.len() - *at < n {
        return err("the blueprint ends early");
    }
    let range = *at..*at + n;
    *at += n;
    Ok(range)
}

/// Values of a paletted container of 4096 entries; the bits per value are
/// those the data's length allows.
pub fn section_values(data: &[i64]) -> Vec<u32> {
    if data.is_empty() {
        return vec![0; 4096];
    }
    let per_long = 4096usize.div_ceil(data.len());
    let bits = (64 / per_long).max(1) as u32;
    let mask = (1u64 << bits) - 1;
    let per = (64 / bits) as usize;
    (0..4096)
        .map(|i| ((data.get(i / per).copied().unwrap_or(0) as u64 >> ((i % per) as u32 * bits)) & mask) as u32)
        .collect()
}

pub fn read(bytes: &[u8], name: &str) -> Result<Schematic> {
    if !is_blueprint(bytes) {
        return err("not an Axiom blueprint");
    }
    let mut at = 4;
    let header = chunk(bytes, &mut at)?;
    let header = read_root(&bytes[header], Endian::Big).map(|(_, t)| t).ok();
    let _thumbnail = chunk(bytes, &mut at)?;
    let data = chunk(bytes, &mut at)?;
    let (_, body) = read_file(&bytes[data], Endian::Big)?;
    let mut sections: Vec<([i32; 3], Vec<BlockState>, Vec<u32>)> = Vec::new();
    for t in body.get("BlockRegion").and_then(Tag::as_list).unwrap_or(&[]) {
        let Some(c) = t.as_compound() else { continue };
        let (Some(x), Some(y), Some(z)) = (c.i32("X"), c.i32("Y"), c.i32("Z")) else { continue };
        let Some(states) = c.compound("BlockStates") else { continue };
        let palette: Vec<BlockState> = states
            .list("palette")
            .unwrap_or(&[])
            .iter()
            .map(|p| BlockState::from_nbt(p).unwrap_or_else(BlockState::air))
            .collect();
        let values = match states.get("data") {
            Some(Tag::LongArray(d)) => section_values(d),
            _ => vec![0; 4096],
        };
        sections.push(([x, y, z], palette, values));
    }
    if sections.is_empty() {
        return err("an empty blueprint");
    }
    // The box of the blocks that are not air.
    let mut lo = [i32::MAX; 3];
    let mut hi = [i32::MIN; 3];
    for (s, palette, values) in &sections {
        for (i, &v) in values.iter().enumerate() {
            if palette.get(v as usize).is_none_or(BlockState::is_air) {
                continue;
            }
            let p = [s[0] * 16 + (i & 15) as i32, s[1] * 16 + (i >> 8) as i32, s[2] * 16 + (i >> 4 & 15) as i32];
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
    }
    if lo[0] > hi[0] {
        return err("a blueprint of air only");
    }
    let size = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];
    if size.iter().map(|&v| v as u64).product::<u64>() > crate::MAX_VOLUME {
        return err("the blueprint is too large");
    }
    let mut region = Region::new("Main", [0; 3], size);
    for (s, palette, values) in &sections {
        let map: Vec<u32> = palette.iter().map(|p| region.intern(p.clone())).collect();
        for (i, &v) in values.iter().enumerate() {
            let p = [s[0] * 16 + (i & 15) as i32, s[1] * 16 + (i >> 8) as i32, s[2] * 16 + (i >> 4 & 15) as i32];
            if let Some(at) = region.index(p[0] - lo[0], p[1] - lo[1], p[2] - lo[2]) {
                region.blocks[at] = map.get(v as usize).copied().unwrap_or(0);
            }
        }
    }
    let mut entities = BTreeMap::new();
    for t in body.get("BlockEntities").and_then(Tag::as_list).unwrap_or(&[]) {
        let Some(c) = t.as_compound() else { continue };
        let (Some(x), Some(y), Some(z)) = (c.i32("x"), c.i32("y"), c.i32("z")) else { continue };
        let mut tag = c.clone();
        for k in ["x", "y", "z"] {
            tag.remove(k);
        }
        entities.insert([x - lo[0], y - lo[1], z - lo[2]], tag);
    }
    region.block_entities = entities;
    region.compact();
    let meta = header.as_ref().and_then(Tag::as_compound);
    let mut s = Schematic::new(
        meta.and_then(|m| m.str("Name")).filter(|n| !n.is_empty()).unwrap_or(name),
        Format::Axiom,
        vec![region],
    );
    s.author = meta.and_then(|m| m.str("Author")).unwrap_or_default().to_owned();
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::{compound, gzip, write_root};

    #[test]
    fn reads_sections() {
        // One section: palette [air, stone], 4 bits per value, stone at 1 2 3.
        let mut data = vec![0i64; 256];
        let i = (2 << 8) | (3 << 4) | 1;
        data[i / 16] |= 1 << ((i % 16) * 4);
        let body = compound([(
            "BlockRegion",
            Tag::List(vec![compound([
                ("X", Tag::Int(-1)),
                ("Y", Tag::Int(0)),
                ("Z", Tag::Int(0)),
                (
                    "BlockStates",
                    compound([
                        ("palette", Tag::List(vec![BlockState::air().to_nbt(), BlockState::new("stone").to_nbt()])),
                        ("data", Tag::LongArray(data)),
                    ]),
                ),
            ])]),
        )]);
        let header = write_root("", &compound([("Name", Tag::String("Hut".into()))]));
        let blocks = gzip(&write_root("", &body));
        let mut file = MAGIC.to_be_bytes().to_vec();
        for part in [header, vec![0x89, b'P'], blocks] {
            file.extend_from_slice(&(part.len() as u32).to_be_bytes());
            file.extend_from_slice(&part);
        }
        let s = read(&file, "f").unwrap();
        assert_eq!(s.name, "Hut");
        assert_eq!(s.regions[0].size, [1, 1, 1]);
        assert_eq!(s.regions[0].state(0, 0, 0).short(), "stone");
    }
}
