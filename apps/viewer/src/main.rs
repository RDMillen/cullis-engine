//! CullisEngine desktop viewer: a region's terrain from the Lev heightmaps, or a
//! static mesh with its textures.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--screenshot OUT.png] [--size WxH]
//! ```

mod camera;
mod model;
mod render;
mod terrain;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names};
use fable_data::mesh::STATIC_MESH_TYPE;
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
    screenshot: Option<PathBuf>,
    size: [u32; 2],
}

fn parse_args() -> Result<Args, Error> {
    let mut args = Args {
        subject: Subject::Region("StartOakVale".into()),
        screenshot: None,
        camera: None,
        size: [1280, 800],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--region" => args.subject = Subject::Region(value()?),
            "--mesh" => args.subject = Subject::Mesh(value()?),
            "--screenshot" => args.screenshot = Some(value()?.into()),
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
                let mut renderer = TerrainRenderer::new(device, &mesh);
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
    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
        camera = OrbitCamera {
            target: glam::Vec3::new(x, y, z),
            distance,
            yaw,
            pitch,
        };
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

fn run_window(data_dir: PathBuf, wld: Wld, subject: Subject) -> Result<(), Error> {
    let mesh_names = static_mesh_names(&data_dir)?;
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

fn static_mesh_names(data_dir: &Path) -> Result<Vec<String>, Error> {
    let mut file = BufReader::new(File::open(data_dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no MBANK_ALLMESHES")?
        .read_index(&mut file)?;
    let mut names: Vec<String> = index
        .entries
        .into_iter()
        .filter(|e| e.entry_type == STATIC_MESH_TYPE)
        .map(|e| e.name)
        .collect();
    names.sort();
    Ok(names)
}

#[derive(PartialEq, Eq)]
enum ListMode {
    Regions,
    Meshes,
}

struct ViewerApp {
    data_dir: PathBuf,
    wld: Wld,
    mesh_names: Vec<String>,
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
        Ok(Self {
            data_dir,
            wld,
            mesh_names,
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
            }
            Err(e) => self.status = format!("{subject:?}: {e}"),
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) -> Option<Subject> {
        let mut chosen = None;
        let title = match &self.subject {
            Subject::Region(n) | Subject::Mesh(n) => n.clone(),
        };
        ui.heading(title);
        ui.label(&self.status);
        ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.list, ListMode::Regions, "Regions");
            ui.selectable_value(&mut self.list, ListMode::Meshes, "Meshes");
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
            ListMode::Meshes => {
                for name in self.mesh_names.iter().filter(|n| n.contains(&filter)) {
                    let current = self.subject == Subject::Mesh(name.clone());
                    if ui.selectable_label(current, name).clicked() {
                        chosen = Some(Subject::Mesh(name.clone()));
                    }
                }
            }
        });
        chosen
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };

        let chosen = egui::Panel::left("list")
            .resizable(true)
            .show(ui, |ui| self.side_panel(ui))
            .inner;
        if let Some(subject) = chosen.filter(|s| *s != self.subject) {
            self.load(&rs, subject);
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
                    self.camera.zoom(ui.input(|i| i.smooth_scroll_delta.y));
                }

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
