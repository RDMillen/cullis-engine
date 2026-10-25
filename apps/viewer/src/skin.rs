//! Skinned meshes for drawing: the world's animators pose rigs into bone
//! matrices (`engine_world::anim`), and this deforms a mesh's bind-pose
//! vertices with them on the CPU.

use glam::{Mat4, Vec3};

pub use engine_world::anim::{AnimBank, Clip, Rig};

use crate::model::ModelVertex;

/// What a skinned mesh needs to be posed: per vertex, 4 skeleton bone indices
/// and weights (summing to 1), and its rig.
#[derive(Clone)]
pub struct Skin {
    pub influences: Vec<([u16; 4], [f32; 4])>,
    pub rig: Rig,
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

/// A skinned mesh's bind pose and weights, posed on demand from bone matrices.
pub struct SkinnedMesh {
    bind: Vec<ModelVertex>,
    skin: Skin,
    posed: Vec<ModelVertex>,
}

impl SkinnedMesh {
    pub fn new(bind: Vec<ModelVertex>, skin: Skin) -> Self {
        Self {
            bind,
            skin,
            posed: Vec::new(),
        }
    }

    /// The vertices posed by `matrices`.
    pub fn pose(&mut self, matrices: &[Mat4]) -> &[ModelVertex] {
        deform(&self.skin, &self.bind, matrices, &mut self.posed);
        &self.posed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deform_blends_bone_matrices_by_weight() {
        let skin = Skin {
            influences: vec![([0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0])],
            rig: Rig::default(),
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
