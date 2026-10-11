//! Creatures placed in a region, each skinned and playing an idle animation.

use std::collections::HashMap;
use std::path::Path;

use eframe::wgpu;
use glam::Mat4;

use crate::model::{self, ModelData, ModelRenderer, Skin};
use crate::skin::{AnimBank, Animator, Clip};
use crate::terrain::CreaturePlacement;

/// One placed creature: one or more skinned meshes (head, torso, legs) playing
/// the same clip.
pub struct Npc {
    pub parts: Vec<(ModelRenderer, Animator)>,
    pub transform: Mat4,
    /// Seconds since the creature last fidgeted, and when it fidgets next.
    since_fidget: f32,
    next_fidget: f32,
    rng: u64,
}

impl Npc {
    /// Advances the creature's animation and uploads its vertices. It breathes in
    /// its base idle (clip 0) and every so often plays one of its fidgets (the
    /// other clips) once.
    pub fn animate(&mut self, queue: &wgpu::Queue, dt: f32) {
        let Some((_, first)) = self.parts.first() else {
            return;
        };
        let (current, finished, count) = (first.current(), first.finished(), first.clip_count());
        self.since_fidget += dt;
        let next = if current == 0 && count > 1 && self.since_fidget >= self.next_fidget {
            Some(1 + (self.random() as usize) % (count - 1))
        } else if current != 0 && finished {
            self.since_fidget = 0.0;
            self.next_fidget = 6.0 + (self.random() % 9) as f32;
            Some(0)
        } else {
            None
        };
        for (renderer, animator) in &mut self.parts {
            if let Some(clip) = next {
                animator.play(clip);
            }
            renderer.set_vertices(queue, animator.advance(dt));
        }
    }

    fn random(&mut self) -> u64 {
        self.rng = self
            .rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.rng >> 33
    }
}

/// The meshes (with texture swaps) a creature is drawn with. Built-up creatures
/// take one option from each part list; the game picks at random, here the
/// creature's UID picks so a region always looks the same.
fn meshes_for(c: &CreaturePlacement) -> Vec<(u32, Vec<(u32, u32)>)> {
    if c.parts.is_empty() {
        return vec![(c.placement.mesh, Vec::new())];
    }
    c.parts
        .iter()
        .enumerate()
        .filter(|(_, list)| !list.is_empty())
        .map(|(k, list)| {
            let pick = (c.uid.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (8 * k)) as usize % list.len();
            let option = &list[pick];
            (
                option.mesh,
                option
                    .texture_swaps
                    .iter()
                    .map(|&(from, to, _)| (from, to))
                    .collect(),
            )
        })
        .collect()
}

/// An idle by kind, for creatures whose animation table has no `ST_IDLE_SUBTLE`.
fn idle_for(definition: &str) -> &'static str {
    let d = definition.to_ascii_uppercase();
    if d.contains("CHICKEN") {
        "ANIM_CREATURE_CHICKEN_NEW_IDLE"
    } else if ["CHILD", "BOY", "GIRL", "SISTER"]
        .iter()
        .any(|k| d.contains(k))
    {
        "ANIM_VILLAGER_BOY_GENERIC_IDLE_01a"
    } else if d.contains("FEMALE") {
        "ANIM_VILLAGER_FEMALE_GENERIC_IDLE_01a"
    } else {
        "ANIM_VILLAGER_MALE_IDLE_BREATHE_01"
    }
}

/// The young Hero's sister is modelled at adult size like him (see
/// [`model::YOUNG_HERO_SCALE`]).
fn scale_for(definition: &str) -> f32 {
    if definition.contains("YOUNG_SISTER") {
        model::YOUNG_HERO_SCALE
    } else {
        1.0
    }
}

/// Loads and poses every creature (each mesh and clip is decoded once).
pub fn load(
    data_dir: &Path,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    creatures: &[CreaturePlacement],
) -> Vec<Npc> {
    type Key = (u32, Vec<(u32, u32)>);
    let mut meshes: HashMap<Key, Option<(ModelData, Skin)>> = HashMap::new();
    let mut clips: HashMap<(u32, u32), Option<Clip>> = HashMap::new();
    let mut bank = match AnimBank::open(data_dir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("animations: {e}");
            return Vec::new();
        }
    };
    let mut npcs = Vec::new();
    for (i, c) in creatures.iter().enumerate() {
        // The base idle from the creature's table, else one by kind; then its fidgets.
        let Some(base) = c.idles.base.or_else(|| bank.id(idle_for(&c.definition))) else {
            continue;
        };
        let ids: Vec<u32> = std::iter::once(base)
            .chain(c.idles.fidgets.iter().copied())
            .collect();
        let mut parts = Vec::new();
        for key in meshes_for(c) {
            let id = key.0;
            let Some((model, skin)) = meshes
                .entry(key.clone())
                .or_insert_with(|| match model::skinned_model(data_dir, id, &key.1) {
                    Ok((m, Some(s))) => Some((m, s)),
                    Ok((_, None)) => None,
                    Err(e) => {
                        eprintln!("creature mesh {id}: {e}");
                        None
                    }
                })
                .as_ref()
            else {
                continue;
            };
            let part_clips: Vec<Clip> = ids
                .iter()
                .filter_map(|&anim| {
                    clips
                        .entry((id, anim))
                        .or_insert_with(|| {
                            bank.clip(anim, &skin.skeleton)
                                .map_err(|e| eprintln!("animation {anim}: {e}"))
                                .ok()
                        })
                        .clone()
                })
                .collect();
            if part_clips.is_empty() {
                continue;
            }
            let mut animator = Animator::new(model.vertices.clone(), skin.clone(), part_clips);
            let renderer = ModelRenderer::new(device, queue, model);
            // Start each creature at a different point of its loop.
            renderer.set_vertices(queue, animator.advance(i as f32 * 0.37));
            parts.push((renderer, animator));
        }
        if !parts.is_empty() {
            npcs.push(Npc {
                since_fidget: 0.0,
                next_fidget: 3.0 + (c.uid % 11) as f32,
                rng: c.uid | 1,
                parts,
                transform: c.placement.transform()
                    * Mat4::from_scale(glam::Vec3::splat(scale_for(&c.definition))),
            });
        }
    }
    npcs
}
