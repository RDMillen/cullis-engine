//! Creatures: people and animals standing, fidgeting, strolling, sent on
//! errands by quests, gesturing and moving their mouths as they talk. Each has
//! one or more skinned parts (head, torso, legs) playing the same clip.
//!
//! Stand-in behaviour: the game's creature AI is not decoded.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};

use crate::anim::{AnimBank, Animator};
use crate::body;
use crate::walk::{self, HeightGrid, WalkGrid};

/// One placed creature: one or more skinned meshes (head, torso, legs) playing
/// the same clip.
pub struct Npc {
    /// Its parts' animators, all playing the same clips.
    pub parts: Vec<Animator>,
    pub transform: Mat4,
    /// Its `ScriptName` (its role), for what it says.
    pub script_name: Option<String>,
    /// Sent away by a quest: not drawn, not in the way.
    pub hidden: bool,
    /// For lines that come in male and female versions.
    pub female: bool,
    /// A child (a lower face, for the conversation camera).
    pub child: bool,
    /// Its mouth shapes' animation set (`VILLAGER_MALE`: the
    /// `ANIM_VILLAGER_MALE_PHONEME_*` animations), and those loaded (shape name
    /// to each part's clip index; empty until it first speaks).
    viseme_set: &'static str,
    visemes: HashMap<String, Vec<usize>>,
    /// Which way it faces: radians around z, 0 facing +y (as `walk::Hero`).
    heading: f32,
    /// Its flinch when hit, if it has one.
    hit_clip: Option<usize>,
    /// The clip it stands in (its base idle, or a quest's mood).
    base: usize,
    /// Its idle's variants by modifier (bank ids), for [`Npc::set_mood`].
    moods: Vec<(String, u32)>,
    /// Its animation table, and the clips of gestures loaded so far (bank id
    /// to each part's clip index).
    slots: std::sync::Arc<Vec<fable_data::defs::AnimationSlot>>,
    gestures: HashMap<u32, Vec<usize>>,
    /// Seconds since the creature last fidgeted, and when it fidgets next.
    since_fidget: f32,
    next_fidget: f32,
    rng: u64,
    /// Clips 1..=fidgets are fidgets; the walk, if any, comes after them.
    fidgets: usize,
    walk: Option<Wander>,
}

