//! CullisEngine desktop viewer: a region's terrain from the Lev heightmaps, or a
//! static mesh with its textures.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--nav] [--screenshot OUT.png] [--size WxH] [--camera x,y,z,distance,yaw,pitch]
//! ```

mod camera;
mod collide;
mod model;
mod render;
mod skin;
mod terrain;
mod themes;
mod walk;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names};
use fable_data::mesh::{SKINNED_MESH_TYPE, STATIC_MESH_TYPE};
use fable_data::wld::Wld;
use glam::Mat4;

use camera::OrbitCamera;
use model::ModelRenderer;
use render::{Target, TerrainRenderer};

type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subject {
    Region(String),
    Mesh(String),
}

struct Args {
    subject: Subject,
    /// Screenshot camera override: target x, y, z, distance, yaw, pitch.
    camera: Option<[f32; 6]>,
    /// Draw the navigation overlay.
    nav: bool,
    /// Place the young Hero near the camera target (screenshots).
    walk: bool,
    /// Then walk him towards this point (x, y) before the screenshot.
    walk_to: Option<[f32; 2]>,
    /// Pose the Hero with this animation (default: idle, or walk after
    /// `--walk-to`) at `anim_time` seconds.
    anim: Option<String>,
    anim_time: f32,
    screenshot: Option<PathBuf>,
    size: [u32; 2],
}

fn parse_args() -> Result<Args, Error> {
    let mut args = Args {
        subject: Subject::Region("StartOakVale".into()),
        screenshot: None,
        camera: None,
        nav: false,
        walk: false,
        walk_to: None,
        anim: None,
        anim_time: 0.0,
        size: [1280, 800],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--region" => args.subject = Subject::Region(value()?),
            "--mesh" => args.subject = Subject::Mesh(value()?),
            "--screenshot" => args.screenshot = Some(value()?.into()),
            "--nav" => args.nav = true,
            "--walk" => args.walk = true,
            "--anim" => {
                args.anim = Some(value()?);
                args.walk = true;
            }
            "--anim-time" => args.anim_time = value()?.parse()?,
            "--walk-to" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()?;
                args.walk_to = Some(v.try_into().map_err(|_| "--walk-to takes x,y")?);
                args.walk = true;
            }
            "--camera" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()?;
                args.camera = Some(
                    v.try_into()
                        .map_err(|_| "--camera takes x,y,z,distance,yaw,pitch")?,
                );
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size takes WxH")?;
                args.size = [w.parse()?, h.parse()?];
            }
            _ => return Err(format!("unknown argument {arg:?}").into()),
        }
    }
    Ok(args)
}

/// What is on screen: a region's terrain or a single mesh.
enum Scene {
    Terrain(Box<TerrainRenderer>),
    Model(Box<ModelRenderer>),
}

