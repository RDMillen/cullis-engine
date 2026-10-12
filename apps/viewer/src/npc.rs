//! Creatures placed in a region, each skinned and playing an idle animation.

use std::collections::HashMap;
use std::path::Path;

use eframe::wgpu;
use glam::Mat4;

use crate::model::{self, ModelData, ModelRenderer, Skin};
use crate::skin::{AnimBank, Animator, Clip};
use crate::terrain::CreaturePlacement;
use crate::walk::{self, HeightGrid, WalkGrid};
use glam::Vec2;

/// One placed creature: one or more skinned meshes (head, torso, legs) playing
/// the same clip.
pub struct Npc {
    pub parts: Vec<(ModelRenderer, Animator)>,
    pub transform: Mat4,
    /// Its `ScriptName` (its role), for what it says.
    pub script_name: Option<String>,
    /// Sent away by a quest: not drawn, not in the way.
    pub hidden: bool,
    /// For lines that come in male and female versions.
    pub female: bool,
    /// Its flinch when hit, if it has one.
    hit_clip: Option<usize>,
    /// The clip it stands in (its base idle, or a quest's mood).
    base: usize,
    /// Its idle's variants by modifier (bank ids), for [`Npc::set_mood`].
    moods: Vec<(String, u32)>,
    /// Seconds since the creature last fidgeted, and when it fidgets next.
    since_fidget: f32,
    next_fidget: f32,
    rng: u64,
    /// Clips 1..=fidgets are fidgets; the walk, if any, comes after them.
    fidgets: usize,
    walk: Option<Wander>,
}

/// A creature that strolls around where it was placed. Stand-in behaviour: the
/// game's creature AI is not decoded.
struct Wander {
    body: walk::Hero,
    home: Vec2,
    clip: usize,
    /// World units per second (the walk clip's own speed).
    speed: f32,
    target: Option<Vec2>,
    /// Seconds standing since the last stroll, and when to stroll next.
    resting: f32,
    next_stroll: f32,
    /// Seconds without progress while walking.
    stuck: f32,
}

/// How far from its placement a creature strolls.
const WANDER_RADIUS: f32 = 5.0;

impl Npc {
    /// Advances the creature and uploads its vertices. Standing, it breathes in
    /// its base idle (clip 0) and every so often plays one of its fidgets once;
    /// a wandering creature now and then walks to a spot near its placement.
    pub fn animate(
        &mut self,
        queue: &wgpu::Queue,
        dt: f32,
        layers: &[WalkGrid],
        heights: Option<&HeightGrid>,
        blocked: &dyn Fn(Vec2) -> bool,
    ) {
        let Some((_, first)) = self.parts.first() else {
            return;
        };
        let (current, finished) = (first.current(), first.finished());
        let next = match (self.walk.is_some(), heights) {
            (true, Some(heights)) => self.wander(dt, current, finished, layers, heights, blocked),
            _ => self.fidget(dt, current, finished),
        };
        for (renderer, animator) in &mut self.parts {
            if let Some(clip) = next {
                animator.play(clip);
            }
            renderer.set_vertices(queue, animator.advance(dt));
        }
    }

    /// The clip to switch to while standing: a fidget now and then, back to the
    /// base idle when it ends.
    fn fidget(&mut self, dt: f32, current: usize, finished: bool) -> Option<usize> {
        self.since_fidget += dt;
        if current == self.base && self.fidgets > 0 && self.since_fidget >= self.next_fidget {
            Some(1 + (self.random() as usize) % self.fidgets)
        } else if current != 0 && finished {
            self.since_fidget = 0.0;
            self.next_fidget = 6.0 + (self.random() % 9) as f32;
            Some(self.base)
        } else {
            None
        }
    }

    fn wander(
        &mut self,
        dt: f32,
        current: usize,
        finished: bool,
        layers: &[WalkGrid],
        heights: &HeightGrid,
        blocked: &dyn Fn(Vec2) -> bool,
    ) -> Option<usize> {
        let roll = self.random();
        let base = self.base;
        let w = self.walk.as_mut().unwrap();
        if let Some(goal) = w.target {
            let to = goal - w.body.position.truncate();
            if to.length() < 0.3 || w.stuck > 1.0 {
                w.target = None;
                w.resting = 0.0;
                w.next_stroll = 8.0 + (roll % 15) as f32;
                return Some(base);
            }
            let before = w.body.position;
            w.body
                .step_blocked(to, w.speed, dt, layers, heights, blocked);
            let moved = (w.body.position - before).truncate().length();
            w.stuck = if moved < w.speed * dt * 0.2 {
                w.stuck + dt
            } else {
                0.0
            };
            self.transform = body_transform(&w.body);
            return (current != w.clip).then_some(w.clip);
        }
        w.resting += dt;
        if current == base && w.resting >= w.next_stroll {
            // A walkable spot within the radius, if one turns up in a few tries.
            for k in 0..8u64 {
                let r = roll.wrapping_mul(k + 1).wrapping_mul(0x9e37_79b9);
                let angle = (r % 6283) as f32 / 1000.0;
                let dist = 1.5 + ((r >> 16) % 1000) as f32 / 1000.0 * (WANDER_RADIUS - 1.5);
                let spot = w.home + Vec2::new(angle.cos(), angle.sin()) * dist;
                if layers.first().is_some_and(|g| g.walkable(spot)) {
                    w.target = Some(spot);
                    w.stuck = 0.0;
                    return Some(w.clip);
                }
            }
            w.resting = 0.0;
        }
        self.fidget(dt, current, finished)
    }

