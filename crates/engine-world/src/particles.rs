//! Particle effects: which emitters stand where, their layers' readings, and
//! the particles they give off, born, moved and aged each tick near the
//! camera. Drawing them is the renderer's business.
//!
//! Texture, colours over life and blending come from the emitter data; birth
//! rate, lifetime and sizes are readings not yet confirmed, and the starting
//! motion is a stand-in by eye (see `docs/formats/particles.md`).

use std::collections::HashMap;

use engine_core::assets::{Archive, AssetCache};
use fable_data::particles::{Emitter as EmitterData, SpriteLayer};
use glam::Vec3;

/// Which emitter: by name (a mesh's `CREATEPARTICLE` dummy) or by its number
/// in `particles.h`, which is its bank entry id (attachers, chimney smoke).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EmitterRef {
    Name(String),
    Id(u32),
}

/// Emitters this far from the camera's focus are not simulated (world units).
const REACH: f32 = 50.0;
/// Most particles a layer keeps at once.
const MAX_PER_LAYER: usize = 256;
/// Metres a second per unit of a layer's launch speed (by eye: a fireplace
/// flame's 6.4 rises about half a metre in its 0.8 s).
const SPEED_SCALE: f32 = 0.1;

/// A live particle.
#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Seconds since birth, and how long it lives.
    pub age: f32,
    pub life: f32,
}

impl Particle {
    /// How far through its life it is (0..1).
    pub fn progress(&self) -> f32 {
        (self.age / self.life).clamp(0.0, 1.0)
    }
}

/// One layer of an emitter (flame, smoke) and its particles.
#[derive(Debug, Clone)]
pub struct Layer {
    pub look: SpriteLayer,
    /// Its sprite: an index into [`ParticleSystem::textures`].
    pub texture: usize,
    pub particles: Vec<Particle>,
    /// Particles owed by the birth rate, carried between ticks, and whether
    /// its starting burst has been born.
    owed: f32,
    started: bool,
}

/// An emitter standing in the region.
#[derive(Debug, Clone)]
pub struct Emitter {
    pub position: Vec3,
    pub layers: Vec<Layer>,
}

/// The region's particle effects.
#[derive(Debug, Clone)]
pub struct ParticleSystem {
    pub emitters: Vec<Emitter>,
    /// The sprites the layers use (`GBANK_MAIN_PC` ids).
    pub textures: Vec<u32>,
    rng: u64,
}