/// A creature that can walk: one that strolls around where it was placed
/// (stand-in behaviour: the game's creature AI is not decoded), or one a quest
/// sends somewhere.
struct Wander {
    body: walk::Hero,
    home: Vec2,
    clip: usize,
    /// Whether it strolls by itself (scripted people stand until sent).
    strolls: bool,
    /// Where a quest has sent it, and the route there.
    errand: Option<walk::Follow>,
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
    /// Advances the creature. Standing, it breathes in
    /// its base idle (clip 0) and every so often plays one of its fidgets once;
    /// a wandering creature now and then walks to a spot near its placement.
    pub fn animate(
        &mut self,
        dt: f32,
        layers: &[WalkGrid],
        heights: Option<&HeightGrid>,
        blocked: &dyn Fn(Vec2) -> bool,
    ) {
        let Some(first) = self.parts.first() else {
            return;
        };
        let (current, finished) = (first.current(), first.finished());
        let next = match (self.walk.is_some(), heights) {
            (true, Some(heights)) => self.wander(dt, current, finished, layers, heights, blocked),
            _ => self.fidget(dt, current, finished),
        };
        for animator in &mut self.parts {
            if let Some(clip) = next {
                animator.play(clip);
            }
            animator.advance(dt);
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
        // An errand first: along its route, then back to standing.
        if let Some(errand) = &mut w.errand {
            let step = errand.direction(&w.body, layers, heights, dt, blocked, blocked);
            match step {
                Some(to) => {
                    w.body
                        .step_blocked(to, w.speed, dt, layers, heights, blocked);
                    self.transform = body_transform(&w.body);
                    return (current != w.clip).then_some(w.clip);
                }
                None => {
                    w.errand = None;
                    w.home = w.body.position.truncate();
                    w.resting = 0.0;
                    return Some(base);
                }
            }
        }
        if !w.strolls {
            return self.fidget(dt, current, finished);
        }
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
        for animator in &self.parts {
            match bank.clip(id, animator.skeleton()) {
                Ok(clip) => clips.push(clip),
                Err(e) => {
                    log::warn!("mood {modifier} ({id}): {e}");
                    return false;
                }
            }
        }
        for (animator, clip) in self.parts.iter_mut().zip(clips) {
            self.base = animator.add_clip(clip);
            animator.play(self.base);
        }
        self.fidgets = 0;
        self.walk = None;
        true
    }

    /// Moves its mouth: mouth shapes (`AH`, `EE`, `OH`, …) with weights 0..=1,
    /// loading its set's shapes on first use; empty to stop.
    pub fn mouth(&mut self, bank: &mut AnimBank, shapes: &[(&str, f32)]) {
        if !shapes.is_empty() && self.visemes.is_empty() {
            for name in ["AH", "EE", "OH", "SZ", "MM", "WW"] {
                let anim = format!("ANIM_{}_PHONEME_{name}", self.viseme_set);
                let Some(id) = bank.id(&anim) else { continue };
                let clips: Option<Vec<_>> = self
                    .parts
                    .iter()
                    .map(|a| bank.clip(id, a.skeleton()).ok())
                    .collect();
                let Some(clips) = clips else { continue };
                let indices = self
                    .parts
                    .iter_mut()
                    .zip(clips)
                    .map(|(a, clip)| a.add_clip(clip))
                    .collect();
                self.visemes.insert(name.to_owned(), indices);
            }
        }
        for (k, animator) in self.parts.iter_mut().enumerate() {
            let face: Vec<(usize, f32)> = shapes
                .iter()
                .filter_map(|(name, w)| Some((*self.visemes.get(*name)?.get(k)?, *w)))
                .collect();
            animator.set_face(&face);
        }
    }

    /// Whether its skeleton has face bones for mouth shapes to move.
    pub fn has_face(&self) -> bool {
        self.parts.iter().any(|a| a.has_face())
    }

    /// Height of its face above its feet, roughly (world units).
    pub fn face_height(&self) -> f32 {
        if self.child { 1.05 } else { 1.6 }
    }

    /// Where its feet are.
    pub fn feet(&self) -> glam::Vec3 {
        self.transform.w_axis.truncate()
    }

    /// Turns towards `p`, at most `dt` seconds' worth (a quick turn).
    pub fn turn_towards(&mut self, p: Vec2, dt: f32) {
        let Some(d) = (p - self.position()).try_normalize() else {
            return;
        };
        let want = (-d.x).atan2(d.y);
        let diff = (want - self.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let step = 6.0 * dt;
        if diff.abs() < 1e-3 {
            return;
        }
        self.heading += diff.clamp(-step, step);
        let at = self.feet();
        if let Some(w) = &mut self.walk {
            w.body.heading = self.heading;
            w.target = None;
        }
        self.transform =
            Mat4::from_translation(at) * Mat4::from_rotation_z(self.heading) * body::local();
    }

    /// Bank ids of the gestures to go with a line: animation slot `slot` if
    /// the line names one (its plain variant), else its `CONV_GENERAL`
    /// animations for modifier `mood` (`CONVERSATION_ANGRY`), else its happy
    /// ones.
    pub fn gesture_ids(&self, slot: Option<&str>, mood: &str) -> Vec<u32> {
        let find = |name: &str, modifier: Option<&str>| -> Vec<u32> {
            let mut ids: Vec<u32> = self
                .slots
                .iter()
                .filter(|s| s.slot == name && s.modifier.as_deref() == modifier)
                .map(|s| s.animation)
                .collect();
            ids.dedup();
            ids
        };
        if let Some(slot) = slot {
            let ids = find(slot, None);
            if !ids.is_empty() {
                return ids;
            }
        }
        let ids = find("CONV_GENERAL", Some(mood));
        if ids.is_empty() {
            find("CONV_GENERAL", Some("CONVERSATION_HAPPY"))
        } else {
            ids
        }
    }

    /// Plays animation `id` (from its table) once, loading it first; it then
    /// returns to its idle. Stops a stroll. False if the animation cannot load.
    pub fn gesture(&mut self, bank: &mut AnimBank, id: u32) -> bool {
        if !self.gestures.contains_key(&id) {
            let mut clips = Vec::new();
            for animator in &self.parts {
                match bank.clip(id, animator.skeleton()) {
                    Ok(clip) => clips.push(clip),
                    Err(e) => {
                        log::warn!("gesture {id}: {e}");
                        return false;
                    }
                }
            }
            let indices = self
                .parts
                .iter_mut()
                .zip(clips)
                .map(|(animator, clip)| animator.add_clip(clip))
                .collect();
            self.gestures.insert(id, indices);
        }
        if let Some(w) = &mut self.walk {
            w.target = None;
            w.resting = 0.0;
        }
        let indices = &self.gestures[&id];
        for (animator, &clip) in self.parts.iter_mut().zip(indices) {
            animator.play_once(clip, 0.0);
        }
        true
    }

    /// Whether it is in the middle of a gesture.
    pub fn gesturing(&self) -> bool {
        self.parts.first().is_some_and(|a| {
            self.gestures
                .values()
                .any(|g| g.first() == Some(&a.current()))
                && !a.finished()
        })
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
        for animator in &mut self.parts {
            animator.play_once(clip, 0.0);
        }
    }

    /// Where the creature stands.
    pub fn position(&self) -> Vec2 {
        self.transform.w_axis.truncate().truncate()
    }

    /// Sends it walking to `goal` (world position; its height picks the floor
    /// where floors are stacked), finding a way round; false if it cannot walk.
    pub fn walk_to(&mut self, goal: Vec3) -> bool {
        let Some(w) = &mut self.walk else {
            return false;
        };
        w.target = None;
        w.errand = Some(walk::Follow::to_floor(goal.truncate(), Some(goal.z)));
        true
    }

    /// Puts it at world position `at` (on the walking layer nearest that
    /// height), facing world point `facing`.
    pub fn place(
        &mut self,
        at: Vec3,
        facing: Vec2,
        layers: &[WalkGrid],
        heights: Option<&HeightGrid>,
    ) {
        let to = facing - at.truncate();
        let heading = (-to.x).atan2(to.y);
        let stand = heights.and_then(|h| walk::stand_at(layers, h, at.truncate(), Some(at.z)));
        let (layer, position) = match stand {
            Some((layer, spot, z)) => (layer, spot.extend(z)),
            None => (0, at),
        };
        self.heading = heading;
        match &mut self.walk {
            Some(w) => {
                w.body.position = position;
                w.body.heading = heading;
                w.body.layer = layer;
                w.home = position.truncate();
                w.errand = None;
                w.target = None;
                self.transform = body_transform(&w.body);
            }
            None => {
                self.transform = Mat4::from_translation(position)
                    * Mat4::from_rotation_z(heading)
                    * body::local();
            }
        }
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

/// A walking creature's model-to-world transform (meshes face their local -y).
pub fn body_transform(body: &walk::Hero) -> Mat4 {
    Mat4::from_translation(body.position) * Mat4::from_rotation_z(body.heading) * body::local()
}

/// The mouth shape animations a creature uses, by kind: children the boy's
/// (`ANIM_BIPED_BOY_01_PHONEME_*`), guards and bandits the bandit's (their
/// skeletons play bandit animations), else the villagers'. A guess from the
/// sets' names.
pub fn viseme_set(definition: &str) -> &'static str {
    let d = definition.to_ascii_uppercase();
    if ["CHILD", "BOY", "GIRL", "SISTER", "BULLY"]
        .iter()
        .any(|k| d.contains(k))
    {
        "BIPED_BOY_01"
    } else if d.contains("GUARD") || d.contains("BANDIT") {
        "BANDIT"
    } else if d.contains("FEMALE") {
        "VILLAGER_FEMALE"
    } else {
        "VILLAGER_MALE"
    }
}

/// What a creature is made from: its placement and kind, and its parts'
/// animators with clip 0 its base idle, then its fidgets, then its walk and its
/// flinch when it has them.
pub struct Spec {
    pub definition: String,
    pub uid: u64,
    pub script_name: Option<String>,
    pub position: Vec3,
    pub forward: Vec3,
    /// Its model-to-world transform as placed (used when it cannot walk).
    pub transform: Mat4,
    /// Whether it strolls by itself (scripted people stand until sent).
    pub strolls: bool,
    pub has_walk: bool,
    pub has_hit: bool,
    pub moods: Vec<(String, u32)>,
    pub slots: std::sync::Arc<Vec<fable_data::defs::AnimationSlot>>,
    pub parts: Vec<Animator>,
}

impl Npc {
    /// A creature from `spec`; None without parts.
    pub fn new(spec: Spec) -> Option<Self> {
        let first = spec.parts.first()?;
        let clips = spec
            .parts
            .iter()
            .map(Animator::clip_count)
            .min()
            .unwrap_or(0);
        let fidgets =
            clips.saturating_sub(1 + usize::from(spec.has_walk) + usize::from(spec.has_hit));
        let walk_clip = 1 + fidgets;
        let heading = (-spec.forward.x).atan2(spec.forward.y);
        let walk = spec.has_walk.then(|| Wander {
            body: walk::Hero {
                position: spec.position,
                heading,
                layer: 0,
            },
            home: spec.position.truncate(),
            clip: walk_clip,
            strolls: spec.strolls,
            errand: None,
            speed: first.speed(walk_clip).unwrap_or(0.0) * body::MESH_SCALE * first.stride_ratio(),
            target: None,
            resting: 0.0,
            next_stroll: 4.0 + (spec.uid % 13) as f32,
            stuck: 0.0,
        });
        let transform = match &walk {
            Some(w) => body_transform(&w.body),
            None => spec.transform,
        };
        let d = &spec.definition;
        Some(Self {
            script_name: spec.script_name,
            hidden: false,
            female: ["FEMALE", "GIRL", "SISTER"].iter().any(|k| d.contains(k)),
            child: ["CHILD", "BOY", "GIRL", "SISTER", "BULLY"]
                .iter()
                .any(|k| d.contains(k)),
            viseme_set: viseme_set(d),
            visemes: HashMap::new(),
            heading,
            hit_clip: spec.has_hit.then(|| clips - 1),
            base: 0,
            moods: spec.moods,
            slots: spec.slots,
            gestures: HashMap::new(),
            since_fidget: 0.0,
            next_fidget: 3.0 + (spec.uid % 11) as f32,
            rng: spec.uid | 1,
            fidgets,
            walk: walk.filter(|w| w.speed > 0.0),
            parts: spec.parts,
            transform,
        })
    }
}
