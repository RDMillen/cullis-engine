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
        let blocked = |p: Vec2| {
            self.door_blocks(p)
                || self
                    .npcs
                    .iter()
                    .any(|n| !n.hidden && n.position().distance(p) < PERSON_RADIUS * 2.0)
        };
        hero.step_blocked(direction, speed, dt, &self.walk, heights, blocked);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Rig;
    use crate::walk::{HeightGrid, WalkGrid};

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
