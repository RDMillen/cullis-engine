//! Audio banks (`lang/<language>/*.lut`, voices): Lionhead's chunked bank of
//! WAVE files, with a lookup table from sound id to place. See
//! `docs/formats/lut.md`.

use std::io::{Read, Seek, SeekFrom};

use crate::reader::{Reader, latin1};
use crate::{Error, Result};

/// The file's first 8 bytes.
pub const MAGIC: &[u8; 8] = b"LiOnHeAd";
/// Chunk names (32 bytes, zero-padded).
const SOUNDS: &str = "LHAudioBankCompData";
const LOOKUP: &str = "LHAudioBankLookupTable";

/// Where one sound is: its id and its bytes within the sounds chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundEntry {
    pub id: u32,
    pub length: u32,
    /// From the start of the sounds chunk's data.
    pub offset: u32,
}

/// A bank's index: where its sounds are, read without loading them.
#[derive(Debug, Clone)]
pub struct SoundBank {
    /// File offset of the sounds chunk's data.
    data_offset: u64,
    data_len: u32,
    /// The lookup table's first value: 1000 in `Dialogue.lut` and
    /// `ScriptDialogue.lut`, 500 in `Dialogue2.lut`. Unknown.
    pub unknown_lookup: u32,
    /// In id order, as the lookup table lists them (ids may have gaps).
    pub sounds: Vec<SoundEntry>,
    /// Chunks after the lookup table (`LHFileSegmentBankInfo`: the text
    /// "\Generated bank for Fable", "NoFPNameYet", then u32 4), as (name, data).
    pub other_chunks: Vec<(String, Vec<u8>)>,
}

/// One sound: its header and its WAVE file.
#[derive(Debug, Clone)]
pub struct Sound {
    pub id: u32,
    /// The header's bytes after the id (32), kept as read. Observed: u16 1,
    /// u16 sample rate (22050), u16 40000, u16 0 or 1, u16 0, bytes 1 1 6 and
    /// 0x7f or 0x58, u16 100, f32 5.0 or 1.5, f32 25.0 or 18.0, u32 1000 or
    /// 500, i32 -1. Perhaps playback settings (volume, distances, priority).
    pub unknown_header: [u8; 32],
    pub wave: Wave,
}

impl Sound {
    /// The sample rate the header gives (it matches the WAVE format's).
    pub fn header_sample_rate(&self) -> u16 {
        u16::from_le_bytes([self.unknown_header[2], self.unknown_header[3]])
    }
}

/// A RIFF WAVE file's format and sample data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    /// WAVE format tag: 0x69 (Xbox ADPCM) for the voices.
    pub format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    /// The format chunk's extra bytes (for Xbox ADPCM: u16 samples per block, 64).
    pub extra: Vec<u8>,
    pub data: Vec<u8>,
}