impl Scene {
    fn load(
        data_dir: &Path,
        wld: &Wld,
        subject: &Subject,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(Self, OrbitCamera, String), Error> {
        match subject {
            Subject::Region(name) => {
                let names = Names::parse(&std::fs::read(data_dir.join("CompiledDefs/names.bin"))?)?;
                let defs = Defs::parse(
                    &std::fs::read(data_dir.join("CompiledDefs/game.bin"))?,
                    &names,
                )?;
                let (mesh, placements) = terrain::load_region(data_dir, wld, name, &defs)?;
                let (lo, hi) = mesh.bounds();
                let themes = match &mesh.themes {
                    Some(grid) => Some(themes::load(data_dir, &grid.layers)?),
                    None => None,
                };
                let mut renderer = TerrainRenderer::new(device, queue, &mesh, themes.as_ref());
                let things = model::load_placements(data_dir, name, &placements)?;
                let status = format!(
                    "{} maps, {} terrain vertices, {} placed meshes ({} triangles)",
                    mesh.maps,
                    mesh.vertices.len(),
                    placements.len(),
                    things.indices.len() / 3
                );
                if !things.indices.is_empty() {
                    renderer.things = Some(ModelRenderer::new(device, queue, &things));
                    let p = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].position);
                    let n = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].normal);
                    let triangles = things.indices.as_chunks::<3>().0;
                    renderer.colliders = Some(collide::Colliders::new(
                        triangles.iter().map(|t| t.map(p)).collect(),
                        &triangles
                            .iter()
                            .map(|t| n(t[0]) + n(t[1]) + n(t[2]))
                            .collect::<Vec<_>>(),
                    ));
                    if let (Some(colliders), Some(heights)) =
                        (&renderer.colliders, &renderer.heights)
                    {
                        let navigation = renderer.walk.clone();
                        let before: Vec<usize> =
                            renderer.walk.iter().map(|l| l.walkable_cells()).collect();
                        walk::assign_surfaces(&mut renderer.walk, heights, |p| {
                            colliders.surfaces(p)
                        });
                        let after: Vec<usize> =
                            renderer.walk.iter().map(|l| l.walkable_cells()).collect();
                        eprintln!(
                            "walk layers: {before:?} navigation cells, {after:?} with a surface height"
                        );
                        let (v, i) = terrain::TerrainMesh::walk_overlay(
                            &navigation,
                            &renderer.walk,
                            heights,
                        );
                        renderer.set_navigation(device, &v, &i);
                    }
                }
                Ok((
                    Self::Terrain(Box::new(renderer)),
                    OrbitCamera::framing(lo, hi),
                    status,
                ))
            }
            Subject::Mesh(name) => {
                let model = model::load_mesh(data_dir, name)?;
                let status = format!(
                    "{}: {} vertices, {} triangles, {} textures",
                    model.name,
                    model.vertices.len(),
                    model.indices.len() / 3,
                    model.images.len() - 1
                );
                Ok((
                    Self::Model(Box::new(ModelRenderer::new(device, queue, &model))),
                    OrbitCamera::framing(model.lo, model.hi),
                    status,
                ))
            }
        }
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        view_proj: Mat4,
    ) -> bool {
        match self {
            Self::Terrain(r) => r.render(device, queue, size, view_proj),
            Self::Model(r) => r.render(device, queue, size, view_proj),
        }
    }

    fn target(&self) -> Option<&Target> {
        match self {
            Self::Terrain(r) => r.target.as_ref(),
            Self::Model(r) => r.target.as_ref(),
        }
    }
}

fn main() -> Result<(), Error> {
    let args = parse_args()?;
    let data_dir =
        PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR to the game install")?)
            .join("data");
    let wld = Wld::open(data_dir.join("Levels/FinalAlbion.wld"))?;
    match &args.screenshot {
        Some(path) => screenshot(&data_dir, &wld, &args, path),
        None => run_window(data_dir, wld, args.subject),
    }
}

fn screenshot(data_dir: &Path, wld: &Wld, args: &Args, path: &Path) -> Result<(), Error> {
    let (subject, size) = (&args.subject, args.size);
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    eprintln!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let (mut scene, mut camera, status) = Scene::load(data_dir, wld, subject, &device, &queue)?;
    if let Scene::Terrain(renderer) = &mut scene {
        renderer.show_nav = args.nav;
    }
    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
        camera = OrbitCamera {
            target: glam::Vec3::new(x, y, z),
            distance,
            yaw,
            pitch,
        };
    }
    if let (true, Scene::Terrain(renderer)) = (args.walk, &mut scene)
        && let Some(mut hero) = spawn_hero(
            renderer,
            data_dir,
            &device,
            &queue,
            camera.target.truncate(),
        )?
    {
        if let Some([x, y]) = args.walk_to {
            let goal = glam::Vec2::new(x, y);
            let mut highest = hero.position.z;
            for _ in 0..3000 {
                let to = goal - hero.position.truncate();
                if to.length() < 0.3 {
                    break;
                }
                if let Some(heights) = &renderer.heights {
                    hero.step(to, walk::WALK_SPEED, 1.0 / 30.0, &renderer.walk, heights);
                }
                highest = highest.max(hero.position.z);
            }
            camera.yaw = hero.heading - std::f32::consts::FRAC_PI_2;
            eprintln!(
                "walked to {} on layer {} (highest {highest:.1})",
                hero.position, hero.layer
            );
            if let Some((_, transform)) = &mut renderer.hero {
                *transform = hero_transform(&hero);
            }
        }
        let mut clip = usize::from(args.walk_to.is_some());
        if let (Some(name), Some(animator)) = (&args.anim, &mut renderer.hero_animator) {
            clip = match HERO_CLIPS.iter().position(|c| c == name) {
                Some(i) => i,
                None => animator.add_clip(skin::Clip::load(data_dir, name, animator.skeleton())?),
            };
        }
        if let Some(animator) = &renderer.hero_animator {
            for (i, name) in HERO_CLIPS.iter().enumerate() {
                let speed =
                    animator.speed(i).unwrap_or(0.0) * model::MESH_SCALE * model::YOUNG_HERO_SCALE;
                eprintln!("{name}: {speed:.2} m/s");
            }
        }
        animate_hero(renderer, &queue, clip, args.anim_time);
        if let Some(animator) = &renderer.hero_animator {
            eprintln!(
                "posed with {:?} at {}s",
                animator.clip_name(),
                args.anim_time
            );
        }
        let distance = camera.distance.clamp(3.0, 12.0);
        follow(&mut camera, &hero, renderer, distance);
        eprintln!("hero at {}", hero.position);
    }
    eprintln!("{subject:?}: {status}");
    scene.render(
        &device,
        &queue,
        size,
        camera.view_proj(size[0] as f32 / size[1] as f32),
    );
    let pixels = render::read_rgba(scene.target().unwrap(), &device, &queue);
    image::save_buffer(
        path,
        &pixels,
        size[0],
        size[1],
        image::ExtendedColorType::Rgba8,
    )?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// Places the young Hero on the walkable cell nearest `near`.
