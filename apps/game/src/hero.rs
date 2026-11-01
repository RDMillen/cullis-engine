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
    // His fighting clips, after the fidgets, and his weapon's.
    world.hero_combat = Default::default();
    if let Some(animator) = &mut world.hero_animator {
        world.hero_combat = combat_clips(&mut bank, animator, animations.stance);
        world.hero_combat.weapon = weapon_clips(&mut bank, animator);
        renderer.hero_weapon = engine_render::weapon::HeroWeapon::load(
            assets,
            device,
            queue,
            animator.skeleton(),
            HERO_STICK_MESH,
        )
        .map_err(|e| log::warn!("the stick: {e}"))
        .ok();
    }
    Ok(Some(hero))
}

/// The Hero's clips by name, used when his animation table lacks a slot:
/// standing, walking, running (indices used by [`animate_hero`]).
pub const HERO_CLIPS: [&str; 3] = ["ANIM_HERO_IDLE", "ANIM_HERO_WALK", "ANIM_HERO_RUN"];

/// The boy Hero's chains of unarmed strikes (his `STRIKE_MEDIUM_FRONT`
/// animations with the `STAND` modifier include these two runs of three; that
/// they chain in this order is a guess from their names).
pub const HERO_CHAINS: [[&str; 3]; 2] = [
    [
        "ANIM_HERO_UNARMED_STRIKE_RIGHT_2_1",
        "ANIM_HERO_UNARMED_STRIKE_RIGHT_2_2",
        "ANIM_HERO_UNARMED_STRIKE_RIGHT_2_3",
    ],
    [
        "ANIM_HERO_UNARMED_STRIKE_LEFT_2_1",
        "ANIM_HERO_UNARMED_STRIKE_LEFT_2_2",
        "ANIM_HERO_UNARMED_STRIKE_LEFT_2_3",
    ],
];

/// The childhood stick's mesh (`OBJECT_HERO_STICK`'s).
pub const HERO_STICK_MESH: &str = "MESH_HERO_STICK";

/// The boy Hero's chains of strikes with the stick: his `STRIKE_MEDIUM_FRONT`
/// animations with the `SWORD` modifier, right and left swings in turn. Every
/// one ends in his combat stance, so their order is not in the clips; this one
/// is a guess from their names.
pub const ARMED_CHAINS: [[&str; 4]; 2] = [
    [
        "ANIM_HERO_STRIKE_RIGHT_1",
        "ANIM_HERO_STRIKE_LEFT_2",
        "ANIM_HERO_STRIKE_RIGHT_3",
        "ANIM_HERO_STRIKE_LEFT_4",
    ],
    [
        "ANIM_HERO_STRIKE_LEFT_1",
        "ANIM_HERO_STRIKE_RIGHT_2",
        "ANIM_HERO_STRIKE_LEFT_3",
        "ANIM_HERO_STRIKE_RIGHT_4",
    ],
];

/// Adds the Hero's clips for his stick: drawing and putting it away (passing
/// it where his right hand comes nearest the scabbard socket), his armed
/// chains and how he stands, walks and runs with it drawn.
fn weapon_clips(
    bank: &mut skin::AnimBank,
    animator: &mut engine_world::anim::Animator,
) -> engine_world::weapon::WeaponClips {
    use engine_world::weapon::{WeaponClips, nearest_time};
    let scabbard = animator
        .skeleton()
        .named_dummy("WEAPON_SCABBARD_01")
        .map(|d| glam::Vec3::from_array(d.position));
    let mut passing = |name: &str, animator: &mut engine_world::anim::Animator| {
        let clip = clip_named(bank, name, animator)?;
        let at = scabbard.map_or(clip.last_frame_time() / 2.0, |p| {
            nearest_time(animator.skeleton(), &clip, "Bip01 R Hand", p)
        });
        log::debug!(
            "{name}: the stick passes at {at:.2} s of {:.2}",
            clip.duration()
        );
        Some((animator.add_clip(clip), at))
    };
    let draw = passing("ANIM_HERO_UNSHEATHE_STICK", animator);
    let sheathe = passing("ANIM_HERO_SHEATHE_STICK", animator);
    let mut plain = |name: &str| {
        let clip = clip_named(bank, name, animator)?;
        Some(animator.add_clip(clip))
    };
    let (idle, walk, run) = (
        plain("ANIM_HERO_COMBAT_IDLE_1_1"),
        plain("ANIM_HERO_WALK_WITH_SWORD"),
        plain("ANIM_HERO_RUN_WITH_SWORD"),
    );
    let mut chains = Vec::new();
    for names in ARMED_CHAINS {
        let mut chain = Vec::new();
        for name in names {
            let Some(clip) = clip_named(bank, name, animator) else {
                break;
            };
            let clip = clip.with_root_motion();
            let contact = engine_world::combat::contact_time(animator.skeleton(), &clip, HANDS);
            log::debug!(
                "strike {name}: blow at {contact:.2} s of {:.2}",
                clip.duration()
            );
            chain.push(engine_world::combat::Strike {
                clip: animator.add_clip(clip),
                contact,
            });
        }
        if !chain.is_empty() {
            chains.push(chain);
        }
    }
    WeaponClips {
        draw,
        sheathe,
        chains,
        idle,
        walk,
        run,
    }
}

/// The animation named `name` for `animator`'s skeleton, if it loads.
fn clip_named(
    bank: &mut skin::AnimBank,
    name: &str,
    animator: &engine_world::anim::Animator,
) -> Option<engine_world::anim::Clip> {
    let id = bank.id(name)?;
    bank.clip(id, animator.skeleton())
        .map_err(|e| log::warn!("{name}: {e}"))
        .ok()
}

/// The bones his punches land with.
const HANDS: &[&str] = &["Bip01 L Hand", "Bip01 R Hand"];

/// Adds the Hero's strike chains (with when each blow lands) and his stance to
/// his animator.
fn combat_clips(
    bank: &mut skin::AnimBank,
    animator: &mut engine_world::anim::Animator,
    stance: Option<[u32; 5]>,
) -> engine_world::combat::HeroCombat {
    let first_clip = Some(animator.clip_count());
    let load = |bank: &mut skin::AnimBank, id: u32, animator: &engine_world::anim::Animator| {
        bank.clip(id, animator.skeleton())
            .map_err(|e| log::warn!("animation {id}: {e}"))
            .ok()
    };
    let mut chains = Vec::new();
    for names in HERO_CHAINS {
        let mut chain = Vec::new();
        for name in names {
            let Some(clip) = bank.id(name).and_then(|id| load(bank, id, animator)) else {
                log::warn!("strike {name} missing");
                break;
            };
            // Strikes lunge: the clip carries him forward.
            let clip = clip.with_root_motion();
            let contact = engine_world::combat::contact_time(animator.skeleton(), &clip, HANDS);
            log::debug!(
                "strike {name}: blow at {contact:.2} s of {:.2}",
                clip.duration()
            );
            chain.push(engine_world::combat::Strike {
                clip: animator.add_clip(clip),
                contact,
            });
        }
        if !chain.is_empty() {
            chains.push(chain);
        }
    }
    let stance = stance.and_then(|ids| {
        let clips: Vec<usize> = ids
            .iter()
            .filter_map(|&id| load(bank, id, animator).map(|c| animator.add_clip(c)))
            .collect();
        clips.try_into().ok()
    });
    engine_world::combat::HeroCombat {
        chains,
        stance,
        first_clip,
        weapon: Default::default(),
    }
}

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
