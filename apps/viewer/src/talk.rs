//! Showing what people say: lines from the text bank, the queue of subtitles
//! (conversation lines that wait for E, timed remarks, yes/no questions) and
//! on-screen notices.

use std::collections::VecDeque;

use eframe::egui;
use engine_script::{Beat, Mode};
use fable_data::strings::TextBank;

/// How long a notice stays on screen, in seconds.
const NOTICE_SECONDS: f32 = 6.0;

/// One said line: who says it and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub speaker: String,
    pub text: String,
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
        .filter_map(|i| text.strings.get(i))
        .map(|e| Line {
            speaker: speaker_name(&e.speaker),
            text: e.text.clone(),
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
}

impl Subtitles {
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

    /// E: moves past the line on screen. False if there is none.
    pub fn advance(&mut self) -> bool {
        if self.line().is_some() {
            self.items.pop_front();
            self.shown = 0.0;
            true
        } else {
            false
        }
    }

    /// Takes the question on screen down, once answered.
    pub fn answered(&mut self) {
        if self.asking().is_some() {
            self.items.pop_front();
            self.shown = 0.0;
        }
    }

    /// The Hero walked away: drops conversation lines and questions, keeping
    /// remarks and effects.
    pub fn walked_away(&mut self) {
        self.items
            .retain(|i| !matches!(i, Item::Line { wait: true, .. } | Item::Ask { .. }));
        self.shown = 0.0;
    }

    /// Advances time: retires remarks whose time is up and notices, and hands
    /// over the effects reached, in order.
    pub fn step(&mut self, dt: f32, effects: &mut Vec<Beat>) {
        let mut left = dt;
        loop {
            match self.items.front() {
                Some(Item::Effect(beat)) => {
                    effects.push(*beat);
                    self.items.pop_front();
                }
                Some(Item::Notice(text)) => {
                    self.notices.push_back(notice_text(text));
                    self.items.pop_front();
                }
                Some(Item::Line {
                    line, wait: false, ..
                }) => {
                    self.shown += left;
                    left = 0.0;
                    if self.shown < read_time(&line.text) {
                        break;
                    }
                    self.items.pop_front();
                    self.shown = 0.0;
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

    /// Draws the subtitle at the bottom of `rect` and the notice at its top.
    /// Returns the answer if a question's button was clicked.
    pub fn show(&self, ctx: &egui::Context, rect: egui::Rect) -> Option<bool> {
        if let Some((text, kind)) = self.notices.front() {
            // Clear of the HUD in the top right.
            let width = (rect.width() - 480.0).clamp(240.0, 620.0);
            egui::Area::new(egui::Id::new("notice"))
                .fixed_pos(egui::pos2(rect.center().x - width / 2.0, rect.top() + 16.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(width);
                        let colour = match kind {
                            Some(true) => egui::Color32::from_rgb(120, 200, 120),
                            Some(false) => egui::Color32::from_rgb(220, 110, 100),
                            None => ui.visuals().strong_text_color(),
                        };
                        ui.label(egui::RichText::new(text).size(16.0).color(colour));
                    });
                });
        }
        let width = (rect.width() * 0.7).min(720.0);
        let mut answer = None;
        let (speaker, body, hint) = match self.items.front() {
            Some(Item::Line { line, wait, .. }) => (
                line.speaker.as_str(),
                line.text.as_str(),
                wait.then_some("E to continue"),
            ),
            Some(Item::Ask { text, .. }) => ("", text.as_str(), None),
            _ => return None,
        };
        egui::Area::new(egui::Id::new("subtitle"))
            .fixed_pos(egui::pos2(
                rect.center().x - width / 2.0,
                rect.bottom() - 140.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    if !speaker.is_empty() {
                        ui.label(egui::RichText::new(speaker).strong());
                    }
                    ui.label(egui::RichText::new(body).size(17.0));
                    if let Some(hint) = hint {
                        ui.label(egui::RichText::new(hint).weak().small());
                    }
                    if self.asking().is_some() {
                        ui.horizontal(|ui| {
                            if ui.button("Yes (Y)").clicked() {
                                answer = Some(true);
                            }
                            if ui.button("No (N)").clicked() {
                                answer = Some(false);
                            }
                        });
                    }
                });
            });
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_codes_read_as_names() {
        assert_eq!(speaker_name("HERO FATHER"), "Father");
        assert_eq!(speaker_name("HERO"), "Hero");
        assert_eq!(speaker_name("THUG KID"), "Thug Kid");
        assert_eq!(speaker_name("WHORE_SOPHIA_1_5"), "Whore Sophia");
        assert_eq!(speaker_name("LEV1BDT1"), "");
        assert_eq!(speaker_name("MALE VILLAGER1"), "");
        assert_eq!(speaker_name(""), "");
        assert_eq!(role_name("NOVI_Villager"), "Villager");
        assert_eq!(role_name("NOVI_TeddyGirl"), "Teddy Girl");
    }

    #[test]
    fn notices_lose_their_icons() {
        assert_eq!(
            notice_text("[HUD_DEED_GOOD_ICON] You performed a good deed."),
            ("You performed a good deed.".into(), Some(true))
        );
        assert_eq!(
            notice_text("[HUD_DEED_BAD_ICON] You committed a bad deed."),
            ("You committed a bad deed.".into(), Some(false))
        );
        assert_eq!(notice_text("Plain."), ("Plain.".into(), None));
    }

    fn line(text: &str, wait: bool) -> Item {
        Item::Line {
            line: Line {
                speaker: "Father".into(),
                text: text.into(),
            },
            by: Some(0),
            wait,
        }
    }

    #[test]
    fn subtitles_wait_time_out_and_apply_effects_in_order() {
        let mut s = Subtitles::default();
        let mut effects = Vec::new();
        s.push(line("one", true));
        s.push(Item::Effect(Beat::Leave("NOVI_Bully")));
        s.push(line("remark", false));
        s.step(100.0, &mut effects);
        assert_eq!(s.line().unwrap().0.text, "one", "a conversation line waits");
        assert!(effects.is_empty());
        assert!(s.advance());
        s.step(0.1, &mut effects);
        assert_eq!(effects, [Beat::Leave("NOVI_Bully")]);
        assert_eq!(s.line().unwrap().0.text, "remark");
        s.step(10.0, &mut effects);
        assert!(s.line().is_none());
        assert!(!s.advance());
    }

    #[test]
    fn walking_away_drops_the_conversation() {
        let mut s = Subtitles::default();
        s.push(line("one", true));
        s.push(Item::Ask {
            text: "Buy?".into(),
            by: Some(0),
        });
        s.push(Item::Notice("[HUD_DEED_GOOD_ICON] Done.".into()));
        s.push(Item::Effect(Beat::Return("NOVI_BarrelMan")));
        assert_eq!(s.waiting_on(), Some(0));
        s.walked_away();
        let mut effects = Vec::new();
        s.step(0.0, &mut effects);
        assert_eq!(effects, [Beat::Return("NOVI_BarrelMan")]);
        assert!(s.asking().is_none() && s.line().is_none());
        assert_eq!(s.notices.front(), Some(&("Done.".to_owned(), Some(true))));
    }
}
