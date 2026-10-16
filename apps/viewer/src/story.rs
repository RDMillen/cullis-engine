//! The quest being played: the Hero's talking and hitting go to the quest, and
//! what it answers is queued as subtitles, notices and effects.

use eframe::egui;
use engine_script::prologue::{self, Aftermath, Birthday};
use engine_script::{Beat, Person, Quest, World};
use fable_data::strings::TextBank;
use glam::Vec2;

use crate::npc::Npc;
use crate::skin::AnimBank;
use crate::talk::{self, Item, Subtitles};

/// How near the Hero must be to talk to someone, and to kick them.
pub const TALK_REACH: f32 = 2.5;
pub const KICK_REACH: f32 = 1.4;

/// The quest being played.
pub enum Act {
    Birthday(Birthday),
    Aftermath(Aftermath),
}

pub struct Story {
    pub act: Act,
    /// Counts lines said, to vary the alternatives picked.
    rolls: u64,
    beats: Vec<Beat>,
}

fn quest(act: &mut Act) -> &mut dyn Quest {
    match act {
        Act::Birthday(q) => q,
        Act::Aftermath(q) => q,
    }
}

/// Where people are, for the quest.
struct People<'a> {
    npcs: &'a [Npc],
    hero: Vec2,
}

impl World for People<'_> {
    fn distance_to(&self, role: &str) -> Option<f32> {
        self.npcs
            .iter()
            .filter(|n| n.script_name.as_deref() == Some(role))
            .map(|n| n.position().distance(self.hero))
            .min_by(f32::total_cmp)
    }
}

fn person(npc: &Npc) -> Option<Person<'_>> {
    Some(Person {
        role: npc.script_name.as_deref()?,
        female: npc.female,
    })
}

/// The nearest creature with a role within `reach` of `at`, if any; with
/// `facing`, only those in front.
pub fn nearest_person(npcs: &[Npc], at: Vec2, reach: f32, facing: Option<Vec2>) -> Option<usize> {
    npcs.iter()
        .enumerate()
        .filter(|(_, n)| !n.hidden && n.script_name.is_some())
        .map(|(i, n)| (i, n.position() - at))
        .filter(|(_, to)| {
            to.length() < reach && facing.is_none_or(|f| f.dot(to.normalize_or_zero()) > 0.3)
        })
        .min_by(|a, b| a.1.length().total_cmp(&b.1.length()))
        .map(|(i, _)| i)
}

impl Story {
    /// The quest played in this region and quest state, if any.
    pub fn for_region(region: &str, sections: &[String]) -> Option<Self> {
        let has = |section: &str| sections.iter().any(|s| s == section);
        let act = match region {
            "StartOakVale" if has(prologue::SECTIONS[0]) => Act::Birthday(Birthday::new()),
            "StartOakVale" if has(prologue::AFTER[0]) => Act::Aftermath(Aftermath::new()),
            _ => return None,
        };
        Some(Self {
            act,
            rolls: 0,
            beats: Vec::new(),
        })
    }

    /// Whether the quest has something for the person with `role` (so they
    /// glow when in reach).
    pub fn knows(&self, role: &str) -> bool {
        match &self.act {
            Act::Birthday(_) => prologue::ROLES.contains(&role),
            Act::Aftermath(_) => role == prologue::DEAD_FATHER,
        }
    }

    /// After a region loads: hides the people the quest has sent away and
    /// poses those it poses.
    pub fn place(&mut self, npcs: &mut [Npc], data_dir: &std::path::Path) {
        let mut bank = None;
        for npc in npcs {
            let Some(role) = npc.script_name.clone() else {
                continue;
            };
            npc.hidden = matches!(&self.act, Act::Birthday(q) if q.away(&role));
            if let Some(mood) = quest(&mut self.act).mood(&role) {
                if bank.is_none() {
                    bank = AnimBank::open(data_dir)
                        .map_err(|e| eprintln!("animations: {e}"))
                        .ok();
                }
                if let Some(bank) = &mut bank
                    && !npc.set_mood(bank, mood)
                {
                    eprintln!("{role}: no {mood} idle");
                }
            }
        }
    }

