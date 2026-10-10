# CullisEngine

Clean-room Rust engine for Fable: The Lost Chapters (requires the original game). Read `PLAN.md` and `docs/progress.md` at the start of every session.

## Clean-room rules (non-negotiable)

- Never paste, request, reproduce or translate decompiled, disassembled or leaked Fable code or symbols. Never run a decompiler or disassembler on game binaries.
- Work only from published format docs, community tool behaviour, the bytes of data files, and observed gameplay.
- Never read openalbion's source or docs, and never use knowledge from any project built on decompiled or leaked material. Check where a reference project's knowledge came from before using it.
- EgoCore, Fable Script Extender and jamen/fable-defs: never read their source. EgoCore may be run as a black box on our own files to check our decoders' output.
- No game assets in the repo. `tests/fixtures/` holds synthetic files only. Screenshots of the original game (from Ross playing it) are fine to look at as observed behaviour, but never commit them.

## Game data

- `$FABLE_DIR` points at the install (`/home/ross/fable_data` on the dev VM). It is read-only; never commit anything from it.
- Tests that need it are `#[ignore]`d: `FABLE_DIR=/home/ross/fable_data cargo test --release -- --ignored`.
- Test internal consistency, not sizes, offsets or counts that may differ between the DVD and Steam releases.

## Build

- Rust comes from rustup in `~/.cargo/bin` (add it to `PATH` in non-interactive shells); `rust-toolchain.toml` pins the nightly.
- Desktop: `cargo build`, `cargo test`, `cargo clippy --all-targets`.
- Survey of the install: `cargo run --release -p fable-data --example survey > docs/survey.md`.

## Code rules

- Desktop only: this repo targets x86-64 desktop with wgpu. A PS Vita port will be a separate repository that reuses these crates, so follow the portability rules:
  - No x86-specific code: no SIMD intrinsics or inline assembly without a portable fallback.
  - Parse with explicit little-endian reads, never by casting structs over bytes; never assume a pointer width.
  - `fable-data`, `engine-core`, `engine-script` and `engine-audio` must not depend on wgpu, windowing or desktop-only crates.
  - Shared engine code: no per-frame allocation, no `std::process`, no threads beyond the job system.
- Parsers keep unknown fields as `unknown_*` and document what has been observed about them.
- Round-trip formats where possible: decode, re-encode, compare bytes.
- Every new or changed format updates its page in `docs/formats/`.
- One task per session, scoped to one crate. Append to `docs/progress.md` at the end of every session.
