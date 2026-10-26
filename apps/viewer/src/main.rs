#![recursion_limit = "256"]
//! CullisEngine desktop viewer: the game with a browser of regions, meshes,
//! textures and text beside it, and debug options (screenshots, scripted
//! walks, quest states).
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--nav] [--screenshot OUT.png] [--size WxH] [--camera x,y,z,distance,yaw,pitch]
//! ```

mod app;
mod args;
mod screenshot;

use engine_render::render::TerrainRenderer;
use engine_render::{model, render, skin};
use engine_ui::subtitles;
use engine_world::{camera, lighting, walk};
use game::{DEFAULT_HOUR, Error, story, terrain};

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use engine_core::assets::AssetCache;
use engine_core::time::TICK;
use engine_world::hero::PERSON_RADIUS;
use engine_world::world::World;
use fable_data::big::BigFile;
use fable_data::mesh::{SKINNED_MESH_TYPE, STATIC_MESH_TYPE};
use fable_data::wld::Wld;

use camera::OrbitCamera;
use game::hero::*;
use game::scene::*;
use game::session::Session;

use app::*;
use args::*;
use screenshot::*;

fn main() -> Result<(), Error> {
    // Engine messages at info and above unless RUST_LOG says otherwise.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    let args = parse_args()?;
    let data_dir =
        PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR to the game install")?)
            .join("data");
    let wld = Wld::open(data_dir.join("Levels/FinalAlbion.wld"))?;
    match &args.screenshot {
        Some(path) => screenshot(&data_dir, &wld, &args, path),
        None => run_window(data_dir, wld, args),
    }
}

fn run_window(data_dir: PathBuf, wld: Wld, args: Args) -> Result<(), Error> {
    let subject = args.subject.clone();
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
                ViewerApp::new(
                    rs,
                    &data_dir,
                    wld,
                    mesh_names,
                    subject,
                    args.sections.clone(),
                )
                .map(|mut app| {
                    // --camera, --walk and --walk-to also work in the window:
                    // the Hero then walks himself towards the point.
                    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
                        app.session.camera = OrbitCamera {
                            target: glam::Vec3::new(x, y, z),
                            distance,
                            yaw,
                            pitch,
                        };
                    }
                    app.session.want_walk = args.walk;
                    app.session.hour = args.hour;
                    app.session.brightness = args.brightness;
                    app.session.highlights = args.highlights;
                    app.session.auto_walk = args.walk_to.map(|[x, y, z]| {
                        let height = (!z.is_nan()).then_some(z);
                        walk::Follow::to_floor(glam::Vec2::new(x, y), height)
                    });
                    app.session.auto_talk = args.talk > 0;
                    if let (true, Some(story)) = (args.with_present, &mut app.session.story)
                        && let story::Act::Birthday(quest) = &mut story.act
                    {
                        quest.skip_to_present();
                    }
                    app
                })
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
