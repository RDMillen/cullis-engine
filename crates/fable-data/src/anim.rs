//! Animations in the `MBANK_ALLMESHES` bank of `graphics/graphics.big` (entry
//! types 6, 7 and 9). See `docs/formats/anim.md`.
//!
//! An entry is a `u32` length and an LZO1X stream. Unpacked, it is a `>>>>` tag,
//! a `3DAF` header (version and copyright string), then chunks of (4-byte tag,
//! `u32` length). The `ANRT` chunk holds one `AOBJ` per animated object, which
//! holds one `XSEQ` track per bone: keyed rotations (float quaternions) and
//! positions (3 x `i16`, scaled so their largest component is 1; the length comes
//! from the skeleton the animation is played on).

use crate::reader::Reader;
use crate::{Error, Result, lzo};

/// Bank entry types of animations: full-body (6), short or facial (7) and partial
/// (9: upper body, looks).
pub const ANIMATION_TYPES: [u32; 3] = [6, 7, 9];

/// The bank entry's 24-byte info block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimInfo {
    /// Seconds: frame count / 30 (0 for one-frame poses).
    pub duration: f32,
    /// Seconds: (frame count - 1) / 30.
    pub last_frame_time: f32,
    /// 16 bytes, zero in every entry looked at.
    pub unknown: [u8; 16],
}

impl AnimInfo {
    pub fn parse(info: &[u8]) -> Result<Self> {
        let mut r = Reader::new(info);
        Ok(Self {
            duration: f32::from_bits(r.u32()?),
            last_frame_time: f32::from_bits(r.u32()?),
            unknown: r.array()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// `3DAF` version: 100.
    pub version: u32,
    /// `ANRT`'s first byte (1) and its float (the duration in seconds).
    pub unknown_root: (u8, f32),
    pub objects: Vec<AnimObject>,
    /// Chunks after `ANRT` (`HLPR`, `MVEC`, `XALO`, …) as (tag, bytes); not decoded.
    pub other_chunks: Vec<([u8; 4], Vec<u8>)>,
    /// Bytes after the last chunk, too short to be one (4 zero bytes seen).
    pub unknown_trailer: Vec<u8>,
}

/// The tracks of one animated object (`HeroUnclothed`, `Crypt_door`, …).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimObject {
    pub name: String,
    pub unknown: u32,
    pub tracks: Vec<Track>,
    /// Chunks other than `XSEQ` inside the object, as (tag, bytes).
    pub other_chunks: Vec<([u8; 4], Vec<u8>)>,
}

/// One bone's keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// 0x7ada in every track looked at.
    pub unknown_id: u32,
    /// Index of the parent track, `None` for the root.
    pub parent: Option<usize>,
    pub name: String,
    /// 2 on the `Movement` bone of some animations, else 0.
    pub unknown_flag: u8,
    pub fps: f32,
    pub frames: u32,
    /// Bytes 0–3 of the keys: has rotations (0/1), rotation layout (1 one key,
    /// 2 a key per frame, 3 a palette with per-frame indices), position layout
    /// (the same, or 0 for none), and 0. Informational: [`Track::rotation`] and
    /// [`Track::position`] follow the key and index counts.
    pub layout: [u8; 4],
    /// Small (1e-4 to 1e-2); perhaps the compression tolerance.
    pub unknown_tolerance: f32,
    /// 0x0100 and 0x3800 in every track looked at.
    pub unknown_words: [u16; 2],
    /// Quaternions (x, y, z, w), in the convention of [`crate::mesh::Bone::rotation`].
    pub rotations: Vec<[f32; 4]>,
    /// Per frame, an index into `rotations` (empty unless the keys are a palette).
    pub rotation_frames: Vec<u8>,
    /// Positions divided by their largest component, times 32767.
    pub positions: Vec<[i16; 3]>,
    pub position_frames: Vec<u8>,
}

impl Track {
    fn key<T: Copy>(keys: &[T], frames: &[u8], frame: u32) -> Option<T> {
        let i = if !frames.is_empty() {
            usize::from(*frames.get(frame as usize).or(frames.last())?)
        } else if keys.len() == 1 {
            0
        } else {
            (frame as usize).min(keys.len().checked_sub(1)?)
        };
        keys.get(i).copied()
    }

    /// The rotation at `frame` (clamped to the last), if the track has rotations.
    pub fn rotation(&self, frame: u32) -> Option<[f32; 4]> {
        Self::key(&self.rotations, &self.rotation_frames, frame)
    }

    /// The position at `frame` as a direction scaled so its largest component is
    /// ±1; multiply by the bone's length (its rest translation's largest
    /// component) to get model units.
    pub fn position(&self, frame: u32) -> Option<[f32; 3]> {
        Self::key(&self.positions, &self.position_frames, frame)
            .map(|p| p.map(|v| f32::from(v) / 32767.0))
    }

    fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let unknown_id = r.u32()?;
        let parent = r.u32()? as i32;
        let name = r.c_string()?;
        let [unknown_flag] = r.array()?;
        let fps = f32::from_bits(r.u32()?);
        let frames = r.u32()?;
        let layout = r.array()?;
        let unknown_tolerance = f32::from_bits(r.u32()?);
        let unknown_words = [
            u16::from_le_bytes(r.array()?),
            u16::from_le_bytes(r.array()?),
        ];
        let count = |r: &mut Reader| r.array().map(u16::from_le_bytes).map(usize::from);
        let n = count(&mut r)?;
        let rotations = (0..n)
            .map(|_| {
                Ok([
                    f32::from_bits(r.u32()?),
                    f32::from_bits(r.u32()?),
                    f32::from_bits(r.u32()?),
                    f32::from_bits(r.u32()?),
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        let n = count(&mut r)?;
        let rotation_frames = r.bytes(n)?.to_vec();
        let n = count(&mut r)?;
        let positions = (0..n)
            .map(|_| {
                let v: [u8; 6] = r.array()?;
                Ok([0, 2, 4].map(|o| i16::from_le_bytes([v[o], v[o + 1]])))
            })
            .collect::<Result<Vec<_>>>()?;
        let n = count(&mut r)?;
        let position_frames = r.bytes(n)?.to_vec();
        if r.pos() != body.len() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!("{} bytes left in track {name}", body.len() - r.pos()),
            });
        }
        let bad_index = |frames: &[u8], keys: usize| frames.iter().any(|&i| usize::from(i) >= keys);
        if bad_index(&rotation_frames, rotations.len())
            || bad_index(&position_frames, positions.len())
        {
            return Err(Error::Invalid {
                offset: 0,
                reason: format!("track {name} indexes past its keys"),
            });
        }
        Ok(Self {
            unknown_id,
            parent: usize::try_from(parent).ok(),
            name,
            unknown_flag,
            fps,
            frames,
            layout,
            unknown_tolerance,
            unknown_words,
            rotations,
            rotation_frames,
            positions,
            position_frames,
        })
    }
}

impl Animation {
    /// The first 3 floats of the `MVEC` chunk (inside `HLPR`, inside the `AOBJ`):
    /// how far the animation carries the character in one loop, in mesh units
    /// (meshes face -y, so walks are (0, -d, 0): 266.67 for `ANIM_HERO_WALK`'s 1 s
    /// loop, 390 for `ANIM_HERO_RUN`'s 0.6 s). The rest of the chunk is not decoded.
    pub fn movement(&self) -> Option<[f32; 3]> {
        let helpers = self
            .objects
            .iter()
            .flat_map(|o| &o.other_chunks)
            .chain(&self.other_chunks)
            .filter(|(tag, _)| tag == b"HLPR");
        for (_, helper) in helpers {
            let Ok((inner, _)) = chunks_with_trailer(helper) else {
                continue;
            };
            if let Some((_, body)) = inner.iter().find(|(tag, _)| tag == b"MVEC") {
                let f = |i: usize| {
                    body.get(i * 4..i * 4 + 4)
                        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                };
                return Some([f(0)?, f(1)?, f(2)?]);
            }
        }
        None
    }

    /// Unpacks and parses a bank entry's data.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let len = data
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or(Error::UnexpectedEof {
                offset: 0,
                wanted: 4,
            })?;
        Self::parse_unpacked(&lzo::decompress(&data[4..], len)?)
    }

    /// Parses the unpacked chunks.
    pub fn parse_unpacked(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let magic: [u8; 4] = r.array()?;
        let header: [u8; 4] = r.array()?;
        if &magic != b">>>>" || &header != b"3DAF" {
            return Err(Error::Invalid {
                offset: 0,
                reason: format!("animation starts {magic:02x?} {header:02x?}"),
            });
        }
        let version = r.u32()?;
        let _copyright = r.c_string()?;
        let mut objects = Vec::new();
        let mut other_chunks = Vec::new();
        let mut unknown_root = (0, 0.0);
        let rest = &data[r.pos()..];
        let (top, trailer) = chunks_with_trailer(rest)?;
        for (tag, body) in top {
            if &tag != b"ANRT" {
                other_chunks.push((tag, body.to_vec()));
                continue;
            }
            let mut a = Reader::new(body);
            let [flag] = a.array()?;
            unknown_root = (flag, f32::from_bits(a.u32()?));
            for (tag, body) in chunks(&body[a.pos()..])? {
                if &tag == b"AOBJ" {
                    objects.push(AnimObject::parse(body)?);
                } else {
                    other_chunks.push((tag, body.to_vec()));
                }
            }
        }
        Ok(Self {
            version,
            unknown_root,
            objects,
            other_chunks,
            unknown_trailer: trailer.to_vec(),
        })
    }
}

