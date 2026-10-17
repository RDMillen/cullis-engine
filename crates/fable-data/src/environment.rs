//! Lighting by time of day: environment themes (`ENVIRONMENT_THEME_DAY`
//! definitions such as `ENVIRONMENT_OAKVALE`) and the colour table they index,
//! `LightingTable/lighting_colours.tga`. See `docs/formats/environment.md`.
//!
//! A theme is a list of keyframes, each at an hour of the day with sky textures
//! and other settings, plus a column of the colour table: column
//! `table_index + k` holds keyframe `k`'s colours, one per row. What each row
//! lights is not named in the data; [`rows`] lists what has been observed.

use std::collections::HashMap;

use crate::reader::Reader;
use crate::{Error, Result};

/// Field hashes, named by hashing guesses ([`crate::defs::name_hash`]).
pub mod fields {
    /// Region: its outdoor environment theme (a definition index).
    pub const ENVIRONMENT_THEME: u32 = 0x4a59_983b;
    /// Region: its indoor environment theme.
    pub const INTERNAL_ENVIRONMENT_THEME: u32 = 0x98e5_785a;
    /// Region, name unknown: 2 in `REGION_HERO_GUILD_COMPLEX_INSIDE`, 1 in every
    /// other region; taken to mean the region is lit by its internal theme.
    pub const REGION_LIGHTING: u32 = 0x2794_5c1b;
    /// Theme: the number of keyframes.
    pub const TIME: u32 = 0xeee2_e867;
    /// Keyframe: its hour (0..24), the first field of each.
    pub const TIME_OF_DAY: u32 = 0xb400_c903;
    /// Keyframe: the sky textures (`GBANK_MAIN_PC` ids), e.g.
    /// `GRAPHIC_ATMOSPHERIC_SKY_MIDNIGHT`.
    pub const SKY_TEXTURE_0: u32 = 0x425d_252f;
    pub const SKY_TEXTURE_1: u32 = 0x355a_15b9;
    /// Theme, name unknown: the first column of the colour table.
    pub const TABLE_INDEX: u32 = 0x12a7_b130;
    /// Theme: how high the sun and the moon rise (0.2 and 0.9 in Oakvale).
    pub const SUN_HEIGHT: u32 = 0x37c5_9465;
    pub const MOON_HEIGHT: u32 = 0x79ff_0650;
    /// The one keyframe field whose value is a single byte (0 or 1; 1 at the
    /// night keyframes of Oakvale and the Guild).
    pub const ONE_BYTE: u32 = 0x8632_d8f5;
}

/// Rows of the colour table, by what they look like they light. Guesses from
/// how the colours change through the day (and which rows indoor themes fill),
/// to be compared with the game.
pub mod rows {
    /// Warm by day, grey at night; filled indoors too. Used as the ambient light
    /// indoors (neutral grey in `ENVIRONMENT_INTERNALS`).
    pub const AMBIENT: usize = 0;
    /// Outdoors: blue-violet by day (46405f at noon), dark blue at night. The
    /// usual cool counterpart to a warm sun; used as the ambient light
    /// outdoors (row 0 tinted everything orange-brown). Very dark brown in
    /// indoor themes, which use row 0 instead.
    pub const SKY_FILL: usize = 1;
    /// Night 14181e, noon white, evening orange (f4ce9d): the sun's light.
    pub const SUN: usize = 8;
    /// Dark blue at night, hazy at noon, orange-brown in the evening: the sky
    /// near the horizon.
    pub const HORIZON: usize = 5;
    /// Blue at noon (6186b4), pale at night: the sky overhead.
    pub const ZENITH: usize = 9;
    /// Near white in every theme, indoor ones too (cccccc at midnight to
    /// e0e0e0 at noon in `ENVIRONMENT_INTERNALS`). Used indoors as the light
    /// coming in through windows and doors.
    pub const KEY: usize = 7;
}

/// One keyframe of a theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// Hour of the day, 0..24.
    pub hour: f32,
    /// Every field after the hour, in order, as (hash, value bytes); values are
    /// 4 bytes (`f32` or `u32`) except [`fields::ONE_BYTE`]'s.
    pub fields: Vec<(u32, Vec<u8>)>,
}

impl Keyframe {
    /// A field's value as a `u32`.
    pub fn u32(&self, field: u32) -> Option<u32> {
        let (_, v) = self.fields.iter().find(|(h, _)| *h == field)?;
        Some(u32::from_le_bytes(v.as_slice().try_into().ok()?))
    }

    /// A field's value as an `f32`.
    pub fn f32(&self, field: u32) -> Option<f32> {
        self.u32(field).map(f32::from_bits)
    }
}

/// An `ENVIRONMENT_THEME_DAY` definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// The record's first 5 bytes (`01 00 01 00 00`).
    pub unknown_header: [u8; 5],
    pub keyframes: Vec<Keyframe>,
    /// The theme-wide fields after the keyframes, as (hash, value).
    pub fields: Vec<(u32, u32)>,
}

