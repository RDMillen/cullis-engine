//! The Hero's movement: speed from his walk and run clips (so his feet do not
//! slide), stepping round what is in the way, and which clip he plays: idle
//! with a fidget now and then, walk, run, a strike, or while locked on to a foe
//! his combat stance, facing it ([`crate::combat`]).

use glam::Vec2;

use crate::anim::Animator;
use crate::body::MESH_SCALE;
use crate::walk::{self, Hero};
use crate::world::World;

/// His first clips: idle, walk, run; his fidgets follow, then his fighting
/// clips.
pub const MAIN_CLIPS: usize = 3;
/// How wide a person is, for keeping the Hero and creatures apart.
pub const PERSON_RADIUS: f32 = 0.35;
/// Seconds standing before a fidget.
const FIDGET_AFTER: f32 = 10.0;

/// What the Hero is doing between ticks.
#[derive(Debug, Clone, Default)]
pub struct HeroState {
    /// The strike under way: he stands still until it ends.
    pub(crate) striking: Option<crate::combat::Striking>,
    /// Chains of strikes begun (they take turns), the foe locked on to, and
    /// blows landed since last asked.
    pub(crate) chains_struck: usize,
    pub(crate) locked: Option<crate::combat::Foe>,
    pub(crate) blows: Vec<crate::combat::Foe>,
    /// Where his weapon is, if he has one, and things broken since last asked.
    pub(crate) carried: Option<crate::weapon::Carried>,
    pub(crate) broken: Vec<hecs::Entity>,
    /// Seconds standing since he last moved or fidgeted, and fidgets so far.
    standing: f32,
    fidgets: usize,
}

/// The Hero's animations from his table, as bank ids: idle (`ST_IDLE_SUBTLE`),
/// walk (`ST_WALK`), run (`ST_RUN`), fidgets (`ST_IDLE`), and his unarmed
/// stance while locked on (`STRAFE_IDLE`, `STRAFE_MOVEMENT_FORWARD`, `_BACK`,
/// `_LEFT`, `_RIGHT`, all with the `STAND` modifier).
#[derive(Debug, Clone, Default)]
pub struct HeroAnimations {
    pub main: [Option<u32>; 3],
    pub fidgets: Vec<u32>,
    pub stance: Option<[u32; 5]>,
}

impl HeroAnimations {
    pub fn from_slots(slots: &[fable_data::defs::AnimationSlot]) -> Self {
        let idles = crate::region::IdleAnimations::from_slots(slots);
        let plain = |slot: &str| {
            slots
                .iter()
                .find(|s| s.slot == slot && s.modifier.as_deref().is_none_or(|m| m == "STAND"))
                .map(|s| s.animation)
        };
        let stand = |slot: &str| {
            slots
                .iter()
                .find(|s| s.slot == slot && s.modifier.as_deref() == Some("STAND"))
                .map(|s| s.animation)
        };
        let stance = [
            "STRAFE_IDLE",
            "STRAFE_MOVEMENT_FORWARD",
            "STRAFE_MOVEMENT_BACK",
            "STRAFE_MOVEMENT_LEFT",
            "STRAFE_MOVEMENT_RIGHT",
        ]
        .map(stand);
        Self {
            main: [idles.base, plain("ST_WALK"), plain("ST_RUN")],
            fidgets: idles.fidgets,
            stance: stance
                .iter()
                .all(Option::is_some)
                .then(|| stance.map(Option::unwrap_or_default)),
        }
    }
}

