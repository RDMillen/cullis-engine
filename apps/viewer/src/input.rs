//! What the player asks for, read from the keyboard once a frame.

use crate::*;

/// What the player asks for in a frame, read from the keyboard once and
/// consumed by the frame's simulation ticks. Held keys (movement, run) apply
/// to every tick; presses are taken by the first.
#[derive(Debug, Clone, Default)]
pub(crate) struct Actions {
    /// Forward (W minus S) and right (D minus A), -1..1.
    pub(crate) forward: f32,
    pub(crate) right: f32,
    pub(crate) run: bool,
    /// E: go on, talk, use.
    pub(crate) use_: bool,
    /// F: kick.
    pub(crate) kick: bool,
    /// Y or N to a question.
    pub(crate) answer: Option<bool>,
}

impl Actions {
    pub(crate) fn read(i: &egui::InputState) -> Self {
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
