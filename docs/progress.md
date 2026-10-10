# Progress log

Newest first. One entry per session: what changed, what was learned, what is next.

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
