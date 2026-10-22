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
//! 3. The ring: the Guildmaster, by the training dummy, asks him to punch it
//!    (`TEXT_CS_028_PREMELEE_PUNCH`). The lesson itself comes next.
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
/// The training ring: the Guildmaster's stand (`M_MeleeTeacherStand`), Whisper's
/// start (`WhisperMeleeStart`) and the dummy (`PreMeleeDummy`).
const RING_GUILDMASTER: [f32; 3] = [4759.6, 3642.6, 17.3];
const RING_WHISPER: [f32; 3] = [4767.5, 3637.9, 17.0];
pub const DUMMY: [f32; 2] = [4769.7, 3640.1];

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
    /// At the ring: the dummy lesson.
    Dummy,
}

/// The Guild training, from the arrival.
#[derive(Debug, Clone)]
pub struct GuildTraining {
    stage: Stage,
}

impl Default for GuildTraining {
    fn default() -> Self {
        Self {
            stage: Stage::Arriving,
        }
    }
}

impl GuildTraining {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the dummy lesson has begun.
    pub fn at_dummy(&self) -> bool {
        self.stage == Stage::Dummy
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

    fn answer(&mut self, _: bool, _: &mut Vec<Beat>) {}

    fn update(&mut self, _: f32, world: &dyn World, out: &mut Vec<Beat>) {
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
                for entry in [
                    "TEXT_CS_028_PREMELEE_PUNCH_10",
                    "TEXT_CS_028_PREMELEE_PUNCH_20",
                    "TEXT_CS_028_PREMELEE_PUNCH_30",
                ] {
                    out.push(Beat::said_by(GUILDMASTER, entry, Speech));
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
        assert!(ring.len() == 4 && q.at_dummy());
    }
}
