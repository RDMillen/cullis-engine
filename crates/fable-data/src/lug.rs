//! Ambient sound banks: `Sound/<name>.lug` holds the sounds' data, and
//! `Sound/<name>.met` lists them with how they play. See `docs/formats/lug.md`.

use std::io::{Read, Seek, SeekFrom};

use crate::reader::{Reader, latin1};
use crate::{Error, Result};

/// One sound in a `.lug`, as its `.met` lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientSound {
    /// 1-based, as the `.met` numbers them.
    pub index: u32,
    /// The original file (`S:\BigBlueBox\Fable atmos\Country\1644XBADPCM\garden.wav`).
    pub path: String,
    pub size: u32,
    /// Offset of the sound's data in the `.lug`.
    pub offset: u32,
    pub channels: u16,
    /// WAVE format tag: 0x69 (Xbox ADPCM).
    pub format: u16,
    pub sample_rate: u32,
    /// Loop start and end in samples, or -1 for none.
    pub loop_start: i32,
    pub loop_end: i32,
}

impl AmbientSound {
    /// The file name without its folders (`garden.wav`).
    pub fn file_name(&self) -> &str {
        self.path.rsplit('\\').next().unwrap_or(&self.path)
    }
}

/// One way an ambient sound plays: the second table of a `.met` (u32 sound
/// index, a u32-length name, then 40 bytes), mostly not understood. A sound
/// may have several (`mm_wind_lost_bay.wav` by day and by night, at different
/// volumes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playback {
    /// The sound it plays ([`AmbientSound::index`]).
    pub sound: u32,
    /// Usually empty; `Black and White Shoreline` in `shore.met`.
    pub name: String,
    /// 1 for day sounds, 2 for night ones (`NightCreature_*` in `country`).
    pub group: u16,
    /// 9999 for a background bed that loops (`garden.wav`), 1 for sounds
    /// played once each time.
    pub repeats: u32,
    /// Volume in percent of the recording (100, 70, 50, 40 and 140 seen).
    pub volume: u16,
    /// Per sound, perhaps how often it plays: 2000 or 4000 for birds, 800 for
    /// night creatures, 1 for the bed.
    pub frequency: u32,
    /// The record's 40 bytes after the name, as read (the fields above included):
    /// u16 group, u16 (1 in some of `Graveyard.met`, else 0), u32 repeats, i32 -1 or 0, u32 0, u16 7/90/127, u16 volume,
    /// u16 0/10/20, u16 12/8/4, 8 zero bytes, u32 frequency, then a u32 that
    /// is usually the next record's number counting from 1 (0 on the last) but
    /// not always (`Lost_Bay.met`: record 4 has 6, record 9 has 13 of 10).
    pub raw: [u8; 40],
}

/// A `.met`: the sounds of its `.lug`.
#[derive(Debug, Clone)]
pub struct AmbientBank {
    /// 1 in every file seen.
    pub version: u32,
    pub sounds: Vec<AmbientSound>,
    pub plays: Vec<Playback>,
    /// The second table's second value (1 in every file seen).
    pub unknown_table: u32,
}

