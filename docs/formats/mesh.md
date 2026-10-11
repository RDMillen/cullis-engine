# Meshes

Entries of type 1 (static meshes) in the `MBANK_ALLMESHES` bank of `graphics/graphics.big` ([big.md](big.md)). Parser: `fable_data::mesh`. All 2,583 static meshes parse (10.4 million triangles); 29 of them use the rigged layout below. Every texture id their materials name exists in `GBANK_MAIN_PC` (`tests/real_data.rs`). Type 5 entries are skinned meshes (characters and creatures), all decoded: see [Skinned meshes](#skinned-meshes-type-5). Types 6, 7 and 9 are animations ([anim.md](anim.md)). Types 2 (grass blades), 3 (`[PHYSICS]` collision meshes) and 4 are not decoded.

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

Positions use the same axes as the terrain (z up) but are in hundredths of a world unit: a chair is 119 units tall, a map is 96 world units wide. Placed meshes are scaled by 0.01.

**Orientation.** A mesh's front is its local -y. When a Tng thing gives `forward` and `up`, local -y maps to `forward`, local -x to `right` (forward x up) and z to `up`. Evidence: Oakvale's trader bridge (`MESH_VILLAGE_TOWN_TRADERBRIDGE_01`, local y from -980 to 2309) only rests on both of its 21 m plateaus this way round; the other way, one end hangs over the 12 m dip and the other runs into the hillside. The first guess (front = +y) put house doors facing the cliffs; Ross checked it against the game.

## Rigged static meshes

29 type 1 entries (the charred villagers of the burnt Oakvale, statues such as `MESH_OBJECT_STATUE_BEAR`, summoners, `MESH_BS_SLUM_04`) are laid out like skinned meshes: each level of detail starts at the beginning of the entry, with the name, a flag byte (0 here, 1 on skinned meshes) and a skeleton ([below](#skeleton)), then materials and chunks. Their chunks are static (15-byte sub-mesh records naming their material, no vertex weights). `Mesh::parse_static` tries the static layout first and falls back to this one.

## Skinned meshes (type 5)

504 entries: characters, creatures and the Hero's outfit pieces (`MESH_YOUNGHERO_02`, `MESH_HERO_APPRENTICE_SHIRT_01`, `MESH_SEAGULL_01`, …). The info block has the static layout, so texture ids can be read the same way (`MESH_YOUNGHERO_02`: `YOUNGHERO_SKIN_FACE`, `YOUNGHERO_HAIR`, `YOUNGHERO_SKIN_TORSO`, `HERO_SKIN_MOUTH`). All 504 (1431 levels of detail) parse: `Mesh::parse_skinned`, real-data test `skinned_meshes_parse_with_consistent_skeletons`.

### Levels of detail

Unlike static meshes, the levels start at the beginning of the entry, back to back, with the info block's sizes; whatever follows the last one (starting with the name again) is not decoded. Each level is self-contained:

```
name             NUL-terminated
u8 1
40 bytes         bounding sphere and box, as in the info block
skeleton         below
materials        as for static meshes
chunks           skinned layout, up to the end of the level
```

### Skeleton

A list whose count is 0 is not stored at all (not even a stream header).

| Field | Type | Notes |
|---|---|---|
| helper count | u16 | 12 on the young Hero; 1 on the seagull |
| dummy count | u16 | 6 on the young Hero |
| helper names length | u32 | |
| | u16 | 0 |
| helpers | stream | 20 bytes each; not decoded |
| dummies | stream | 56 bytes each; not decoded |
| helper names | stream | NUL-separated: `focus_camera_01`, `weapon_pos_head`, `left_foot`, `EYE_SET_L`, `MOUTH_FOCUS`, `WEAPON`, `SCABBARD_0`, … |
| | 2 x u32 | 4, 4 on the young Hero; 5, 5 on the sister |
| bone count | u32 | 76 on the young Hero |
| bone names length | u32 | |
| | 5 bytes | `01 00 00 07 00` on both Heroes |
| name offsets | bone count x u16 | into the bone names |
| bone names | stream | `Scene Root`, `Movement_dummy`, `Sub_movement_dummy`, `Bip01`, `Bip01 Pelvis`, … |
| hierarchy | stream | 60 bytes per bone, below |
| rest poses | stream | 48 bytes per bone, below |
| inverse bind matrices | stream | 64 bytes per bone: a row-major 4x4 matrix (last row 0, 0, 0, 1), model space to bone space, acting on column vectors |
| | 48 bytes | identity 3x4 matrix on the young Hero |

Hierarchy record: u32 (unknown, maybe a name hash), i32 parent (-1 for the root; parents come first), u32 (unknown, perhaps a child count), then the inverse bind matrix again as 4 rows of 3 floats (the transpose of the 64-byte form's top 3 rows).

Rest pose record: quaternion (x, y, z, w), translation (3 floats), a float (0), scale (3 floats), a float (0). The quaternion is the inverse rotation: building each bone's local matrix as translate · rotate(conjugate) · scale and composing from the root reproduces every inverse bind matrix to within 4e-6 over all 504 meshes.

### Skinned chunk

As a static chunk except:

- The first word is the material index for all of the chunk's sub-meshes.
- The two words before the vertex count are (unknown, sub-mesh count) instead of (sub-mesh count, 0).
- Sub-mesh records are 19 bytes plus a bone list: u32 primitive count, u32 first index, 3 flag bytes (first = 1 for strips), u32 vertex count, u16 (3), u8 (1), u8 bone count, then that many skeleton bone indices.
- Every vertex format gains 8 bytes right after the position: 4 bone indices (stored x3, so `0x21` is the sub-mesh's 11th bone list entry) and 4 weights summing to 255. The normal and texture coordinates follow. Seen: format 4 (stride 20), 20 (28), 6 (28), 22 (36).

Vertices are stored in the bind pose, in model space, so a mesh can be drawn without its skeleton. The young Hero stands 201 units tall, like the father (195) and the rival Heroes (about 200), though the Prologue Hero is a child; his scale in the game is not decoded (the viewer uses an estimate of 0.65).

### After each chunk (static and skinned)

A u32 count of physics blocks (0 on most chunks, 1 on the young Hero's hair, 4 on `MESH_BS_WASHINGLINE_05`), each two u32s (0 then 0–6 on the hair; 2 then 3 on the washing line), a length, and a stream of that length. On the young Hero's hair the block is a count (333), a length, then records pairing two vertices with a rest length and a stiffness (0.1): probably hair or cloth physics. Not decoded further.
