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
    /// F: attack (a strike, or the next of a chain); Q: lock on to a foe or
    /// let go.
    pub attack: bool,
    pub lock: bool,
    /// Y or N to a question.
    pub answer: Option<bool>,
}

/// The key bound to one of the game's actions as its text names them
/// (`[GAME_ACTION_ATTACK]` in instructions), if it has one here.
pub fn key_name(action: &str) -> Option<&'static str> {
    Some(match action {
        "GAME_ACTION_ATTACK" => "F",
        "GAME_ACTION_LOCK_TARGET" => "Q",
        _ => return None,
    })
}

/// `text` with every `[GAME_ACTION_…]` it names replaced by its key; ones
/// without a key here are left as they are.
pub fn fill_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(']') {
            Some(close) if key_name(&after[..close]).is_some() => {
                out.push_str(key_name(&after[..close]).unwrap_or_default());
                rest = &after[close + 1..];
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
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
            attack: i.key_pressed(egui::Key::F),
            lock: i.key_pressed(egui::Key::Q),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_name_our_keys() {
        assert_eq!(
            fill_keys("Tap [GAME_ACTION_LOCK_TARGET] to target, [GAME_ACTION_ATTACK] to attack."),
            "Tap Q to target, F to attack."
        );
        assert_eq!(
            fill_keys("Press [GAME_ACTION_UNKNOWN] [and] [GAME_ACTION_ATTACK"),
            "Press [GAME_ACTION_UNKNOWN] [and] [GAME_ACTION_ATTACK",
            "unbound actions and stray brackets stay"
        );
    }
}
