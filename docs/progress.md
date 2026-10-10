# Progress log

Newest first. One entry per session: what changed, what was learned, what is next.

## 2026-10-10 (later): textured terrain

- Grid point bytes 10–14 are two theme slots and their weights (plus a third slot); themes are `ENGINE_THEME` definitions naming a base (flat) and a cliff texture (`Def::theme_textures`). Every grid point uses a listed slot; 343 of 344 themes have textures.
- The viewer textures terrain: a region-wide grid texture of (layer, layer, weight, weight), base and cliff texture arrays (512x512, box-filtered mips), bilinear blending between grid points in the shader, cliff texture by slope (projected sideways). StartOakVale reads as Oakvale: grass, mud paths, the stream bed (`LANDSCAPE_WATER`), ploughed fields, earthy cliffs.
- Unnamed theme slots: a key-based lookup did not hold (the same key names different themes even within a region), so they get the map's first real named theme as a labelled stand-in.
- Set by eye, to compare with the game: texture tiling (12 world units per repeat), the slope range for cliffs.
- Ross saw water drawn on some paths. Water themes paint thin full-weight strips beside paths and on slopes; the viewer now draws them with their cliff texture (see defs.md). The grid's weight bytes are also not fully understood: where bytes 11 and 12 name the same theme, bytes 13 and 14 do not add to 255, so there may be three weights.
- Water surfaces: Ross sees water across the path under the bridge. Lake themes carry `WaterType` and `WaterHeight` fields, but `WaterHeight` is not a world height and lake themes also cover dry hillsides, so surfaces live elsewhere. Not in the Tng files (no water things). The stb sea banks use an unidentified compression (not LZO, not deflate); the Lev data after the grid is undecoded. Candidate patch for the bridge water: `WATER_BWLAKE_2` at x 3541–3544, y 901–907.
- Next: decide with Ross between decoding water surfaces now or moving on.

## 2026-10-10 (later): definitions and placed meshes

- Added `fable_data::defs`: `names.bin` and `game.bin` (type table, directory, zlib chunks, records), with `Def::graphic` finding the mesh id. 95% of placed buildings and objects resolve to a mesh.
- The viewer now places every building and object of a region on its terrain (602 of 629 meshes in `StartOakVale`; the rest are skinned). Meshes are scaled by 0.01: mesh units are centimetres if world units are metres (a chair is 119 units tall, a map 96 wide). Fences and walls join up end to end, which suggests orientation (local x right, y forward, z up) is right.
- `--camera x,y,z,distance,yaw,pitch` overrides the screenshot camera.
- Orientation fix after Ross compared with the game (tavern in the right place, but the bridge did not span its two high points and some house doors faced the cliffs): meshes face their local -y. The bridge test confirmed it from the terrain heights: only that way round do both ends rest on the 21 m plateaus.
- Clean-room note: screenshots of the game as Ross plays it count as observed gameplay. They are fine to look at but are kept out of the repo.
- Not yet: textured terrain; skinned meshes; definitions without a `Graphic`.
- Next: terrain texturing from the Lev themes, then compare against the game for Milestone A.

## 2026-10-10 (later): Tng placement

- Added `fable_data::tng` (things, components, typed accessors for definition, position, orientation) on a shared `text` tokenizer (moved out of `wld`). All 398 Tng files parse: 21,800 things.
- Confirmed the coordinate frame: buildings and objects sit at the Lev ground height (median gap 0.12 over 12,255 things); positions are map-relative, z up.
- Found that `names.bin` maps JAMCRC hashes to definition names (13,593 names). `game.bin` does not reference names by hash; its layout (offset tables, records at odd byte offsets) needs its own session.
- Next: decode `game.bin` enough to map a definition (e.g. `BUILDING_TOWN_WAREHOUSE_02`) to its mesh, then place meshes on the terrain for Milestone A.

## 2026-10-10 (later): static meshes

- Added `fable_data::mesh`: info block, LOD blocks (materials, geometry chunks, sub-meshes with strips or lists, packed or float positions, packed normals, UVs, compressed or raw streams). 2,519 of 2,583 static meshes parse (10.1 million triangles); all their texture ids exist.
- The viewer gained `--mesh NAME` and a searchable mesh list. Checked by eye: chalk circle (alpha), quest card (UVs), stump chair, Oakvale house and tavern (multi-chunk, 5 textures) all render with their original textures.
- Learned: LOD blocks fill the end of the entry, so the header can be skipped; a LOD can hold up to 6 geometry chunks; vertex format flag 16 means float positions; meshes use the same compressed-stream scheme as textures.
- Not yet: skinned meshes (bones), the mesh header (helpers, bones), the 8 extra bytes in formats 6 and 22, triangle winding (drawn double-sided for now).
- Next: place meshes in a region from the Tng files (Milestone A needs Oakvale's buildings standing on its terrain).

## 2026-10-10 (later): textures, and desktop-only scope

- Added `fable_data::dxt` (DXT1/3/5 decoding) and `fable_data::texture`. All 6,717 textures in `textures.big` and `frontend.big` unpack exactly (one 16-bit texture unsupported). PNG export via the `texture_png` example; DXT1, DXT3 and DXT5 samples checked by eye.
- Learned: the top level is LZO1X-compressed except its last 3 bytes, which follow raw (or the whole level is raw when the length is 0); the rest of the mip chain is raw; levels are padded to powers of two; levels below 4x4 are stored at bits-per-pixel size; bank type 1 is a frame sequence, other types one image; `0x20` is DXT3, `0x23` DXT5.
- Scope change (Ross): this repository is the desktop reimplementation only. The PS Vita version will be a separate repository, an ARM port that reuses these crates. PLAN.md, README, CLAUDE.md and the Nix shell were updated; Vita budgets and tooling moved to PLAN.md's "Future: PS Vita port" section, and portability rules were added.
- Next: Bbm meshes from `graphics.big`, shown textured in the viewer.

## 2026-10-10 (later): sea plane

- The viewer draws a translucent sea plane at height 3.0 for regions that have an stb sea bank and whose ground dips below 3.0.
- Learned: sea level is 3.0 (731 raised stb tiles top out there); the `.wld` `IsSea` flag does not mean "has sea"; the 25 stb sea banks are per region and compressed in an unidentified format; inland lakes have their own heights (10.5 to 78).
- Next: lake water heights (probably from the sea banks), texture themes per grid point, then `textures.big` and `graphics.big`.

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
