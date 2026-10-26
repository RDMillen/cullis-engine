//! Showing what people say: lines from the text bank, the queue of subtitles
//! (conversation lines that wait for E, timed remarks, yes/no questions) and
//! on-screen notices.

use std::collections::VecDeque;

use engine_script::{Beat, Mode};
use fable_data::strings::TextBank;

/// How long a notice stays on screen, in seconds.
const NOTICE_SECONDS: f32 = 6.0;

/// One said line: who says it and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub speaker: String,
    pub text: String,
    /// The text entry's id (for its voice).
    pub id: u32,
    /// The text entry's tags (`CONVERSATION_ATTITUDE_ANGER`,
    /// `ANIM:STANDARD_HEAD_SCRATCH`, `CAM:(…)`).
    pub tags: Vec<String>,
}

/// How a line is said, from its tags: the animation slot it asks for
/// (`ANIM:STANDARD_HEAD_SCRATCH`), if any, and its mood as the `CONV_GENERAL`
/// modifier (`CONVERSATION_ATTITUDE_ANGER` → `CONVERSATION_ANGRY`). Lines with
/// no mood (most) count as happy, a guess.
pub fn gesture_of(tags: &[String]) -> (Option<&str>, &'static str) {
    let slot = tags.iter().find_map(|t| t.strip_prefix("ANIM:"));
    let mood = tags
        .iter()
        .find_map(|t| t.strip_prefix("CONVERSATION_ATTITUDE_"))
        .map_or("CONVERSATION_HAPPY", |a| match a {
            "ANGER" => "CONVERSATION_ANGRY",
            "FEAR" => "CONVERSATION_FEAR",
            "SADNESS" => "CONVERSATION_SAD",
            "BELLIGERENT" | "RIDICULE" => "CONVERSATION_BELLIGERENT",
            _ => "CONVERSATION_HAPPY",
        });
    (slot, mood)
}