impl AnimObject {
    fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Reader::new(body);
        let name = r.c_string()?;
        let unknown = r.u32()?;
        let mut tracks = Vec::new();
        let mut other_chunks = Vec::new();
        for (tag, chunk) in chunks(&body[r.pos()..])? {
            if &tag == b"XSEQ" {
                tracks.push(Track::parse(chunk)?);
            } else {
                other_chunks.push((tag, chunk.to_vec()));
            }
        }
        Ok(Self {
            name,
            unknown,
            tracks,
            other_chunks,
        })
    }
}

type Chunks<'a> = Vec<([u8; 4], &'a [u8])>;

/// Splits `data` into (tag, body) chunks that must fill it exactly.
fn chunks(data: &[u8]) -> Result<Chunks<'_>> {
    match chunks_with_trailer(data)? {
        (out, []) => Ok(out),
        (_, rest) => Err(Error::Invalid {
            offset: data.len() - rest.len(),
            reason: format!("{} bytes after the last chunk", rest.len()),
        }),
    }
}

/// Splits `data` into (tag, body) chunks, returning any tail shorter than a chunk
/// header.
fn chunks_with_trailer(data: &[u8]) -> Result<(Chunks<'_>, &[u8])> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    while data.len() - r.pos() >= 8 {
        let tag: [u8; 4] = r.array()?;
        let len = r.u32()? as usize;
        out.push((tag, r.bytes(len)?));
    }
    Ok((out, &data[r.pos()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A track with a 2-key rotation palette over 3 frames and one constant position.
    fn track() -> Vec<u8> {
        let mut t = Vec::new();
        t.extend_from_slice(&0x7adau32.to_le_bytes());
        t.extend_from_slice(&(-1i32).to_le_bytes());
        t.extend_from_slice(b"Bip01\0");
        t.push(0);
        t.extend_from_slice(&30.0f32.to_le_bytes());
        t.extend_from_slice(&3u32.to_le_bytes());
        t.extend_from_slice(&[1, 3, 1, 0]);
        t.extend_from_slice(&0.001f32.to_le_bytes());
        t.extend_from_slice(&[0, 1, 0, 0x38]);
        t.extend_from_slice(&2u16.to_le_bytes());
        for q in [[0.0f32, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]] {
            q.iter().for_each(|v| t.extend_from_slice(&v.to_le_bytes()));
        }
        t.extend_from_slice(&3u16.to_le_bytes());
        t.extend_from_slice(&[0, 1, 1]);
        t.extend_from_slice(&1u16.to_le_bytes());
        [32767i16, 0, -16384]
            .iter()
            .for_each(|v| t.extend_from_slice(&v.to_le_bytes()));
        t.extend_from_slice(&0u16.to_le_bytes());
        t
    }

    fn animation() -> Vec<u8> {
        let mut obj = b"Test\0".to_vec();
        obj.extend_from_slice(&0u32.to_le_bytes());
        obj.extend(chunk(b"XSEQ", &track()));
        let mut root = vec![1];
        root.extend_from_slice(&0.1f32.to_le_bytes());
        root.extend(chunk(b"AOBJ", &obj));
        let mut data = b">>>>3DAF".to_vec();
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(b"Copyright\0");
        data.extend(chunk(b"ANRT", &root));
        let mut movement = Vec::new();
        [0.0f32, -100.0, 0.0]
            .iter()
            .for_each(|v| movement.extend_from_slice(&v.to_le_bytes()));
        data.extend(chunk(b"HLPR", &chunk(b"MVEC", &movement)));
        data.extend_from_slice(&[0; 4]);
        data
    }

    #[test]
    fn parses_tracks_and_keys() {
        let anim = Animation::parse_unpacked(&animation()).unwrap();
        assert_eq!(anim.version, 100);
        assert_eq!(anim.other_chunks.len(), 1);
        assert_eq!(anim.unknown_trailer, [0; 4]);
        assert_eq!(anim.movement(), Some([0.0, -100.0, 0.0]));
        let t = &anim.objects[0].tracks[0];
        assert_eq!((t.name.as_str(), t.parent, t.frames), ("Bip01", None, 3));
        assert_eq!(t.rotation(0), Some([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(t.rotation(2), Some([0.0, 0.0, 1.0, 0.0]));
        assert_eq!(t.rotation(9), Some([0.0, 0.0, 1.0, 0.0]), "clamped");
        assert_eq!(t.position(1), Some([1.0, 0.0, -16384.0 / 32767.0]));
    }

    #[test]
    fn rejects_out_of_range_indices() {
        let mut data = animation();
        // The third rotation index (last 1 of "0 1 1") becomes 5.
        let at = data.windows(3).position(|w| w == [0, 1, 1]).unwrap();
        data[at + 2] = 5;
        assert!(Animation::parse_unpacked(&data).is_err());
    }
}
