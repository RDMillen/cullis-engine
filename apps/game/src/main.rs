//! CullisEngine: plays Fable: The Lost Chapters from an installed copy.
//!
//! ```text
//! FABLE_DIR=... game [--region NAME] [--section NAME]...
//! ```

use std::path::PathBuf;

use eframe::egui;
use engine_script::prologue;
use fable_data::wld::Wld;
use game::scene::{Scene, Subject};
use game::session::Session;

type Error = Box<dyn std::error::Error>;

/// The game's window: one session, filling it.
struct GameApp {
    session: Session,
}

impl eframe::App for GameApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };
        if self.session.loading_screen(ui, &rs) {
            return;
        }
        self.session.update(&rs);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.session.view(ui, &rs));
        ui.ctx().request_repaint();
    }
}

fn main() -> Result<(), Error> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    let mut region = "StartOakVale".to_owned();
    let mut sections = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--region" => region = args.next().ok_or("--region needs a name")?,
            "--section" => sections.push(args.next().ok_or("--section needs a name")?),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let data_dir =
        PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR to the game install")?)
            .join("data");
    let wld = Wld::open(data_dir.join("Levels/FinalAlbion.wld"))?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("CullisEngine"),
        ..Default::default()
    };
    eframe::run_native(
        "CullisEngine",
        options,
        Box::new(move |cc| {
            let rs = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("the wgpu renderer is not available")?;
            let mut session = Session::new(rs, &data_dir, wld, Subject::Region(region), sections)
                .map_err(|e| e.to_string())?;
            // The game is played as the Hero. The Prologue opens with him beside
            // his father; elsewhere he starts where the region's view does.
            if let Scene::Terrain(_, world) = &session.scene
                && let Some(father) = world
                    .npcs
                    .iter()
                    .find(|n| n.script_name.as_deref() == Some(prologue::FATHER))
            {
                let at = father.position() + glam::Vec2::new(1.2, 0.0);
                session.camera.target = at.extend(session.camera.target.z);
            }
            session.want_walk = true;
            Ok(Box::new(GameApp { session }))
        }),
    )?;
    Ok(())
}
