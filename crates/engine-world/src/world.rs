//! The world: one region's simulation state. Its navigation and collision
//! geometry, its light by time of day and the camera's focus, and its
//! entities in a `hecs` world: so far doors (a [`Door`] and an animator);
//! creatures, the Hero and emitters follow (see `docs/architecture.md`,
//! migration step 3).

use glam::{Vec2, Vec3};
use hecs::Entity;

use crate::anim::Animator;
use crate::door::{self, Door};

use crate::collide::Colliders;
use crate::lighting::{self, Light, Lighting, PointLight};
use crate::walk::{HeightGrid, WalkGrid};

/// How much brighter nights are made than the table's colours (by eye: they
/// read about 30% too dark against the original).
const NIGHT_LIFT: f32 = 0.3;

/// A region's simulation state.
#[derive(Default)]
pub struct World {
    /// The region's entities.
    pub ecs: hecs::World,
    /// Where things can walk (one grid per walking level) and the ground's
    /// height.
    pub walk: Vec<WalkGrid>,
    pub heights: Option<HeightGrid>,
    /// Placed meshes as triangles, for camera rays, walls and floors.
    pub colliders: Option<Colliders>,
    /// The region's environment themes, and the light now outdoors and inside
    /// buildings (shadows are the renderer's to add).
    pub lighting: Option<Lighting>,
    pub light: Light,
    pub indoor_light: Light,
    /// The region's lamps, fires and light markers.
    pub point_lights: Vec<PointLight>,
    /// Where the camera looks: lights and effects near it are the ones that
    /// count.
    pub focus: Vec3,
    /// Simulated seconds since the region loaded.
    pub time: f32,
}

impl World {
    /// Sets the light for time of day `hour`, scaled by `brightness` (with
    /// nights lifted) and capped at `highlight_limit`.
    pub fn set_hour(&mut self, hour: f32, brightness: f32, highlight_limit: f32) {
        let Some(themes) = &self.lighting else {
            return;
        };
        self.light = themes.at(hour, false);
        self.indoor_light = themes.at(hour, true);
        // Nights lifted by up to NIGHT_LIFT as the sun's light fades.
        let sun = self.light.sun.iter().sum::<f32>() / 3.0;
        let night = 1.0 - ((sun - 0.15) / 0.5).clamp(0.0, 1.0);
        for light in [&mut self.light, &mut self.indoor_light] {
            light.brightness = brightness * (1.0 + NIGHT_LIFT * night);
            light.sky_brightness = brightness * (1.0 - (1.0 - lighting::NIGHT_SKY) * night);
            light.highlight_limit = highlight_limit;
        }
        self.place_point_lights();
    }

    /// Advances the world `dt` seconds: animations play, placed lights
    /// flicker.
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        for (_, animator) in self.ecs.query_mut::<&mut Animator>() {
            animator.advance(dt);
        }
        self.place_point_lights();
    }

    /// Adds a door, held at the end of its opening animation (open) or its
    /// closing one (closed), so nothing moves until it is used.
    pub fn spawn_door(&mut self, door: Door, mut animator: Animator) -> Entity {
        let clip = if door.open {
            door::OPEN_CLIP
        } else {
            door::CLOSE_CLIP
        };
        animator.play_once(clip, f32::MAX);
        animator.advance(0.0);
        self.ecs.spawn((door, animator))
    }

    /// Whether a closed door is in the way at `p`.
    pub fn door_blocks(&self, p: Vec2) -> bool {
        self.ecs.query::<&Door>().iter().any(|(_, d)| d.blocks(p))
    }

    /// The door nearest `p` within reach, on about its floor.
    pub fn nearest_door(&self, p: Vec3) -> Option<Entity> {
        self.ecs
            .query::<&Door>()
            .iter()
            .map(|(e, d)| (e, d.centre().distance(p.truncate()), d.floor()))
            .filter(|&(_, dist, floor)| dist < door::REACH && (floor - p.z).abs() < 3.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(e, _, _)| e)
    }

    /// Opens a closed door or closes an open one, swinging it; whether it is
    /// now open.
    pub fn toggle_door(&mut self, e: Entity) -> Option<bool> {
        let (door, animator) = self
            .ecs
            .query_one_mut::<(&mut Door, &mut Animator)>(e)
            .ok()?;
        door.open = !door.open;
        let clip = if door.open {
            door::OPEN_CLIP
        } else {
            door::CLOSE_CLIP
        };
        animator.play_once(clip, 0.0);
        Some(door.open)
    }

    /// The placed lights near the focus, flickering, in both lights.
    fn place_point_lights(&mut self) {
        for light in [&mut self.light, &mut self.indoor_light] {
            lighting::set_points(light, &self.point_lights, self.focus, self.time);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Rig;
    use glam::Mat4;

    #[test]
    fn a_door_used_stops_blocking() {
        let mut world = World::default();
        let door = Door::new(Mat4::IDENTITY, (0.0, 1.0), 1.0, false);
        let e = world.spawn_door(door, Animator::new(Rig::default(), Vec::new()));
        let doorway = Vec2::new(0.5, 0.0);
        assert!(world.door_blocks(doorway));
        assert_eq!(world.nearest_door(Vec3::new(0.5, 1.0, 0.0)), Some(e));
        assert_eq!(
            world.nearest_door(Vec3::new(0.5, 1.0, 5.0)),
            None,
            "another floor"
        );
        assert_eq!(world.toggle_door(e), Some(true));
        assert!(!world.door_blocks(doorway));
        world.tick(0.1);
    }
}
