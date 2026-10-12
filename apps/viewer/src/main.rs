#![recursion_limit = "256"]
//! CullisEngine desktop viewer: a region's terrain from the Lev heightmaps, or a
//! static mesh with its textures.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--nav] [--screenshot OUT.png] [--size WxH] [--camera x,y,z,distance,yaw,pitch]
//! ```

mod camera;
mod collide;
mod door;
mod model;
mod npc;
mod render;
mod skin;
mod story;
mod talk;
mod terrain;
mod themes;
mod walk;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use fable_data::big::BigFile;
use fable_data::defs::{Defs, Names};
use fable_data::gtg::{Gtg, split_connected_uid};
use fable_data::mesh::{SKINNED_MESH_TYPE, STATIC_MESH_TYPE};
use fable_data::wld::Wld;
use glam::Mat4;

use camera::OrbitCamera;
use model::ModelRenderer;
use render::{Target, TerrainRenderer};

type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subject {
    Region(String),
    Mesh(String),
}

struct Args {
    subject: Subject,
    /// Screenshot camera override: target x, y, z, distance, yaw, pitch.
    camera: Option<[f32; 6]>,
    /// Draw the navigation overlay.
    nav: bool,
    /// Print ledges in the walk grid near the camera target (diagnostic).
    ledges: bool,
    /// Place the young Hero near the camera target (screenshots).
    walk: bool,
    /// Then walk him towards this point (x, y) before the screenshot.
    walk_to: Option<[f32; 2]>,
    /// Tng sections (quest states) to show besides `NULL`; repeatable. Default:
    /// the region's starting state (`terrain::default_sections`).
    sections: Vec<String>,
    /// Pose the Hero with this animation (default: idle, or walk after
    /// `--walk-to`) at `anim_time` seconds.
    anim: Option<String>,
    anim_time: f32,
    /// Advance the creatures this many seconds before the screenshot.
    npc_time: f32,
    /// After walking (`--walk-to`), use the nearest door, then walk again.
    use_door: bool,
    /// After walking, talk to the nearest creature this many times, printing
    /// what it says.
    talk: u32,
    /// Start the Prologue quest with Theresa's present bought (for testing the
    /// ending).
    with_present: bool,
    screenshot: Option<PathBuf>,
    size: [u32; 2],
}

fn parse_args() -> Result<Args, Error> {
    let mut args = Args {
        subject: Subject::Region("StartOakVale".into()),
        screenshot: None,
        camera: None,
        nav: false,
        ledges: false,
        walk: false,
        walk_to: None,
        sections: Vec::new(),
        anim: None,
        anim_time: 0.0,
        npc_time: 0.0,
        use_door: false,
        talk: 0,
        with_present: false,
        size: [1280, 800],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--region" => args.subject = Subject::Region(value()?),
            "--mesh" => args.subject = Subject::Mesh(value()?),
            "--screenshot" => args.screenshot = Some(value()?.into()),
            "--nav" => args.nav = true,
            "--ledges" => args.ledges = true,
            "--walk" => args.walk = true,
            "--section" => args.sections.push(value()?),
            "--anim" => {
                args.anim = Some(value()?);
                args.walk = true;
            }
            "--anim-time" => args.anim_time = value()?.parse()?,
            "--npc-time" => args.npc_time = value()?.parse()?,
            "--use" => args.use_door = true,
            "--talk" => args.talk = value()?.parse()?,
            "--with-present" => args.with_present = true,
            "--walk-to" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()?;
                args.walk_to = Some(v.try_into().map_err(|_| "--walk-to takes x,y")?);
                args.walk = true;
            }
            "--camera" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()?;
                args.camera = Some(
                    v.try_into()
                        .map_err(|_| "--camera takes x,y,z,distance,yaw,pitch")?,
                );
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size takes WxH")?;
                args.size = [w.parse()?, h.parse()?];
            }
            _ => return Err(format!("unknown argument {arg:?}").into()),
        }
    }
    Ok(args)
}

/// What is on screen: a region's terrain or a single mesh.
enum Scene {
    Terrain(Box<TerrainRenderer>),
    Model(Box<ModelRenderer>),
}

impl Scene {
    fn load(
        data_dir: &Path,
        wld: &Wld,
        subject: &Subject,
        sections: &[String],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(Self, OrbitCamera, String), Error> {
        match subject {
            Subject::Region(name) => {
                let names = Names::parse(&std::fs::read(data_dir.join("CompiledDefs/names.bin"))?)?;
                let defs = Defs::parse(
                    &std::fs::read(data_dir.join("CompiledDefs/game.bin"))?,
                    &names,
                )?;
                let (mut mesh, placements) =
                    terrain::load_region(data_dir, wld, name, &defs, sections)?;
                for c in &mut mesh.creatures {
                    if let Some(def) = defs.get(&c.definition) {
                        c.idles =
                            terrain::IdleAnimations::from_slots(&defs.animation_slots(def, &names));
                        c.morphs = defs.skeletal_morphs(def, &names);
                    }
                }
                for d in &mut mesh.doors {
                    if let Some(def) = defs.get(&d.definition) {
                        let slots = defs.animation_slots(def, &names);
                        let find =
                            |slot: &str| slots.iter().find(|s| s.slot == slot).map(|s| s.animation);
                        d.animations = [find("DOOR_OPEN"), find("DOOR_CLOSE")];
                    }
                }
                let hero_slots = defs
                    .get(HERO_DEFINITION)
                    .map(|d| defs.animation_slots(d, &names))
                    .unwrap_or_default();
                let hero_morphs = defs
                    .get(HERO_DEFINITION)
                    .map(|d| defs.skeletal_morphs(d, &names))
                    .unwrap_or_default();
                let (lo, hi) = mesh.bounds();
                let themes = match &mesh.themes {
                    Some(grid) => Some(themes::load(data_dir, &grid.layers)?),
                    None => None,
                };
                let mut renderer = TerrainRenderer::new(device, queue, &mesh, themes.as_ref());
                renderer.hero_animations = HeroAnimations::from_slots(&hero_slots);
                renderer.hero_morphs = hero_morphs;
                let resolve = |object: &str| {
                    defs.get(object)
                        .map(|d| defs.meshes_of(d))
                        .unwrap_or_default()
                };
                let (things, buildings) =
                    model::load_placements(data_dir, name, &placements, &resolve)?;
                renderer.buildings = buildings;
                let status = format!(
                    "{} maps, {} terrain vertices, {} placed meshes ({} triangles)",
                    mesh.maps,
                    mesh.vertices.len(),
                    placements.len(),
                    things.indices.len() / 3
                );
                if !things.indices.is_empty() {
                    renderer.things = Some(ModelRenderer::new(device, queue, &things));
                    renderer.npcs = npc::load(data_dir, device, queue, &mesh.creatures);
                    renderer.doors = door::load(data_dir, device, queue, &mesh.doors);
                    eprintln!("{} doors of {}", renderer.doors.len(), mesh.doors.len());
                    eprintln!(
                        "{} creatures placed of {}",
                        renderer.npcs.len(),
                        mesh.creatures.len()
                    );
                    let p = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].position);
                    let n = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].normal);
                    let triangles = things.indices.as_chunks::<3>().0;
                    renderer.colliders = Some(collide::Colliders::new(
                        triangles.iter().map(|t| t.map(p)).collect(),
                        &triangles
                            .iter()
                            .map(|t| n(t[0]) + n(t[1]) + n(t[2]))
                            .collect::<Vec<_>>(),
                    ));
                    if let (Some(colliders), Some(heights)) =
                        (&renderer.colliders, &renderer.heights)
                    {
                        let navigation = renderer.walk.clone();
                        let before: Vec<usize> =
                            renderer.walk.iter().map(|l| l.walkable_cells()).collect();
                        walk::assign_surfaces(&mut renderer.walk, heights, |p| {
                            colliders.surfaces(p)
                        });
                        let after: Vec<usize> =
                            renderer.walk.iter().map(|l| l.walkable_cells()).collect();
                        eprintln!(
                            "walk layers: {before:?} navigation cells, {after:?} with a surface height"
                        );
                        let (v, i) = terrain::TerrainMesh::walk_overlay(
                            &navigation,
                            &renderer.walk,
                            heights,
                        );
                        renderer.set_navigation(device, &v, &i);
                    }
                }
                Ok((
                    Self::Terrain(Box::new(renderer)),
                    OrbitCamera::framing(lo, hi),
                    status,
                ))
            }
            Subject::Mesh(name) => {
                let model = model::load_mesh(data_dir, name)?;
                let status = format!(
                    "{}: {} vertices, {} triangles, {} textures",
                    model.name,
                    model.vertices.len(),
                    model.indices.len() / 3,
                    model.images.len() - 1
                );
                Ok((
                    Self::Model(Box::new(ModelRenderer::new(device, queue, &model))),
                    OrbitCamera::framing(model.lo, model.hi),
                    status,
                ))
            }
        }
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        view_proj: Mat4,
    ) -> bool {
        match self {
            Self::Terrain(r) => r.render(device, queue, size, view_proj),
            Self::Model(r) => r.render(device, queue, size, view_proj),
        }
    }

    fn target(&self) -> Option<&Target> {
        match self {
            Self::Terrain(r) => r.target.as_ref(),
            Self::Model(r) => r.target.as_ref(),
        }
    }
}

