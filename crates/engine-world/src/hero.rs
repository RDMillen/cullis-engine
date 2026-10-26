//! The Hero's movement: speed from his walk and run clips (so his feet do not
//! slide), stepping round what is in the way, and which clip he plays: idle
//! with a fidget now and then, walk, run, or his kick.

use glam::Vec2;

use crate::anim::Animator;
use crate::body::MESH_SCALE;
use crate::walk::{self, Hero};
use crate::world::World;

/// His first clips: idle, walk, run; his fidgets follow, then his kick.
pub const MAIN_CLIPS: usize = 3;
/// How wide a person is, for keeping the Hero and creatures apart.
pub const PERSON_RADIUS: f32 = 0.35;
/// Seconds standing before a fidget.
const FIDGET_AFTER: f32 = 10.0;

/// What the Hero is doing between ticks.
#[derive(Debug, Clone, Default)]
pub struct HeroState {
    /// Kicking: he stands still until the kick ends.
    pub kicking: bool,
    /// Seconds standing since he last moved or fidgeted, and fidgets so far.
    standing: f32,
    fidgets: usize,
}

/// The Hero's animations from his table, as bank ids: idle (`ST_IDLE_SUBTLE`),
/// walk (`ST_WALK`), run (`ST_RUN`) and fidgets (`ST_IDLE`).
#[derive(Debug, Clone, Default)]
pub struct HeroAnimations {
    pub main: [Option<u32>; 3],
    pub fidgets: Vec<u32>,
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
        Self {
            main: [idles.base, plain("ST_WALK"), plain("ST_RUN")],
            fidgets: idles.fidgets,
        }
    }
}

impl World {
    /// Starts the Hero's kick, unless he is kicking already or has none.
    pub fn start_kick(&mut self) -> bool {
        match (
            self.hero_state.kicking,
            self.hero_kick,
            &mut self.hero_animator,
        ) {
            (false, Some(kick), Some(animator)) => {
                animator.play_once(kick, 0.0);
                self.hero_state.kicking = true;
                true
            }
            _ => false,
        }
    }

    /// Moves `hero` (out of the world for his step) `dt` seconds towards
    /// `direction` (world x, y; zero to stand), running if `run`, blocked by
    /// closed doors and people, and plays his clip. Whether he moved.
    pub fn move_hero(&mut self, hero: &mut Hero, direction: Vec2, run: bool, dt: f32) -> bool {
        let Some(heights) = &self.heights else {
            return false;
        };
        let kicking = self.hero_state.kicking;
        let direction = if kicking { Vec2::ZERO } else { direction };
        // The clips' own movement sets the speed.
        let clip_speed = self
            .hero_animator
            .as_ref()
            .and_then(|a| {
                a.speed(if run { 2 } else { 1 })
                    .map(|s| s * a.stride_ratio())
            })
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
        let kick = self.hero_kick;
        let kicking = kicking
            && self
                .hero_animator
                .as_ref()
                .is_some_and(|a| Some(a.current()) == kick && !a.finished());
        self.hero_state.kicking = kicking;
        let clip = match (moved, run) {
            _ if kicking => kick.unwrap_or(0),
            (false, _) => idle_clip(
                self.hero_animator.as_ref(),
                kick.unwrap_or(usize::MAX),
                &mut self.hero_state,
                dt,
            ),
            (true, false) => 1,
            (true, true) => 2,
        };
        if moved {
            self.hero_state.standing = 0.0;
        }
        if let Some(animator) = &mut self.hero_animator {
            animator.play(clip);
            animator.advance(dt);
        }
        moved
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
    if current >= MAIN_CLIPS && !animator.finished() {
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

    /// Whether the Hero may not stand at `p`: a closed door, or a creature there.
    pub fn hero_blocked(&self, p: Vec2) -> bool {
        self.door_blocks(p) || self.person_at(p, PERSON_RADIUS * 2.0)
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
        // Without a kick clip there is no kick.
        assert!(!w.start_kick());
        assert!(!w.move_hero(&mut hero, Vec2::ZERO, false, 0.1), "standing");
    }
}
