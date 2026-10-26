//! Draws creatures: each creature is an `engine_world::creature::Npc`; a
//! [`CreatureModel`] holds its parts' skinned meshes and deforms them from the
//! world's poses.

use engine_world::creature::Npc;

use crate::model::{ModelRenderer, ModelVertex};
use crate::skin::{Skin, deform};

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
