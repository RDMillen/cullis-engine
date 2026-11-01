//! Fighting: the Hero's unarmed strikes in chains, locking on to a foe, and what
//! takes the blows (training dummies react; people flinch, which the app shows).
//! When a blow lands is read from each strike's clip ([`contact_time`]); which
//! clips make up a chain, how far a blow reaches and how a foe is picked are
//! guesses (see `docs/quests/guild-training.md`).

use glam::{Mat4, Vec2, Vec3};
use hecs::Entity;

use crate::anim::{self, Animator, Clip};
use crate::body::Placed;
use crate::hero::PERSON_RADIUS;
use crate::walk::Hero;
use crate::world::World;

/// How far away a foe can be locked on to, and how far it may go before the
/// lock is lost.
pub const LOCK_RANGE: f32 = 10.0;
const LOSE_RANGE: f32 = 15.0;
/// How far in front of the Hero a blow reaches: to a foe's edge.
pub const STRIKE_REACH: f32 = 1.0;
/// How far either side of straight ahead a blow lands (the cosine: 60 degrees).
const STRIKE_ARC: f32 = 0.5;
/// How much higher or lower than the Hero a foe may stand.
const FLOOR: f32 = 2.0;

/// The dummy animator's clips, from its table (`DEFAULT`, `GET_HIT`, `WOBBLE`,
/// `GET_HIT_SPIN`).
pub const DUMMY_DEFAULT: usize = 0;
pub const DUMMY_HIT: usize = 1;
pub const DUMMY_WOBBLE: usize = 2;
pub const DUMMY_SPIN: usize = 3;
/// Blows with a weapon that break a training dummy (a guess); fists never do
/// ("Not making much of an impact there").
pub const BREAKING_BLOWS: u32 = 3;

/// One strike: its clip in the Hero's animator, and how far into it the blow
/// lands, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strike {
    pub clip: usize,
    pub contact: f32,
}

/// The Hero's fighting clips: his chains of strikes (pressing attack again
/// during a strike plays the next; chains take turns), and his stance while
/// locked on (idle, then stepping forward, back, left and right).
#[derive(Debug, Clone, Default)]
pub struct HeroCombat {
    pub chains: Vec<Vec<Strike>>,
    pub stance: Option<[usize; 5]>,
    /// The first of these clips in his animator (his fidgets end there).
    pub first_clip: Option<usize>,
    /// His weapon's clips (drawing it, putting it away, armed strikes).
    pub weapon: crate::weapon::WeaponClips,
}

/// A strike under way: which chain and step, whether its blow has landed, and
/// whether the next strike is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Striking {
    chain: usize,
    step: usize,
    landed: bool,
    next: bool,
}

/// Something the Hero can lock on to and hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foe {
    /// An entity with a [`Target`] (a training dummy).
    Thing(Entity),
    /// A creature, by its index in [`World::npcs`].
    Person(usize),
}

/// A thing that can be locked on to and hit (a component): where it stands and
/// how wide it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub at: Vec3,
    pub radius: f32,
}

/// A training dummy (a component): it reels from blows by its own clips.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dummy {
    pub hits: u32,
    /// Blows from a weapon, and whether they have broken it.
    pub armed_hits: u32,
    pub broken: bool,
}

/// A thing's `ScriptName` from its level file (a component), by which quests
/// know it: the lesson's dummy is `PreMeleeDummy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptName(pub String);

/// When the blow of strike `clip` lands: the end of the frame in which one of
/// the `limbs` (bone names: `Bip01 R Hand`, …) snaps forward fastest, measured
/// from the pelvis (meshes face their local -y; strikes also carry the whole
/// body forward, and many end held at full stretch, so neither where the limb
/// is nor how far it reaches marks the blow), in seconds.
pub fn contact_time(skeleton: &fable_data::mesh::Skeleton, clip: &Clip, limbs: &[&str]) -> f32 {
    let find = |name: &str| skeleton.bones.iter().position(|b| b.name == name);
    let limbs: Vec<usize> = limbs.iter().filter_map(|n| find(n)).collect();
    let pelvis = find("Bip01 Pelvis");
    let ahead = |pose: &[Mat4], i: usize| {
        -pose[i]
            .transform_point3(anim::bind_origin(&skeleton.bones[i]))
            .y
    };
    let reach = |frame: u32| -> Vec<f32> {
        let pose = anim::pose(skeleton, Some(clip), frame as f32 / clip.fps(), &[]);
        let from = pelvis.map_or(0.0, |p| ahead(&pose, p));
        limbs.iter().map(|&i| ahead(&pose, i) - from).collect()
    };
    let mut best = (f32::NEG_INFINITY, 0.0);
    let mut before = reach(0);
    for frame in 1..clip.frames() {
        let now = reach(frame);
        for (a, b) in before.iter().zip(&now) {
            if b - a > best.0 {
                best = (b - a, frame as f32 / clip.fps());
            }
        }
        before = now;
    }
    best.1
}

