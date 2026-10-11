//! Plays animations on a skinned mesh: samples a clip into bone matrices and
//! deforms the bind-pose vertices on the CPU.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use fable_data::anim::{ANIMATION_TYPES, Animation, Track};
use fable_data::big::BigFile;
use fable_data::mesh::{Affine, Bone, Skeleton, mul_affine};
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::model::{ModelVertex, Skin};

type Error = Box<dyn std::error::Error>;

/// An animation's tracks matched to a skeleton's bones by name.
#[derive(Clone)]
pub struct Clip {
    pub name: String,
    fps: f32,
    frames: u32,
    /// Per skeleton bone, its track, if the animation moves it.
    tracks: Vec<Option<Track>>,
    /// Distance carried per loop in mesh units (see `Animation::movement`).
    movement: f32,
}

impl Clip {
    /// Loads animation `name` from `graphics/graphics.big` for `skeleton`.
    pub fn load(data_dir: &Path, name: &str, skeleton: &Skeleton) -> Result<Self, Error> {
        let mut file = BufReader::new(File::open(data_dir.join("graphics/graphics.big"))?);
        let big = BigFile::read(&mut file)?;
        let index = big
            .bank("MBANK_ALLMESHES")
            .ok_or("no MBANK_ALLMESHES")?
            .read_index(&mut file)?;
        let entry = index
            .entries
            .iter()
            .find(|e| e.name == name && ANIMATION_TYPES.contains(&e.entry_type))
            .ok_or_else(|| format!("no animation {name}"))?;
        let anim = Animation::parse(&entry.read_data(&mut file)?)?;
        let movement = anim
            .movement()
            .map_or(0.0, |m| m.iter().map(|v| v * v).sum::<f32>().sqrt());
        let object = anim
            .objects
            .into_iter()
            .next()
            .ok_or("animation has no object")?;
        let (fps, frames) = object
            .tracks
            .iter()
            .map(|t| (t.fps, t.frames))
            .max_by_key(|&(_, f)| f)
            .unwrap_or((30.0, 1));
        let mut tracks: Vec<Option<Track>> = vec![None; skeleton.bones.len()];
        for track in object.tracks {
            if let Some(i) = skeleton
                .bones
                .iter()
                .position(|b| b.name.eq_ignore_ascii_case(&track.name))
            {
                tracks[i] = Some(track);
            }
        }
        Ok(Self {
            name: name.to_owned(),
            fps: if fps > 0.0 { fps } else { 30.0 },
            frames: frames.max(1),
            tracks,
            movement,
        })
    }

    /// Speed the clip is made for, in mesh units per second (0 in place).
    pub fn speed(&self) -> f32 {
        self.movement * self.fps / self.frames as f32
    }
}

/// A bone's rest pose with the clip's keys at `time` (looping) substituted.
/// Positions are directions scaled by the bone's length, the largest component of
/// its rest translation; frames are blended linearly.
fn animated_bone(bone: &Bone, track: Option<&Track>, frames: u32, fps: f32, time: f32) -> Bone {
    let Some(track) = track else {
        return bone.clone();
    };
    let t = (time * fps).rem_euclid(frames as f32);
    let (f0, k) = (t.floor() as u32, t.fract());
    let f1 = (f0 + 1) % frames;
    let mut out = bone.clone();
    if let (Some(a), Some(b)) = (track.rotation(f0), track.rotation(f1)) {
        let (a, b) = (Quat::from_array(a), Quat::from_array(b));
        let b = if a.dot(b) < 0.0 { -b } else { b };
        out.rotation = a.lerp(b, k).normalize().to_array();
    }
    if let (Some(a), Some(b)) = (track.position(f0), track.position(f1)) {
        let length = bone
            .translation
            .iter()
            .fold(0.0f32, |m, v| if v.abs() > m.abs() { *v } else { m })
            .abs();
        let p = Vec3::from_array(a).lerp(Vec3::from_array(b), k) * length;
        out.translation = p.to_array();
    }
    out
}

fn to_mat4(m: &Affine) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(m[0][0], m[1][0], m[2][0], 0.0),
        Vec4::new(m[0][1], m[1][1], m[2][1], 0.0),
        Vec4::new(m[0][2], m[1][2], m[2][2], 0.0),
        Vec4::new(m[0][3], m[1][3], m[2][3], 1.0),
    )
}

