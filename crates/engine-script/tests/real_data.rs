//! Checks quest code against the game's text. Needs the game:
//! `FABLE_DIR=... cargo test -p engine-script -- --ignored`.

use std::collections::BTreeSet;

use engine_script::prologue::{self, Birthday};
use engine_script::{Beat, Person, Quest, World};
use fable_data::strings::TextBank;

struct At(f32);

impl World for At {
    fn distance_to(&self, _: &str) -> Option<f32> {
        Some(self.0)
    }
}

/// Plays random events through the birthday quest and collects every text
/// entry it uses (and its finishing quest states).
fn explore(runs: u64, events: usize) -> BTreeSet<&'static str> {
    let mut seen = BTreeSet::new();
    for run in 0..runs {
        let mut rng = run.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut next = |n: usize| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % n as u64) as usize
        };
        // Some runs never hit anyone.
        let gentle = run % 2 == 0;
        let mut q = Birthday::new();
        let mut out = Vec::new();
        for _ in 0..events {
            let role = prologue::ROLES[next(prologue::ROLES.len())];
            let who = Person {
                role,
                female: next(2) == 0,
            };
            match next(6) {
                0 | 1 => q.talk(who, &mut out),
                2 if !gentle => q.hit(who, &mut out),
                2 => q.talk(who, &mut out),
                3 | 4 => q.answer(next(3) != 0, &mut out),
                _ => {
                    // Ten seconds, near the stock or away from it.
                    let at = At(if next(2) == 0 { 1.0 } else { 50.0 });
                    for _ in 0..10 {
                        q.update(1.0, &at, &mut out);
                    }
                }
            }
        }
        for beat in out {
            match beat {
                Beat::Say { entry, .. } | Beat::Ask(entry) | Beat::Notice(entry) => {
                    seen.insert(entry);
                }
                _ => {}
            }
        }
        seen.extend(q.items());
    }
    seen
}

#[test]
#[ignore = "needs FABLE_DIR"]
fn every_line_the_birthday_quest_uses_exists() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("FABLE_DIR").expect("FABLE_DIR")).join("data");
    let text = TextBank::open(&dir, "English").unwrap();
    let seen = explore(2000, 150);
    // Every `TEXT_QST_048_` entry the quest names is reached.
    let source = include_str!("../src/prologue.rs");
    let source = &source[..source.find("mod tests").unwrap_or(source.len())];
    for name in source
        .split("t!(\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        // `concat!("TEXT_QST_048_", …)` in the macro itself.
        .filter(|name| *name != "TEXT_QST_048_")
    {
        let entry = format!("TEXT_QST_048_{name}");
        assert!(seen.contains(entry.as_str()), "{entry} is never used");
    }
    for entry in &seen {
        let id = text
            .id(entry)
            .unwrap_or_else(|| panic!("{entry} is not in the text"));
        let said = match text.groups.get(&id) {
            Some(group) => group.iter().all(|m| text.strings.contains_key(m)) && !group.is_empty(),
            None => text.strings.get(&id).is_some_and(|e| !e.text.is_empty()),
        };
        assert!(said, "{entry} has no text");
    }
}