impl World {
    /// Adds a training dummy placed by `transform`, standing still, named
    /// `name` if its level file names it.
    pub fn spawn_dummy(
        &mut self,
        transform: Mat4,
        target: Target,
        name: Option<String>,
        mut animator: Animator,
    ) -> Entity {
        animator.play(DUMMY_DEFAULT);
        animator.advance(0.0);
        let e = self
            .ecs
            .spawn((Placed(transform), target, Dummy::default(), animator));
        if let Some(name) = name {
            let _ = self.ecs.insert_one(e, ScriptName(name));
        }
        e
    }

    /// The script name of thing `e`, if it has one.
    pub fn thing_name(&self, e: Entity) -> Option<String> {
        self.ecs.get::<&ScriptName>(e).ok().map(|n| n.0.clone())
    }

    /// Attack: starts a chain of strikes, or asks for the next strike of the
    /// one under way. Whether the Hero strikes.
    pub fn attack(&mut self) -> bool {
        if self.weapon_clip().is_some() {
            return false;
        }
        let count = self.chains().len();
        if count == 0 {
            return false;
        }
        if let Some(s) = &mut self.hero_state.striking {
            s.next = true;
            return true;
        }
        let chain = self.hero_state.chains_struck % count;
        let Some(first) = self.chains()[chain].first().copied() else {
            return false;
        };
        let Some(animator) = &mut self.hero_animator else {
            return false;
        };
        animator.play_once(first.clip, 0.0);
        log::debug!(target: "world::combat", "chain {chain} begins (clip {})", first.clip);
        self.hero_state.chains_struck += 1;
        self.hero_state.striking = Some(Striking {
            chain,
            step: 0,
            landed: false,
            next: false,
        });
        true
    }

