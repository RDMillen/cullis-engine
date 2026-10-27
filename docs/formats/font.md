# Bitmap fonts

`lang/<language>/fonts.big` ([big.md](big.md)) holds the game's fonts: Windows fonts rasterised into a texture with a glyph table. Parser: `fable_data::font::Font`; all eight English fonts parse and write back byte for byte (`tests/real_data.rs`, `fonts_parse_and_write_back`).

## Banks

| Bank | Entries |
| --- | --- |
| `FONT_ENGLISH_MAIN` | the fonts, type 0: `ENG_TAHOMA_10`, `ENG_TAHOMA_12`, `ENG_ARIAL_12/16/18/24/28`, `ENG_TIMES_NEW_ROMAN_16` (whose face is Arial) |
| `STREAMING_FONT_ENGLISH_PC`, `_XBOX` | small entries of the same names (sources `eng_tahoma_10.txt`, `jpn_arial_24.txt`, …) and a 26 MB `GlyphData` (type 2): not decoded, perhaps the large character sets |

## Layout

All little-endian.

| Offset | Type | Field |
| --- | --- | --- |
| 0 | NUL-terminated | face name (`Arial`, `Tahoma`) |
| | u32 | height in pixels of a line of glyphs (`ENG_ARIAL_16`: 21; `ENG_TAHOMA_12`: 16; `ENG_ARIAL_28`: 37) |
| | u32 | weight, 400 in every font |
| | u8 | 0 in every font (perhaps italic) |
| | u32 | another height: the same, or one less (`ENG_ARIAL_28`: 36) |
| | u32, u32 | texture width and height in texels (128×128, 128×256, 256×256, 256×512) |
| | u32, u32 | first and last character (0x20, 0x7F) |
| | u32 | number of ranges (2) |
| | ranges | each: three u32s, then that many glyph records; the third u32 is the count (32 for 0x20–0x3F, then 64 for 0x40–0x7F), the first two (0, 0x20) and (1, 0) in every font, meaning unknown |
| | 22 bytes | not decoded: u16 0x12, u16 texel bytes ÷ 65536, …, at bytes 16–19 the texture's width and height again as u16s, ending `20 28` |
| end − w×h×4 | texels | the texture, 4 bytes a texel, rows top to bottom: the first three bytes 255 where the glyph covers the texel at all (else 0), the fourth the anti-aliased coverage |

A glyph record is 22 bytes: four f32 (left, top, right, bottom: whole texels divided by the texture's size less one, so `ENG_TAHOMA_10`'s "E" is 48/127 to 56/127 across its 128 texels and `ENG_ARIAL_16`'s rows are 22/255 high; read as fractions of the size, glyphs past the middle come out a texel off and pick up a column of their neighbour; the cell is one texel wider than the glyph), then three i16: the offset from the pen to the glyph's left edge, the glyph's width, and the advance, in pixels. Characters follow on from the first character in record order.

The last row of `ENG_ARIAL_18` (`z` to `~` and DEL) and DEL in `ENG_ARIAL_12` reach 7–8% past the texture's bottom edge: the glyphs were set in more rows than the texture holds, so their bottoms are cut off in the data.

`fable_data::font::Font::rgba` gives the texture as white with the coverage as alpha.

`engine_ui::text::BitmapFont` draws text in these fonts; the HUD's subtitles use `ENG_ARIAL_16` and its hints `ENG_TAHOMA_10`.
