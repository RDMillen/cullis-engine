//! Loads training dummies: each is a world entity
//! (`engine_world::combat::Dummy`, a `Target` and an animator playing its
//! `DEFAULT`, `GET_HIT`, `WOBBLE` and `GET_HIT_SPIN`), drawn by an
//! [`AnimatedModel`].

use eframe::wgpu;
use engine_world::anim::Animator;
use engine_world::combat::Target;
use engine_world::world::World;

use crate::door::AnimatedModel;
use crate::model::{self, ModelRenderer};
use crate::skin::AnimBank;
use crate::terrain::DummyPlacement;

/// How wide a dummy is to blows, in world units (by eye: its straw body).
const RADIUS: f32 = 0.35;

/// Adds every dummy to the world, standing still, and returns their meshes.
pub fn load(
    assets: &engine_core::assets::AssetCache,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    dummies: &[DummyPlacement],
    world: &mut World,
) -> Vec<AnimatedModel> {
    let mut bank = match AnimBank::open(assets) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("animations: {e}");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for d in dummies {
        let (model, Some(skin)) = (match model::skinned_model(assets, d.placement.mesh, &[]) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("dummy mesh {}: {e}", d.placement.mesh);
                continue;
            }
        }) else {
            continue;
        };
        // A missing reaction stands in with the default pose.
        let Some(default) = d.animations[0] else {
            continue;
        };
        let clips: Vec<_> = d
            .animations
            .iter()
            .map(|id| id.unwrap_or(default))
            .filter_map(|id| {
                bank.clip(id, &skin.rig.skeleton)
                    .map_err(|e| log::warn!("dummy animation {id}: {e}"))
                    .ok()
            })
            .collect();
        if clips.len() != 4 {
            continue;
        }
        let transform = d.placement.transform();
        let target = Target {
            at: d.placement.position,
            radius: RADIUS,
        };
        let animator = Animator::new(skin.rig.clone(), clips);
        let entity = world.spawn_dummy(transform, target, d.script_name.clone(), animator);
        let mut drawn = AnimatedModel::new(
            entity,
            ModelRenderer::new(device, queue, &model),
            model.vertices.clone(),
            skin,
        );
        drawn.update(queue, world);
        out.push(drawn);
    }
    out
}
