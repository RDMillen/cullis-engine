# Lev map files

One per map, stored in `Levels/FinalAlbion.wad` (see [bbbb.md](bbbb.md)). Parser: `fable_data::lev`. Integers are little-endian and often unaligned.

The start of the file has a fixed layout in all 398 maps:

| Offset | Content |
| --- | --- |
| 0x00 | `u32` 0x19 (all maps) |
| 0x0c–0x18 | Per-map values, not decoded |
| 0x19 | `u32` 0x10822 (all maps) |
| 0x26 | `u32` width, `u32` height: map size in world units. Equals the stb header size for every map |
| 0xaf | 255 texture theme slots of 132 bytes: `u32` key, 128-byte NUL-padded name (an `ENGINE_THEME` definition, e.g. `GROUND_GRASS_GREEN`, `SEA_OAKVALE_0`). Grid points refer to slots by position. The key's meaning is unknown: it is not a usage count, and the same value names different themes in different maps, even within a region. Some used slots have no name (all of StartOakVale_Filler_05's points use one); how the game resolves them is not known |
| 0x842f | `u32` 3 in the maps checked |
| 0x8433 | `u32` sound entry count (see below) |
| 0x10837 | Optional `u32` value, then `count - 1` length-prefixed sound theme names (`SOUND_THEME_COUNTRY`, …) |
| after the names | Heightmap grid |
| after the grid | Not decoded (probably navigation and more) |

The optional `u32` equals word 13 of the map's stb header. It is present in every map except `HauntedHouse_Filler_01`, whose stb header has 0 there and whose Lev starts the names directly at 0x10837. In both cases there are `count - 1` names. The parser tells the cases apart by whether 0x10837 holds a printable length-prefixed string.

## Heightmap grid

`(width + 1) * (height + 1)` points of 21 bytes, row by row, `width + 1` points per row, starting at the map's origin (its `.wld` position). One point per world unit.

| Byte | Content |
| --- | --- |
| 0–3 | `u32` 0x15 (21, the record length) |
| 4 | 7 |
| 5–8 | `f32` height; multiply by 2048 for world units |
| 9 | 0 |
| 10, 11, 12 | Theme slots. Every point in every map uses a listed slot |
| 13, 14 | Weights of the first two themes; they add up to 255 at 99.5% of points |
| 15–20 | Not decoded. Byte 15 is 0 or 1, byte 16 is always 1 |

The 2048 scale is confirmed against the stb: across all 10,257 stb tiles, 10,253 tile bounding boxes contain all of the Lev heights under them, and 10,230 tile minimums equal the lowest Lev height exactly (within 0.01). The tiles whose maximum is higher than the terrain are mostly shorelines, where a water surface at about height 3 is part of the tile.