fn spawn_hero(
    renderer: &mut TerrainRenderer,
    data_dir: &Path,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    near: glam::Vec2,
) -> Result<Option<walk::Hero>, Error> {
    let (Some(walk), Some(heights)) = (renderer.walk.first(), &renderer.heights) else {
        return Ok(None);
    };
    let Some(at) = walk.nearest_walkable(near) else {
        return Ok(None);
    };
    let hero = walk::Hero::new(at, heights);
    let (model, skin) = model::hero_model(data_dir)?;
    renderer.hero = Some((
        ModelRenderer::new(device, queue, &model),
        hero_transform(&hero),
    ));
    renderer.hero_animator = skin.and_then(|skin| {
        let clips: Vec<skin::Clip> = HERO_CLIPS
            .iter()
            .filter_map(|name| {
                skin::Clip::load(data_dir, name, &skin.skeleton)
                    .map_err(|e| eprintln!("{name}: {e}"))
                    .ok()
            })
            .collect();
        (clips.len() == HERO_CLIPS.len())
            .then(|| skin::Animator::new(model.vertices.clone(), skin, clips))
    });
    Ok(Some(hero))
}

/// The Hero's clips: standing, walking, running (indices used by [`animate_hero`]).
const HERO_CLIPS: [&str; 3] = ["ANIM_HERO_IDLE", "ANIM_HERO_WALK", "ANIM_HERO_RUN"];

/// Poses the Hero with clip `clip` advanced by `dt` and uploads his vertices.
fn animate_hero(renderer: &mut TerrainRenderer, queue: &wgpu::Queue, clip: usize, dt: f32) {
    if let (Some(animator), Some((model, _))) = (&mut renderer.hero_animator, &renderer.hero) {
        animator.play(clip);
        model.set_vertices(queue, animator.advance(dt));
    }
}

fn hero_transform(hero: &walk::Hero) -> glam::Mat4 {
    glam::Mat4::from_translation(hero.position)
        * glam::Mat4::from_rotation_z(hero.heading)
        * model::hero_local()
}