impl Wave {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let tag = |r: &mut Reader, want: &[u8; 4]| -> Result<()> {
            let found: [u8; 4] = r.array()?;
            if &found != want {
                return Err(Error::BadMagic {
                    expected: *want,
                    found,
                });
            }
            Ok(())
        };
        tag(&mut r, b"RIFF")?;
        r.u32()?;
        tag(&mut r, b"WAVE")?;
        let mut format = None;
        let mut data = None;
        while r.pos() + 8 <= bytes.len() {
            let id: [u8; 4] = r.array()?;
            let len = r.u32()? as usize;
            let at = r.pos();
            let body = r.bytes(len)?;
            match &id {
                b"fmt " => {
                    let mut f = Reader::new(body);
                    let u16 =
                        |f: &mut Reader| -> Result<u16> { Ok(u16::from_le_bytes(f.array()?)) };
                    let fields = (
                        u16(&mut f)?,
                        u16(&mut f)?,
                        f.u32()?,
                        f.u32()?,
                        u16(&mut f)?,
                        u16(&mut f)?,
                    );
                    let extra = if len > 16 {
                        let n = u16(&mut f)? as usize;
                        f.bytes(n)?.to_vec()
                    } else {
                        Vec::new()
                    };
                    format = Some((fields, extra));
                }
                b"data" => data = Some(body.to_vec()),
                _ => {}
            }
            // Chunks are padded to an even length.
            if len % 2 == 1 && r.pos() < bytes.len() {
                r.bytes(1)?;
            }
            let _ = at;
        }
        let ((format, channels, sample_rate, byte_rate, block_align, bits_per_sample), extra) =
            format.ok_or(Error::Invalid {
                offset: 0,
                reason: "WAVE without a fmt chunk".into(),
            })?;
        Ok(Self {
            format,
            channels,
            sample_rate,
            byte_rate,
            block_align,
            bits_per_sample,
            extra,
            data: data.ok_or(Error::Invalid {
                offset: 0,
                reason: "WAVE without a data chunk".into(),
            })?,
        })
    }

    /// Decodes the samples to 16-bit PCM (interleaved), if the format is one
    /// this crate knows (Xbox ADPCM, or 16-bit PCM).
    pub fn decode(&self) -> Result<Vec<i16>> {
        match self.format {
            1 if self.bits_per_sample == 16 => Ok(self
                .data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b))
                .collect()),
            0x69 => crate::adpcm::decode_xbox(&self.data, self.channels.max(1) as usize),
            f => Err(Error::Invalid {
                offset: 0,
                reason: format!("unsupported WAVE format {f:#x}"),
            }),
        }
    }
}

/// Reads a 32-byte zero-padded chunk name and the chunk's u32 length.
fn chunk_header<R: Read>(r: &mut R) -> Result<(String, u32)> {
    let mut name = [0u8; 32];
    r.read_exact(&mut name).map_err(Error::Io)?;
    let end = name.iter().position(|&b| b == 0).unwrap_or(32);
    let mut len = [0u8; 4];
    r.read_exact(&mut len).map_err(Error::Io)?;
    Ok((latin1(&name[..end]), u32::from_le_bytes(len)))
}

