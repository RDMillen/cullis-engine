//! The Prologue in Oakvale (`StartOakVale`).
//!
//! [`Birthday`] (quest state `Q_NewOakValeIntro_PreAttack`): the young Hero earns
//! gold with good deeds, buys his sister Theresa a present from the Trader and
//! gives it to her; then the bandits come.
//!
//! [`Aftermath`] (quest state `Q__OakValeIntro_PostAttack`): the Hero finds his
//! father dying in the burnt village; a bandit comes for him and Maze takes him
//! away to the Heroes' Guild, which ends the Prologue.
//!
//! Written from how the Prologue plays. The people and their lines come from the
//! game data (roles from the Tng `ScriptName`s, text from `TEXT_QST_048_*`);
//! the order of events, prices and timings are this file's. Values not checked
//! against the original are marked "guess".
//!
//! Good deeds: driving off the bully (three kicks), returning the teddy bear to
//! the girl, watching the warehouse worker's stock, telling the wife about her
//! husband's affair. Bad deeds: hitting people, taking the husband's bribe,
//! giving the teddy to the bully, leaving the stock unwatched. The father pays a
//! gold piece for each good deed; the bribe and the bully pay too.

use crate::{Beat, Mode, Person, Quest, World};

use Mode::{Pick, Speech};

/// A `TEXT_QST_048_` text entry.
macro_rules! t {
    ($name:literal) => {
        concat!("TEXT_QST_048_", $name)
    };
}

pub const FATHER: &str = "NOVI_LiveFather";
pub const THERESA: &str = "NOVI_Theresa";
pub const BULLY: &str = "NOVI_Bully";
pub const VICTIM: &str = "NOVI_Victim";
pub const TEDDY_GIRL: &str = "NOVI_TeddyGirl";
pub const BARREL_MAN: &str = "NOVI_BarrelMan";
pub const BARREL_THUG: &str = "NOVI_BarrelThug";
pub const TRADER: &str = "NOVI_BookTrader";
pub const AFFAIR_MAN: &str = "NOVI_AffairMan";
pub const AFFAIR_WOMAN: &str = "NOVI_AffairWoman";
pub const AFFAIR_WIFE: &str = "NOVI_AffairWife";
pub const GUARD: &str = "NOVI_Guard";
pub const VILLAGER: &str = "NOVI_Villager";

/// Every role the quest deals with.
pub const ROLES: &[&str] = &[
    FATHER,
    THERESA,
    BULLY,
    VICTIM,
    TEDDY_GIRL,
    BARREL_MAN,
    BARREL_THUG,
    TRADER,
    AFFAIR_MAN,
    AFFAIR_WOMAN,
    AFFAIR_WIFE,
    GUARD,
    VILLAGER,
];

/// The quest state the region starts in, and the ones after the quest: the
/// bandit raid's aftermath.
pub const SECTIONS: &[&str] = &["Q_NewOakValeIntro_PreAttack"];
pub const AFTER: &[&str] = &["Q__OakValeIntro_PostAttack"];

/// The father after the raid.
pub const DEAD_FATHER: &str = "OVI_DeadFather";
/// Where Maze takes the Hero: the Heroes' Guild in its training state, beside
/// the Guildmaster and Maze (`TheRealGuildmaster`, `PreMeleeMaze`).
pub const GUILD: &str = "HeroGuildComplexInside";
pub const GUILD_SECTIONS: &[&str] = &["Q_GuildTraining"];
pub const GUILD_ARRIVAL: [f32; 3] = [4648.0, 3668.0, 21.3];
/// The Guildmaster's spot, which the Hero arrives facing.
pub const GUILDMASTER: [f32; 2] = [4646.3, 3670.2];
/// How near the father the Hero must come to find him. Guess.
const FIND_REACH: f32 = 3.0;

/// What the sweets cost (the Trader says three gold pieces).
pub const SWEETS_PRICE: u32 = 3;
/// Kicks before the bully gives up. Guess.
const BULLY_GIVES_UP: u8 = 3;
/// The bully's payment for the teddy ("half his pocket money"). Guess.
const BULLY_PAYS: u32 = 1;
/// How long the warehouse worker is away, how near his spot the Hero must stay,
/// and how long he may be elsewhere before it counts as leaving. Guesses.
const WATCH_SECONDS: f32 = 40.0;
const WATCH_REACH: f32 = 5.0;
const WATCH_AWAY_LIMIT: f32 = 8.0;
/// Seconds between the thug's taunts while the Hero watches the stock.
const TAUNT_EVERY: f32 = 5.5;

