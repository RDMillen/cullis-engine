//! Textures in the `GBANK_*` banks of `textures.big` and `frontend.big`.
//! See `docs/formats/texture.md`.

use crate::dxt::{self, BlockFormat};
use crate::reader::Reader;
use crate::{Error, Result, lzo};

/// Bank entry type whose frames are stored one after another, each with its own
/// mip chain. Every other type stores one image (sprite sheets keep their frames as
/// tiles inside it).
pub const FRAME_SEQUENCE_TYPE: u32 = 1;

/// The bank entry's 34-byte info block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureInfo {
    pub width: u16,
    pub height: u16,
    /// 0 for every texture seen.
    pub depth: u16,
    pub frame_width: u16,
    pub frame_height: u16,
    pub frame_count: u16,
    pub format: TextureFormat,
    /// 1 for textures with alpha, 0 otherwise.
    pub alpha_flag: u8,
    pub mip_count: u8,
    /// Size of the top mip level as stored.
    pub top_len: u32,
    /// Bytes taken by the compressed top mip level (header, LZO stream and 3 raw
    /// bytes); 0 when every level is stored uncompressed.
    pub compressed_len: u32,
    pub bits_per_pixel: u8,
    pub raw: [u8; 34],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// `0x1f`, 4 bits per pixel.
    Dxt1,
    /// `0x20`, 8 bits per pixel, used for textures with alpha.
    Dxt3,
    /// `0x23`, 8 bits per pixel; four textures.
    Dxt5,
    /// `0x01`, 32 bits per pixel, stored B, G, R, A.
    Bgra8,
    /// Any other code (`0x18`, 16 bits per pixel, is used once).
    Other(u8),
}

impl TextureFormat {
    fn from_code(code: u8) -> Self {
        match code {
            0x1f => Self::Dxt1,
            0x20 => Self::Dxt3,
            0x23 => Self::Dxt5,
            0x01 => Self::Bgra8,
            other => Self::Other(other),
        }
    }

    fn block(self) -> Option<BlockFormat> {
        match self {
            Self::Dxt1 => Some(BlockFormat::Dxt1),
            Self::Dxt3 => Some(BlockFormat::Dxt3),
            Self::Dxt5 => Some(BlockFormat::Dxt5),
            _ => None,
        }
    }

    /// Bytes for one stored `width` x `height` level, or `None` for unsupported
    /// formats. Block-compressed levels smaller than a 4x4 block are stored at their
    /// bits-per-pixel size (a 2x2 DXT5 level takes 4 bytes), not as a whole block.
    pub fn level_len(self, width: u32, height: u32) -> Option<usize> {
        let (w, h) = (width.max(1) as usize, height.max(1) as usize);
        match self {
            Self::Bgra8 => Some(w * h * 4),
            _ if w < 4 || h < 4 => self
                .block()
                .map(|b| (w * h * dxt::bits_per_pixel(b)).div_ceil(8)),
            _ => self.block().map(|b| dxt::image_len(b, width, height)),
        }
    }
}