fn main() -> Result<(), Error> {
    let args = parse_args()?;
    let data_dir =
        PathBuf::from(std::env::var_os("FABLE_DIR").ok_or("set FABLE_DIR to the game install")?)
            .join("data");
    let wld = Wld::open(data_dir.join("Levels/FinalAlbion.wld"))?;
    match &args.screenshot {
        Some(path) => screenshot(&data_dir, &wld, &args, path),
        None => run_window(data_dir, wld, args),
    }
}

fn screenshot(data_dir: &Path, wld: &Wld, args: &Args, path: &Path) -> Result<(), Error> {
    let (subject, size) = (&args.subject, args.size);
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    eprintln!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let sections = match subject {
        Subject::Region(name) if args.sections.is_empty() => terrain::default_sections(name),
        _ => args.sections.clone(),
    };
    let (mut scene, mut camera, status) =
        Scene::load(data_dir, wld, subject, &sections, &device, &queue)?;
    if let Scene::Terrain(renderer) = &mut scene {
        renderer.show_nav = args.nav;
        if let (true, Some(heights)) = (args.ledges, &renderer.heights) {
            let near = args.camera.map(|c| glam::Vec2::new(c[0], c[1]));
            for (layer, grid) in renderer.walk.iter().enumerate() {
                if let (Some(n), Some(colliders), 0) = (near, &renderer.colliders, layer) {
                    for dy in -6..=6 {
                        for dx in -6..=6 {
                            let p = n + glam::Vec2::new(dx as f32, dy as f32) * walk::CELL;
                            eprintln!(
                                "cell {p}: terrain {:.2}, walks at {:?}, surfaces {:?}",
                                heights.height(p),
                                grid.height(p, heights),
                                colliders.surfaces(p)
                            );
                        }
                    }
                }
                for (p, lo, hi) in grid.ledges(heights) {
                    if near.is_none_or(|n| n.distance(p) < 12.0) {
                        eprintln!("ledge layer {layer} at {p}: {lo:.2} .. {hi:.2}");
                    }
                }
            }
        }
    }
    // The region's quest, with its people placed and posed.
    let mut story = match subject {
        Subject::Region(region) => story::Story::for_region(region, &sections),
        Subject::Mesh(_) => None,
    };
    if let (Some(story), Scene::Terrain(renderer)) = (&mut story, &mut scene) {
        story.place(&mut renderer.npcs, data_dir);
    }
    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
        camera = OrbitCamera {
            target: glam::Vec3::new(x, y, z),
            distance,
            yaw,
            pitch,
        };
    }
    let mut pending_exit = None;
    if let (true, Scene::Terrain(renderer)) = (args.walk, &mut scene)
        && let Some(mut hero) = spawn_hero(
            renderer,
            data_dir,
            &device,
            &queue,
            camera.target.truncate(),
            args.camera.map(|c| c[2]),
        )?
    {
        if let Some([x, y]) = args.walk_to {
            let goal = glam::Vec2::new(x, y);
            let mut highest = hero.position.z;
            // With --use: walk, use the nearest door, let it swing, walk on.
            for pass in 0..1 + usize::from(args.use_door) {
                if pass == 1 {
                    match door::nearest(&renderer.doors, hero.position) {
                        Some(d) => {
                            renderer.doors[d].toggle();
                            eprintln!("used door {d} (open: {})", renderer.doors[d].open);
                            for door in &mut renderer.doors {
                                door.animate(&queue, 3.0);
                            }
                        }
                        None => eprintln!("no door within reach"),
                    }
                }
                // Follow a route there, or head straight for it if there is none.
                let mut follow = walk::Follow::new(goal);
                for _ in 0..6000 {
                    let Some(heights) = &renderer.heights else {
                        break;
                    };
                    let (doors, npcs) = (&renderer.doors, &renderer.npcs);
                    let Some(to) = follow.direction(
                        &hero,
                        &renderer.walk,
                        heights,
                        1.0 / 30.0,
                        |p| blocked_for_route(doors, npcs, p),
                        |p| blocked_for_hero(doors, npcs, p),
                    ) else {
                        break;
                    };
                    hero.step_blocked(
                        to,
                        walk::WALK_SPEED,
                        1.0 / 30.0,
                        &renderer.walk,
                        heights,
                        |p| blocked_for_hero(doors, npcs, p),
                    );
                    highest = highest.max(hero.position.z);
                }
                eprintln!("walk pass {pass} ended at {}", hero.position);
                eprintln!(
                    "{}",
                    if follow.routed() {
                        "followed a route"
                    } else {
                        "no route; walked straight"
                    }
                );
            }
            camera.yaw = hero.heading - std::f32::consts::FRAC_PI_2;
            eprintln!(
                "walked to {} on layer {} (highest {highest:.1})",
                hero.position, hero.layer
            );
            if let Some((_, transform)) = &mut renderer.hero {
                *transform = hero_transform(&hero);
            }
        }
        if args.talk > 0 {
            let text = fable_data::strings::TextBank::open(data_dir, "English")?;
            let mut subs = talk::Subtitles::default();
            let at = hero.position.truncate();
            for _ in 0..args.talk {
                let near = story::nearest_person(&renderer.npcs, at, story::TALK_REACH, None);
                let (Some(story), Some(i)) = (&mut story, near) else {
                    eprintln!("nobody to talk to");
                    continue;
                };
                story.talk(&renderer.npcs, i, at, &text, &mut subs);
                let who = renderer.npcs[i].script_name.as_deref().unwrap_or("?");
                let mut effects = Vec::new();
                loop {
                    subs.step(0.0, &mut effects);
                    if let Some((line, _)) = subs.line() {
                        eprintln!("{who}: {}: {}", line.speaker, line.text);
                        subs.advance();
                    } else {
                        if let Some(question) = subs.asking() {
                            eprintln!("{who} asks: {question}");
                        }
                        break;
                    }
                }
                subs.clear();
            }
        }
        pending_exit = exit_at(renderer, &hero);
        if pending_exit.is_none() {
            pose_and_frame(renderer, data_dir, &queue, &mut camera, &hero, args)?;
        }
    }
    // Walked into a region exit: load where it leads, as the game would.
    if let Some(connection) = pending_exit {
        let gtg = load_gtg(data_dir)?;
        let (region, at, forward) =
            resolve_exit(wld, &gtg, connection).ok_or("exit leads nowhere")?;
        eprintln!("region exit -> {region} at {at}");
        let sections = terrain::default_sections(&region);
        scene = Scene::load(
            data_dir,
            wld,
            &Subject::Region(region),
            &sections,
            &device,
            &queue,
        )?
        .0;
        let Scene::Terrain(renderer) = &mut scene else {
            return Err("not a region".into());
        };
        let mut hero = spawn_hero(
            renderer,
            data_dir,
            &device,
            &queue,
            at.truncate(),
            Some(at.z),
        )?
        .ok_or("nowhere walkable at the entrance")?;
        hero.heading = (-forward.x).atan2(forward.y);
        camera.yaw = hero.heading - std::f32::consts::FRAC_PI_2;
        if let Some((_, transform)) = &mut renderer.hero {
            *transform = hero_transform(&hero);
        }
        pose_and_frame(renderer, data_dir, &queue, &mut camera, &hero, args)?;
    }
    if let Scene::Terrain(renderer) = &mut scene {
        let (layers, heights) = (&renderer.walk, renderer.heights.as_ref());
        let steps = (args.npc_time * 30.0) as u32;
        let doors = &renderer.doors;
        let blocked = |p: glam::Vec2| doors.iter().any(|d| d.blocks(p));
        for _ in 0..steps {
            for npc in &mut renderer.npcs {
                npc.animate(&queue, 1.0 / 30.0, layers, heights, &blocked);
            }
        }
        for door in &mut renderer.doors {
            door.animate(&queue, args.npc_time);
        }
    }
    if let (Scene::Terrain(renderer), true) = (&scene, args.npc_time > 0.0) {
        let wandered: Vec<String> = renderer
            .npcs
            .iter()
            .filter_map(|n| n.wandered())
            .map(|d| format!("{d:.1}"))
            .collect();
        eprintln!(
            "{} wandering creatures, distances from home after {} s: {wandered:?}",
            wandered.len(),
            args.npc_time
        );
    }
    eprintln!("{subject:?}: {status}");
    scene.render(
        &device,
        &queue,
        size,
        camera.view_proj(size[0] as f32 / size[1] as f32),
    );
    let pixels = render::read_rgba(scene.target().unwrap(), &device, &queue);
    image::save_buffer(
        path,
        &pixels,
        size[0],
        size[1],
        image::ExtendedColorType::Rgba8,
    )?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// How wide a person is, for keeping the Hero and creatures apart.
