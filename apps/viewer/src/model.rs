//! Loads a static mesh with its textures and draws it into an offscreen target.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use fable_data::big::BigFile;
use fable_data::mesh::{Mesh, STATIC_MESH_TYPE};
use fable_data::texture::{Texture, TextureInfo};
use glam::{Mat4, Vec3};

use crate::render::{COLOR_FORMAT, DEPTH_FORMAT, SKY, Target, create_target};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// An RGBA8 image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A range of `indices` drawn with one texture.
pub struct Draw {
    pub first: u32,
    pub count: u32,
    pub image: usize,
}

pub struct ModelData {
    pub name: String,
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<Draw>,
    pub images: Vec<Image>,
    pub lo: Vec3,
    pub hi: Vec3,
}

/// Loads the most detailed level of the first static mesh whose name contains
/// `needle` (case-insensitive), with its materials' main textures.
pub fn load_mesh(data_dir: &Path, needle: &str) -> Result<ModelData, Box<dyn std::error::Error>> {
    let needle = needle.to_ascii_uppercase();
    let path = data_dir.join("graphics/graphics.big");
    let mut file = BufReader::new(File::open(&path)?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no MBANK_ALLMESHES")?
        .read_index(&mut file)?;
    let entry = index
        .entries
        .iter()
        .filter(|e| e.entry_type == STATIC_MESH_TYPE)
        .find(|e| e.name.eq_ignore_ascii_case(&needle))
        .or_else(|| {
            index
                .entries
                .iter()
                .filter(|e| e.entry_type == STATIC_MESH_TYPE)
                .find(|e| e.name.contains(&needle))
        })
        .ok_or_else(|| format!("no static mesh matching {needle}"))?;
    let mesh = Mesh::parse(&entry.info, &entry.read_data(&mut file)?)?;
    let lod = &mesh.lods[0];

    let mut textures = TextureBank::open(data_dir)?;
    let mut model = ModelData {
        name: entry.name.clone(),
        vertices: Vec::new(),
        indices: Vec::new(),
        draws: Vec::new(),
        images: vec![Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
        lo: Vec3::splat(f32::INFINITY),
        hi: Vec3::splat(f32::NEG_INFINITY),
    };
    let mut loaded: HashMap<u32, usize> = HashMap::new();
    for chunk in &lod.chunks {
        let base = model.vertices.len() as u32;
        let (positions, normals, uvs) = (chunk.positions(), chunk.normals(), chunk.uvs());
        model
            .vertices
            .extend((0..positions.len()).map(|i| ModelVertex {
                position: positions[i],
                normal: normals[i],
                uv: uvs[i],
            }));
        for sub in &chunk.sub_meshes {
            let material = &lod.materials[sub.material as usize];
            let image = match material.texture {
                0 => 0,
                id => *loaded.entry(id).or_insert_with(|| match textures.load(id) {
                    Ok(img) => {
                        model.images.push(img);
                        model.images.len() - 1
                    }
                    Err(e) => {
                        eprintln!("texture {id} for {}: {e}", material.name);
                        0
                    }
                }),
            };
            let first = model.indices.len() as u32;
            for t in chunk.triangles(sub) {
                model.indices.extend(t.map(|i| base + u32::from(i)));
            }
            model.draws.push(Draw {
                first,
                count: model.indices.len() as u32 - first,
                image,
            });
        }
    }
    // Frame what is actually drawn; the info block's box is not always tight.
    for v in &model.vertices {
        model.lo = model.lo.min(Vec3::from_array(v.position));
        model.hi = model.hi.max(Vec3::from_array(v.position));
    }
    Ok(model)
}

struct TextureBank {
    file: BufReader<File>,
    entries: HashMap<u32, fable_data::bank::Entry>,
}

impl TextureBank {
    fn open(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = BufReader::new(File::open(data_dir.join("graphics/pc/textures.big"))?);
        let big = BigFile::read(&mut file)?;
        let index = big
            .bank("GBANK_MAIN_PC")
            .ok_or("no GBANK_MAIN_PC")?
            .read_index(&mut file)?;
        let entries = index.entries.into_iter().map(|e| (e.id, e)).collect();
        Ok(Self { file, entries })
    }

    fn load(&mut self, id: u32) -> Result<Image, Box<dyn std::error::Error>> {
        let entry = self
            .entries
            .get(&id)
            .ok_or_else(|| format!("no texture {id}"))?;
        let info = TextureInfo::parse(&entry.info)?;
        let texture = Texture::unpack(&info, entry.entry_type, &entry.read_data(&mut self.file)?)?;
        Ok(Image {
            width: texture.width,
            height: texture.height,
            rgba: texture.to_rgba(0, 0)?,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    sun: [f32; 4],
}

pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_groups: Vec<wgpu::BindGroup>,
    draws: Vec<(u32, u32, usize)>,
    pub target: Option<Target>,
}

impl ModelRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, model: &ModelData) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("model.wgsl"));
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_groups = model
            .images
            .iter()
            .map(|img| {
                let texture = device.create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some("model texture"),
                        size: wgpu::Extent3d {
                            width: img.width,
                            height: img.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &img.rgba,
                );
                let view = texture.create_view(&Default::default());
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model texture"),
                    layout: &texture_layout,
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
                })
            })
            .collect();
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<ModelVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            // Winding is not known yet, so draw both sides.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Self {
            pipeline,
            vertices: buffer(
                "model vertices",
                bytemuck::cast_slice(&model.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "model indices",
                bytemuck::cast_slice(&model.indices),
                wgpu::BufferUsages::INDEX,
            ),
            uniforms,
            uniform_group,
            texture_groups,
            draws: model
                .draws
                .iter()
                .map(|d| (d.first, d.count, d.image))
                .collect(),
            target: None,
        }
    }

    /// Renders into the target texture, recreating it if the size changed.
    /// Returns true if the target was recreated.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        view_proj: Mat4,
    ) -> bool {
        let size = [size[0].max(1), size[1].max(1)];
        let recreated = self.target.as_ref().is_none_or(|t| t.size != size);
        if recreated {
            self.target = Some(create_target(device, size));
        }
        let target = self.target.as_ref().unwrap();
        let sun = Vec3::new(-0.4, -0.5, 0.75).normalize();
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            sun: sun.extend(0.0).to_array(),
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("model"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            for &(first, count, image) in &self.draws {
                pass.set_bind_group(1, &self.texture_groups[image], &[]);
                pass.draw_indexed(first..first + count, 0, 0..1);
            }
        }
        queue.submit([encoder.finish()]);
        recreated
    }
}
