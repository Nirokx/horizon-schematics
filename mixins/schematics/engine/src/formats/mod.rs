//! Every schematic format: told apart by their content (the extension only
//! breaks ties), read into a [`Schematic`], and the ones the mod saves.

pub mod axiom;
pub mod bedrock;
pub mod legacy;
pub mod litematic;
pub mod mcedit;
pub mod sponge;
pub mod structure;

use crate::nbt::{self, Endian, Result, err};
use crate::schematic::{BlockState, Entity, Region, Schematic};
use std::collections::BTreeMap;
use std::path::Path;

/// The extensions the file list shows.
pub const EXTENSIONS: [&str; 6] = ["litematic", "schem", "schematic", "nbt", "mcstructure", "bp"];

pub fn is_schematic(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub(crate) fn namespaced_id(id: &str) -> String {
    crate::schematic::namespaced(id)
}

/// The file's name without its extension.
pub fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "schematic".into())
}

pub fn load(path: &Path) -> Result<Schematic> {
    let bytes = std::fs::read(path).map_err(|e| nbt::Error(format!("can't read {}: {e}", path.display())))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    read(&bytes, &ext, &stem(path))
}

/// A schematic from a file's bytes; `ext` (lower case) only breaks ties.
pub fn read(bytes: &[u8], ext: &str, name: &str) -> Result<Schematic> {
    if axiom::is_blueprint(bytes) {
        return axiom::read(bytes, name);
    }
    if ext == "mcstructure" {
        let raw = nbt::decompress(bytes)?;
        let (_, root) = nbt::read_root(&raw, Endian::Little)?;
        return bedrock::read(&root, name);
    }
    let raw = nbt::decompress(bytes)?;
    let (root_name, root) = match nbt::read_root(&raw, Endian::Big) {
        Ok(r) => r,
        Err(e) => {
            // An uncompressed Bedrock file under another extension.
            if let Ok((_, root)) = nbt::read_root(&raw, Endian::Little)
                && bedrock::is_mcstructure(&root)
            {
                return bedrock::read(&root, name);
            }
            return Err(e);
        }
    };
    let mut s = if root.get("Regions").is_some() {
        litematic::read(&root, name)?
    } else if mcedit::is_mcedit(&root) {
        mcedit::read(&root, name)?
    } else if sponge::is_sponge(&root_name, &root) {
        sponge::read(&root_name, &root, name)?
    } else if structure::is_structure(&root) {
        structure::read(&root, name)?
    } else if bedrock::is_mcstructure(&root) {
        bedrock::read(&root, name)?
    } else {
        return err(match ext {
            "litematic" => "this .litematic has no regions",
            "schem" | "schematic" => "not a schematic this mod knows (Sponge, MCEdit or WorldEdit)",
            "nbt" => "not a structure file",
            _ => "not a schematic format this mod knows",
        });
    };
    if s.regions.iter().all(|r| r.block_count() == 0) && s.regions.iter().all(|r| r.entities.is_empty()) {
        return err("the schematic has no blocks");
    }
    s.normalize();
    Ok(s)
}

/// The formats the mod saves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Litematic,
    Sponge,
    Structure,
}

impl SaveFormat {
    pub const ALL: [SaveFormat; 3] = [SaveFormat::Litematic, SaveFormat::Sponge, SaveFormat::Structure];
    pub fn extension(self) -> &'static str {
        match self {
            SaveFormat::Litematic => "litematic",
            SaveFormat::Sponge => "schem",
            SaveFormat::Structure => "nbt",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "litematic" => Some(SaveFormat::Litematic),
            "schem" | "sponge" | "worldedit" => Some(SaveFormat::Sponge),
            "nbt" | "structure" => Some(SaveFormat::Structure),
            _ => None,
        }
    }
}

/// The gzipped bytes of `s` in `format`.
pub fn encode(s: &Schematic, format: SaveFormat, data_version: i32, now_millis: i64) -> Vec<u8> {
    let raw = match format {
        SaveFormat::Litematic => nbt::write_root("", &litematic::write(s, data_version, now_millis)),
        SaveFormat::Sponge => nbt::write_root("", &sponge::write(s, data_version)),
        SaveFormat::Structure => nbt::write_root("", &structure::write(s, data_version)),
    };
    nbt::gzip(&raw)
}