/// The thug's taunts while the Hero watches the stock, in turn.
const TAUNTS: &[&str] = &[
    t!("BARRELTHUG_SCRMSG_TEMPT_30"),
    t!("BARRELTHUG_SCRMSG_TEMPT_40"),
    t!("BARRELTHUG_SCRMSG_TEMPT_50"),
    t!("BARRELTHUG_SCRMSG_TEMPT_60"),
    t!("BARRELTHUG_SCRMSG_TEMPT_70"),
    t!("BARRELTHUG_SCRMSG_TEMPT_20"),
];

/// Text entries for the Hero's items.
pub const CHOCOLATES: &str = "TXT_OBJT_CHOCOLATES";
pub const TEDDY_BEAR: &str = "TEXT_OBJECT_TEDDY_BEAR_TITLE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Question {
    BuySweets,
    TakeBribe,
    TellWife,
    TeddyToGirl,
    TeddyToBully,
    GiveChocolates,
}

/// Where the girl's teddy bear, Rosie, is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Teddy {
    /// With the bully's victim.
    Lost,
    Held,
    WithGirl,
    WithBully,
}

/// The warehouse worker's favour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Watch {
    NotAsked,
    Watching { time: f32, away: f32, taunts: usize },
    Done,
    Failed,
}

/// The birthday quest's state.
#[derive(Debug, Clone)]
pub struct Birthday {
    pub gold: u32,
    pub good_deeds: u32,
    pub bad_deeds: u32,
    /// Good deeds the father has not paid for yet.
    unrewarded: u32,
    /// Bad deeds the father has told the Hero off for.
    bad_heard: u32,
    /// Roles talked to, once per talk.
    talks: Vec<&'static str>,
    /// Roles hit at least once.
    hit: Vec<&'static str>,
    chocolates: bool,
    sweets_bought: bool,
    teddy: Teddy,
    bully_hits: u8,
    bully_gone: bool,
    victim_hit: bool,
    affair_seen: bool,
    bribe_taken: bool,
    wife_told: bool,
    watch: Watch,
    /// The guards' list of the Hero's misdeeds not yet read out (text entries).
    crimes: Vec<&'static str>,
    crimes_read: bool,
    pending: Option<Question>,
    finished: bool,
}

impl Default for Birthday {
    fn default() -> Self {
        Self::new()
    }
}

impl Birthday {
    pub fn new() -> Self {
        Self {
            gold: 0,
            good_deeds: 0,
            bad_deeds: 0,
            unrewarded: 0,
            bad_heard: 0,
            talks: Vec::new(),
            hit: Vec::new(),
            chocolates: false,
            sweets_bought: false,
            teddy: Teddy::Lost,
            bully_hits: 0,
            bully_gone: false,
            victim_hit: false,
            affair_seen: false,
            bribe_taken: false,
            wife_told: false,
            watch: Watch::NotAsked,
            crimes: Vec::new(),
            crimes_read: false,
            pending: None,
            finished: false,
        }
    }

    /// For testing: as if the sweets had been bought, so the ending can be
    /// reached straight away.
    pub fn skip_to_present(&mut self) {
        self.chocolates = true;
        self.sweets_bought = true;
    }