/// A speaker code (`BAD_MAN`, `THUG KID`) as a name: "Thug Kid". The Hero's
/// family are named without "Hero" (`HERO FATHER` is "Father", as `FATHER`
/// is). Voice-set codes (`LEV1BDT1`, `CM1`, `VILLAGER1`: a word mixing letters and
/// digits) name no one and give "".
pub fn speaker_name(code: &str) -> String {
    let mut words: Vec<&str> = code
        .split([' ', '_'])
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if words.len() > 1 && words[0] == "HERO" {
        words.remove(0);
    }
    if words.iter().any(|w| w.chars().any(|c| c.is_ascii_digit())) {
        return String::new();
    }
    words
        .iter()
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase()
                    .chain(c.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A role (`NOVI_Villager`) as a name for its speaker: "Villager".
pub fn role_name(script_name: &str) -> String {
    let role = script_name.split_once('_').map_or(script_name, |(_, r)| r);
    // Split CamelCase: `TeddyGirl` -> "Teddy Girl".
    let mut out = String::new();
    for (i, c) in role.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// The lines of text entry `entry` (a string, or a group said as `mode`), `roll`
/// choosing among alternatives.
pub fn lines(text: &TextBank, entry: &str, mode: Mode, roll: u64) -> Vec<Line> {
    let Some(id) = text.id(entry) else {
        return Vec::new();
    };
    let ids: Vec<u32> = match (text.groups.get(&id), mode) {
        (Some(group), Mode::Speech) => group.clone(),
        (Some(group), Mode::Pick) if !group.is_empty() => {
            vec![group[(roll % group.len() as u64) as usize]]
        }
        (Some(_), Mode::Pick) => Vec::new(),
        (None, _) => vec![id],
    };
    ids.iter()
        .filter_map(|&i| Some((i, text.strings.get(&i)?)))
        .map(|(id, e)| Line {
            speaker: speaker_name(&e.speaker),
            text: e.text.clone(),
            id,
            tags: e.tags.clone(),
        })
        .collect()
}

/// A notice's text without its HUD icon tokens (`[HUD_DEED_GOOD_ICON] You …`),
/// and whether it marks a good (`Some(true)`) or bad (`Some(false)`) deed.
pub fn notice_text(text: &str) -> (String, Option<bool>) {
    let mut kind = None;
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("[HUD_") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find(']')
            .map_or(rest.len(), |e| start + e + 1);
        let token = &rest[start..end];
        if token.contains("GOOD") {
            kind = Some(true);
        } else if token.contains("BAD") {
            kind = Some(false);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out.trim().to_owned(), kind)
}

/// Seconds to show a line that does not wait for E.
fn read_time(text: &str) -> f32 {
    (1.5 + text.chars().count() as f32 * 0.06).clamp(2.5, 7.0)
}

/// Something queued to show, in order.
#[derive(Debug, Clone)]
pub enum Item {
    /// A line said by creature `by` (an index into the region's creatures); a
    /// conversation line waits for E, a remark goes after a while.
    Line {
        line: Line,
        by: Option<usize>,
        wait: bool,
    },
    /// A yes/no question, asked by creature `by`.
    Ask { text: String, by: Option<usize> },
    /// A notice, put up when the subtitles reach it (after what is being said).
    Notice(String),
    /// A quest beat to apply when the subtitles reach it (someone leaving).
    Effect(Beat),
}

/// The subtitles and notices on screen.
#[derive(Debug, Default)]
pub struct Subtitles {
    items: VecDeque<Item>,
    /// Seconds the first item has been shown.
    shown: f32,
    /// Notices (text, good or bad deed) and how long the first has been up.
    notices: VecDeque<(String, Option<bool>)>,
    notice_shown: f32,
    /// Whether the line on screen has been handed out to be voiced, and the
    /// least time to show it (its voice's length).
    voiced: bool,
    hold: f32,
}

impl Subtitles {
    /// Takes the first item down.
    fn pop(&mut self) {
        self.items.pop_front();
        self.shown = 0.0;
        self.voiced = false;
        self.hold = 0.0;
    }

    /// The text id of a line newly on screen, once, so its voice can start.
    pub fn next_to_voice(&mut self) -> Option<u32> {
        let id = self.line()?.0.id;
        (!std::mem::replace(&mut self.voiced, true)).then_some(id)
    }

    /// The creature saying the line on screen, if it is one of the region's.
    pub fn speaker(&self) -> Option<usize> {
        match self.items.front()? {
            Item::Line { by, .. } => *by,
            _ => None,
        }
    }

    /// How long the line on screen takes to say: its voice's length when it
    /// has one ([`Subtitles::hold_for`]), else a reading time.
    pub fn saying_time(&self) -> f32 {
        match self.items.front() {
            Some(Item::Line { line, .. }) if self.hold > 0.0 => self.hold.max(0.5),
            Some(Item::Line { line, .. }) => read_time(&line.text),
            _ => 0.0,
        }
    }

    /// Keeps the line on screen for at least `seconds` (while it is spoken).
    pub fn hold_for(&mut self, seconds: f32) {
        self.hold = seconds;
    }

    pub fn push(&mut self, item: Item) {
        self.items.push_back(item);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The line on screen, and whether it waits for E.
    pub fn line(&self) -> Option<(&Line, bool)> {
        match self.items.front()? {
            Item::Line { line, wait, .. } => Some((line, *wait)),
            _ => None,
        }
    }

    /// The question on screen.
    pub fn asking(&self) -> Option<&str> {
        match self.items.front()? {
            Item::Ask { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The creature a waiting line or question on screen is from.
    pub fn waiting_on(&self) -> Option<usize> {
        match self.items.front()? {
            Item::Line { by, wait: true, .. } | Item::Ask { by, .. } => *by,
            _ => None,
        }
    }

    /// Whether a scene is playing that the Hero must not walk away from: a
    /// conversation line or question on screen, or lines leading up to the end
    /// of a quest (its finish or a journey still queued).
    pub fn in_scene(&self) -> bool {
        self.waiting_on().is_some()
            || self
                .items
                .iter()
                .any(|i| matches!(i, Item::Effect(Beat::Finish(_) | Beat::Travel { .. })))
    }

    /// E: moves past the line on screen. False if there is none.
    pub fn advance(&mut self) -> bool {
        if self.line().is_some() {
            self.pop();
            true
        } else {
            false
        }
    }

    /// Takes the question on screen down, once answered.
    pub fn answered(&mut self) {
        if self.asking().is_some() {
            self.pop();
        }
    }

    /// The Hero walked away: drops conversation lines and questions, keeping
    /// remarks and effects.
    pub fn walked_away(&mut self) {
        self.items
            .retain(|i| !matches!(i, Item::Line { wait: true, .. } | Item::Ask { .. }));
        self.shown = 0.0;
        self.voiced = false;
        self.hold = 0.0;
    }

    /// Advances time: retires remarks whose time is up and notices, and hands
    /// over the effects reached, in order.
    pub fn step(&mut self, dt: f32, effects: &mut Vec<Beat>) {
        let mut left = dt;
        loop {
            match self.items.front() {
                Some(Item::Effect(beat)) => {
                    effects.push(*beat);
                    self.pop();
                }
                Some(Item::Notice(text)) => {
                    let notice = notice_text(text);
                    self.notices.push_back(notice);
                    self.pop();
                }
                Some(Item::Line {
                    line, wait: false, ..
                }) => {
                    let needed = read_time(&line.text).max(self.hold);
                    self.shown += left;
                    left = 0.0;
                    if self.shown < needed {
                        break;
                    }
                    self.pop();
                }
                _ => break,
            }
        }
        if !self.notices.is_empty() {
            self.notice_shown += dt;
            if self.notice_shown >= NOTICE_SECONDS {
                self.notices.pop_front();
                self.notice_shown = 0.0;
            }
        }
    }

    /// The notice on screen: its text and whether it marks a good or bad deed.
    pub fn notice(&self) -> Option<(&str, Option<bool>)> {
        self.notices.front().map(|(t, k)| (t.as_str(), *k))
    }

    /// The first item queued, the one on screen.
    pub fn front(&self) -> Option<&Item> {
        self.items.front()
    }
}
