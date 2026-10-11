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

## 2026-10-11 (later): every static mesh parses

- 29 static (type 1) meshes, among them the burnt Oakvale's charred villagers and the statues, use the skinned layout (levels from the start of the entry, each with a skeleton) with static chunks: `Mesh::parse_static` falls back to it. The chunk end word is a count of physics blocks (4 on washing lines). All 2,583 static meshes parse; the real-data test now requires it.
- Stream lengths and bone counts are bounded before allocating (a misread length had asked for 1.2 TB).

## 2026-10-11 (later): quest-state sections, depth fix

- Ross saw bandits among the villagers and meshes changing as the camera or Hero moved.
- Tng sections are quest states and village events (`docs/formats/tng.md`); the viewer was showing all of them at once. It now shows `NULL` plus a default per region (StartOakVale: `Q_NewOakValeIntro_PreAttack`); the side panel's "Quest state" list and `--section` switch others on (the raid's aftermath, Oak Bay's events). Pre-raid Oakvale: 630 meshes, 29 creatures, all load.
- The camera's far plane was 10x its distance, so in walk mode (about 6 m, less after camera collision) meshes beyond 60 m, or a few metres, were clipped and popped in and out as the distance changed. Now fixed planes (0.05 to 5000) with reversed-Z depth (`Depth32Float` cleared to 0, `Greater`).

## 2026-10-11 (later): loading screen

- The viewer loads regions (and meshes) on a background thread and shows "Loading <region>…" with a spinner meanwhile; for a region exit the Hero is placed at the entrance on that thread too. Checked in the window (captured with `xwd`): Oak Bay -> loading screen -> Barrow Fields, 4.4 s. A queued exit or `--walk-to` target does not carry into the new region.
- `--camera`, `--walk` and `--walk-to` now also work in the window (the Hero walks himself to the point), for testing.
- Mesh chunk end word: any non-zero value is followed by a physics block (4 on `MESH_BS_WASHINGLINE_05`, whose later data still does not parse).

## 2026-10-11 (later): roofs hidden indoors

- A building's `CMultiStaticMeshDef` flags each mesh as exterior or interior (`docs/formats/defs.md`). The viewer records each building's exterior draws and triangles and its interior's box; while the Hero is inside the box, the exterior (walls and roof) is not drawn and camera rays ignore it, so the camera looks into single-storey houses from above. In two-storey buildings (the Inn) the upper floor, part of the interior, still keeps the camera low.

## 2026-10-11 (later): NPCs

- Creatures (`AICreature` things) keep their placement in `CTCPhysicsNavigator`; `Thing::position` now reads it. Their `Graphic` uses value tag 4 (skinned meshes), as do beds and doors, which now also render.
- Built-up creatures (villagers, guards, children) are a head, a torso and legs chosen from part lists in their `CCreatureDef`, each with texture swaps (Bowerstone textures -> Oakvale ones); `docs/formats/defs.md`.
- Viewer: all 30 creatures in StartOakVale stand in place, each part skinned on the CPU and playing an idle (chosen by kind as a stand-in: chickens, children, women, men), loops staggered. The UID picks each villager's parts, in place of the game's random choice.
- Not yet: creature AI or movement, which animation the game picks, scale of the young sister (0.65 like the Hero).

## 2026-10-11 (later): region transitions

- Region exits decoded (`docs/formats/gtg.md`): a `REGION_EXIT_POINT` thing's `EntranceConnectedToUID` is the destination map's `MapUID` << 40 plus the low 40 bits of a `REGION_ENTRANCE_POINT`'s UID in `Levels/FinalAlbion.gtg` (Tng text per map, now parsed by `fable_data::gtg`). All 129 exits in real regions resolve.
- Viewer: walking into an exit's radius loads the destination region and places the Hero at its entrance (exits re-arm once he has left them). Checked in screenshots: Oak Bay → Barrow Fields → Haunted House, the Phase 3 gate's three linked regions. Each region loads whole, in a few seconds; there is no loading screen or streaming yet.
- New example `region_dump` (a region's maps and things; `UID:` lookups).

## 2026-10-11 (later): building interiors

- Ross noticed the Hero walked on the ground under the Inn's furniture: building interiors were never placed. A building's `CMultiStaticMeshDef` component lists its meshes (exterior and interior); `Graphic` names only the exterior. `Def::component`, `Def::static_meshes`, `Defs::meshes_of`; the viewer places all of them (7 more meshes in StartOakVale).
- The interiors carry the floors: in StartOakVale the second navigation layer now gets heights on 1,210 of 1,220 cells (was 342, the bridge only), and the Hero walks on the Inn's and the houses' floors. The spawn point now uses the walk layer's height, not the terrain's.
- New example `mesh_dump` prints a mesh's levels, materials and chunks.

## 2026-10-11 (later): animations, the young Hero walks and runs

- Animations (bank types 6, 7, 9; 3,435 entries, 210,743 bone tracks) all parse: `fable_data::anim`, `docs/formats/anim.md`. Each entry is a u32 length and one LZO stream of chunks (`3DAF`, `ANRT` > `AOBJ` > `XSEQ` per bone, `HLPR` > `MVEC`, `XALO`). Tracks hold float quaternion keys and i16 position keys, each either constant, per frame or a palette with per-frame indices.
- Conventions checked against `MESH_YOUNGHERO_02`'s skeleton: rotations match the rest-pose convention; positions are directions scaled so the largest component is 1, their length comes from the skeleton (so adult Hero animations fit the young Hero).
- `MVEC` starts with the distance an animation carries the character per loop: `ANIM_HERO_WALK` 266.67 mesh units per second, `ANIM_HERO_RUN` 390 per 0.6 s. Walk cycles stay in place.
- Viewer: the Hero is skinned on the CPU each frame and plays `ANIM_HERO_IDLE` standing, `ANIM_HERO_WALK` walking and `ANIM_HERO_RUN` with Shift; his speed comes from the clips' `MVEC` (1.73 and 4.22 m/s at the young Hero's estimated 0.65 scale), so the feet keep pace. `--anim NAME --anim-time T` poses him for screenshots.
- Not yet: which animation the game uses for what (`CAppearanceDef` slot values are not bank ids), blending between clips, the young Hero's true scale.

