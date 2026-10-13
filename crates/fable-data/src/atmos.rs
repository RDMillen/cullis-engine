//! Music and ambience: the numbered types in `Defs/atmos_types.h` (a plain C
//! header shipped with the game), and which file in `Sound/` each names.
//!
//! Region definitions give their music as a music set number
//! (`EMusicSetType`: `MUSIC_SET_OAKVALE`), and sound themes give their
//! ambience as an atmosphere number (`EAtmosType`: `ATMOS_COUNTRY`). See
//! `docs/formats/lug.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::defs::Def;

/// Field of `REGION` definitions holding the region's music set
/// (`EMusicSetType`), 19 (`MUSIC_SET_OAKVALE`) for `REGION_OAK_VALE_INTRO`.
/// Its name is not known; found by matching regions to their music.
pub const REGION_MUSIC_FIELD: u32 = 0xcab1_fa53;
/// Field of `SOUND_THEME` definitions holding the theme's atmosphere
/// (`EAtmosType`), 10 (`ATMOS_COUNTRY`) for `SOUND_THEME_COUNTRY`.
pub const THEME_ATMOS_FIELD: u32 = 0xe001_b756;

/// The enums of a C header: per enum, its constants and values.
#[derive(Debug, Clone, Default)]
pub struct Enums {
    by_enum: HashMap<String, Vec<(String, i64)>>,
}

impl Enums {
    /// Parses `enum Name { A = 1, B, C = OTHER, … };` blocks. A constant without
    /// a value follows the one before; a value may be a number or an earlier
    /// constant (of any enum).
    pub fn parse(header: &str) -> Self {
        let mut by_enum = HashMap::new();
        let mut known: HashMap<String, i64> = HashMap::new();
        // Comments out.
        let text: String = header
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let mut rest = text.as_str();
        while let Some(start) = rest.find("enum ") {
            rest = &rest[start + 5..];
            let Some(open) = rest.find('{') else { break };
            let name = rest[..open].trim().to_owned();
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let body = &rest[open + 1..open + close];
            rest = &rest[open + close..];
            let mut next = 0i64;
            let mut members = Vec::new();
            for item in body.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let (constant, value) = match item.split_once('=') {
                    Some((c, v)) => {
                        let v = v.trim();
                        let value = v
                            .parse::<i64>()
                            .ok()
                            .or_else(|| known.get(v).copied())
                            .unwrap_or(next);
                        (c.trim(), value)
                    }
                    None => (item, next),
                };
                known.insert(constant.to_owned(), value);
                members.push((constant.to_owned(), value));
                next = value + 1;
            }
            by_enum.insert(name, members);
        }
        Self { by_enum }
    }

    /// Reads `Defs/atmos_types.h` from the data directory.
    pub fn open(data_dir: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(data_dir.join("Defs/atmos_types.h"))?;
        Ok(Self::parse(&crate::reader::latin1(&bytes)))
    }

    /// The constant of enum `enum_name` with value `value`.
    pub fn name(&self, enum_name: &str, value: i64) -> Option<&str> {
        self.by_enum
            .get(enum_name)?
            .iter()
            .find(|(_, v)| *v == value)
            .map(|(n, _)| n.as_str())
    }

    /// The value of constant `constant` in enum `enum_name`.
    pub fn value(&self, enum_name: &str, constant: &str) -> Option<i64> {
        self.by_enum
            .get(enum_name)?
            .iter()
            .find(|(n, _)| n == constant)
            .map(|(_, v)| *v)
    }
}

/// A name compared loosely: lower case without underscores, dashes or spaces.
fn loose(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The file in `sound_dir` with extension `ext` whose name, loosely, is `stem`.
fn find(sound_dir: &Path, stem: &str, ext: &str) -> Option<PathBuf> {
    let want = loose(stem);
    std::fs::read_dir(sound_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| loose(s) == want)
        })
}

/// The music file of a music set (`MUSIC_SET_OAKVALE` → `Sound/OAKVALE.OGG`),
/// if one is named after it.
pub fn music_file(sound_dir: &Path, music_set: &str) -> Option<PathBuf> {
    find(sound_dir, music_set.strip_prefix("MUSIC_SET_")?, "ogg")
}

/// The ambient bank (`.met`; its `.lug` beside it) of an atmosphere
/// (`ATMOS_COUNTRY` → `Sound/country.met`), if one is named after it.
pub fn atmos_bank(sound_dir: &Path, atmos: &str) -> Option<PathBuf> {
    find(sound_dir, atmos.strip_prefix("ATMOS_")?, "met")
}

/// A region definition's music set number.
pub fn region_music_set(region: &Def) -> Option<u32> {
    region.value_after(REGION_MUSIC_FIELD)
}

/// A sound theme definition's atmosphere number.
pub fn theme_atmos(theme: &Def) -> Option<u32> {
    theme.value_after(THEME_ATMOS_FIELD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_enums_with_values_from_other_enums() {
        let enums = Enums::parse(
            "enum ESound\n{\n\tSOUND_A=1,\n\tSOUND_B,\n\tNO_SOUNDS,\n};\n\
             enum EAtmos\n{\n\tATMOS_NULL = 0,\n\tATMOS_INN = NO_SOUNDS, // comment\n\tATMOS_NIGHT,\n};",
        );
        assert_eq!(enums.value("ESound", "NO_SOUNDS"), Some(3));
        assert_eq!(enums.value("EAtmos", "ATMOS_INN"), Some(3));
        assert_eq!(enums.name("EAtmos", 4), Some("ATMOS_NIGHT"));
        assert_eq!(enums.name("EAtmos", 9), None);
    }

    #[test]
    fn loose_names_match_files() {
        assert_eq!(loose("KRAKEN_WATER_FLOW"), loose("KrakenWaterFlow"));
        assert_eq!(
            loose("DEMON_DOOR_LOOKOUT_POINT"),
            loose("Demon_Door_Lookout_Point")
        );
        assert_ne!(loose("COUNTRY"), loose("country2"));
    }
}