    /// Whether Theresa has her present.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// The items the Hero carries, as text entries naming them.
    pub fn items(&self) -> impl Iterator<Item = &'static str> {
        [
            (self.chocolates, CHOCOLATES),
            (self.teddy == Teddy::Held, TEDDY_BEAR),
        ]
        .into_iter()
        .filter_map(|(has, name)| has.then_some(name))
    }

    /// Whether the person with `role` is away (has left the scene).
    pub fn away(&self, role: &str) -> bool {
        (role == BULLY && self.bully_gone)
            || (role == BARREL_MAN && matches!(self.watch, Watch::Watching { .. }))
    }

    fn talked(&self, role: &str) -> usize {
        self.talks.iter().filter(|&&r| r == role).count()
    }

    fn good(&mut self, out: &mut Vec<Beat>) {
        out.push(Beat::Notice(if self.good_deeds == 0 {
            t!("SCRMSG_DID_FIRST_GOOD_DEED")
        } else {
            t!("SCRMSG_DID_GOOD_DEED")
        }));
        self.good_deeds += 1;
        self.unrewarded += 1;
    }

    fn bad(&mut self, crime: &'static str, out: &mut Vec<Beat>) {
        out.push(Beat::Notice(if self.bad_deeds == 0 {
            t!("SCRMSG_DID_FIRST_BAD_DEED")
        } else {
            t!("SCRMSG_DID_BAD_DEED")
        }));
        self.bad_deeds += 1;
        if !self.crimes.contains(&crime) {
            self.crimes.push(crime);
        }
    }

    fn ask(&mut self, question: Question, entry: &'static str, out: &mut Vec<Beat>) {
        self.pending = Some(question);
        out.push(Beat::Ask(entry));
    }

    fn father(&mut self, first: bool, out: &mut Vec<Beat>) {
        let say = |out: &mut Vec<Beat>, entry| out.push(Beat::say(entry, Speech));
        if first {
            say(out, t!("FATHER_INTRO"));
            for notice in [
                t!("INSTRUCTION_HIGHLIGHTING_70"),
                t!("INSTRUCTION_HIGHLIGHTING_60"),
                t!("INSTRUCTION_HIGHLIGHTING_40"),
                t!("INSTRUCTION_HIGHLIGHTING_50"),
            ] {
                out.push(Beat::Notice(notice));
            }
        } else if self.chocolates {
            say(out, t!("DAD_GIVE_PRESENT"));
        } else if self.unrewarded > 0 {
            self.gold += self.unrewarded;
            self.unrewarded = 0;
            if self.bad_deeds > self.bad_heard {
                self.bad_heard = self.bad_deeds;
                say(out, t!("DAD_GIVE_REWARD_PART_BAD"));
            } else {
                say(out, t!("DAD_GIVE_REWARD_JUST_GOOD"));
            }
            if self.gold >= SWEETS_PRICE && !self.sweets_bought {
                say(out, t!("DAD_YOU_HAVE_ENOUGH"));
            } else if !self.sweets_bought {
                say(out, t!("DAD_DO_MORE"));
            }
        } else if self.bad_deeds > self.bad_heard {
            self.bad_heard = self.bad_deeds;
            say(out, t!("DAD_ANTISOCIAL"));
        } else if self.gold >= SWEETS_PRICE {
            say(out, t!("DAD_YOU_HAVE_ENOUGH"));
        } else if self.good_deeds == 0 {
            say(out, t!("DAD_DONE_NOTHING_YET"));
        } else {
            say(out, t!("DAD_DO_MORE"));
        }
    }

    fn theresa(&mut self, first: bool, out: &mut Vec<Beat>) {
        if first {
            out.push(Beat::say(t!("THERESA_INTRODUCTION"), Speech));
        }
        if self.chocolates {
            self.ask(Question::GiveChocolates, t!("GIVE_CHOCOLATE_BOX"), out);
        } else if first {
            // The introduction ends asking for a present.
        } else if self.teddy == Teddy::Held {
            out.push(Beat::say(t!("THERESA_BETTER_PRESENT"), Speech));
        } else if self.talked(THERESA) == 1 {
            out.push(Beat::say(t!("THERESA_GET_PRESENT"), Speech));
        } else {
            out.push(Beat::say(t!("THERESA_REALLY_GET_PRESENT"), Speech));
        }
    }

    fn trader(&mut self, first: bool, out: &mut Vec<Beat>) {
        if self.sweets_bought {
            out.push(Beat::say(t!("TRADER_ROLL_UP"), Pick));
            return;
        }
        out.push(Beat::say(
            if first {
                t!("TRADER_INTRO")
            } else {
                t!("TRADER_STILL_GOT")
            },
            Speech,
        ));
        self.ask(Question::BuySweets, t!("TRADER_BUY_SWEETS"), out);
    }

    fn bully(&mut self, first: bool, out: &mut Vec<Beat>) {
        if self.teddy == Teddy::Held {
            out.push(Beat::say(t!("BULLY_FOUND_TEDDY_ONE"), Speech));
            self.ask(Question::TeddyToBully, t!("GIVE_TEDDY_TO_BULLY"), out);
        } else if self.victim_hit {
            out.push(Beat::say(
                if first {
                    t!("BULLY_IN_COMMON")
                } else {
                    t!("BULLY_NASTY_STREAK")
                },
                Speech,
            ));
        } else if first {
            out.push(Beat::say(t!("BULLY_GET_LOST"), Speech));
        } else {
            out.push(Beat::say(t!("BULLY_BADGERING"), Speech));
        }
    }

    fn victim(&mut self, out: &mut Vec<Beat>) {
        let entry = match (self.bully_gone, self.teddy, self.victim_hit) {
            (true, Teddy::Lost, _) => {
                // Rescued, he hands over Rosie for safe keeping.
                out.push(Beat::say(t!("VICTIM_THANKS"), Speech));
                self.teddy = Teddy::Held;
                t!("VICTIM_THANKS_HAVE_ROSIE")
            }
            (true, _, true) => t!("VICTIM_THANKS_AFTER_HIT"),
            (true, _, false) => t!("VICTIM_THANKS"),
            (false, _, true) => t!("VICTIM_PLEA_AFTER_ATTACK"),
            (false, _, false) => t!("VICTIM_PLEA"),
        };
        out.push(Beat::say(entry, Speech));
    }

    fn teddy_girl(&mut self, first: bool, out: &mut Vec<Beat>) {
        match self.teddy {
            Teddy::Held => self.ask(Question::TeddyToGirl, t!("GIVE_TEDDY_TO_GIRL"), out),
            Teddy::WithGirl => out.push(Beat::say(t!("TEDDYGIRL_REPEAT_FOUND"), Speech)),
            Teddy::WithBully => out.push(Beat::say(t!("TEDDYGIRL_BAD_FEELING"), Speech)),
            Teddy::Lost if first => out.push(Beat::say(t!("TEDDYGIRL_LOST_TEDDY"), Speech)),
            Teddy::Lost => out.push(Beat::say(t!("TEDDYGIRL_REPEAT_PLEA"), Speech)),
        }
    }

    fn barrel_man(&mut self, out: &mut Vec<Beat>) {
        match self.watch {
            Watch::NotAsked => {
                out.push(Beat::say(t!("BARRELMAN_FAVOUR"), Speech));
                out.push(Beat::Leave(BARREL_MAN));
                self.watch = Watch::Watching {
                    time: 0.0,
                    away: 0.0,
                    taunts: 0,
                };
            }
            Watch::Watching { .. } => {}
            Watch::Done => out.push(Beat::say(t!("BARRELMAN_NO_TIME"), Speech)),
            Watch::Failed => out.push(Beat::say(t!("BARRELMAN_LETDOWN_NOT_BROKE"), Speech)),
        }
    }

    fn barrel_thug(&mut self, out: &mut Vec<Beat>) {
        let entry = match self.watch {
            Watch::NotAsked => t!("BARRELTHUG_SMASH"),
            Watch::Watching { .. } => t!("BARRELTHUG_EXPLAIN"),
            Watch::Done => t!("BARRELTHUG_WHY_NOT_SMASH"),
            Watch::Failed => t!("BARRELTHUG_OUTRO"),
        };
        out.push(Beat::say(entry, Speech));
    }

    fn affair_man(&mut self, out: &mut Vec<Beat>) {
        let entry = match (self.wife_told, self.bribe_taken, self.affair_seen) {
            (true, true, _) => t!("AFFAIRMAN_HAD_A_DEAL"),
            (true, false, _) => t!("AFFAIRMAN_SOME_NERVE"),
            (false, true, _) => t!("AFFAIRMAN_DONT_TELL"),
            (false, false, true) => t!("AFFAIRMAN_SHOO"),
            (false, false, false) => {
                self.affair_seen = true;
                out.push(Beat::say(t!("AFFAIRMAN_INTRO"), Speech));
                self.ask(
                    Question::TakeBribe,
                    t!("AFFAIRMAN_QUESTION_WILL_YOU_TELL"),
                    out,
                );
                return;
            }
        };
        out.push(Beat::say(entry, Speech));
    }

    fn affair_wife(&mut self, first: bool, out: &mut Vec<Beat>) {
        if self.wife_told {
            out.push(Beat::say(t!("AFFAIR_WIFE_THANKYOU_SINGLE"), Speech));
        } else if self.affair_seen {
            out.push(Beat::say(t!("AFFAIR_WIFE_WHERES_HUSBAND"), Speech));
            self.ask(Question::TellWife, t!("AFFAIR_WIFE_QUESTION"), out);
        } else if first {
            out.push(Beat::say(t!("AFFAIR_WIFE_LAYABOUT"), Speech));
        } else {
            out.push(Beat::say(t!("AFFAIR_WIFE_PLEA"), Speech));
        }
    }

    fn guard(&mut self, out: &mut Vec<Beat>) {
        if self.crimes.is_empty() {
            let entry = if self.bad_deeds > self.good_deeds {
                t!("GUARD_ON_TALK_BAD")
            } else if self.good_deeds > 0 {
                t!("GUARD_ON_TALK_GOOD")
            } else {
                t!("GUARD_ON_TALK_NEUTRAL")
            };
            out.push(Beat::say(entry, Pick));
            return;
        }
        // Caught: the guard reads out the list of misdeeds.
        out.push(Beat::say(
            if self.crimes_read {
                t!("GUARD_CAUGHT_YOU_AGAIN")
            } else {
                t!("GUARD_CAUGHT_YOU")
            },
            Speech,
        ));
        out.extend(self.crimes.drain(..).map(|c| Beat::say(c, Speech)));
        out.push(Beat::say(t!("GUARD_AFTER_READ_LIST"), Speech));
        self.crimes_read = true;
    }

    /// The first time the Hero hits someone (other than the bully) is a bad deed.
    fn violence(&mut self, role: &'static str, out: &mut Vec<Beat>) {
        if !self.hit.contains(&role) {
            self.hit.push(role);
            self.bad(t!("GUARD_CRIME_VIOLENCE"), out);
        }
    }
}

