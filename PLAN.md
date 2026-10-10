# CullisEngine: Project Plan

CullisEngine is an open-source engine for Fable: The Lost Chapters (requires the original game). Repo and crate prefix: `cullis-engine`.

Last updated: 2026-10-10 · Owner: Ross

This file is the project plan. Read it at the start of every session, together with `CLAUDE.md` and `docs/progress.md`.

## Overview

Build a clean-room engine, in Rust, that runs Fable: The Lost Chapters from the player's own PC game files, first on desktop and then on a modded PS Vita. Desktop comes first because every system is easier to build and debug there. The Vita is a second backend, not a separate project.

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

- Milestone A: the Prologue's Oakvale region (`StartOakVale`: three maps plus the nine it sees) renders at a stable 30 fps on a Vita at 960x544.
- Milestone B: the Prologue is playable start to finish on desktop, with its quests reimplemented by hand.
- Milestone C: the Prologue is playable on a Vita within the memory actually available to a homebrew app, measured in Phase 0.

## Development environment

- Dev VM: Debian 13 with Nix. `nix develop` gives the desktop dependencies (Vulkan loader, lavapipe, windowing libraries, pkg-config, ffmpeg) and sets `VITASDK`. Rust comes from rustup, pinned by `rust-toolchain.toml`; VitaSDK is installed with vdpm at `/usr/local/vitasdk`.
- Claude Code CLI runs in the VM inside tmux; code review happens from the host over SSH.
- VM networking is bridged, so the Vita can reach the VM for cargo-vita uploads and PrincessLog output.
- Original game is installed and played on the host (Proton on Linux). Its install folder is shared into the VM read-only and exposed as `$FABLE_DIR` (currently `/home/ross/fable_data`).
- The install in use is a European retail DVD copy of The Lost Chapters (multilingual readmes, `banks_dvd.ini`). The Steam release may differ, so tests must not hard-code file sizes or offsets.
- Reference clones live in `/home/ross/`: `cargo-vita`, `vita-rust-examples` and `vitaGL`. The openalbion clone was removed (see Landscape).
- Desktop rendering in the VM may use software Vulkan (lavapipe): fine for correctness, never for performance numbers. Performance is measured on real Vita hardware only.
- The VM has no `xxd`; use `od -A x -t x1z` for hex dumps.

## Game data at a glance

Observed in `$FABLE_DIR/data` (about 2.7 GB). This replaces guesswork about where things live; `docs/inventory.md` will hold the full listing.

| Path | Size | What it is |
| --- | --- | --- |
| `Levels/FinalAlbion.wld` | small, text | The world: 398 maps (world coordinates, Lev file, script name, `LoadedOnPlayerProximity` flag) and 141 regions, each listing the maps it contains and the maps it can see |
| `Levels/FinalAlbion.wad` | 174 MB | `BBBB` bank holding every map's Lev and Tng files |
| `Levels/FinalAlbion_RT.stb` | 598 MB | `BBBB` bank: one entry per Lev (566 MB, about 3.8 times the Levs), 25 sea static-map banks and a common header. Names suggest pre-built static map data; contents not yet decoded |
| `Levels/FinalAlbion.bwd`, `.gtg` | small | Binary per-map table (starts with Lev paths); text map/section data (`NEWMAP`, `XXXSectionStart`). Not yet known |
| `Levels/FinalAlbion.qst`, `GlobalQuests.qst` | 23 KB total | Quest registration only: 187 `AddQuest`/`AddTestQuest` lines naming quests, start points and `.end` files. **No quest logic** |
| `Misc/FinalAlbion_StartingRegionGraph.txt` | small, text | Which regions connect to which |
| `graphics/graphics.big` | 244 MB | `BIGB` archive of meshes (`MBANK_ALLMESHES`, `MBANK_ENGINE`) |
| `graphics/pc/textures.big` | 533 MB | Main and GUI texture banks (`GBANK_MAIN_PC`, `GBANK_GUI_PC`) |
| `graphics/pc/frontend.big` | | Front-end (menu) textures |
| `shaders/pc/shaders.big` | | Original D3D9 shaders. Not used; materials are rewritten |
| `CompiledDefs/*.bin` | | DefBin game definitions (`game`, `script`, `frontend`, `names`) |
| `Defs/RetailHeaders/*.h` | | Plain-text headers naming bank entries (`meshdata.h`, `engmesh.h`, `fonts.h`, lipsync) |
| `Bones/*.bncfg` | 60 files | Skeleton configs |
| `lang/English/` | about 680 MB | `text.big`, `dialogue.big`, `fonts.big` (54 MB) and dialogue audio `.lut` banks (about 670 MB) |
| `Sound/` | | Per-area `.lug`/`.met` banks plus 66 `.ogg` music and cutscene tracks |
| `Video/` | 47 files | `.wmv` cinematics, including `intro_comp.wmv` and `1_raid_on_oak_vale_comp.wmv` (Prologue) |
| `banks_dvd.ini` | text | Maps each bank name to its `.big` file |

