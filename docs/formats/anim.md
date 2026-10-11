# Animations (`MBANK_ALLMESHES` types 6, 7, 9)

Entries of types 6 (3,272 full-body animations: `ANIM_HERO_WALK`, `ANIM_BIPED_CHICKEN_WALK`, …), 7 (56 short ones: head turns, facial) and 9 (107 partial ones: upper body, looks) in the `MBANK_ALLMESHES` bank of `graphics/graphics.big` ([big.md](big.md)). Parser: `fable_data::anim`. All 3,435 parse (210,743 tracks); all but 5 of 4.7 million rotation keys are unit quaternions (`tests/real_data.rs`).

## Info block (24 bytes)

| Field | Type | Notes |
|---|---|---|
| duration | f32 | frame count / 30 seconds |
| last frame time | f32 | (frame count - 1) / 30 |
| | 16 bytes | zero in every entry looked at |

## Entry data

A u32 unpacked length, then one LZO1X stream (not the length-prefixed stream of meshes and textures) that unpacks to exactly that many bytes.

Unpacked:

```
">>>>"
"3DAF"  u32 version (100)  NUL-terminated copyright string
chunks: 4-byte tag, u32 length, body
a few bytes too short to be a chunk (4 zero bytes seen)
```

Top-level chunk: `ANRT` (u8 1, f32 duration, then chunks). Inside it, `AOBJ`: NUL-terminated object name (`HeroUnclothed`, `Crypt_door`, `Power_Generator_Wet_01`), u32 0, then chunks: one `XSEQ` per bone, then `HLPR`, which holds `MVEC` and `XALO` (and more on some animations).

`MVEC` starts with 3 floats: how far the animation carries the character in one loop, in mesh units along the way meshes face (-y): (0, -266.67, 0) for `ANIM_HERO_WALK` (1 s loop), (0, -390, 0) for `ANIM_HERO_RUN` (0.6 s), (0, 0, 0) for idles. The game evidently moves characters at that speed: the cycles themselves stay in place (`Bip01` sways and bobs but does not drift). Longer `MVEC` bodies carry more data, not decoded. `XALO` (8 bytes, `01 00 00 00` then a u32) is not decoded.

## `XSEQ`: one bone's track

| Field | Type | Notes |
|---|---|---|
| | u32 | 0x7ada in every track looked at |
| parent | i32 | index of the parent track; -1 for the root |
| name | NUL-terminated | the skeleton's bone name (`Bip01 L UpperArm`) |
| | u8 | 2 on `Movement` in some animations, else 0 |
| fps | f32 | 30 |
| frames | u32 | |
| layout | 4 x u8 | has rotations (0/1); rotation layout (1 one key, 2 one key per frame, 3 a palette of keys with per-frame indices); position layout (0 none, or as for rotations); 0 |
| | f32 | 1e-4 to 1e-2; perhaps the compression tolerance |
| | 2 x u16 | 0x0100, 0x3800 |
| rotation keys | u16 count, count x 4 f32 | quaternions (x, y, z, w) |
| rotation frames | u16 count, count x u8 | per frame, an index into the rotation keys; empty unless they are a palette |
| position keys | u16 count, count x 3 i16 | below |
| position frames | u16 count, count x u8 | as for rotations |

A frame's key: the palette entry its index names; else the only key; else the key for that frame. The layout byte agrees with the counts in every track.

Rotations use the convention of the skeleton's rest pose ([mesh.md](mesh.md#skeleton)): a bone that does not move stores exactly its rest quaternion.

Positions are stored divided by their largest component (one component is always ±32767): the direction of the bone's offset from its parent. The length comes from the skeleton the animation is played on: the largest component of the bone's rest translation. Checked on `ANIM_HERO_UNARMED_COMBAT_IDLE_1` against `MESH_YOUNGHERO_02`'s skeleton: every constant position equals the rest translation divided by its largest component. Animations made for the adult Hero (`HeroUnclothed`) therefore fit the young Hero's smaller skeleton.

Played this way (frames blended linearly, quaternions normalised), `ANIM_HERO_IDLE`, `ANIM_HERO_WALK` and `ANIM_HERO_RUN` pose `MESH_YOUNGHERO_02` plausibly in the viewer.

## Not decoded

- Which animation the game picks for what: `CAppearanceDef` definitions list animation slots (`ST_IDLE_SUBTLE`, `CS_SHIFT_WEIGHT3`, …) with values such as `0x49c95` that are not bank entry ids.
- `HLPR` contents beyond `MVEC`'s first 3 floats, `XALO`.
