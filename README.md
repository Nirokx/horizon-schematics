# Schematics for Horizon

A native Horizon mod (Minecraft 26.3, Horizon Nightly `c9d474d`): schematics
shown in the world as see-through ghost blocks, to build them by hand or
paste them. Written from scratch for Horizon; it reads and writes `.litematic`
files among the other common schematic formats.

## Formats

| Read | Written by |
|---|---|
| `.litematic` | Format versions 1 to 7 (several regions, negative sizes) |
| `.schem` | WorldEdit 7, FAWE, Sponge schematic versions 1, 2 and 3 |
| `.schematic` | MCEdit, WorldEdit 6, Schematica: numeric ids of 1.12 and before, translated to today's blocks (`SchematicaMapping` and `BlockIDs` names too) |
| `.nbt` | Vanilla structure blocks and jigsaw pieces |
| `.mcstructure` | Bedrock Edition structure blocks (names and states translated to Java's where they differ) |
| `.bp` | Axiom blueprints |

Files are told apart by their content, compressed (gzip, zlib) or not.
Saving writes `.litematic`, `.schem` (Sponge v3) or `.nbt`.

## Use

Put schematic files in `schematics/` in the game folder
(`~/.local/share/horizon/schematics/`; subfolders work).

- **M**: the menu.
  - **Load**: the schematics folder; click a file to place it at your feet.
  - **Placements**: every placement of this world: select, show / hide, remove.
  - **Placement**: move it here, nudge it a block on each axis, turn it left or right, mirror it, paste it.
    Shows how much of it is already built: correct, missing, wrong and extra blocks.
  - **Materials**: what it takes, what is still missing, and what you carry.
  - **Save area**: save the area selection as `.litematic`, `.schem` or `.nbt`.
  - **Settings**: ghost opacity, render layers, overlays, range, what a paste includes.
- **In the world**: missing blocks show as ghosts, wrong blocks red, blocks in the wrong state
  orange, blocks where the schematic has air magenta. The selected placement's box is blue.
- **A stick**: left click a block for the selection's first corner, right click for the second;
  **Ctrl + wheel** moves the selected placement a block the way you look.
- **Page Up / Page Down**: render one layer at a time and move it.
- **Options > Mods > Schematics**: change the keys.

Pasting places blocks without block updates, block entity data (chest contents, sign text) and
entities. In single player it runs on the world's own server; on a server it sends `/setblock`
and `/summon` commands, which need permission there.

Placements are kept per world and per server, in
`horizon-mods/.horizon-mod-data/schematics/placements/`.

### Commands

`/schematics` (or `/sch`): `menu`, `list`, `load <file>`, `placements`, `move [x y z]`,
`rotate [cw|ccw]`, `mirror`, `show`, `hide`, `remove`, `paste`, `cancel`, `pos1`, `pos2`,
`save <name> [litematic|schem|nbt]`, `layer <all|single|below|above> [y]`.

## Build and install

Uses the Horizon launcher's own Rust and Horizon source:

```sh
L=~/.local/share/horizon-launcher
export PATH=$L/rust/1.99.1-x86_64-unknown-linux-gnu/bin:$PATH CARGO_HOME=$L/cargo
cargo test --offline                      # the engine: formats, transforms, menus
python3 tools/make_samples.py             # one test build in every format, in schematics/samples/
python3 tools/package.py --install        # target/schematics-1.0.0.hnmod, into the launcher's native mods
```

Then build the game with its mods in the launcher. The mod is pinned to Horizon `c9d474d`: it
reads the game's block models, atlas and world, and the single player server's levels, which the
stable hook points don't offer. A newer Nightly may need the pin in `compatibility.json` and
`mixins/schematics/horizon-mixin.toml` moved.

## Layout

- `mixins/schematics/engine/`: the mod's crate, free of Horizon's types, with its tests: NBT,
  every format, transforms, placements, the material list, the menus
- `mixins/schematics/bridge.rs`: the game's side (block states, ghost meshes, pasting, saving)
- `mixins/schematics/ghost.rs`: the renderer's side (the ghosts drawn over the world)
- `mixins/schematics/*.rs`: one small hook adapter per hook point
- `tools/make_samples.py`: test files in every format, from a writer of its own
- `tools/inspect/`: prints what the engine reads from files