impl TextureInfo {
    pub fn parse(info: &[u8]) -> Result<Self> {
        let raw: [u8; 34] = info.try_into().map_err(|_| Error::Invalid {
            offset: 0,
            reason: format!("texture info is {} bytes, expected 34", info.len()),
        })?;
        let u16_at = |o: usize| u16::from_le_bytes([raw[o], raw[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
        Ok(Self {
            width: u16_at(0),
            height: u16_at(2),
            depth: u16_at(4),
            frame_width: u16_at(6),
            frame_height: u16_at(8),
            frame_count: u16_at(10),
            format: TextureFormat::from_code(raw[12]),
            alpha_flag: raw[16],
            mip_count: raw[17],
            top_len: u32_at(20),
            compressed_len: u32_at(24),
            bits_per_pixel: raw[29],
            raw,
        })
    }
}

/// A texture's stored images, still in their stored format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub format: TextureFormat,
    /// Size of each image's top level. For frame sequences this is the frame size.
    pub width: u32,
    pub height: u32,
    /// Slices of a volume texture (rain uses 8); 1 for ordinary textures.
    pub depth: u32,
    /// One image per frame for frame sequences, otherwise a single image.
    pub images: Vec<MipChain>,
}

/// Mip levels, largest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipChain {
    pub levels: Vec<Vec<u8>>,
}

impl Texture {
    /// Unpacks an entry's data. `entry_type` is the bank entry's type.
    pub fn unpack(info: &TextureInfo, entry_type: u32, data: &[u8]) -> Result<Self> {
        let (width, height, images) = if entry_type == FRAME_SEQUENCE_TYPE {
            (info.frame_width, info.frame_height, info.frame_count.max(1))
        } else {
            (info.width, info.height, 1)
        };
        let mut texture = Self {
            format: info.format,
            width: width.into(),
            height: height.into(),
            depth: u32::from(info.depth).max(1),
            images: Vec::new(),
        };
        let lens: Vec<usize> = (0..u32::from(info.mip_count))
            .map(|m| {
                let [w, h] = texture.stored_level_size(m);
                let slices = (texture.depth >> m).max(1) as usize;
                info.format.level_len(w, h).map(|len| len * slices)
            })
            .collect::<Option<_>>()
            .ok_or_else(|| Error::Invalid {
                offset: 0,
                reason: format!("unsupported texture format {:?}", info.format),
            })?;
        let mut r = Reader::new(data);
        for image in 0..images {
            let mut levels = Vec::with_capacity(lens.len());
            for (m, &len) in lens.iter().enumerate() {
                if m == 0 && image == 0 && info.compressed_len != 0 {
                    levels.push(top_level(&mut r, len)?);
                } else {
                    levels.push(r.bytes(len)?.to_vec());
                }
            }
            texture.images.push(MipChain { levels });
        }
        if r.pos() != data.len() {
            return Err(Error::Invalid {
                offset: r.pos(),
                reason: format!(
                    "{} bytes left after the last mip level",
                    data.len() - r.pos()
                ),
            });
        }
        Ok(texture)
    }

    /// Size of mip level `level`.
    pub fn level_size(&self, level: u32) -> [u32; 2] {
        [(self.width >> level).max(1), (self.height >> level).max(1)]
    }

    /// Stored size of mip level `level`: levels are padded to power-of-two sizes
    /// (a 20x20 icon is stored as 32x32), with the image in the top-left corner.
    pub fn stored_level_size(&self, level: u32) -> [u32; 2] {
        let pow2 = |v: u32| v.max(1).next_power_of_two();
        [
            (pow2(self.width) >> level).max(1),
            (pow2(self.height) >> level).max(1),
        ]
    }

