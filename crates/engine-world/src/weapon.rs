//! The Hero's weapon (so far the childhood stick): given to him, carried on his
//! back, drawn into his hand and put away by its own clips, and struck with in
//! his armed chains. While it is drawn he stands, walks and runs as one who
//! holds it.

use crate::combat::Strike;
use crate::world::World;

/// The Hero's clips for his weapon, as indices into his animator.
#[derive(Debug, Clone, Default)]
pub struct WeaponClips {
    /// Drawing it (`STANDARD_UNSHEATHE_STICK`) and putting it away
    /// (`STANDARD_SHEATHE_STICK`), each with the time the weapon passes from
    /// back to hand or hand to back (where the hand comes nearest the
    /// scabbard).
    pub draw: Option<(usize, f32)>,
    pub sheathe: Option<(usize, f32)>,
    /// His chains of strikes while it is drawn.
    pub chains: Vec<Vec<Strike>>,
    /// Standing, walking and running while it is drawn (`ST_IDLE_SUBTLE`,
    /// `ST_WALK`, `ST_RUN` with the `SWORD` modifier).
    pub idle: Option<usize>,
    pub walk: Option<usize>,
    pub run: Option<usize>,
}

/// When in `clip` bone `bone` (`Bip01 R Hand`) comes nearest `point` (a
/// socket's place in the bind pose, mesh units): where a drawn weapon leaves
/// the scabbard, or a sheathed one reaches it. Seconds.
pub fn nearest_time(
    skeleton: &fable_data::mesh::Skeleton,
    clip: &crate::anim::Clip,
    bone: &str,
    point: glam::Vec3,
) -> f32 {
    let Some(i) = skeleton.bones.iter().position(|b| b.name == bone) else {
        return clip.last_frame_time() / 2.0;
    };
    let origin = crate::anim::bind_origin(&skeleton.bones[i]);
    (0..clip.frames())
        .map(|f| {
            let t = f as f32 / clip.fps();
            let at = crate::anim::pose(skeleton, Some(clip), t, &[])[i].transform_point3(origin);
            (at.distance(point), t)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0.0, |(_, t)| t)
}

/// Where the Hero's weapon is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    Back,
    /// Being drawn (by his draw clip) or put away (by his sheathe clip).
    Drawing,
    Hand,
    Sheathing,
}

impl World {
    /// Gives the Hero his weapon, carried on his back.
    pub fn give_weapon(&mut self) {
        if self.hero_state.carried.is_none() {
            self.hero_state.carried = Some(Carried::Back);
        }
    }

    /// Draws the weapon from his back or puts it away; whether he does. Not
    /// while striking or already doing either.
    pub fn toggle_weapon(&mut self) -> bool {
        if self.hero_state.striking.is_some() {
            return false;
        }
        let clips = &self.hero_combat.weapon;
        let (next, clip) = match self.hero_state.carried {
            Some(Carried::Back) => (Carried::Drawing, clips.draw),
            Some(Carried::Hand) => (Carried::Sheathing, clips.sheathe),
            _ => return false,
        };
        let (Some((clip, _)), Some(animator)) = (clip, &mut self.hero_animator) else {
            // Without the clip the weapon simply moves.
            self.hero_state.carried = Some(match next {
                Carried::Drawing => Carried::Hand,
                _ => Carried::Back,
            });
            return true;
        };
        animator.play_once(clip, 0.0);
        self.hero_state.carried = Some(next);
        true
    }

    /// Where the weapon is drawn: `None` without one, else whether it is in
    /// his hand (`false`: on his back), passing between them partway through
    /// drawing and putting away.
    pub fn weapon_in_hand(&self) -> Option<bool> {
        let clips = &self.hero_combat.weapon;
        let time = self.hero_animator.as_ref().map_or(0.0, |a| a.time());
        Some(match self.hero_state.carried? {
            Carried::Back => false,
            Carried::Hand => true,
            Carried::Drawing => clips.draw.is_none_or(|(_, at)| time >= at),
            Carried::Sheathing => clips.sheathe.is_some_and(|(_, at)| time < at),
        })
    }

    /// Whether the weapon is drawn and ready (in his hand, not being put away).
    pub fn armed(&self) -> bool {
        self.hero_state.carried == Some(Carried::Hand)
    }