impl AmbientBank {
    /// Parses a `.met`.
    pub fn parse_met(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let u16 = |r: &mut Reader| -> Result<u16> { Ok(u16::from_le_bytes(r.array()?)) };
        let i32 = |r: &mut Reader| -> Result<i32> { Ok(i32::from_le_bytes(r.array()?)) };
        let version = r.u32()?;
        let count = r.u32()?;
        if count > 100_000 {
            return Err(Error::Invalid {
                offset: 4,
                reason: format!("{count} sounds"),
            });
        }
        let mut sounds = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let index = r.u32()?;
            let len = r.u32()? as usize;
            let path = latin1(r.bytes(len)?);
            sounds.push(AmbientSound {
                index,
                path,
                size: r.u32()?,
                offset: r.u32()?,
                channels: u16(&mut r)?,
                format: u16(&mut r)?,
                sample_rate: r.u32()?,
                loop_start: i32(&mut r)?,
                loop_end: i32(&mut r)?,
            });
        }
        let table_count = r.u32()?;
        let unknown_table = r.u32()?;
        if table_count > 100_000 {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!("{table_count} playback records"),
            });
        }
        let mut plays = Vec::with_capacity(table_count as usize);
        for _ in 0..table_count {
            let at_record = r.pos();
            let sound = r.u32()?;
            if !sounds.iter().any(|s| s.index == sound) {
                return Err(Error::Invalid {
                    offset: at_record,
                    reason: format!("playback of sound {sound}, which is not listed"),
                });
            }
            let len = r.u32()? as usize;
            let name = latin1(r.bytes(len)?);
            let raw: [u8; 40] = r.array()?;
            let at = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
            plays.push(Playback {
                sound,
                name,
                group: u16::from_le_bytes([raw[0], raw[1]]),
                repeats: at(4),
                volume: u16::from_le_bytes([raw[18], raw[19]]),
                frequency: at(32),
                raw,
            });
        }
        Ok(Self {
            version,
            sounds,
            plays,
            unknown_table,
        })
    }

    /// Reads and decodes one sound from its `.lug` (each sound there is a
    /// complete RIFF WAVE file): 16-bit samples, interleaved.
    pub fn read_samples<R: Read + Seek>(
        &self,
        lug: &mut R,
        sound: &AmbientSound,
    ) -> Result<Vec<i16>> {
        lug.seek(SeekFrom::Start(u64::from(sound.offset)))?;
        let mut data = vec![0u8; sound.size as usize];
        lug.read_exact(&mut data)?;
        crate::lut::Wave::parse(&data)?.decode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn met(sounds: &[(&str, u32, u32, u16, u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let put = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        put(&mut out, 1);
        put(&mut out, sounds.len() as u32);
        for (i, (path, size, offset, channels, _, _)) in sounds.iter().enumerate() {
            put(&mut out, i as u32 + 1);
            put(&mut out, path.len() as u32);
            out.extend_from_slice(path.as_bytes());
            put(&mut out, *size);
            put(&mut out, *offset);
            out.extend_from_slice(&channels.to_le_bytes());
            out.extend_from_slice(&0x69u16.to_le_bytes());
            put(&mut out, 44100);
            out.extend_from_slice(&(-1i32).to_le_bytes());
            out.extend_from_slice(&(-1i32).to_le_bytes());
        }
        put(&mut out, sounds.len() as u32);
        put(&mut out, 1);
        for (i, (_, _, _, _, repeats, frequency)) in sounds.iter().enumerate() {
            put(&mut out, i as u32 + 1);
            // The first record named, as in shore.met.
            let name: &[u8] = if i == 0 { b"Shoreline" } else { b"" };
            put(&mut out, name.len() as u32);
            out.extend_from_slice(name);
            let mut rec = [0u8; 40];
            rec[..4].copy_from_slice(&1u32.to_le_bytes());
            rec[4..8].copy_from_slice(&repeats.to_le_bytes());
            rec[18..20].copy_from_slice(&100u16.to_le_bytes());
            rec[32..36].copy_from_slice(&frequency.to_le_bytes());
            out.extend_from_slice(&rec);
        }
        out
    }

    #[test]
    fn parses_sounds_and_how_they_play() {
        let data = met(&[
            ("S:\\atmos\\garden.wav", 72, 600, 2, 9999, 1),
            ("S:\\atmos\\Bird001.wav", 36, 672, 1, 1, 2000),
        ]);
        let bank = AmbientBank::parse_met(&data).unwrap();
        assert_eq!(bank.sounds.len(), 2);
        let bed = &bank.sounds[0];
        assert_eq!(bed.file_name(), "garden.wav");
        assert_eq!((bed.offset, bed.channels, bed.sample_rate), (600, 2, 44100));
        assert_eq!(bank.plays.len(), 2);
        assert_eq!(bank.plays[0].sound, 1);
        assert_eq!((bank.plays[0].repeats, bank.plays[0].volume), (9999, 100));
        assert_eq!(bank.plays[0].name, "Shoreline");
        assert_eq!(bank.plays[1].frequency, 2000);

        // Reading a sound decodes the WAVE file at its offset in the .lug.
        let mut wave = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        wave.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 1] {
            wave.extend_from_slice(&v.to_le_bytes());
        }
        for v in [44100u32, 88200] {
            wave.extend_from_slice(&v.to_le_bytes());
        }
        for v in [2u16, 16] {
            wave.extend_from_slice(&v.to_le_bytes());
        }
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&4u32.to_le_bytes());
        wave.extend_from_slice(&[123, 0, 0xff, 0xff]);
        let mut lug = vec![0u8; 672];
        lug.extend_from_slice(&wave);
        let mut sound = bank.sounds[1].clone();
        sound.size = wave.len() as u32;
        let samples = bank
            .read_samples(&mut std::io::Cursor::new(lug), &sound)
            .unwrap();
        assert_eq!(samples, [123, -1]);
    }

    #[test]
    fn rejects_playbacks_of_unlisted_sounds() {
        let mut data = met(&[("a.wav", 36, 0, 1, 1, 1)]);
        // The playback record's sound index: after the table's count and
        // value, one record of index, name length, `Shoreline` and 40 bytes.
        let at = data.len() - (4 + 4 + 9 + 40);
        data[at..at + 4].copy_from_slice(&7u32.to_le_bytes());
        assert!(AmbientBank::parse_met(&data).is_err());
    }
}
