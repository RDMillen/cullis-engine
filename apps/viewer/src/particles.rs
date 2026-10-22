//! Particle effects: the emitters meshes start at their `CREATEPARTICLE`
//! dummies (a fireplace's flames), simulated near the camera and drawn as
//! camera-facing sprites.
//!
//! Texture, colours over life and blending come from the emitter data; birth
//! rate, lifetime and sizes are readings not yet confirmed, and the starting
//! motion is a stand-in by eye (see `docs/formats/particles.md`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use fable_data::particles::{Emitter as EmitterData, SpriteLayer};
use glam::{Mat4, Vec3};

use crate::render::{COLOR_FORMAT, DEPTH_FORMAT};

/// Emitters this far from the camera's focus are not simulated (world units).
const REACH: f32 = 50.0;
/// Most particles a layer keeps at once.
const MAX_PER_LAYER: usize = 256;
/// Metres per unit of a layer's size (sizes are stored in thousandths).
const SIZE_SCALE: f32 = 1.0;
/// Starting upward speed of flames and of smoke, and sideways spread (m/s),
/// by eye: the emitter's own motion fields are not decoded.
const FLAME_RISE: f32 = 0.35;
const SMOKE_RISE: f32 = 0.25;
const SPREAD: f32 = 0.08;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    colour: [f32; 4],
    layer: f32,
}

struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
}

struct Layer {
    look: SpriteLayer,
    /// Its sprite's layer in the texture array.
    texture: f32,
    particles: Vec<Particle>,
    /// Particles owed by the birth rate, carried between frames.
    owed: f32,
}

struct Emitter {
    position: Vec3,
    layers: Vec<Layer>,
}

/// The region's particle effects and what draws them.
pub struct Particles {
    emitters: Vec<Emitter>,
    additive: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    capacity: usize,
    /// Vertices written this frame: alpha-blended ones first, then additive.
    alpha_count: u32,
    additive_count: u32,
    rng: u64,
    scratch: Vec<Vertex>,
}