/// The role constant equal to `role`, if the quest knows it.
fn known(role: &str) -> Option<&'static str> {
    ROLES.iter().copied().find(|&r| r == role)
}

/// A villager's line by what the Hero has done, in the villager's voice.
fn villager_line(female: bool, good: bool, bad: bool) -> &'static str {
    match (female, good, bad) {
        (false, false, false) => t!("VILLAGER_SPOKEN_TO_MALE"),
        (false, true, false) => t!("VILLAGER_DONE_GOOD_DEEDS_MALE"),
        (false, false, true) => t!("VILLAGER_DONE_BAD_DEEDS_MALE"),
        (false, true, true) => t!("VILLAGER_DONE_BOTH_DEEDS_MALE"),
        (true, false, false) => t!("VILLAGER_SPOKEN_TO_FEMALE"),
        (true, true, false) => t!("VILLAGER_DONE_GOOD_DEEDS_FEMALE"),
        (true, false, true) => t!("VILLAGER_DONE_BAD_DEEDS_FEMALE"),
        (true, true, true) => t!("VILLAGER_DONE_BOTH_DEEDS_FEMALE"),
    }
}

impl Quest for Birthday {
    fn talk(&mut self, who: Person<'_>, out: &mut Vec<Beat>) {
        self.pending = None;
        let Some(role) = known(who.role) else {
            return;
        };
        if self.away(role) {
            return;
        }
        let first = self.talked(role) == 0;
        match role {
            FATHER => self.father(first, out),
            THERESA => self.theresa(first, out),
            TRADER => self.trader(first, out),
            BULLY => self.bully(first, out),
            VICTIM => self.victim(out),
            TEDDY_GIRL => self.teddy_girl(first, out),
            BARREL_MAN => self.barrel_man(out),
            BARREL_THUG => self.barrel_thug(out),
            AFFAIR_MAN => self.affair_man(out),
            AFFAIR_WOMAN => out.push(Beat::say(t!("AFFAIRWOMAN_BUSY"), Speech)),
            AFFAIR_WIFE => self.affair_wife(first, out),
            GUARD => self.guard(out),
            _ => out.push(Beat::say(
                villager_line(who.female, self.good_deeds > 0, self.bad_deeds > 0),
                Pick,
            )),
        }
        self.talks.push(role);
    }

