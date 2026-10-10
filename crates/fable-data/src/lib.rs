//! Parsers for Fable: The Lost Chapters data files.
//!
//! Written clean-room from file observation. Each format has a page in `docs/formats/`.

pub mod bank;
pub mod bbbb;
pub mod big;
pub mod dxt;
mod error;
pub mod lev;
pub mod lzo;
pub mod mesh;
mod reader;
pub mod stb;
mod text;
pub mod texture;
pub mod tng;
pub mod wld;

pub use error::{Error, Result};