impl Particles {
    /// The emitters named at `placed` (emitter name, world position), loaded
    /// from `Misc/pc/effects.big` with their sprites.
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data_dir: &Path,
        placed: &[(String, Vec3)],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let path = data_dir.join("Misc/pc/effects.big");
        let mut file = BufReader::new(File::open(&path)?);
        let big = fable_data::big::BigFile::read(&mut file)?;
        let index = big
            .bank("PARTICLE_MAIN_PC")
            .ok_or("no PARTICLE_MAIN_PC")?
            .read_index(&mut file)?;
        let mut parsed: HashMap<String, Option<Vec<SpriteLayer>>> = HashMap::new();
        let mut textures: Vec<u32> = Vec::new();
        let mut emitters = Vec::new();
        for (name, position) in placed {
            let looks = parsed.entry(name.to_ascii_uppercase()).or_insert_with(|| {
                let entry = index
                    .entries
                    .iter()
                    .find(|e| e.name.eq_ignore_ascii_case(name))?;
                let data = entry.read_data(&mut file).ok()?;
                let emitter = EmitterData::parse(&data)
                    .map_err(|e| eprintln!("particles {name}: {e}"))
                    .ok()?;
                Some(
                    emitter
                        .layers
                        .iter()
                        .filter_map(|l| l.sprite_layer())
                        .collect(),
                )
            });
            let Some(looks) = looks else {
                eprintln!("particles: no emitter {name}");
                continue;
            };
            let layers = looks
                .iter()
                .map(|look| {
                    let id = look.sprite.texture;
                    let k = textures.iter().position(|&t| t == id).unwrap_or_else(|| {
                        textures.push(id);
                        textures.len() - 1
                    });
                    Layer {
                        look: *look,
                        texture: k as f32,
                        particles: Vec::new(),
                        owed: 0.0,
                    }
                })
                .collect();
            emitters.push(Emitter {
                position: *position,
                layers,
            });
        }
        // At least two layers, so the array binds as an array everywhere.
        let mut layers = crate::themes::load_ids(data_dir, &textures)?;
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
        Ok(Self::with(device, emitters, &view))
    }

    fn with(device: &wgpu::Device, emitters: Vec<Emitter>, view: &wgpu::TextureView) -> Self {
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
            emitters,
            additive,
            alpha,
            uniforms,
            group,
            vertices,
            capacity,
            alpha_count: 0,
            additive_count: 0,
            rng: 0x2545_f491_4f6c_dd1d,
            scratch: Vec::new(),
        }
    }

    /// How many emitters were placed.
    pub fn len(&self) -> usize {
        self.emitters.len()
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Moves time on `dt` seconds for the emitters near `focus`: particles are
    /// born, rise and die.
    pub fn update(&mut self, dt: f32, focus: Vec3) {
        let mut emitters = std::mem::take(&mut self.emitters);
        for emitter in &mut emitters {
            let near = emitter.position.distance(focus) < REACH;
            for layer in &mut emitter.layers {
                let look = layer.look;
                for p in &mut layer.particles {
                    p.velocity.z -= look.fall * dt;
                    p.position += p.velocity * dt;
                    p.age += dt;
                }
                layer.particles.retain(|p| p.age < p.life);
                if !near {
                    layer.owed = 0.0;
                    continue;
                }
                layer.owed += look.rate * dt;
                while layer.owed >= 1.0 && layer.particles.len() < MAX_PER_LAYER {
                    layer.owed -= 1.0;
                    let rise = if look.additive {
                        FLAME_RISE
                    } else {
                        SMOKE_RISE
                    };
                    let (a, b, c, d) = (self.random(), self.random(), self.random(), self.random());
                    let jitter = Vec3::new(a - 0.5, b - 0.5, 0.0);
                    layer.particles.push(Particle {
                        position: emitter.position + jitter * 0.1,
                        velocity: jitter * 2.0 * SPREAD + Vec3::Z * rise * (0.7 + 0.6 * c),
                        age: 0.0,
                        life: look.lifetime.max(0.05) * (0.8 + 0.4 * d),
                    });
                }
                layer.owed = layer.owed.min(1.0);
            }
        }
        self.emitters = emitters;
    }

    /// Writes this frame's sprites, facing the camera at `eye`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        eye: Vec3,
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&view_proj.to_cols_array()),
        );
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        for pass_additive in [false, true] {
            for emitter in &self.emitters {
                let to_eye = (eye - emitter.position).normalize_or(Vec3::Y);
                let right = to_eye.cross(Vec3::Z).normalize_or(Vec3::X);
                let up = right.cross(to_eye).normalize_or(Vec3::Z);
                for layer in &emitter.layers {
                    if layer.look.additive != pass_additive {
                        continue;
                    }
                    for p in &layer.particles {
                        let t = (p.age / p.life).clamp(0.0, 1.0);
                        let [s0, s1] = layer.look.size;
                        let half = (s0 + (s1 - s0) * t) * SIZE_SCALE * 0.5;
                        if half <= 0.0 {
                            continue;
                        }
                        let colour = colour_at(&layer.look.sprite.colours, t);
                        let (r, u) = (right * half, up * half);
                        let corner = |dx: f32, dy: f32, uv: [f32; 2]| Vertex {
                            position: (p.position + r * dx + u * dy).to_array(),
                            uv,
                            colour,
                            layer: layer.texture,
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

/// A particle's colour at `t` (0..1 of its life), linear RGB and alpha: its
/// birth colour to its death colour, through the middle one unless that is
/// opaque black (left unused in torches and candles).
fn colour_at(keys: &[[u8; 4]; 3], t: f32) -> [f32; 4] {
    let f = |k: [u8; 4]| k.map(|c| f32::from(c) / 255.0);
    let (a, b, c) = (f(keys[0]), f(keys[1]), f(keys[2]));
    let unused_middle = keys[1] == [0, 0, 0, 255];
    let mix = |x: [f32; 4], y: [f32; 4], s: f32| std::array::from_fn(|i| x[i] + (y[i] - x[i]) * s);
    let gamma: [f32; 4] = if unused_middle {
        mix(a, c, t)
    } else if t < 0.5 {
        mix(a, b, t * 2.0)
    } else {
        mix(b, c, t * 2.0 - 1.0)
    };
    [
        gamma[0].powf(2.2),
        gamma[1].powf(2.2),
        gamma[2].powf(2.2),
        gamma[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_run_birth_to_death() {
        let keys = [[255, 255, 255, 255], [0, 0, 0, 255], [255, 0, 0, 0]];
        assert_eq!(colour_at(&keys, 0.0), [1.0, 1.0, 1.0, 1.0]);
        let end = colour_at(&keys, 1.0);
        assert_eq!(
            (end[0], end[1], end[3]),
            (1.0, 0.0, 0.0),
            "the unused black middle is skipped"
        );
        let keys = [[0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 0]];
        assert_eq!(colour_at(&keys, 0.5)[3], 1.0, "through a used middle");
    }
}
