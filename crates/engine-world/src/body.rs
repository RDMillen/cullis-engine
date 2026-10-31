//! How creature and Hero meshes stand in the world.

use glam::{Mat4, Vec3};

/// World units per mesh unit.
pub const MESH_SCALE: f32 = 0.01;

/// Takes a creature's mesh to its local space: feet at the origin, facing +y
/// (meshes face their local -y, so it is turned round), in world units.
pub fn local() -> Mat4 {
    Mat4::from_scale(Vec3::splat(MESH_SCALE)) * Mat4::from_rotation_z(std::f32::consts::PI)
}

/// Where an animated object's mesh stands: its mesh space to the world (a
/// component of doors and training dummies).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed(pub Mat4);
