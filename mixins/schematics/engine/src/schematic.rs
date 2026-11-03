//! A schematic as every format is read into: one or more regions of blocks,
//! each with its palette, block entities and entities.

use crate::nbt::{Compound, Tag};
use std::collections::BTreeMap;
use std::fmt;

/// A block state by name, with its properties sorted by key
/// (`minecraft:oak_stairs[facing=east,half=bottom,...]`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockState {
    pub name: String,
    pub props: Vec<(String, String)>,
}

pub const AIR: &str = "minecraft:air";

impl BlockState {
    pub fn new(name: &str) -> Self {
        Self { name: namespaced(name), props: Vec::new() }
    }
    pub fn air() -> Self {
        Self::new(AIR)
    }
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.set(key, value);
        self
    }
    /// `name[key=value,...]`, as commands and Sponge palettes write a state.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        let (name, rest) = match text.find('[') {
            Some(i) => (&text[..i], Some(&text[i + 1..])),
            None => (text, None),
        };
        let mut state = Self::new(name);
        if let Some(rest) = rest {
            let rest = rest.strip_suffix(']').unwrap_or(rest);
            for pair in rest.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    state.set(k.trim(), v.trim());
                }
            }
        }
        state
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, key: &str, value: &str) {
        match self.props.binary_search_by(|(k, _)| k.as_str().cmp(key)) {
            Ok(i) => self.props[i].1 = value.to_owned(),
            Err(i) => self.props.insert(i, (key.to_owned(), value.to_owned())),
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let i = self.props.iter().position(|(k, _)| k == key)?;
        Some(self.props.remove(i).1)
    }
    pub fn is_air(&self) -> bool {
        matches!(self.name.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
    }
    /// The name without `minecraft:`.
    pub fn short(&self) -> &str {
        self.name.strip_prefix("minecraft:").unwrap_or(&self.name)
    }
    /// `{Name, Properties}`, as `.litematic` and structure files keep a state.
    pub fn to_nbt(&self) -> Tag {
        let mut c = Compound::new();
        c.insert("Name", Tag::String(self.name.clone()));
        if !self.props.is_empty() {
            let props = self.props.iter().map(|(k, v)| (k.clone(), Tag::String(v.clone()))).collect();
            c.insert("Properties", Tag::Compound(Compound(props)));
        }
        Tag::Compound(c)
    }
    pub fn from_nbt(tag: &Tag) -> Option<Self> {
        let c = tag.as_compound()?;
        let mut state = Self::new(c.str("Name")?);
        if let Some(props) = c.compound("Properties") {
            for (k, v) in props.iter() {
                let value = match v {
                    Tag::String(s) => s.clone(),
                    other => other.as_i64().map(|n| n.to_string()).unwrap_or_default(),
                };
                state.set(k, &value);
            }
        }
        Some(state)
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.props.is_empty() {
            f.write_str("[")?;
            for (i, (k, v)) in self.props.iter().enumerate() {
                if i > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{k}={v}")?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

pub fn namespaced(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase();
    if name.contains(':') { name } else { format!("minecraft:{name}") }
}

/// An entity: its position in the region (blocks from the region's corner)
/// and its tag (with `id`).
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub pos: [f64; 3],
    pub nbt: Compound,
}

/// A box of blocks. `pos` is its lowest corner in the schematic, `size` its
/// extent (all positive); blocks are palette indices, x fastest, then z,
/// then y. Palette entry 0 is always air.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub name: String,
    pub pos: [i32; 3],
    pub size: [i32; 3],
    pub palette: Vec<BlockState>,
    pub blocks: Vec<u32>,
    /// Block entity tags by their position in the region, without `x`, `y`, `z`.
    pub block_entities: BTreeMap<[i32; 3], Compound>,
    pub entities: Vec<Entity>,
}

impl Region {
    /// An all-air region.
    pub fn new(name: &str, pos: [i32; 3], size: [i32; 3]) -> Self {
        let volume = size.iter().map(|&s| s.max(0) as usize).product();
        Self {
            name: name.to_owned(),
            pos,
            size,
            palette: vec![BlockState::air()],
            blocks: vec![0; volume],
            block_entities: BTreeMap::new(),
            entities: Vec::new(),
        }
    }
    pub fn volume(&self) -> usize {
        self.blocks.len()
    }
    pub fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let [sx, sy, sz] = self.size;
        if x < 0 || y < 0 || z < 0 || x >= sx || y >= sy || z >= sz {
            return None;
        }
        Some((y as usize * sz as usize + z as usize) * sx as usize + x as usize)
    }
    /// The palette index at a position in the region (air outside it).
    pub fn at(&self, x: i32, y: i32, z: i32) -> u32 {
        self.index(x, y, z).map_or(0, |i| self.blocks[i])
    }
    pub fn state(&self, x: i32, y: i32, z: i32) -> &BlockState {
        &self.palette[self.at(x, y, z) as usize]
    }
    /// The palette index of `state`, added when new.
    pub fn intern(&mut self, state: BlockState) -> u32 {
        if state.is_air() && state.name == AIR {
            return 0;
        }
        match self.palette.iter().position(|s| *s == state) {
            Some(i) => i as u32,
            None => {
                self.palette.push(state);
                (self.palette.len() - 1) as u32
            }
        }
    }
    pub fn set(&mut self, x: i32, y: i32, z: i32, state: BlockState) {
        let id = self.intern(state);
        if let Some(i) = self.index(x, y, z) {
            self.blocks[i] = id;
        }
    }
    /// Blocks other than air.
    pub fn block_count(&self) -> usize {
        let air: Vec<bool> = self.palette.iter().map(BlockState::is_air).collect();
        self.blocks.iter().filter(|&&b| !air[b as usize]).count()
    }
    /// Takes palette entries no block uses out, keeping air first.
    pub fn compact(&mut self) {
        let mut used = vec![false; self.palette.len()];
        used[0] = true;
        for &b in &self.blocks {
            used[b as usize] = true;
        }
        let mut map = vec![0u32; self.palette.len()];
        let mut palette = Vec::new();
        for (i, state) in std::mem::take(&mut self.palette).into_iter().enumerate() {
            if used[i] {
                map[i] = palette.len() as u32;
                palette.push(state);
            }
        }
        self.palette = palette;
        for b in &mut self.blocks {
            *b = map[*b as usize];
        }
    }
}

/// The file format a schematic came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Litematic,
    Sponge(u8),
    McEdit,
    Structure,
    Bedrock,
    Axiom,
}

impl Format {
    pub fn label(self) -> String {
        match self {
            Format::Litematic => ".litematic".into(),
            Format::Sponge(v) => format!("Sponge v{v}"),
            Format::McEdit => "MCEdit".into(),
            Format::Structure => "Structure".into(),
            Format::Bedrock => "Bedrock".into(),
            Format::Axiom => "Axiom".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schematic {
    pub name: String,
    pub author: String,
    pub description: String,
    pub format: Format,
    /// The game data version the file was saved with (0 when unknown).
    pub data_version: i32,
    pub regions: Vec<Region>,
}

impl Schematic {
    pub fn new(name: &str, format: Format, regions: Vec<Region>) -> Self {
        Self {
            name: name.to_owned(),
            author: String::new(),
            description: String::new(),
            format,
            data_version: 0,
            regions,
        }
    }
    /// The lowest and highest corners of all regions (inclusive).
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        for r in &self.regions {
            for a in 0..3 {
                lo[a] = lo[a].min(r.pos[a]);
                hi[a] = hi[a].max(r.pos[a] + r.size[a] - 1);
            }
        }
        if self.regions.is_empty() {
            return ([0; 3], [0; 3]);
        }
        (lo, hi)
    }
    pub fn size(&self) -> [i32; 3] {
        let (lo, hi) = self.bounds();
        [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1]
    }
    pub fn block_count(&self) -> usize {
        self.regions.iter().map(Region::block_count).sum()
    }
    pub fn volume(&self) -> usize {
        self.regions.iter().map(Region::volume).sum()
    }
    /// Moves every region so the lowest corner is at 0 0 0.
    pub fn normalize(&mut self) {
        let (lo, _) = self.bounds();
        for r in &mut self.regions {
            for a in 0..3 {
                r.pos[a] -= lo[a];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_states() {
        let s = BlockState::parse("oak_stairs[half=bottom,facing=east]");
        assert_eq!(s.name, "minecraft:oak_stairs");
        assert_eq!(s.get("facing"), Some("east"));
        assert_eq!(s.to_string(), "minecraft:oak_stairs[facing=east,half=bottom]");
        assert_eq!(BlockState::parse("mod:thing").to_string(), "mod:thing");
    }

    #[test]
    fn regions_index_x_then_z_then_y_and_compact() {
        let mut r = Region::new("r", [0; 3], [2, 3, 4]);
        r.set(1, 2, 3, BlockState::new("stone"));
        r.set(0, 0, 0, BlockState::new("dirt"));
        r.set(0, 0, 0, BlockState::air());
        assert_eq!(r.index(1, 2, 3), Some(23));
        assert_eq!(r.state(1, 2, 3).name, "minecraft:stone");
        assert_eq!(r.block_count(), 1);
        r.compact();
        assert_eq!(r.palette.len(), 2);
        assert_eq!(r.state(1, 2, 3).name, "minecraft:stone");
    }
}
