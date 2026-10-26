//! The Hero in the app: placing him and keeping the camera on him.

use crate::*;

/// Places the young Hero on the walkable cell nearest `near`.
pub fn spawn_hero(
    renderer: &mut TerrainRenderer,
    world: &mut World,
    assets: &AssetCache,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    near: glam::Vec2,
    height: Option<f32>,
) -> Result<Option<walk::Hero>, Error> {
    let Some(heights) = &world.heights else {
        return Ok(None);
    };
    let Some((layer, at, z)) = walk::stand_at(&world.walk, heights, near, height) else {
        return Ok(None);
    };
    let mut hero = walk::Hero::new(at, heights);
    hero.position.z = z;
    hero.layer = layer;
    let (model, skin) = model::hero_model(assets)?;
    renderer.hero = Some((
        ModelRenderer::new(device, queue, &model),
        hero_transform(&hero),
    ));
    let mut bank = skin::AnimBank::open(assets)?;
    let animations = world.hero_animations.clone();
    let configs = model::bone_configs(assets.data_dir(), &world.hero_morphs);
    world.hero_animator = skin.and_then(|mut skin| {
        skin.rig.set_morph(&configs);
        // Idle, walk and run from his table (or by name), then his fidgets.
        let ids: Vec<u32> = animations
            .main
            .iter()
            .zip(HERO_CLIPS)
            .filter_map(|(id, name)| id.or_else(|| bank.id(name)))
            .collect();
        if ids.len() != HERO_CLIPS.len() {
            return None;
        }
        let clips: Vec<skin::Clip> = ids
            .iter()
            .chain(&animations.fidgets)
            .filter_map(|&id| {
                bank.clip(id, &skin.rig.skeleton)
                    .map_err(|e| log::warn!("animation {id}: {e}"))
                    .ok()
            })
            .collect();
        if clips.len() < HERO_CLIPS.len() {
            return None;
        }
        let animator = engine_world::anim::Animator::new(skin.rig.clone(), clips);
        renderer.hero_mesh = Some(skin::SkinnedMesh::new(model.vertices.clone(), skin));
        Some(animator)
    });
    // His kick, after the fidgets.
    world.hero_kick = None;
    if let Some(animator) = &mut world.hero_animator
        && let Some(id) = bank.id(HERO_KICK)
    {
        match bank.clip(id, animator.skeleton()) {
            Ok(clip) => world.hero_kick = Some(animator.add_clip(clip)),
            Err(e) => log::warn!("kick: {e}"),
        }
    }
    Ok(Some(hero))
}

/// The Hero's clips by name, used when his animation table lacks a slot:
/// standing, walking, running (indices used by [`animate_hero`]).
pub const HERO_CLIPS: [&str; 3] = ["ANIM_HERO_IDLE", "ANIM_HERO_WALK", "ANIM_HERO_RUN"];

/// The boy Hero's kick (one of his `STRIKE_MEDIUM_FRONT` animations).
pub const HERO_KICK: &str = "ANIM_HERO_UNARMED_KICK_RIGHT_2_1";

/// The young Hero's definition, whose animation table the Hero plays.
pub const HERO_DEFINITION: &str = "CREATURE_HERO_CHILD";

/// Poses the Hero with clip `clip` advanced by `dt` and uploads his vertices.
pub fn animate_hero(world: &mut World, clip: usize, dt: f32) {
    if let Some(animator) = &mut world.hero_animator {
        animator.play(clip);
        animator.advance(dt);
    }
}

pub fn hero_transform(hero: &walk::Hero) -> glam::Mat4 {
    glam::Mat4::from_translation(hero.position)
        * glam::Mat4::from_rotation_z(hero.heading)
        * model::hero_local()
}

/// Points the camera at the Hero's chest from `distance`, or closer when a placed
/// mesh or the ground is in the way. The camera is treated as having some width:
/// besides the ray to it, rays to points around it are cast, so it does not end up
/// pressed against (or poking into) a wall or a cart.
pub fn follow(
    camera: &mut OrbitCamera,
    hero: &walk::Hero,
    renderer: &TerrainRenderer,
    world: &World,
    distance: f32,
) {
    /// Gap kept between the camera and what it would hit.
    const MARGIN: f32 = 0.3;
    /// Half the width of the camera's probe.
    const RADIUS: f32 = 0.5;
    camera.target = hero.position + glam::Vec3::Z;
    camera.pitch = camera.pitch.clamp(0.15, 0.8);
    camera.distance = distance;
    let back = (camera.eye() - camera.target).normalize_or_zero();
    let right = back.cross(glam::Vec3::Z).normalize_or(glam::Vec3::X);
    let up = right.cross(back);
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let offsets = [
        glam::Vec3::ZERO,
        right,
        -right,
        up,
        -up,
        (right + up) * d,
        (right - up) * d,
        (up - right) * d,
        (-right - up) * d,
    ];
    let mut nearest = distance;
    let skip = renderer.hidden_triangles();
    for offset in offsets {
        let to = camera.target + back * (distance + MARGIN) + offset * RADIUS;
        let (dir, len) = (
            (to - camera.target).normalize_or_zero(),
            (to - camera.target).length(),
        );
        let hits = [
            world
                .colliders
                .as_ref()
                .and_then(|c| c.ray(camera.target, dir, len, skip)),
            world
                .heights
                .as_ref()
                .and_then(|h| collide::ground_ray(h, camera.target, dir, len, MARGIN)),
        ];
        for hit in hits.into_iter().flatten() {
            // Distance along the camera's own line at which this ray hit.
            nearest = nearest.min(hit * dir.dot(back) - MARGIN);
        }
    }
    camera.distance = nearest.clamp(0.4, distance);
}
