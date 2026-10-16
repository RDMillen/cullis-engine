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
| after the grid | Sound map, fixed block, sections (below) |

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
| 15–20 | Not decoded: flags. In StartOakValeWest bytes 15, 16, 19 and 20 are 0 or 1 and bytes 17 and 18 are always 0 (`examples/lev_bytes.rs` pictures them), so the grid holds no baked lighting or shadow |

The 2048 scale is confirmed against the stb: across all 10,257 stb tiles, 10,253 tile bounding boxes contain all of the Lev heights under them, and 10,230 tile minimums equal the lowest Lev height exactly (within 0.01). The tiles whose maximum is higher than the terrain are mostly shorelines, where a water surface at about height 3 is part of the tile.

## After the grid (`Lev::parse_tail`)

All 398 maps parse this far (`tests/real_data.rs`).

```
sound map       (width / 4) x (height / 4) records, row by row: u32 11, then 7 bytes
u32 length      33,801 in every map
fixed block     that many bytes; starts with 1, then mostly zeros; not decoded
u32 count
count x { u32-length name, u32 offset }      sections, e.g. NULL, Q_NewOakValeIntro_PreAttack
sections        each runs from its offset to the next section's offset (the last to the end of the file)
```

**Sound map records**: `u32` 11, then 7 bytes ending in a value with only a few distinct values per map (9, 21 and 37 in StartOakValeWest; 33 all over the Arena; 3 in the prison cells). The value counts from 1 into the map's sound theme names: in StartOakValeWest 9 is `SOUND_THEME_COUNTRY`, 21 `SOUND_THEME_OCEAN` and 37 `SOUND_THEME_SHORE`, where the village, the sea and the shore are. Blocks run row by row from the map's origin, 4 units each. See `lug.md` for what the themes play.

**Sections**: one per map state. `NULL` is the default; others are named after quest states that change the map (StartOakValeWest has `Q_NewOakValeIntro_PreAttack` and `Q__OakValeIntro_PostAttack` for the Prologue raid; the Arena has `Q_Arena`). Filler and background maps have none: 233 sections over 398 maps. Each section starts:

```
u32 end offset    equals the next section's offset
u32 levels        8 in every section
f32 width, f32 height    the map's size
u32, u32          counts? (StartOakValeWest: 10, 172; Arena: 4, 0)
records           variable length, unaligned; ids with positions at half-size steps
                  (8, 24), (12, 28), (14, 30), (15, 29.5), ...: a subdivision, probably navigation
```
