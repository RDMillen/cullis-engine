#![recursion_limit = "256"]
//! CullisEngine desktop viewer: a region's terrain from the Lev heightmaps, or a
//! static mesh with its textures.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--nav] [--screenshot OUT.png] [--size WxH] [--camera x,y,z,distance,yaw,pitch]
//! ```

mod app;
mod args;
mod audio;
mod hero;
mod input;
mod scene;
mod screenshot;
// The simulation's modules, kept at their old paths (`crate::walk`, …).
use engine_render::{model, particles, render, skin, sky, themes};
use engine_world::{camera, collide, conversation, lighting, walk};
mod door;
mod npc;
mod soundscape;
mod story;
mod talk;
mod terrain;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use engine_core::assets::AssetCache;
use engine_core::time::TICK;
use engine_world::hero::{HeroAnimations, PERSON_RADIUS};
use engine_world::world::World;
use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names};
use fable_data::gtg::{Gtg, split_connected_uid};
use fable_data::mesh::{SKINNED_MESH_TYPE, STATIC_MESH_TYPE};
use fable_data::wld::Wld;
use glam::Mat4;

use camera::OrbitCamera;
use model::ModelRenderer;
use render::{Target, TerrainRenderer};

type Error = Box<dyn std::error::Error>;

use app::*;
use args::*;
use hero::*;
use input::*;
use scene::*;
use screenshot::*;
/// How long a night passing stays black, and then takes to fade back in
/// (seconds).
const NIGHT_HOLD: f32 = 1.0;
const NIGHT_FADE: f32 = 1.5;

/// The time of day a region opens at: mid-morning (a choice, not the game's).
const DEFAULT_HOUR: f32 = 10.0;

/// Game hours that pass per real second while the clock runs: a day in 24
/// minutes (a choice, not measured from the game).
const CLOCK_RATE: f32 = 1.0 / 60.0;

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
                    data_dir,
                    wld,
                    mesh_names,
                    subject,
                    args.sections.clone(),
                )
                .map(|mut app| {
                    // --camera, --walk and --walk-to also work in the window:
                    // the Hero then walks himself towards the point.
                    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
                        app.camera = OrbitCamera {
                            target: glam::Vec3::new(x, y, z),
                            distance,
                            yaw,
                            pitch,
                        };
                    }
                    app.want_walk = args.walk;
                    app.hour = args.hour;
                    app.brightness = args.brightness;
                    app.highlights = args.highlights;
                    app.auto_walk = args.walk_to.map(|[x, y, z]| {
                        let height = (!z.is_nan()).then_some(z);
                        walk::Follow::to_floor(glam::Vec2::new(x, y), height)
                    });
                    app.auto_talk = args.talk > 0;
                    if let (true, Some(story)) = (args.with_present, &mut app.story)
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
