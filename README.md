# CullisEngine

An open-source, clean-room reimplementation of the **Fable: The Lost Chapters** engine for desktop PCs, written in Rust. A PS Vita port is planned as a separate project that will build on this one.

**Requires the original game.** CullisEngine ships no game data. It reads files from your own legally owned PC install.

## Status

Early. Nothing is playable yet. The desktop viewer draws any region from the original data: textured terrain, sea, and its buildings and objects placed and textured; or any static mesh on its own. The `fable-data` crate reads:

- the bank and archive containers (`.wad`, `.stb`, `.big`) and their indexes
- the world layout (`.wld`): 398 maps and 141 regions
- LZO1X-compressed data
- Lev heightmaps
- textures (DXT1/3/5 and uncompressed), exportable to PNG
- static meshes (all levels of detail; skinned meshes not yet)
- Tng thing placement and compiled game definitions (definition to mesh)
- terrain themes, the sound map and navigation (walkable areas, shown with `--nav`)

See [PLAN.md](PLAN.md) for the roadmap and [docs/progress.md](docs/progress.md) for the session log.

## Building

You need Rust via [rustup](https://rustup.rs). `rust-toolchain.toml` pins the nightly. On Linux with Nix, `nix develop` provides the rest of the desktop dependencies.

```sh
cargo build
cargo test
```

Tests that need the game data are ignored by default. Point `FABLE_DIR` at your install to run them:

```sh
FABLE_DIR=/path/to/Fable cargo test --release -- --ignored
```

To open the viewer (drag to orbit, right-drag to pan, scroll to zoom, pick a region on the left):

```sh
FABLE_DIR=/path/to/Fable cargo run --release -p viewer -- --region StartOakVale
FABLE_DIR=/path/to/Fable cargo run --release -p viewer -- --mesh MESH_OAKVALE_HOUSE_02
```

The side panel switches between regions, meshes and textures, with a search box. In a region, "Show navigation" draws the walkable area and "Walk" puts the young Hero on it, animated (WASD to walk, Shift to run, the scroll wheel sets the camera distance; the camera keeps out of walls and buildings; bridge decks and building floors are walkable, and walking into a region exit loads the next region). Villagers, guards, children and chickens stand around playing idle animations. Going indoors hides the building's roof and outer walls. Regions load in the background behind a loading screen. The side panel's "Quest state" list shows things from other quest states (the Prologue raid's aftermath, village events); `--section NAME` does the same for screenshots. For screenshots, `--walk` places the Hero and `--walk-to x,y` walks him towards a point first, and `--anim ANIM_HERO_RUN --anim-time 0.2` poses him with an animation. Character meshes are listed with the others and drawn in their bind pose.

Add `--screenshot out.png --size 1280x800` to render one frame to a file without opening a window.

To print a survey of your install's banks and regions:

```sh
FABLE_DIR=/path/to/Fable cargo run --release -p fable-data --example survey
```

## Documentation

- [docs/formats/](docs/formats/): file formats, written from observation of the data files
- [docs/inventory.md](docs/inventory.md) and [docs/survey.md](docs/survey.md): what the reference install contains

## Clean-room policy

No decompiled, disassembled or leaked code or symbols, and no knowledge taken from projects that rely on them. Format knowledge comes from public modding documentation and from observing data files and gameplay. No game assets are stored in this repository. Contributors should read [CLAUDE.md](CLAUDE.md), which states the rules in full.

## Licence

GPLv3 or later; see [LICENSE](LICENSE).

Fable is a trademark of Microsoft. This project is not affiliated with or endorsed by Microsoft or Lionhead Studios.
