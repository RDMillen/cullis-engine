//! Lip sync for voice lines: the `LIPSYNC_<LANGUAGE>_*` banks of
//! `lang/<language>/dialogue.big`, one entry per voice sound
//! (`ScriptDialogue_3642`). Each is a track of mouth shapes (visemes: `AH`,
//! `EE`, `OH`, `SZ`) with a weight per shape per frame. See
//! `docs/formats/lut.md`.

use crate::reader::Reader;
use crate::{Error, Result};

/// One line's mouth movement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LipSync {
    /// The shapes this track uses, by their index in the frames (`1 AH`).
    pub visemes: Vec<(u8, String)>,
    /// Frames per second: 43 in every entry seen.
    pub rate: u32,
    /// Per frame, (shape index, weight 0..=255) for the shapes in use.
    pub frames: Vec<Vec<(u8, u8)>>,
}

impl LipSync {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let count = r.u32()?;
        if count > 64 {
            return Err(Error::Invalid {
                offset: 0,
                reason: format!("{count} visemes"),
            });
        }
        let mut visemes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let [index] = r.array()?;
            visemes.push((index, r.c_string()?));
        }
        let rate = r.u32()?;
        let frame_count = r.u32()?;
        if frame_count as usize > data.len() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!("{frame_count} frames in {} bytes", data.len()),
            });
        }
        let mut frames = Vec::with_capacity(frame_count as usize);
        for _ in 0..frame_count {
            let [n] = r.array()?;
            let pairs = r.bytes(2 * usize::from(n))?;
            frames.push(
                pairs
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&[a, b]| (a, b))
                    .collect(),
            );
        }
        if r.pos() != data.len() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!("{} bytes after the frames", data.len() - r.pos()),
            });
        }
        Ok(Self {
            visemes,
            rate,
            frames,
        })
    }

    /// Length in seconds.
    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 / self.rate.max(1) as f32
    }

    /// The mouth shapes at `time` seconds, as (name, weight 0..=1), blending the
    /// two frames around it. Empty past the end.
    pub fn weights_at(&self, time: f32) -> Vec<(&str, f32)> {
        let at = time.max(0.0) * self.rate as f32;
        let i = at as usize;
        let Some(a) = self.frames.get(i) else {
            return Vec::new();
        };
        let b = self.frames.get(i + 1).unwrap_or(a);
        let t = at - i as f32;
        let weight = |frame: &[(u8, u8)], v: u8| {
            frame
                .iter()
                .find(|(k, _)| *k == v)
                .map_or(0.0, |&(_, w)| f32::from(w) / 255.0)
        };
        self.visemes
            .iter()
            .map(|(v, name)| {
                let w = weight(a, *v) * (1.0 - t) + weight(b, *v) * t;
                (name.as_str(), w)
            })
            .filter(|(_, w)| *w > 0.0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Vec<u8> {
        let mut d = 2u32.to_le_bytes().to_vec();
        d.extend_from_slice(b"\x01AH\0\x02OH\0");
        d.extend_from_slice(&10u32.to_le_bytes());
        d.extend_from_slice(&3u32.to_le_bytes());
        d.extend_from_slice(&[2, 1, 255, 2, 0]); // AH full
        d.extend_from_slice(&[1, 2, 255]); // OH full
        d.extend_from_slice(&[0]); // silence
        d
    }

    #[test]
    fn parses_a_track() {
        let lip = LipSync::parse(&track()).unwrap();
        assert_eq!(lip.visemes, [(1, "AH".into()), (2, "OH".into())]);
        assert_eq!(lip.rate, 10);
        assert_eq!(lip.frames, [vec![(1, 255), (2, 0)], vec![(2, 255)], vec![]]);
        assert!((lip.seconds() - 0.3).abs() < 1e-6);
        let mut bad = track();
        bad.push(0);
        assert!(LipSync::parse(&bad).is_err(), "trailing bytes");
    }

    #[test]
    fn weights_blend_between_frames() {
        let lip = LipSync::parse(&track()).unwrap();
        assert_eq!(lip.weights_at(0.0), [("AH", 1.0)]);
        let mid = lip.weights_at(0.05);
        assert!(
            mid.contains(&("AH", 0.5)) && mid.contains(&("OH", 0.5)),
            "{mid:?}"
        );
        assert!(lip.weights_at(1.0).is_empty(), "past the end");
    }
}