/// Points the camera at the Hero's chest from `distance`, or closer when a placed
/// mesh or the ground is in the way. The camera is treated as having some width:
/// besides the ray to it, rays to points around it are cast, so it does not end up
/// pressed against (or poking into) a wall or a cart.
fn follow(camera: &mut OrbitCamera, hero: &walk::Hero, renderer: &TerrainRenderer, distance: f32) {
    /// Gap kept between the camera and what it would hit.
    const MARGIN: f32 = 0.3;
    /// Half the width of the camera's probe.
    const RADIUS: f32 = 0.5;
    camera.target = hero.position + glam::Vec3::Z;
    camera.pitch = camera.pitch.clamp(0.15, 0.8);
    camera.distance = distance;
    let back = (camera.eye() - camera.target).normalize_or_zero();
    let right = back.cross(glam::Vec3::Z).normalize_or(glam::Vec3::X);
    let up = right.cross(back);
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let offsets = [
        glam::Vec3::ZERO,
        right,
        -right,
        up,
        -up,
        (right + up) * d,
        (right - up) * d,
        (up - right) * d,
        (-right - up) * d,
    ];
    let mut nearest = distance;
    for offset in offsets {
        let to = camera.target + back * (distance + MARGIN) + offset * RADIUS;
        let (dir, len) = (
            (to - camera.target).normalize_or_zero(),
            (to - camera.target).length(),
        );
        let hits = [
            renderer
                .colliders
                .as_ref()
                .and_then(|c| c.ray(camera.target, dir, len)),
            renderer
                .heights
                .as_ref()
                .and_then(|h| collide::ground_ray(h, camera.target, dir, len, MARGIN)),
        ];
        for hit in hits.into_iter().flatten() {
            // Distance along the camera's own line at which this ray hit.
            nearest = nearest.min(hit * dir.dot(back) - MARGIN);
        }
    }
    camera.distance = nearest.clamp(0.4, distance);
}

fn run_window(data_dir: PathBuf, wld: Wld, subject: Subject) -> Result<(), Error> {
    let mesh_names = mesh_names(&data_dir)?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("CullisEngine viewer"),
        ..Default::default()
    };
    eframe::run_native(
        "CullisEngine viewer",
        options,
        Box::new(move |cc| {
            let rs = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("the wgpu renderer is not available")?;
            Ok(Box::new(
                ViewerApp::new(rs, data_dir, wld, mesh_names, subject)
                    .map_err(|e| e.to_string())?,
            ))
        }),
    )?;
    Ok(())
}

fn mesh_names(data_dir: &Path) -> Result<Vec<String>, Error> {
    let mut file = BufReader::new(File::open(data_dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no MBANK_ALLMESHES")?
        .read_index(&mut file)?;
    let mut names: Vec<String> = index
        .entries
        .into_iter()
        .filter(|e| [STATIC_MESH_TYPE, SKINNED_MESH_TYPE].contains(&e.entry_type))
        .map(|e| e.name)
        .collect();
    names.sort();
    Ok(names)
}

#[derive(PartialEq, Eq)]
enum ListMode {
    Regions,
    Meshes,
    Textures,
}

/// A texture shown in place of the 3D view.
struct TextureView {
    id: u32,
    name: String,
    handle: egui::TextureHandle,
    size: [usize; 2],
}

struct ViewerApp {
    data_dir: PathBuf,
    wld: Wld,
    mesh_names: Vec<String>,
    textures: model::TextureBank,
    texture_names: Vec<(u32, String)>,
    texture_view: Option<TextureView>,
    /// The walking Hero, when walk mode is on.
    walker: Option<walk::Hero>,
    /// Preferred camera distance in walk mode (the scroll wheel changes it).
    follow_distance: f32,
    want_walk: bool,
    list: ListMode,
    filter: String,
    subject: Subject,
    status: String,
    scene: Scene,
    camera: OrbitCamera,
    texture: Option<egui::TextureId>,
}

impl ViewerApp {
    fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: PathBuf,
        wld: Wld,
        mesh_names: Vec<String>,
        subject: Subject,
    ) -> Result<Self, Error> {
        let (scene, camera, status) =
            Scene::load(&data_dir, &wld, &subject, &rs.device, &rs.queue)?;
        let list = match subject {
            Subject::Region(_) => ListMode::Regions,
            Subject::Mesh(_) => ListMode::Meshes,
        };
        let textures = model::TextureBank::open(&data_dir)?;
        let texture_names = textures.names();
        Ok(Self {
            data_dir,
            wld,
            mesh_names,
            texture_names,
            texture_view: None,
            walker: None,
            follow_distance: 6.0,
            want_walk: false,
            textures,
            list,
            filter: String::new(),
            subject,
            status,
            scene,
            camera,
            texture: None,
        })
    }

    fn load(&mut self, rs: &egui_wgpu::RenderState, subject: Subject) {
        match Scene::load(&self.data_dir, &self.wld, &subject, &rs.device, &rs.queue) {
            Ok((scene, camera, status)) => {
                self.scene = scene;
                self.camera = camera;
                self.status = status;
                if let Some(id) = self.texture.take() {
                    rs.renderer.write().free_texture(&id);
                }
                self.subject = subject;
                self.walker = None;
                self.want_walk = false;
            }
            Err(e) => self.status = format!("{subject:?}: {e}"),
        }
    }

    /// Returns a chosen region or mesh, and a chosen texture id.
    fn side_panel(&mut self, ui: &mut egui::Ui) -> (Option<Subject>, Option<u32>) {
        let mut chosen = None;
        let mut chosen_texture = None;
        let title = match &self.subject {
            Subject::Region(n) | Subject::Mesh(n) => n.clone(),
        };
        ui.heading(title);
        ui.label(&self.status);
        ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
        if let Scene::Terrain(renderer) = &mut self.scene {
            ui.checkbox(&mut renderer.show_nav, "Show navigation");
            ui.checkbox(&mut self.want_walk, "Walk (WASD, Shift runs)");
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.list, ListMode::Regions, "Regions");
            ui.selectable_value(&mut self.list, ListMode::Meshes, "Meshes");
            ui.selectable_value(&mut self.list, ListMode::Textures, "Textures");
        });
        ui.text_edit_singleline(&mut self.filter);
        let filter = self.filter.to_ascii_uppercase();
        egui::ScrollArea::vertical().show(ui, |ui| match self.list {
            ListMode::Regions => {
                for r in &self.wld.regions {
                    if !r.name.to_ascii_uppercase().contains(&filter) {
                        continue;
                    }
                    let current = self.subject == Subject::Region(r.name.clone());
                    if ui.selectable_label(current, &r.name).clicked() {
                        chosen = Some(Subject::Region(r.name.clone()));
                    }
                }
            }
            ListMode::Textures => {
                for (id, name) in self
                    .texture_names
                    .iter()
                    .filter(|(_, n)| n.contains(&filter))
                {
                    let current = self.texture_view.as_ref().is_some_and(|t| t.id == *id);
                    if ui.selectable_label(current, name).clicked() {
                        chosen_texture = Some(*id);
                    }
                }
            }
            ListMode::Meshes => {
                for name in self.mesh_names.iter().filter(|n| n.contains(&filter)) {
                    let current = self.subject == Subject::Mesh(name.clone());
                    if ui.selectable_label(current, name).clicked() {
                        chosen = Some(Subject::Mesh(name.clone()));
                    }
                }
            }
        });
        (chosen, chosen_texture)
    }
}

