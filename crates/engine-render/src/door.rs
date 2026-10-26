//! Draws doors: each door is a world entity (`engine_world::door::Door` and its
//! animator); a [`DoorModel`] holds its skinned mesh and deforms it with the
//! animator's pose each frame.

use engine_world::anim::Animator;
use engine_world::door::Door;
use engine_world::world::World;
use hecs::Entity;

use crate::model::{ModelRenderer, ModelVertex};
use crate::skin::{Skin, deform};

/// A door's mesh, for the door entity it draws.
pub struct DoorModel {
    pub entity: Entity,
    pub renderer: ModelRenderer,
    bind: Vec<ModelVertex>,
    skin: Skin,
    posed: Vec<ModelVertex>,
}

impl DoorModel {
    /// Poses the mesh as its entity's animator stands and uploads it; the
    /// door's placement, if the entity is still there.
    pub fn update(&mut self, queue: &wgpu::Queue, world: &World) -> Option<glam::Mat4> {
        let mut q = world
            .ecs
            .query_one::<(&Door, &Animator)>(self.entity)
            .ok()?;
        let (door, animator) = q.get()?;
        deform(&self.skin, &self.bind, animator.matrices(), &mut self.posed);
        self.renderer.set_vertices(queue, &self.posed);
        Some(door.transform)
    }
}

impl DoorModel {
    /// The mesh drawing door `entity`: its renderer, bind pose and weights.
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
