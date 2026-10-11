//! The game's text: the `TEXT_ENGLISH_MAIN` bank of `lang/English/text.big`.
//! See `docs/formats/strings.md`.
//!
//! Entries of type 0 are strings (UTF-16 text, the voice bank and speaker of the
//! spoken line, the identifier and tags), type 1 are groups of alternative
//! strings, and the one type 2 entry, `[NarratorList]`, names the speakers.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::big::BigFile;
use crate::reader::{Reader, latin1};
use crate::{Error, Result};

/// Bank entry types.
pub const STRING_TYPE: u32 = 0;
pub const GROUP_TYPE: u32 = 1;
pub const NARRATOR_LIST_TYPE: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextEntry {
    pub text: String,
    /// The audio bank the spoken line is in (`Dialogue.lug`,
    /// `ScriptDialogue.lug`, `ScriptDialogue2.lug`); empty for unspoken text.
    pub sound_bank: String,
    /// Who says it (a `[NarratorList]` name such as `THERESA`); empty if nobody.
    pub speaker: String,
    /// The entry's own name (`TXT_REGION_OAKVALE`).
    pub identifier: String,
    /// Tags such as `CONVERSATION_ATTITUDE_ANGER`.
    pub tags: Vec<String>,
}

impl TextEntry {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut units = Vec::new();
        let mut r = Reader::new(data);
        loop {
            let unit = u16::from_le_bytes(r.array()?);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        let text = String::from_utf16_lossy(&units);
        let string = |r: &mut Reader| -> Result<String> {
            let len = r.u32()? as usize;
            Ok(latin1(r.bytes(len)?).trim_end_matches('\0').to_owned())
        };
        let sound_bank = string(&mut r)?;
        let speaker = string(&mut r)?;
        let identifier = string(&mut r)?;
        let count = r.u32()?;
        if count > 64 {
            return Err(Error::Invalid {
                offset: r.pos() - 4,
                reason: format!("{count} tags"),
            });
        }
        // Each tag: a u32 (0 in every one looked at), then a NUL-terminated name.
        let tags = (0..count)
            .map(|_| {
                let _unknown = r.u32()?;
                r.c_string()
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            text,
            sound_bank,
            speaker,
            identifier,
            tags,
        })
    }
}

/// A group entry: ids of strings that are alternatives for one line.
pub fn parse_group(data: &[u8]) -> Result<Vec<u32>> {
    let mut r = Reader::new(data);
    let count = r.u32()?;
    (0..count).map(|_| r.u32()).collect()
}

/// The `[NarratorList]` entry's info block: a `u32` count, then NUL-terminated
/// speaker names.
pub fn parse_narrators(info: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(info);
    let count = r.u32()?;
    (0..count).map(|_| r.c_string()).collect()
}

/// The text bank, with every string decoded.
#[derive(Debug, Clone, Default)]
pub struct TextBank {
    pub strings: HashMap<u32, TextEntry>,
    pub groups: HashMap<u32, Vec<u32>>,
    pub narrators: Vec<String>,
    by_name: HashMap<String, u32>,
}

impl TextBank {
    /// Reads `lang/<language>/text.big` under the data directory.
    pub fn open(data_dir: &Path, language: &str) -> Result<Self> {
        let path = data_dir.join("lang").join(language).join("text.big");
        let mut file = BufReader::new(File::open(&path)?);
        let big = BigFile::read(&mut file)?;
        let bank = big.banks.first().ok_or(Error::Invalid {
            offset: 0,
            reason: "text.big has no bank".into(),
        })?;
        let index = bank.read_index(&mut file)?;
        let mut text = Self::default();
        for entry in &index.entries {
            match entry.entry_type {
                STRING_TYPE => {
                    let data = entry.read_data(&mut file)?;
                    text.strings.insert(entry.id, TextEntry::parse(&data)?);
                }
                GROUP_TYPE => {
                    let data = entry.read_data(&mut file)?;
                    text.groups.insert(entry.id, parse_group(&data)?);
                }
                NARRATOR_LIST_TYPE => text.narrators = parse_narrators(&entry.info)?,
                _ => {}
            }
            text.by_name.insert(entry.name.clone(), entry.id);
        }
        Ok(text)
    }

    /// The entry id of `name` (a string or a group).
    pub fn id(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    /// The text of string `name`, or of the first string of group `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        let id = self.id(name)?;
        let id = match self.groups.get(&id) {
            Some(group) => *group.first()?,
            None => id,
        };
        self.strings.get(&id).map(|e| e.text.as_str())
    }

    /// The name of entry `id`.
    pub fn name(&self, id: u32) -> Option<&str> {
        self.by_name
            .iter()
            .find(|(_, i)| **i == id)
            .map(|(n, _)| n.as_str())
    }

    /// Every entry name, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.by_name.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn parses_a_spoken_line() {
        let mut data: Vec<u8> = "Hello, Hero!"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        data.extend_from_slice(&[0, 0]);
        string(&mut data, "ScriptDialogue.lug");
        string(&mut data, "THERESA");
        string(&mut data, "TEXT_TEST_10");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(b"CONVERSATION_ATTITUDE_ANGER\0");
        let e = TextEntry::parse(&data).unwrap();
        assert_eq!(e.text, "Hello, Hero!");
        assert_eq!(
            (e.sound_bank.as_str(), e.speaker.as_str()),
            ("ScriptDialogue.lug", "THERESA")
        );
        assert_eq!(e.identifier, "TEXT_TEST_10");
        assert_eq!(e.tags, ["CONVERSATION_ATTITUDE_ANGER"]);
    }

    #[test]
    fn parses_groups_and_narrators() {
        let mut g = 2u32.to_le_bytes().to_vec();
        g.extend_from_slice(&7u32.to_le_bytes());
        g.extend_from_slice(&9u32.to_le_bytes());
        assert_eq!(parse_group(&g).unwrap(), [7, 9]);
        let mut n = 2u32.to_le_bytes().to_vec();
        n.extend_from_slice(b"NONE\0THERESA\0");
        assert_eq!(parse_narrators(&n).unwrap(), ["NONE", "THERESA"]);
    }
}
