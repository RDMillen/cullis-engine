//! What the player asks for, read from the keyboard once a frame.

use crate::*;

/// What the player asks for in a frame, read from the keyboard once and
/// consumed by the frame's simulation ticks. Held keys (movement, run) apply
/// to every tick; presses are taken by the first.
#[derive(Debug, Clone, Default)]
pub struct Actions {
    /// Forward (W minus S) and right (D minus A), -1..1.
    pub forward: f32,
    pub right: f32,
    pub run: bool,
    /// E: go on, talk, use.
    pub use_: bool,
    /// F: kick.
    pub kick: bool,
    /// Y or N to a question.
    pub answer: Option<bool>,
}

impl Actions {
    pub fn read(i: &egui::InputState) -> Self {
        let axis = |pos: egui::Key, neg: egui::Key| {
            f32::from(u8::from(i.key_down(pos))) - f32::from(u8::from(i.key_down(neg)))
        };
        Self {
            forward: axis(egui::Key::W, egui::Key::S),
            right: axis(egui::Key::D, egui::Key::A),
            run: i.modifiers.shift,
            use_: i.key_pressed(egui::Key::E),
            kick: i.key_pressed(egui::Key::F),
            answer: if i.key_pressed(egui::Key::Y) {
                Some(true)
            } else if i.key_pressed(egui::Key::N) {
                Some(false)
            } else {
                None
            },
        }
    }
}