    fn hit(&mut self, who: Person<'_>, out: &mut Vec<Beat>) {
        self.pending = None;
        let Some(role) = known(who.role) else {
            return;
        };
        if self.away(role) {
            return;
        }
        let entry = match role {
            BULLY => {
                self.bully_hits += 1;
                if self.bully_hits >= BULLY_GIVES_UP {
                    self.bully_gone = true;
                    out.push(Beat::say(t!("BULLY_GOING_AWAY"), Speech));
                    out.push(Beat::Leave(BULLY));
                    self.good(out);
                } else {
                    out.push(Beat::say(t!("BULLY_SCRMSG_GET_OFF"), Pick));
                    out.push(Beat::said_by(VICTIM, t!("VICTIM_REVENGE"), Pick));
                }
                return;
            }
            VICTIM => {
                if self.teddy == Teddy::Lost && !self.victim_hit {
                    // He gives up Rosie to make it stop.
                    self.teddy = Teddy::Held;
                    out.push(Beat::say(t!("VICTIM_ON_ATTACKED_50"), Speech));
                } else {
                    out.push(Beat::say(t!("VICTIM_ON_ATTACKED"), Pick));
                }
                if !self.bully_gone {
                    out.push(Beat::said_by(BULLY, t!("BULLY_HERO_ATTACKS_VICTIM"), Pick));
                }
                self.victim_hit = true;
                self.violence(role, out);
                return;
            }
            FATHER => t!("DAD_TEMPER"),
            THERESA => t!("THERESA_DONT_HIT"),
            TEDDY_GIRL => t!("TEDDYGIRL_DONT_HIT"),
            TRADER => t!("TRADER_ON_HIT"),
            AFFAIR_MAN => t!("AFFAIRMAN_ON_HIT"),
            AFFAIR_WOMAN => t!("AFFAIRWOMAN_ON_HIT"),
            AFFAIR_WIFE => t!("AFFAIR_WIFE_ON_HIT"),
            GUARD => t!("GUARD_ON_HIT"),
            BARREL_THUG => t!("BARRELTHUG_WHY_HIT"),
            _ if who.female => t!("VILLAGER_ATTACKED_FEMALE"),
            _ => t!("VILLAGER_ATTACKED_MALE"),
        };
        // Groups of alternatives pick one; speeches (the father's, the guard's) run.
        let mode = match role {
            FATHER | GUARD => Speech,
            _ => Pick,
        };
        out.push(Beat::say(entry, mode));
        self.violence(role, out);
    }

