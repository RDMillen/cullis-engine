//! Parsers for Fable: The Lost Chapters data files.
//!
//! Written clean-room from file observation. Each format has a page in `docs/formats/`.

pub mod adpcm;
pub mod anim;
pub mod atmos;
pub mod bank;
pub mod bbbb;
pub mod big;
pub mod bncfg;
pub mod defs;
pub mod dxt;
pub mod environment;
mod error;
pub mod gtg;
pub mod lev;
pub mod light;
pub mod lipsync;
pub mod lug;
pub mod lut;
pub mod lzo;
pub mod mesh;
pub mod nav;
pub mod particles;
mod reader;
pub mod stb;
pub mod strings;
mod text;
pub mod texture;
pub mod tng;
pub mod voices;
pub mod wld;

pub use error::{Error, Result};
