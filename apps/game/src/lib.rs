#![recursion_limit = "256"]
//! CullisEngine, the game: a session (`session::Session`) loads a region into a
//! world and renderer, runs it in fixed ticks from the player's actions, plays
//! the region's quest, voices and sound, and draws it with subtitles and the
//! HUD. The `game` binary plays it; the viewer builds on the same session.

pub mod audio;
pub mod door;
pub mod hero;
pub mod input;
pub mod npc;
pub mod scene;
pub mod session;
pub mod soundscape;
pub mod story;
pub mod terrain;

// Names the modules share through `use crate::*`.
use engine_render::{model, particles, render, skin, sky, themes};
use engine_ui::{draw, subtitles};
use engine_world::{camera, collide, conversation, lighting, walk};

use std::path::Path;

use eframe::{egui, egui_wgpu, wgpu};
use engine_core::assets::AssetCache;
use engine_core::time::TICK;
use engine_world::hero::{HeroAnimations, PERSON_RADIUS};
use engine_world::world::World;
use fable_data::defs::{Defs, Names};
use fable_data::gtg::{Gtg, split_connected_uid};
use fable_data::wld::Wld;
use glam::Mat4;

use camera::OrbitCamera;
use model::ModelRenderer;
use render::{Target, TerrainRenderer};

pub type Error = Box<dyn std::error::Error>;

use hero::*;
use input::*;
use scene::*;
use session::*;

/// How long a night passing stays black, and then takes to fade back in
/// (seconds).
pub const NIGHT_HOLD: f32 = 1.0;
pub const NIGHT_FADE: f32 = 1.5;

/// The time of day a region opens at: mid-morning (a choice, not the game's).
pub const DEFAULT_HOUR: f32 = 10.0;

/// Game hours that pass per real second while the clock runs: a day in 24
/// minutes (a choice, not measured from the game).
pub const CLOCK_RATE: f32 = 1.0 / 60.0;
