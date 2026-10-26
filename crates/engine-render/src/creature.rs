//! Draws creatures: each creature is an `engine_world::creature::Npc`; a
//! [`CreatureModel`] holds its parts' skinned meshes and deforms them from the
//! world's poses, and its eyeballs, which follow their head's bone.

use engine_world::creature::Npc;
use glam::Mat4;

use crate::model::{ModelRenderer, ModelVertex};
use crate::skin::{Skin, deform};

/// A creature's parts' meshes, drawn for the world creature at the same index.
pub struct CreatureModel {
    pub parts: Vec<PartModel>,
    pub eyes: Vec<EyeModel>,
}

/// An eyeball: a static mesh at one of a head's eye dummies (`EYE_SET_L`,
/// `EYE_SET_R`), moving with the head bone.
pub struct EyeModel {
    pub renderer: ModelRenderer,
    /// The part whose head it sits in, and that part's head bone.
    part: usize,
    bone: usize,
    /// The dummy's place in the part's bind pose.
    socket: Mat4,
    /// Where it is drawn this frame, in the world.
    pub transform: Mat4,
}

impl EyeModel {
    /// An eyeball drawn by `renderer` at `socket` (a dummy's matrix in the bind
    /// pose) in part `part`, following its bone `bone`.
    pub fn new(renderer: ModelRenderer, part: usize, bone: usize, socket: Mat4) -> Self {
        Self {
            renderer,
            part,
            bone,
            socket,
            transform: Mat4::IDENTITY,
        }
    }
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
        // The bone's skinning matrix takes the bind pose to the pose.
        for eye in &mut self.eyes {
            let bone = npc
                .parts
                .get(eye.part)
                .and_then(|a| a.matrices().get(eye.bone))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            eye.transform = npc.transform * bone * eye.socket;
        }
    }
}

impl PartModel {
    /// A part's mesh: its renderer, bind pose and weights.
    pub fn new(renderer: ModelRenderer, bind: Vec<ModelVertex>, skin: Skin) -> Self {
        Self {
            renderer,
            bind,
            skin,
            posed: Vec::new(),
        }
    }
}
