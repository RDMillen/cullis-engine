//! Draws the world's particle effects (`engine_world::particles`) as
//! camera-facing sprites: additive for flames, alpha-blended for smoke.

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use engine_core::assets::AssetCache;
use engine_world::particles::{ParticleSystem, colour_at};
use glam::{Mat4, Vec3};

pub use engine_world::particles::EmitterRef;

use crate::render::{COLOR_FORMAT, DEPTH_FORMAT};

/// Metres per unit of a layer's size (sizes are stored in thousandths).
const SIZE_SCALE: f32 = 1.0;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    colour: [f32; 4],
    layer: f32,
}

/// The region's particle effects and what draws them.
pub struct Particles {
    /// What is simulated.
    pub system: ParticleSystem,
    additive: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    capacity: usize,
    /// Vertices written this frame: alpha-blended ones first, then additive.
    alpha_count: u32,
    additive_count: u32,
    scratch: Vec<Vertex>,
}

impl Particles {
    /// The emitters named at `placed` (emitter name, world position), loaded
    /// from `Misc/pc/effects.big` with their sprites.
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        assets: &AssetCache,
        placed: &[(EmitterRef, Vec3)],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let system = ParticleSystem::load(assets, placed)?;
        // At least two layers, so the array binds as an array everywhere.
        let mut layers = crate::themes::load_ids(assets, &system.textures)?;
        while layers.len() < 2 {
            layers.push(layers.first().cloned().unwrap_or_else(|| {
                vec![vec![
                    0;
                    (crate::themes::LAYER_SIZE * crate::themes::LAYER_SIZE * 4)
                        as usize
                ]]
            }));
        }
        let view = crate::render::layer_array(device, queue, "particle sprites", &layers);
        Ok(Self::with(device, system, &view))
    }

    fn with(device: &wgpu::Device, system: ParticleSystem, view: &wgpu::TextureView) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("particles.wgsl"));
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle uniforms"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = |label, entry, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                // Hidden behind walls, but not hiding each other.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let additive = pipeline(
            "particles additive",
            "fs_additive",
            wgpu::BlendState {
                color: add,
                alpha: add,
            },
        );
        let alpha = pipeline(
            "particles alpha",
            "fs_alpha",
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let capacity = 1024;
        let vertices = vertex_buffer(device, capacity);
        Self {
            system,
            additive,
            alpha,
            uniforms,
            group,
            vertices,
            capacity,
            alpha_count: 0,
            additive_count: 0,
            scratch: Vec::new(),
        }
    }

    /// How many emitters were placed.
    pub fn len(&self) -> usize {
        self.system.emitters.len()
    }

    /// Writes this frame's sprites, facing the camera at `eye`. Smoke (alpha-
    /// blended layers) is lit by `smoke_light` (linear RGB); flames light
    /// themselves.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        eye: Vec3,
        smoke_light: [f32; 3],
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&view_proj.to_cols_array()),
        );
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        for pass_additive in [false, true] {
            for emitter in &self.system.emitters {
                let to_eye = (eye - emitter.position).normalize_or(Vec3::Y);
                let right = to_eye.cross(Vec3::Z).normalize_or(Vec3::X);
                let up = right.cross(to_eye).normalize_or(Vec3::Z);
                for layer in &emitter.layers {
                    if layer.look.additive != pass_additive {
                        continue;
                    }
                    for p in &layer.particles {
                        let t = p.progress();
                        let [s0, s1] = layer.look.size;
                        let half = (s0 + (s1 - s0) * t) * SIZE_SCALE * 0.5;
                        if half <= 0.0 {
                            continue;
                        }
                        // Smoke (alpha-blended) reads its keys doubled; flames as
                        // stored (by eye).
                        let doubled = !layer.look.additive;
                        let mut colour = colour_at(&layer.look.sprite.colours, t, doubled);
                        if !layer.look.additive {
                            for (c, l) in colour.iter_mut().zip(smoke_light) {
                                *c *= l;
                            }
                        }
                        let (r, u) = (right * half, up * half);
                        let corner = |dx: f32, dy: f32, uv: [f32; 2]| Vertex {
                            position: (p.position + r * dx + u * dy).to_array(),
                            uv,
                            colour,
                            layer: layer.texture as f32,
                        };
                        let quad = [
                            corner(-1.0, -1.0, [0.0, 1.0]),
                            corner(1.0, -1.0, [1.0, 1.0]),
                            corner(1.0, 1.0, [1.0, 0.0]),
                            corner(-1.0, 1.0, [0.0, 0.0]),
                        ];
                        scratch.extend_from_slice(&[
                            quad[0], quad[1], quad[2], quad[0], quad[2], quad[3],
                        ]);
                    }
                }
            }
            if pass_additive {
                self.additive_count = scratch.len() as u32 - self.alpha_count;
            } else {
                self.alpha_count = scratch.len() as u32;
            }
        }
        if scratch.len() > self.capacity {
            self.capacity = scratch.len().next_power_of_two();
            self.vertices = vertex_buffer(device, self.capacity);
        }
        if !scratch.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&scratch));
        }
        self.scratch = scratch;
    }

    /// Draws the sprites written by [`Particles::prepare`].
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.alpha_count + self.additive_count == 0 {
            return;
        }
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        if self.alpha_count > 0 {
            pass.set_pipeline(&self.alpha);
            pass.draw(0..self.alpha_count, 0..1);
        }
        if self.additive_count > 0 {
            pass.set_pipeline(&self.additive);
            pass.draw(
                self.alpha_count..self.alpha_count + self.additive_count,
                0..1,
            );
        }
    }
}

fn vertex_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle vertices"),
        size: (capacity * size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