impl SoundBank {
    pub fn read<R: Read + Seek>(r: &mut R) -> Result<Self> {
        r.seek(SeekFrom::Start(0)).map_err(Error::Io)?;
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic).map_err(Error::Io)?;
        if &magic != MAGIC {
            return Err(Error::Invalid {
                offset: 0,
                reason: format!("not an audio bank: {:?}", latin1(&magic)),
            });
        }
        let (name, data_len) = chunk_header(r)?;
        if name != SOUNDS {
            return Err(Error::Invalid {
                offset: 8,
                reason: format!("expected {SOUNDS}, found {name:?}"),
            });
        }
        let data_offset = 8 + 36;
        let end = r.seek(SeekFrom::End(0)).map_err(Error::Io)?;
        let mut at = r
            .seek(SeekFrom::Start(data_offset + u64::from(data_len)))
            .map_err(Error::Io)?;
        let mut bank = Self {
            data_offset,
            data_len,
            unknown_lookup: 0,
            sounds: Vec::new(),
            other_chunks: Vec::new(),
        };
        while at + 36 <= end {
            let (name, len) = chunk_header(r)?;
            if u64::from(len) > end - at - 36 {
                return Err(Error::Invalid {
                    offset: at as usize,
                    reason: format!("chunk {name:?} runs past the end"),
                });
            }
            let mut body = vec![0u8; len as usize];
            r.read_exact(&mut body).map_err(Error::Io)?;
            if name == LOOKUP {
                let mut b = Reader::new(&body);
                bank.unknown_lookup = b.u32()?;
                let count = b.u32()?;
                for _ in 0..count {
                    let entry = SoundEntry {
                        id: b.u32()?,
                        length: b.u32()?,
                        offset: b.u32()?,
                    };
                    if u64::from(entry.offset) + u64::from(entry.length) > u64::from(data_len) {
                        return Err(Error::Invalid {
                            offset: at as usize,
                            reason: format!("sound {} runs past the data", entry.id),
                        });
                    }
                    bank.sounds.push(entry);
                }
            } else {
                bank.other_chunks.push((name, body));
            }
            at += 36 + u64::from(len);
        }
        Ok(bank)
    }

    /// The sound with id `id`, if the bank has it.
    pub fn entry(&self, id: u32) -> Option<SoundEntry> {
        self.sounds
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| self.sounds[i])
    }

    /// Reads and parses one sound.
    pub fn read_sound<R: Read + Seek>(&self, r: &mut R, entry: SoundEntry) -> Result<Sound> {
        r.seek(SeekFrom::Start(self.data_offset + u64::from(entry.offset)))
            .map_err(Error::Io)?;
        let mut bytes = vec![0u8; entry.length as usize];
        r.read_exact(&mut bytes).map_err(Error::Io)?;
        let mut b = Reader::new(&bytes);
        let id = b.u32()?;
        let unknown_header = b.array()?;
        let wave = Wave::parse(&bytes[36..])?;
        Ok(Sound {
            id,
            unknown_header,
            wave,
        })
    }

    /// Bytes in the sounds chunk.
    pub fn data_len(&self) -> u32 {
        self.data_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(out: &mut Vec<u8>, name: &str, body: &[u8]) {
        let mut n = [0u8; 32];
        n[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&n);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
    }

    fn wave(samples: &[i16]) -> Vec<u8> {
        let mut fmt = Vec::new();
        for v in [1u16, 1] {
            fmt.extend_from_slice(&v.to_le_bytes());
        }
        for v in [22050u32, 44100] {
            fmt.extend_from_slice(&v.to_le_bytes());
        }
        for v in [2u16, 16] {
            fmt.extend_from_slice(&v.to_le_bytes());
        }
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&((4 + 8 + fmt.len() + 8 + data.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn reads_a_bank_and_its_sounds() {
        // Two sounds (ids 1 and 3), each a header then a 16-bit PCM WAVE.
        let mut sounds = Vec::new();
        let mut entries = Vec::new();
        for (id, samples) in [(1u32, vec![1i16, -2, 3]), (3, vec![7, 8])] {
            let mut s = id.to_le_bytes().to_vec();
            s.extend_from_slice(&[0u8; 32]);
            s.extend_from_slice(&wave(&samples));
            entries.push((id, s.len() as u32, sounds.len() as u32));
            sounds.extend_from_slice(&s);
        }
        let mut file = MAGIC.to_vec();
        chunk(&mut file, SOUNDS, &sounds);
        let mut lookup = 1000u32.to_le_bytes().to_vec();
        lookup.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (id, len, off) in &entries {
            for v in [id, len, off] {
                lookup.extend_from_slice(&v.to_le_bytes());
            }
        }
        chunk(&mut file, LOOKUP, &lookup);
        chunk(&mut file, "LHFileSegmentBankInfo", b"info");

        let mut r = std::io::Cursor::new(file);
        let bank = SoundBank::read(&mut r).unwrap();
        assert_eq!(bank.unknown_lookup, 1000);
        assert_eq!(bank.sounds.len(), 2);
        assert_eq!(
            bank.other_chunks,
            [("LHFileSegmentBankInfo".into(), b"info".to_vec())]
        );
        assert!(bank.entry(2).is_none());
        let sound = bank.read_sound(&mut r, bank.entry(3).unwrap()).unwrap();
        assert_eq!(sound.id, 3);
        assert_eq!(sound.wave.sample_rate, 22050);
        assert_eq!(sound.wave.decode().unwrap(), [7, 8]);
    }

    #[test]
    fn rejects_other_files() {
        let mut r = std::io::Cursor::new(b"RIFF....WAVE".to_vec());
        assert!(SoundBank::read(&mut r).is_err());
    }
}
