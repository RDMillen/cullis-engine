//! Bitmap fonts: entries of the `FONT_ENGLISH_MAIN` bank of
//! `lang/<language>/fonts.big` (`ENG_ARIAL_16`, `ENG_TAHOMA_12`, …), Windows
//! fonts rasterised into one texture with a table of glyphs. See
//! `docs/formats/font.md`.

use crate::reader::Reader;
use crate::{Error, Result};

/// One character's place in the texture and how it is set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub ch: char,
    /// Left, top, right, bottom, as fractions of the texture's size. The cell
    /// is a texel wider than `width`.
    pub uv: [f32; 4],
    /// Pixels from the pen to the glyph's left edge.
    pub offset: i16,
    /// The glyph's width in pixels.
    pub width: i16,
    /// Pixels the pen moves on.
    pub advance: i16,
}

/// A run of consecutive characters, with its 12-byte header.
#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    /// The header's first two `u32`s: (0, 0x20) before 0x20–0x3F and (1, 0)
    /// before 0x40–0x7F in every font looked at; meaning not known.
    pub unknown_header: [u32; 2],
    pub glyphs: Vec<Glyph>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The face (`Arial`, `Tahoma`; `ENG_TIMES_NEW_ROMAN_16` is Arial too).
    pub name: String,
    /// Pixel height of a line of glyphs (21 for `ENG_ARIAL_16`).
    pub height: u32,
    /// Its weight, 400 (normal) in every font.
    pub weight: u32,
    /// A byte after the weight, 0 in every font (perhaps italic).
    pub unknown_flag: u8,
    /// Another height: equal to `height`, or one less (`ENG_ARIAL_28`: 37, 36).
    pub unknown_height: u32,
    /// The texture's size in texels.
    pub texture_size: [u32; 2],
    /// The first and last characters (0x20 and 0x7F).
    pub first: u32,
    pub last: u32,
    pub ranges: Vec<Range>,
    /// The bytes between the glyph table and the texture (22 in every font
    /// looked at; not decoded).
    pub unknown_trailer: Vec<u8>,
    /// The texture, 4 bytes a texel row by row: the first three 255 where the
    /// glyph covers the texel at all, the fourth its anti-aliased coverage.
    pub texels: Vec<u8>,
}

/// Bytes in a glyph record.
const GLYPH_SIZE: usize = 22;

impl Font {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let name = r.c_string()?;
        let height = r.u32()?;
        let weight = r.u32()?;
        let unknown_flag = r.array::<1>()?[0];
        let unknown_height = r.u32()?;
        let texture_size = [r.u32()?, r.u32()?];
        let first = r.u32()?;
        let last = r.u32()?;
        let range_count = r.u32()?;
        let texel_bytes = (texture_size[0] as usize)
            .checked_mul(texture_size[1] as usize)
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= data.len())
            .ok_or_else(|| Error::Invalid {
                offset: r.pos(),
                reason: format!("texture {texture_size:?} does not fit"),
            })?;
        let mut ranges = Vec::new();
        let mut ch = first;
        for _ in 0..range_count {
            let unknown_header = [r.u32()?, r.u32()?];
            let count = r.u32()?;
            let mut glyphs = Vec::new();
            for _ in 0..count {
                let b = r.bytes(GLYPH_SIZE)?;
                let f = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
                let s = |i: usize| i16::from_le_bytes(b[i..i + 2].try_into().unwrap());
                glyphs.push(Glyph {
                    ch: char::from_u32(ch).unwrap_or('\u{fffd}'),
                    uv: [f(0), f(4), f(8), f(12)],
                    offset: s(16),
                    width: s(18),
                    advance: s(20),
                });
                ch += 1;
            }
            ranges.push(Range {
                unknown_header,
                glyphs,
            });
        }
        let texels_at = data.len() - texel_bytes;
        if texels_at < r.pos() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: "glyph table runs into the texture".into(),
            });
        }
        Ok(Self {
            name,
            height,
            weight,
            unknown_flag,
            unknown_height,
            texture_size,
            first,
            last,
            ranges,
            unknown_trailer: data[r.pos()..texels_at].to_vec(),
            texels: data[texels_at..].to_vec(),
        })
    }

    /// The font as stored.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.texels.len() + 4096);
        out.extend_from_slice(self.name.as_bytes());
        out.push(0);
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&self.weight.to_le_bytes());
        out.push(self.unknown_flag);
        for v in [
            self.unknown_height,
            self.texture_size[0],
            self.texture_size[1],
            self.first,
            self.last,
            self.ranges.len() as u32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for range in &self.ranges {
            for v in [
                range.unknown_header[0],
                range.unknown_header[1],
                range.glyphs.len() as u32,
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for g in &range.glyphs {
                for v in g.uv {
                    out.extend_from_slice(&v.to_le_bytes());
                }
                for v in [g.offset, g.width, g.advance] {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        out.extend_from_slice(&self.unknown_trailer);
        out.extend_from_slice(&self.texels);
        out
    }

    /// Every glyph, in character order.
    pub fn glyphs(&self) -> impl Iterator<Item = &Glyph> {
        self.ranges.iter().flat_map(|r| &r.glyphs)
    }

    /// The glyph for `ch`, if the font has it.
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        let i = (ch as u32).checked_sub(self.first)? as usize;
        self.glyphs().nth(i).filter(|g| g.ch == ch)
    }

    /// The texture as white RGBA, alpha the glyphs' coverage.
    pub fn rgba(&self) -> Vec<u8> {
        self.texels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|t| [255, 255, 255, t[3]])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font of two glyphs ("A", "B") in two ranges on a 2x1 texture.
    fn tiny() -> Vec<u8> {
        let mut b = b"Arial\0".to_vec();
        for v in [21u32, 400] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        for v in [21u32, 2, 1, 0x41, 0x42, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for (k, (header, uv)) in [
            ([0u32, 0x41], [0.0f32, 0.0, 0.5, 1.0]),
            ([1, 0], [0.5, 0.0, 1.0, 1.0]),
        ]
        .iter()
        .enumerate()
        {
            for v in [header[0], header[1], 1] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in uv {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in [k as i16, 1, 2] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(&[0x12, 0, 2, 0]);
        b.extend_from_slice(&[255, 255, 255, 200, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn parses_glyphs_and_texture_and_writes_them_back() {
        let data = tiny();
        let font = Font::parse(&data).unwrap();
        assert_eq!((font.name.as_str(), font.height), ("Arial", 21));
        assert_eq!(font.texture_size, [2, 1]);
        let b = font.glyph('B').unwrap();
        assert_eq!((b.uv, b.offset, b.advance), ([0.5, 0.0, 1.0, 1.0], 1, 2));
        assert_eq!(font.glyph('C'), None);
        assert_eq!(font.unknown_trailer, [0x12, 0, 2, 0]);
        assert_eq!(font.rgba(), [255, 255, 255, 200, 255, 255, 255, 0]);
        assert_eq!(font.to_bytes(), data);
    }

    #[test]
    fn rejects_a_texture_larger_than_the_file() {
        let mut data = tiny();
        // Texture height 1000.
        let at = b"Arial\0".len() + 4 + 4 + 1 + 4 + 4;
        data[at..at + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert!(Font::parse(&data).is_err());
    }
}