## 2026-10-11: skinned meshes, the real young Hero

- Skinned meshes (bank entry type 5, 504 entries, 1431 levels of detail) all parse: skeleton (bone names, parents, rest poses, inverse bind matrices), skinned chunks with per-sub-mesh bone lists, 4 bone indices and weights per vertex, and the optional physics stream after a chunk (`docs/formats/mesh.md`). The rest pose composed down the hierarchy reproduces every inverse bind matrix (worst error 4e-6), which fixes the matrix and quaternion conventions.
- Static meshes: the chunk end flag and physics stream apply to them too; 2,549 of 2,583 now parse (was 2,519).
- `game.bin`: creature definitions are lists of components (`CAppearanceDef`, `CCreatureDef`, `CHeroMorphDef`, …) pointing at component definitions by global index. `CREATURE_HERO_CHILD`'s `CAppearanceDef` holds its animation table (`ST_IDLE_SUBTLE`, `CS_SHIFT_WEIGHT3`, …). New example `def_dump` prints a definition with known name hashes resolved.
- Viewer camera collision in walk mode: the follow camera casts rays from the Hero's chest to the camera and to eight points 0.5 m around it, against the placed meshes' triangles (bucketed in 4 m cells) and the heightmap, and pulls in in front of the nearest hit. The scroll wheel sets the preferred distance (2–15 m).
- Viewer upper walking levels: one walk grid per navigation layer; each cell's height is chosen among the terrain and the placed meshes' upward-facing surfaces by growing out from known heights (step 0.6, highest surface within a step, terrain covered lower than 1.6 not walked on). The Hero changes layer where surfaces meet within a step. The Oakvale trader bridge works end to end (`--walk-to 3560,884.5` from `--camera 3526,884.5,21,5,0,0.3`); three other second-layer patches stay unwalkable (see `docs/formats/nav.md`). "Show navigation" now draws the walk grids at their heights.
- Viewer: skinned meshes appear in the Meshes list and draw in their bind pose; walk mode now uses the real `MESH_YOUNGHERO_02` (bind pose, scaled by an estimated 0.65, since the mesh is adult-sized and the scale in the data is not found yet).

## 2026-10-10 (later): placeholder Hero and Hero textures

- Hero textures: the Prologue Hero is `MESH_YOUNGHERO_02` (mesh id 4300, type 5, skinned); its info block lists `YOUNGHERO_SKIN_FACE`, `YOUNGHERO_HAIR`, `YOUNGHERO_SKIN_TORSO` (outfit atlas) and `HERO_SKIN_MOUTH`; all decode correctly (checked by eye). Type-5 meshes (504, the character parts) share the static-mesh info layout. Creature definitions (`CREATURE_HERO_CHILD`, …) do not reference meshes through `Graphic`; how they pick body parts is not decoded.
- Viewer: a Textures tab lists every texture by name and shows the selected one.
- Viewer walk mode ("Walk" checkbox, or `--walk` for screenshots): a blocky placeholder Hero textured with the young Hero's textures, on a 0.5 m walkability grid from the ground-layer navigation leaves, height from the heightmaps, WASD relative to the camera, Shift to run, sliding along blocked edges, follow camera.
- Ross confirmed walk mode and the texture browser work in the window (2026-10-11).
- Not yet: camera collision (the camera can end up inside houses), walking on upper navigation layers (bridge decks), skinned meshes and animation.

## 2026-10-10 (later): the rest of the Lev

- Ross chose to move on from water. Added `Lev::parse_tail`: sound map (one record per 4x4 units), a fixed 33,801-byte block, and named sections, one per map state (`NULL`, quest states like `Q_NewOakValeIntro_PreAttack`). All 398 maps parse; filler maps have no sections.
- The section records look like a navigation subdivision (8 levels, node positions at half-size steps). Decoding them is the next step for walking and collision (Phase 3).
- Decoded them: `fable_data::nav` parses every section exactly (outline points, then layers of 32-unit root quadtrees down to 0.5 units; leaves carry a kind, a value, neighbour ids). Drawn in the viewer (`--nav`, or the "Show navigation" checkbox), StartOakValeWest's leaves follow the village paths and walkable ground, with holes at buildings; layer 1 is the bridge deck and raised floors.
- Next: Phase 3 proper, a walkable Hero: movement constrained to navigation leaves, ground height from the Lev, a follow camera.

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
