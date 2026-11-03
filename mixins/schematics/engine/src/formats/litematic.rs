//! The `.litematic` format: gzip NBT with named regions, each with a
//! palette and its block indices packed in longs, a value allowed to span
//! two longs.

use crate::nbt::{Compound, Result, Tag, compound, err};
use crate::schematic::{BlockState, Entity, Format, Region, Schematic};

/// The `.litematic` format version written (the one of Minecraft 1.21.5 on).
pub const VERSION: i32 = 7;
pub const SUB_VERSION: i32 = 1;

fn vec3(c: Option<&Compound>) -> Option<[i32; 3]> {
    let c = c?;
    Some([c.i32("x")?, c.i32("y")?, c.i32("z")?])
}

fn vec3_tag(v: [i32; 3]) -> Tag {
    compound([("x", Tag::Int(v[0])), ("y", Tag::Int(v[1])), ("z", Tag::Int(v[2]))])
}

/// Bits per value: at least 2, enough for the palette.
pub fn bits_for(palette: usize) -> u32 {
    (u32::BITS - (palette.max(1) as u32 - 1).leading_zeros()).max(2)
}

pub fn unpack(longs: &[i64], bits: u32, count: usize) -> Vec<u32> {
    let mask = (1u64 << bits) - 1;
    let words: Vec<u64> = longs.iter().map(|&l| l as u64).collect();
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let start = i as u64 * u64::from(bits);
        let (word, offset) = ((start >> 6) as usize, (start & 63) as u32);
        let end = ((start + u64::from(bits) - 1) >> 6) as usize;
        let low = words.get(word).copied().unwrap_or(0) >> offset;
        let value = if word == end {
            low & mask
        } else {
            (low | words.get(end).copied().unwrap_or(0) << (64 - offset)) & mask
        };
        out.push(value as u32);
    }
    out
}

pub fn pack(values: &[u32], bits: u32) -> Vec<i64> {
    let total = (values.len() as u64 * u64::from(bits)).div_ceil(64) as usize;
    let mut words = vec![0u64; total];
    for (i, &v) in values.iter().enumerate() {
        let v = u64::from(v) & ((1u64 << bits) - 1);
        let start = i as u64 * u64::from(bits);
        let (word, offset) = ((start >> 6) as usize, (start & 63) as u32);
        let end = ((start + u64::from(bits) - 1) >> 6) as usize;
        words[word] |= v << offset;
        if end != word {
            words[end] |= v >> (64 - offset);
        }
    }
    words.into_iter().map(|w| w as i64).collect()
}

pub fn read(root: &Tag, name: &str) -> Result<Schematic> {
    let Some(root) = root.as_compound() else {
        return err("not a compound");
    };
    let version = root.i32("Version").unwrap_or(0);
    if version > 7 {
        return err(format!(".litematic format version {version} is newer than this mod knows (7)"));
    }
    let Some(regions_tag) = root.compound("Regions") else {
        return err("a .litematic without Regions");
    };
    let mut regions = Vec::new();
    for (region_name, tag) in regions_tag.iter() {
        let Some(r) = tag.as_compound() else { continue };
        let (Some(pos), Some(size)) = (vec3(r.compound("Position")), vec3(r.compound("Size"))) else {
            return err(format!("region {region_name} has no position or size"));
        };
        // A negative size grows down from the position.
        let mut lo = pos;
        let mut extent = size;
        for a in 0..3 {
            if size[a] < 0 {
                lo[a] = pos[a] + size[a] + 1;
            }
            extent[a] = size[a].abs();
        }
        if extent.iter().any(|&e| e == 0) {
            continue;
        }
        let volume: u64 = extent.iter().map(|&e| e as u64).product();
        if volume > crate::MAX_VOLUME {
            return err(format!("region {region_name} is too large ({volume} blocks)"));
        }
        let mut region = Region::new(region_name, lo, extent);
        let palette: Vec<BlockState> = r
            .list("BlockStatePalette")
            .unwrap_or(&[])
            .iter()
            .map(|t| BlockState::from_nbt(t).unwrap_or_else(BlockState::air))
            .collect();
        let longs = match r.get("BlockStates") {
            Some(Tag::LongArray(l)) => l.as_slice(),
            _ => &[],
        };
        let raw = unpack(longs, bits_for(palette.len()), volume as usize);
        // Our palette keeps air first; map the file's onto it.
        let map: Vec<u32> = palette.iter().map(|s| region.intern(s.clone())).collect();
        for (dst, v) in region.blocks.iter_mut().zip(raw) {
            *dst = map.get(v as usize).copied().unwrap_or(0);
        }
        for t in r.list("TileEntities").unwrap_or(&[]) {
            let Some(c) = t.as_compound() else { continue };
            let (Some(x), Some(y), Some(z)) = (c.i32("x"), c.i32("y"), c.i32("z")) else { continue };
            let mut c = c.clone();
            for k in ["x", "y", "z"] {
                c.remove(k);
            }
            region.block_entities.insert([x, y, z], c);
        }
        for t in r.list("Entities").unwrap_or(&[]) {
            let Some(c) = t.as_compound() else { continue };
            let Some(p) = c.get("Pos").and_then(Tag::as_doubles).filter(|p| p.len() == 3) else { continue };
            region.entities.push(Entity { pos: [p[0], p[1], p[2]], nbt: c.clone() });
        }
        region.compact();
        regions.push(region);
    }
    let meta = root.compound("Metadata");
    let mut s = Schematic::new(
        meta.and_then(|m| m.str("Name")).filter(|n| !n.is_empty()).unwrap_or(name),
        Format::Litematic,
        regions,
    );
    s.author = meta.and_then(|m| m.str("Author")).unwrap_or_default().to_owned();
    s.description = meta.and_then(|m| m.str("Description")).unwrap_or_default().to_owned();
    s.data_version = root.i32("MinecraftDataVersion").unwrap_or(0);
    Ok(s)
}