    fn answer(&mut self, yes: bool, out: &mut Vec<Beat>) {
        let Some(question) = self.pending.take() else {
            return;
        };
        let say = |out: &mut Vec<Beat>, entry| out.push(Beat::say(entry, Speech));
        match (question, yes) {
            (Question::BuySweets, true) if self.gold >= SWEETS_PRICE => {
                self.gold -= SWEETS_PRICE;
                self.chocolates = true;
                self.sweets_bought = true;
                say(out, t!("TRADER_GIVES_SWEETS"));
            }
            (Question::BuySweets, true) => say(out, t!("TRADER_NOT_ENOUGH_CASH")),
            (Question::BuySweets, false) => say(out, t!("TRADER_BUY_LATER")),
            (Question::TakeBribe, true) => {
                self.bribe_taken = true;
                self.gold += 1;
                say(out, t!("AFFAIRMAN_GOOD_LAD"));
                self.bad(t!("GUARD_CRIME_CONCEALED_AFFAIR"), out);
            }
            (Question::TakeBribe, false) => say(out, t!("AFFAIRMAN_DONT_TELL")),
            (Question::TellWife, true) => {
                self.wife_told = true;
                say(out, t!("AFFAIR_WIFE_THANKYOU"));
                say(out, t!("AFFAIR_WIFE_RUNNING_TO_HUBBY"));
                self.good(out);
            }
            (Question::TellWife, false) => say(out, t!("AFFAIR_WIFE_PLEA")),
            (Question::TeddyToGirl, true) => {
                self.teddy = Teddy::WithGirl;
                say(out, t!("TEDDYGIRL_FOUND_TEDDY"));
                self.good(out);
            }
            (Question::TeddyToGirl, false) => say(out, t!("TEDDYGIRL_REPEAT_PLEA")),
            (Question::TeddyToBully, true) => {
                self.teddy = Teddy::WithBully;
                self.gold += BULLY_PAYS;
                say(out, t!("BULLY_FOUND_TEDDY_TWO"));
                self.bad(t!("GUARD_CRIME_TEDDY_TO_BULLY"), out);
            }
            (Question::TeddyToBully, false) => say(out, t!("BULLY_DONT_WANT")),
            (Question::GiveChocolates, true) => {
                self.chocolates = false;
                self.finished = true;
                say(out, t!("THERESA_HELLO"));
                // The raid begins (the cutscene's lines, then Theresa's).
                say(out, "TEXT_CS_048_OIF_10");
                say(out, "TEXT_CS_048_OIF_20");
                say(out, t!("THERESA_BANDITS"));
                out.push(Beat::Finish(AFTER));
            }
            (Question::GiveChocolates, false) => say(out, t!("THERESA_REALLY_GET_PRESENT")),
        }
    }

    fn update(&mut self, dt: f32, world: &dyn World, out: &mut Vec<Beat>) {
        let Watch::Watching { time, away, taunts } = &mut self.watch else {
            return;
        };
        *time += dt;
        let near = world
            .distance_to(BARREL_MAN)
            .is_some_and(|d| d < WATCH_REACH);
        if !near {
            *away += dt;
        }
        // The thug tempts the Hero now and then while he stands guard.
        if near && *time >= 4.0 + *taunts as f32 * TAUNT_EVERY {
            let entry = match *taunts {
                0 => t!("BARRELTHUG_SCRMSG_INTRO"),
                n => TAUNTS[(n - 1) % TAUNTS.len()],
            };
            *taunts += 1;
            out.push(Beat::said_by(BARREL_THUG, entry, Speech));
        }
        if *time < WATCH_SECONDS {
            return;
        }
        let failed = *away > WATCH_AWAY_LIMIT;
        out.push(Beat::Return(BARREL_MAN));
        if failed {
            self.watch = Watch::Failed;
            out.push(Beat::said_by(
                BARREL_MAN,
                t!("SCRMSG_BARRELMAN_WHERE_GONE"),
                Speech,
            ));
            out.push(Beat::Notice(t!("INSTRUCTION_LEFT_WAREHOUSE_UNATTENDED")));
            self.bad(t!("GUARD_CRIME_DERELICTION_OF_DUTY"), out);
        } else {
            self.watch = Watch::Done;
            out.push(Beat::said_by(BARREL_MAN, t!("BARRELMAN_THANKS"), Speech));
            out.push(Beat::said_by(
                BARREL_THUG,
                t!("BARRELTHUG_SCRMSG_TEMPT_10"),
                Speech,
            ));
            self.good(out);
        }
    }
}

/// After the raid: finding the dying father ends the Prologue.
#[derive(Debug, Clone, Default)]
pub struct Aftermath {
    found: bool,
}

impl Aftermath {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the Hero has found his father (and is being taken away).
    pub fn finished(&self) -> bool {
        self.found
    }