    /// Decodes one level of one image to RGBA8, cropped to [`Texture::level_size`].
    /// Volume texture slices come out stacked top to bottom.
    pub fn to_rgba(&self, image: usize, level: u32) -> Result<Vec<u8>> {
        let bytes = self
            .images
            .get(image)
            .and_then(|i| i.levels.get(level as usize))
            .ok_or_else(|| Error::Invalid {
                offset: 0,
                reason: format!("no image {image} level {level}"),
            })?;
        let stored = self.stored_level_size(level);
        let [w, h] = self.level_size(level);
        let slices = (self.depth >> level).max(1) as usize;
        let slice_len = bytes.len() / slices;
        let mut out = Vec::with_capacity(w as usize * h as usize * 4 * slices);
        for slice in bytes.chunks_exact(slice_len.max(1)).take(slices) {
            let full = decode(self.format, stored, slice)?;
            let row = stored[0] as usize * 4;
            for r in full.chunks(row).take(h as usize) {
                out.extend_from_slice(&r[..w as usize * 4]);
            }
        }
        Ok(out)
    }
}

/// The compressed top level: a `u16` length (or `0xffff` and a `u32` length), an LZO1X
/// stream of that length giving all but the last 3 bytes, then those 3 bytes raw.
/// A length of 0 means the whole level follows uncompressed.
fn top_level(r: &mut Reader, len: usize) -> Result<Vec<u8>> {
    let short = u16::from_le_bytes(r.array()?);
    let stream_len = if short == 0xffff {
        r.u32()? as usize
    } else {
        short.into()
    };
    // A zero length means the level did not compress and follows raw.
    if stream_len == 0 {
        return Ok(r.bytes(len)?.to_vec());
    }
    let mut level = lzo::decompress(r.bytes(stream_len)?, len.saturating_sub(3))?;
    level.extend_from_slice(r.bytes(3)?);
    Ok(level)
}

fn decode(format: TextureFormat, [w, h]: [u32; 2], bytes: &[u8]) -> Result<Vec<u8>> {
    let short = || Error::Invalid {
        offset: 0,
        reason: "texture level too short".into(),
    };
    match format {
        TextureFormat::Bgra8 => {
            let len = format.level_len(w, h).unwrap();
            let px = bytes.get(..len).ok_or_else(short)?;
            Ok(px
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect())
        }
        f => {
            let block = f.block().ok_or_else(|| Error::Invalid {
                offset: 0,
                reason: format!("unsupported texture format {f:?}"),
            })?;
            dxt::decode(block, w, h, bytes).ok_or_else(short)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(
        format: u8,
        size: u16,
        frame: u16,
        mips: u8,
        frames: u16,
        compressed_len: u32,
    ) -> TextureInfo {
        let mut raw = [0u8; 34];
        for (i, v) in [size, size, 0, frame, frame, frames].iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        raw[12] = format;
        raw[17] = mips;
        raw[24..28].copy_from_slice(&compressed_len.to_le_bytes());
        TextureInfo::parse(&raw).unwrap()
    }

    #[test]
    fn unpacks_frame_sequence() {
        // 8x8 DXT1 frames with 2 levels: 32 + 8 bytes per frame, 2 frames.
        let i = info(0x1f, 8, 8, 2, 2, 0);
        let data: Vec<u8> = (0..80).collect();
        let t = Texture::unpack(&i, FRAME_SEQUENCE_TYPE, &data).unwrap();
        assert_eq!(t.images.len(), 2);
        assert_eq!(t.images[1].levels[1], (72..80).collect::<Vec<u8>>());
    }

    #[test]
    fn sprite_sheet_is_one_image() {
        // 16x16 sheet of 8x8 frames: one 16x16 DXT1 image (128 bytes), 1 level.
        let i = info(0x1f, 16, 8, 1, 4, 0);
        let t = Texture::unpack(&i, 5, &[0; 128]).unwrap();
        assert_eq!((t.width, t.images.len()), (16, 1));
    }

    #[test]
    fn unpacks_compressed_top_level() {
        // 8x8 DXT1: top level 32 bytes = 29 from LZO + 3 raw, then an 8-byte level.
        let i = info(0x1f, 8, 8, 2, 1, 1);
        let mut data = vec![0, 0, 17 + 29];
        data.extend(0..29u8);
        data.extend([0x11, 0, 0]);
        data[0] = (data.len() - 2) as u8;
        data.extend([200, 201, 202]);
        data.extend([9; 8]);
        let t = Texture::unpack(&i, 0, &data).unwrap();
        let top = &t.images[0].levels[0];
        assert_eq!(top.len(), 32);
        assert_eq!(&top[29..], &[200, 201, 202]);
        assert_eq!(t.images[0].levels[1], [9; 8]);
    }

    #[test]
    fn zero_length_top_level_is_raw() {
        let i = info(0x1f, 4, 4, 1, 1, 10);
        let mut data = vec![0, 0];
        data.extend(1..=8u8);
        let t = Texture::unpack(&i, 0, &data).unwrap();
        assert_eq!(t.images[0].levels[0], (1..=8).collect::<Vec<u8>>());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let i = info(0x1f, 4, 4, 1, 1, 0);
        assert!(Texture::unpack(&i, 0, &[0; 9]).is_err());
    }

    #[test]
    fn pads_to_power_of_two_and_crops() {
        // 3x3 BGRA is stored as 4x4.
        let i = info(0x01, 3, 3, 1, 1, 0);
        let data: Vec<u8> = (0..64).collect();
        let t = Texture::unpack(&i, 0, &data).unwrap();
        assert_eq!(t.stored_level_size(0), [4, 4]);
        let rgba = t.to_rgba(0, 0).unwrap();
        assert_eq!(rgba.len(), 3 * 3 * 4);
        assert_eq!(
            &rgba[12..16],
            &[18, 17, 16, 19],
            "second row starts at stored offset 16"
        );
    }
}
