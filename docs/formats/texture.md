# Textures

Entries of the `GBANK_*` banks in `graphics/pc/textures.big` and `graphics/pc/frontend.big` ([big.md](big.md)). Parser: `fable_data::texture`; DXT decoding: `fable_data::dxt`. All 6,717 textures in the reference install unpack with no bytes left over, except one in an unsupported format (`tests/real_data.rs`).

## Info block (34 bytes)

| Offset | Type | Field | Notes |
| --- | --- | --- | --- |
| 0 | u16 | width | |
| 2 | u16 | height | |
| 4 | u16 | depth | 0, or the slice count of a volume texture (8 for `WEATHER_RAIN_PC`) |
| 6 | u16 | frame width | Equal to width except for sprite sheets and frame sequences |
| 8 | u16 | frame height | |
| 10 | u16 | frame count | |
| 12 | u8 | format | See below |
| 16 | u8 | alpha flag | 0 opaque, 1 alpha, 3 seen on the DXT5 sky |
| 17 | u8 | mip count | Levels stored per image |
| 20 | u32 | top level length | Bytes of the top level as stored |
| 24 | u32 | compressed length | Bytes the compressed top level takes; 0 when nothing is compressed |
| 29 | u8 | bits per pixel | 4, 8, 16 or 32 |

| Format | Bits | Count | Meaning |
| --- | --- | --- | --- |
| `0x1f` | 4 | 3,703 | DXT1 |
| `0x20` | 8 | 2,574 | DXT3 (confirmed by eye: smooth alpha edges on `SEAGULL_WING_32`) |
| `0x23` | 8 | 4 | DXT5 (the atmospheric skies) |
| `0x01` | 32 | 8 | Uncompressed, bytes B, G, R, A |
| `0x18` | 16 | 1 | Not supported yet |

## Layout

The bank entry type decides the layout:

- **Type 1 (frame sequences, e.g. `GRAPHIC_ANIM_CAMPFIRE`)**: `frame count` images of `frame width` x `frame height`, one after another, each with its own mip chain, all uncompressed.
- **Every other type**: one image of `width` x `height`. Sprite sheets (type 5, e.g. `SPRITE_DUST_01`) keep their frames as tiles inside it; a frame count on other types is metadata only.

Each image is a mip chain, largest level first:

- Levels are stored padded to power-of-two sizes, with the image in the top-left corner (a 20x20 icon is stored as 32x32).
- A volume texture's level holds all its slices one after another.
- Levels of at least 4x4 take whole 4x4 blocks. Smaller levels of a block format take their bits-per-pixel size (a 2x2 DXT5 level is 4 bytes, a 1x1 is 1 byte).

When `compressed length` is non-zero, the first image's top level is stored compressed:

```
u16 stream_len           or 0xffff followed by u32 stream_len
LZO1X stream             stream_len bytes, decompressing to (top level length - 3) bytes
3 bytes                  the last 3 bytes of the top level, raw
```

`compressed length` = header + `stream_len` + 3. A `stream_len` of 0 means the level did not compress and follows raw. The rest of the chain follows uncompressed.

## Exporting

`cargo run --release -p fable-data --example texture_png -- ARCHIVE NAME_PART OUT.png [LEVEL] [IMAGE]` writes one level as PNG.
