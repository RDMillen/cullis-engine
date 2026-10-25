//! Loads creatures into the world and draws them: each creature is an
//! `engine_world::creature::Npc` (behaviour and its parts' animators); this
//! holds its parts' skinned meshes and deforms them from the world's poses.

use std::collections::HashMap;

use eframe::wgpu;
use engine_world::anim::Animator;
use engine_world::creature::{Npc, Spec};

use crate::model::{self, ModelData, ModelRenderer, ModelVertex, Skin};
use crate::skin::{AnimBank, Clip, deform};
use crate::terrain::CreaturePlacement;

/// A creature's parts' meshes, drawn for the world creature at the same index.
pub struct CreatureModel {
    pub parts: Vec<PartModel>,
}

/// One part's mesh: its renderer, bind pose and weights, and the posed copy.
pub struct PartModel {
    pub renderer: ModelRenderer,
    bind: Vec<ModelVertex>,
    skin: Skin,
    posed: Vec<ModelVertex>,
}

impl CreatureModel {
    /// Poses each part as `npc`'s matching animator stands and uploads it.
    pub fn update(&mut self, queue: &wgpu::Queue, npc: &Npc) {
        for (part, animator) in self.parts.iter_mut().zip(&npc.parts) {
            deform(&part.skin, &part.bind, animator.matrices(), &mut part.posed);
            part.renderer.set_vertices(queue, &part.posed);
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

/// Loads and poses every creature (each mesh and clip is decoded once): the
/// world's creatures and, at the same indices, their meshes.
pub fn load(
    assets: &engine_core::assets::AssetCache,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    creatures: &[CreaturePlacement],
) -> (Vec<Npc>, Vec<CreatureModel>) {
    type Key = (u32, Vec<(u32, u32)>);
    let mut meshes: HashMap<Key, Option<(ModelData, Skin)>> = HashMap::new();
    let mut clips: HashMap<(u32, u32), Option<Clip>> = HashMap::new();
    let mut bank = match AnimBank::open(assets) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("animations: {e}");
            return (Vec::new(), Vec::new());
        }
    };
    let mut npcs = Vec::new();
    let mut models = Vec::new();
    for (i, c) in creatures.iter().enumerate() {
        // The base idle from the creature's table, else one by kind; then its fidgets.
        let Some(base) = c.idles.base.or_else(|| bank.id(idle_for(&c.definition))) else {
            continue;
        };
        let configs = model::bone_configs(assets.data_dir(), &c.morphs);
        let wanders = !c.scripted && !c.definition.contains("GUARD");
        // Scripted people walk too, when a quest sends them somewhere.
        let walk_id = c.idles.walk.filter(|_| wanders || c.scripted);
        let ids: Vec<u32> = std::iter::once(base)
            .chain(c.idles.fidgets.iter().copied())
            .chain(walk_id)
            .chain(c.idles.hit)
            .collect();
        let mut parts = Vec::new();
        for key in meshes_for(c) {
            let id = key.0;
            let Some((model, skin)) = meshes
                .entry(key.clone())
                .or_insert_with(|| match model::skinned_model(assets, id, &key.1) {
                    Ok((m, Some(s))) => Some((m, s)),
                    Ok((_, None)) => None,
                    Err(e) => {
                        log::warn!("creature mesh {id}: {e}");
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
                            bank.clip(anim, &skin.rig.skeleton)
                                .map_err(|e| log::warn!("animation {anim}: {e}"))
                                .ok()
                        })
                        .clone()
                })
                .collect();
            if part_clips.is_empty() {
                continue;
            }
            let mut skin = skin.clone();
            skin.rig.set_morph(&configs);
            let mut animator = Animator::new(skin.rig.clone(), part_clips);
            // Start each creature at a different point of its loop.
            animator.advance(i as f32 * 0.37);
            let renderer = ModelRenderer::new(device, queue, model);
            parts.push((
                PartModel {
                    renderer,
                    bind: model.vertices.clone(),
                    skin,
                    posed: Vec::new(),
                },
                animator,
            ));
        }
        // Only if every part loaded every clip can the walk be relied on.
        let complete = parts.iter().all(|(_, a)| a.clip_count() == ids.len());
        let (meshes, animators): (Vec<PartModel>, Vec<Animator>) = parts.into_iter().unzip();
        let spec = Spec {
            definition: c.definition.clone(),
            uid: c.uid,
            script_name: c.script_name.clone(),
            position: c.placement.position,
            forward: c.placement.forward,
            transform: c.placement.transform(),
            strolls: wanders,
            has_walk: walk_id.is_some() && complete,
            has_hit: c.idles.hit.is_some() && complete,
            moods: c.idles.moods.clone(),
            slots: c.slots.clone(),
            parts: animators,
        };
        if let Some(mut npc) = Npc::new(spec) {
            let mut model = CreatureModel { parts: meshes };
            model.update(queue, &npc);
            npc.hidden = false;
            npcs.push(npc);
            models.push(model);
        }
    }
    (npcs, models)
}