**A region is a set of maps, not one Lev file.** Each `.wld` region lists the maps it contains (`ContainsMap`) and the maps visible from it (`SeesMap`). The Prologue's Oakvale is region `StartOakVale` (3 maps, sees 9 more); the adult Oakvale is `OakBay` (2 maps, sees 14). Loading, memory budgets and Milestone A are all measured per region: its contained plus seen maps. Per-region sizes are in `docs/survey.md`.

## Landscape

| Project | What it gives | Notes |
| --- | --- | --- |
| [openalbion](https://github.com/jamen/openalbion) | A Rust Fable engine that already renders levels | **Off-limits.** Its AGENTS.md says it is "ported from oracles": Ghidra decompilation, symbols from leaked debug builds and a shader disassembly. Do not fork it, read its source or docs, or copy facts from it. |
| [fable_data 0.1.1](https://crates.io/crates/fable_data) | An early (April 2020) Rust parser crate, Zlib | Provenance unverified and long out of date. Do not depend on it until its sources are confirmed clean; by default write our own parsers. |
| [EgoCore](https://www.nexusmods.com/fablethelostchapters/mods/592) | C++ modding framework: DXT1/3/5 and ARGB textures, glTF export of meshes and animations, audio format work | Black-box reference for format behaviour only. Do not copy its code. Check its sources before relying on it. |
| ChocolateBox, Fable Explorer, fabletlcmod.com wiki | Legacy modding tools and format notes, including Wad, Lev, Tng and Big | The main clean source of format knowledge. Windows tools; run under Wine if needed. |
| [cargo-vita](https://github.com/vita-rust/cargo-vita) + [Rust Vita target](https://doc.rust-lang.org/rustc/platform-support/armv7-sony-vita-newlibeabihf.html) | Builds Rust to a .vpk, uploads and runs it, streams logs | Tier 3 target, needs nightly and build-std. `std` mostly works; `std::process` does not. |
| [vitaGL](https://github.com/Rinnegatamante/vitaGL) | OpenGL-style API over GXM, GLSL-to-Cg translation | Actively maintained. Runtime shader compiling needs `libshacccg.suprx` extracted on the console. |
| [VitaSDK](https://vitasdk.org) | Toolchain, headers, `vita-pack-vpk` | Required by everything above. |

Formats we need parsers for: Big (archives), Bbm (mesh), Bba (animation), Bncfg (bones), Wad and Stb (`BBBB` banks), Lev (heightmap, navigation, sound map), Tng (thing placement), Wld, Bwd and Gtg (world layout), Def/DefBin (game definitions), Qst (quest registration), Lut/Lug/Met (audio), text and font banks, Save. Texture banks use DXT1/3/5, which matches the Vita's native block-compressed texture support.

## Vita hardware budget

The figures below are working targets to test against, not measured figures. The system keeps part of the 512 MB main RAM and 128 MB CDRAM, and one of the four cores, so an app gets less than the totals. Measure the real ceiling with a test allocation in Phase 0 and set the budgets from that.

| Resource | Vita total | Working budget per loaded map set | How to stay inside it |
| --- | --- | --- | --- |
| CPU | 4x Cortex-A9 (3 for apps), 333 MHz default, 444 MHz unlocked | Game logic under 16 ms per frame on one core at 444 MHz | Fixed 30 Hz simulation; AI and pathfinding on a second core; no per-frame allocation |
| Main RAM | 512 MB | About 300 MB, to confirm in Phase 0 | Stream maps by proximity; arena allocator per map |
| VRAM (CDRAM) | 128 MB | About 96 MB, to confirm in Phase 0 | DXT textures, top mip dropped, shared atlases for props |
| Screen | 960x544 | Render at native, 30 fps target | Lower LOD distances; fog to cap draw distance |
| Draw calls | No hard limit | Under 500 per frame | Static batching per map; instancing for repeated props |
| Storage | Memory card or SD2Vita | Converted pack well under the 2.7 GB original | One language only; re-encode dialogue audio; convert video |

Open question: how much memory does the busiest region need once loaded? By raw map data (Lev + Tng + Stb for contained and seen maps), the heaviest is `ExecutionTree` at about 83 MB and `StartOakVale` is about 33 MB (`docs/survey.md`). Still to add: meshes, textures, creatures and the decoded (not file) sizes. Measure on desktop in Phase 1 before committing to the Vita budget.

## Architecture

One Rust engine runs on both targets. The Vita differs only in its renderer backend and in loading a pre-converted asset pack.

```
Game files ($FABLE_DIR, read-only)
   └─> fable-data (format parsers, written clean-room)
          ├─> desktop: engine loads original files directly
          └─> fable-convert (runs once on PC)
                 └─> Vita asset pack (DXT, small mips, batched, H.264 video)
                        └─> Vita: engine loads the pack

Engine (one codebase for both targets)
   ├─ engine-core     world, map streaming, ECS, time, input
   ├─ engine-script   Tng things, Def lookups, quest API and quest code
   ├─ engine-audio    Lut/Lug decoding, Ogg music, mixer
   ├─ engine-ui       HUD, menus, dialogue text, fonts
   ├─ engine-video    cinematic playback
   └─ engine-render   renderer trait
          ├─ wgpu backend    (desktop: Vulkan, GL, Metal)
          └─ vitaGL backend  (Vita: GL calls over GXM)
```

**Key decisions**

- **Language: Rust.** Has a working Vita target. Call vitaGL through a small `bindgen` FFI crate.
- **Own parsers.** fable-data is written in this repo from community docs and file observation, not forked.
- **Renderer trait, not an API translation.** engine-render defines meshes, materials, passes and draw lists. The wgpu backend serves desktop; the vitaGL backend serves the Vita. Design the trait around what the Vita does well: static batches, baked lighting, DXT textures.
- **Materials are rewritten.** The original D3D9 shaders in `shaders.big` are not read. Water, foliage, skin, terrain blending and lightmaps are matched by eye against the original game.
- **Map streaming.** engine-core loads a region's contained and seen maps from `.wld` and frees them on leaving, keeping maps shared with the next region.
- **Fixed 30 Hz simulation**, decoupled from rendering, so game logic behaves the same on both targets.
- **Data-driven from Def files.** Read creature, item and object definitions from the original DefBin rather than hardcoding them.
- **Quests are new code.** The game data only names quests; their logic is not in the data. Each quest is written by hand against a quest API, from playthroughs of the original. Choose in Phase 3 whether quests are Rust or an embedded scripting language (Lua or Rhai), so quest work can become content rather than engine work.
- **Video is converted.** WMV is not playable on either target. fable-convert transcodes cinematics to H.264 for the Vita's hardware decoder; desktop plays the same converted files.
- **Desktop-first, Vita-continuous.** Every phase ends with a Vita build, so performance problems surface early.

## Roadmap

Seven phases, each ending at a testable gate. The Vita renderer comes early (Phase 2) so the hardware limits shape everything after it.

| Phase | Work | Gate |
| --- | --- | --- |
| 0. Setup | Toolchains, workspace, CLAUDE.md, data survey, Vita memory test, first textured mesh in the viewer | Rust test app runs on the Vita and reports its usable RAM and VRAM |
| 1. Assets and viewer | All formats parse and round-trip; any map set renders on desktop; decide whether Stb is needed | Every map loads; busiest loaded map set measured |
| 2. Vita renderer | fable-convert and the vitaGL backend; static map set with a free camera | **Milestone A:** Oakvale at 30 fps on the Vita |
| 3. Gameplay core | Hero movement, collision, camera, animation, NPCs, map streaming and region transitions; quest API design | Walk the Hero across three linked regions |
| 4. Scripts and quests | Tng things, Def data, quest API, dialogue and text UI, video playback, cutscenes, saves; Prologue quests one by one | First Prologue quest complete; then **Milestone B:** Prologue playable on desktop |
| 5. Vita parity | Profile on device, AI on a second core, cut every map set to the memory budget | **Milestone C:** Prologue playable on the Vita |
| 6. Full game | Remaining quests, combat, magic, Lost Chapters regions, polish | Full playthrough on both targets |

Phases 0 to 2 are realistic for one person working evenings with Claude Code. Phase 4 and Phase 6 are the long poles: the game registers 187 quests (including test entries), and every one is written from scratch. After the first Prologue quest, record how long it took and use that to estimate the rest.

## Working with Claude Code

Claude Code works best on small, testable crates with clear rules in CLAUDE.md, so the repo is a Cargo workspace and every format and system gets tests against the local game files.

**Repo layout**

```
cullis-engine/
  PLAN.md              this file
  CLAUDE.md            rules, build commands, clean-room policy
  docs/inventory.md    full listing of $FABLE_DIR/data
  docs/formats/        one page per file format, written from observation
  docs/quests/         expected behaviour of each quest, from playthroughs
  docs/progress.md     session log
  crates/
    fable-data/        parsers, written clean-room in this repo
    fable-convert/     offline converter: original data -> Vita pack
    engine-core/       ECS, world, map streaming, time, input
    engine-render/     renderer trait + wgpu and vitaGL backends
    engine-audio/      Lut/Lug decoding, Ogg music, mixer
    engine-script/     Tng things, Def lookups, quest API and quests
    engine-ui/         HUD, menus, dialogue text, fonts
    engine-video/      cinematic playback
  apps/
    viewer/            desktop asset viewer (egui)
    game-desktop/
    game-vita/         cargo-vita package
  tests/fixtures/      synthetic files only, never real game data
```

**What goes in CLAUDE.md**

- The clean-room rule in plain words: never paste, request or reproduce decompiled or disassembled Fable code; work only from format docs, file bytes and observed behaviour.
- Never read openalbion's source or docs, and never reproduce knowledge from any project built on decompiled or leaked material.
- Game data lives at `$FABLE_DIR`, read-only, never committed. Tests that need it are marked `#[ignore]` and run with `cargo test -- --ignored`.
- Tests must not hard-code file sizes or offsets that could differ between the DVD and Steam releases.
- Build commands for desktop and `cargo vita build vpk --upload` for the Vita.
- Memory rules for shared code: no per-frame allocation, no `std::process`, no threads beyond the job system.
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
| Level rendering likely depends on the undecoded Stb per-map entries | "Render any region" stalls | Decode one Stb entry early in Phase 1, from bytes and by comparing against the game on screen; decide whether it is needed or can be rebuilt from Lev and Tng |
| Vita CPU too slow for busy areas | Frame rate drops below 30 | Profile on device from Phase 2; move AI to a second core; cap active NPCs per map |
| Loaded map set exceeds the memory budget | Crashes on load | Measure real app memory in Phase 0 and the busiest map set in Phase 1; tighten proximity radius if needed |
| Rust Vita target is tier 3 | Toolchain breaks after a nightly update | Pin the nightly in `rust-toolchain.toml` (or the Nix flake); CI builds the Vita package on every push |
| Runtime shader compiler needs `libshacccg.suprx` | Users can't run it without an extra setup step | Ship precompiled shaders if vitaGL supports it for these shaders; otherwise document extraction clearly |
| Clean-room contamination, including through openalbion or other projects built on decompiled or leaked material | Legal takedown | CLAUDE.md rule; openalbion clone removed from the dev machine; no decompiler on the project machine; check the sources of every reference tool; contributors sign off on sources |
| Single-maintainer burnout | Project stalls | Publish milestones early; keep quest work as content others can contribute |
| Microsoft takedown | Repo removal | No assets, no Fable trademarks in the project name, clear "requires original game" wording |

## First two weeks

By the end of week two: one Fable mesh, textured, in a desktop window; a Rust hello-world running on the Vita; and a map of what the data contains.

**Week 1: foundations**

- [x] Install Nix, VitaSDK, nightly Rust and cargo-vita
- [ ] Set up vitacompanion and PrincessLog on the Vita
- [ ] Build and run a vita-rust example on device
- [ ] On the Vita, find the largest main-RAM and CDRAM allocations a homebrew app can make; record them in `docs/progress.md`
- [x] Create the workspace and write CLAUDE.md with the clean-room rule
- [x] Remove the `openalbion` clone from `/home/ross/`
- [x] Set `$FABLE_DIR` to the install; list its `data/` tree into `docs/inventory.md`, including the install version

**Week 1–2: data survey**

- [x] List the entries of `FinalAlbion.wad` and `FinalAlbion_RT.stb` (both `BBBB` banks), with a test
- [x] Parse `FinalAlbion.wld` into a table of maps: name, coordinates, script name, proximity flag, Lev size
- [x] Group maps into regions (`.wld` defines them); list Oakvale's maps
- [ ] Note what `.bwd` and `.gtg` contain

**Week 2: first pixels**

- [x] Open `graphics/graphics.big` and `graphics/pc/textures.big` and parse every bank index, with a test
- [ ] Match mesh entries to names in `Defs/RetailHeaders/meshdata.h`; find out what each entry type number means
- [ ] Decode one Stb per-map entry header
- [ ] Decode a DXT texture to PNG; check it by eye
- [ ] Load one Bbm mesh into the egui viewer with wgpu
- [x] Write `docs/formats/big.md` and `docs/formats/wld.md` (also `bbbb.md`, `bank-index.md`)
- [ ] Write `docs/formats/bbm.md`
- [x] Note the size of the largest texture bank and mesh bank for the budget (`GBANK_MAIN_PC` 501 MB, `MBANK_ALLMESHES` 223 MB; `docs/survey.md`)

## Sources

- [fable_data 0.1.1 on crates.io](https://crates.io/crates/fable_data) (provenance unverified, see Landscape)
- [EgoCore on Nexus Mods](https://www.nexusmods.com/fablethelostchapters/mods/592)
- [Rust armv7-sony-vita-newlibeabihf target](https://doc.rust-lang.org/rustc/platform-support/armv7-sony-vita-newlibeabihf.html)
- [cargo-vita](https://github.com/vita-rust/cargo-vita) and [vita-rust examples](https://github.com/vita-rust/examples)
- [vitaGL](https://github.com/Rinnegatamante/vitaGL)

Vita hardware figures are from general knowledge, not a fetched spec sheet. Per-map budgets are planning targets to verify on device. Game data figures were observed in the local install on 2026-10-10.
