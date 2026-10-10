//! Decoding of DXT1/DXT3/DXT5 (S3TC / BC1–BC3) block-compressed images to RGBA8,
//! written from the public S3TC format description.

/// Bytes per 4x4 block.
pub const fn block_len(format: BlockFormat) -> usize {
    match format {
        BlockFormat::Dxt1 => 8,
        BlockFormat::Dxt3 | BlockFormat::Dxt5 => 16,
    }
}

pub const fn bits_per_pixel(format: BlockFormat) -> usize {
    block_len(format) * 8 / 16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFormat {
    Dxt1,
    Dxt3,
    Dxt5,
}

/// Size in bytes of a `width` x `height` image, rounding up to whole blocks.
pub fn image_len(format: BlockFormat, width: u32, height: u32) -> usize {
    width.div_ceil(4).max(1) as usize * height.div_ceil(4).max(1) as usize * block_len(format)
}

/// Decodes a block-compressed image to tightly packed RGBA8 rows.
/// Returns `None` if `data` is shorter than [`image_len`].
pub fn decode(format: BlockFormat, width: u32, height: u32, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < image_len(format, width, height) {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let blocks_x = width.div_ceil(4).max(1) as usize;
    let mut out = vec![0; w * h * 4];
    for (i, block) in data
        .chunks_exact(block_len(format))
        .take(image_len(format, width, height) / block_len(format))
        .enumerate()
    {
        let texels = match format {
            BlockFormat::Dxt1 => color_block(block, true),
            BlockFormat::Dxt3 => {
                with_alpha(color_block(&block[8..], false), explicit_alpha(&block[..8]))
            }
            BlockFormat::Dxt5 => with_alpha(
                color_block(&block[8..], false),
                interpolated_alpha(&block[..8]),
            ),
        };
        let (bx, by) = (i % blocks_x * 4, i / blocks_x * 4);
        for (t, rgba) in texels.iter().enumerate() {
            let (x, y) = (bx + t % 4, by + t / 4);
            if x < w && y < h {
                out[(y * w + x) * 4..][..4].copy_from_slice(rgba);
            }
        }
    }
    Some(out)
}

fn rgb565(c: u16) -> [u16; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// The 16 texels of a DXT1-style colour block. `allow_transparent` enables DXT1's
/// three-colour-plus-transparent mode when the first colour is not greater.
fn color_block(block: &[u8], allow_transparent: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (a, b) = (rgb565(c0), rgb565(c1));
    let mix = |wa: u16, wb: u16, d: u16| -> [u8; 4] {
        let ch = |i: usize| ((a[i] * wa + b[i] * wb) / d) as u8;
        [ch(0), ch(1), ch(2), 255]
    };
    let palette = if c0 > c1 || !allow_transparent {
        [mix(1, 0, 1), mix(0, 1, 1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [mix(1, 0, 1), mix(0, 1, 1), mix(1, 1, 2), [0, 0, 0, 0]]
    };
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    std::array::from_fn(|t| palette[((indices >> (2 * t)) & 3) as usize])
}

fn explicit_alpha(block: &[u8]) -> [u8; 16] {
    let bits = u64::from_le_bytes(block[..8].try_into().unwrap());
    std::array::from_fn(|t| ((bits >> (4 * t)) & 15) as u8 * 17)
}

fn interpolated_alpha(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let mut table = [0u8; 8];
    table[0] = a0 as u8;
    table[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7 {
            table[i + 1] = (((7 - i as u16) * a0 + i as u16 * a1) / 7) as u8;
        }
    } else {
        for i in 1..5 {
            table[i + 1] = (((5 - i as u16) * a0 + i as u16 * a1) / 5) as u8;
        }
        table[6] = 0;
        table[7] = 255;
    }
    let mut bits = [0u8; 8];
    bits[..6].copy_from_slice(&block[2..8]);
    let bits = u64::from_le_bytes(bits);
    std::array::from_fn(|t| table[((bits >> (3 * t)) & 7) as usize])
}

fn with_alpha(mut texels: [[u8; 4]; 16], alpha: [u8; 16]) -> [[u8; 4]; 16] {
    for (t, a) in texels.iter_mut().zip(alpha) {
        t[3] = a;
    }
    texels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_four_colour_block() {
        // c0 = pure red, c1 = pure blue; indices 0,1,2,3 across the first row.
        let block = [0x00, 0xf8, 0x1f, 0x00, 0b11_10_01_00, 0, 0, 0];
        let out = decode(BlockFormat::Dxt1, 4, 4, &block).unwrap();
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out[4..8], &[0, 0, 255, 255]);
        assert_eq!(&out[8..12], &[170, 0, 85, 255]);
        assert_eq!(&out[12..16], &[85, 0, 170, 255]);
        assert_eq!(&out[16..20], &[255, 0, 0, 255], "second row uses index 0");
    }

    #[test]
    fn dxt1_transparent_mode() {
        // c0 <= c1 selects three colours plus transparent; index 3 is transparent.
        let block = [0x1f, 0x00, 0x00, 0xf8, 0b11, 0, 0, 0];
        let out = decode(BlockFormat::Dxt1, 4, 4, &block).unwrap();
        assert_eq!(&out[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn dxt3_explicit_alpha() {
        let mut block = [0u8; 16];
        block[0] = 0xf0; // texel 0 alpha 0, texel 1 alpha 15
        let out = decode(BlockFormat::Dxt3, 4, 4, &block).unwrap();
        assert_eq!((out[3], out[7]), (0, 255));
    }

    #[test]
    fn dxt5_interpolated_alpha() {
        let mut block = [0u8; 16];
        block[0] = 255;
        block[1] = 0;
        block[2] = 0b010_001_000; // texels 0..2 use alpha indices 0, 1, 2
        block[3] = 0;
        let out = decode(BlockFormat::Dxt5, 4, 4, &block).unwrap();
        assert_eq!((out[3], out[7], out[11]), (255, 0, 218));
    }

    #[test]
    fn small_images_use_whole_blocks() {
        assert_eq!(image_len(BlockFormat::Dxt1, 2, 2), 8);
        assert_eq!(image_len(BlockFormat::Dxt5, 8, 4), 32);
        let out = decode(BlockFormat::Dxt1, 2, 2, &[0x00, 0xf8, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(out.len(), 16);
        assert!(decode(BlockFormat::Dxt1, 8, 8, &[0; 8]).is_none());
    }
}