    fn find_father(&mut self, out: &mut Vec<Beat>) {
        if std::mem::replace(&mut self.found, true) {
            return;
        }
        out.push(Beat::said_by(DEAD_FATHER, t!("FATHER_DYING_MOAN"), Speech));
        // A bandit comes for the boy; Maze saves him (the cutscene's lines).
        for entry in [
            "TEXT_CS_048_OVID_10",
            "TEXT_CS_048_OVID_20",
            "TEXT_CS_048_OVID_30",
            "TEXT_CS_048_OVID_40",
        ] {
            out.push(Beat::say(entry, Speech));
        }
        out.push(Beat::Travel {
            region: GUILD,
            sections: GUILD_SECTIONS,
            at: GUILD_ARRIVAL,
            facing: GUILDMASTER,
        });
    }
}

impl Quest for Aftermath {
    fn talk(&mut self, who: Person<'_>, out: &mut Vec<Beat>) {
        if who.role == DEAD_FATHER {
            self.find_father(out);
        }
    }

    fn hit(&mut self, _: Person<'_>, _: &mut Vec<Beat>) {}

    fn answer(&mut self, _: bool, _: &mut Vec<Beat>) {}

    fn update(&mut self, _: f32, world: &dyn World, out: &mut Vec<Beat>) {
        if world
            .distance_to(DEAD_FATHER)
            .is_some_and(|d| d < FIND_REACH)
        {
            self.find_father(out);
        }
    }

    fn mood(&self, role: &str) -> Option<&'static str> {
        (role == DEAD_FATHER).then_some("MORTALLY_WOUNDED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world where the Hero is always this far from everyone.
    struct At(f32);

    impl World for At {
        fn distance_to(&self, _: &str) -> Option<f32> {
            Some(self.0)
        }

        fn hero(&self) -> [f32; 3] {
            [0.0; 3]
        }
    }

    fn person(role: &str) -> Person<'_> {
        Person {
            role,
            female: false,
        }
    }

    fn talk(q: &mut Birthday, role: &str) -> Vec<Beat> {
        let mut out = Vec::new();
        q.talk(person(role), &mut out);
        out
    }

    fn hit(q: &mut Birthday, role: &str) -> Vec<Beat> {
        let mut out = Vec::new();
        q.hit(person(role), &mut out);
        out
    }

    fn answer(q: &mut Birthday, yes: bool) -> Vec<Beat> {
        let mut out = Vec::new();
        q.answer(yes, &mut out);
        out
    }

    fn says(beats: &[Beat], entry: &str) -> bool {
        beats
            .iter()
            .any(|b| matches!(b, Beat::Say { entry: e, .. } if *e == entry))
    }

    fn asks(beats: &[Beat]) -> bool {
        beats.iter().any(|b| matches!(b, Beat::Ask(_)))
    }

    #[test]
    fn good_deeds_buy_the_present() {
        let mut q = Birthday::new();
        assert!(says(&talk(&mut q, FATHER), t!("FATHER_INTRO")));
        assert!(says(&talk(&mut q, FATHER), t!("DAD_DONE_NOTHING_YET")));

        // Not enough gold yet.
        assert!(asks(&talk(&mut q, TRADER)));
        assert!(says(&answer(&mut q, true), t!("TRADER_NOT_ENOUGH_CASH")));

        // Drive off the bully; the victim hands over Rosie; return her.
        assert!(hit(&mut q, BULLY).contains(&Beat::said_by(VICTIM, t!("VICTIM_REVENGE"), Pick)));
        hit(&mut q, BULLY);
        let beats = hit(&mut q, BULLY);
        assert!(beats.contains(&Beat::Leave(BULLY)));
        assert!(beats.contains(&Beat::Notice(t!("SCRMSG_DID_FIRST_GOOD_DEED"))));
        assert!(q.away(BULLY) && talk(&mut q, BULLY).is_empty());
        assert!(says(&talk(&mut q, VICTIM), t!("VICTIM_THANKS_HAVE_ROSIE")));
        assert_eq!(q.items().collect::<Vec<_>>(), [TEDDY_BEAR]);
        assert!(asks(&talk(&mut q, TEDDY_GIRL)));
        assert!(says(&answer(&mut q, true), t!("TEDDYGIRL_FOUND_TEDDY")));

        // Watch the stock until the worker is back.
        assert!(talk(&mut q, BARREL_MAN).contains(&Beat::Leave(BARREL_MAN)));
        let mut beats = Vec::new();
        for _ in 0..(WATCH_SECONDS as usize + 1) {
            q.update(1.0, &At(1.0), &mut beats);
        }
        assert!(beats.contains(&Beat::Return(BARREL_MAN)));
        assert!(beats.contains(&Beat::said_by(BARREL_MAN, t!("BARRELMAN_THANKS"), Speech)));

        // Tell the wife.
        assert!(asks(&talk(&mut q, AFFAIR_MAN)));
        assert!(says(&answer(&mut q, false), t!("AFFAIRMAN_DONT_TELL")));
        assert!(asks(&talk(&mut q, AFFAIR_WIFE)));
        answer(&mut q, true);
        assert_eq!((q.good_deeds, q.bad_deeds, q.gold), (4, 0, 0));

        // The father pays; buy the sweets; give them to Theresa.
        let beats = talk(&mut q, FATHER);
        assert!(says(&beats, t!("DAD_GIVE_REWARD_JUST_GOOD")));
        assert!(says(&beats, t!("DAD_YOU_HAVE_ENOUGH")));
        assert_eq!(q.gold, 4);
        talk(&mut q, TRADER);
        assert!(says(&answer(&mut q, true), t!("TRADER_GIVES_SWEETS")));
        assert_eq!(q.gold, 1);
        assert_eq!(q.items().collect::<Vec<_>>(), [CHOCOLATES]);
        assert!(says(&talk(&mut q, THERESA), t!("THERESA_INTRODUCTION")));
        let beats = answer(&mut q, true);
        assert!(says(&beats, t!("THERESA_HELLO")));
        assert_eq!(beats.last(), Some(&Beat::Finish(AFTER)));
        assert!(q.finished());
    }

    #[test]
    fn bad_deeds_pay_and_the_guards_hear_of_them() {
        let mut q = Birthday::new();
        talk(&mut q, FATHER);
        talk(&mut q, AFFAIR_MAN);
        let beats = answer(&mut q, true);
        assert!(beats.contains(&Beat::Notice(t!("SCRMSG_DID_FIRST_BAD_DEED"))));
        assert_eq!(q.gold, 1);
        hit(&mut q, VILLAGER);
        hit(&mut q, VILLAGER);
        assert_eq!(q.bad_deeds, 2, "one bad deed per person hit");

        let beats = talk(&mut q, GUARD);
        assert!(says(&beats, t!("GUARD_CAUGHT_YOU")));
        assert!(says(&beats, t!("GUARD_CRIME_CONCEALED_AFFAIR")));
        assert!(says(&beats, t!("GUARD_CRIME_VIOLENCE")));
        assert!(says(&talk(&mut q, GUARD), t!("GUARD_ON_TALK_BAD")));
        assert!(says(&talk(&mut q, FATHER), t!("DAD_ANTISOCIAL")));

        // Hitting the victim gets the teddy, which the bully pays for.
        assert!(says(&hit(&mut q, VICTIM), t!("VICTIM_ON_ATTACKED_50")));
        assert!(asks(&talk(&mut q, BULLY)));
        answer(&mut q, true);
        assert_eq!(q.gold, 1 + BULLY_PAYS);
        assert!(says(&talk(&mut q, TEDDY_GIRL), t!("TEDDYGIRL_BAD_FEELING")));
    }

    #[test]
    fn leaving_the_stock_is_a_bad_deed() {
        let mut q = Birthday::new();
        talk(&mut q, BARREL_MAN);
        let mut beats = Vec::new();
        for _ in 0..(WATCH_SECONDS as usize + 1) {
            q.update(1.0, &At(50.0), &mut beats);
        }
        assert!(beats.contains(&Beat::Notice(t!("INSTRUCTION_LEFT_WAREHOUSE_UNATTENDED"))));
        assert_eq!(q.bad_deeds, 1);
        assert!(!beats.iter().any(|b| matches!(
            b,
            Beat::Say {
                who: Some(BARREL_THUG),
                ..
            }
        )));
    }

    #[test]
    fn finding_the_father_ends_the_prologue_at_the_guild() {
        let mut q = Aftermath::new();
        assert_eq!(q.mood(DEAD_FATHER), Some("MORTALLY_WOUNDED"));
        let mut out = Vec::new();
        q.update(1.0, &At(10.0), &mut out);
        assert!(out.is_empty() && !q.finished());
        q.update(1.0, &At(2.0), &mut out);
        assert!(says(&out, "TEXT_CS_048_OVID_40"));
        assert_eq!(
            out.last(),
            Some(&Beat::Travel {
                region: GUILD,
                sections: GUILD_SECTIONS,
                at: GUILD_ARRIVAL,
                facing: GUILDMASTER,
            })
        );
        let before = out.len();
        q.update(1.0, &At(2.0), &mut out);
        q.talk(person(DEAD_FATHER), &mut out);
        assert_eq!(out.len(), before, "found once");
    }

    #[test]
    fn talking_to_someone_else_drops_a_question() {
        let mut q = Birthday::new();
        talk(&mut q, TRADER);
        talk(&mut q, VILLAGER);
        assert!(answer(&mut q, true).is_empty());
    }

    #[test]
    fn villagers_answer_in_their_own_voice() {
        let mut q = Birthday::new();
        let mut out = Vec::new();
        q.talk(
            Person {
                role: VILLAGER,
                female: true,
            },
            &mut out,
        );
        assert!(says(&out, t!("VILLAGER_SPOKEN_TO_FEMALE")));
        assert!(talk(&mut q, "SOMEONE_ELSE").is_empty());
    }
}
