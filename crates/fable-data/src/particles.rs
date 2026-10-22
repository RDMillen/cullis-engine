//! Particle emitters: `Misc/pc/effects.big`, bank `PARTICLE_MAIN_PC`, one entry
//! per emitter (`TORCHFIRE_01`, `FIREPLACE_LOGS_01`; numbered in
//! `Defs/RetailHeaders/pc/particles.h`). An emitter is layers, each built from
//! components (`CPSCRenderSprite`, `CPSCEmitterGeneric`, `CPSCUpdateNormal`,
//! …). See `docs/formats/particles.md`.

use crate::reader::Reader;
use crate::{Error, Result};

/// One emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    /// 100 in every emitter.
    pub version: u32,
    /// As written by its author (`Fireplace_Logs_01`); the bank entry's name is
    /// the upper-case form.
    pub name: String,
    /// The 36 bytes after the name: 4 zero bytes, `01 00 01`, three floats
    /// (12, 18, 12 for `CANDLE_FLAME`; 10, 15, 10 for `FIREPLACE_LOGS_01`),
    /// 8 zero bytes, a u32 (0 or 2), 5 bytes of flags.
    pub unknown_header: [u8; 36],
    pub layers: Vec<Layer>,
}

/// A layer: a set of particles drawn and moved one way (`flame`, `smoke`).
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// 14 bytes: `01 01`, then three floats (1.0 in every layer seen).
    pub unknown_header: [u8; 14],
    pub components: Vec<Component>,
}

/// A component: its class name and its data (starting with the class's hash).
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub class: String,
    pub data: Vec<u8>,
}

/// Data size of the classes whose size is fixed (counting the 4-byte class
/// hash); the others are sized by where the next component starts.
fn fixed_size(class: &str) -> Option<usize> {
    Some(match class {
        "CPSCRenderSprite" => 109,
        "CPSCUpdateNormal" => 162,
        "CPSCRenderMesh" => 133,
        "CPSCLight" => 77,
        "CPSCDecalRenderer" => 114,
        _ => return None,
    })
}

/// `CPSCRenderSprite`'s decoded fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    /// `GBANK_MAIN_PC` texture id (`SPRITE_FLAMELICK_02`), and a second one
    /// (0 in most).
    pub texture: u32,
    pub texture_2: u32,
    /// Colours over a particle's life (birth, middle, death) as RGBA.
    pub colours: [[u8; 4]; 3],
    /// 3 or 4 in fires and smoke (a blend mode?).
    pub unknown_mode: u32,
}

impl Component {
    /// The sprite fields, for a `CPSCRenderSprite`.
    pub fn sprite(&self) -> Option<Sprite> {
        if self.class != "CPSCRenderSprite" || self.data.len() < 29 {
            return None;
        }
        let d = &self.data;
        let u32_at = |at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap());
        // Stored blue, green, red, alpha.
        let colour = |at: usize| [d[at + 2], d[at + 1], d[at], d[at + 3]];
        Some(Sprite {
            texture: u32_at(5),
            texture_2: u32_at(9),
            colours: [colour(13), colour(17), colour(21)],
            unknown_mode: u32_at(25),
        })
    }
}

impl Emitter {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.u32()?;
        let name = r.c_string()?;
        let unknown_header = r.array()?;
        let layer_count = r.u32()?;
        if layer_count > 64 {
            return Err(invalid(&r, format!("{layer_count} layers")));
        }
        let mut layers = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            let name = r.c_string()?;
            let unknown_header = r.array()?;
            let count = r.u32()?;
            if count > 16 {
                return Err(invalid(&r, format!("{count} components")));
            }
            let mut components = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let class = r.c_string()?;
                if !class.starts_with("CPSC") {
                    return Err(invalid(&r, format!("component class {class:?}")));
                }
                let size = match fixed_size(&class) {
                    Some(n) => n,
                    None => next_component(data, r.pos() + 4).unwrap_or(data.len()) - r.pos(),
                };
                let data = r.bytes(size)?.to_vec();
                components.push(Component { class, data });
            }
            layers.push(Layer {
                name,
                unknown_header,
                components,
            });
        }
        if r.pos() != data.len() {
            return Err(invalid(
                &r,
                format!("{} bytes after the layers", data.len() - r.pos()),
            ));
        }
        Ok(Self {
            version,
            name,
            unknown_header,
            layers,
        })
    }
}

/// Where the next component's data ends: the start of the next class name,
/// less the next layer's header if one comes between. Used for the classes
/// without a fixed size.
fn next_component(data: &[u8], from: usize) -> Option<usize> {
    let at = data[from..].windows(4).position(|w| w == b"CPSC")? + from;
    // A component count (u32) right before the name starts a layer: its name,
    // 14 header bytes and the count come before it.
    let count = u32::from_le_bytes(data.get(at - 4..at)?.try_into().ok()?);
    if count < 16 && count > 0 {
        let header_end = at - 4 - 14;
        let name_end = header_end - 1;
        let name_start = data[..name_end]
            .iter()
            .rposition(|&b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b' '))
            .map_or(0, |p| p + 1);
        return Some(name_start);
    }
    Some(at)
}

fn invalid(r: &Reader, reason: String) -> Error {
    Error::Invalid {
        offset: r.pos(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter() -> Vec<u8> {
        let mut d = 100u32.to_le_bytes().to_vec();
        d.extend_from_slice(b"Test\0");
        d.extend_from_slice(&[0; 36]);
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"flame\0");
        d.extend_from_slice(&[1; 14]);
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"CPSCRenderSprite\0");
        let mut sprite = vec![0u8; 109];
        sprite[5..9].copy_from_slice(&4422u32.to_le_bytes());
        // Birth colour: orange (blue first), alpha 35.
        sprite[13..17].copy_from_slice(&[0x00, 0xd7, 0xff, 0x23]);
        sprite[25..29].copy_from_slice(&4u32.to_le_bytes());
        d.extend_from_slice(&sprite);
        d
    }

    #[test]
    fn parses_layers_and_sprites() {
        let e = Emitter::parse(&emitter()).unwrap();
        assert_eq!((e.version, e.name.as_str()), (100, "Test"));
        assert_eq!(e.layers.len(), 1);
        assert_eq!(e.layers[0].name, "flame");
        let sprite = e.layers[0].components[0].sprite().unwrap();
        assert_eq!(sprite.texture, 4422);
        assert_eq!(sprite.colours[0], [0xff, 0xd7, 0x00, 0x23]);
        assert_eq!(sprite.unknown_mode, 4);
        let mut bad = emitter();
        bad.push(0);
        assert!(Emitter::parse(&bad).is_err(), "trailing bytes");
    }
}
