# Progress log

Newest first. One entry per session: what changed, what was learned, what is next.

## 2026-10-10 (later): first terrain in the viewer

- Added `apps/viewer` (eframe + wgpu): loads every map a region contains or sees, builds one terrain mesh from the Lev heightmaps, draws it with placeholder height/slope colours into an offscreen texture shown in an egui window. A region list switches regions. `--screenshot` renders headlessly to a PNG.
- `StartOakVale` (12 maps) and `OakBay` (16 maps) both render as one seamless coastline; the two are the same place remodelled, which is a good sanity check.
- Ross confirmed the window opens and the terrain matches the game's orientation (world x and y as stored, z up).
- Not yet: no water plane (the sea floor shows at height 0); in the VM, rendering uses llvmpipe (software OpenGL), fine for correctness only.
- Next: water plane at the sea height, texture themes per grid point (Lev bytes 10–20 probably), then meshes and textures from `graphics.big` and `textures.big`.

## 2026-10-10 (later): Lev heightmap matches the Stb

- Added `fable_data::lev`: map size, texture theme slots, sound themes and the heightmap grid. Tested on all 398 maps against the stb (size, per-map value, tile heights).
- Learned:
  - The Lev heightmap is `(w+1)*(h+1)` 21-byte points with an unaligned `f32` height at byte 5; world height = value * 2048.
  - Stb tile bounds contain the Lev terrain in 10,253 of 10,257 tiles; the stb tile vertices carry the same heights, so stb tiles are pre-built terrain meshes from the Lev.
  - One map (`HauntedHouse_Filler_01`) leaves out an optional per-map value before its sound themes.
  - EgoCore is MIT and Windows-only, but its README cites reverse engineering, it installs Fable Script Extender, and its Def compiler is jamen/fable-defs. Black-box use only; recorded in PLAN.md and CLAUDE.md.
- Next: render the Lev heightmap in the viewer as the first terrain (it needs only the grid), then decode the rest of the stb vertex record and the grid point bytes 10–20.

## 2026-10-10 (later): Stb tiles and LZO

- Added `fable_data::lzo` (LZO1X decompression, written from the kernel's `lzo.rst` bitstream description) and `fable_data::stb` (common header, per-map headers, tile tables, tile decompression). Added `README.md`.
- Learned:
  - The stb's common header has one header per map giving its size and position (matching `.wld` for all 398 maps).
  - Each map entry holds a grid of 16x16-unit tiles, LZO1X-compressed, with empty tiles left out. Every tile in the install decompresses; output matches the reference LZO library.
  - After the tiles comes a section that looks like a spatial tree. Not decoded.
- Next: decode the decompressed tile format (terrain vertices?) and compare it with the Lev heightmap for the same map; then the tree section.

## 2026-10-10: workspace and data survey

- Created the Cargo workspace (`crates/fable-data`), pinned `nightly-2026-10-10`, added GPLv3 `LICENSE` and `CLAUDE.md`.
- Parsers with tests: the shared bank index (`bank`), `BBBB` banks (`bbbb`), `BIGB` archive bank tables (`big`) and the world file (`wld`). Every index in the install round-trips byte-for-byte.
- Wrote `docs/inventory.md`, `docs/survey.md` and format pages for the bank index, `BBBB`, `BIGB` and `Wld`.
- Learned:
  - `.wld` defines 141 regions explicitly, each with `ContainsMap` and `SeesMap` lists, so the set of maps loaded together is known.
  - The Stb has one entry per Lev (566 MB in total, about 3.8 times the Levs); it is the biggest part of every region's map data.
  - Heaviest region by Lev + Tng + Stb is `ExecutionTree` at about 83 MB, almost all from maps it sees. The Prologue's `StartOakVale` is about 33 MB.
  - The Prologue's Oakvale is region `StartOakVale`; the adult Oakvale is `OakBay`.
- Removed the openalbion clone (sourced from decompiled and leaked material; see PLAN.md).
- Next: decode a Stb per-map entry and a Lev header; list mesh and texture entries with names from `Defs/RetailHeaders`; Vita memory test once the Vita is set up.