impl World {
    /// Moves `hero` (out of the world for his step) `dt` seconds towards
    /// `direction` (world x, y; zero to stand), running if `run`, blocked by
    /// closed doors and people, and plays his clip. Striking, he stands; locked
    /// on to a foe, he faces it and steps in his stance. Whether he moved.
    pub fn move_hero(&mut self, hero: &mut Hero, direction: Vec2, run: bool, dt: f32) -> bool {
        let foe = self.locked_position(hero);
        let Some(heights) = &self.heights else {
            return false;
        };
        // Striking, drawing or putting away his weapon, he stands.
        let strike = self.strike_clip().or(self.weapon_clip());
        let direction = if strike.is_some() {
            Vec2::ZERO
        } else {
            direction
        };
        let [armed_idle, walk_clip, run_clip] = self.moving_clips();
        let (walk_clip, run_clip) = (walk_clip.unwrap_or(1), run_clip.unwrap_or(2));
        let to_foe = foe.map(|at| (at - hero.position).truncate().normalize_or_zero());
        let stance = match (self.hero_combat.stance, to_foe) {
            (Some(stance), Some(f)) if f != Vec2::ZERO => {
                Some(crate::combat::stance_clip(stance, direction, f))
            }
            _ => None,
        };
        // The clips' own movement sets the speed.
        let moving_clip = stance.unwrap_or(if run { run_clip } else { walk_clip });
        let clip_speed = self
            .hero_animator
            .as_ref()
            .and_then(|a| a.speed(moving_clip).map(|s| s * a.stride_ratio()))
            .map(|s| s * MESH_SCALE);
        let speed = clip_speed.unwrap_or(if run {
            walk::RUN_SPEED
        } else {
            walk::WALK_SPEED
        });
        let before = hero.position;
        hero.step_blocked(direction, speed, dt, &self.walk, heights, |p| {
            self.hero_blocked(p)
        });
        let moved = (hero.position - before).truncate().length() > 1e-4;
        if let Some(f) = to_foe.filter(|f| *f != Vec2::ZERO) {
            hero.heading = (-f.x).atan2(f.y);
        }
        let first_combat = self.hero_combat.first_clip.unwrap_or(usize::MAX);
        let clip = match (moved, run) {
            _ if strike.is_some() => strike.unwrap_or(0),
            // Standing locked on with his weapon drawn: its stance.
            (false, _) if stance.is_some() && armed_idle.is_some() => armed_idle.unwrap_or(0),
            _ if stance.is_some() => stance.unwrap_or(0),
            (false, _) if armed_idle.is_some() => armed_idle.unwrap_or(0),
            (false, _) => idle_clip(
                self.hero_animator.as_ref(),
                first_combat,
                &mut self.hero_state,
                dt,
            ),
            (true, false) => walk_clip,
            (true, true) => run_clip,
        };
        if moved {
            self.hero_state.standing = 0.0;
        }
        let mut travel = Vec2::ZERO;
        if let Some(animator) = &mut self.hero_animator {
            animator.play(clip);
            animator.advance(dt);
            // A strike's lunge carries him along (meshes face -y; turned by
            // his heading), stepping as a walk would, but stopping rather than
            // sliding round whatever it meets (a foe, a wall).
            let t = -animator.take_travel() * MESH_SCALE * animator.stride_ratio();
            travel = Vec2::from_angle(hero.heading).rotate(t);
        }
        let ahead = hero.position.truncate() + travel;
        if travel != Vec2::ZERO && dt > 0.0 && !self.hero_blocked(ahead) {
            let heading = hero.heading;
            hero.step_blocked(travel, travel.length() / dt, dt, &self.walk, heights, |p| {
                self.hero_blocked(p)
            });
            hero.heading = heading;
        }
        self.step_strike(hero);
        self.step_weapon();
        moved || travel != Vec2::ZERO
    }
}

/// The clip the standing Hero plays: his idle (0), or after he has stood for a
/// while one of his fidgets (clips from [`MAIN_CLIPS`] up to `end`), played
/// once.
fn idle_clip(animator: Option<&Animator>, end: usize, state: &mut HeroState, dt: f32) -> usize {
    let Some(animator) = animator else {
        return 0;
    };
    let (current, count) = (animator.current(), animator.clip_count().min(end));
    if (MAIN_CLIPS..count).contains(&current) && !animator.finished() {
        return current;
    }
    state.standing += dt;
    if count > MAIN_CLIPS && state.standing > FIDGET_AFTER {
        state.standing = 0.0;
        state.fidgets += 1;
        return MAIN_CLIPS + state.fidgets % (count - MAIN_CLIPS);
    }
    0
}

impl World {
    /// Where a route should not go: near people, with a margin so that walking
    /// between the route's points does not graze them. Closed doors are no
    /// obstacle to a route: whoever follows it opens them.
    pub fn route_blocked(&self, p: Vec2) -> bool {
        self.person_at(p, PERSON_RADIUS * 2.0 + walk::CELL)
    }

    /// Whether the Hero may not stand at `p`: a closed door, a creature or a
    /// training dummy there.
    pub fn hero_blocked(&self, p: Vec2) -> bool {
        self.door_blocks(p) || self.person_at(p, PERSON_RADIUS * 2.0) || self.target_at(p)
    }