impl Environment {
    /// Parses a definition record.
    pub fn parse(record: &[u8]) -> Result<Self> {
        let mut r = Reader::new(record);
        let unknown_header = r.array()?;
        expect(&mut r, fields::TIME)?;
        let count = r.u32()?;
        if count as usize > record.len() / 8 {
            return Err(invalid(&r, format!("{count} keyframes")));
        }
        let mut keyframes: Vec<Keyframe> = Vec::with_capacity(count as usize);
        for k in 0..count as usize {
            expect(&mut r, fields::TIME_OF_DAY)?;
            let hour = f32::from_bits(r.u32()?);
            // A keyframe has the same fields as the first; the first runs up to
            // the next keyframe's hour, or the theme's fields.
            let mut fields = Vec::new();
            let known: Option<Vec<u32>> =
                (k > 0).then(|| keyframes[0].fields.iter().map(|(h, _)| *h).collect());
            loop {
                if let Some(known) = &known
                    && fields.len() == known.len()
                {
                    break;
                }
                let at = r.pos();
                let hash = r.u32()?;
                if known.is_none()
                    && (hash == fields::TIME_OF_DAY || theme_field(hash))
                    && !fields.is_empty()
                {
                    r = Reader::new(record);
                    r.bytes(at)?;
                    break;
                }
                if let Some(known) = &known
                    && known[fields.len()] != hash
                {
                    return Err(invalid(&r, format!("field {hash:08x} in keyframe {k}")));
                }
                let len = if hash == fields::ONE_BYTE { 1 } else { 4 };
                fields.push((hash, r.bytes(len)?.to_vec()));
            }
            keyframes.push(Keyframe { hour, fields });
        }
        let mut theme = Vec::new();
        while r.pos() < record.len() {
            let hash = r.u32()?;
            theme.push((hash, r.u32()?));
        }
        Ok(Self {
            unknown_header,
            keyframes,
            fields: theme,
        })
    }

    /// A theme-wide field.
    pub fn field(&self, hash: u32) -> Option<u32> {
        self.fields
            .iter()
            .find(|(h, _)| *h == hash)
            .map(|(_, v)| *v)
    }

    /// The colour table column of its first keyframe.
    pub fn table_index(&self) -> Option<usize> {
        self.field(fields::TABLE_INDEX).map(|v| v as usize)
    }

    /// The keyframes either side of `hour` and how far between them it is
    /// (0..1), wrapping round midnight. `None` without keyframes.
    pub fn between(&self, hour: f32) -> Option<(usize, usize, f32)> {
        let n = self.keyframes.len();
        if n == 0 {
            return None;
        }
        let hour = hour.rem_euclid(24.0);
        // The last keyframe at or before `hour`, else the day's last one.
        let a = (0..n)
            .rev()
            .find(|&k| self.keyframes[k].hour <= hour)
            .unwrap_or(n - 1);
        let b = (a + 1) % n;
        let (start, mut end) = (self.keyframes[a].hour, self.keyframes[b].hour);
        if end <= start {
            end += 24.0;
        }
        let mut at = hour;
        if at < start {
            at += 24.0;
        }
        let t = if end > start {
            (at - start) / (end - start)
        } else {
            0.0
        };
        Some((a, b, t.clamp(0.0, 1.0)))
    }

    /// The colour of table `row` at `hour`, blended between keyframes, as linear
    /// 0..1 values of the table's (sRGB) bytes. `None` if the table has no such
    /// colour or the row is unused (magenta).
    pub fn colour(&self, table: &LightingTable, row: usize, hour: f32) -> Option<[f32; 3]> {
        let base = self.table_index()?;
        let (a, b, t) = self.between(hour)?;
        let ca = table.colour(base + a, row)?;
        let cb = table.colour(base + b, row)?;
        Some(std::array::from_fn(|i| {
            let x = f32::from(ca[i]) * (1.0 - t) + f32::from(cb[i]) * t;
            x / 255.0
        }))
    }
}

/// Fields that follow the keyframes, so end the first keyframe.
fn theme_field(hash: u32) -> bool {
    [0xf299_5fa0, fields::SUN_HEIGHT, fields::TABLE_INDEX].contains(&hash)
}

fn expect(r: &mut Reader, hash: u32) -> Result<()> {
    let got = r.u32()?;
    if got != hash {
        return Err(invalid(r, format!("field {got:08x}, expected {hash:08x}")));
    }
    Ok(())
}

fn invalid(r: &Reader, reason: String) -> Error {
    Error::Invalid {
        offset: r.pos(),
        reason,
    }
}

