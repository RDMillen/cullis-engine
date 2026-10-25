//! Doors: they swing open and shut by their `DOOR_OPEN` and `DOOR_CLOSE`
//! animations (clips 0 and 1 of their animator), and block their doorway
//! while closed.

use glam::{Mat4, Vec2};

/// How far in front of and behind a closed door it blocks, in world units.
const HALF_DEPTH: f32 = 0.25;
/// How near the Hero must be to use a door.
pub const REACH: f32 = 2.0;
/// The door animator's clips.
pub const OPEN_CLIP: usize = 0;
pub const CLOSE_CLIP: usize = 1;

/// A door's placement and state (a component; its animator is another).
#[derive(Debug, Clone)]
pub struct Door {
    /// Its mesh's space to the world.
    pub transform: Mat4,
    /// World to the mesh's space, for the blocking test.
    inverse: Mat4,
    /// The mesh's extent across the doorway (its local x), and how far one
    /// world unit is in mesh units.
    width: (f32, f32),
    mesh_per_world: f32,
    pub open: bool,
}

impl Door {
    /// A door placed by `transform` whose mesh spans `width` across its
    /// doorway (in mesh units, `mesh_per_world` of them to a world unit).
    pub fn new(transform: Mat4, width: (f32, f32), mesh_per_world: f32, open: bool) -> Self {
        Self {
            transform,
            inverse: transform.inverse(),
            width,
            mesh_per_world,
            open,
        }
    }

    pub fn centre(&self) -> Vec2 {
        self.transform.w_axis.truncate().truncate()
    }

    /// The height of its threshold.
    pub fn floor(&self) -> f32 {
        self.transform.w_axis.z
    }

    /// Whether a closed door is in the way at `p`.
    pub fn blocks(&self, p: Vec2) -> bool {
        if self.open {
            return false;
        }
        let local = self.inverse.transform_point3(p.extend(self.floor()));
        let depth = HALF_DEPTH * self.mesh_per_world;
        local.x >= self.width.0 && local.x <= self.width.1 && local.y.abs() <= depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_door_blocks_its_doorway_only() {
        // A door 1 unit wide along x at the origin, mesh units = world units.
        let mut d = Door::new(Mat4::IDENTITY, (0.0, 1.0), 1.0, false);
        assert!(d.blocks(Vec2::new(0.5, 0.0)));
        assert!(!d.blocks(Vec2::new(0.5, 1.0)), "in front of it");
        assert!(!d.blocks(Vec2::new(2.0, 0.0)), "beside it");
        d.open = true;
        assert!(!d.blocks(Vec2::new(0.5, 0.0)));
    }
}