const PERSON_RADIUS: f32 = 0.35;

/// Where a route should not go: where the Hero may not stand, with a margin
/// around people so that walking between the route's points does not graze them.
fn blocked_for_route(doors: &[door::Door], npcs: &[npc::Npc], p: glam::Vec2) -> bool {
    blocked_for_hero(doors, npcs, p)
        || npcs
            .iter()
            .any(|n| !n.hidden && n.position().distance(p) < PERSON_RADIUS * 2.0 + walk::CELL)
}

/// Whether the Hero may not stand at `p`: a closed door, or a creature there.
fn blocked_for_hero(doors: &[door::Door], npcs: &[npc::Npc], p: glam::Vec2) -> bool {
    doors.iter().any(|d| d.blocks(p))
        || npcs
            .iter()
            .any(|n| !n.hidden && n.position().distance(p) < PERSON_RADIUS * 2.0)
}

/// The clip the standing Hero plays: his idle (0), or after he has stood for a
/// while one of his fidgets (clips from 3 up to `end`), played once.
fn hero_idle_clip(
    animator: Option<&skin::Animator>,
    end: usize,
    standing: &mut f32,
    fidgets: &mut usize,
    dt: f32,
) -> usize {
    let Some(animator) = animator else {
        return 0;
    };
    let (current, count) = (animator.current(), animator.clip_count().min(end));
    if current >= HERO_CLIPS.len() && !animator.finished() {
        return current;
    }
    *standing += dt;
    if count > HERO_CLIPS.len() && *standing > 10.0 {
        *standing = 0.0;
        *fidgets += 1;
        return HERO_CLIPS.len() + *fidgets % (count - HERO_CLIPS.len());
    }
    0
}

/// Poses the Hero for a screenshot (`--anim`, or idle/walk) and frames him.
fn pose_and_frame(
    renderer: &mut TerrainRenderer,
    data_dir: &Path,
    queue: &wgpu::Queue,
    camera: &mut OrbitCamera,
    hero: &walk::Hero,
    args: &Args,
) -> Result<(), Error> {
    let mut clip = usize::from(args.walk_to.is_some());
    if let (Some(name), Some(animator)) = (&args.anim, &mut renderer.hero_animator) {
        clip = match HERO_CLIPS.iter().position(|c| c == name) {
            Some(i) => i,
            None => animator.add_clip(skin::Clip::load(data_dir, name, animator.skeleton())?),
        };
    }
    if let Some(animator) = &renderer.hero_animator {
        for (i, name) in HERO_CLIPS.iter().enumerate() {
            let speed =
                animator.speed(i).unwrap_or(0.0) * model::MESH_SCALE * animator.stride_ratio();
            eprintln!("{name}: {speed:.2} m/s");
        }
    }
    animate_hero(renderer, queue, clip, args.anim_time);
    if let Some(animator) = &renderer.hero_animator {
        eprintln!(
            "posed with {:?} at {}s",
            animator.clip_name(),
            args.anim_time
        );
    }
    renderer.update_interior(hero.position);
    if let Some(i) = renderer.inside {
        eprintln!("inside building {i} (roof hidden)");
    }
    let distance = camera.distance.clamp(3.0, 12.0);
    follow(camera, hero, renderer, distance);
    eprintln!("hero at {}", hero.position);
    Ok(())
}

/// Reads `Levels/FinalAlbion.gtg` (region entrances).
fn load_gtg(data_dir: &Path) -> Result<Gtg, Error> {
    let bytes = std::fs::read(data_dir.join("Levels/FinalAlbion.gtg"))?;
    Ok(Gtg::parse(
        &bytes.iter().map(|&b| b as char).collect::<String>(),
    )?)
}

