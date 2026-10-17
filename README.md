# CullisEngine

An open-source, clean-room reimplementation of the Fable: The Lost Chapters engine for desktop PCs, written in Rust. A PS Vita port is planned as a separate project that will build on this one.

Requires the original game. CullisEngine ships no game data. It reads files from your own legally owned PC install.

## Status

Early, but the Prologue can be played through in the desktop viewer:

- any region renders from the original data: textured terrain, sea, buildings and objects, doors, and creatures with their idle animations
- the young Hero walks and runs through Oakvale with collision, a following camera, walkable building floors and bridge decks, doors to open, and region exits to the next region
- the Prologue's quests: Theresa's birthday (good and bad deeds, gold, the Trader's sweets), the bandit raid's aftermath, and Maze taking the boy to the Heroes' Guild
- conversations from the game's own text, spoken in the original voices and shown as subtitles with yes/no questions; speakers gesture as they talk, by the line's mood, their mouths move with the voice, and the camera frames the conversation between letterbox bars; whoever or whatever E would use glows green
- each region's music, and ambience that follows where the Hero walks (village, shore, sea, river)
- light and sky by time of day from the region's environment theme: dawn, day, dusk and night, sun shadows from buildings, people and objects, warm light indoors, and the red glow over Oakvale after the raid

Not yet: combat beyond a kick, sound effects, night ambience, The Lost Chapters' voices, cutscenes, clouds, torches and other placed lights, particles (fires), saves. See [PLAN.md](PLAN.md) for the roadmap and [docs/progress.md](docs/progress.md) for the session log.

## Crates

- `fable-data` reads the game's files:
  - the bank and archive containers (`.wad`, `.stb`, `.big`) and their indexes, and LZO1X-compressed data
  - the world layout (`.wld`, `.gtg`): 398 maps, 141 regions, region entrances
  - Lev heightmaps, terrain themes, the sound map and navigation
  - textures (DXT1/3/5 and uncompressed), exportable to PNG
  - static and skinned meshes, skeletons, bone configs and animations
  - Tng thing placement and compiled game definitions
  - the text bank (`lang/<language>/text.big`) and the voice banks (`*.lut`, Xbox ADPCM), with which line is which sound
  - the ambient banks (`Sound/*.met`, `*.lug`) and which music and ambience play where
- `engine-audio` mixes sounds for an output device (voices, music, ambience; fades and looping) and decodes Ogg Vorbis; no platform dependency.
- `engine-script` holds the quest API and the quests, written by hand from how the game plays (the data has no quest logic). No rendering dependency.
- `apps/viewer` is the desktop viewer (egui and wgpu), where everything is tried out.

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

### The game data

`FABLE_DIR` is the **install folder itself**, the one that contains `data` (with `Levels`, `graphics`, `lang`, `Sound` and so on inside), not the `data` folder. Pointed at `data`, every data test fails at once, which looks like an incompatible version but isn't.

The DVD and Steam releases of The Lost Chapters both work: the Steam release (app 204030, Windows depot) has the DVD's layout, and the parsers and every data test pass on it.

To get the Steam release on macOS with SteamCMD:

- If SteamCMD from Homebrew reports `Breakpad.framework` as damaged (so `steamconsole.dylib` fails to load), the cask is quarantined: clear the quarantine attribute on the cask's folder (`xattr -dr com.apple.quarantine <folder>`).
- Ask for the Windows depot, and set the install folder before logging in: `steamcmd +@sSteamCmdForcePlatformType windows +force_install_dir <folder> +login <user> +app_update 204030 validate +quit`.

### Platforms

Developed on Linux x86-64 (Vulkan). Reported working unchanged on macOS on Apple Silicon (Metal) with the Steam release, all tests passing: thanks to Romane Donnet for testing it ([#1](https://github.com/RDMillen/cullis-engine/issues/1)).

## The viewer

```sh
FABLE_DIR=/path/to/Fable cargo run --release -p viewer -- --region StartOakVale
FABLE_DIR=/path/to/Fable cargo run --release -p viewer -- --mesh MESH_OAKVALE_HOUSE_02
```

The side panel switches between regions (with the game's names), meshes, textures and the game's text, each with a search box. In a region, "Show navigation" draws the walkable area, "Walk" puts the young Hero in it, "Sound" sets the overall, voice, music and ambience volumes, and "Quest state" adds things from other quest states (the raid's aftermath, village events). Regions load in the background behind a loading screen.

| Key | Action |
| --- | --- |
| Drag, right-drag, scroll | Orbit, pan, zoom (scroll sets the camera distance while walking) |
| W A S D, Shift | Walk, run |
| E | Talk to someone, go on with what they say, or open a door |
| F | Kick |
| Y, N | Answer a question |

The Prologue starts in Oakvale (`StartOakVale`): talk to the Hero's father near the house at the top of the village. Its expected behaviour, and the values not yet checked against the game, are in [docs/quests/prologue-birthday.md](docs/quests/prologue-birthday.md).

Command-line options:

| Option | Effect |
| --- | --- |
| `--region NAME`, `--mesh NAME` | What to show |
| `--section NAME` | Show a quest state (repeatable), such as `Q__OakValeIntro_PostAttack` |
| `--camera x,y,z,distance,yaw,pitch` | Where the camera looks (and where `--walk` places the Hero) |
| `--walk`, `--walk-to x,y` | Place the Hero; walk him to a point first, along a route round walls, fences and people |
| `--talk N` | On arrival, talk to the nearest person (N times, printed, with `--screenshot`) |
| `--use` | After `--walk-to`, use the nearest door, then walk on |
| `--npc-time S` | Let the creatures move for S seconds before a screenshot |
| `--gesture ROLE` | Make the creature with this role (`NOVI_LiveFather`) start a conversation gesture, for screenshots with `--npc-time` |
| `--hour H` | Time of day for the screenshot (default 10); the window has a slider and a running clock |
| `--brightness B` | Light brightness for the screenshot (default 2.1); also a slider in the window |
| `--mouth ROLE:SHAPE` | Hold that creature's mouth in a shape (`NOVI_LiveFather:AH`), for screenshots |
| `--with-present` | Start the birthday quest with the sweets bought (to test the ending) |
| `--anim NAME --anim-time T` | Pose the Hero with an animation |
| `--nav`, `--ledges` | Draw the walkable area; print walk-grid ledges near the camera (diagnostics) |
| `--screenshot out.png --size 1280x800` | Render one frame to a file without opening a window |

To print a survey of your install's banks and regions:

```sh
FABLE_DIR=/path/to/Fable cargo run --release -p fable-data --example survey
```

## Documentation

- [docs/formats/](docs/formats/): file formats, written from observation of the data files
- [docs/quests/](docs/quests/): each quest's expected behaviour, from playthroughs
- [docs/inventory.md](docs/inventory.md) and [docs/survey.md](docs/survey.md): what the reference install contains

## Clean-room policy

No decompiled, disassembled or leaked code or symbols, and no knowledge taken from projects that rely on them. Format knowledge comes from public modding documentation and from observing data files and gameplay. No game assets are stored in this repository. Contributors should read [CLAUDE.md](CLAUDE.md), which states the rules in full.

## Licence

GPLv3 or later; see [LICENSE](LICENSE).

Fable is a trademark of Microsoft. This project is not affiliated with or endorsed by Microsoft or Lionhead Studios.
