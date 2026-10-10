# CullisEngine

An open-source, clean-room engine for **Fable: The Lost Chapters**, written in Rust. It targets desktop first and a modded PS Vita second.

**Requires the original game.** CullisEngine ships no game data. It reads files from your own legally owned PC install.

## Status

Early. Nothing is playable or drawn on screen yet. So far the `fable-data` crate reads:

- the bank and archive containers (`.wad`, `.stb`, `.big`) and their indexes
- the world layout (`.wld`): 398 maps and 141 regions
- LZO1X-compressed data

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