    /// The clip of drawing or putting away under way, if any.
    pub(crate) fn weapon_clip(&self) -> Option<usize> {
        let clips = &self.hero_combat.weapon;
        match self.hero_state.carried? {
            Carried::Drawing => clips.draw.map(|(c, _)| c),
            Carried::Sheathing => clips.sheathe.map(|(c, _)| c),
            _ => None,
        }
    }

    /// After the Hero's animator has advanced: drawing or putting away ends
    /// with its clip.
    pub(crate) fn step_weapon(&mut self) {
        let done = self
            .hero_animator
            .as_ref()
            .is_none_or(|a| Some(a.current()) != self.weapon_clip() || a.finished());
        self.hero_state.carried = match self.hero_state.carried {
            Some(Carried::Drawing) if done => Some(Carried::Hand),
            Some(Carried::Sheathing) if done => Some(Carried::Back),
            other => other,
        };
    }

    /// The strike chains in use: the armed ones while the weapon is drawn.
    pub(crate) fn chains(&self) -> &[Vec<Strike>] {
        let armed = &self.hero_combat.weapon.chains;
        if self.armed() && !armed.is_empty() {
            armed
        } else {
            &self.hero_combat.chains
        }
    }

    /// His standing, walking and running clips: armed ones while the weapon is
    /// drawn, where he has them.
    pub(crate) fn moving_clips(&self) -> [Option<usize>; 3] {
        let w = &self.hero_combat.weapon;
        if self.armed() {
            [w.idle, w.walk.or(Some(1)), w.run.or(Some(2))]
        } else {
            [None, Some(1), Some(2)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Animator, Clip, Rig};
    use crate::walk::{HeightGrid, Hero, WalkGrid};
    use glam::Vec2;

    /// A Hero with idle, walk and run, an unarmed strike (3), the draw (4) and
    /// sheathe (5) clips passing the weapon at 0.2 s, and an armed strike (6).
    fn armed_hero() -> (World, Hero) {
        let mut grid = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        grid.mark(Vec2::splat(5.0), 10.0);
        let clips = (0..7).map(|_| Clip::still("clip", 10.0, 5)).collect();
        let w = World {
            walk: vec![grid],
            heights: Some(HeightGrid::new(Vec2::ZERO, [11, 11])),
            hero_animator: Some(Animator::new(Rig::default(), clips)),
            hero_combat: crate::combat::HeroCombat {
                chains: vec![vec![Strike {
                    clip: 3,
                    contact: 0.2,
                }]],
                stance: None,
                first_clip: Some(3),
                weapon: WeaponClips {
                    draw: Some((4, 0.2)),
                    sheathe: Some((5, 0.2)),
                    chains: vec![vec![Strike {
                        clip: 6,
                        contact: 0.2,
                    }]],
                    ..Default::default()
                },
            },
            ..World::default()
        };
        let hero = Hero::new(Vec2::new(5.0, 5.0), w.heights.as_ref().unwrap());
        (w, hero)
    }

    fn run(w: &mut World, hero: &mut Hero, ticks: usize) {
        for _ in 0..ticks {
            w.move_hero(hero, Vec2::ZERO, false, 0.05);
        }
    }

    #[test]
    fn a_weapon_is_drawn_into_the_hand_and_struck_with() {
        let (mut w, mut hero) = armed_hero();
        assert_eq!(w.weapon_in_hand(), None);
        assert!(!w.toggle_weapon(), "nothing to draw");
        w.give_weapon();
        assert_eq!(w.weapon_in_hand(), Some(false), "on his back");
        assert!(w.toggle_weapon());
        run(&mut w, &mut hero, 2);
        assert_eq!(w.weapon_in_hand(), Some(false), "not yet");
        run(&mut w, &mut hero, 4);
        assert_eq!(w.weapon_in_hand(), Some(true), "passed to his hand");
        run(&mut w, &mut hero, 10);
        assert!(w.armed());
        assert!(w.attack());
        assert_eq!(w.strike_clip(), Some(6), "the armed strike");
        run(&mut w, &mut hero, 20);
        assert!(w.toggle_weapon());
        run(&mut w, &mut hero, 20);
        assert_eq!(w.weapon_in_hand(), Some(false), "put away");
        w.attack();
        assert_eq!(w.strike_clip(), Some(3), "unarmed again");
    }
}
