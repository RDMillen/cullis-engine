//! Creatures placed in a region, each skinned and playing an idle animation.

use std::collections::HashMap;
use std::path::Path;

use eframe::wgpu;
use glam::Mat4;

use crate::model::{self, ModelData, ModelRenderer, Skin};
use crate::skin::{Animator, Clip};
use crate::terrain::CreaturePlacement;

/// One placed creature: one or more skinned meshes (head, torso, legs) playing
/// the same clip.
pub struct Npc {
    pub parts: Vec<(ModelRenderer, Animator)>,
    pub transform: Mat4,
}

impl Npc {
    /// Advances the creature's animation and uploads its vertices.
    pub fn animate(&mut self, queue: &wgpu::Queue, dt: f32) {
        for (renderer, animator) in &mut self.parts {
            renderer.set_vertices(queue, animator.advance(dt));
        }
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

/// The idle a creature plays. Chosen by name, as a stand-in: which animation the
/// game uses is in the creature's `CAppearanceDef` slots, not decoded yet.
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
    let mut clips: HashMap<(u32, &str), Option<Clip>> = HashMap::new();
    let mut npcs = Vec::new();
    for (i, c) in creatures.iter().enumerate() {
        let name = idle_for(&c.definition);
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
            let Some(clip) = clips
                .entry((id, name))
                .or_insert_with(|| {
                    Clip::load(data_dir, name, &skin.skeleton)
                        .map_err(|e| eprintln!("{name}: {e}"))
                        .ok()
                })
                .clone()
            else {
                continue;
            };
            let mut animator = Animator::new(model.vertices.clone(), skin.clone(), vec![clip]);
            let renderer = ModelRenderer::new(device, queue, model);
            // Start each creature at a different point of its loop.
            renderer.set_vertices(queue, animator.advance(i as f32 * 0.37));
            parts.push((renderer, animator));
        }
        if !parts.is_empty() {
            npcs.push(Npc {
                parts,
                transform: c.placement.transform()
                    * Mat4::from_scale(glam::Vec3::splat(scale_for(&c.definition))),
            });
        }
    }
    npcs
}
