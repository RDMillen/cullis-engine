//! CullisEngine desktop viewer. Shows a region's terrain from the Lev heightmaps.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME] [--screenshot OUT.png] [--size WxH]
//! ```

mod camera;
mod render;
mod terrain;

use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use fable_data::wld::Wld;

use camera::OrbitCamera;
use render::TerrainRenderer;
use terrain::TerrainMesh;

type Error = Box<dyn std::error::Error>;

struct Args {
    region: String,
    screenshot: Option<PathBuf>,
    size: [u32; 2],
}

fn parse_args() -> Result<Args, Error> {
    let mut args = Args {
        region: "StartOakVale".into(),
        screenshot: None,
        size: [1280, 800],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--region" => args.region = value()?,
            "--screenshot" => args.screenshot = Some(value()?.into()),
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

fn main() -> Result<(), Error> {
    let args = parse_args()?;
    let data_dir =
        PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR to the game install")?)
            .join("data");
    let wld = Wld::open(data_dir.join("Levels/FinalAlbion.wld"))?;
    let mesh = terrain::load_region(&data_dir, &wld, &args.region)?;
    eprintln!(
        "{}: {} maps, {} vertices, {} triangles",
        args.region,
        mesh.maps,
        mesh.vertices.len(),
        mesh.indices.len() / 3
    );
    match &args.screenshot {
        Some(path) => screenshot(&mesh, args.size, path),
        None => run_window(data_dir, wld, args.region, mesh),
    }
}

fn screenshot(mesh: &TerrainMesh, size: [u32; 2], path: &Path) -> Result<(), Error> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    eprintln!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let mut renderer = TerrainRenderer::new(&device, mesh);
    let (lo, hi) = mesh.bounds();
    let camera = OrbitCamera::framing(lo, hi);
    renderer.render(
        &device,
        &queue,
        size,
        camera.view_proj(size[0] as f32 / size[1] as f32),
    );
    let pixels = renderer.read_rgba(&device, &queue);
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

fn run_window(data_dir: PathBuf, wld: Wld, region: String, mesh: TerrainMesh) -> Result<(), Error> {
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
            Ok(Box::new(ViewerApp::new(rs, data_dir, wld, region, &mesh)))
        }),
    )?;
    Ok(())
}

struct ViewerApp {
    data_dir: PathBuf,
    wld: Wld,
    region: String,
    status: String,
    renderer: TerrainRenderer,
    camera: OrbitCamera,
    texture: Option<egui::TextureId>,
}

impl ViewerApp {
    fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: PathBuf,
        wld: Wld,
        region: String,
        mesh: &TerrainMesh,
    ) -> Self {
        let (lo, hi) = mesh.bounds();
        Self {
            status: describe(mesh),
            renderer: TerrainRenderer::new(&rs.device, mesh),
            camera: OrbitCamera::framing(lo, hi),
            texture: None,
            data_dir,
            wld,
            region,
        }
    }

    fn load(&mut self, rs: &egui_wgpu::RenderState, region: String) {
        match terrain::load_region(&self.data_dir, &self.wld, &region) {
            Ok(mesh) => {
                let (lo, hi) = mesh.bounds();
                self.renderer = TerrainRenderer::new(&rs.device, &mesh);
                self.camera = OrbitCamera::framing(lo, hi);
                self.status = describe(&mesh);
                if let Some(id) = self.texture.take() {
                    rs.renderer.write().free_texture(&id);
                }
                self.region = region;
            }
            Err(e) => self.status = format!("{region}: {e}"),
        }
    }
}

fn describe(mesh: &TerrainMesh) -> String {
    format!("{} maps, {} vertices", mesh.maps, mesh.vertices.len())
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };

        let mut chosen = None;
        egui::Panel::left("regions").resizable(true).show(ui, |ui| {
            ui.heading(&self.region);
            ui.label(&self.status);
            ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for r in &self.wld.regions {
                    if ui
                        .selectable_label(r.name == self.region, &r.name)
                        .clicked()
                    {
                        chosen = Some(r.name.clone());
                    }
                }
            });
        });
        if let Some(region) = chosen.filter(|r| *r != self.region) {
            self.load(&rs, region);
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
                let recreated = self.renderer.render(&rs.device, &rs.queue, size, view_proj);
                let view = &self.renderer.target.as_ref().unwrap().color_view;
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
