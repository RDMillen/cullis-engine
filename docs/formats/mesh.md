# Meshes

Entries of type 1 (static meshes) in the `MBANK_ALLMESHES` bank of `graphics/graphics.big` ([big.md](big.md)). Parser: `fable_data::mesh`. 2,519 of the 2,583 static meshes parse completely (10.1 million triangles); the rest have bones (skinned), which are not decoded yet. Every texture id their materials name exists in `GBANK_MAIN_PC` (`tests/real_data.rs`). Other entry types (2, 4, 5: probably skinned and character meshes; 3, 6, 7, 9: short info blocks, probably animations) are not decoded.

All integers are little-endian; many fields are unaligned.

## Info block

| Field | Type | Notes |
| --- | --- | --- |
| unknown | u32 | 0 |
| bounding sphere | 4 x f32 | centre x, y, z, radius |
| bounding box | 6 x f32 | min x, y, z, max x, y, z |
| LOD count | u32 | 1–3 typically |
| LOD sizes | count x u32 | byte length of each level of detail, most detailed first |
| unknown | u32 | 0 |
| LOD switches | (count - 1) x f32 | e.g. 0.025, 0.04 |
| texture count | u32 | |
| texture ids | count x u32 | `GBANK_MAIN_PC` entry ids |

This layout accounts for every byte of the info block in all 2,583 static meshes.

## Entry data

```
NUL-terminated mesh name
u8, then the bounding sphere and box again (10 x f32)
... header: helper points (e.g. TARGET_CURSOR_POINT), bones, more (not decoded)
u32, then a 3x4 transform (identity rotation, zero translation in the meshes seen)
LOD blocks, back to back, filling the end of the entry
```

The LOD blocks are the last `sum(LOD sizes)` bytes, so the header does not need to be decoded to reach them. The `u32` before the transform matches the material count in simple meshes but not always (the stump chair has 3 materials and 4 there).

## LOD block

```
materials        until the next record is not a material
chunk            one or more
u32 0            after each chunk
trailer          about 145–152 bytes starting with the mesh name again (not decoded)
```

The parser reads chunks until the bytes after a chunk start with the mesh name.

### Material (name + 33 bytes)

| Field | Type | Notes |
| --- | --- | --- |
| id | u32 | small; 0 or the material's index |
| name | NUL-terminated | 3ds Max material names, e.g. `7 - Default`, `exterior_stone`, `DegenerateTriangles` |
| unknown | u32 | 0 |
| texture | u32 | main texture id, 0 for none |
| second texture | u32 | 0 for none; role unknown |
| rest | 21 bytes | not decoded |

### Chunk

| Field | Type | Notes |
| --- | --- | --- |
| node | u32 | 0 for the first chunk; other values (e.g. 3) for later ones |
| unknown | u32 | 0 |
| bounding sphere | 4 x f32 | |
| unknown | f32 | small, e.g. 0.0012–0.11 |
| sub-mesh count | u32 | |
| unknown | u32 | 0 |
| vertex count | u32 | |
| triangle count | u32 | |
| index count | u32 | |
| vertex format | u32 | flags, see below |
| sub-mesh count | u32 | again |
| unknown | u32 | 0 |
| sub-meshes | count x 15 bytes | see below |
| position scale | 3 x f32 | half size of the chunk's box; 1, 1, 1 for float positions |
| unknown | f32 | 1.0 |
| position centre | 3 x f32 | centre of the chunk's box; 0, 0, 0 for float positions |
| unknown | u32 | 0 |
| stride | u32 | bytes per vertex |
| unknown | u32 | 0 |
| vertices | stream | `vertex count x stride` bytes |
| indices | stream | `index count x 2` bytes, u16 |

Sub-mesh (15 bytes): `u32` primitive count, `u32` first index, `u8` strip flag (1 = triangle strip, 0 = list), 2 bytes unknown, `u32` material index. A list uses `3 x count` indices, a strip `count + 2`. The sub-meshes cover every index exactly.

A **stream** is a `u16` length (or `0xffff` and a `u32` length). A length of 0 means the bytes follow raw; otherwise an LZO1X stream of that length gives all but the last 3 bytes, and those 3 follow raw. Textures use the same scheme ([texture.md](texture.md)).

### Vertex formats

| Format | Stride | Count (LOD chunks) | Layout |
| --- | --- | --- | --- |
| 4 | 12 | 7,239 | packed position, packed normal, UV |
| 6 | 20 | 443 | as 4, then 8 bytes not decoded |
| 20 | 20 | 956 | float position (3 x f32), packed normal, UV |
| 22 | 28 | 126 | as 20, then 8 bytes not decoded |

Flag 16 selects float positions; flag 2 adds 8 bytes.

- **Packed position**: one `u32`, signed 11-bit x (bits 0–10), 11-bit y (11–21), 10-bit z (22–31), each divided by its maximum (1023, 1023, 511). World position = centre + value x scale.
- **Packed normal**: the same packing, unit length.
- **UV**: two `u16`; `(value - 0x4000) / 0x800`. Checked by eye on several textured meshes.

Positions are in the same units and axes as the terrain (z up).
