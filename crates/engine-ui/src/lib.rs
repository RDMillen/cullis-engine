//! The game's UI: what people say (subtitles, questions, notices) and the
//! HUD. `subtitles` is the state and timing, with no drawing; `draw` and `hud`
//! draw with egui over the 3D view, from the game's sprites (`sprites`). See
//! `docs/architecture.md`.

pub mod draw;
pub mod hud;
pub mod sprites;
pub mod subtitles;

pub use hud::{Action, GoldPopup, Hud};
pub use sprites::Sprites;
