//! Quest code: what people say and do as the story goes on.
//!
//! The game data names quests and holds their text, but not their logic, so each
//! quest is written by hand from how the original plays. A quest takes events
//! (the Hero talks to someone, hits someone, answers a question, time passes)
//! and answers with [`Beat`]s for the engine to present: lines to say, questions
//! to ask, notices to show, people leaving or coming back, the quest ending.
//!
//! People are known by role: a creature's `ScriptName` in the level files, such
//! as the Prologue's `NOVI_LiveFather`. Text is named by its text bank entry
//! (`TEXT_QST_048_DAD_THERESA_BIRTHDAY_INTRO`); the engine looks the lines up.
//!
//! No rendering, windowing or platform dependency, so ports can reuse it.

pub mod guild;
pub mod prologue;

/// How a text entry is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A single string, or every line of a group in order (a speech: `_10`, `_20`, …).
    Speech,
    /// One line of a group, picked at random (alternatives).
    Pick,
}

/// Someone the Hero deals with, as quests see them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Person<'a> {
    /// The creature's `ScriptName`.
    pub role: &'a str,
    /// Generic lines come in male and female versions.
    pub female: bool,
}

/// One thing for the engine to present, in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Beat {
    /// Someone says a text entry. `who` is `None` for the person the event was
    /// about (talked to, hit), else the role of someone else.
    Say {
        who: Option<&'static str>,
        entry: &'static str,
        mode: Mode,
    },
    /// A yes/no question (a text entry); the answer goes to [`Quest::answer`].
    Ask(&'static str),
    /// A message on screen, not spoken (deeds done, instructions).
    Notice(&'static str),
    /// The person with this role leaves the scene.
    Leave(&'static str),
    /// The person with this role comes back.
    Return(&'static str),
    /// The quest is over; the world moves on to these quest states (Tng sections).
    Finish(&'static [&'static str]),
    /// The person with this role walks to world position `to` (x, y, and the
    /// height of the floor, which picks a floor where they are stacked),
    /// finding a way round what is in the way.
    Walk { who: &'static str, to: [f32; 3] },
    /// The person with this role is put at world position `at` (x, y and the
    /// height of the floor), facing world point `facing`.
    Place {
        who: &'static str,
        at: [f32; 3],
        facing: [f32; 2],
    },
    /// Night passes: the screen fades, the clock moves on to `hour`, and the
    /// Hero wakes at `at` facing `facing` (world coordinates, as for `Place`).
    Sleep {
        hour: f32,
        at: [f32; 3],
        facing: [f32; 2],
    },
    /// The quest is over and the Hero is taken to another region, in these quest
    /// states, appearing near world position `at` (x, y, and the height of the
    /// floor he stands on) facing towards world point `facing` (x, y).
    Travel {
        region: &'static str,
        sections: &'static [&'static str],
        at: [f32; 3],
        facing: [f32; 2],
    },
}

impl Beat {
    /// The person the event was about says `entry`.
    pub const fn say(entry: &'static str, mode: Mode) -> Self {
        Self::Say {
            who: None,
            entry,
            mode,
        }
    }

    /// Role `who` says `entry`.
    pub const fn said_by(who: &'static str, entry: &'static str, mode: Mode) -> Self {
        Self::Say {
            who: Some(who),
            entry,
            mode,
        }
    }
}

/// What a quest needs to know about the world each update.
pub trait World {
    /// How far the Hero is from the person with `role` (world units), if that
    /// person is in the region.
    fn distance_to(&self, role: &str) -> Option<f32>;
    /// Where the Hero stands (world x, y, and the height of his feet).
    fn hero(&self) -> [f32; 3];
}

/// A quest in progress. Each call appends what happens to `out`.
pub trait Quest {
    /// The Hero talks to `who`.
    fn talk(&mut self, who: Person<'_>, out: &mut Vec<Beat>);
    /// The Hero hits `who`.
    fn hit(&mut self, who: Person<'_>, out: &mut Vec<Beat>);
    /// The Hero answers the last [`Beat::Ask`]. Talking to someone else first
    /// drops the question.
    fn answer(&mut self, yes: bool, out: &mut Vec<Beat>);
    /// `dt` seconds pass.
    fn update(&mut self, dt: f32, world: &dyn World, out: &mut Vec<Beat>);

    /// How the person with `role` is posed, as an animation table modifier
    /// (`MORTALLY_WOUNDED`: their idle's variant with that modifier), if the
    /// quest poses them.
    fn mood(&self, role: &str) -> Option<&'static str> {
        let _ = role;
        None
    }

    /// Who the quest's main thread wants the Hero to go to now, if anyone (its
    /// role): the HUD marks them with the core quest orb.
    fn goal(&self) -> Option<&'static str> {
        None
    }
}
