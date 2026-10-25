//! Doors: skinned meshes posed by their `DOOR_OPEN` and `DOOR_CLOSE` animations,
//! blocking their doorway while closed.

use eframe::wgpu;
use glam::{Mat4, Vec2, Vec3};

use crate::model::{self, ModelRenderer};
use crate::skin::{AnimBank, Animator};
use crate::terrain::DoorPlacement;

/// How far in front of and behind a closed door it blocks, in world units.
const HALF_DEPTH: f32 = 0.25;
/// How near the Hero must be to use a door.
pub const REACH: f32 = 2.0;

pub struct Door {
    pub renderer: ModelRenderer,
    pub transform: Mat4,
    animator: Animator,
    /// World to the door's mesh space, for the blocking test.
    inverse: Mat4,
    /// The mesh's extent across the doorway (its local x), in mesh units.
    width: (f32, f32),
    pub open: bool,
}

impl Door {
    pub fn centre(&self) -> Vec2 {
        self.transform.w_axis.truncate().truncate()
    }

    /// Whether a closed door is in the way at `p`.
    pub fn blocks(&self, p: Vec2) -> bool {
        if self.open {
            return false;
        }
        let z = self.transform.w_axis.z;
        let local = self.inverse.transform_point3(p.extend(z));
        let depth = HALF_DEPTH / model::MESH_SCALE;
        local.x >= self.width.0 && local.x <= self.width.1 && local.y.abs() <= depth
    }

    /// Opens a closed door or closes an open one, animating it.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.animator.play_once(if self.open { 0 } else { 1 }, 0.0);
    }

    pub fn animate(&mut self, queue: &wgpu::Queue, dt: f32) {
        self.renderer.set_vertices(queue, self.animator.advance(dt));
    }
}

/// Loads every door, posed open or closed as its level file says.
pub fn load(
    assets: &engine_core::assets::AssetCache,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    doors: &[DoorPlacement],
) -> Vec<Door> {
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
                bank.clip(id, &skin.skeleton)
                    .map_err(|e| log::warn!("door animation {id}: {e}"))
                    .ok()
            })
            .collect();
        if clips.len() != 2 {
            continue;
        }
        let mut animator = Animator::new(model.vertices.clone(), skin, clips);
        // Hold the end of the opening animation (open) or of the closing one
        // (closed), so nothing moves until the door is used.
        animator.play_once(if d.open { 0 } else { 1 }, f32::MAX);
        let renderer = ModelRenderer::new(device, queue, &model);
        renderer.set_vertices(queue, animator.advance(0.0));
        let transform = d.placement.transform();
        out.push(Door {
            renderer,
            transform,
            animator,
            inverse: transform.inverse(),
            width: (model.lo.x, model.hi.x),
            open: d.open,
        });
    }
    out
}

/// The door nearest `p` within [`REACH`], if any.
pub fn nearest(doors: &[Door], p: Vec3) -> Option<usize> {
    doors
        .iter()
        .enumerate()
        .map(|(i, d)| (i, d.centre().distance(p.truncate())))
        .filter(|&(i, dist)| dist < REACH && (doors[i].transform.w_axis.z - p.z).abs() < 3.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}
