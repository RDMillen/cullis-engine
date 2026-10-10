# BIGB archives

Every `.big` file in the install. Parser: `fable_data::big`. `banks_dvd.ini` maps bank names to archive files and to the header files in `Defs/RetailHeaders/`.

## Header (16 bytes)

| Offset | Field | Notes |
| --- | --- | --- |
| 0x00 | magic | `BIGB` |
| 0x04 | version | 100 in every file seen |
| 0x08 | bank_table_offset | |
| 0x0c | bank_table_len | Offset plus length equals the file size in every file seen |

## Bank table

```
u32 bank_count
bank_count x {
  NUL-terminated name    e.g. MBANK_ALLMESHES, GBANK_MAIN_PC
  u32 unknown            same for every bank in one file (0xcd in graphics.big, 0x7a in textures.big)
  u32 entry_count
  u32 index_offset
  u32 index_len
  u32 block_size         0x800
}
```

Each bank's index, at `index_offset`, uses the shared [bank index](bank-index.md) format and holds `entry_count` entries.

## Banks in the reference install

See [../survey.md](../survey.md) for the full list with sizes and type counts. The main ones:

- `graphics/graphics.big`: `MBANK_ALLMESHES` (8112 entries, 8 types) and `MBANK_ENGINE`.
- `graphics/pc/textures.big`: `GBANK_MAIN_PC` (6290 entries) and `GBANK_GUI_PC`.
- `lang/English/text.big`, `dialogue.big` (lipsync), `fonts.big`; `graphics/pc/frontend.big`; `Misc/pc/effects.big`; `shaders/pc/shaders.big`.

Mesh entry names are symbols (`MESH_OBJECT_STATUE_BEAR`); texture entry names are bracketed upper-case source paths. What each entry type number means is not yet known.