/// Per bone, the matrix taking bind-pose model space to posed model space.
pub fn pose(skeleton: &Skeleton, clip: Option<&Clip>, time: f32) -> Vec<Mat4> {
    let mut world: Vec<Affine> = Vec::with_capacity(skeleton.bones.len());
    for (i, bone) in skeleton.bones.iter().enumerate() {
        let local = match clip {
            Some(c) => animated_bone(bone, c.tracks[i].as_ref(), c.frames, c.fps, time).local(),
            None => bone.local(),
        };
        let m = match bone.parent.and_then(|p| world.get(p)) {
            Some(parent) => mul_affine(parent, &local),
            None => local,
        };
        world.push(m);
    }
    world
        .iter()
        .zip(&skeleton.bones)
        .map(|(w, b)| to_mat4(&mul_affine(w, &b.inverse_bind)))
        .collect()
}

/// Writes the posed `bind` vertices into `out` (resized to match).
pub fn deform(skin: &Skin, bind: &[ModelVertex], matrices: &[Mat4], out: &mut Vec<ModelVertex>) {
    out.clear();
    out.extend(
        bind.iter()
            .zip(&skin.influences)
            .map(|(v, (bones, weights))| {
                let mut m = Mat4::ZERO;
                for (&b, &w) in bones.iter().zip(weights) {
                    if w > 0.0
                        && let Some(bm) = matrices.get(usize::from(b))
                    {
                        m += *bm * w;
                    }
                }
                ModelVertex {
                    position: m.transform_point3(Vec3::from_array(v.position)).to_array(),
                    normal: m
                        .transform_vector3(Vec3::from_array(v.normal))
                        .normalize_or_zero()
                        .to_array(),
                    uv: v.uv,
                }
            }),
    );
}

/// A skinned model playing one of a few clips.
pub struct Animator {
    bind: Vec<ModelVertex>,
    skin: Skin,
    clips: Vec<Clip>,
    current: usize,
    time: f32,
    posed: Vec<ModelVertex>,
}

impl Animator {
    pub fn new(bind: Vec<ModelVertex>, skin: Skin, clips: Vec<Clip>) -> Self {
        Self {
            bind,
            skin,
            clips,
            current: 0,
            time: 0.0,
            posed: Vec::new(),
        }
    }

    /// Adds a clip, returning its index.
    pub fn add_clip(&mut self, clip: Clip) -> usize {
        self.clips.push(clip);
        self.clips.len() - 1
    }

    pub fn skeleton(&self) -> &fable_data::mesh::Skeleton {
        &self.skin.skeleton
    }

    /// Switches to clip `i` (restarting it) unless it is already playing.
    pub fn play(&mut self, i: usize) {
        if i != self.current && i < self.clips.len() {
            self.current = i;
            self.time = 0.0;
        }
    }

    /// Speed clip `i` is made for, in mesh units per second.
    pub fn speed(&self, i: usize) -> Option<f32> {
        self.clips.get(i).map(Clip::speed).filter(|&s| s > 0.0)
    }

    pub fn clip_name(&self) -> Option<&str> {
        self.clips.get(self.current).map(|c| c.name.as_str())
    }

    /// Advances the current clip by `dt` seconds and returns the posed vertices.
    pub fn advance(&mut self, dt: f32) -> &[ModelVertex] {
        self.time += dt;
        let matrices = pose(&self.skin.skeleton, self.clips.get(self.current), self.time);
        deform(&self.skin, &self.bind, &matrices, &mut self.posed);
        &self.posed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bone(parent: Option<usize>, translation: [f32; 3]) -> Bone {
        Bone {
            name: String::new(),
            parent,
            inverse_bind: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
            rotation: [0.0, 0.0, 0.0, 1.0],
            translation,
            scale: [1.0; 3],
        }
    }

    #[test]
    fn rest_pose_without_a_clip_leaves_the_bind_pose() {
        // A root at the origin whose inverse bind undoes nothing: identity skinning.
        let skeleton = Skeleton {
            helper_count: 0,
            dummy_count: 0,
            bones: vec![bone(None, [0.0; 3])],
        };
        let m = pose(&skeleton, None, 0.0);
        assert!(m[0].abs_diff_eq(Mat4::IDENTITY, 1e-6));
    }

    #[test]
    fn deform_blends_bone_matrices_by_weight() {
        let skin = Skin {
            influences: vec![([0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0])],
            skeleton: Skeleton {
                helper_count: 0,
                dummy_count: 0,
                bones: Vec::new(),
            },
        };
        let bind = [ModelVertex {
            position: [1.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0; 2],
        }];
        let matrices = [
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0)),
        ];
        let mut out = Vec::new();
        deform(&skin, &bind, &matrices, &mut out);
        assert_eq!(out[0].position, [1.0, 1.0, 0.0]);
        assert_eq!(out[0].normal, [0.0, 0.0, 1.0]);
    }
}