/// `LightingTable/lighting_colours.tga`: an uncompressed 24-bit TGA, one column
/// per keyframe of each theme, one row per light. Magenta (ff00ff) marks
/// unused cells and the gaps between themes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightingTable {
    pub width: usize,
    pub height: usize,
    /// RGB, row 0 first (the image's top row).
    pub pixels: Vec<[u8; 3]>,
}

impl LightingTable {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let [id_len, colour_map, kind] = r.array()?;
        let _map_spec: [u8; 5] = r.array()?;
        let _origin: [u8; 4] = r.array()?;
        let [w0, w1, h0, h1, bits, descriptor] = r.array()?;
        if colour_map != 0 || kind != 2 || bits != 24 {
            return Err(invalid(
                &r,
                format!("TGA type {kind}, colour map {colour_map}, {bits} bits"),
            ));
        }
        r.bytes(usize::from(id_len))?;
        let (width, height) = (
            usize::from(u16::from_le_bytes([w0, w1])),
            usize::from(u16::from_le_bytes([h0, h1])),
        );
        let raw = r.bytes(width * height * 3)?;
        let top_first = descriptor & 0x20 != 0;
        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            let stored = if top_first { row } else { height - 1 - row };
            for x in 0..width {
                let p = &raw[3 * (stored * width + x)..][..3];
                pixels.push([p[2], p[1], p[0]]);
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// The colour at `column`, `row`, if it is in the table and not magenta.
    pub fn colour(&self, column: usize, row: usize) -> Option<[u8; 3]> {
        if column >= self.width || row >= self.height {
            return None;
        }
        let c = self.pixels[row * self.width + column];
        (c != [255, 0, 255]).then_some(c)
    }
}

/// The environment themes by definition index, parsed from the definitions of
/// type `ENVIRONMENT_THEME_DAY`.
pub fn themes(defs: &crate::defs::Defs) -> HashMap<usize, Environment> {
    defs.defs
        .iter()
        .enumerate()
        .filter(|(_, d)| defs.type_name(d) == "ENVIRONMENT_THEME_DAY")
        .filter_map(|(i, d)| Some((i, Environment::parse(&d.record).ok()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut d = vec![1, 0, 1, 0, 0];
        let mut put = |h: u32, v: &[u8]| {
            d.extend_from_slice(&h.to_le_bytes());
            d.extend_from_slice(v);
        };
        put(fields::TIME, &2u32.to_le_bytes());
        for hour in [6.0f32, 18.0] {
            put(fields::TIME_OF_DAY, &hour.to_le_bytes());
            put(fields::ONE_BYTE, &[1]);
            put(fields::SKY_TEXTURE_0, &388u32.to_le_bytes());
        }
        put(0xf299_5fa0, &0.18f32.to_le_bytes());
        put(fields::TABLE_INDEX, &1u32.to_le_bytes());
        d
    }

    fn tga() -> Vec<u8> {
        // 3 x 2, bottom row first: row 1 then row 0.
        let mut d = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 2, 0, 24, 0];
        // Row 1: magenta, black, white (BGR).
        d.extend_from_slice(&[255, 0, 255, 0, 0, 0, 255, 255, 255]);
        // Row 0: red, green, blue.
        d.extend_from_slice(&[0, 0, 255, 0, 255, 0, 255, 0, 0]);
        d
    }

    #[test]
    fn parses_keyframes_and_theme_fields() {
        let env = Environment::parse(&record()).unwrap();
        assert_eq!(env.keyframes.len(), 2);
        assert_eq!(env.keyframes[1].hour, 18.0);
        assert_eq!(env.keyframes[1].u32(fields::SKY_TEXTURE_0), Some(388));
        assert_eq!(env.keyframes[0].fields[0], (fields::ONE_BYTE, vec![1]));
        assert_eq!(env.table_index(), Some(1));
        assert!(Environment::parse(&record()[..20]).is_err());
    }

    #[test]
    fn hours_wrap_round_midnight() {
        let env = Environment::parse(&record()).unwrap();
        assert_eq!(env.between(12.0), Some((0, 1, 0.5)));
        assert_eq!(env.between(0.0), Some((1, 0, 0.5)), "18:00 to 06:00");
        assert_eq!(env.between(19.5), Some((1, 0, 0.125)));
    }

    #[test]
    fn colours_come_from_the_table() {
        let table = LightingTable::parse(&tga()).unwrap();
        assert_eq!(table.colour(0, 0), Some([255, 0, 0]));
        assert_eq!(table.colour(0, 1), None, "magenta is unused");
        let env = Environment::parse(&record()).unwrap();
        // Columns 1 and 2 at 06:00 and 18:00; noon is halfway.
        let row1 = env.colour(&table, 1, 12.0).unwrap();
        assert!(row1.iter().all(|&c| (c - 0.5).abs() < 0.01), "{row1:?}");
        assert_eq!(env.colour(&table, 0, 6.0), Some([0.0, 1.0, 0.0]));
    }
}