    /// The Hero talks to creature `i`.
    pub fn talk(
        &mut self,
        npcs: &[Npc],
        i: usize,
        hero: Vec2,
        text: &TextBank,
        subs: &mut Subtitles,
    ) {
        if let Some(who) = person(&npcs[i]) {
            quest(&mut self.act).talk(who, &mut self.beats);
        }
        self.present(Some(i), true, npcs, hero, text, subs);
    }

    /// The Hero kicks creature `i`.
    pub fn hit(
        &mut self,
        npcs: &[Npc],
        i: usize,
        hero: Vec2,
        text: &TextBank,
        subs: &mut Subtitles,
    ) {
        if let Some(who) = person(&npcs[i]) {
            quest(&mut self.act).hit(who, &mut self.beats);
        }
        self.present(Some(i), false, npcs, hero, text, subs);
    }

    /// The Hero answers the question on screen.
    pub fn answer(
        &mut self,
        yes: bool,
        npcs: &[Npc],
        hero: Vec2,
        text: &TextBank,
        subs: &mut Subtitles,
    ) {
        let by = subs.waiting_on();
        subs.answered();
        quest(&mut self.act).answer(yes, &mut self.beats);
        self.present(by, true, npcs, hero, text, subs);
    }

    pub fn update(
        &mut self,
        dt: f32,
        npcs: &[Npc],
        hero: Vec2,
        text: &TextBank,
        subs: &mut Subtitles,
    ) {
        quest(&mut self.act).update(dt, &People { npcs, hero }, &mut self.beats);
        self.present(None, false, npcs, hero, text, subs);
    }

    /// Queues the quest's beats: lines (waiting for E when `wait`), questions,
    /// notices and effects. `about` is the creature the event was about.
    fn present(
        &mut self,
        about: Option<usize>,
        wait: bool,
        npcs: &[Npc],
        hero: Vec2,
        text: &TextBank,
        subs: &mut Subtitles,
    ) {
        for beat in self.beats.drain(..) {
            match beat {
                Beat::Say { who, entry, mode } => {
                    let by = match who {
                        Some(role) => npcs
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| n.script_name.as_deref() == Some(role))
                            .min_by(|a, b| {
                                a.1.position()
                                    .distance(hero)
                                    .total_cmp(&b.1.position().distance(hero))
                            })
                            .map(|(i, _)| i),
                        None => about,
                    };
                    self.rolls += 1;
                    let roll = self.rolls.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 20;
                    // Voice-set lines name no one: say who by role.
                    let fallback = by
                        .and_then(|i| npcs[i].script_name.as_deref())
                        .map(talk::role_name)
                        .unwrap_or_default();
                    for mut line in talk::lines(text, entry, mode, roll) {
                        if line.speaker.is_empty() {
                            line.speaker.clone_from(&fallback);
                        }
                        subs.push(Item::Line { line, by, wait });
                    }
                }
                Beat::Ask(entry) => subs.push(Item::Ask {
                    text: text.get(entry).unwrap_or(entry).to_owned(),
                    by: about,
                }),
                Beat::Notice(entry) => {
                    subs.push(Item::Notice(text.get(entry).unwrap_or(entry).to_owned()));
                }
                effect => subs.push(Item::Effect(effect)),
            }
        }
    }

    /// The Hero's gold, items and deeds, in the top right of `rect`.
    pub fn hud(&self, ctx: &egui::Context, rect: egui::Rect, text: &TextBank) {
        let Act::Birthday(q) = &self.act else {
            return;
        };
        egui::Area::new(egui::Id::new("hud"))
            .fixed_pos(egui::pos2(rect.right() - 200.0, rect.top() + 16.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(170.0);
                    ui.label(
                        egui::RichText::new(format!("Gold: {}", q.gold))
                            .strong()
                            .size(16.0),
                    );
                    for item in q.items() {
                        ui.label(text.get(item).unwrap_or(item));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Good deeds {} · Bad deeds {}",
                            q.good_deeds, q.bad_deeds
                        ))
                        .small(),
                    );
                    ui.label(
                        egui::RichText::new("E talk · F kick · Y/N answer")
                            .weak()
                            .small(),
                    );
                });
            });
    }
}
