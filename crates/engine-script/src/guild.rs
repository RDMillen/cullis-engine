//! The Guild training (quest 028), written from its text and the markers its
//! quest states place in the Guild (`HeroGuildComplexInside`). So far its
//! childhood stage up to the dummy lesson:
//!
//! 1. Arrival: Maze presents the boy to the Guildmaster, who leads him to the
//!    dormitory upstairs (`TEXT_CS_028_PREMELEE_INTRO`, `_BOOHOO`) and tells
//!    him to sleep.
//! 2. Morning: Whisper wakes him and takes him to the Map Room, where the
//!    Guildmaster was waiting, then on to the training ring across the river
//!    (`TEXT_CS_028_PREMELEE_WAKEUP`).
//! 3. The ring: the Guildmaster, by the training dummy, tells him to get in the
//!    ring, and the cutscene puts him inside it, past its closed gate (as the
//!    original does), then asks him to punch it (`TEXT_CS_028_PREMELEE_PUNCH`),
//!    with the instructions on screen. He nags if the boy does not
//!    (`_HIT_DUMMY`); after a few punches he hands him a stick
//!    (`TEXT_CS_028_PREMELEE_STICK`). The stick and the dummy breaking come next.
//!
//! Positions are the quest's markers (`MK_GTP_BED_GM1`, `M_MeleeTeacherStand`, …)
//! in world coordinates: the Guild map's origin is (4576, 3552).

use crate::{Beat, Mode::Speech, Person, Quest, World};

/// The people, by `ScriptName`.
pub const GUILDMASTER: &str = "TheRealGuildmaster";
pub const MAZE: &str = "PreMeleeMaze";
pub const WHISPER: &str = "PreMeleeWhisper";
pub const ROLES: &[&str] = &[GUILDMASTER, MAZE, WHISPER];

/// Where the Guildmaster stands by the boy's bed (`MK_GTP_BED_GM1`), and where
/// the boy wakes (`MK_GTP_BED_HERO2`), upstairs in the dormitory.
const BED_GUILDMASTER: [f32; 3] = [4671.9, 3676.0, 30.1];
const BED_HERO: [f32; 3] = [4678.2, 3667.9, 30.1];
/// Where Whisper stands to wake him (`MK_GTP_END_GM`).
const WAKE_WHISPER: [f32; 3] = [4679.6, 3667.4, 30.1];
/// The Map Room, where the Guildmaster was waiting (`MK_GTP_LATE_W2` for
/// Whisper, `MK_GTP_LATE_H2` for the Hero).
const MAP_ROOM_WHISPER: [f32; 3] = [4645.2, 3668.8, 21.4];
const MAP_ROOM_HERO: [f32; 3] = [4645.6, 3667.1, 21.4];
/// The training ring: the Guildmaster's stand outside it (`M_MeleeTeacherStand`),
/// where Whisper waits beside him (`PreMeleeChatMarker`, a guess: where she
/// stands during the lesson is not observed), the Hero's spot inside it by the
/// dummy (`MK_GTP_HERO2`) and the dummy (`PreMeleeDummy`). The ring's inside is
/// cut off from the outside by its gate; no route crosses it.
const RING_GUILDMASTER: [f32; 3] = [4759.6, 3642.6, 17.3];
const RING_WHISPER: [f32; 3] = [4758.2, 3644.1, 17.7];
const RING_HERO: [f32; 3] = [4767.7, 3640.8, 17.0];
pub const DUMMY: [f32; 2] = [4769.7, 3640.1];
/// The lesson's dummy, by its `ScriptName`.
pub const DUMMY_NAME: &str = "PreMeleeDummy";
/// Punches before the Guildmaster hands over the stick (a guess: not counted
/// in the original).
pub const PUNCHES_FOR_STICK: u32 = 5;
/// Seconds without a punch before the Guildmaster urges the boy on (a guess).
const NAG_AFTER: f32 = 20.0;

/// The hour the boy wakes.
const MORNING: f32 = 7.0;
/// How near (and how far up or down) the Hero must come to a spot.
const REACH: f32 = 3.0;
const FLOOR: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Not begun: the arrival scene plays on the first update.
    Arriving,
    /// Following the Guildmaster up to the dormitory.
    ToDorm,
    /// Asleep; the next update wakes him.
    Sleeping,
    /// Following Whisper to the Map Room.
    ToMapRoom,
    /// On the way to the training ring.
    ToRing,
    /// At the ring: punching the dummy.
    Dummy,
    /// Given the stick.
    Stick,
}

