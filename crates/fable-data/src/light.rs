//! Light components (`CLightDef` definitions): the point lights of lamps,
//! fireplaces, campfires and light markers. An entity lists one as its
//! `CLightDef` component; placed things (`CTCLight` in a Tng) can switch it
//! on or off and override its values. See `docs/formats/defs.md`.

use crate::reader::Reader;
use crate::{Error, Result};

/// Field hashes (names found by hashing guesses; `Channel` and the unnamed
/// ones are descriptions).
pub mod fields {
    pub const COLOUR: u32 = 0x4c96_c15b;
    pub const INNER_RADIUS: u32 = 0x9864_ec8d;
    pub const OUTER_RADIUS: u32 = 0x9671_943e;
    pub const FLICKER: u32 = 0x075f_6980;
    pub const FLICKER_SPEED: u32 = 0xd7ac_d1be;
    pub const SUNLIGHT_ATTENUATOR: u32 = 0x6377_e74e;
    pub const DAYLIGHT_ATTENUATOR: u32 = 0xd37d_c96b;
    /// Name unknown: 0 on `MARKER_LIGHT_INTERNAL_CHANNEL` and indoor fireplaces,
    /// 2 on `MARKER_LIGHT_EXTERNAL_CHANNEL` and street lamps, 3 on plain
    /// `MARKER_LIGHT`s.
    pub const CHANNEL: u32 = 0x55be_32b1;
}

/// Each field in record order and its value's size in bytes.
const LAYOUT: [(u32, usize); 15] = [
    (fields::COLOUR, 4),
    (0xcf59_348b, 4),
    (fields::INNER_RADIUS, 4),
    (fields::OUTER_RADIUS, 4),
    (fields::FLICKER, 4),
    (fields::FLICKER_SPEED, 4),
    (0x5ec4_3162, 1),
    (fields::SUNLIGHT_ATTENUATOR, 1),
    (fields::DAYLIGHT_ATTENUATOR, 1),
    (fields::CHANNEL, 4),
    (0xa169_6aa7, 1),
    (0x9c8a_70e9, 4),
    (0x7d29_a6f3, 4),
    (0x6b21_7fa2, 4),
    (0x291f_b480, 4),
];

/// A `CLightDef` record.
#[derive(Debug, Clone, PartialEq)]
pub struct LightDef {
    /// RGB, 0..=255 (stored as BGRA; alpha 255 in every light seen).
    pub colour: [u8; 3],
    /// Full strength out to the inner radius, fading to nothing at the outer
    /// (world units).
    pub inner_radius: f32,
    pub outer_radius: f32,
    /// How much it flickers (0..1) and how fast (0.3 in every light seen).
    pub flicker: f32,
    pub flicker_speed: f32,
    /// Dimmed by sunlight, and by daylight (lamps lit only at night).
    pub sunlight_attenuator: bool,
    pub daylight_attenuator: bool,
    /// 0 internal, 2 external, 3 both (see [`fields::CHANNEL`]).
    pub channel: u32,
    /// The first 3 bytes (`01 00 01`).
    pub unknown_header: [u8; 3],
    /// The fields not decoded, as (hash, value bytes): `0xcf59348b` (0 in every
    /// light), `0x5ec43162` (1 in most), `0xa1696aa7` (0), and four values that
    /// look like hours (12.0 and 0; swapped on
    /// `MARKER_LIGHT_INVERSE_DAYLIGHT_ATTENUATOR`).
    pub unknown_fields: Vec<(u32, Vec<u8>)>,
}

