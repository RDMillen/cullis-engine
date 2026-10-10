use glam::camera::rh::{proj, view};
use glam::{Mat4, Vec3};

/// Orbits a target point; z is up.
#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    /// Frames a bounding box from the south-west, looking down at 40 degrees.
    pub fn framing(lo: Vec3, hi: Vec3) -> Self {
        let size = (hi - lo).truncate().length().max(16.0);
        Self {
            target: (lo + hi) * 0.5,
            yaw: -2.2,
            pitch: 0.7,
            distance: size * 0.9,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = proj::directx::perspective(
            50f32.to_radians(),
            aspect,
            self.distance * 0.01,
            self.distance * 10.0,
        );
        proj * view::look_at_mat4(self.eye(), self.target, Vec3::Z)
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(0.05, 1.5);
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.002).exp()).clamp(4.0, 5000.0);
    }

    /// Moves the target across the ground plane, in screen-relative directions.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let forward = Vec3::new(-self.yaw.cos(), -self.yaw.sin(), 0.0);
        let right = forward.cross(Vec3::Z);
        let scale = self.distance * 0.0015;
        self.target += (-right * dx + forward * dy) * scale;
    }
}