/// The Guild training, from the arrival.
#[derive(Debug, Clone)]
pub struct GuildTraining {
    stage: Stage,
    /// Punches on the lesson's dummy, and seconds since the last (or since the
    /// lesson began).
    punches: u32,
    idle: f32,
    /// Skipped to the walk to the ring: people are put in place on the next
    /// update ([`GuildTraining::skip_to_ring`]).
    skipped: bool,
}

impl Default for GuildTraining {
    fn default() -> Self {
        Self {
            stage: Stage::Arriving,
            punches: 0,
            idle: 0.0,
            skipped: false,
        }
    }
}

impl GuildTraining {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the dummy lesson has begun.
    pub fn at_dummy(&self) -> bool {
        matches!(self.stage, Stage::Dummy | Stage::Stick)
    }

    /// For testing the lesson: skips the arrival, the night and the morning to
    /// the walk to the ring, with the Guildmaster and Whisper put in their
    /// places there on the next update.
    pub fn skip_to_ring(&mut self) {
        self.stage = Stage::ToRing;
        self.skipped = true;
    }

    /// Whether the boy has been given the stick.
    pub fn has_stick(&self) -> bool {
        self.stage == Stage::Stick
    }
}

/// Whether the Hero stands within reach of `spot`, on its floor.
fn near(world: &dyn World, spot: [f32; 3]) -> bool {
    let [x, y, z] = world.hero();
    let (dx, dy) = (x - spot[0], y - spot[1]);
    (dx * dx + dy * dy).sqrt() < REACH && (z - spot[2]).abs() < FLOOR
}

fn flat(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[1]]
}

