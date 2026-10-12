//! Talking to people: what a creature says when the Hero talks to it, and the
//! conversation shown as subtitles.
//!
//! Which line a character says is quest logic, which the game data does not
//! hold: the table below is written from how the Prologue plays. Lines
//! themselves come from the text bank.

use fable_data::strings::TextBank;

/// How a text group is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Every line in order (a speech: `_10`, `_20`, …).
    Speech,
    /// One line, picked at random (alternatives).
    Pick,
}

/// What a role says: its first lines, and what it says when talked to again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Role {
    pub first: (&'static str, Mode),
    /// Said (one line, picked) on later talks; `None` repeats `first`.
    pub again: Option<&'static str>,
}

const fn role(first: &'static str, mode: Mode, again: Option<&'static str>) -> Role {
    Role {
        first: (first, mode),
        again,
    }
}

/// What each Prologue role (a creature's `ScriptName`) says when talked to.
const PROLOGUE: &[(&str, Role)] = &[
    (
        "NOVI_LiveFather",
        role(
            "TEXT_QST_048_DAD_THERESA_BIRTHDAY_INTRO",
            Mode::Speech,
            Some("TEXT_QST_048_DAD_DONE_NOTHING_YET"),
        ),
    ),
    (
        "NOVI_Theresa",
        role(
            "TEXT_QST_048_THERESA_INTRODUCTION",
            Mode::Speech,
            Some("TEXT_QST_048_THERESA_GET_PRESENT"),
        ),
    ),
    (
        "NOVI_Bully",
        role("TEXT_QST_048_BULLY_GET_LOST", Mode::Pick, None),
    ),
    (
        "NOVI_Victim",
        role("TEXT_QST_048_VICTIM_PLEA", Mode::Pick, None),
    ),
    (
        "NOVI_TeddyGirl",
        role(
            "TEXT_QST_048_TEDDYGIRL_LOST_TEDDY",
            Mode::Speech,
            Some("TEXT_QST_048_TEDDYGIRL_REPEAT_PLEA"),
        ),
    ),
    (
        "NOVI_BarrelMan",
        role(
            "TEXT_QST_048_BARRELMAN_FAVOUR",
            Mode::Speech,
            Some("TEXT_QST_048_BARRELMAN_NOT_LARKING"),
        ),
    ),
    (
        "NOVI_BarrelThug",
        role(
            "TEXT_QST_048_BARRELTHUG_SMASH",
            Mode::Pick,
            Some("TEXT_QST_048_BARRELTHUG_SCRMSG_TEMPT"),
        ),
    ),
    (
        "NOVI_BookTrader",
        role(
            "TEXT_QST_048_TRADER_INTRO",
            Mode::Speech,
            Some("TEXT_QST_048_TRADER_ROLL_UP"),
        ),
    ),
    (
        "NOVI_AffairWife",
        role("TEXT_QST_048_AFFAIR_WIFE_LAYABOUT", Mode::Pick, None),
    ),
    (
        "NOVI_AffairMan",
        role("TEXT_QST_048_AFFAIRMAN_SHOO", Mode::Pick, None),
    ),
    (
        "NOVI_AffairWoman",
        role("TEXT_QST_048_AFFAIRWOMAN_BUSY", Mode::Pick, None),
    ),
    (
        "NOVI_Guard",
        role("TEXT_QST_048_GUARD_ON_TALK_NEUTRAL", Mode::Pick, None),
    ),
    (
        "NOVI_Villager",
        role(
            "TEXT_AI_OPINION_ACT_GREET_TO_HERO_ASBOY",
            Mode::Pick,
            Some("TEXT_AI_OPINION_ACT_REGREET_TO_HERO_ASBOY"),
        ),
    ),
];

/// What a creature with this `ScriptName` says, if anything.
pub fn role_of(script_name: Option<&str>) -> Option<Role> {
    let name = script_name?;
    PROLOGUE
        .iter()
        .find(|(r, _)| *r == name)
        .map(|&(_, role)| role)
}

/// The entry and mode said on this talk (`again`: talked to before).
pub fn entry(role: Role, again: bool) -> (&'static str, Mode) {
    match role.again {
        Some(e) if again => (e, Mode::Pick),
        _ => role.first,
    }
}

/// One said line: who says it and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub speaker: String,
    pub text: String,
}

/// A speaker code (`HERO FATHER`, `BAD_MAN`, `THUG KID`) as a name: "Hero Father".
/// Voice-set codes (`LEV1BDT1`, `CM1`: a word mixing letters and digits) name
/// no one and give "".
pub fn speaker_name(code: &str) -> String {
    let words: Vec<&str> = code
        .split([' ', '_'])
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
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

/// A conversation in progress: the lines and which is showing.
#[derive(Debug, Clone)]
pub struct Conversation {
    /// Index of the creature talked to.
    pub with: usize,
    pub lines: Vec<Line>,
    pub at: usize,
}

impl Conversation {
    pub fn line(&self) -> Option<&Line> {
        self.lines.get(self.at)
    }

    /// Moves to the next line; false when the conversation is over.
    pub fn advance(&mut self) -> bool {
        self.at += 1;
        self.at < self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_codes_read_as_names() {
        assert_eq!(speaker_name("HERO FATHER"), "Hero Father");
        assert_eq!(speaker_name("THUG KID"), "Thug Kid");
        assert_eq!(speaker_name("WHORE_SOPHIA_1_5"), "Whore Sophia");
        assert_eq!(speaker_name("LEV1BDT1"), "");
        assert_eq!(speaker_name("CM1"), "");
        assert_eq!(speaker_name(""), "");
        assert_eq!(role_name("NOVI_Villager"), "Villager");
        assert_eq!(role_name("NOVI_TeddyGirl"), "Teddy Girl");
    }

    #[test]
    fn roles_map_to_their_lines() {
        let father = role_of(Some("NOVI_LiveFather")).unwrap();
        assert_eq!(
            entry(father, false),
            ("TEXT_QST_048_DAD_THERESA_BIRTHDAY_INTRO", Mode::Speech)
        );
        assert_eq!(
            entry(father, true),
            ("TEXT_QST_048_DAD_DONE_NOTHING_YET", Mode::Pick)
        );
        let bully = role_of(Some("NOVI_Bully")).unwrap();
        assert_eq!(entry(bully, true), entry(bully, false));
        assert_eq!(role_of(Some("SOMEONE_ELSE")), None);
        assert_eq!(role_of(None), None);
    }

    #[test]
    fn conversations_step_through_lines() {
        let line = |t: &str| Line {
            speaker: "Father".into(),
            text: t.into(),
        };
        let mut c = Conversation {
            with: 0,
            lines: vec![line("one"), line("two")],
            at: 0,
        };
        assert_eq!(c.line().unwrap().text, "one");
        assert!(c.advance());
        assert_eq!(c.line().unwrap().text, "two");
        assert!(!c.advance());
        assert!(c.line().is_none());
    }

    /// Every line in the table is in the game's text, so no one is silent.
    #[test]
    #[ignore = "needs FABLE_DIR"]
    fn every_role_has_its_lines() {
        let dir = std::path::PathBuf::from(std::env::var_os("FABLE_DIR").unwrap()).join("data");
        let text = TextBank::open(&dir, "English").unwrap();
        for (name, role) in PROLOGUE {
            for again in [false, true] {
                let (entry, mode) = entry(*role, again);
                let said = lines(&text, entry, mode, 7);
                assert!(!said.is_empty(), "{name}: {entry} has no lines");
                assert!(said.iter().all(|l| !l.text.is_empty()), "{name}: {entry}");
            }
        }
    }
}