impl ParticleSystem {
    /// The emitters named at `placed` (emitter, world position), read from
    /// `Misc/pc/effects.big`. Emitters that are missing or do not parse are
    /// left out with a message.
    pub fn load(
        assets: &AssetCache,
        placed: &[(EmitterRef, Vec3)],
    ) -> Result<Self, engine_core::Error> {
        let mut assets = assets.clone();
        let index = assets.index(Archive::Effects)?;
        let mut parsed: HashMap<EmitterRef, Option<Vec<SpriteLayer>>> = HashMap::new();
        let mut textures: Vec<u32> = Vec::new();
        let mut emitters = Vec::new();
        for (which, position) in placed {
            let name = match which {
                EmitterRef::Name(n) => n.clone(),
                EmitterRef::Id(id) => format!("#{id}"),
            };
            let looks = parsed.entry(which.clone()).or_insert_with(|| {
                // Dummies name emitters in any case; the bank's names are
                // upper case.
                let id = match which {
                    EmitterRef::Name(n) => index.by_name(&n.to_ascii_uppercase())?.id,
                    EmitterRef::Id(id) => *id,
                };
                let (_, data) = assets.read(Archive::Effects, id).ok()?;
                let emitter = EmitterData::parse(&data)
                    .map_err(|e| log::warn!("particles {name}: {e}"))
                    .ok()?;
                // The header's first float over 10 scales speeds and sizes:
                // 1 for the fireplace (as drawn and checked), 4.8 for chimney
                // smoke. A guess.
                let scale = (emitter.unknown_floats()[0] / 10.0).clamp(0.1, 10.0);
                Some(
                    emitter
                        .layers
                        .iter()
                        // Layers moved by undecoded components (fireflies'
                        // orbits, splines, attractors) would sit still: left
                        // out until those are decoded.
                        .filter(|l| {
                            !l.components.iter().any(|c| {
                                matches!(
                                    c.class.as_str(),
                                    "CPSCOrbit" | "CPSCSpline" | "CPSCAttractor"
                                )
                            })
                        })
                        .filter_map(|l| l.sprite_layer())
                        .map(|mut l| {
                            l.speed *= scale;
                            l.size = l.size.map(|s| s * scale);
                            l
                        })
                        .collect(),
                )
            });
            let Some(looks) = looks else {
                log::info!("particles: no emitter {name}");
                continue;
            };
            let layers = looks
                .iter()
                .map(|look| {
                    let id = look.sprite.texture;
                    let k = textures.iter().position(|&t| t == id).unwrap_or_else(|| {
                        textures.push(id);
                        textures.len() - 1
                    });
                    Layer {
                        look: *look,
                        texture: k,
                        particles: Vec::new(),
                        owed: 0.0,
                        started: false,
                    }
                })
                .collect();
            emitters.push(Emitter {
                position: *position,
                layers,
            });
        }
        Ok(Self {
            emitters,
            textures,
            rng: 0x2545_f491_4f6c_dd1d,
        })
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Moves time on `dt` seconds for the emitters near `focus`: particles are
    /// born, rise and die.
    pub fn update(&mut self, dt: f32, focus: Vec3) {
        let mut emitters = std::mem::take(&mut self.emitters);
        for emitter in &mut emitters {
            let near = emitter.position.distance(focus) < REACH;
            for layer in &mut emitter.layers {
                let look = layer.look;
                for p in &mut layer.particles {
                    p.velocity.z -= look.fall * dt;
                    p.position += p.velocity * dt;
                    p.age += dt;
                }
                layer.particles.retain(|p| p.age < p.life);
                if !near {
                    layer.owed = 0.0;
                    layer.started = false;
                    continue;
                }
                let births = look.births;
                let most = MAX_PER_LAYER;
                let mut due = 0;
                if births.single() {
                    // One particle kept alive: a candle's flame, a lamp's glow.
                    due = usize::from(layer.particles.is_empty());
                } else {
                    if !layer.started {
                        due += births.burst as usize;
                    }
                    layer.owed += births.per_second() * dt;
                    due += layer.owed as usize;
                    layer.owed = layer.owed.fract();
                }
                layer.started = true;
                let room = most.saturating_sub(layer.particles.len());
                for _ in 0..due.min(room) {
                    let p = self.born(emitter.position, &look);
                    layer.particles.push(p);
                }
            }
        }
        self.emitters = emitters;
    }

    /// A new particle of `look` at `at`: launched at its speed in a direction
    /// tilted from straight up by up to its spread (radians, a reading), living
    /// its lifetime, both varied a little.
    fn born(&mut self, at: Vec3, look: &SpriteLayer) -> Particle {
        let (a, b, c, d) = (self.random(), self.random(), self.random(), self.random());
        let tilt = look.spread * a;
        let turn = std::f32::consts::TAU * b;
        let direction = Vec3::new(tilt.sin() * turn.cos(), tilt.sin() * turn.sin(), tilt.cos());
        Particle {
            position: at,
            velocity: direction * look.speed * SPEED_SCALE * (0.7 + 0.6 * c),
            age: 0.0,
            life: look.lifetime.max(0.05) * (0.8 + 0.4 * d),
        }
    }
}

/// A particle's colour at `t` (0..1 of its life), linear RGB and alpha, from
/// its birth, middle and death keys. A black middle key (torches, candles,
/// smoke) is taken as no middle colour: the colour runs birth to death while
/// the alpha still passes through the middle. `doubled` reads the keys with
/// 128 as 1.0, the old doubled-modulate convention (smoke's grey is 127,
/// which then leaves its sprite unchanged).
pub fn colour_at(keys: &[[u8; 4]; 3], t: f32, doubled: bool) -> [f32; 4] {
    let unit = if doubled { 127.5 } else { 255.0 };
    let f = |k: [u8; 4]| k.map(|c| f32::from(c) / unit);
    let (a, b, c) = (f(keys[0]), f(keys[1]), f(keys[2]));
    let mix = |x: [f32; 4], y: [f32; 4], s: f32| -> [f32; 4] {
        std::array::from_fn(|i| x[i] + (y[i] - x[i]) * s)
    };
    let through = if t < 0.5 {
        mix(a, b, t * 2.0)
    } else {
        mix(b, c, t * 2.0 - 1.0)
    };
    let mut gamma = through;
    if keys[1][..3] == [0, 0, 0] {
        let straight = mix(a, c, t);
        gamma[..3].copy_from_slice(&straight[..3]);
    }
    [
        gamma[0].powf(2.2),
        gamma[1].powf(2.2),
        gamma[2].powf(2.2),
        gamma[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_run_birth_to_death() {
        let keys = [[255, 255, 255, 255], [0, 0, 0, 255], [255, 0, 0, 0]];
        assert_eq!(colour_at(&keys, 0.0, false), [1.0, 1.0, 1.0, 1.0]);
        let end = colour_at(&keys, 1.0, false);
        assert_eq!(
            (end[0], end[1], end[3]),
            (1.0, 0.0, 0.0),
            "the unused black middle is skipped"
        );
        let keys = [[0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 0]];
        assert_eq!(
            colour_at(&keys, 0.5, false)[3],
            1.0,
            "through a used middle"
        );
        let smoke = [[127, 127, 127, 55], [0, 0, 0, 25], [127, 127, 127, 0]];
        let mid = colour_at(&smoke, 0.5, true);
        assert!(
            mid[0] > 0.9 && (mid[3] - 25.0 / 127.5).abs() < 1e-4,
            "{mid:?}"
        );
    }
}