    /// Locks on to the foe nearest in front of `hero`, or lets go of the one
    /// locked on to. Whether he is now locked on.
    pub fn toggle_lock(&mut self, hero: &Hero) -> bool {
        if self.hero_state.locked.take().is_some() {
            return false;
        }
        let facing = facing(hero);
        self.hero_state.locked = self
            .foes()
            .filter_map(|(foe, at, _)| {
                let to = (at - hero.position).truncate();
                let d = to.length();
                let ahead = to.normalize_or_zero().dot(facing);
                (d < LOCK_RANGE && (at.z - hero.position.z).abs() < FLOOR)
                    .then_some((foe, d * (2.0 - ahead)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(foe, _)| foe);
        self.hero_state.locked.is_some()
    }

    /// The foe the Hero is locked on to, if any.
    pub fn locked_on(&self) -> Option<Foe> {
        self.hero_state.locked
    }

    /// The blows that have landed since last asked (people struck flinch in
    /// the app and quests hear of it).
    pub fn take_blows(&mut self) -> Vec<Foe> {
        std::mem::take(&mut self.hero_state.blows)
    }

    /// Whether a target (a dummy) stands in the Hero's way at `p`.
    pub fn target_at(&self, p: Vec2) -> bool {
        self.ecs
            .query::<&Target>()
            .iter()
            .any(|(_, t)| t.at.truncate().distance(p) < t.radius + PERSON_RADIUS)
    }

    /// The things broken since last asked (training dummies).
    pub fn take_broken(&mut self) -> Vec<Entity> {
        std::mem::take(&mut self.hero_state.broken)
    }

    /// Where `foe` stands, if it is still there.
    pub fn foe_position(&self, foe: Foe) -> Option<Vec3> {
        match foe {
            Foe::Thing(e) => self.ecs.get::<&Target>(e).ok().map(|t| t.at),
            Foe::Person(i) => self
                .npcs
                .get(i)
                .filter(|n| !n.hidden)
                .map(|n| n.transform.w_axis.truncate()),
        }
    }

    /// Every foe: what it is, where it stands, and how wide it is.
    fn foes(&self) -> impl Iterator<Item = (Foe, Vec3, f32)> + '_ {
        let things = self
            .ecs
            .query::<&Target>()
            .iter()
            .map(|(e, t)| (Foe::Thing(e), t.at, t.radius))
            .collect::<Vec<_>>();
        let people = self
            .npcs
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.hidden)
            .map(|(i, n)| (Foe::Person(i), n.transform.w_axis.truncate(), PERSON_RADIUS));
        things.into_iter().chain(people)
    }

    /// Where the locked-on foe stands; the lock is let go once it is gone or
    /// too far away.
    pub(crate) fn locked_position(&mut self, hero: &Hero) -> Option<Vec3> {
        let at = self.hero_state.locked.and_then(|f| self.foe_position(f));
        let at = at.filter(|a| (*a - hero.position).truncate().length() < LOSE_RANGE);
        if at.is_none() {
            self.hero_state.locked = None;
        }
        at
    }

    /// The strike clip playing, if the Hero is striking.
    pub(crate) fn strike_clip(&self) -> Option<usize> {
        let s = self.hero_state.striking?;
        Some(self.chains()[s.chain][s.step].clip)
    }

    /// After the Hero's animator has advanced: lands the strike's blow at its
    /// contact time, and when the strike ends plays the next if asked for.
    pub(crate) fn step_strike(&mut self, hero: &Hero) {
        let Some(mut s) = self.hero_state.striking else {
            return;
        };
        let chain = &self.chains()[s.chain];
        let (strike, last) = (chain[s.step], s.step + 1 == chain.len());
        let next = chain.get(s.step + 1).copied();
        let Some(animator) = &self.hero_animator else {
            return;
        };
        let (playing, time, finished) = (
            animator.current() == strike.clip,
            animator.time(),
            animator.finished(),
        );
        if playing && !s.landed && time >= strike.contact {
            s.landed = true;
            if let Some(foe) = self.blow(hero, last) {
                self.hero_state.blows.push(foe);
            }
        }
        if !playing || finished {
            log::debug!(
                target: "world::combat",
                "strike {}:{} ends (playing {playing}, at {time:.2}, landed {})",
                s.chain,
                s.step,
                s.landed
            );
            match (next, s.next) {
                (Some(next), true) => {
                    s = Striking {
                        step: s.step + 1,
                        landed: false,
                        next: false,
                        ..s
                    };
                    if let Some(a) = &mut self.hero_animator {
                        a.play_once(next.clip, 0.0);
                    }
                }
                // Pressed again during a chain's last strike: the next chain.
                (None, true) => {
                    self.hero_state.striking = None;
                    self.attack();
                    return;
                }
                (_, false) => {
                    self.hero_state.striking = None;
                    return;
                }
            }
        }
        self.hero_state.striking = Some(s);
    }

    /// A blow from `hero`: the foe it reaches (the one locked on to first, else
    /// the nearest in front). A dummy reels; the last blow of a chain spins it.
    fn blow(&mut self, hero: &Hero, last: bool) -> Option<Foe> {
        let facing = facing(hero);
        let locked = self.hero_state.locked;
        if log::log_enabled!(target: "world::combat", log::Level::Debug)
            && let Some((_, at, radius)) = self.foes().min_by(|a, b| {
                let d = |p: Vec3| (p - hero.position).truncate().length();
                d(a.1).total_cmp(&d(b.1))
            })
        {
            let to = (at - hero.position).truncate();
            log::debug!(
                target: "world::combat",
                "blow: nearest foe {:.2} to its edge, facing it {:.2}",
                to.length() - radius,
                to.normalize_or_zero().dot(facing)
            );
        }
        let foe = self
            .foes()
            .filter(|&(_, at, radius)| {
                let to = (at - hero.position).truncate();
                to.length() - radius <= STRIKE_REACH
                    && to.normalize_or_zero().dot(facing) >= STRIKE_ARC
                    && (at.z - hero.position.z).abs() < FLOOR
            })
            .map(|(foe, at, _)| {
                let d = (at - hero.position).truncate().length();
                (foe, if Some(foe) == locked { -1.0 } else { d })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(foe, _)| foe)?;
        let armed = self.armed();
        if let Foe::Thing(e) = foe
            && let Ok((dummy, animator)) = self.ecs.query_one_mut::<(&mut Dummy, &mut Animator)>(e)
        {
            dummy.hits += 1;
            animator.play_once(if last { DUMMY_SPIN } else { DUMMY_HIT }, 0.0);
            dummy.armed_hits += u32::from(armed);
            if dummy.armed_hits >= BREAKING_BLOWS && !dummy.broken {
                // Broken: no longer a target; the app swaps its mesh.
                dummy.broken = true;
                let _ = self.ecs.remove_one::<Target>(e);
                self.hero_state.broken.push(e);
                if self.hero_state.locked == Some(foe) {
                    self.hero_state.locked = None;
                }
            }
        }
        Some(foe)
    }

    /// Dummies settle after a blow: they wobble, then stand still.
    pub(crate) fn settle_dummies(&mut self) {
        for (_, (_, animator)) in self.ecs.query_mut::<(&Dummy, &mut Animator)>() {
            if !animator.finished() {
                continue;
            }
            match animator.current() {
                DUMMY_HIT | DUMMY_SPIN => animator.play_once(DUMMY_WOBBLE, 0.0),
                DUMMY_WOBBLE => animator.play(DUMMY_DEFAULT),
                _ => {}
            }
        }
    }
}

/// The way `hero` faces, in world x, y.
fn facing(hero: &Hero) -> Vec2 {
    Vec2::new(-hero.heading.sin(), hero.heading.cos())
}

/// The stance clip for stepping along `direction` (zero to stand) while facing
/// `facing`: idle, forward, back, left or right, whichever is nearest.
pub(crate) fn stance_clip(stance: [usize; 5], direction: Vec2, facing: Vec2) -> usize {
    if direction == Vec2::ZERO {
        return stance[0];
    }
    let right = Vec2::new(facing.y, -facing.x);
    let (ahead, side) = (direction.dot(facing), direction.dot(right));
    match (ahead.abs() >= side.abs(), ahead >= 0.0, side >= 0.0) {
        (true, true, _) => stance[1],
        (true, false, _) => stance[2],
        (false, _, false) => stance[3],
        (false, _, true) => stance[4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Rig;
    use crate::walk::{HeightGrid, WalkGrid};

    /// A Hero at (2, 5) facing +x, with one chain of two strikes (clips 0 and
    /// 1, 0.5 s long, landing at 0.2 s), and a dummy 1.2 units ahead.
    fn arena() -> (World, Hero, Entity) {
        let mut grid = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        grid.mark(Vec2::splat(5.0), 10.0);
        let clips = vec![Clip::still("left", 10.0, 5), Clip::still("right", 10.0, 5)];
        let mut w = World {
            walk: vec![grid],
            heights: Some(HeightGrid::new(Vec2::ZERO, [11, 11])),
            hero_animator: Some(Animator::new(Rig::default(), clips)),
            hero_combat: HeroCombat {
                chains: vec![vec![
                    Strike {
                        clip: 0,
                        contact: 0.2,
                    },
                    Strike {
                        clip: 1,
                        contact: 0.2,
                    },
                ]],
                stance: None,
                first_clip: Some(0),
                weapon: Default::default(),
            },
            ..World::default()
        };
        let mut hero = Hero::new(Vec2::new(2.0, 5.0), w.heights.as_ref().unwrap());
        hero.heading = -std::f32::consts::FRAC_PI_2;
        let dummy_clips = (0..4).map(|_| Clip::still("dummy", 10.0, 5)).collect();
        let at = Vec3::new(3.2, 5.0, 0.0);
        let dummy = w.spawn_dummy(
            Mat4::from_translation(at),
            Target { at, radius: 0.3 },
            Some("TestDummy".into()),
            Animator::new(Rig::default(), dummy_clips),
        );
        (w, hero, dummy)
    }

    fn run(w: &mut World, hero: &mut Hero, ticks: usize) {
        for _ in 0..ticks {
            w.move_hero(hero, Vec2::ZERO, false, 1.0 / 30.0);
            w.tick(1.0 / 30.0);
        }
    }

    #[test]
    fn a_blow_lands_at_its_contact_time_and_the_dummy_reels() {
        let (mut w, mut hero, dummy) = arena();
        assert!((facing(&hero) - Vec2::X).length() < 1e-5, "facing +x");
        assert!(w.attack());
        run(&mut w, &mut hero, 3);
        assert!(w.take_blows().is_empty(), "not yet at 0.1 s");
        run(&mut w, &mut hero, 5);
        assert_eq!(w.take_blows(), [Foe::Thing(dummy)]);
        assert_eq!(w.thing_name(dummy).as_deref(), Some("TestDummy"));
        let (hits, reeling) = {
            let mut q = w.ecs.query_one::<(&Dummy, &Animator)>(dummy).unwrap();
            let (d, a) = q.get().unwrap();
            (d.hits, a.current())
        };
        assert_eq!((hits, reeling), (1, DUMMY_HIT));
        // The strike ends without a second press, and the dummy settles.
        run(&mut w, &mut hero, 60);
        assert_eq!(w.strike_clip(), None);
        let a = w.ecs.get::<&Animator>(dummy).unwrap().current();
        assert_eq!(a, DUMMY_DEFAULT);
    }

    #[test]
    fn pressing_again_chains_the_next_strike_and_its_last_spins_the_dummy() {
        let (mut w, mut hero, dummy) = arena();
        w.attack();
        run(&mut w, &mut hero, 2);
        assert!(w.attack(), "asks for the next");
        run(&mut w, &mut hero, 16);
        assert_eq!(w.strike_clip(), Some(1), "second strike");
        assert_eq!(w.take_blows().len(), 2);
        let a = w.ecs.get::<&Animator>(dummy).unwrap().current();
        assert_eq!(a, DUMMY_SPIN);
    }

    #[test]
    fn strikes_pressed_apart_take_turns_and_each_lands() {
        let (mut w, mut hero, _) = arena();
        // A second chain, of clip 1 alone.
        w.hero_combat.chains = vec![
            vec![Strike {
                clip: 0,
                contact: 0.2,
            }],
            vec![Strike {
                clip: 1,
                contact: 0.2,
            }],
        ];
        let mut blows = 0;
        for _ in 0..4 {
            assert!(w.attack());
            run(&mut w, &mut hero, 30);
            blows += w.take_blows().len();
        }
        assert_eq!(blows, 4);
    }

    #[test]
    fn pressing_during_a_chains_last_strike_starts_the_next_chain() {
        let (mut w, mut hero, _) = arena();
        w.hero_combat.chains = vec![
            vec![Strike {
                clip: 0,
                contact: 0.2,
            }],
            vec![Strike {
                clip: 1,
                contact: 0.2,
            }],
        ];
        w.attack();
        run(&mut w, &mut hero, 2);
        w.attack();
        run(&mut w, &mut hero, 40);
        assert_eq!(w.take_blows().len(), 2, "both chains struck");
    }

    #[test]
    fn weapon_blows_break_a_dummy_and_fists_never_do() {
        let (mut w, mut hero, dummy) = arena();
        for _ in 0..BREAKING_BLOWS + 2 {
            w.attack();
            run(&mut w, &mut hero, 30);
        }
        assert!(w.take_broken().is_empty(), "punches");
        // The same strikes, now with a weapon in hand.
        w.hero_combat.weapon.chains = w.hero_combat.chains.clone();
        w.hero_state.carried = Some(crate::weapon::Carried::Hand);
        w.toggle_lock(&hero);
        for _ in 0..BREAKING_BLOWS {
            w.attack();
            run(&mut w, &mut hero, 30);
        }
        assert_eq!(w.take_broken(), [dummy]);
        assert!(w.ecs.get::<&Target>(dummy).is_err(), "no longer a target");
        assert_eq!(w.locked_on(), None, "the lock lets go of it");
    }

    #[test]
    fn a_blow_misses_what_is_out_of_reach_or_behind() {
        let (mut w, mut hero, _) = arena();
        hero.heading = std::f32::consts::FRAC_PI_2; // facing -x, away
        w.attack();
        run(&mut w, &mut hero, 30);
        assert!(w.take_blows().is_empty());
    }

    #[test]
    fn locking_on_picks_the_dummy_and_lets_go() {
        let (mut w, hero, dummy) = arena();
        assert!(w.toggle_lock(&hero));
        assert_eq!(w.locked_on(), Some(Foe::Thing(dummy)));
        assert!(!w.toggle_lock(&hero));
        assert_eq!(w.locked_on(), None);
    }

    #[test]
    fn the_stance_steps_the_way_asked_relative_to_the_foe() {
        let stance = [10, 11, 12, 13, 14];
        let facing = Vec2::Y;
        assert_eq!(stance_clip(stance, Vec2::ZERO, facing), 10);
        assert_eq!(stance_clip(stance, Vec2::Y, facing), 11);
        assert_eq!(stance_clip(stance, -Vec2::Y, facing), 12);
        assert_eq!(stance_clip(stance, -Vec2::X, facing), 13, "left of +y");
        assert_eq!(stance_clip(stance, Vec2::X, facing), 14);
    }
}
