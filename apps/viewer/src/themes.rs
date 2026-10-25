//! Theme textures for the terrain: every theme's base and cliff textures, resampled
//! to one size with mip chains, ready for texture arrays.

use crate::model::{Image, TextureBank};
use crate::terrain::ThemeLayer;

/// Size of every array layer.
pub const LAYER_SIZE: u32 = 512;

/// One layer's mip chain of RGBA8 images, largest first.
pub type Mips = Vec<Vec<u8>>;

pub struct ThemeTextures {
    pub base: Vec<Mips>,
    pub cliff: Vec<Mips>,
}

/// Loads base and cliff textures for every layer; missing ones become grey.
pub fn load(
    assets: &engine_core::assets::AssetCache,
    layers: &[ThemeLayer],
) -> Result<ThemeTextures, Box<dyn std::error::Error>> {
    let mut bank = TextureBank::open(assets);
    let mut get = |id: u32, what: &str, name: &str| -> Mips {
        let level0 = match id {
            0 => grey(),
            id => match bank.load(id) {
                Ok(img) => resample(&img, LAYER_SIZE),
                Err(e) => {
                    log::warn!("{what} texture {id} for theme {name}: {e}");
                    grey()
                }
            },
        };
        mips(level0, LAYER_SIZE)
    };
    let base = layers
        .iter()
        .map(|l| get(l.base, "base", &l.name))
        .collect();
    let cliff = layers
        .iter()
        .map(|l| get(l.cliff, "cliff", &l.name))
        .collect();
    Ok(ThemeTextures { base, cliff })
}

/// Loads textures by id as array layers (missing ones grey).
pub fn load_ids(
    assets: &engine_core::assets::AssetCache,
    ids: &[u32],
) -> Result<Vec<Mips>, Box<dyn std::error::Error>> {
    let mut bank = TextureBank::open(assets);
    Ok(ids
        .iter()
        .map(|&id| {
            let level0 = match bank.load(id) {
                Ok(img) => resample(&img, LAYER_SIZE),
                Err(e) => {
                    log::warn!("texture {id}: {e}");
                    grey()
                }
            };
            mips(level0, LAYER_SIZE)
        })
        .collect())
}

fn grey() -> Vec<u8> {
    [128u8, 128, 128, 255].repeat((LAYER_SIZE * LAYER_SIZE) as usize)
}

/// Nearest-neighbour resample to `size` x `size`.
fn resample(img: &Image, size: u32) -> Vec<u8> {
    if img.width == size && img.height == size {
        return img.rgba.clone();
    }
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        let sy = (y * img.height / size) as usize;
        for x in 0..size {
            let sx = (x * img.width / size) as usize;
            let i = (sy * img.width as usize + sx) * 4;
            out.extend_from_slice(&img.rgba[i..i + 4]);
        }
    }
    out
}

/// Box-filtered mip chain down to 1x1.
fn mips(level0: Vec<u8>, size: u32) -> Mips {
    let mut chain = vec![level0];
    let mut s = size as usize;
    while s > 1 {
        let prev = chain.last().unwrap();
        let h = s / 2;
        let mut next = Vec::with_capacity(h * h * 4);
        for y in 0..h {
            for x in 0..h {
                for c in 0..4 {
                    let at = |dx: usize, dy: usize| {
                        u32::from(prev[((2 * y + dy) * s + 2 * x + dx) * 4 + c])
                    };
                    next.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
                }
            }
        }
        chain.push(next);
        s = h;
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_halves_to_one_pixel() {
        let chain = mips(vec![200; 4 * 4 * 4], 4);
        assert_eq!(chain.iter().map(Vec::len).collect::<Vec<_>>(), [64, 16, 4]);
        assert_eq!(chain[2], [200; 4]);
    }

    #[test]
    fn resamples_nearest() {
        let img = Image {
            width: 2,
            height: 1,
            rgba: vec![1, 1, 1, 1, 9, 9, 9, 9],
        };
        let out = resample(&img, 4);
        assert_eq!(&out[0..4], &[1; 4]);
        assert_eq!(&out[12..16], &[9; 4]);
        assert_eq!(
            &out[16..20],
            &[1; 4],
            "second row repeats the only source row"
        );
    }
}