    fn person_at(&self, p: Vec2, within: f32) -> bool {
        self.npcs
            .iter()
            .any(|n| !n.hidden && n.position().distance(p) < within)
    }

    /// The direction for `hero` to walk along `follow`'s route this tick, None
    /// once there or given up; a closed door just ahead on the way is opened.
    pub fn follow_route(
        &mut self,
        hero: &Hero,
        follow: &mut walk::Follow,
        dt: f32,
    ) -> Option<Vec2> {
        let heights = self.heights.as_ref()?;
        let to = follow.direction(
            hero,
            &self.walk,
            heights,
            dt,
            |p| self.route_blocked(p),
            |p| self.hero_blocked(p),
        )?;
        if let Some(door) = self.door_ahead(hero.position, to) {
            log::debug!(target: "world::door", "opened on the way at {}", hero.position);
            self.toggle_door(door);
        }
        Some(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Rig;
    use crate::walk::{HeightGrid, WalkGrid};
    use glam::Mat4;

    fn world() -> World {
        let mut grid = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        grid.mark(Vec2::splat(5.0), 10.0);
        World {
            walk: vec![grid],
            heights: Some(HeightGrid::new(Vec2::ZERO, [11, 11])),
            hero_animator: Some(Animator::new(Rig::default(), Vec::new())),
            ..World::default()
        }
    }

    /// Two rooms joined by a corridor with a closed door across it.
    fn rooms_with_a_door() -> (World, hecs::Entity) {
        let mut grid = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        grid.mark(Vec2::new(2.0, 5.0), 4.0);
        grid.mark(Vec2::new(8.0, 5.0), 4.0);
        grid.mark(Vec2::new(5.0, 5.0), 2.0);
        let mut w = World {
            walk: vec![grid],
            heights: Some(HeightGrid::new(Vec2::ZERO, [11, 11])),
            ..World::default()
        };
        // Its local x runs across the corridor (world y 4 to 6), at x = 5.
        let transform = Mat4::from_translation(glam::Vec3::new(5.0, 4.0, 0.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let door = crate::door::Door::new(transform, (0.0, 2.0), 1.0, false);
        let e = w.spawn_door(door, Animator::new(Rig::default(), Vec::new()));
        (w, e)
    }

    #[test]
    fn a_route_through_a_closed_door_opens_it() {
        let (mut w, door) = rooms_with_a_door();
        let heights = w.heights.clone().unwrap();
        let mut hero = Hero::new(Vec2::new(2.0, 5.0), &heights);
        assert!(w.hero_blocked(Vec2::new(5.0, 5.0)), "closed");
        let goal = Vec2::new(8.0, 5.0);
        let mut follow = walk::Follow::to_floor(goal, None);
        for _ in 0..600 {
            let Some(to) = w.follow_route(&hero, &mut follow, 1.0 / 30.0) else {
                break;
            };
            w.move_hero(&mut hero, to, false, 1.0 / 30.0);
        }
        assert!(follow.routed(), "a route through the doorway");
        assert!(
            hero.position.truncate().distance(goal) < 0.5,
            "ended at {}",
            hero.position
        );
        assert!(w.ecs.get::<&crate::door::Door>(door).unwrap().open);
    }

    #[test]
    fn walking_into_a_closed_door_does_not_open_it() {
        let (mut w, door) = rooms_with_a_door();
        let heights = w.heights.clone().unwrap();
        let mut hero = Hero::new(Vec2::new(3.5, 5.0), &heights);
        for _ in 0..90 {
            w.move_hero(&mut hero, Vec2::X, false, 1.0 / 30.0);
        }
        assert!(
            hero.position.x < 5.0,
            "stopped at the door: {}",
            hero.position
        );
        assert!(!w.ecs.get::<&crate::door::Door>(door).unwrap().open);
    }

    #[test]
    fn the_hero_walks_then_stands() {
        let mut w = world();
        let heights = w.heights.clone().unwrap();
        let mut hero = Hero::new(Vec2::new(2.0, 5.0), &heights);
        let moved = w.move_hero(&mut hero, Vec2::X, false, 0.5);
        assert!(moved && hero.position.x > 2.5, "{}", hero.position);
        // Without strikes there is no attack.
        assert!(!w.attack());
        assert!(!w.move_hero(&mut hero, Vec2::ZERO, false, 0.1), "standing");
    }
}
