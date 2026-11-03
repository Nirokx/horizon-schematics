//! Schematics for Horizon's engine: schematic files in every common format,
//! placements (moved, rotated, mirrored), the material list and the mod's
//! menus, as plain data. The game's side (blocks, meshes, pasting) is in the
//! bundle's `bridge.rs`.

pub mod formats;
pub mod materials;
pub mod nbt;
pub mod schematic;
pub mod snbt;
pub mod state;
pub mod transform;
pub mod ui;

/// The most blocks one region may hold (a 1024 x 384 x 1024 box would be
/// 400 million): loading more would take the game's memory.
pub const MAX_VOLUME: u64 = 128 * 1024 * 1024;
