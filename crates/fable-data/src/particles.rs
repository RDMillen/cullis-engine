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

/// How a sprite layer looks and moves, for drawing it: the decoded fields and,
/// where marked, readings that are not confirmed (see `particles.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteLayer {
    pub sprite: Sprite,
    /// Mode 2 (smoke) blends by alpha; 3 and 4 (flames, glows: sprites drawn on
    /// black with no alpha) add. A reading of `Sprite::unknown_mode`.
    pub additive: bool,
    /// Seconds a particle lives (`CPSCUpdateNormal` offset 36; 0.8 for a
    /// fireplace flame, 5 for chimney smoke). Not confirmed.
    pub lifetime: f32,
    /// Births (`CPSCEmitterGeneric` offsets 47, 55, 82, 13; most of the
    /// emitter's numbers are fixed-point with 1.0 = 1024). Not confirmed.
    pub births: Births,
    /// Launch speed (offset 43, 1.0 = 1024: 6.4 for a fireplace flame, 1.6
    /// for chimney smoke, 0 for dust and fireflies) and the spread about
    /// straight up (offset 51: 0.5 in most). Units not known.
    pub speed: f32,
    pub spread: f32,
    /// Size at birth and death in thousandths (`CPSCRenderSprite` offsets 45
    /// and 49; 248 and 0 for a fireplace flame). Not confirmed.
    pub size: [f32; 2],
    /// `CPSCUpdateNormal` offset 100 (−0.3 for a fireplace, 0.68 for a torch's
    /// flame): taken as an acceleration, negative upwards. Not confirmed.
    pub fall: f32,
}

/// How a layer's particles are born, as read from `CPSCEmitterGeneric`. The
/// readings fit the emitters compared (fires, smoke, candles, lamps, blood,
/// sparks) but are not confirmed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Births {
    /// Offset 47 as an integer: particles born a second (82 for a fireplace
    /// flame), or 0 when offset 55 gives the rate.
    pub rate: u32,
    /// Offset 55: the rate where offset 47 is 0 (41 for chimney smoke).
    pub count: u32,
    /// Offset 82 (15 for a fireplace flame, 41 for chimney smoke): perhaps the
    /// most alive at once; capping fires to it left them too sparse, so it is
    /// not used.
    pub most: u32,
    /// Particles born at once when the emitter starts (offset 13: 20 for
    /// sword sparks, 10 for blood, 0 for steady emitters).
    pub burst: u32,
}

impl Births {
    /// A single particle kept alive (count and most both 0: candles, lamp
    /// glows, each firefly).
    pub fn single(&self) -> bool {
        self.count == 0 && self.most == 0
    }

    /// Particles born a second, for steady emitters.
    pub fn per_second(&self) -> f32 {
        if self.rate > 0 {
            self.rate as f32
        } else {
            self.count as f32
        }
    }
}

impl Layer {
    /// The layer's look and motion, if it draws sprites.
    pub fn sprite_layer(&self) -> Option<SpriteLayer> {
        let find = |class: &str| self.components.iter().find(|c| c.class == class);
        let sprite = find("CPSCRenderSprite")?.sprite()?;
        let s = &find("CPSCRenderSprite")?.data;
        let emitter = &find("CPSCEmitterGeneric")?.data;
        let update = &find("CPSCUpdateNormal")?.data;
        let u32_at = |d: &[u8], at: usize| {
            d.get(at..at + 4)
                .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
        };
        let f32_at = |d: &[u8], at: usize| f32::from_bits(u32_at(d, at));
        Some(SpriteLayer {
            sprite,
            additive: sprite.unknown_mode != 2,
            lifetime: f32_at(update, 36),
            births: Births {
                rate: u32_at(emitter, 47),
                count: u32_at(emitter, 55),
                most: u32_at(emitter, 82),
                burst: u32_at(emitter, 13),
            },
            speed: u32_at(emitter, 43) as f32 / 1024.0,
            spread: u32_at(emitter, 51) as f32 / 1024.0,
            size: [u32_at(s, 45) as f32 / 1000.0, u32_at(s, 49) as f32 / 1000.0],
            fall: f32_at(update, 100),
        })
    }
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
    /// The three floats in the header (12, 18, 12 for `CANDLE_FLAME`; 10, 15,
    /// 10 for `FIREPLACE_LOGS_01`; 48, 60, 48 for `CHIMNEY_SMOKE_01` and
    /// `TORCHFIRE_01`). Meaning not known; the viewer takes the first, over
    /// 10, as a scale for speeds and sizes.
    pub fn unknown_floats(&self) -> [f32; 3] {
        std::array::from_fn(|i| {
            let at = 7 + 4 * i;
            f32::from_le_bytes(self.unknown_header[at..at + 4].try_into().unwrap())
        })
    }

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

/// An emitter an entity carries (`CParticleAttacherDef`): which, and at which
/// helper of its mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attachment {
    /// The emitter's number in `particles.h`, which is its bank entry id
    /// (`LAMPGLOW` = 255).
    pub emitter: u32,
    /// Byte offset of the helper's name in `names.bin` ([`crate::defs::Names::at`]):
    /// `FLARE_POINT`, `LIGHT_POINT`, `EMITTER_LOWERARM`; `0xffffffff` for none
    /// (taken as the mesh's origin).
    pub helper_name: u32,
    /// 10.0 or 20.0 (a distance?).
    pub unknown_f32: f32,
    /// 0 on lamps, 1 on creatures.
    pub unknown_flag: u8,
}

/// Field of `CParticleAttacherDef` holding the attachments.
const ATTACHMENTS_FIELD: u32 = 0x3696_f44d;

/// The attachments of a `CParticleAttacherDef` record: `01 00 01`, the field
/// hash, a u32 count, then 13 bytes each (u32 emitter, u32 helper name offset,
/// f32, u8).
pub fn parse_attachments(record: &[u8]) -> Result<Vec<Attachment>> {
    let mut r = Reader::new(record);
    let _header: [u8; 3] = r.array()?;
    let field = r.u32()?;
    if field != ATTACHMENTS_FIELD {
        return Err(invalid(&r, format!("field {field:08x}")));
    }
    let count = r.u32()?;
    if count > 64 {
        return Err(invalid(&r, format!("{count} attachments")));
    }
    (0..count)
        .map(|_| {
            Ok(Attachment {
                emitter: r.u32()?,
                helper_name: r.u32()?,
                unknown_f32: f32::from_bits(r.u32()?),
                unknown_flag: r.array::<1>()?[0],
            })
        })
        .collect()
}

/// Field of `CSmokeGeneratorDef` records holding the emitter's number
/// (`CHIMNEY_SMOKE_01` = 10 on 318 of 323 buildings, `CAMPFIRE_01` = 64 on the
/// rest). The record's other field, `0x54a805b2`, is 32.0 in all.
pub const SMOKE_EMITTER_FIELD: u32 = 0x76e5_195b;

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
    fn parses_attachments() {
        let mut d = vec![1, 0, 1];
        d.extend_from_slice(&ATTACHMENTS_FIELD.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&255u32.to_le_bytes());
        d.extend_from_slice(&0x5a005u32.to_le_bytes());
        d.extend_from_slice(&10.0f32.to_le_bytes());
        d.push(0);
        let a = parse_attachments(&d).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].emitter, a[0].helper_name), (255, 0x5a005));
        assert_eq!(a[0].unknown_f32, 10.0);
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
