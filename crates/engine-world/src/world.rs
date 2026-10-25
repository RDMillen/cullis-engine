//! The world: one region's simulation state. So far its navigation and
//! collision geometry, its light by time of day and the camera's focus;
//! creatures, doors, the Hero and emitters move here as entities next (see
//! `docs/architecture.md`, migration step 3).

use glam::Vec3;

use crate::collide::Colliders;
use crate::lighting::{self, Light, Lighting, PointLight};
use crate::walk::{HeightGrid, WalkGrid};

/// How much brighter nights are made than the table's colours (by eye: they
/// read about 30% too dark against the original).
const NIGHT_LIFT: f32 = 0.3;

/// A region's simulation state.
#[derive(Default)]
pub struct World {
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

    /// Advances the world `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        self.place_point_lights();
    }

    /// The placed lights near the focus, flickering, in both lights.
    fn place_point_lights(&mut self) {
        for light in [&mut self.light, &mut self.indoor_light] {
            lighting::set_points(light, &self.point_lights, self.focus, self.time);
        }
    }
}
