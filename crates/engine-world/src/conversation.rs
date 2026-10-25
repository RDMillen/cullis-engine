//! The conversation camera: while someone talks to the Hero, the view frames
//! them, with letterbox bars.
//!
//! Lines can name their framing with a `CAM:` tag: where the camera stands
//! (by whom) and whose face it looks at, e.g.
//! `CAM:(SPEAKER,BEHIND_RIGHT_SHOULDER_NEAR)(SPEAKEE,FOCUS_FACE)`. Lines
//! without one (nearly all) get [`Shot::DEFAULT`], over the Hero's shoulder
//! onto the speaker. Distances are set by eye.

use glam::{Vec2, Vec3};

use crate::camera::OrbitCamera;

/// Someone in a conversation: where they stand (their feet) and how high
/// their face is above that.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Actor {
    pub feet: Vec3,
    pub face_height: f32,
}

impl Actor {
    pub fn face(&self) -> Vec3 {
        self.feet + Vec3::Z * self.face_height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Speaker,
    /// The one spoken to: the Hero.
    Speakee,
}

/// Where the camera stands, relative to someone facing the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    InFrontOfFace,
    BehindRightShoulderNear,
}

/// A framing: the camera at `spot` of `from`, looking at `look`'s face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub from: Who,
    pub spot: Spot,
    pub look: Who,
}

impl Shot {
    /// Over the Hero's right shoulder, onto the speaker's face.
    pub const DEFAULT: Shot = Shot {
        from: Who::Speakee,
        spot: Spot::BehindRightShoulderNear,
        look: Who::Speaker,
    };

    /// A line's `CAM:(who,spot)(who,FOCUS_FACE)` tag, if it has one this knows.
    pub fn from_tags(tags: &[String]) -> Option<Shot> {
        let tag = tags.iter().find_map(|t| t.strip_prefix("CAM:"))?;
        let parts: Vec<(&str, &str)> = tag
            .split(')')
            .filter_map(|p| p.trim_start_matches('(').split_once(','))
            .collect();
        let who = |w: &str| match w {
            "SPEAKER" => Some(Who::Speaker),
            "SPEAKEE" => Some(Who::Speakee),
            _ => None,
        };
        let &[(from, spot), (look, focus)] = parts.as_slice() else {
            return None;
        };
        let spot = match spot {
            "IN_FRONT_OF_FACE" => Spot::InFrontOfFace,
            "BEHIND_RIGHT_SHOULDER_NEAR" => Spot::BehindRightShoulderNear,
            _ => return None,
        };
        (focus == "FOCUS_FACE").then_some(())?;
        Some(Shot {
            from: who(from)?,
            spot,
            look: who(look)?,
        })
    }

    /// The camera for this shot of `speaker` talking to `speakee`.
    pub fn camera(&self, speaker: Actor, speakee: Actor) -> OrbitCamera {
        let pick = |w: Who| match w {
            Who::Speaker => (speaker, speakee),
            Who::Speakee => (speakee, speaker),
        };
        let (from, other) = pick(self.from);
        // Facing the other, level.
        let facing = (other.feet - from.feet)
            .truncate()
            .try_normalize()
            .unwrap_or(Vec2::Y)
            .extend(0.0);
        let right = Vec3::new(facing.y, -facing.x, 0.0);
        let eye = match self.spot {
            Spot::InFrontOfFace => from.face() + facing * 1.1 + right * 0.15 + Vec3::Z * 0.05,
            Spot::BehindRightShoulderNear => {
                from.face() - facing * 1.0 + right * 0.55 + Vec3::Z * 0.2
            }
        };
        let target = pick(self.look).0.face();
        looking_from(eye, target)
    }
}

/// An orbit camera at `eye` looking at `target`.
pub fn looking_from(eye: Vec3, target: Vec3) -> OrbitCamera {
    let offset = eye - target;
    let distance = offset.length().max(0.1);
    OrbitCamera {
        target,
        distance,
        yaw: offset.y.atan2(offset.x),
        pitch: (offset.z / distance).clamp(-1.0, 1.0).asin(),
    }
}

/// `a` moved towards `b` by `t` (0..=1): target and distance in a line, angles
/// the short way round.
pub fn blend(a: &OrbitCamera, b: &OrbitCamera, t: f32) -> OrbitCamera {
    let t = t.clamp(0.0, 1.0);
    let turn = |from: f32, to: f32| {
        let d = (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        from + d * t
    };
    OrbitCamera {
        target: a.target.lerp(b.target, t),
        distance: a.distance + (b.distance - a.distance) * t,
        yaw: turn(a.yaw, b.yaw),
        pitch: a.pitch + (b.pitch - a.pitch) * t,
    }
}

/// Eases 0..=1 in and out.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &str) -> Vec<String> {
        vec![t.to_owned()]
    }

    #[test]
    fn reads_camera_tags() {
        assert_eq!(
            Shot::from_tags(&tags("CAM:(SPEAKER,IN_FRONT_OF_FACE)(SPEAKER,FOCUS_FACE)")),
            Some(Shot {
                from: Who::Speaker,
                spot: Spot::InFrontOfFace,
                look: Who::Speaker
            })
        );
        assert_eq!(
            Shot::from_tags(&tags(
                "CAM:(SPEAKER,BEHIND_RIGHT_SHOULDER_NEAR)(SPEAKEE,FOCUS_FACE)"
            )),
            Some(Shot {
                from: Who::Speaker,
                spot: Spot::BehindRightShoulderNear,
                look: Who::Speakee
            })
        );
        assert_eq!(Shot::from_tags(&tags("CONVERSATION_ATTITUDE_ANGER")), None);
        assert_eq!(Shot::from_tags(&tags("CAM:(SOMEONE,ELSEWHERE)")), None);
    }

    #[test]
    fn the_default_looks_over_the_heros_shoulder_at_the_speaker() {
        // The speaker 2 m north of the Hero.
        let hero = Actor {
            feet: Vec3::ZERO,
            face_height: 1.1,
        };
        let speaker = Actor {
            feet: Vec3::new(0.0, 2.0, 0.0),
            face_height: 1.6,
        };
        let cam = Shot::DEFAULT.camera(speaker, hero);
        assert!((cam.target - speaker.face()).length() < 1e-4);
        let eye = cam.eye();
        assert!(eye.y < 0.0, "behind the Hero: {eye}");
        assert!(
            eye.x > 0.0,
            "to his right (facing north, right is east): {eye}"
        );
        assert!(eye.z > hero.face_height, "above his shoulder: {eye}");
    }

    #[test]
    fn blending_turns_the_short_way_round() {
        let a = OrbitCamera {
            target: Vec3::ZERO,
            yaw: 3.0,
            pitch: 0.0,
            distance: 4.0,
        };
        let b = OrbitCamera {
            yaw: -3.0,
            distance: 2.0,
            ..a
        };
        let mid = blend(&a, &b, 0.5);
        assert!(
            (mid.yaw.cos() - (-1.0)).abs() < 1e-3,
            "through pi: {}",
            mid.yaw
        );
        assert!((mid.distance - 3.0).abs() < 1e-5);
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
    }
}