impl Quest for GuildTraining {
    fn talk(&mut self, _: Person<'_>, _: &mut Vec<Beat>) {}

    fn hit(&mut self, _: Person<'_>, _: &mut Vec<Beat>) {}

    fn struck(&mut self, thing: &str, out: &mut Vec<Beat>) {
        if self.stage != Stage::Dummy || thing != DUMMY_NAME {
            return;
        }
        self.punches += 1;
        self.idle = 0.0;
        if self.punches >= PUNCHES_FOR_STICK {
            self.stage = Stage::Stick;
            // "Not making much of an impact there, are you? Here, try with this."
            for entry in [
                "TEXT_CS_028_PREMELEE_STICK_10",
                "TEXT_CS_028_PREMELEE_STICK_20",
            ] {
                out.push(Beat::said_by(GUILDMASTER, entry, Speech));
            }
        }
    }

    fn answer(&mut self, _: bool, _: &mut Vec<Beat>) {}

    fn update(&mut self, dt: f32, world: &dyn World, out: &mut Vec<Beat>) {
        if std::mem::take(&mut self.skipped) {
            out.push(Beat::Place {
                who: GUILDMASTER,
                at: RING_GUILDMASTER,
                facing: DUMMY,
            });
            out.push(Beat::Place {
                who: WHISPER,
                at: RING_WHISPER,
                facing: DUMMY,
            });
            return;
        }
        match self.stage {
            Stage::Arriving => {
                self.stage = Stage::ToDorm;
                out.push(Beat::said_by(MAZE, "TEXT_CS_028_PREMELEE_INTRO_10", Speech));
                out.push(Beat::said_by(MAZE, "TEXT_CS_028_PREMELEE_INTRO_20", Speech));
                out.push(Beat::said_by(
                    GUILDMASTER,
                    "TEXT_CS_028_PREMELEE_INTRO_45",
                    Speech,
                ));
                out.push(Beat::said_by(
                    GUILDMASTER,
                    "TEXT_CS_028_PREMELEE_INTRO_50",
                    Speech,
                ));
                out.push(Beat::Walk {
                    who: GUILDMASTER,
                    to: BED_GUILDMASTER,
                });
            }
            Stage::ToDorm if near(world, BED_GUILDMASTER) => {
                self.stage = Stage::Sleeping;
                for entry in [
                    "TEXT_CS_028_PREMELEE_BOOHOO_10",
                    "TEXT_CS_028_PREMELEE_BOOHOO_20",
                    "TEXT_CS_028_PREMELEE_INTRO_123",
                ] {
                    out.push(Beat::said_by(GUILDMASTER, entry, Speech));
                }
                // While he sleeps the Guildmaster goes to the training ring and
                // Whisper comes back from the Woods to wake him.
                out.push(Beat::Sleep {
                    hour: MORNING,
                    at: BED_HERO,
                    facing: flat(WAKE_WHISPER),
                });
                out.push(Beat::Place {
                    who: GUILDMASTER,
                    at: RING_GUILDMASTER,
                    facing: DUMMY,
                });
                out.push(Beat::Place {
                    who: WHISPER,
                    at: WAKE_WHISPER,
                    facing: flat(BED_HERO),
                });
            }
            Stage::Sleeping => {
                self.stage = Stage::ToMapRoom;
                for n in [10, 20, 30, 40, 50, 70, 80] {
                    out.push(Beat::said_by(WHISPER, wakeup(n), Speech));
                }
                out.push(Beat::Walk {
                    who: WHISPER,
                    to: MAP_ROOM_WHISPER,
                });
            }
            Stage::ToMapRoom if near(world, MAP_ROOM_HERO) => {
                self.stage = Stage::ToRing;
                out.push(Beat::said_by(WHISPER, wakeup(90), Speech));
                out.push(Beat::said_by(WHISPER, wakeup(100), Speech));
                out.push(Beat::Walk {
                    who: WHISPER,
                    to: RING_WHISPER,
                });
            }
            Stage::ToRing if near(world, RING_GUILDMASTER) => {
                self.stage = Stage::Dummy;
                out.push(Beat::said_by(
                    GUILDMASTER,
                    "TEXT_QST_028_GUILDMASTER_PREMELEE_COMMENT_FIRST",
                    Speech,
                ));
                // "Get in the ring."
                out.push(Beat::said_by(
                    GUILDMASTER,
                    "TEXT_CS_028_PREMELEE_PUNCH_10",
                    Speech,
                ));
                out.push(Beat::Cut {
                    at: RING_HERO,
                    facing: DUMMY,
                });
                for entry in [
                    "TEXT_CS_028_PREMELEE_PUNCH_20",
                    "TEXT_CS_028_PREMELEE_PUNCH_30",
                ] {
                    out.push(Beat::said_by(GUILDMASTER, entry, Speech));
                }
                out.push(Beat::Notice("TEXT_QST_028_PREMELEE_INSTRUCTIONS_PUNCH_PC"));
            }
            Stage::Dummy => {
                self.idle += dt;
                if self.idle > NAG_AFTER {
                    self.idle = 0.0;
                    // "Come on, boy, you can hit it. Walk right up to it and
                    // give it a thump."
                    out.push(Beat::said_by(
                        GUILDMASTER,
                        "TEXT_QST_028_GUILDMASTER_PREMELEE_HIT_DUMMY",
                        Speech,
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Whisper's wake-up line number `n`.
fn wakeup(n: u32) -> &'static str {
    match n {
        10 => "TEXT_CS_028_PREMELEE_WAKEUP_10",
        20 => "TEXT_CS_028_PREMELEE_WAKEUP_20",
        30 => "TEXT_CS_028_PREMELEE_WAKEUP_30",
        40 => "TEXT_CS_028_PREMELEE_WAKEUP_40",
        50 => "TEXT_CS_028_PREMELEE_WAKEUP_50",
        70 => "TEXT_CS_028_PREMELEE_WAKEUP_70",
        80 => "TEXT_CS_028_PREMELEE_WAKEUP_80",
        90 => "TEXT_CS_028_PREMELEE_WAKEUP_90",
        _ => "TEXT_CS_028_PREMELEE_WAKEUP_100",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with the Hero standing at a point.
    struct HeroAt([f32; 3]);

    impl World for HeroAt {
        fn distance_to(&self, _: &str) -> Option<f32> {
            None
        }

        fn hero(&self) -> [f32; 3] {
            self.0
        }
    }

    fn update(q: &mut GuildTraining, at: [f32; 3]) -> Vec<Beat> {
        let mut out = Vec::new();
        q.update(0.1, &HeroAt(at), &mut out);
        out
    }

    /// A training at the dummy lesson, the boy by the dummy.
    fn at_the_dummy() -> GuildTraining {
        let mut q = GuildTraining::new();
        for at in [
            [4648.0, 3668.0, 21.3],
            BED_GUILDMASTER,
            BED_HERO,
            MAP_ROOM_HERO,
            RING_GUILDMASTER,
        ] {
            update(&mut q, at);
        }
        assert!(q.at_dummy());
        q
    }

    fn strike(q: &mut GuildTraining, thing: &str) -> Vec<Beat> {
        let mut out = Vec::new();
        q.struck(thing, &mut out);
        out
    }

    #[test]
    fn skipping_to_the_ring_puts_people_in_place_then_plays_the_ring() {
        let mut q = GuildTraining::new();
        q.skip_to_ring();
        let placed = update(&mut q, RING_GUILDMASTER);
        assert_eq!(placed.len(), 2);
        assert!(placed.iter().all(|b| matches!(b, Beat::Place { .. })));
        update(&mut q, RING_GUILDMASTER);
        assert!(q.at_dummy());
    }

    #[test]
    fn punches_on_the_dummy_earn_the_stick() {
        let mut q = at_the_dummy();
        assert!(strike(&mut q, "MeleePractice").is_empty(), "another dummy");
        for _ in 1..PUNCHES_FOR_STICK {
            assert!(strike(&mut q, DUMMY_NAME).is_empty());
        }
        let given = strike(&mut q, DUMMY_NAME);
        assert!(given.contains(&Beat::said_by(
            GUILDMASTER,
            "TEXT_CS_028_PREMELEE_STICK_20",
            Speech
        )));
        assert!(q.has_stick());
        assert!(strike(&mut q, DUMMY_NAME).is_empty(), "once");
    }

    #[test]
    fn the_guildmaster_urges_a_boy_who_does_not_punch() {
        let mut q = at_the_dummy();
        let mut nag = Vec::new();
        q.update(NAG_AFTER - 1.0, &HeroAt(RING_HERO), &mut nag);
        assert!(nag.is_empty());
        q.update(2.0, &HeroAt(RING_HERO), &mut nag);
        assert_eq!(nag.len(), 1, "{nag:?}");
        // A punch starts the wait again.
        strike(&mut q, DUMMY_NAME);
        let mut quiet = Vec::new();
        q.update(NAG_AFTER - 1.0, &HeroAt(RING_HERO), &mut quiet);
        assert!(quiet.is_empty());
    }

    #[test]
    fn arrival_to_the_dummy() {
        let mut q = GuildTraining::new();
        let hall = [4648.0, 3668.0, 21.3];
        let arrival = update(&mut q, hall);
        assert!(arrival.contains(&Beat::Walk {
            who: GUILDMASTER,
            to: BED_GUILDMASTER
        }));
        assert!(update(&mut q, hall).is_empty(), "waits for the Hero");
        // Right under the bed, a floor down: not there yet.
        let below = [BED_GUILDMASTER[0], BED_GUILDMASTER[1], 21.3];
        assert!(update(&mut q, below).is_empty(), "the dorm is upstairs");
        let bed = update(&mut q, BED_GUILDMASTER);
        assert!(bed.iter().any(|b| matches!(b, Beat::Sleep { .. })));
        let wake = update(&mut q, BED_HERO);
        assert!(
            wake.iter()
                .any(|b| matches!(b, Beat::Walk { who: WHISPER, .. }))
        );
        assert!(update(&mut q, MAP_ROOM_HERO).len() == 3);
        assert!(!q.at_dummy());
        let ring = update(&mut q, RING_GUILDMASTER);
        assert!(ring.len() == 6 && q.at_dummy());
        assert!(ring.contains(&Beat::Notice("TEXT_QST_028_PREMELEE_INSTRUCTIONS_PUNCH_PC")));
        // Put inside the ring after "Get in the ring", before the punching.
        let cut = ring
            .iter()
            .position(|b| matches!(b, Beat::Cut { at, .. } if *at == RING_HERO))
            .expect("a cut into the ring");
        let said = |entry| {
            ring.iter()
                .position(|b| matches!(b, Beat::Say { entry: e, .. } if *e == entry))
                .unwrap()
        };
        assert!(said("TEXT_CS_028_PREMELEE_PUNCH_10") < cut);
        assert!(cut < said("TEXT_CS_028_PREMELEE_PUNCH_20"));
    }
}
