//! The sky behind the terrain: the environment's sky textures blended by the
//! light's weights and tinted, with its two cloud layers drifting over them
//! (see `sky.wgsl`).

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use glam::{Mat4, Vec3};

use crate::lighting::Light;
use crate::render::{COLOR_FORMAT, DEPTH_FORMAT};
use crate::themes::{LAYER_SIZE, Mips};

/// Sky and cloud textures one sky can hold.
pub const LAYERS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    tint: [f32; 4],
    weights: [[f32; 4]; 2],
    sun: [f32; 4],
    clouds: [f32; 4],
    cloud_colour: [f32; 4],
}

pub struct Sky {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    /// The texture id in each layer.
    ids: Vec<u32>,
    /// When the sky was made, for the clouds' drift.
    start: std::time::Instant,
}

impl Sky {
    /// A sky of textures `ids` (the first [`LAYERS`]) with their mip chains.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, ids: &[u32], layers: &[Mips]) -> Self {
        let mut layers: Vec<Mips> = layers.iter().take(LAYERS).cloned().collect();
        // Always all layers, padded with black, as the shader reads them all.
        while layers.len() < LAYERS {
            layers.push(black_mips());
        }
        let view = crate::render::layer_array(device, queue, "sky", &layers);
        let shader = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            // Clouds tile both ways; the sky lookup keeps itself inside.
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // Behind everything: drawn first, without touching depth.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            uniforms,
            group,
            ids: ids.iter().take(LAYERS).copied().collect(),
            start: std::time::Instant::now(),
        }
    }

    /// Whether `light` has any of this sky's textures to show.
    pub fn shows(&self, light: &Light) -> bool {
        light.sky.iter().any(|(id, _)| self.ids.contains(id))
    }

    pub fn prepare(&self, queue: &wgpu::Queue, view_proj: Mat4, eye: Vec3, light: &Light) {
        let mut weights = [[0.0; 4]; 2];
        for (id, w) in &light.sky {
            if let Some(k) = self.ids.iter().position(|i| i == id) {
                weights[k / 4][k % 4] += w;
            }
        }
        let layer = |id: u32| {
            self.ids
                .iter()
                .position(|&i| i == id && id != 0)
                .map_or(-1.0, |k| k as f32)
        };
        let [(a, oa), (b, ob)] = light.clouds;
        let [cr, cg, cb] = light.cloud_colour;
        let t = light.sky_tint;
        let uniforms = Uniforms {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            eye: eye.extend(1.0).to_array(),
            tint: [t[0], t[1], t[2], 1.0],
            weights,
            sun: light.sun_dir.extend(0.0).to_array(),
            clouds: [layer(a), layer(b), oa, ob],
            cloud_colour: [cr, cg, cb, self.start.elapsed().as_secs_f32()],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn black_mips() -> Mips {
    let mut chain = Vec::new();
    let mut side = LAYER_SIZE as usize;
    loop {
        chain.push([0u8, 0, 0, 255].repeat(side * side));
        if side == 1 {
            break;
        }
        side /= 2;
    }
    chain
}