/// Writes `s` into `dir` as `<name>.<ext>`, never over an existing file
/// (a number is added). The path written.
pub fn save(dir: &Path, name: &str, s: &Schematic, format: SaveFormat, data_version: i32, now_millis: i64) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let clean: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.') { c } else { '_' })
        .collect();
    let clean = clean.trim().trim_matches('.');
    let clean = if clean.is_empty() { "schematic" } else { clean };
    let mut path = dir.join(format!("{clean}.{}", format.extension()));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{clean} ({n}).{}", format.extension()));
        n += 1;
    }
    std::fs::write(&path, encode(s, format, data_version, now_millis))?;
    Ok(path)
}

/// All regions in one box, for the formats that hold one.
pub struct Merged {
    pub size: [i32; 3],
    pub palette: Vec<BlockState>,
    pub blocks: Vec<u32>,
    pub block_entities: BTreeMap<[i32; 3], nbt::Compound>,
    pub entities: Vec<Entity>,
}

pub fn merged(s: &Schematic) -> Merged {
    let (lo, _) = s.bounds();
    let size = s.size();
    let mut out = Region::new("Main", [0; 3], size);
    for r in &s.regions {
        let map: Vec<u32> = r.palette.iter().map(|p| out.intern(p.clone())).collect();
        for y in 0..r.size[1] {
            for z in 0..r.size[2] {
                for x in 0..r.size[0] {
                    let v = r.at(x, y, z);
                    if v == 0 && r.palette[0].is_air() {
                        continue;
                    }
                    let p = [r.pos[0] - lo[0] + x, r.pos[1] - lo[1] + y, r.pos[2] - lo[2] + z];
                    if let Some(i) = out.index(p[0], p[1], p[2]) {
                        out.blocks[i] = map[v as usize];
                    }
                }
            }
        }
        for (p, tag) in &r.block_entities {
            out.block_entities.insert([r.pos[0] - lo[0] + p[0], r.pos[1] - lo[1] + p[1], r.pos[2] - lo[2] + p[2]], tag.clone());
        }
        for e in &r.entities {
            let pos = [
                e.pos[0] + f64::from(r.pos[0] - lo[0]),
                e.pos[1] + f64::from(r.pos[1] - lo[1]),
                e.pos[2] + f64::from(r.pos[2] - lo[2]),
            ];
            out.entities.push(Entity { pos, nbt: e.nbt.clone() });
        }
    }
    Merged { size, palette: out.palette, blocks: out.blocks, block_entities: out.block_entities, entities: out.entities }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schematic::Format;

    fn sample() -> Schematic {
        let mut a = Region::new("A", [0, 0, 0], [2, 2, 2]);
        a.set(0, 0, 0, BlockState::new("stone"));
        a.set(1, 1, 1, BlockState::new("oak_planks"));
        let mut b = Region::new("B", [3, 0, 0], [1, 1, 1]);
        b.set(0, 0, 0, BlockState::new("glass"));
        let mut s = Schematic::new("two", Format::Litematic, vec![a, b]);
        s.author = "me".into();
        s
    }

    #[test]
    fn every_save_format_reads_back() {
        for format in SaveFormat::ALL {
            let bytes = encode(&sample(), format, 4671, 0);
            let back = read(&bytes, format.extension(), "x").unwrap();
            let m = merged(&back);
            assert_eq!(m.size, [4, 2, 2], "{format:?}");
            let at = |x: i32, y: i32, z: i32| m.palette[m.blocks[((y * 2 + z) * 4 + x) as usize] as usize].short().to_owned();
            assert_eq!(at(0, 0, 0), "stone", "{format:?}");
            assert_eq!(at(1, 1, 1), "oak_planks", "{format:?}");
            assert_eq!(at(3, 0, 0), "glass", "{format:?}");
            assert_eq!(at(2, 0, 0), "air", "{format:?}");
        }
    }

    #[test]
    fn refuses_what_is_not_a_schematic() {
        assert!(read(b"hello", "schem", "x").is_err());
        assert!(read(&nbt::write_root("", &nbt::compound([("a", nbt::Tag::Int(1))])), "nbt", "x").is_err());
    }

    #[test]
    fn saving_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("schematics-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = save(&dir, "my/house", &sample(), SaveFormat::Litematic, 1, 0).unwrap();
        let b = save(&dir, "my/house", &sample(), SaveFormat::Litematic, 1, 0).unwrap();
        assert_eq!(a.file_name().unwrap(), "my_house.litematic");
        assert_eq!(b.file_name().unwrap(), "my_house (2).litematic");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
