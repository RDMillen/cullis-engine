//! The game's UI: what people say (subtitles, questions, notices) and the
//! HUD. `subtitles` is the state and timing, with no drawing; `draw` draws it
//! with egui over the 3D view. See `docs/architecture.md`.

pub mod draw;
pub mod subtitles;

/// What the HUD shows: the Hero's gold, the names of his items, and his deeds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hud {
    pub gold: u32,
    pub items: Vec<String>,
    pub good_deeds: u32,
    pub bad_deeds: u32,
}
