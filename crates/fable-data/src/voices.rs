//! Which recorded sound speaks each line of text.
//!
//! The game data holds no explicit link: a text entry names only its voice bank
//! (`ScriptDialogue.lug`), and the banks (`*.lut`) number their sounds 1, 2, ….
//! Within a bank, sounds follow the bank's spoken lines in name order,
//! compared without case (so `_` sorts before letters), but lines were added
//! and cut after the banks were built, leaving gaps both ways. The link is
//! rebuilt here by aligning the two sequences: each line paired with a sound
//! whose length (from the lipsync bank, `dialogue.big`) fits the line's text,
//! skipping lines or sounds where that fits better. See `docs/formats/lut.md`.
//!
//! Aligned banks are `Dialogue` and `ScriptDialogue`, where the alignment is
//! clear. `ScriptDialogue2` (The Lost Chapters' lines) is not in name order
//! and is left out until its order is known.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::Result;
use crate::big::BigFile;
use crate::strings::TextBank;

/// Banks whose order is known, with their lipsync bank in `dialogue.big`.
pub const BANKS: &[&str] = &["Dialogue", "Dialogue2", "ScriptDialogue"];

/// How far (in slots) an alignment may stray from the diagonal: the gaps seen
/// add up to a few dozen.
const BAND: usize = 300;
/// Cost of leaving a line or a sound unpaired.
const SKIP: f64 = 1.2;

/// A spoken line's sound: its bank (`ScriptDialogue`) and id in that bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    pub bank: String,
    pub sound: u32,
}

/// Text entry id to its voice.
#[derive(Debug, Clone, Default)]
pub struct VoiceIndex {
    voices: HashMap<u32, Voice>,
}

/// Expected length in seconds of a line of `chars` characters.
fn expected(chars: usize) -> f64 {
    0.4 + chars as f64 / 14.0
}

/// How badly a line of `chars` characters fits a sound of `duration` seconds.
fn misfit(chars: usize, duration: f64) -> f64 {
    ((duration + 0.5) / (expected(chars) + 0.5)).ln().abs()
}

/// Whether a text entry's speaker says it aloud.
pub fn spoken(speaker: &str) -> bool {
    !speaker.is_empty()
        && !speaker.eq_ignore_ascii_case("NONE")
        && !speaker.eq_ignore_ascii_case("UNSPOKEN")
}

/// Sound slots per lipsync bank, in id order: (id, duration in seconds, or
/// None for an empty slot).
pub type Slots = HashMap<String, Vec<(u32, Option<f64>)>>;