/// The root tag of a `.litematic` (to gzip).
pub fn write(s: &Schematic, data_version: i32, now_millis: i64) -> Tag {
    let mut regions = Compound::new();
    let mut total_blocks = 0i64;
    let mut total_volume = 0i64;
    for r in &s.regions {
        let bits = bits_for(r.palette.len());
        total_blocks += r.block_count() as i64;
        total_volume += r.volume() as i64;
        let tiles = r
            .block_entities
            .iter()
            .map(|(p, c)| {
                let mut c = c.clone();
                c.insert("x", Tag::Int(p[0]));
                c.insert("y", Tag::Int(p[1]));
                c.insert("z", Tag::Int(p[2]));
                Tag::Compound(c)
            })
            .collect();
        let entities = r
            .entities
            .iter()
            .map(|e| {
                let mut c = e.nbt.clone();
                c.insert("Pos", Tag::List(e.pos.iter().map(|&v| Tag::Double(v)).collect()));
                Tag::Compound(c)
            })
            .collect();
        regions.insert(
            r.name.clone(),
            compound([
                ("Position", vec3_tag(r.pos)),
                ("Size", vec3_tag(r.size)),
                ("BlockStatePalette", Tag::List(r.palette.iter().map(BlockState::to_nbt).collect())),
                ("BlockStates", Tag::LongArray(pack(&r.blocks, bits))),
                ("TileEntities", Tag::List(tiles)),
                ("Entities", Tag::List(entities)),
                ("PendingBlockTicks", Tag::List(Vec::new())),
                ("PendingFluidTicks", Tag::List(Vec::new())),
            ]),
        );
    }
    compound([
        ("MinecraftDataVersion", Tag::Int(data_version)),
        ("Version", Tag::Int(VERSION)),
        ("SubVersion", Tag::Int(SUB_VERSION)),
        (
            "Metadata",
            compound([
                ("Name", Tag::String(s.name.clone())),
                ("Author", Tag::String(s.author.clone())),
                ("Description", Tag::String(s.description.clone())),
                ("RegionCount", Tag::Int(s.regions.len() as i32)),
                ("TotalBlocks", Tag::Int(total_blocks as i32)),
                ("TotalVolume", Tag::Int(total_volume as i32)),
                ("EnclosingSize", vec3_tag(s.size())),
                ("TimeCreated", Tag::Long(now_millis)),
                ("TimeModified", Tag::Long(now_millis)),
            ]),
        ),
        ("Regions", Tag::Compound(regions)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_arrays_span_longs() {
        assert_eq!(bits_for(1), 2);
        assert_eq!(bits_for(4), 2);
        assert_eq!(bits_for(5), 3);
        assert_eq!(bits_for(257), 9);
        let values: Vec<u32> = (0..100).map(|i| (i * 7) % 5).collect();
        let packed = pack(&values, 3);
        assert_eq!(packed.len(), 5);
        assert_eq!(unpack(&packed, 3, 100), values);
        let wide: Vec<u32> = (0..77).map(|i| (i * 131) % 300).collect();
        assert_eq!(unpack(&pack(&wide, 9), 9, 77), wide);
    }

    #[test]
    fn negative_sizes_grow_down() {
        let mut palette = vec![BlockState::air().to_nbt(), BlockState::new("stone").to_nbt()];
        palette.truncate(2);
        let values = vec![1u32, 0, 0, 0];
        let root = compound([
            ("Version", Tag::Int(6)),
            ("Metadata", compound([("Name", Tag::String("Test".into()))])),
            (
                "Regions",
                compound([(
                    "main",
                    compound([
                        ("Position", vec3_tag([5, 0, 0])),
                        ("Size", vec3_tag([-2, 1, 2])),
                        ("BlockStatePalette", Tag::List(palette)),
                        ("BlockStates", Tag::LongArray(pack(&values, 2))),
                    ]),
                )]),
            ),
        ]);
        let s = read(&root, "file").unwrap();
        assert_eq!(s.name, "Test");
        let r = &s.regions[0];
        assert_eq!(r.pos, [4, 0, 0]);
        assert_eq!(r.size, [2, 1, 2]);
        assert_eq!(r.state(0, 0, 0).name, "minecraft:stone");
        assert_eq!(r.block_count(), 1);
    }

    #[test]
    fn round_trips() {
        let mut r = Region::new("Unnamed", [0, 0, 0], [3, 2, 2]);
        r.set(2, 1, 1, BlockState::new("oak_log").with("axis", "x"));
        r.set(0, 0, 0, BlockState::new("chest").with("facing", "north"));
        let mut items = Compound::new();
        items.insert("id", Tag::String("minecraft:chest".into()));
        r.block_entities.insert([0, 0, 0], items);
        let mut s = Schematic::new("Round", Format::Litematic, vec![r]);
        s.author = "me".into();
        let back = read(&write(&s, 4671, 0), "x").unwrap();
        assert_eq!(back.regions[0].blocks, s.regions[0].blocks);
        assert_eq!(back.regions[0].palette, s.regions[0].palette);
        assert_eq!(back.regions[0].block_entities, s.regions[0].block_entities);
        assert_eq!(back.author, "me");
    }
}