/// Where a region exit leads: the region to load, and the entrance's world
/// position and facing.
fn resolve_exit(wld: &Wld, gtg: &Gtg, connection: u64) -> Option<(String, glam::Vec3, glam::Vec3)> {
    let (map_uid, local) = split_connected_uid(connection);
    let map = wld.maps.iter().find(|m| m.uid == map_uid)?;
    let entrance = gtg.map(map.number)?.thing(local)?;
    let region = wld
        .regions
        .iter()
        .find(|r| !r.region_def.is_empty() && r.contains_maps.contains(&map.level_name))?;
    let origin = glam::Vec3::new(map.x as f32, map.y as f32, 0.0);
    Some((
        region.name.clone(),
        origin + glam::Vec3::from_array(entrance.position()?),
        glam::Vec3::from_array(entrance.forward().unwrap_or([0.0, 1.0, 0.0])),
    ))
}

/// The region exit the Hero stands in, if any.
fn exit_at(renderer: &TerrainRenderer, hero: &walk::Hero) -> Option<u64> {
    renderer
        .exits
        .iter()
        .find(|e| {
            e.position.truncate().distance(hero.position.truncate()) < e.radius
                && (e.position.z - hero.position.z).abs() < 4.0
        })
        .map(|e| e.connection)
}

/// Places the young Hero on the walkable cell nearest `near`.
fn spawn_hero(
    renderer: &mut TerrainRenderer,
    data_dir: &Path,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    near: glam::Vec2,
    height: Option<f32>,
) -> Result<Option<walk::Hero>, Error> {
    let Some(heights) = &renderer.heights else {
        return Ok(None);
    };
    // The walkable spot nearest `near` on each layer, standing on its walking
    // surface (a floor, not the terrain under it). Given a height, the layer
    // whose spot is nearest it (an upper floor); else the ground layer.
    let Some((_, layer, at, z)) = renderer
        .walk
        .iter()
        .enumerate()
        .filter_map(|(layer, grid)| {
            let at = grid.nearest_walkable(near)?;
            let z = match grid.height(at, heights) {
                Some(z) => z,
                None if layer == 0 => heights.height(at),
                None => return None,
            };
            let off = match height {
                Some(h) => (z - h).abs(),
                None if layer == 0 => 0.0,
                None => return None,
            };
            Some((at.distance(near) + off, layer, at, z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
    else {
        return Ok(None);
    };
    let mut hero = walk::Hero::new(at, heights);
    hero.position.z = z;
    hero.layer = layer;
    let (model, skin) = model::hero_model(data_dir)?;
    renderer.hero = Some((
        ModelRenderer::new(device, queue, &model),
        hero_transform(&hero),
    ));
    let mut bank = skin::AnimBank::open(data_dir)?;
    let animations = renderer.hero_animations.clone();
    let configs = model::bone_configs(data_dir, &renderer.hero_morphs);
    renderer.hero_animator = skin.and_then(|mut skin| {
        skin.set_morph(&configs);
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
                bank.clip(id, &skin.skeleton)
                    .map_err(|e| eprintln!("animation {id}: {e}"))
                    .ok()
            })
            .collect();
        (clips.len() >= HERO_CLIPS.len())
            .then(|| skin::Animator::new(model.vertices.clone(), skin, clips))
    });
    // His kick, after the fidgets.
    renderer.hero_kick = None;
    if let Some(animator) = &mut renderer.hero_animator
        && let Some(id) = bank.id(HERO_KICK)
    {
        match bank.clip(id, animator.skeleton()) {
            Ok(clip) => renderer.hero_kick = Some(animator.add_clip(clip)),
            Err(e) => eprintln!("kick: {e}"),
        }
    }
    Ok(Some(hero))
}

/// The Hero's clips by name, used when his animation table lacks a slot:
/// standing, walking, running (indices used by [`animate_hero`]).
const HERO_CLIPS: [&str; 3] = ["ANIM_HERO_IDLE", "ANIM_HERO_WALK", "ANIM_HERO_RUN"];

/// The boy Hero's kick (one of his `STRIKE_MEDIUM_FRONT` animations).
const HERO_KICK: &str = "ANIM_HERO_UNARMED_KICK_RIGHT_2_1";

/// The young Hero's definition, whose animation table the Hero plays.
const HERO_DEFINITION: &str = "CREATURE_HERO_CHILD";

/// The Hero's animations from his table, as bank ids: idle (`ST_IDLE_SUBTLE`),
/// walk (`ST_WALK`), run (`ST_RUN`) and fidgets (`ST_IDLE`).
#[derive(Debug, Clone, Default)]
pub struct HeroAnimations {
    pub main: [Option<u32>; 3],
    pub fidgets: Vec<u32>,
}

impl HeroAnimations {
    fn from_slots(slots: &[fable_data::defs::AnimationSlot]) -> Self {
        let idles = terrain::IdleAnimations::from_slots(slots);
        let plain = |slot: &str| {
            slots
                .iter()
                .find(|s| s.slot == slot && s.modifier.as_deref().is_none_or(|m| m == "STAND"))
                .map(|s| s.animation)
        };
        Self {
            main: [idles.base, plain("ST_WALK"), plain("ST_RUN")],
            fidgets: idles.fidgets,
        }
    }
}

/// Poses the Hero with clip `clip` advanced by `dt` and uploads his vertices.
fn animate_hero(renderer: &mut TerrainRenderer, queue: &wgpu::Queue, clip: usize, dt: f32) {
    if let (Some(animator), Some((model, _))) = (&mut renderer.hero_animator, &renderer.hero) {
        animator.play(clip);
        model.set_vertices(queue, animator.advance(dt));
    }
}

fn hero_transform(hero: &walk::Hero) -> glam::Mat4 {
    glam::Mat4::from_translation(hero.position)
        * glam::Mat4::from_rotation_z(hero.heading)
        * model::hero_local()
}

/// Points the camera at the Hero's chest from `distance`, or closer when a placed
/// mesh or the ground is in the way. The camera is treated as having some width:
/// besides the ray to it, rays to points around it are cast, so it does not end up
/// pressed against (or poking into) a wall or a cart.
fn follow(camera: &mut OrbitCamera, hero: &walk::Hero, renderer: &TerrainRenderer, distance: f32) {
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
            renderer
                .colliders
                .as_ref()
                .and_then(|c| c.ray(camera.target, dir, len, skip)),
            renderer
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

fn run_window(data_dir: PathBuf, wld: Wld, args: Args) -> Result<(), Error> {
    let subject = args.subject.clone();
    let mesh_names = mesh_names(&data_dir)?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("CullisEngine viewer"),
        ..Default::default()
    };
    eframe::run_native(
        "CullisEngine viewer",
        options,
        Box::new(move |cc| {
            let rs = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("the wgpu renderer is not available")?;
            Ok(Box::new(
                ViewerApp::new(
                    rs,
                    data_dir,
                    wld,
                    mesh_names,
                    subject,
                    args.sections.clone(),
                )
                .map(|mut app| {
                    // --camera, --walk and --walk-to also work in the window:
                    // the Hero then walks himself towards the point.
                    if let Some([x, y, z, distance, yaw, pitch]) = args.camera {
                        app.camera = OrbitCamera {
                            target: glam::Vec3::new(x, y, z),
                            distance,
                            yaw,
                            pitch,
                        };
                    }
                    app.want_walk = args.walk;
                    app.auto_walk = args
                        .walk_to
                        .map(|g| walk::Follow::new(glam::Vec2::from_array(g)));
                    app.auto_talk = args.talk > 0;
                    if let (true, Some(story)) = (args.with_present, &mut app.story)
                        && let story::Act::Birthday(quest) = &mut story.act
                    {
                        quest.skip_to_present();
                    }
                    app
                })
                .map_err(|e| e.to_string())?,
            ))
        }),
    )?;
    Ok(())
}

fn mesh_names(data_dir: &Path) -> Result<Vec<String>, Error> {
    let mut file = BufReader::new(File::open(data_dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no MBANK_ALLMESHES")?
        .read_index(&mut file)?;
    let mut names: Vec<String> = index
        .entries
        .into_iter()
        .filter(|e| [STATIC_MESH_TYPE, SKINNED_MESH_TYPE].contains(&e.entry_type))
        .map(|e| e.name)
        .collect();
    names.sort();
    Ok(names)
}

#[derive(PartialEq, Eq)]
enum ListMode {
    Regions,
    Meshes,
    Textures,
    Text,
}

/// A texture shown in place of the 3D view.
struct TextureView {
    id: u32,
    name: String,
    handle: egui::TextureHandle,
    size: [usize; 2],
}

/// A scene being loaded on a background thread.
struct Loading {
    subject: Subject,
    rx: std::sync::mpsc::Receiver<Result<Loaded, String>>,
    started: std::time::Instant,
}

/// What a background load produces.
struct Loaded {
    scene: Scene,
    camera: OrbitCamera,
    status: String,
    hero: Option<walk::Hero>,
}

struct ViewerApp {
    /// The Tng sections (quest states) shown besides `NULL`.
    sections: Vec<String>,
    /// Sections chosen in the side panel; the region reloads with them.
    reload_sections: Option<Vec<String>>,
    /// A scene being loaded in the background, shown as a loading screen.
    loading: Option<Loading>,
    /// The quest being played, if this region has one, and what is on screen.
    story: Option<story::Story>,
    subs: talk::Subtitles,
    /// An answer clicked on a question, handled next frame.
    clicked_answer: Option<bool>,
    /// The Hero is kicking (he stands still until the kick ends).
    kicking: bool,
    /// Seconds the Hero has stood still, and fidgets played (picks the next).
    hero_standing: f32,
    hero_fidgets: usize,
    /// A point the Hero walks himself towards (`--walk-to`), until he is there.
    auto_walk: Option<walk::Follow>,
    /// Talk to whoever is near once `auto_walk` arrives (`--talk`).
    auto_talk: bool,
    data_dir: PathBuf,
    wld: Wld,
    mesh_names: Vec<String>,
    textures: model::TextureBank,
    texture_names: Vec<(u32, String)>,
    texture_view: Option<TextureView>,
    /// The game's text (`lang/English/text.big`), if it loaded.
    text: Option<fable_data::strings::TextBank>,
    /// Entry names of the text, sorted, and the one shown.
    text_names: Vec<String>,
    text_view: Option<String>,
    /// The walking Hero, when walk mode is on.
    walker: Option<walk::Hero>,
    /// Preferred camera distance in walk mode (the scroll wheel changes it).
    follow_distance: f32,
    /// Region entrances, for region exits.
    gtg: Gtg,
    /// A region exit the Hero walked into, taken at the start of the next frame.
    pending_exit: Option<u64>,
    /// Exits trigger only after the Hero has been outside all of them, so arriving
    /// on an entrance next to an exit does not send him straight back.
    exits_armed: bool,
    want_walk: bool,
    list: ListMode,
    filter: String,
    subject: Subject,
    status: String,
    scene: Scene,
    camera: OrbitCamera,
    texture: Option<egui::TextureId>,
}

impl ViewerApp {
    fn new(
        rs: &egui_wgpu::RenderState,
        data_dir: PathBuf,
        wld: Wld,
        mesh_names: Vec<String>,
        subject: Subject,
        sections: Vec<String>,
    ) -> Result<Self, Error> {
        let sections = match &subject {
            Subject::Region(name) if sections.is_empty() => terrain::default_sections(name),
            _ => sections,
        };
        let (mut scene, camera, status) =
            Scene::load(&data_dir, &wld, &subject, &sections, &rs.device, &rs.queue)?;
        // The region's quest, if it has one, with its people placed.
        let story = match &subject {
            Subject::Region(name) => story::Story::for_region(name, &sections),
            Subject::Mesh(_) => None,
        };
        let mut story = story;
        if let (Some(story), Scene::Terrain(renderer)) = (&mut story, &mut scene) {
            story.place(&mut renderer.npcs, &data_dir);
        }
        let list = match subject {
            Subject::Region(_) => ListMode::Regions,
            Subject::Mesh(_) => ListMode::Meshes,
        };
        let textures = model::TextureBank::open(&data_dir)?;
        let text = fable_data::strings::TextBank::open(&data_dir, "English")
            .map_err(|e| eprintln!("text: {e}"))
            .ok();
        let text_names = text
            .as_ref()
            .map(|t| t.names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        let gtg = load_gtg(&data_dir)?;
        let texture_names = textures.names();
        Ok(Self {
            data_dir,
            wld,
            mesh_names,
            texture_names,
            texture_view: None,
            text,
            text_names,
            text_view: None,
            walker: None,
            follow_distance: 6.0,
            gtg,
            pending_exit: None,
            loading: None,
            story,
            subs: talk::Subtitles::default(),
            clicked_answer: None,
            kicking: false,
            sections,
            reload_sections: None,
            auto_walk: None,
            auto_talk: false,
            hero_standing: 0.0,
            hero_fidgets: 0,
            exits_armed: false,
            want_walk: false,
            textures,
            list,
            filter: String::new(),
            subject,
            status,
            scene,
            camera,
            texture: None,
        })
    }

    /// Starts loading `subject` on a background thread; the window shows a
    /// loading screen until [`ViewerApp::finish_load`] installs it. `arrival`
    /// places the Hero at an entrance (position, facing) once a region loads.
    fn load(
        &mut self,
        rs: &egui_wgpu::RenderState,
        subject: Subject,
        arrival: Option<(glam::Vec3, glam::Vec3)>,
        sections: Vec<String>,
    ) {
        self.sections = sections.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (data_dir, wld, device, queue) = (
            self.data_dir.clone(),
            self.wld.clone(),
            rs.device.clone(),
            rs.queue.clone(),
        );
        let job = subject.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Loaded, Error> {
                let (mut scene, mut camera, status) =
                    Scene::load(&data_dir, &wld, &job, &sections, &device, &queue)?;
                let mut hero = None;
                if let (Some((at, forward)), Scene::Terrain(renderer)) = (arrival, &mut scene) {
                    let mut h = spawn_hero(
                        renderer,
                        &data_dir,
                        &device,
                        &queue,
                        at.truncate(),
                        Some(at.z),
                    )?
                    .ok_or("nowhere walkable at the entrance")?;
                    h.heading = (-forward.x).atan2(forward.y);
                    if let Some((_, transform)) = &mut renderer.hero {
                        *transform = hero_transform(&h);
                    }
                    camera.yaw = h.heading - std::f32::consts::FRAC_PI_2;
                    hero = Some(h);
                }
                Ok(Loaded {
                    scene,
                    camera,
                    status,
                    hero,
                })
            })();
            // The window may have gone; nothing to do then.
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
        // Nothing from the old scene carries over: no queued exit, no scripted walk.
        self.pending_exit = None;
        self.exits_armed = false;
        self.auto_walk = None;
        self.loading = Some(Loading {
            subject,
            rx,
            started: std::time::Instant::now(),
        });
    }

    /// Installs a finished background load. Returns whether one is still running.
    fn finish_load(&mut self, rs: &egui_wgpu::RenderState) -> bool {
        let Some(loading) = &self.loading else {
            return false;
        };
        let result = match loading.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("loader stopped".into()),
        };
        let loading = self.loading.take().unwrap();
        let subject = loading.subject;
        match result {
            Ok(loaded) => {
                self.scene = loaded.scene;
                self.camera = loaded.camera;
                self.status = format!(
                    "{} (loaded in {:.1} s)",
                    loaded.status,
                    loading.started.elapsed().as_secs_f32()
                );
                if let Some(id) = self.texture.take() {
                    rs.renderer.write().free_texture(&id);
                }
                self.subject = subject;
                self.walker = None;
                self.want_walk = false;
                self.subs.clear();
                self.kicking = false;
                if let Subject::Region(region) = &self.subject
                    && self.story.is_none()
                {
                    self.story = story::Story::for_region(region, &self.sections);
                }
                if let (Some(story), Scene::Terrain(renderer)) = (&mut self.story, &mut self.scene)
                {
                    story.place(&mut renderer.npcs, &self.data_dir);
                }
                if let (Some(hero), Scene::Terrain(renderer)) = (loaded.hero, &mut self.scene) {
                    follow(&mut self.camera, &hero, renderer, self.follow_distance);
                    self.walker = Some(hero);
                    self.want_walk = true;
                    self.exits_armed = false;
                }
            }
            Err(e) => self.status = format!("{subject:?}: {e}"),
        }
        false
    }

    /// Starts loading the region a region exit leads to, with the Hero arriving at
    /// its entrance.
    fn take_exit(&mut self, rs: &egui_wgpu::RenderState, connection: u64) {
        match resolve_exit(&self.wld, &self.gtg, connection) {
            Some((region, at, forward)) => {
                let sections = terrain::default_sections(&region);
                self.load(rs, Subject::Region(region), Some((at, forward)), sections)
            }
            None => self.status = format!("region exit {connection:#x} leads nowhere"),
        }
    }

    /// Returns a chosen region or mesh, and a chosen texture id.
    fn side_panel(&mut self, ui: &mut egui::Ui) -> (Option<Subject>, Option<u32>) {
        let mut chosen = None;
        let mut chosen_texture = None;
        let title = match &self.subject {
            Subject::Region(n) => self.region_title(n),
            Subject::Mesh(n) => n.clone(),
        };
        ui.heading(title);
        ui.label(&self.status);
        ui.label("Drag: orbit · Right-drag: pan · Scroll: zoom");
        if let Scene::Terrain(renderer) = &mut self.scene {
            ui.checkbox(&mut renderer.show_nav, "Show navigation");
            ui.checkbox(&mut self.want_walk, "Walk (WASD, Shift runs)");
            if !renderer.sections.is_empty() {
                egui::CollapsingHeader::new("Quest state (Tng sections)").show(ui, |ui| {
                    for section in &renderer.sections {
                        let mut on = self.sections.contains(section);
                        if ui.checkbox(&mut on, section).changed() {
                            let mut next = self.sections.clone();
                            next.retain(|s| s != section);
                            if on {
                                next.push(section.clone());
                            }
                            self.reload_sections = Some(next);
                        }
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.list, ListMode::Regions, "Regions");
            ui.selectable_value(&mut self.list, ListMode::Meshes, "Meshes");
            ui.selectable_value(&mut self.list, ListMode::Textures, "Textures");
            ui.selectable_value(&mut self.list, ListMode::Text, "Text");
        });
        ui.text_edit_singleline(&mut self.filter);
        let filter = self.filter.to_ascii_uppercase();
        egui::ScrollArea::vertical().show(ui, |ui| match self.list {
            ListMode::Regions => {
                for r in &self.wld.regions {
                    let label = self.region_title(&r.name);
                    if !label.to_ascii_uppercase().contains(&filter) {
                        continue;
                    }
                    let current = self.subject == Subject::Region(r.name.clone());
                    if ui.selectable_label(current, label).clicked() {
                        chosen = Some(Subject::Region(r.name.clone()));
                    }
                }
            }
            ListMode::Textures => {
                for (id, name) in self
                    .texture_names
                    .iter()
                    .filter(|(_, n)| n.contains(&filter))
                {
                    let current = self.texture_view.as_ref().is_some_and(|t| t.id == *id);
                    if ui.selectable_label(current, name).clicked() {
                        chosen_texture = Some(*id);
                    }
                }
            }
            ListMode::Text => {
                // Names, or text containing the search, up to a few hundred.
                let text = self.text.as_ref();
                let matches = self.text_names.iter().filter(|n| {
                    n.contains(&filter)
                        || (filter.len() > 2
                            && text
                                .and_then(|t| t.get(n))
                                .is_some_and(|s| s.to_ascii_uppercase().contains(&filter)))
                });
                for name in matches.take(500) {
                    let current = self.text_view.as_deref() == Some(name.as_str());
                    if ui.selectable_label(current, name).clicked() {
                        self.text_view = Some(name.clone());
                    }
                }
            }
            ListMode::Meshes => {
                for name in self.mesh_names.iter().filter(|n| n.contains(&filter)) {
                    let current = self.subject == Subject::Mesh(name.clone());
                    if ui.selectable_label(current, name).clicked() {
                        chosen = Some(Subject::Mesh(name.clone()));
                    }
                }
            }
        });
        (chosen, chosen_texture)
    }
}

impl ViewerApp {
    /// A region's name in the game's text (`TXT_REGION_OAKVALE` -> "Oakvale"), or
    /// its own name.
    fn region_display_name(&self, region: &str) -> String {
        self.wld
            .region(region)
            .and_then(|r| self.text.as_ref()?.get(&r.display_name))
            .filter(|t| !t.is_empty())
            .map_or_else(|| region.to_owned(), str::to_owned)
    }

    /// "Oakvale (OakBay)": the game's name and the wld's.
    fn region_title(&self, region: &str) -> String {
        let shown = self.region_display_name(region);
        if shown == region {
            shown
        } else {
            format!("{shown} ({region})")
        }
    }

    /// Spawns or removes the Hero when the walk checkbox changes.
    fn update_walk_mode(&mut self, rs: &egui_wgpu::RenderState) {
        let Scene::Terrain(renderer) = &mut self.scene else {
            return;
        };
        match (self.want_walk, self.walker.is_some()) {
            (true, false) => {
                match spawn_hero(
                    renderer,
                    &self.data_dir,
                    &rs.device,
                    &rs.queue,
                    self.camera.target.truncate(),
                    None,
                ) {
                    Ok(Some(hero)) => {
                        follow(&mut self.camera, &hero, renderer, self.follow_distance);
                        self.walker = Some(hero);
                    }
                    Ok(None) => {
                        self.status = "nowhere walkable in this region".into();
                        self.want_walk = false;
                    }
                    Err(e) => {
                        self.status = format!("hero: {e}");
                        self.want_walk = false;
                    }
                }
            }
            (false, true) => {
                renderer.hero = None;
                self.walker = None;
            }
            _ => {}
        }
    }

    /// Moves the Hero from the keyboard, relative to the camera, and follows it.
    fn walk(&mut self, ui: &egui::Ui, queue: &wgpu::Queue) {
        if self.loading.is_some() {
            return;
        }
        let (Some(hero), Scene::Terrain(renderer)) = (&mut self.walker, &mut self.scene) else {
            return;
        };
        let (walk, Some(heights)) = (&renderer.walk, &renderer.heights) else {
            return;
        };
        let (forward, right, run, dt) = ui.input(|i| {
            let axis = |pos: egui::Key, neg: egui::Key| {
                f32::from(u8::from(i.key_down(pos))) - f32::from(u8::from(i.key_down(neg)))
            };
            (
                axis(egui::Key::W, egui::Key::S),
                axis(egui::Key::D, egui::Key::A),
                i.modifiers.shift,
                i.stable_dt.min(0.1),
            )
        });
        let view = (self.camera.target - self.camera.eye())
            .truncate()
            .normalize_or_zero();
        let side = glam::Vec2::new(view.y, -view.x);
        let mut direction = view * forward + side * right;
        // A walk to a point (`--walk-to`) along a route, while no key steers.
        if direction == glam::Vec2::ZERO
            && let Some(follow) = &mut self.auto_walk
        {
            let (doors, npcs) = (&renderer.doors, &renderer.npcs);
            match follow.direction(
                hero,
                walk,
                heights,
                dt,
                |p| blocked_for_route(doors, npcs, p),
                |p| blocked_for_hero(doors, npcs, p),
            ) {
                Some(to) => direction = to,
                None => {
                    self.auto_walk = None;
                    let at = hero.position.truncate();
                    if std::mem::take(&mut self.auto_talk)
                        && let (Some(text), Some(story)) = (&self.text, &mut self.story)
                        && let Some(i) =
                            story::nearest_person(&renderer.npcs, at, story::TALK_REACH, None)
                    {
                        story.talk(&renderer.npcs, i, at, text, &mut self.subs);
                    }
                }
            }
        }
        // The clips' own movement sets the speed, so the feet do not slide.
        let clip_speed = renderer
            .hero_animator
            .as_ref()
            .and_then(|a| {
                a.speed(if run { 2 } else { 1 })
                    .map(|s| s * a.stride_ratio())
            })
            .map(|s| s * model::MESH_SCALE);
        let speed = clip_speed.unwrap_or(if run {
            walk::RUN_SPEED
        } else {
            walk::WALK_SPEED
        });
        let before = hero.position;
        let at = hero.position.truncate();
        let (talk_key, kick_key, answer_key) = ui.input(|i| {
            let answer = if i.key_pressed(egui::Key::Y) {
                Some(true)
            } else if i.key_pressed(egui::Key::N) {
                Some(false)
            } else {
                None
            };
            (
                i.key_pressed(egui::Key::E),
                i.key_pressed(egui::Key::F),
                answer,
            )
        });
        // E: go on with what is being said, else talk to someone near, else use a door.
        if talk_key && !self.subs.advance() && self.subs.asking().is_none() {
            let near = story::nearest_person(&renderer.npcs, at, story::TALK_REACH, None);
            if let (Some(story), Some(text), Some(i)) = (&mut self.story, &self.text, near) {
                story.talk(&renderer.npcs, i, at, text, &mut self.subs);
            } else if let Some(d) = door::nearest(&renderer.doors, hero.position) {
                renderer.doors[d].toggle();
            }
        }
        // Y/N (or a click): answer the question on screen.
        if let Some(yes) = answer_key.or(self.clicked_answer.take())
            && self.subs.asking().is_some()
            && let (Some(story), Some(text)) = (&mut self.story, &self.text)
        {
            story.answer(yes, &renderer.npcs, at, text, &mut self.subs);
        }
        // F: kick whoever is in front.
        if kick_key
            && !self.kicking
            && let (Some(kick), Some(animator)) = (renderer.hero_kick, &mut renderer.hero_animator)
        {
            animator.play_once(kick, 0.0);
            self.kicking = true;
            let facing = glam::Vec2::new(-hero.heading.sin(), hero.heading.cos());
            if let Some(i) =
                story::nearest_person(&renderer.npcs, at, story::KICK_REACH, Some(facing))
            {
                renderer.npcs[i].flinch();
                if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
                    story.hit(&renderer.npcs, i, at, text, &mut self.subs);
                }
            }
        }
        if self.kicking {
            direction = glam::Vec2::ZERO;
        }
        // Walking away ends a conversation.
        if let Some(i) = self.subs.waiting_on()
            && renderer
                .npcs
                .get(i)
                .is_none_or(|n| n.position().distance(at) > 4.0)
        {
            self.subs.walked_away();
        }
        if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
            story.update(dt, &renderer.npcs, at, text, &mut self.subs);
        }
        let (doors, npcs) = (&renderer.doors, &renderer.npcs);
        hero.step_blocked(direction, speed, dt, walk, heights, |p| {
            blocked_for_hero(doors, npcs, p)
        });
        let moved = (hero.position - before).truncate().length() > 1e-4;
        if let Some((_, transform)) = &mut renderer.hero {
            *transform = hero_transform(hero);
        }
        let kick = renderer.hero_kick;
        let kicking = self.kicking
            && renderer
                .hero_animator
                .as_ref()
                .is_some_and(|a| Some(a.current()) == kick && !a.finished());
        self.kicking = kicking;
        let clip = match (moved, run) {
            _ if kicking => kick.unwrap_or(0),
            (false, _) => hero_idle_clip(
                renderer.hero_animator.as_ref(),
                kick.unwrap_or(usize::MAX),
                &mut self.hero_standing,
                &mut self.hero_fidgets,
                dt,
            ),
            (true, false) => 1,
            (true, true) => 2,
        };
        if moved {
            self.hero_standing = 0.0;
        }
        animate_hero(renderer, queue, clip, dt);
        ui.ctx().request_repaint();
        renderer.update_interior(hero.position);
        follow(&mut self.camera, hero, renderer, self.follow_distance);
        match exit_at(renderer, hero) {
            Some(connection) if self.exits_armed => self.pending_exit = Some(connection),
            Some(_) => {}
            None => self.exits_armed = true,
        }
    }

    /// Applies a quest beat the subtitles reached: someone leaving or coming back,
    /// or the quest ending (the region reloads in the next quest state, the Hero
    /// where he stands).
    fn apply(&mut self, rs: &egui_wgpu::RenderState, beat: engine_script::Beat) {
        use engine_script::Beat;
        match beat {
            Beat::Leave(role) | Beat::Return(role) => {
                if let Scene::Terrain(renderer) = &mut self.scene {
                    for npc in &mut renderer.npcs {
                        if npc.script_name.as_deref() == Some(role) {
                            npc.hidden = matches!(beat, Beat::Leave(_));
                        }
                    }
                }
            }
            Beat::Finish(sections) => {
                self.story = None;
                let arrival = self.walker.as_ref().map(|h| {
                    (
                        h.position,
                        glam::Vec3::new(-h.heading.sin(), h.heading.cos(), 0.0),
                    )
                });
                let sections = sections.iter().map(|s| (*s).to_owned()).collect();
                self.load(rs, self.subject.clone(), arrival, sections);
            }
            Beat::Travel {
                region,
                sections,
                at: [x, y, z],
                facing: [fx, fy],
            } => {
                self.story = None;
                let forward = (glam::Vec2::new(fx, fy) - glam::Vec2::new(x, y))
                    .normalize_or(glam::Vec2::Y)
                    .extend(0.0);
                let arrival = (glam::Vec3::new(x, y, z), forward);
                let sections = sections.iter().map(|s| (*s).to_owned()).collect();
                self.load(
                    rs,
                    Subject::Region(region.to_owned()),
                    Some(arrival),
                    sections,
                );
            }
            _ => {}
        }
    }

    /// Decodes texture `id` and shows it in place of the 3D view.
    fn show_texture(&mut self, ctx: &egui::Context, id: u32) {
        let name = self
            .texture_names
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.to_string(), |(_, n)| n.clone());
        match self.textures.load(id) {
            Ok(img) => {
                let size = [img.width as usize, img.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &img.rgba);
                let handle = ctx.load_texture(&name, image, egui::TextureOptions::LINEAR);
                self.status = format!("texture {id}: {name}, {}x{}", size[0], size[1]);
                self.texture_view = Some(TextureView {
                    id,
                    name,
                    handle,
                    size,
                });
            }
            Err(e) => self.status = format!("texture {id}: {e}"),
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state().cloned() else {
            return;
        };
        if self.finish_load(&rs) {
            // The loading screen: the region's name while it loads.
            let name = match &self.loading.as_ref().unwrap().subject {
                Subject::Region(n) => self.region_display_name(n),
                Subject::Mesh(n) => n.clone(),
            };
            egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 2.0 - 40.0);
                        ui.heading(format!("Loading {name}…"));
                        ui.add(egui::Spinner::new().size(32.0));
                    });
                });
            });
            ui.ctx().request_repaint();
            return;
        }

        let (chosen, chosen_texture) = egui::Panel::left("list")
            .resizable(true)
            .show(ui, |ui| self.side_panel(ui))
            .inner;
        if let Some(subject) = chosen.filter(|s| *s != self.subject) {
            let sections = match &subject {
                Subject::Region(name) => terrain::default_sections(name),
                Subject::Mesh(_) => Vec::new(),
            };
            self.load(&rs, subject, None, sections);
        }
        if let Some(id) = chosen_texture {
            self.show_texture(ui.ctx(), id);
        }
        if let Some(sections) = self.reload_sections.take() {
            self.load(&rs, self.subject.clone(), None, sections);
        }
        if let Some(connection) = self.pending_exit.take() {
            self.take_exit(&rs, connection);
        }
        self.update_walk_mode(&rs);
        if self.list == ListMode::Text
            && let (Some(name), Some(text)) = (&self.text_view, &self.text)
        {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading(name);
                let id = text.id(name);
                let members: Vec<u32> = match id.and_then(|i| text.groups.get(&i)) {
                    Some(group) => {
                        ui.label(format!("A group of {} alternatives:", group.len()));
                        group.clone()
                    }
                    None => id.into_iter().collect(),
                };
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for m in members {
                        let Some(e) = text.strings.get(&m) else {
                            continue;
                        };
                        ui.separator();
                        let mut details = Vec::new();
                        if !e.speaker.is_empty() {
                            details.push(format!("speaker {}", e.speaker));
                        }
                        if !e.sound_bank.is_empty() {
                            details.push(format!("voice in {}", e.sound_bank));
                        }
                        if !e.tags.is_empty() {
                            details.push(e.tags.join(", "));
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  {}",
                                e.identifier,
                                details.join(" · ")
                            ))
                            .weak(),
                        );
                        ui.label(egui::RichText::new(&e.text).size(16.0));
                    }
                });
            });
            return;
        }
        if self.list == ListMode::Textures
            && let Some(view) = &self.texture_view
        {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(format!(
                    "{} (id {}): {}x{}",
                    view.name, view.id, view.size[0], view.size[1]
                ));
                let avail = ui.available_size();
                let scale = (avail.x / view.size[0] as f32)
                    .min(avail.y / view.size[1] as f32)
                    .min(4.0);
                let size = egui::vec2(view.size[0] as f32 * scale, view.size[1] as f32 * scale);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((view.handle.id(), size)));
                });
            });
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, egui::Sense::drag());
                let delta = response.drag_delta();
                if response.dragged_by(egui::PointerButton::Primary) {
                    self.camera.orbit(delta.x, delta.y);
                }
                if response.dragged_by(egui::PointerButton::Secondary) {
                    self.camera.pan(delta.x, delta.y);
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if self.walker.is_some() {
                        self.follow_distance =
                            (self.follow_distance * (-scroll * 0.002).exp()).clamp(2.0, 15.0);
                    } else {
                        self.camera.zoom(scroll);
                    }
                }
                self.walk(ui, &rs.queue);
                let mut effects = Vec::new();
                self.subs
                    .step(ui.input(|i| i.stable_dt.min(0.1)), &mut effects);
                for effect in effects {
                    self.apply(&rs, effect);
                }
                if let Scene::Terrain(renderer) = &mut self.scene
                    && !renderer.npcs.is_empty()
                {
                    let dt = ui.input(|i| i.stable_dt.min(0.1));
                    let (layers, heights) = (&renderer.walk, renderer.heights.as_ref());
                    let hero = self.walker.as_ref().map(|h| h.position.truncate());
                    let doors = &renderer.doors;
                    let blocked = |p: glam::Vec2| {
                        doors.iter().any(|d| d.blocks(p))
                            || hero.is_some_and(|h| h.distance(p) < PERSON_RADIUS * 2.0)
                    };
                    for npc in &mut renderer.npcs {
                        npc.animate(&rs.queue, dt, layers, heights, &blocked);
                    }
                    for door in &mut renderer.doors {
                        door.animate(&rs.queue, dt);
                    }
                    ui.ctx().request_repaint();
                }

                let ppp = ui.ctx().pixels_per_point();
                let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
                let view_proj = self.camera.view_proj(rect.width() / rect.height().max(1.0));
                let recreated = self.scene.render(&rs.device, &rs.queue, size, view_proj);
                let view = &self.scene.target().unwrap().color_view;
                let mut egui_renderer = rs.renderer.write();
                let id = match self.texture {
                    Some(id) if recreated => {
                        egui_renderer.update_egui_texture_from_wgpu_texture(
                            &rs.device,
                            view,
                            wgpu::FilterMode::Linear,
                            id,
                        );
                        id
                    }
                    Some(id) => id,
                    None => egui_renderer.register_native_texture(
                        &rs.device,
                        view,
                        wgpu::FilterMode::Linear,
                    ),
                };
                self.texture = Some(id);
                drop(egui_renderer);
                ui.painter().image(
                    id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                // What is being said, notices, and the Hero's gold and items.
                if let Some(answer) = self.subs.show(ui.ctx(), rect) {
                    self.clicked_answer = Some(answer);
                }
                if let (Some(story), Some(text), true) =
                    (&self.story, &self.text, self.walker.is_some())
                {
                    story.hud(ui.ctx(), rect, text);
                }
            });
        ui.ctx().request_repaint();
    }
}