/// Pairs lines (their lengths, in order) with sound slots (durations in order;
/// `None` for an empty slot, skipped for free), allowing skips: per line, the
/// index of its slot. Banded dynamic programming around the diagonal.
// The loops walk the band by position in both lists.
#[allow(clippy::needless_range_loop)]
pub fn align(lines: &[usize], slots: &[Option<f64>]) -> Vec<Option<usize>> {
    let (n, m) = (lines.len(), slots.len());
    let width = 2 * BAND + 1;
    // Cell (i, j) is stored at column j - i + BAND when within the band.
    let col = |i: usize, j: usize| (j + BAND).checked_sub(i).filter(|&c| c < width);
    let mut cost = vec![f64::INFINITY; (n + 1) * width];
    let mut from = vec![0u8; (n + 1) * width];
    cost[BAND] = 0.0;
    for i in 0..=n {
        let lo = i.saturating_sub(BAND);
        let hi = (i + BAND).min(m);
        for j in lo..=hi {
            let Some(c) = col(i, j) else { continue };
            let here = cost[i * width + c];
            if !here.is_finite() {
                continue;
            }
            let mut relax = |i2: usize, j2: usize, add: f64, how: u8| {
                if let Some(c2) = col(i2, j2) {
                    let k = i2 * width + c2;
                    if here + add < cost[k] {
                        cost[k] = here + add;
                        from[k] = how;
                    }
                }
            };
            if i < n
                && j < m
                && let Some(d) = slots[j]
            {
                relax(i + 1, j + 1, misfit(lines[i], d), 1);
            }
            if i < n {
                relax(i + 1, j, SKIP, 2);
            }
            if j < m {
                relax(i, j + 1, if slots[j].is_none() { 0.0 } else { SKIP }, 3);
            }
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (n, m);
    if col(i, j).is_none() {
        // The two lists differ by more than the band: nothing to trust.
        return out;
    }
    while i > 0 || j > 0 {
        let Some(c) = col(i, j) else { break };
        match from[i * width + c] {
            1 => {
                out[i - 1] = Some(j - 1);
                i -= 1;
                j -= 1;
            }
            2 => i -= 1,
            3 => j -= 1,
            _ => break,
        }
    }
    out
}

/// Sound slots per lipsync bank, in id order: (id, duration in seconds, or
/// None for an `[Empty]` entry). The lipsync banks are `LIPSYNC_<LANGUAGE>_*`
/// in `dialogue.big`, their entries named `<bank>_<id>` with the duration as
/// a little-endian f32 in the entry's info.
pub fn lipsync_slots(path: &Path) -> Result<Slots> {
    let mut f = BufReader::new(File::open(path)?);
    let big = BigFile::read(&mut f)?;
    let mut out = Slots::new();
    for bank in &big.banks {
        for e in bank.read_index(&mut f)?.entries {
            let empty = e.name.ends_with("[Empty]");
            let name = e.name.trim_end_matches("[Empty]");
            let Some((b, id)) = name.rsplit_once('_') else {
                continue;
            };
            let Ok(id) = id.parse::<u32>() else { continue };
            let duration = (!empty)
                .then(|| <[u8; 4]>::try_from(e.info.as_slice()).ok())
                .flatten()
                .map(|x| f64::from(f32::from_le_bytes(x)));
            out.entry(b.to_owned()).or_default().push((id, duration));
        }
    }
    for slots in out.values_mut() {
        slots.sort_by_key(|s| s.0);
    }
    Ok(out)
}

impl VoiceIndex {
    /// Builds the index for `language` from the text bank and `dialogue.big`.
    pub fn build(data_dir: &Path, language: &str, text: &TextBank) -> Result<Self> {
        let slots = lipsync_slots(&data_dir.join("lang").join(language).join("dialogue.big"))?;
        Ok(Self::from_parts(text, &slots))
    }

    pub fn from_parts(text: &TextBank, slots: &Slots) -> Self {
        let mut lines: HashMap<&str, Vec<(String, u32, usize)>> = HashMap::new();
        for name in text.names() {
            let Some(id) = text.id(name) else { continue };
            let Some(e) = text.strings.get(&id) else {
                continue;
            };
            let Some(bank) = e.sound_bank.strip_suffix(".lug") else {
                continue;
            };
            if spoken(&e.speaker) && BANKS.contains(&bank) {
                lines.entry(bank).or_default().push((
                    name.to_ascii_lowercase(),
                    id,
                    e.text.chars().count(),
                ));
            }
        }
        let mut voices = HashMap::new();
        for (bank, mut ls) in lines {
            let Some(ss) = slots.get(bank) else { continue };
            ls.sort();
            let lengths: Vec<usize> = ls.iter().map(|l| l.2).collect();
            let durations: Vec<Option<f64>> = ss.iter().map(|s| s.1).collect();
            for (line, slot) in ls.iter().zip(align(&lengths, &durations)) {
                if let Some(j) = slot {
                    voices.insert(
                        line.1,
                        Voice {
                            bank: bank.to_owned(),
                            sound: ss[j].0,
                        },
                    );
                }
            }
        }
        Self { voices }
    }

    /// The voice of text entry `id`, if it has one.
    pub fn get(&self, id: u32) -> Option<&Voice> {
        self.voices.get(&id)
    }

    pub fn len(&self) -> usize {
        self.voices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_through_cut_lines_and_unused_sounds() {
        // Sounds of 2, 6, 3, 0.5 and 4 seconds, and an empty slot after the
        // second. Lines fitting 2 s, 6 s, (a long line cut from the recording),
        // 3 s and 4 s: the 0.5 s sound has no line any more.
        let slots = [Some(2.0), Some(6.0), None, Some(3.0), Some(0.5), Some(4.0)];
        let lines = [22, 78, 200, 36, 50];
        assert_eq!(
            align(&lines, &slots),
            [Some(0), Some(1), None, Some(3), Some(5)]
        );
    }

    #[test]
    fn unspoken_speakers() {
        assert!(spoken("FATHER"));
        assert!(!spoken(""));
        assert!(!spoken("NONE"));
        assert!(!spoken("Unspoken"));
    }
}
