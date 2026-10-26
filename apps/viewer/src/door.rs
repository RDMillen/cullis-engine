//! Draws doors: each door is a world entity (`engine_world::door::Door` and its
//! animator); this holds its skinned mesh and deforms it with the animator's
//! pose each frame.

use eframe::wgpu;
use engine_world::anim::Animator;
use engine_world::door::Door;
use engine_world::world::World;

pub use engine_render::door::DoorModel;

use crate::model::{self, ModelRenderer};
use crate::skin::AnimBank;
use crate::terrain::DoorPlacement;

/// Adds every door to the world, posed open or closed as its level file says,
/// and returns their meshes.
pub fn load(
    assets: &engine_core::assets::AssetCache,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    doors: &[DoorPlacement],
    world: &mut World,
) -> Vec<DoorModel> {
    let mut bank = match AnimBank::open(assets) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("animations: {e}");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for d in doors {
        let [Some(open), Some(close)] = d.animations else {
            continue;
        };
        let (model, Some(skin)) = (match model::skinned_model(assets, d.placement.mesh, &[]) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("door mesh {}: {e}", d.placement.mesh);
                continue;
            }
        }) else {
            continue;
        };
        let clips: Vec<_> = [open, close]
            .iter()
            .filter_map(|&id| {
                bank.clip(id, &skin.rig.skeleton)
                    .map_err(|e| log::warn!("door animation {id}: {e}"))
                    .ok()
            })
            .collect();
        if clips.len() != 2 {
            continue;
        }
        let door = Door::new(
            d.placement.transform(),
            (model.lo.x, model.hi.x),
            1.0 / model::MESH_SCALE,
            d.open,
        );
        let entity = world.spawn_door(door, Animator::new(skin.rig.clone(), clips));
        let mut drawn = DoorModel::new(
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
