//! Draws a terrain mesh and its sea into an offscreen colour texture, shown by the
//! window and read back for screenshots.

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use glam::{Mat4, Vec3};

use crate::collide::Colliders;
use crate::model::ModelRenderer;
use crate::terrain::{TerrainMesh, Vertex};
use crate::themes::{LAYER_SIZE, Mips, ThemeTextures};
use crate::walk::{HeightGrid, WalkGrid};

/// World units covered by one repeat of a theme texture. Set by eye; to be
/// compared with the game.
pub const THEME_TILE: f32 = 12.0;

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const SKY: wgpu::Color = wgpu::Color {
    r: 0.45,
    g: 0.62,
    b: 0.85,
    a: 1.0,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    sun: [f32; 4],
    /// Theme grid origin (x, y) and size (w, h) in world units.
    grid: [f32; 4],
    /// x: world units per texture repeat.
    params: [f32; 4],
}

pub struct Target {
    pub size: [u32; 2],
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
}

/// Vertex and index buffers for one indexed draw.
struct Geometry {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

impl Geometry {
    fn new(device: &wgpu::Device, label: &str, vertices: &[Vertex], indices: &[u32]) -> Self {
        let buffer = |contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Self {
            vertices: buffer(bytemuck::cast_slice(vertices), wgpu::BufferUsages::VERTEX),
            indices: buffer(bytemuck::cast_slice(indices), wgpu::BufferUsages::INDEX),
            count: indices.len() as u32,
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}

pub struct TerrainRenderer {
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    nav: Option<Geometry>,
    /// Draw the navigation overlay.
    pub show_nav: bool,
    terrain: Geometry,
    sea: Option<Geometry>,
    /// Meshes placed on the terrain, drawn after it and before the sea.
    pub things: Option<ModelRenderer>,
    /// The walking Hero and its model-to-world transform.
    pub hero: Option<(ModelRenderer, Mat4)>,
    /// Buildings with interiors, and the one the Hero is inside (its exterior is
    /// hidden).
    pub buildings: Vec<crate::model::Building>,
    pub inside: Option<usize>,
    /// Creatures standing in the region.
    pub npcs: Vec<crate::npc::Npc>,
    /// Plays the Hero's animations, when his mesh is skinned.
    pub hero_animator: Option<crate::skin::Animator>,
    /// Region exits in the region's own maps.
    pub exits: Vec<crate::terrain::RegionExit>,
    /// Placed meshes as triangles, for camera collision.
    pub colliders: Option<Colliders>,
    /// Where the Hero can walk and the ground height, if the region has them.
    pub walk: Vec<WalkGrid>,
    pub heights: Option<HeightGrid>,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    grid: [f32; 4],
    pub target: Option<Target>,
}

/// Creates a 2D array texture from per-layer mip chains of `LAYER_SIZE` images.
fn layer_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    layers: &[Mips],
) -> wgpu::TextureView {
    let mip_count = layers[0].len() as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: LAYER_SIZE,
            height: LAYER_SIZE,
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, mips) in layers.iter().enumerate() {
        for (level, data) in mips.iter().enumerate() {
            let side = (LAYER_SIZE >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

impl TerrainRenderer {
    /// Triangles camera rays should ignore: the hidden exterior, if any.
    pub fn hidden_triangles(&self) -> std::ops::Range<u32> {
        self.inside
            .map_or(0..0, |i| self.buildings[i].exterior_triangles.clone())
    }

    /// Hides the exterior of the building whose interior contains `p`, and
    /// shows it again once `p` leaves.
    pub fn update_interior(&mut self, p: glam::Vec3) {
        let inside = self.buildings.iter().position(|b| b.contains(p));
        if inside == self.inside {
            return;
        }
        self.inside = inside;
        if let Some(things) = &mut self.things {
            things.set_hidden(inside.map_or(0..0, |i| self.buildings[i].exterior_draws.clone()));
        }
    }

    /// Replaces the navigation overlay.
    pub fn set_navigation(&mut self, device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) {
        self.nav =
            (!indices.is_empty()).then(|| Geometry::new(device, "navigation", vertices, indices));
    }

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &TerrainMesh,
        themes: Option<&ThemeTextures>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("terrain.wgsl"));
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture_entry = |binding, sample_type, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let filterable = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(
                    1,
                    wgpu::TextureSampleType::Uint,
                    wgpu::TextureViewDimension::D2,
                ),
                texture_entry(2, filterable, wgpu::TextureViewDimension::D2Array),
                texture_entry(3, filterable, wgpu::TextureViewDimension::D2Array),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Theme grid: one texel per grid point, [layer, layer, weight, weight].
        let (grid_size, grid_origin, cells) = match &mesh.themes {
            Some(g) => (g.size, g.origin, g.cells.clone()),
            None => ([1, 1], [0.0, 0.0], vec![[0; 4]]),
        };
        let grid_texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("theme grid"),
                size: wgpu::Extent3d {
                    width: grid_size[0],
                    height: grid_size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&cells),
        );
        let grey: Vec<Mips> = vec![vec![
            [128u8, 128, 128, 255].repeat((LAYER_SIZE * LAYER_SIZE) as usize),
        ]];
        let (base, cliff) = match themes {
            Some(t) if !t.base.is_empty() => (
                layer_array(device, queue, "theme base", &t.base),
                layer_array(device, queue, "theme cliff", &t.cliff),
            ),
            _ => (
                layer_array(device, queue, "theme base", &grey),
                layer_array(device, queue, "theme cliff", &grey),
            ),
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &grid_texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&base),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&cliff),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let grid = [
            grid_origin[0],
            grid_origin[1],
            grid_size[0] as f32,
            grid_size[1] as f32,
        ];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        // Terrain is opaque; water blends over it, depth-tested but not written.
        let make_pipeline = |label, fs_entry, blend, depth_write| {
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
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let terrain_pipeline = make_pipeline("terrain", "fs_main", None, true);
        let water_pipeline = make_pipeline(
            "water",
            "fs_water",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let overlay_pipeline = make_pipeline(
            "overlay",
            "fs_overlay",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let nav = (!mesh.nav_indices.is_empty())
            .then(|| Geometry::new(device, "navigation", &mesh.nav_vertices, &mesh.nav_indices));
        let terrain = Geometry::new(device, "terrain", &mesh.vertices, &mesh.indices);
        let sea = mesh
            .sea_quad()
            .map(|(vertices, indices)| Geometry::new(device, "sea", &vertices, &indices));
        Self {
            terrain_pipeline,
            water_pipeline,
            overlay_pipeline,
            nav,
            show_nav: false,
            terrain,
            sea,
            things: None,
            hero: None,
            hero_animator: None,
            npcs: Vec::new(),
            buildings: Vec::new(),
            inside: None,
            colliders: None,
            exits: mesh.exits.clone(),
            walk: mesh.walk.clone(),
            heights: mesh.heights.clone(),
            uniforms,
            bind_group,
            grid,
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
            grid: self.grid,
            params: [THEME_TILE, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        if let Some(things) = &self.things {
            things.prepare(queue, view_proj);
        }
        if let Some((hero, transform)) = &self.hero {
            hero.prepare(queue, view_proj * *transform);
        }
        for npc in &self.npcs {
            for (part, _) in &npc.parts {
                part.prepare(queue, view_proj * npc.transform);
            }
        }

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain"),
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
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.terrain_pipeline);
            self.terrain.draw(&mut pass);
            if let Some(things) = &self.things {
                things.draw(&mut pass);
                pass.set_bind_group(0, &self.bind_group, &[]);
            }
            if let Some((hero, _)) = &self.hero {
                hero.draw(&mut pass);
                pass.set_bind_group(0, &self.bind_group, &[]);
            }
            for (part, _) in self.npcs.iter().flat_map(|n| &n.parts) {
                part.draw(&mut pass);
            }
            if !self.npcs.is_empty() {
                pass.set_bind_group(0, &self.bind_group, &[]);
            }
            if let (true, Some(nav)) = (self.show_nav, &self.nav) {
                pass.set_pipeline(&self.overlay_pipeline);
                nav.draw(&mut pass);
            }
            if let Some(sea) = &self.sea {
                pass.set_pipeline(&self.water_pipeline);
                sea.draw(&mut pass);
            }
        }
        queue.submit([encoder.finish()]);
        recreated
    }
}

/// Reads a target back as tightly packed RGBA8 rows.
pub fn read_rgba(target: &Target, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
    {
        let [w, h] = target.size;
        let padded = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback buffer"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll device");
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .expect("read mapped buffer");
        mapped
            .chunks(padded as usize)
            .flat_map(|row| &row[..(w * 4) as usize])
            .copied()
            .collect()
    }
}

pub fn create_target(device: &wgpu::Device, size: [u32; 2]) -> Target {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let texture = |label, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        "terrain color",
        COLOR_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        "terrain depth",
        DEPTH_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    Target {
        size,
        color_view: color.create_view(&Default::default()),
        color,
        depth_view: depth.create_view(&Default::default()),
    }
}