    /// Poses it in its idle's variant with animation table modifier `modifier`
    /// (`MORTALLY_WOUNDED`), staying put. False if it has no such variant.
    pub fn set_mood(&mut self, bank: &mut AnimBank, modifier: &str) -> bool {
        let Some(&(_, id)) = self.moods.iter().find(|(m, _)| m == modifier) else {
            return false;
        };
        let mut clips = Vec::new();
        for (_, animator) in &self.parts {
            match bank.clip(id, animator.skeleton()) {
                Ok(clip) => clips.push(clip),
                Err(e) => {
                    eprintln!("mood {modifier} ({id}): {e}");
                    return false;
                }
            }
        }
        for ((_, animator), clip) in self.parts.iter_mut().zip(clips) {
            self.base = animator.add_clip(clip);
            animator.play(self.base);
        }
        self.fidgets = 0;
        self.walk = None;
        true
    }

    /// Plays its flinch once, stopping a stroll.
    pub fn flinch(&mut self) {
        let Some(clip) = self.hit_clip else {
            return;
        };
        if let Some(w) = &mut self.walk {
            w.target = None;
            w.resting = 0.0;
        }
        for (_, animator) in &mut self.parts {
            animator.play_once(clip, 0.0);
        }
    }

    /// Where the creature stands.
    pub fn position(&self) -> Vec2 {
        self.transform.w_axis.truncate().truncate()
    }

    /// For a wandering creature, how far it is from its placement.
    pub fn wandered(&self) -> Option<f32> {
        self.walk
            .as_ref()
            .map(|w| w.body.position.truncate().distance(w.home))
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

/// A walking creature's model-to-world transform (meshes face their local -y).
fn body_transform(body: &walk::Hero) -> Mat4 {
    Mat4::from_translation(body.position)
        * Mat4::from_rotation_z(body.heading)
        * model::hero_local()
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
        let configs = model::bone_configs(data_dir, &c.morphs);
        let wanders = !c.scripted && !c.definition.contains("GUARD");
        let walk_id = c.idles.walk.filter(|_| wanders);
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
            let mut skin = skin.clone();
            skin.set_morph(&configs);
            let mut animator = Animator::new(model.vertices.clone(), skin, part_clips);
            let renderer = ModelRenderer::new(device, queue, model);
            // Start each creature at a different point of its loop.
            renderer.set_vertices(queue, animator.advance(i as f32 * 0.37));
            parts.push((renderer, animator));
        }
        // Only if every part loaded every clip can the walk be relied on.
        let clips = parts.iter().map(|(_, a)| a.clip_count()).min().unwrap_or(0);
        let complete = parts.iter().all(|(_, a)| a.clip_count() == ids.len());
        if !parts.is_empty() {
            let has_walk = walk_id.is_some() && complete;
            let has_hit = c.idles.hit.is_some() && complete;
            let fidgets = clips.saturating_sub(1 + usize::from(has_walk) + usize::from(has_hit));
            let walk_clip = 1 + fidgets;
            let walk = has_walk.then(|| {
                let (_, animator) = &parts[0];
                let p = c.placement.position;
                let f = c.placement.forward;
                Wander {
                    body: walk::Hero {
                        position: p,
                        heading: (-f.x).atan2(f.y),
                        layer: 0,
                    },
                    home: p.truncate(),
                    clip: walk_clip,
                    speed: animator.speed(walk_clip).unwrap_or(0.0)
                        * model::MESH_SCALE
                        * animator.stride_ratio(),
                    target: None,
                    resting: 0.0,
                    next_stroll: 4.0 + (c.uid % 13) as f32,
                    stuck: 0.0,
                }
            });
            let transform = match &walk {
                Some(w) => body_transform(&w.body),
                None => c.placement.transform(),
            };
            npcs.push(Npc {
                script_name: c.script_name.clone(),
                hidden: false,
                female: ["FEMALE", "GIRL", "SISTER"]
                    .iter()
                    .any(|k| c.definition.contains(k)),
                hit_clip: has_hit.then(|| clips - 1),
                base: 0,
                moods: c.idles.moods.clone(),
                since_fidget: 0.0,
                next_fidget: 3.0 + (c.uid % 11) as f32,
                rng: c.uid | 1,
                fidgets,
                walk: walk.filter(|w| w.speed > 0.0),
                parts,
                transform,
            });
        }
    }
    npcs
}
