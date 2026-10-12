# CullisEngine: Project Plan

CullisEngine is an open-source engine for Fable: The Lost Chapters (requires the original game). Repo and crate prefix: `cullis-engine`.

Last updated: 2026-10-10 · Owner: Ross

This file is the project plan. Read it at the start of every session, together with `CLAUDE.md` and `docs/progress.md`.

## Overview

Build a clean-room reimplementation of the Fable: The Lost Chapters engine, in Rust, for desktop PCs. It runs the game from the player's own PC game files. The primary target is x86-64 desktop (Linux first, then Windows and macOS through wgpu).

This repository is desktop only. A PS Vita version will come later, in a **separate repository** that ports this engine to ARM and reuses its crates (see [Future: PS Vita port](#future-ps-vita-port)). Nothing Vita-specific lives here, but the portability rules below keep that port possible.

**Scope, in order of priority**

1. Load and render any region from the original data.
2. Walk the Hero around a region with collision, camera and animation.
3. Region transitions, NPCs, combat and saves.
4. Quest and cutscene scripting, then full-game playthrough.

**Legal stance (non-negotiable)**

- No original Lionhead code in the repo: no decompiled, disassembled or leaked source, and no translating it.
- No code or knowledge from projects that themselves work from decompiled, disassembled or leaked material. This rules out current openalbion (see Landscape).
- No game assets in the repo or in releases. The engine asks for the path to a legally owned install and reads from it.
- Format knowledge comes from published modding documentation, community tools, and black-box observation of files and gameplay.
- Licence: GPLv3 for the engine.
- No Fable trademarks in the project name (hence CullisEngine); describe it as "requires the original game".

**Success criteria**

- Milestone A: the Prologue's Oakvale region (`StartOakVale`: three maps plus the nine it sees) renders on desktop with textured terrain, water, and the static meshes placed by its Tng files.
- Milestone B: the Prologue is playable start to finish on desktop, with its quests reimplemented by hand.
- Milestone C: the full game is playable start to finish on desktop.

## Development environment

- Dev VM: Debian 13 with Nix. `nix develop` gives the desktop dependencies (Vulkan loader, lavapipe, windowing libraries, pkg-config, ffmpeg). Rust comes from rustup, pinned by `rust-toolchain.toml`.
- Claude Code CLI runs in the VM inside tmux; code review happens from the host over SSH. The VM has an X display (`:0`), so the viewer can open a window there.
- Original game is installed and played on the host (Proton on Linux). Its install folder is shared into the VM read-only and exposed as `$FABLE_DIR` (currently `/home/ross/fable_data`).
- The install in use is a European retail DVD copy of The Lost Chapters (multilingual readmes, `banks_dvd.ini`). The Steam release may differ, so tests must not hard-code file sizes or offsets.
- Rendering in the VM uses software rendering (llvmpipe or lavapipe): fine for correctness, never for performance numbers.
- The VM has no `xxd`; use `od -A x -t x1z` for hex dumps.
- VitaSDK and the Vita reference clones (`cargo-vita`, `vita-rust-examples`, `vitaGL` in `/home/ross/`) are already installed for the future Vita repository; this repository does not use them.

## Game data at a glance

Observed in `$FABLE_DIR/data` (about 2.7 GB). `docs/inventory.md` holds the full listing and `docs/survey.md` the bank and region sizes.

| Path | Size | What it is |
| --- | --- | --- |
| `Levels/FinalAlbion.wld` | small, text | The world: 398 maps (world coordinates, Lev file, script name, `LoadedOnPlayerProximity` flag) and 141 regions, each listing the maps it contains and the maps it can see |
| `Levels/FinalAlbion.wad` | 174 MB | `BBBB` bank holding every map's Lev and Tng files |
| `Levels/FinalAlbion_RT.stb` | 598 MB | `BBBB` bank: one entry per Lev (566 MB) holding pre-built terrain tiles made from the Lev heightmap, 25 compressed per-region water banks, and a common header |
| `Levels/FinalAlbion.bwd`, `.gtg` | small | Binary per-map table (starts with Lev paths); text map/section data (`NEWMAP`, `XXXSectionStart`). Not yet known |
| `Levels/FinalAlbion.qst`, `GlobalQuests.qst` | 23 KB total | Quest registration only: 187 `AddQuest`/`AddTestQuest` lines naming quests, start points and `.end` files. **No quest logic** |
| `Misc/FinalAlbion_StartingRegionGraph.txt` | small, text | Which regions connect to which |
| `graphics/graphics.big` | 244 MB | `BIGB` archive of meshes (`MBANK_ALLMESHES`, `MBANK_ENGINE`) |
| `graphics/pc/textures.big` | 533 MB | Main and GUI texture banks (`GBANK_MAIN_PC`, `GBANK_GUI_PC`); DXT1/3/5, decoded |
| `graphics/pc/frontend.big` | | Front-end (menu) textures |
| `shaders/pc/shaders.big` | | Original D3D9 shaders. Not used; materials are rewritten |
| `CompiledDefs/*.bin` | | DefBin game definitions (`game`, `script`, `frontend`, `names`) |
| `Defs/RetailHeaders/*.h` | | Plain-text headers naming bank entries (`meshdata.h`, `engmesh.h`, `fonts.h`, lipsync) |
| `Bones/*.bncfg` | 60 files | Skeleton configs |
| `lang/English/` | about 680 MB | `text.big`, `dialogue.big`, `fonts.big` (54 MB) and dialogue audio `.lut` banks (about 670 MB) |
| `Sound/` | | Per-area `.lug`/`.met` banks plus 66 `.ogg` music and cutscene tracks |
| `Video/` | 47 files | `.wmv` cinematics, including `intro_comp.wmv` and `1_raid_on_oak_vale_comp.wmv` (Prologue) |
| `banks_dvd.ini` | text | Maps each bank name to its `.big` file |

**A region is a set of maps, not one Lev file.** Each `.wld` region lists the maps it contains (`ContainsMap`) and the maps visible from it (`SeesMap`). The Prologue's Oakvale is region `StartOakVale` (3 maps, sees 9 more); the adult Oakvale is `OakBay` (2 maps, sees 14). Loading is per region: its contained plus seen maps.

## Landscape

| Project | What it gives | Notes |
| --- | --- | --- |
| [openalbion](https://github.com/jamen/openalbion) | A Rust Fable engine that already renders levels | **Off-limits.** Its AGENTS.md says it is "ported from oracles": Ghidra decompilation, symbols from leaked debug builds and a shader disassembly. Do not fork it, read its source or docs, or copy facts from it. |
| [fable_data 0.1.1](https://crates.io/crates/fable_data) | An early (April 2020) Rust parser crate, Zlib | Provenance unverified and long out of date. Not used; our own parsers replace it. |
| [EgoCore](https://github.com/eeeeeAeoN/EgoCore) | Windows asset bank editor (MIT): bank editing for `.big`/`.lut`/`.lug`, glTF mesh export, Def editor and compiler, `.wad` extraction, Fable Script Extender integration | **Black-box only.** Its README cites "hundreds of hours of reverse engineering" without saying of what, it installs Fable Script Extender (which hooks the game executable), and its Def compiler is jamen/fable-defs (openalbion's author). Do not read its source. Running it on our own files (under Wine or on the host) and comparing its exports with our decoders is allowed, as a test oracle. |
| ChocolateBox, Fable Explorer, fabletlcmod.com wiki | Legacy modding tools and format notes, including Wad, Lev, Tng and Big | The main clean source of format knowledge. Windows tools; run under Wine if needed. |

Formats we need parsers for: Big (archives), Bbm (mesh), Bba (animation), Bncfg (bones), Wad and Stb (`BBBB` banks), Lev (heightmap, navigation, sound map), Tng (thing placement), Wld, Bwd and Gtg (world layout), Def/DefBin (game definitions), Qst (quest registration), Lut/Lug/Met (audio), text and font banks, Save.

## Architecture

```
Game files ($FABLE_DIR, read-only)
   └─> fable-data (format parsers, written clean-room)
          └─> engine loads original files directly

Engine
   ├─ engine-core     world, map streaming, ECS, time, input
   ├─ engine-script   Tng things, Def lookups, quest API and quest code
   ├─ engine-audio    Lut/Lug decoding, Ogg music, mixer
   ├─ engine-ui       HUD, menus, dialogue text, fonts
   ├─ engine-video    cinematic playback
   └─ engine-render   backend-neutral render API + wgpu backend
```

**Key decisions**

- **Language: Rust**, built for x86-64 desktop with wgpu (Vulkan, DX12, Metal, GL).
- **Own parsers.** fable-data is written in this repo from community docs and file observation, not forked.
- **Backend-neutral render API.** engine-render exposes meshes, materials, passes and draw lists; wgpu is the only backend in this repo. Game code never touches wgpu directly, so another backend can be added elsewhere.
- **Materials are rewritten.** The original D3D9 shaders in `shaders.big` are not read. Water, foliage, skin, terrain blending and lightmaps are matched by eye against the original game.
- **Map streaming.** engine-core loads a region's contained and seen maps from `.wld` and frees them on leaving, keeping maps shared with the next region.
- **Fixed 30 Hz simulation**, decoupled from rendering, so game logic is deterministic and frame-rate independent.
- **Data-driven from Def files.** Read creature, item and object definitions from the original DefBin rather than hardcoding them.
- **Quests are new code.** The game data only names quests; their logic is not in the data. Each quest is written by hand against a quest API, from playthroughs of the original. Choose in Phase 3 whether quests are Rust or an embedded scripting language (Lua or Rhai), so quest work can become content rather than engine work.
- **Video is converted.** WMV is not practical to decode in-engine. A one-time import step converts the cinematics to a modern codec with the user's ffmpeg; the engine plays the converted files.

**Portability rules** (so the later ARM port can reuse these crates unchanged)

- No x86-specific code: no SIMD intrinsics or inline assembly without a portable fallback.
- Parse every file with explicit little-endian reads (never by casting structs over bytes) and never assume a pointer width.
- `fable-data`, `engine-core`, `engine-script` and `engine-audio` have no dependency on wgpu, windowing or desktop-only crates. Platform code lives in `engine-render`'s backend and in `apps/`.
- Shared engine code: no per-frame allocation, no `std::process`, no threads beyond the job system.

## Roadmap

Six phases, each ending at a testable gate.

| Phase | Work | Gate |
| --- | --- | --- |
| 0. Setup | Toolchains, workspace, CLAUDE.md, data survey | Workspace builds; install inventoried (**done**) |
| 1. Assets and viewer | Containers, world, heightmaps, textures, meshes parse with tests; terrain in the viewer | Any region's terrain renders on desktop (**done**); every texture decodes (**done**); static meshes render textured (**done**; skinned meshes remain) |
| 2. Rendering a region | Textured terrain (themes), water, meshes, Tng thing placement, materials matched by eye | **Milestone A:** `StartOakVale` rendered on desktop (terrain textures, placement and sea done; to compare with the game) |
| 3. Gameplay core | Hero movement, collision, camera, animation, NPCs, map streaming and region transitions; quest API design | Walk the Hero across three linked regions (navigation decoded: `docs/formats/nav.md`; the young Hero, skinned mesh and animations decoded, walks and runs animated in the viewer, with camera collision, bridge decks and building interiors; region exits work: Oak Bay → Barrow Fields → Haunted House; NPCs placed and idling) |
| 4. Scripts and quests | Tng things, Def data, quest API, dialogue and text UI, video playback, cutscenes, saves; Prologue quests one by one | First Prologue quest complete (**done** in the viewer: Theresa's birthday, `docs/quests/prologue-birthday.md`; quest API in `engine-script`); then **Milestone B:** Prologue playable on desktop |
| 5. Full game | Remaining quests, combat, magic, Lost Chapters regions, polish | **Milestone C:** full playthrough on desktop |

Phase 4 and Phase 5 are the long poles: the game registers 187 quests (including test entries), and every one is written from scratch. After the first Prologue quest, record how long it took and use that to estimate the rest.

## Working with Claude Code

Claude Code works best on small, testable crates with clear rules in CLAUDE.md, so the repo is a Cargo workspace and every format and system gets tests against the local game files.

**Repo layout**

```
cullis-engine/
  PLAN.md              this file
  CLAUDE.md            rules, build commands, clean-room policy
  docs/inventory.md    full listing of $FABLE_DIR/data
  docs/survey.md       bank, world and region sizes
  docs/formats/        one page per file format, written from observation
  docs/quests/         expected behaviour of each quest, from playthroughs
  docs/progress.md     session log
  crates/
    fable-data/        parsers, written clean-room in this repo
    engine-core/       ECS, world, map streaming, time, input
    engine-render/     backend-neutral render API + wgpu backend
    engine-audio/      Lut/Lug decoding, Ogg music, mixer
    engine-script/     Tng things, Def lookups, quest API and quests
    engine-ui/         HUD, menus, dialogue text, fonts
    engine-video/      cinematic playback
  apps/
    viewer/            desktop asset viewer (egui)
    game/              the desktop game
  tests/fixtures/      synthetic files only, never real game data
```

**What goes in CLAUDE.md**

- The clean-room rule in plain words: never paste, request or reproduce decompiled or disassembled Fable code; work only from format docs, file bytes and observed behaviour.
- Never read openalbion's source or docs, and never reproduce knowledge from any project built on decompiled or leaked material.
- Game data lives at `$FABLE_DIR`, read-only, never committed. Tests that need it are marked `#[ignore]` and run with `cargo test -- --ignored`.
- Tests must not hard-code file sizes or offsets that could differ between the DVD and Steam releases.
- Desktop build and test commands.
- The portability rules above.
- Every new or changed format must update its page in `docs/formats/`.

**Working pattern**

1. One task per session, scoped to one crate ("parse Lev sound map cells", not "do levels").
2. Write the test first, using a real file from `$FABLE_DIR`, then the parser.
3. Round-trip where possible: decode, re-encode, compare bytes.
4. Check results by eye in the viewer before moving on.
5. Append to `docs/progress.md` at the end of every session so the next one can pick up where this one stopped.

## Risks

The biggest risk is the quests, not the renderer: the data contains none of their logic, so every quest is new code, and they decide whether the game is playable at all.

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Quest logic is not in the game data | Every quest (187 registered) is written from scratch; quests break or soft-lock | Quest API designed in Phase 3; implement quest by quest, starting with the Prologue; write expected behaviour from playthroughs into `docs/quests/`; estimate from the first quest |
| Parts of the level data stay undecoded (Stb vertex attributes, water banks, Lev grid bytes 10–20) | Regions render without the original's lighting, texture blending or lakes | Decode against the game on screen; fall back to rebuilding from the Lev heightmap, which already works |
| Desktop code grows x86- or platform-specific assumptions | The later ARM port has to fork or rewrite crates | Portability rules in CLAUDE.md; keep platform code in the render backend and `apps/` |
| Clean-room contamination, including through openalbion or other projects built on decompiled or leaked material | Legal takedown | CLAUDE.md rule; openalbion clone removed from the dev machine; no decompiler on the project machine; check the sources of every reference tool; contributors sign off on sources |
| Single-maintainer burnout | Project stalls | Publish milestones early; keep quest work as content others can contribute |
| Microsoft takedown | Repo removal | No assets, no Fable trademarks in the project name, clear "requires original game" wording |

## First two weeks

By the end of week two: one Fable mesh, textured, in a desktop window, and a map of what the data contains. (Done 2026-10-10.)

**Week 1: foundations**

- [x] Install Nix and nightly Rust
- [x] Create the workspace and write CLAUDE.md with the clean-room rule
- [x] Remove the `openalbion` clone from `/home/ross/`
- [x] Set `$FABLE_DIR` to the install; list its `data/` tree into `docs/inventory.md`, including the install version

**Week 1–2: data survey**

- [x] List the entries of `FinalAlbion.wad` and `FinalAlbion_RT.stb` (both `BBBB` banks), with a test
- [x] Parse `FinalAlbion.wld` into a table of maps: name, coordinates, script name, proximity flag, Lev size
- [x] Group maps into regions (`.wld` defines them); list Oakvale's maps
- [ ] Note what `.bwd` and `.gtg` contain
- [x] Parse every Tng file and confirm thing positions against the terrain (`docs/formats/tng.md`)
- [x] Map definitions to meshes through `CompiledDefs/game.bin` (`docs/formats/defs.md`)
- [x] Place a region's buildings and objects on its terrain in the viewer
- [x] Check placed mesh orientation against the game (meshes face local -y; bridge and houses re-checked)
- [x] Texture the terrain from the Lev themes (tiling and cliff blend set by eye; compare with the game)

**Week 2: first pixels**

- [x] Open `graphics/graphics.big` and `graphics/pc/textures.big` and parse every bank index, with a test
- [ ] Match mesh entries to names in `Defs/RetailHeaders/meshdata.h`; find out what each entry type number means
- [x] Decode the Stb common header, map tile tables and LZO1X tile compression (`docs/formats/stb.md`)
- [x] Decode the Lev heightmap and confirm it against the Stb tiles (`docs/formats/lev.md`)
- [ ] Decode the rest of the Stb tile vertex records and the section after the tiles
- [x] Draw the Lev heightmap of `StartOakVale` as terrain in the viewer (`apps/viewer`, any region selectable)
- [x] Check the viewer's orientation against the game (confirmed by Ross: correct, not mirrored)
- [x] Sea plane at sea level (3.0) in the viewer
- [x] Decode every texture (DXT1/3/5, BGRA) and export to PNG; checked by eye (`docs/formats/texture.md`)
- [x] Load a static mesh into the egui viewer with wgpu, textured (`--mesh`, mesh list in the side panel)
- [x] Write `docs/formats/big.md` and `docs/formats/wld.md` (also `bbbb.md`, `bank-index.md`)
- [x] Write `docs/formats/mesh.md`
- [x] Note the size of the largest texture bank and mesh bank (`GBANK_MAIN_PC` 501 MB, `MBANK_ALLMESHES` 223 MB; `docs/survey.md`)

## Future: PS Vita port

A separate repository, started once the desktop engine is far enough along. It ports this engine to ARM (the Vita's Cortex-A9, target `armv7-sony-vita-newlibeabihf`), depending on this repository's crates rather than copying them, and adds what only the Vita needs:

- a vitaGL render backend for engine-render ([vitaGL](https://github.com/Rinnegatamante/vitaGL), with `libshacccg.suprx` extracted on the console for runtime shader compiling);
- an offline converter that turns the original data into a Vita-sized pack (DXT textures with the top mip dropped, batched static geometry, H.264 cinematics for the hardware decoder, one language);
- memory and CPU budgets measured on the device (the system reserves part of the 512 MB RAM, 128 MB CDRAM and one of the four cores);
- builds with [cargo-vita](https://github.com/vita-rust/cargo-vita) and [VitaSDK](https://vitasdk.org) (already installed on the dev VM at `/usr/local/vitasdk`).

Notes for that repository from this one: the heaviest region by raw map data is `ExecutionTree` at about 83 MB and `StartOakVale` about 33 MB (`docs/survey.md`), before meshes and textures; texture banks are already DXT, which the Vita samples natively.

## Sources

- [fable_data 0.1.1 on crates.io](https://crates.io/crates/fable_data) (provenance unverified, see Landscape)
- [EgoCore on GitHub](https://github.com/eeeeeAeoN/EgoCore)
- Vita port references: [Rust armv7-sony-vita-newlibeabihf target](https://doc.rust-lang.org/rustc/platform-support/armv7-sony-vita-newlibeabihf.html), [cargo-vita](https://github.com/vita-rust/cargo-vita), [vita-rust examples](https://github.com/vita-rust/examples), [vitaGL](https://github.com/Rinnegatamante/vitaGL)

Game data figures were observed in the local install on 2026-10-10.