impl ViewerApp {
    /// Spawns or removes the Hero when the walk checkbox changes.
    fn update_walk_mode(&mut self, rs: &egui_wgpu::RenderState) {
        let Scene::Terrain(renderer) = &mut self.scene else {
            return;
        };
        match (self.want_walk, self.walker.is_some()) {
            (true, false) => {
                match spawn_hero(
                    renderer,
                    &self.data_dir,
                    &rs.device,
                    &rs.queue,
                    self.camera.target.truncate(),
                ) {
                    Ok(Some(hero)) => {
                        follow(&mut self.camera, &hero, renderer, self.follow_distance);
                        self.walker = Some(hero);
                    }
                    Ok(None) => {
                        self.status = "nowhere walkable in this region".into();
                        self.want_walk = false;
                    }
                    Err(e) => {
                        self.status = format!("hero: {e}");
                        self.want_walk = false;
                    }
                }
            }
            (false, true) => {
                renderer.hero = None;
                self.walker = None;
            }
            _ => {}
        }
    }

    /// Moves the Hero from the keyboard, relative to the camera, and follows it.
    fn walk(&mut self, ui: &egui::Ui, queue: &wgpu::Queue) {
        let (Some(hero), Scene::Terrain(renderer)) = (&mut self.walker, &mut self.scene) else {
            return;
        };
        let (walk, Some(heights)) = (&renderer.walk, &renderer.heights) else {
            return;
        };
        let (forward, right, run, dt) = ui.input(|i| {
            let axis = |pos: egui::Key, neg: egui::Key| {
                f32::from(u8::from(i.key_down(pos))) - f32::from(u8::from(i.key_down(neg)))
            };
            (
                axis(egui::Key::W, egui::Key::S),
                axis(egui::Key::D, egui::Key::A),
                i.modifiers.shift,
                i.stable_dt.min(0.1),
            )
        });
        let view = (self.camera.target - self.camera.eye())
            .truncate()
            .normalize_or_zero();
        let side = glam::Vec2::new(view.y, -view.x);
        let direction = view * forward + side * right;
        // The clips' own movement sets the speed, so the feet do not slide.
        let clip_speed = renderer
            .hero_animator
            .as_ref()
            .and_then(|a| a.speed(if run { 2 } else { 1 }))
            .map(|s| s * model::MESH_SCALE * model::YOUNG_HERO_SCALE);
        let speed = clip_speed.unwrap_or(if run {
            walk::RUN_SPEED
        } else {
            walk::WALK_SPEED
        });
        let before = hero.position;
        hero.step(direction, speed, dt, walk, heights);
        let moved = (hero.position - before).truncate().length() > 1e-4;
        if let Some((_, transform)) = &mut renderer.hero {
            *transform = hero_transform(hero);
        }
        let clip = match (moved, run) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        };
        animate_hero(renderer, queue, clip, dt);
        ui.ctx().request_repaint();
        follow(&mut self.camera, hero, renderer, self.follow_distance);
    }

    /// Decodes texture `id` and shows it in place of the 3D view.
    fn show_texture(&mut self, ctx: &egui::Context, id: u32) {
        let name = self
            .texture_names
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.to_string(), |(_, n)| n.clone());
        match self.textures.load(id) {
            Ok(img) => {
                let size = [img.width as usize, img.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                let handle = ctx.load_texture(&name, image, egui::TextureOptions::LINEAR);
                self.status = format!("texture {id}: {name}, {}x{}", size[0], size[1]);
                self.texture_view = Some(TextureView {
                    id,
                    name,
                    handle,
                    size,
                });
            }
            Err(e) => self.status = format!("texture {id}: {e}"),
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };

        let (chosen, chosen_texture) = egui::Panel::left("list")
            .resizable(true)
            .show(ui, |ui| self.side_panel(ui))
            .inner;
        if let Some(subject) = chosen.filter(|s| *s != self.subject) {
            self.load(&rs, subject);
        }
        if let Some(id) = chosen_texture {
            self.show_texture(ui.ctx(), id);
        }
        self.update_walk_mode(&rs);
        if self.list == ListMode::Textures
            && let Some(view) = &self.texture_view
        {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(format!(
                    "{} (id {}): {}x{}",
                    view.name, view.id, view.size[0], view.size[1]
                ));
                let avail = ui.available_size();
                let scale = (avail.x / view.size[0] as f32)
                    .min(avail.y / view.size[1] as f32)
                    .min(4.0);
                let size = egui::vec2(view.size[0] as f32 * scale, view.size[1] as f32 * scale);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((view.handle.id(), size)));
                });
            });
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, egui::Sense::drag());
                let delta = response.drag_delta();
                if response.dragged_by(egui::PointerButton::Primary) {
                    self.camera.orbit(delta.x, delta.y);
                }
                if response.dragged_by(egui::PointerButton::Secondary) {
                    self.camera.pan(delta.x, delta.y);
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if self.walker.is_some() {
                        self.follow_distance =
                            (self.follow_distance * (-scroll * 0.002).exp()).clamp(2.0, 15.0);
                    } else {
                        self.camera.zoom(scroll);
                    }
                }
                self.walk(ui, &rs.queue);

                let ppp = ui.ctx().pixels_per_point();
                let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
                let view_proj = self.camera.view_proj(rect.width() / rect.height().max(1.0));
                let recreated = self.scene.render(&rs.device, &rs.queue, size, view_proj);
                let view = &self.scene.target().unwrap().color_view;
                let mut egui_renderer = rs.renderer.write();
                let id = match self.texture {
                    Some(id) if recreated => {
                        egui_renderer.update_egui_texture_from_wgpu_texture(
                            &rs.device,
                            view,
                            wgpu::FilterMode::Linear,
                            id,
                        );
                        id
                    }
                    Some(id) => id,
                    None => egui_renderer.register_native_texture(
                        &rs.device,
                        view,
                        wgpu::FilterMode::Linear,
                    ),
                };
                self.texture = Some(id);
                drop(egui_renderer);
                ui.painter().image(
                    id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            });
        ui.ctx().request_repaint();
    }
}
