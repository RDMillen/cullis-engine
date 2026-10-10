# Stb static map data

`Levels/FinalAlbion_RT.stb`, a [BBBB bank](bbbb.md). Parser: `fable_data::stb`. All integers are little-endian `u32`, floats are `f32`.

Entries:

- `__STATIC_MAP_COMMON_HEADER__`: one header per map (below).
- One entry per map, named like its Lev (`Data\Levels\FinalAlbion\<Map>.lev`).
- 25 `__ENGINE_SEA_STATIC_MAP_BANK_FILE__<Region>` entries. Not yet examined.

## Common header

```
u32 map_count                                  398, the same as the .wld
map_count x { NUL-terminated Lev path, u32 header_offset }   sorted by path
per-map headers                                at header_offset, packed back to back
```

Each per-map header runs from its offset to the next one, and is 174 bytes or more (2030 for LookoutPoint). The first words:

| Word | Meaning |
| --- | --- |
| 0 | Always 1 |
| 1, 2 | Unknown |
| 3, 4 | Map width and height in world units (e.g. 96, 128) |
| 5, 6 | Map position in world units; equal to the `.wld` `MapX`/`MapY` for every map |
| 7 | 0 so far |
| 8, 9 | Offsets inside the common header, pointing a little further into this same header |
| 10, 11 | Unknown |
| 13 | Per-map value, equal to the optional value in the map's Lev before its sound themes ([lev.md](lev.md)); 0 for `HauntedHouse_Filler_01` |

The rest is not decoded. Further in, StartOakValeEast's header contains `0x800` and `0x6d800`, the offsets of the tile table and of the section after the tiles in its map entry.

## Map entry

| Offset | Content |
| --- | --- |
| 0x000 | `u32` 1, then zero padding |
| 0x800 | Tile table |
| after the table | Compressed tile blocks, each starting on a `0x800` boundary |
| after the tiles | Another section, block-aligned (StartOakValeEast: `0x6d800` to the end, 1.6 MB). Not decoded; see below |

### Tile table

36-byte records, ended by an all-zero record:

| Field | Type | Notes |
| --- | --- | --- |
| offset | u32 | Offset of the tile block within the map entry |
| size | u32 | Block length including its 8-byte header |
| min | 3 x f32 | x, y in world units; z is the lowest height |
| max | 3 x f32 | Always exactly 16 units more than min in x and y |
| unknown | u32 | `0x00c92e01` for every tile in StartOakValeEast |

The map is cut into a grid of 16x16-unit tiles, and tiles that would be empty are left out. StartOakValeEast (96x128) has all 48 tiles; 180 of the 398 maps have gaps.

### Tile block

```
u32 decompressed_len
u32 compressed_len      always block size - 8
LZO1X stream            ends with the standard end-of-stream marker
```

Every tile in every map decompresses to exactly `decompressed_len` bytes (`tests/real_data.rs`). `fable_data::lzo` was checked against the reference LZO library on all of StartOakValeEast's tiles, with identical output.

The decompressed format is not fully decoded. It begins with a short header, then 15-byte vertex records. Each record holds small grid indexes and an `f32` height that equals the Lev heightmap height at that point ([lev.md](lev.md)) to about 0.004, slightly rounded. In StartOakValeEast's first tile the first four vertices are grid points (0,0), (1,0), (1,1), (0,1). So the tiles are pre-built terrain meshes made from the Lev heightmap; the rest of each record (probably normal and colour) is not decoded.

### Section after the tiles

Starts `u32 0, u16 width, u16 height` (96 and 128 for StartOakValeEast), then what looks like a tree of nodes. Each node seems to have a bounding box, the value 16 (the tile size) and offsets. Not decoded.
