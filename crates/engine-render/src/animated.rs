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
    /// What a training dummy looks like broken, drawn in its place once it is.
    pub broken: Option<ModelRenderer>,
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
        if self.is_broken(world) {
            return Some(placed.0);
        }
        deform(&self.skin, &self.bind, animator.matrices(), &mut self.posed);
        self.renderer.set_vertices(queue, &self.posed);
        Some(placed.0)
    }

    /// Whether its entity is a broken dummy with a broken mesh to show.
    pub fn is_broken(&self, world: &World) -> bool {
        self.broken.is_some()
            && world
                .ecs
                .get::<&engine_world::combat::Dummy>(self.entity)
                .is_ok_and(|d| d.broken)
    }

    /// What is drawn for it now: its mesh, or its broken mesh once broken.
    pub fn shown(&self, world: &World) -> &ModelRenderer {
        match &self.broken {
            Some(broken) if self.is_broken(world) => broken,
            _ => &self.renderer,
        }
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
            broken: None,
            bind,
            skin,
            posed: Vec::new(),
        }
    }
}
