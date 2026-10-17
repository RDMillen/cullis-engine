//! Sun shadows: a depth map rendered from the sun over the area around the
//! camera, which the terrain and model shaders compare against. The game's
//! own shadowing is not decoded; this is a stand-in drawn the usual way.

use eframe::wgpu;
use glam::camera::rh::{proj, view};
use glam::{Mat4, Vec3};

/// Texels along each side of the map.
pub const SIZE: u32 = 2048;
/// Half the side of the square of ground covered, in world units (metres).
pub const REACH: f32 = 45.0;
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The bind group layout the lit shaders read the map through: the depth
/// texture and a comparison sampler.
pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

/// A shadow map: the view to render depth into and the group to read it by.
pub struct ShadowMap {
    pub view: wgpu::TextureView,
    pub group: wgpu::BindGroup,
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device, size: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout: &layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self { view, group }
    }

    /// A 1x1 map for drawing without shadows (the group must still be bound).
    pub fn none(device: &wgpu::Device) -> Self {
        Self::new(device, 1)
    }
}

/// Depth state for shadow passes: nearest wins, with a slope bias against
/// surfaces shadowing themselves.
pub fn depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: wgpu::DepthBiasState {
            constant: 0,
            slope_scale: 2.0,
            clamp: 0.0,
        },
    }
}

/// World to the sun's clip space, covering [`REACH`] around `centre`, snapped
/// to whole texels so shadow edges do not crawl as the camera moves.
pub fn view_proj(sun_dir: Vec3, centre: Vec3) -> Mat4 {
    let dir = sun_dir.normalize_or(Vec3::Z);
    let up = if dir.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    let depth = 300.0;
    let view = view::look_at_mat4(centre + dir * depth * 0.5, centre, up);
    // Depth 0 at the near plane, 1 at the far one (wgpu clip space).
    let proj = proj::directx::orthographic(-REACH, REACH, -REACH, REACH, 0.0, depth);
    let mut m = proj * view;
    // Snap: move the projection by the origin's sub-texel offset.
    let texel = 2.0 / SIZE as f32;
    let origin = m.project_point3(Vec3::ZERO);
    let snap = |v: f32| (v / texel).round() * texel - v;
    m = Mat4::from_translation(Vec3::new(snap(origin.x), snap(origin.y), 0.0)) * m;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_lands_mid_map_and_depth_grows_away_from_the_sun() {
        let sun = Vec3::new(0.3, -0.4, 0.8);
        let centre = Vec3::new(3500.0, 860.0, 15.0);
        let m = view_proj(sun, centre);
        let c = m.project_point3(centre);
        assert!(c.x.abs() < 0.01 && c.y.abs() < 0.01, "{c}");
        let nearer = m.project_point3(centre + sun.normalize() * 5.0);
        assert!(nearer.z < c.z, "closer to the sun is less deep");
        assert!((0.0..1.0).contains(&c.z));
    }
}
