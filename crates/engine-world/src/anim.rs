//! Skeletal animation: clips matched to a skeleton, posing and blending them
//! into bone matrices, crossfades, mouth shapes over the face, and the bone
//! configs that reshape a creature's skeleton ([`Rig`]). Turning matrices into
//! posed vertices is the renderer's business.

use engine_core::assets::{Archive, AssetCache};
use fable_data::anim::{ANIMATION_TYPES, Animation, Track};
use fable_data::mesh::{Affine, Bone, Skeleton, mul_affine};
use glam::{Mat4, Quat, Vec3, Vec4};

/// What can go wrong loading an animation.
#[derive(Debug)]
pub enum Error {
    /// The archive could not be read.
    Assets(engine_core::Error),
    /// The animation did not parse.
    Data(fable_data::Error),
    /// No animation by that name or id, or one with nothing in it.
    Missing(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Assets(e) => write!(f, "{e}"),
            Error::Data(e) => write!(f, "{e}"),
            Error::Missing(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<engine_core::Error> for Error {
    fn from(e: engine_core::Error) -> Self {
        Error::Assets(e)
    }
}

impl From<fable_data::Error> for Error {
    fn from(e: fable_data::Error) -> Self {
        Error::Data(e)
    }
}

/// A skeleton and the per-bone scales its creature's bone configs give it.
#[derive(Debug, Clone, Default)]
pub struct Rig {
    pub skeleton: Skeleton,
    /// Per bone, a scale from the creature's bone configs (`Bones/*.bncfg`):
    /// along the bone and across it. Empty for none.
    pub morph: Vec<[f32; 3]>,
}

impl Rig {
    pub fn new(skeleton: Skeleton) -> Self {
        Self {
            skeleton,
            morph: Vec::new(),
        }
    }

    /// How much the morph lowers the hips: the pelvis's rest height with the morph
    /// over without (1 with no morph). Animations made for full-size skeletons
    /// cover that much less ground.
    pub fn stride_ratio(&self) -> f32 {
        let pelvis = |morph: &[[f32; 3]]| {
            let i = self
                .skeleton
                .bones
                .iter()
                .position(|b| b.name == "Bip01 Pelvis")?;
            let m = pose(&self.skeleton, None, 0.0, morph)[i];
            // The bone's origin: the inverse bind maps it from model space.
            let bind = bind_origin(&self.skeleton.bones[i]);
            Some(m.transform_point3(bind).z)
        };
        match (pelvis(&self.morph), pelvis(&[])) {
            (Some(a), Some(b)) if b.abs() > 1e-3 => a / b,
            _ => 1.0,
        }
    }

    /// Applies bone configs: each bone's scale is the product of the configs'
    /// scales for it.
    pub fn set_morph(&mut self, configs: &[fable_data::bncfg::BoneConfig]) {
        self.morph = self
            .skeleton
            .bones
            .iter()
            .map(|b| {
                configs
                    .iter()
                    .filter_map(|c| c.scale(&b.name))
                    .fold([1.0; 3], |a, s| [a[0] * s[0], a[1] * s[1], a[2] * s[2]])
            })
            .collect();
    }
}

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

/// The animations in `graphics/graphics.big`, through the asset cache.
pub struct AnimBank {
    assets: AssetCache,
}

impl AnimBank {
    pub fn open(assets: &AssetCache) -> Result<Self, Error> {
        Ok(Self {
            assets: assets.clone(),
        })
    }

    /// The bank id of the animation named `name`.
    pub fn id(&mut self, name: &str) -> Option<u32> {
        let index = self.assets.index(Archive::Meshes).ok()?;
        let entry = index.by_name(name)?;
        ANIMATION_TYPES
            .contains(&entry.entry_type)
            .then_some(entry.id)
    }

    /// Loads animation `id` for `skeleton`.
    pub fn clip(&mut self, id: u32, skeleton: &Skeleton) -> Result<Clip, Error> {
        let (entry, data) = self.assets.read(Archive::Meshes, id)?;
        if !ANIMATION_TYPES.contains(&entry.entry_type) {
            return Err(Error::Missing(format!(
                "{} is not an animation",
                entry.name
            )));
        }
        let anim = Animation::parse(&data)?;
        Clip::from_animation(entry.name, anim, skeleton)
    }
}

impl Clip {
    /// Loads animation `name` from `graphics/graphics.big` for `skeleton`.
    pub fn load(assets: &AssetCache, name: &str, skeleton: &Skeleton) -> Result<Self, Error> {
        let mut bank = AnimBank::open(assets)?;
        let id = bank
            .id(name)
            .ok_or_else(|| Error::Missing(format!("no animation {name}")))?;
        bank.clip(id, skeleton)
    }

    fn from_animation(name: String, anim: Animation, skeleton: &Skeleton) -> Result<Self, Error> {
        let movement = anim
            .movement()
            .map_or(0.0, |m| m.iter().map(|v| v * v).sum::<f32>().sqrt());
        let object = anim
            .objects
            .into_iter()
            .next()
            .ok_or_else(|| Error::Missing("animation has no object".into()))?;
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
            name,
            fps: if fps > 0.0 { fps } else { 30.0 },
            frames: frames.max(1),
            tracks,
            movement,
        })
    }

    /// When the last frame starts, in seconds.
    pub fn last_frame_time(&self) -> f32 {
        (self.frames.saturating_sub(1)) as f32 / self.fps
    }

    /// Length of one loop in seconds.
    pub fn duration(&self) -> f32 {
        self.frames as f32 / self.fps
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

/// Where a bone sits in model space in the bind pose (the inverse of its
/// inverse bind matrix, applied to the origin).
pub fn bind_origin(bone: &Bone) -> Vec3 {
    to_mat4(&bone.inverse_bind)
        .inverse()
        .transform_point3(Vec3::ZERO)
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
///
/// `morph` (per bone, may be empty) reshapes the skeleton: a bone's scale applies
/// to its own vertices (in bone space) and to its children's offsets from it, but
/// is not inherited, so a shortened thigh brings the calf up without shrinking it.
pub fn pose(skeleton: &Skeleton, clip: Option<&Clip>, time: f32, morph: &[[f32; 3]]) -> Vec<Mat4> {
    pose_blended(skeleton, clip, time, None, morph, &[], &[])
}

/// A bone's pose under `clip` at `time` (its rest pose without a clip).
fn bone_pose(bone: &Bone, i: usize, clip: Option<&Clip>, time: f32) -> Bone {
    match clip {
        Some(c) => animated_bone(bone, c.tracks[i].as_ref(), c.frames, c.fps, time),
        None => bone.clone(),
    }
}

/// Blends two poses of a bone: `k` 0 is `a`, 1 is `b` (rotations interpolated
/// along the shorter way, translations linearly).
fn blend_bones(a: &Bone, b: &Bone, k: f32) -> Bone {
    let (qa, qb) = (Quat::from_array(a.rotation), Quat::from_array(b.rotation));
    let qb = if qa.dot(qb) < 0.0 { -qb } else { qb };
    let mut out = b.clone();
    out.rotation = qa.lerp(qb, k).normalize().to_array();
    out.translation = Vec3::from_array(a.translation)
        .lerp(Vec3::from_array(b.translation), k)
        .to_array();
    out
}

/// As [`pose`], fading in from `from` (a clip, its time, and how far the fade
/// has come, 0 to 1).
/// `face` layers single-pose clips (mouth shapes) over the bones `face_mask`
/// marks, each with a weight: those bones take the weighted average of the
/// body's pose and the shapes' (the body keeps whatever weight is left).
pub fn pose_blended(
    skeleton: &Skeleton,
    clip: Option<&Clip>,
    time: f32,
    from: Option<(&Clip, f32, f32)>,
    morph: &[[f32; 3]],
    face: &[(&Clip, f32)],
    face_mask: &[bool],
) -> Vec<Mat4> {
    let mut world: Vec<Affine> = Vec::with_capacity(skeleton.bones.len());
    for (i, bone) in skeleton.bones.iter().enumerate() {
        let mut posed = bone_pose(bone, i, clip, time);
        if let Some((old, old_time, k)) = from {
            let before = bone_pose(bone, i, Some(old), old_time);
            posed = blend_bones(&before, &posed, k);
        }
        if face_mask.get(i).copied().unwrap_or(false) && !face.is_empty() {
            // A running weighted average: the body with the weight left over,
            // then each shape in turn.
            let shapes: f32 = face.iter().map(|(_, w)| w.max(0.0)).sum();
            let mut total = (1.0 - shapes).max(0.0);
            for (shape, w) in face {
                let w = w.max(0.0);
                if w <= 0.0 || shape.tracks.get(i).is_none_or(Option::is_none) {
                    continue;
                }
                total += w;
                let pose = bone_pose(bone, i, Some(shape), 0.0);
                posed = blend_bones(&posed, &pose, w / total);
            }
        }
        if let Some(s) = bone.parent.and_then(|p| morph.get(p)) {
            posed.translation = std::array::from_fn(|k| posed.translation[k] * s[k]);
        }
        let local = posed.local();
        let m = match bone.parent.and_then(|p| world.get(p)) {
            Some(parent) => mul_affine(parent, &local),
            None => local,
        };
        world.push(m);
    }
    world
        .iter()
        .zip(&skeleton.bones)
        .enumerate()
        .map(|(i, (w, b))| {
            let s = morph.get(i).copied().unwrap_or([1.0; 3]);
            let scale: Affine = [
                [s[0], 0.0, 0.0, 0.0],
                [0.0, s[1], 0.0, 0.0],
                [0.0, 0.0, s[2], 0.0],
            ];
            to_mat4(&mul_affine(&mul_affine(w, &scale), &b.inverse_bind))
        })
        .collect()
}

/// Per bone, whether it is part of the face: below `Bip01 Head` (its chin,
/// lips, cheeks, brows and eyelids), the head itself not.
pub fn face_bones(skeleton: &Skeleton) -> Vec<bool> {
    let head = skeleton
        .bones
        .iter()
        .position(|b| b.name.eq_ignore_ascii_case("Bip01 Head"));
    skeleton
        .bones
        .iter()
        .map(|b| {
            let mut parent = b.parent;
            while let Some(p) = parent {
                if Some(p) == head {
                    return true;
                }
                parent = skeleton.bones.get(p).and_then(|b| b.parent);
            }
            false
        })
        .collect()
}

/// A rig playing one of a few clips, producing its bone matrices.
pub struct Animator {
    rig: Rig,
    clips: Vec<Clip>,
    current: usize,
    time: f32,
    /// The bone matrices of the last [`Animator::advance`].
    matrices: Vec<Mat4>,
    /// The current clip plays once and holds its last frame instead of looping.
    once: bool,
    /// The clip being faded out after a switch: index, its time, whether it was
    /// held, and seconds since the switch.
    fading: Option<(usize, f32, bool, f32)>,
    /// Mouth shapes layered over the face (clip index, weight), and which bones
    /// make up the face (those below `Bip01 Head`).
    face: Vec<(usize, f32)>,
    face_mask: Vec<bool>,
}

/// How long a switch between clips crossfades, in seconds.
const FADE: f32 = 0.25;

impl Animator {
    pub fn new(rig: Rig, clips: Vec<Clip>) -> Self {
        let face_mask = face_bones(&rig.skeleton);
        Self {
            rig,
            clips,
            current: 0,
            time: 0.0,
            matrices: Vec::new(),
            once: false,
            fading: None,
            face: Vec::new(),
            face_mask,
        }
    }

    /// Layers mouth shapes over the face: (clip index, weight 0..=1); empty for
    /// none.
    pub fn set_face(&mut self, shapes: &[(usize, f32)]) {
        self.face.clear();
        self.face.extend_from_slice(shapes);
    }

    /// Whether the skeleton has face bones to move.
    pub fn has_face(&self) -> bool {
        self.face_mask.iter().any(|&f| f)
    }

    /// Adds a clip, returning its index.
    pub fn add_clip(&mut self, clip: Clip) -> usize {
        self.clips.push(clip);
        self.clips.len() - 1
    }

    pub fn skeleton(&self) -> &fable_data::mesh::Skeleton {
        &self.rig.skeleton
    }

    /// Switches to clip `i` (restarting it) unless it is already playing.
    pub fn play(&mut self, i: usize) {
        if i != self.current && i < self.clips.len() {
            self.fading = Some((self.current, self.time, self.once, 0.0));
            self.current = i;
            self.time = 0.0;
            self.once = false;
        }
    }

    /// Plays clip `i` once from `time` seconds (restarting it even if it is
    /// playing) and then holds its last frame: a door swinging open.
    pub fn play_once(&mut self, i: usize, time: f32) {
        if i < self.clips.len() {
            // Starting partway (a door's resting pose) jumps; from the start fades.
            self.fading = (time == 0.0).then_some((self.current, self.time, self.once, 0.0));
            self.current = i;
            self.time = time;
            self.once = true;
        }
    }

    /// Speed clip `i` is made for, in mesh units per second.
    pub fn speed(&self, i: usize) -> Option<f32> {
        self.clips.get(i).map(Clip::speed).filter(|&s| s > 0.0)
    }

    /// How much shorter the morph makes the creature's stride (see
    /// [`Rig::stride_ratio`]).
    pub fn stride_ratio(&self) -> f32 {
        self.rig.stride_ratio()
    }

    /// Index of the clip playing.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Whether the current clip has played through once (a clip played once
    /// holds at its last frame, so it is through on reaching it).
    pub fn finished(&self) -> bool {
        self.clips.get(self.current).is_none_or(|c| {
            let end = if self.once {
                c.last_frame_time()
            } else {
                c.duration()
            };
            self.time >= end
        })
    }

    /// Number of clips.
    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    pub fn clip_name(&self) -> Option<&str> {
        self.clips.get(self.current).map(|c| c.name.as_str())
    }

    /// The bone matrices of the last [`Animator::advance`].
    pub fn matrices(&self) -> &[Mat4] {
        &self.matrices
    }

    /// Advances the current clip by `dt` seconds and returns the bone matrices.
    pub fn advance(&mut self, dt: f32) -> &[Mat4] {
        let hold = |clips: &[Clip], i: usize, once: bool, t: f32| match clips.get(i) {
            // The last frame, not the wrap back to the first.
            Some(c) if once => t.min(c.last_frame_time()),
            _ => t,
        };
        self.time = hold(&self.clips, self.current, self.once, self.time + dt);
        if let Some((i, t, once, since)) = self.fading {
            let since = since + dt;
            self.fading =
                (since < FADE).then(|| (i, hold(&self.clips, i, once, t + dt), once, since));
        }
        let from = self.fading.and_then(|(i, t, _, since)| {
            let k = since / FADE;
            Some((self.clips.get(i)?, t, k * k * (3.0 - 2.0 * k)))
        });
        let face: Vec<(&Clip, f32)> = self
            .face
            .iter()
            .filter_map(|&(i, w)| Some((self.clips.get(i)?, w)))
            .collect();
        self.matrices = pose_blended(
            &self.rig.skeleton,
            self.clips.get(self.current),
            self.time,
            from,
            &self.rig.morph,
            &face,
            &self.face_mask,
        );
        &self.matrices
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
            bones: vec![bone(None, [0.0; 3])],
            ..Default::default()
        };
        let m = pose(&skeleton, None, 0.0, &[]);
        assert!(m[0].abs_diff_eq(Mat4::IDENTITY, 1e-6));
    }

    #[test]
    fn morph_moves_children_without_inheriting() {
        // A root at the origin and a child 10 along x; halving the root's length
        // brings the child to 5, while the child keeps its own scale (1).
        let skeleton = Skeleton {
            bones: vec![bone(None, [0.0; 3]), bone(Some(0), [10.0, 0.0, 0.0])],
            ..Default::default()
        };
        let m = pose(&skeleton, None, 0.0, &[[0.5, 1.0, 1.0], [1.0; 3]]);
        // The child's inverse bind is identity here, so its matrix maps the
        // origin to the child's position, and keeps unit length along x.
        assert!(
            m[1].transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(5.0, 0.0, 0.0), 1e-5)
        );
        assert!(m[1].transform_vector3(Vec3::X).abs_diff_eq(Vec3::X, 1e-5));
        // The root's own vertices shrink along x.
        assert!(
            m[0].transform_point3(Vec3::X)
                .abs_diff_eq(Vec3::new(0.5, 0.0, 0.0), 1e-5)
        );
    }

    #[test]
    fn blending_two_poses_meets_halfway() {
        let a = bone(None, [0.0; 3]);
        let mut b = bone(None, [2.0, 0.0, 0.0]);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        b.rotation = [0.0, 0.0, h, h]; // 90 degrees about z
        let mid = blend_bones(&a, &b, 0.5);
        assert_eq!(mid.translation, [1.0, 0.0, 0.0]);
        let angle = Quat::from_array(mid.rotation).to_axis_angle().1;
        assert!(
            (angle - std::f32::consts::FRAC_PI_4).abs() < 1e-3,
            "{angle}"
        );
    }

    #[test]
    fn clips_played_once_finish_on_their_last_frame() {
        let clip = |name: &str| Clip {
            name: name.into(),
            fps: 10.0,
            frames: 5,
            tracks: Vec::new(),
            movement: 0.0,
        };
        let mut a = Animator::new(Rig::default(), vec![clip("idle"), clip("kick")]);
        a.advance(0.45);
        assert!(!a.finished(), "a loop is through after its full length");
        a.advance(0.1);
        assert!(a.finished());
        a.play_once(1, 0.0);
        a.advance(0.3);
        assert!(!a.finished());
        a.advance(10.0);
        assert!(a.finished(), "held at the last frame, the kick is over");
    }

    #[test]
    fn the_face_is_what_hangs_below_the_head() {
        let named = |name: &str, parent: Option<usize>| Bone {
            name: name.into(),
            ..bone(parent, [0.0; 3])
        };
        let skeleton = Skeleton {
            bones: vec![
                named("Bip01", None),
                named("Bip01 Head", Some(0)),
                named("Chin", Some(1)),
                named("Chin_tip", Some(2)),
                named("Bip01 L Hand", Some(0)),
            ],
            ..Default::default()
        };
        assert_eq!(face_bones(&skeleton), [false, false, true, true, false]);
    }
}
