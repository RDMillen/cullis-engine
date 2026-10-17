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
/// How much brighter nights are made than the table's colours (by eye: they
/// read about 30% too dark against the original).
const NIGHT_LIFT: f32 = 0.3;

/// How dark sun shadows are: the share of the sun's light they take away.
const SHADOW_STRENGTH: f32 = 1.0;

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
    sun_colour: [f32; 4],
    ambient: [f32; 4],
    shadow_proj: [[f32; 4]; 4],
    shadow: [f32; 4],
    points: [[f32; 4]; 2 * crate::lighting::MAX_POINTS],
    point_count: [f32; 4],
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

/// Something that can glow as usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    Npc(usize),
    Door(usize),
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
    /// Doors, posed open or closed.
    pub doors: Vec<crate::door::Door>,
    /// Creatures standing in the region.
    pub npcs: Vec<crate::npc::Npc>,
    /// The Hero's animations from his animation table, and his bone configs.
    pub hero_animations: crate::HeroAnimations,
    pub hero_morphs: Vec<String>,
    /// Plays the Hero's animations, when his mesh is skinned.
    pub hero_animator: Option<crate::skin::Animator>,
    /// The Hero's kick among his clips, if it loaded.
    pub hero_kick: Option<usize>,
    /// The region's music and ambience.
    pub soundscape: crate::soundscape::Soundscape,
    /// The light now, the region's themes it comes from, and the sky; the
    /// clear colour shows where there is no sky.
    pub light: crate::lighting::Light,
    /// The light inside buildings: their interior meshes, what stands in them,
    /// and creatures there.
    pub indoor_light: crate::lighting::Light,
    pub lighting: Option<crate::lighting::Lighting>,
    pub sky: Option<crate::sky::Sky>,
    pub clear: wgpu::Color,
    /// Where the camera looks: the middle of the shadowed area.
    pub focus: Vec3,
    /// The region's lamps, fires and light markers, and when the renderer was
    /// made (for flicker).
    pub point_lights: Vec<crate::terrain::PointLight>,
    started: std::time::Instant,
    /// Sun shadows: the map, the terrain's depth pipeline and its uniforms
    /// (the sun's view), and the group binding them.
    shadow_map: crate::shadow::ShadowMap,
    terrain_shadow_pipeline: wgpu::RenderPipeline,
    shadow_uniforms: wgpu::Buffer,
    shadow_bind_group: wgpu::BindGroup,
    /// What glows as usable (a creature or door) and its colour and strength,
    /// and where the camera is (for the glow's rim).
    pub highlight: Option<(Highlight, [f32; 4])>,
    pub eye: Vec3,
    /// The region's quest-state sections (for choosing which are shown).
    pub sections: Vec<String>,
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
pub fn layer_array(
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
    /// Triangles camera rays should ignore: the hidden roof, if any.
    pub fn hidden_triangles(&self) -> &[std::ops::Range<u32>] {
        self.inside
            .map_or(&[], |i| self.buildings[i].roof_triangles.as_slice())
    }

    /// Hides the roof of the building whose interior contains `p`, and shows it
    /// again once `p` leaves.
    pub fn update_interior(&mut self, p: glam::Vec3) {
        let inside = self.buildings.iter().position(|b| b.contains(p));
        if inside == self.inside {
            return;
        }
        self.inside = inside;
        if let Some(things) = &mut self.things {
            things
                .set_hidden(inside.map_or_else(Vec::new, |i| self.buildings[i].roof_draws.clone()));
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
        let shadow_layout = crate::shadow::layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&layout), Some(&shadow_layout)],
            ..Default::default()
        });
        let shadow_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain shadow uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grid_view = grid_texture.create_view(&Default::default());
        let group_with = |label, buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&grid_view),
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
            })
        };
        let shadow_bind_group = group_with("terrain shadow", &shadow_uniforms);
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain shadow"),
                bind_group_layouts: &[Some(&layout)],
                ..Default::default()
            });
        let terrain_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain shadow"),
                layout: Some(&shadow_pipeline_layout),
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
                fragment: None,
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(crate::shadow::depth_state()),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
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
                    depth_compare: Some(wgpu::CompareFunction::Greater),
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
            hero_kick: None,
            soundscape: Default::default(),
            light: Default::default(),
            indoor_light: Default::default(),
            lighting: None,
            sky: None,
            clear: SKY,
            focus: Vec3::ZERO,
            point_lights: mesh.lights.clone(),
            started: std::time::Instant::now(),
            shadow_map: crate::shadow::ShadowMap::new(device, crate::shadow::SIZE),
            terrain_shadow_pipeline,
            shadow_uniforms,
            shadow_bind_group,
            highlight: None,
            eye: Vec3::ZERO,
            hero_animations: Default::default(),
            hero_morphs: Vec::new(),
            npcs: Vec::new(),
            doors: Vec::new(),
            buildings: Vec::new(),
            inside: None,
            colliders: None,
            exits: mesh.exits.clone(),
            sections: mesh.sections.iter().cloned().collect(),
            walk: mesh.walk.clone(),
            heights: mesh.heights.clone(),
            uniforms,
            bind_group,
            grid,
            target: None,
        }
    }

    /// Sets the light to the time of day `hour`, indoors while the Hero is in a
    /// building.
    pub fn set_hour(&mut self, hour: f32, brightness: f32, highlight_limit: f32) {
        let Some(lighting) = &self.lighting else {
            return;
        };
        self.light = lighting.at(hour, false);
        self.indoor_light = lighting.at(hour, true);
        // Nights lifted by up to NIGHT_LIFT as the sun's light fades (by eye).
        let sun = self.light.sun.iter().sum::<f32>() / 3.0;
        let night = 1.0 - ((sun - 0.15) / 0.5).clamp(0.0, 1.0);
        for light in [&mut self.light, &mut self.indoor_light] {
            light.brightness = brightness * (1.0 + NIGHT_LIFT * night);
            light.sky_brightness = brightness * (1.0 - (1.0 - crate::lighting::NIGHT_SKY) * night);
            light.highlight_limit = highlight_limit;
        }
        // Without a sky (indoor themes), a dark background.
        let shown = self.sky.as_ref().is_some_and(|s| s.shows(&self.light));
        self.clear = if shown {
            wgpu::Color::BLACK
        } else {
            wgpu::Color {
                r: 0.01,
                g: 0.01,
                b: 0.012,
                a: 1.0,
            }
        };
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
        // Shadows outdoors, where the sun is up enough to cast them.
        // Sun shadows outdoors whenever the sun is up enough to cast them,
        // wherever the Hero is. Indoor light keeps the sun's view (so models
        // lit indoors still cast shadows) at no strength.
        let sun_up = self.light.sun.iter().any(|&c| c > 0.15);
        self.light.shadow_proj = crate::shadow::view_proj(self.light.sun_dir, self.focus);
        let texel = 1.0 / crate::shadow::SIZE as f32;
        let strength = if sun_up { SHADOW_STRENGTH } else { 0.0 };
        self.light.shadow = [strength, texel, 0.0004, 0.0];
        self.indoor_light.shadow_proj = self.light.shadow_proj;
        self.indoor_light.shadow = [0.0, texel, 0.0004, 0.0];
        let time = self.started.elapsed().as_secs_f32();
        for light in [&mut self.light, &mut self.indoor_light] {
            crate::lighting::set_points(light, &self.point_lights, self.focus, time);
        }
        let light = &self.light;
        let [sr, sg, sb] = light.sun;
        let [ar, ag, ab] = light.ambient;
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            sun: light.sun_dir.extend(0.0).to_array(),
            grid: self.grid,
            params: [THEME_TILE, 0.0, 0.0, 0.0],
            sun_colour: [sr, sg, sb, light.highlight_limit],
            ambient: [ar, ag, ab, light.brightness],
            shadow_proj: light.shadow_proj.to_cols_array_2d(),
            shadow: light.shadow,
            points: light.points,
            point_count: [light.point_count as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let shadowed = sun_up;
        if shadowed {
            let mut from_sun = uniforms;
            from_sun.view_proj = light.shadow_proj.to_cols_array_2d();
            queue.write_buffer(&self.shadow_uniforms, 0, bytemuck::bytes_of(&from_sun));
        }
        let sky = self.sky.as_ref().filter(|s| s.shows(light));
        if let Some(sky) = sky {
            sky.prepare(queue, view_proj, self.eye, light);
        }
        if let Some(things) = &self.things {
            things.prepare(queue, view_proj, light);
            things.prepare_indoor(queue, view_proj, &self.indoor_light);
        }
        // Creatures and doors take the indoor light while inside a building.
        let light_at = |transform: &Mat4| {
            let p = transform.w_axis.truncate();
            if self.buildings.iter().any(|b| b.contains(p)) {
                &self.indoor_light
            } else {
                light
            }
        };
        if let Some((hero, transform)) = &self.hero {
            hero.prepare_placed(
                queue,
                view_proj,
                *transform,
                self.eye,
                [0.0; 4],
                light_at(transform),
            );
        }
        let glow = |what: Highlight| match self.highlight {
            Some((h, colour)) if h == what => colour,
            _ => [0.0; 4],
        };
        for (i, npc) in self.npcs.iter().enumerate().filter(|(_, n)| !n.hidden) {
            for (part, _) in &npc.parts {
                part.prepare_placed(
                    queue,
                    view_proj,
                    npc.transform,
                    self.eye,
                    glow(Highlight::Npc(i)),
                    light_at(&npc.transform),
                );
            }
        }
        for (i, door) in self.doors.iter().enumerate() {
            door.renderer.prepare_placed(
                queue,
                view_proj,
                door.transform,
                self.eye,
                glow(Highlight::Door(i)),
                light_at(&door.transform),
            );
        }

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            // Depth from the sun (cleared even when unused, so it reads as lit).
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if shadowed {
                pass.set_pipeline(&self.terrain_shadow_pipeline);
                pass.set_bind_group(0, &self.shadow_bind_group, &[]);
                self.terrain.draw(&mut pass);
                if let Some(things) = &self.things {
                    things.draw_shadow(&mut pass);
                }
                if let Some((hero, _)) = &self.hero {
                    hero.draw_shadow(&mut pass);
                }
                for (part, _) in self
                    .npcs
                    .iter()
                    .filter(|n| !n.hidden)
                    .flat_map(|n| &n.parts)
                {
                    part.draw_shadow(&mut pass);
                }
                for door in &self.doors {
                    door.renderer.draw_shadow(&mut pass);
                }
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some(sky) = sky {
                sky.draw(&mut pass);
            }
            // Terrain pipelines read the shadow map as group 1, models as 2.
            let terrain_groups = |pass: &mut wgpu::RenderPass<'_>| {
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_bind_group(1, &self.shadow_map.group, &[]);
            };
            terrain_groups(&mut pass);
            pass.set_pipeline(&self.terrain_pipeline);
            self.terrain.draw(&mut pass);
            pass.set_bind_group(2, &self.shadow_map.group, &[]);
            if let Some(things) = &self.things {
                things.draw(&mut pass);
            }
            if let Some((hero, _)) = &self.hero {
                hero.draw(&mut pass);
            }
            for (part, _) in self
                .npcs
                .iter()
                .filter(|n| !n.hidden)
                .flat_map(|n| &n.parts)
            {
                part.draw(&mut pass);
            }
            for door in &self.doors {
                door.renderer.draw(&mut pass);
            }
            terrain_groups(&mut pass);
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