impl LightDef {
    pub fn parse(record: &[u8]) -> Result<Self> {
        let mut r = Reader::new(record);
        let unknown_header = r.array()?;
        let mut values: Vec<(u32, &[u8])> = Vec::with_capacity(LAYOUT.len());
        for (hash, len) in LAYOUT {
            let got = r.u32()?;
            if got != hash {
                return Err(Error::Invalid {
                    offset: r.pos() - 4,
                    reason: format!("field {got:08x}, expected {hash:08x}"),
                });
            }
            values.push((hash, r.bytes(len)?));
        }
        if r.pos() != record.len() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!("{} bytes after the fields", record.len() - r.pos()),
            });
        }
        let get = |hash: u32| values.iter().find(|(h, _)| *h == hash).map(|(_, v)| *v);
        let f32_of = |hash| get(hash).map_or(0.0, |v| f32::from_le_bytes(v.try_into().unwrap()));
        let flag = |hash| get(hash).is_some_and(|v| v[0] != 0);
        let colour = get(fields::COLOUR).unwrap_or(&[0; 4]);
        let known = [
            fields::COLOUR,
            fields::INNER_RADIUS,
            fields::OUTER_RADIUS,
            fields::FLICKER,
            fields::FLICKER_SPEED,
            fields::SUNLIGHT_ATTENUATOR,
            fields::DAYLIGHT_ATTENUATOR,
            fields::CHANNEL,
        ];
        Ok(Self {
            colour: [colour[2], colour[1], colour[0]],
            inner_radius: f32_of(fields::INNER_RADIUS),
            outer_radius: f32_of(fields::OUTER_RADIUS),
            flicker: f32_of(fields::FLICKER),
            flicker_speed: f32_of(fields::FLICKER_SPEED),
            sunlight_attenuator: flag(fields::SUNLIGHT_ATTENUATOR),
            daylight_attenuator: flag(fields::DAYLIGHT_ATTENUATOR),
            channel: get(fields::CHANNEL).map_or(0, |v| u32::from_le_bytes(v.try_into().unwrap())),
            unknown_header,
            unknown_fields: values
                .iter()
                .filter(|(h, _)| !known.contains(h))
                .map(|(h, v)| (*h, v.to_vec()))
                .collect(),
        })
    }
}

/// A Tng `CRGBColour(r,g,b,a)` value as RGB.
pub fn parse_tng_colour(value: &str) -> Option<[u8; 3]> {
    let inner = value
        .trim()
        .strip_prefix("CRGBColour(")?
        .strip_suffix(')')?;
    let mut parts = inner.split(',').map(|p| p.trim().parse::<u8>());
    Some([
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut d = vec![1, 0, 1];
        for (hash, len) in LAYOUT {
            d.extend_from_slice(&hash.to_le_bytes());
            let v: Vec<u8> = match hash {
                fields::COLOUR => vec![0x25, 0x47, 0x7d, 0xff],
                fields::INNER_RADIUS => 4.0f32.to_le_bytes().to_vec(),
                fields::OUTER_RADIUS => 6.0f32.to_le_bytes().to_vec(),
                fields::FLICKER => 0.5f32.to_le_bytes().to_vec(),
                fields::DAYLIGHT_ATTENUATOR => vec![1],
                fields::CHANNEL => 2u32.to_le_bytes().to_vec(),
                _ => vec![0; len],
            };
            d.extend_from_slice(&v);
        }
        d
    }

    #[test]
    fn parses_a_light() {
        let light = LightDef::parse(&record()).unwrap();
        assert_eq!(light.colour, [0x7d, 0x47, 0x25], "stored blue first");
        assert_eq!((light.inner_radius, light.outer_radius), (4.0, 6.0));
        assert_eq!(light.flicker, 0.5);
        assert!(light.daylight_attenuator && !light.sunlight_attenuator);
        assert_eq!(light.channel, 2);
        assert_eq!(light.unknown_fields.len(), 7);
        let mut bad = record();
        bad.push(0);
        assert!(LightDef::parse(&bad).is_err());
    }

    #[test]
    fn reads_tng_colours() {
        assert_eq!(
            parse_tng_colour("CRGBColour(130,60,5,255)"),
            Some([130, 60, 5])
        );
        assert_eq!(parse_tng_colour("CRGBColour(1,2)"), None);
    }
}
