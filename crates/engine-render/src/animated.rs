//! Draws animated objects: doors and training dummies are world entities (a
//! [`Placed`] and an animator); an [`AnimatedModel`] holds one's skinned mesh
//! and deforms it with the animator's pose each frame.

use engine_world::anim::Animator;
use engine_world::body::Placed;
use engine_world::world::World;
use hecs::Entity;

use crate::model::{ModelRenderer, ModelVertex};
use crate::skin::{Skin, deform};

/// An animated object's mesh, for the entity it draws.
pub struct AnimatedModel {
    pub entity: Entity,
    pub renderer: ModelRenderer,
    bind: Vec<ModelVertex>,
    skin: Skin,
    posed: Vec<ModelVertex>,
}

impl AnimatedModel {
    /// Poses the mesh as its entity's animator stands and uploads it; the
    /// entity's placement, if it is still there.
    pub fn update(&mut self, queue: &wgpu::Queue, world: &World) -> Option<glam::Mat4> {
        let mut q = world
            .ecs
            .query_one::<(&Placed, &Animator)>(self.entity)
            .ok()?;
        let (placed, animator) = q.get()?;
        deform(&self.skin, &self.bind, animator.matrices(), &mut self.posed);
        self.renderer.set_vertices(queue, &self.posed);
        Some(placed.0)
    }

    /// The mesh drawing `entity`: its renderer, bind pose and weights.
    pub fn new(
        entity: Entity,
        renderer: ModelRenderer,
        bind: Vec<ModelVertex>,
        skin: Skin,
    ) -> Self {
        Self {
            entity,
            renderer,
            bind,
            skin,
            posed: Vec::new(),
        }
    }
}
