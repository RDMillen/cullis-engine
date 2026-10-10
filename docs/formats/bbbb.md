# BBBB bank files

`Levels/FinalAlbion.wad` and `Levels/FinalAlbion_RT.stb`. Parser: `fable_data::bbbb`.

## Header (32 bytes, then zero padding to the first block)

| Offset | Field | Wad | Stb | Notes |
| --- | --- | --- | --- | --- |
| 0x00 | magic | `BBBB` | `BBBB` | |
| 0x04 | version | `0x73` | `0x73` | |
| 0x08 | unknown_a | 2 | `0xae` | |
| 0x0c | unknown_b | 1 | 1 | |
| 0x10 | block_size | `0x800` | `0x800` | Entry data starts on multiples of this |
| 0x14 | entry_count_a | 796 | 425 | Equals the index count in the wad; one more in the stb |
| 0x18 | entry_count_b | 796 | 424 | Equals the index count in both |
| 0x1c | index_offset | | | The index runs from here to the end of the file |

The index format is in [bank-index.md](bank-index.md). Entry data sits between the first block and the index, block-aligned and non-overlapping.

## Contents

- **Wad**: one `.lev` and one `.tng` per map, named `Data\Levels\FinalAlbion\<Map>.lev|tng`. Every Lev has a Tng.
- **Stb**: one entry per Lev with the same name as in the wad (398 entries; on average about 3.8 times the Lev's size), 25 entries named `__ENGINE_SEA_STATIC_MAP_BANK_FILE__<Region>`, and one `__STATIC_MAP_COMMON_HEADER__`. See [stb.md](stb.md).
