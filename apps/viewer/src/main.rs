#![recursion_limit = "256"]
//! CullisEngine desktop viewer: a region's terrain from the Lev heightmaps, or a
//! static mesh with its textures.
//!
//! ```text
//! FABLE_DIR=... viewer [--region NAME | --mesh NAME] [--nav] [--screenshot OUT.png] [--size WxH] [--camera x,y,z,distance,yaw,pitch]
//! ```

mod audio;
// The simulation's modules, kept at their old paths (`crate::walk`, …).
use engine_world::{camera, collide, conversation, lighting, walk};
mod door;
mod model;
mod npc;
mod particles;
mod render;
mod shadow;
mod skin;
mod sky;
mod soundscape;
mod story;
mod talk;
mod terrain;
mod themes;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use eframe::{egui, egui_wgpu, wgpu};
use engine_core::assets::AssetCache;
use engine_core::time::TICK;
use engine_world::world::World;
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
    /// x, y, and the floor's height (NaN for any floor).
    walk_to: Option<[f32; 3]>,
    /// Tng sections (quest states) to show besides `NULL`; repeatable. Default:
    /// the region's starting state (`terrain::default_sections`).
    sections: Vec<String>,
    /// Pose the Hero with this animation (default: idle, or walk after
    /// `--walk-to`) at `anim_time` seconds.
    anim: Option<String>,
    anim_time: f32,
    /// Advance the creatures this many seconds before the screenshot.
    npc_time: f32,
    /// Leave out placed lights (lamps, fires), for comparing screenshots.
    no_lights: bool,
    /// Time of day, in hours, and the light's brightness.
    hour: f32,
    brightness: f32,
    /// The most light a surface may get (see `lighting::HIGHLIGHT_LIMIT`).
    highlights: f32,
    /// After walking (`--walk-to`), use the nearest door, then walk again.
    use_door: bool,
    /// After walking, talk to the nearest creature this many times, printing
    /// what it says.
    talk: u32,
    /// Start the Prologue quest with Theresa's present bought (for testing the
    /// ending).
    with_present: bool,
    /// Make the creature with this role start a happy conversation gesture
    /// before `--npc-time` runs (for checking gestures in screenshots).
    gesture: Option<String>,
    /// Hold the mouth of the creature with this role in a shape
    /// (`NOVI_LiveFather:AH`), for checking lip sync in screenshots.
    mouth: Option<(String, String)>,
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
        no_lights: false,
        hour: DEFAULT_HOUR,
        brightness: lighting::BRIGHTNESS,
        highlights: lighting::HIGHLIGHT_LIMIT,
        use_door: false,
        talk: 0,
        with_present: false,
        gesture: None,
        mouth: None,
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
            "--hour" => args.hour = value()?.parse()?,
            "--no-lights" => args.no_lights = true,
            "--brightness" => args.brightness = value()?.parse()?,
            "--highlights" => args.highlights = value()?.parse()?,
            "--use" => args.use_door = true,
            "--talk" => args.talk = value()?.parse()?,
            "--with-present" => args.with_present = true,
            "--gesture" => args.gesture = Some(value()?),
            "--mouth" => {
                let v = value()?;
                let (role, shape) = v.split_once(':').ok_or("--mouth takes ROLE:SHAPE")?;
                args.mouth = Some((role.to_owned(), shape.to_owned()));
            }
            "--walk-to" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()?;
                // x,y, or x,y,z for a floor among stacked ones.
                args.walk_to = match *v.as_slice() {
                    [x, y] => Some([x, y, f32::NAN]),
                    [x, y, z] => Some([x, y, z]),
                    _ => return Err("--walk-to takes x,y or x,y,z".into()),
                };
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

/// What the player asks for in a frame, read from the keyboard once and
/// consumed by the frame's simulation ticks. Held keys (movement, run) apply
/// to every tick; presses are taken by the first.
#[derive(Debug, Clone, Default)]
struct Actions {
    /// Forward (W minus S) and right (D minus A), -1..1.
    forward: f32,
    right: f32,
    run: bool,
    /// E: go on, talk, use.
    use_: bool,
    /// F: kick.
    kick: bool,
    /// Y or N to a question.
    answer: Option<bool>,
}

impl Actions {
    fn read(i: &egui::InputState) -> Self {
        let axis = |pos: egui::Key, neg: egui::Key| {
            f32::from(u8::from(i.key_down(pos))) - f32::from(u8::from(i.key_down(neg)))
        };
        Self {
            forward: axis(egui::Key::W, egui::Key::S),
            right: axis(egui::Key::D, egui::Key::A),
            run: i.modifiers.shift,
            use_: i.key_pressed(egui::Key::E),
            kick: i.key_pressed(egui::Key::F),
            answer: if i.key_pressed(egui::Key::Y) {
                Some(true)
            } else if i.key_pressed(egui::Key::N) {
                Some(false)
            } else {
                None
            },
        }
    }
}

/// How long a night passing stays black, and then takes to fade back in
/// (seconds).
const NIGHT_HOLD: f32 = 1.0;
const NIGHT_FADE: f32 = 1.5;

/// The time of day a region opens at: mid-morning (a choice, not the game's).
const DEFAULT_HOUR: f32 = 10.0;

/// Game hours that pass per real second while the clock runs: a day in 24
/// minutes (a choice, not measured from the game).
const CLOCK_RATE: f32 = 1.0 / 60.0;

/// What is on screen: a region's terrain or a single mesh.
enum Scene {
    /// A region: its renderer and its world.
    Terrain(Box<TerrainRenderer>, Box<engine_world::world::World>),
    Model(Box<ModelRenderer>),
}

impl Scene {
    fn load(
        assets: &AssetCache,
        wld: &Wld,
        subject: &Subject,
        sections: &[String],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(Self, OrbitCamera, String), Error> {
        let data_dir = assets.data_dir();
        match subject {
            Subject::Region(name) => {
                let names = Names::parse(&std::fs::read(data_dir.join("CompiledDefs/names.bin"))?)?;
                let defs = Defs::parse(
                    &std::fs::read(data_dir.join("CompiledDefs/game.bin"))?,
                    &names,
                )?;
                let (mut mesh, placements) =
                    terrain::load_region(data_dir, wld, name, &defs, &names, sections)?;
                for c in &mut mesh.creatures {
                    if let Some(def) = defs.get(&c.definition) {
                        let slots = defs.animation_slots(def, &names);
                        c.idles = terrain::IdleAnimations::from_slots(&slots);
                        c.slots = std::sync::Arc::new(slots);
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
                    Some(grid) => Some(themes::load(assets, &grid.layers)?),
                    None => None,
                };
                let mut renderer = TerrainRenderer::new(device, queue, &mesh, themes.as_ref());
                let mut world = World {
                    walk: mesh.walk.clone(),
                    heights: mesh.heights.clone(),
                    point_lights: mesh.lights.clone(),
                    ..World::default()
                };
                if let Some(r) = wld.region(name) {
                    renderer.soundscape = soundscape::Soundscape::resolve(
                        data_dir,
                        &r.region_def,
                        &defs,
                        std::mem::take(&mut mesh.sound_areas),
                    );
                    log::info!(
                        "sound: music {:?}, ambience {:?}",
                        renderer
                            .soundscape
                            .music
                            .as_ref()
                            .and_then(|m| m.file_name()),
                        renderer.soundscape.bank_names()
                    );
                }
                if let Some(r) = wld.region(name) {
                    let outside = story::environment(name, sections);
                    match lighting::Lighting::load(data_dir, &defs, &r.region_def, outside) {
                        Ok(l) => {
                            let ids = l.sky_textures();
                            log::info!(
                                "lighting: {} (inside {:?}), sky textures {ids:?}",
                                l.outside.0,
                                l.inside.as_ref().map(|i| &i.0)
                            );
                            if !ids.is_empty() {
                                let layers = themes::load_ids(assets, &ids)?;
                                renderer.sky = Some(sky::Sky::new(device, queue, &ids, &layers));
                            }
                            world.lighting = Some(l);
                        }
                        Err(e) => log::warn!("lighting: {e}"),
                    }
                    world.set_hour(
                        DEFAULT_HOUR,
                        lighting::BRIGHTNESS,
                        lighting::HIGHLIGHT_LIMIT,
                    );
                }
                log::info!("{} placed lights", world.point_lights.len());
                renderer.hero_animations = HeroAnimations::from_slots(&hero_slots);
                renderer.hero_morphs = hero_morphs;
                let resolve = |object: &str| {
                    defs.get(object)
                        .map(|d| defs.meshes_of(d))
                        .unwrap_or_default()
                };
                let (things, buildings) =
                    model::load_placements(assets, name, &placements, &resolve)?;
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
                    match particles::Particles::load(device, queue, assets, &things.particles) {
                        Ok(p) => {
                            log::info!("{} particle emitters", p.len());
                            renderer.particles = Some(p);
                        }
                        Err(e) => log::warn!("particles: {e}"),
                    }
                    // Interiors and what stands in them take the indoor light.
                    let indoor: Vec<usize> = renderer
                        .buildings
                        .iter()
                        .flat_map(|b| b.indoor_draws.iter().copied())
                        .collect();
                    if let Some(things) = &mut renderer.things {
                        things.set_indoor(&indoor);
                    }
                    let (npcs, models) = npc::load(assets, device, queue, &mesh.creatures);
                    world.npcs = npcs;
                    renderer.creatures = models;
                    renderer.doors = door::load(assets, device, queue, &mesh.doors, &mut world);
                    log::info!("{} doors of {}", renderer.doors.len(), mesh.doors.len());
                    log::info!(
                        "{} creatures placed of {}",
                        world.npcs.len(),
                        mesh.creatures.len()
                    );
                    let p = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].position);
                    let n = |i: u32| glam::Vec3::from_array(things.vertices[i as usize].normal);
                    let triangles = things.indices.as_chunks::<3>().0;
                    world.colliders = Some(collide::Colliders::new(
                        triangles.iter().map(|t| t.map(p)).collect(),
                        &triangles
                            .iter()
                            .map(|t| n(t[0]) + n(t[1]) + n(t[2]))
                            .collect::<Vec<_>>(),
                    ));
                    if let (Some(colliders), Some(heights)) = (&world.colliders, &world.heights) {
                        let navigation = world.walk.clone();
                        let before: Vec<usize> =
                            world.walk.iter().map(|l| l.walkable_cells()).collect();
                        walk::assign_surfaces(&mut world.walk, heights, |p| colliders.surfaces(p));
                        let after: Vec<usize> =
                            world.walk.iter().map(|l| l.walkable_cells()).collect();
                        // Floors the navigation leaves out, from the meshes.
                        let extra = walk::extend_floors(
                            &mut world.walk,
                            heights,
                            |p| colliders.surfaces(p),
                            |a, b| {
                                let d = b - a;
                                let len = d.length();
                                len > 0.0 && colliders.ray(a, d / len, len, &[]).is_some()
                            },
                        );
                        log::info!(
                            "walk layers: {before:?} navigation cells, {after:?} with a surface height, {extra} more from mesh floors"
                        );
                        let (v, i) =
                            terrain::TerrainMesh::walk_overlay(&navigation, &world.walk, heights);
                        renderer.set_navigation(device, &v, &i);
                    }
                }
                Ok((
                    Self::Terrain(Box::new(renderer), Box::new(world)),
                    OrbitCamera::framing(lo, hi),
                    status,
                ))
            }
            Subject::Mesh(name) => {
                let model = model::load_mesh(assets, name)?;
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
            Self::Terrain(r, world) => r.render(device, queue, size, view_proj, world),
            Self::Model(r) => r.render(device, queue, size, view_proj),
        }
    }

    fn target(&self) -> Option<&Target> {
        match self {
            Self::Terrain(r, _) => r.target.as_ref(),
            Self::Model(r) => r.target.as_ref(),
        }
    }
}

fn main() -> Result<(), Error> {
    // Engine messages at info and above unless RUST_LOG says otherwise.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
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
    let assets = AssetCache::new(data_dir);
    let (subject, size) = (&args.subject, args.size);
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    log::info!("adapter: {}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let sections = match subject {
        Subject::Region(name) if args.sections.is_empty() => terrain::default_sections(name),
        _ => args.sections.clone(),
    };
    let (mut scene, mut camera, status) =
        Scene::load(&assets, wld, subject, &sections, &device, &queue)?;
    if let Scene::Terrain(renderer, world) = &mut scene {
        renderer.show_nav = args.nav;
        if let (true, Some(heights)) = (args.ledges, &world.heights) {
            let near = args.camera.map(|c| glam::Vec2::new(c[0], c[1]));
            for (layer, grid) in world.walk.iter().enumerate() {
                if let (Some(n), Some(colliders), 0) = (near, &world.colliders, layer) {
                    for dy in -6..=6 {
                        for dx in -6..=6 {
                            let p = n + glam::Vec2::new(dx as f32, dy as f32) * walk::CELL;
                            log::info!(
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
                        log::info!("ledge layer {layer} at {p}: {lo:.2} .. {hi:.2}");
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
    if let (Some(story), Scene::Terrain(_renderer, world)) = (&mut story, &mut scene) {
        story.place(&mut world.npcs, &assets);
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
    if let (true, Scene::Terrain(renderer, world)) = (args.walk, &mut scene)
        && let Some(mut hero) = spawn_hero(
            renderer,
            world,
            &assets,
            &device,
            &queue,
            camera.target.truncate(),
            args.camera.map(|c| c[2]),
        )?
    {
        if let Some([x, y, z]) = args.walk_to {
            let goal = glam::Vec2::new(x, y);
            let floor = (!z.is_nan()).then_some(z);
            if let Some(h) = &world.heights {
                log::info!(
                    "walk goal stands at {:?} (layer, spot, floor)",
                    walk::stand_at(&world.walk, h, goal, floor)
                );
                for (l, grid) in world.walk.iter().enumerate() {
                    log::info!(
                        "  layer {l}: walkable {}, floor {:?}",
                        grid.walkable(goal),
                        grid.height(goal, h)
                    );
                }
                if let Some(c) = &world.colliders {
                    log::info!("  mesh surfaces there: {:?}", c.surfaces(goal));
                    // CULLIS_PROBE=x,y: sample the line from there to the goal.
                    if let Some(from) = std::env::var("CULLIS_PROBE").ok().and_then(|v| {
                        let f: Vec<f32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
                        (f.len() == 2).then(|| glam::Vec2::new(f[0], f[1]))
                    }) {
                        let steps = (from.distance(goal) / 0.5) as usize;
                        for k in 0..=steps {
                            let q = from.lerp(goal, k as f32 / steps.max(1) as f32);
                            let walkable: Vec<(usize, f32)> = world
                                .walk
                                .iter()
                                .enumerate()
                                .filter_map(|(l, g)| Some((l, g.height(q, h)?)))
                                .collect();
                            let floors: Vec<String> =
                                c.surfaces(q).iter().map(|z| format!("{z:.1}")).collect();
                            log::info!("  probe {q:.1}: walk {walkable:?} floors {floors:?}");
                        }
                    }
                }
            }
            let mut highest = hero.position.z;
            // With --use: walk, use the nearest door, let it swing, walk on.
            for pass in 0..1 + usize::from(args.use_door) {
                if pass == 1 {
                    match world.nearest_door(hero.position) {
                        Some(d) => {
                            let open = world.toggle_door(d);
                            log::info!("used door {d:?} (open: {open:?})");
                            // Let it swing.
                            for _ in 0..(3.0 / TICK) as u32 {
                                world.tick(TICK);
                            }
                        }
                        None => log::info!("no door within reach"),
                    }
                }
                // Follow a route there, or head straight for it if there is none.
                let mut follow = walk::Follow::to_floor(goal, floor);
                for _ in 0..6000 {
                    let Some(heights) = &world.heights else {
                        break;
                    };
                    let (doors, npcs) = (&**world, &world.npcs);
                    let Some(to) = follow.direction(
                        &hero,
                        &world.walk,
                        heights,
                        TICK,
                        |p| blocked_for_route(doors, npcs, p),
                        |p| blocked_for_hero(doors, npcs, p),
                    ) else {
                        break;
                    };
                    hero.step_blocked(to, walk::WALK_SPEED, TICK, &world.walk, heights, |p| {
                        blocked_for_hero(doors, npcs, p)
                    });
                    highest = highest.max(hero.position.z);
                }
                log::info!("walk pass {pass} ended at {}", hero.position);
                log::warn!(
                    "{}",
                    if follow.routed() {
                        "followed a route"
                    } else {
                        "no route; walked straight"
                    }
                );
            }
            camera.yaw = hero.heading - std::f32::consts::FRAC_PI_2;
            log::info!(
                "walked to {} on layer {} (highest {highest:.1})",
                hero.position,
                hero.layer
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
                let near = story::nearest_person(&world.npcs, at, story::TALK_REACH, None);
                let (Some(story), Some(i)) = (&mut story, near) else {
                    log::info!("nobody to talk to");
                    continue;
                };
                story.talk(&world.npcs, i, at, &text, &mut subs);
                let who = world.npcs[i].script_name.as_deref().unwrap_or("?");
                let mut effects = Vec::new();
                loop {
                    subs.step(0.0, &mut effects);
                    if let Some((line, _)) = subs.line() {
                        log::info!("{who}: {}: {}", line.speaker, line.text);
                        subs.advance();
                    } else {
                        if let Some(question) = subs.asking() {
                            log::info!("{who} asks: {question}");
                        }
                        break;
                    }
                }
                subs.clear();
            }
        }
        pending_exit = exit_at(renderer, &hero);
        if pending_exit.is_none() {
            pose_and_frame(renderer, world, &assets, &queue, &mut camera, &hero, args)?;
        }
    }
    // Walked into a region exit: load where it leads, as the game would.
    if let Some(connection) = pending_exit {
        let gtg = load_gtg(data_dir)?;
        let (region, at, forward) =
            resolve_exit(wld, &gtg, connection).ok_or("exit leads nowhere")?;
        log::info!("region exit -> {region} at {at}");
        let sections = terrain::default_sections(&region);
        scene = Scene::load(
            &assets,
            wld,
            &Subject::Region(region),
            &sections,
            &device,
            &queue,
        )?
        .0;
        let Scene::Terrain(renderer, world) = &mut scene else {
            return Err("not a region".into());
        };
        let mut hero = spawn_hero(
            renderer,
            world,
            &assets,
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
        pose_and_frame(renderer, world, &assets, &queue, &mut camera, &hero, args)?;
    }
    if let (Some((role, shape)), Scene::Terrain(_renderer, world)) = (&args.mouth, &mut scene) {
        let mut bank = skin::AnimBank::open(&assets)?;
        match world
            .npcs
            .iter_mut()
            .find(|n| n.script_name.as_deref() == Some(role.as_str()))
        {
            Some(npc) => {
                npc.mouth(&mut bank, &[(shape.as_str(), 1.0)]);
                log::info!("mouth {role}: {shape}, face bones {}", npc.has_face());
            }
            None => log::info!("mouth: no creature with role {role}"),
        }
    }
    if let (Some(role), Scene::Terrain(_renderer, world)) = (&args.gesture, &mut scene) {
        let mut bank = skin::AnimBank::open(&assets)?;
        match world
            .npcs
            .iter_mut()
            .find(|n| n.script_name.as_deref() == Some(role.as_str()))
        {
            Some(npc) => {
                let ids = npc.gesture_ids(None, "CONVERSATION_HAPPY");
                let played = ids.first().is_some_and(|&id| npc.gesture(&mut bank, id));
                log::info!("gesture {role}: {ids:?}, played {played}");
            }
            None => log::info!("gesture: no creature with role {role}"),
        }
    }
    if let Scene::Terrain(_renderer, world) = &mut scene {
        let (_layers, _heights) = (&world.walk, world.heights.as_ref());
        let steps = (args.npc_time / TICK) as u32;
        for _ in 0..steps {
            world.step_creatures(TICK, None, PERSON_RADIUS);
        }
        for _ in 0..steps {
            world.tick(TICK);
        }
    }
    if let (Scene::Terrain(_renderer, world), true) = (&scene, args.npc_time > 0.0) {
        let wandered: Vec<String> = world
            .npcs
            .iter()
            .filter_map(|n| n.wandered())
            .map(|d| format!("{d:.1}"))
            .collect();
        log::info!(
            "{} wandering creatures, distances from home after {} s: {wandered:?}",
            wandered.len(),
            args.npc_time
        );
    }
    log::info!("{subject:?}: {status}");
    if let Scene::Terrain(renderer, world) = &mut scene {
        renderer.eye = camera.eye();
        world.focus = camera.target;
        // Fires already burning when the picture is taken.
        if let Some(p) = &mut renderer.particles {
            for _ in 0..240 {
                p.system.update(TICK, camera.target);
            }
        }
        if args.no_lights {
            world.point_lights.clear();
        }
        world.set_hour(args.hour, args.brightness, args.highlights);
    }
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
    log::info!("wrote {}", path.display());
    Ok(())
}

/// How wide a person is, for keeping the Hero and creatures apart.
const PERSON_RADIUS: f32 = 0.35;

/// Where a route should not go: where the Hero may not stand, with a margin
/// around people so that walking between the route's points does not graze them.
fn blocked_for_route(doors: &World, npcs: &[engine_world::creature::Npc], p: glam::Vec2) -> bool {
    blocked_for_hero(doors, npcs, p)
        || npcs
            .iter()
            .any(|n| !n.hidden && n.position().distance(p) < PERSON_RADIUS * 2.0 + walk::CELL)
}

/// Whether the Hero may not stand at `p`: a closed door, or a creature there.
fn blocked_for_hero(doors: &World, npcs: &[engine_world::creature::Npc], p: glam::Vec2) -> bool {
    doors.door_blocks(p)
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
    world: &World,
    assets: &AssetCache,
    queue: &wgpu::Queue,
    camera: &mut OrbitCamera,
    hero: &walk::Hero,
    args: &Args,
) -> Result<(), Error> {
    let mut clip = usize::from(args.walk_to.is_some());
    if let (Some(name), Some(animator)) = (&args.anim, &mut renderer.hero_animator) {
        clip = match HERO_CLIPS.iter().position(|c| c == name) {
            Some(i) => i,
            None => {
                let clip = skin::Clip::load(assets, name, animator.skeleton())?;
                animator.add_clip(clip)
            }
        };
    }
    if let Some(animator) = &renderer.hero_animator {
        for (i, name) in HERO_CLIPS.iter().enumerate() {
            let speed =
                animator.speed(i).unwrap_or(0.0) * model::MESH_SCALE * animator.stride_ratio();
            log::info!("{name}: {speed:.2} m/s");
        }
    }
    animate_hero(renderer, queue, clip, args.anim_time);
    if let Some(animator) = &renderer.hero_animator {
        log::info!(
            "posed with {:?} at {}s",
            animator.clip_name(),
            args.anim_time
        );
    }
    renderer.update_interior(hero.position);
    if let Some(i) = renderer.inside {
        log::info!("inside building {i} (roof hidden)");
    }
    let distance = camera.distance.clamp(3.0, 12.0);
    follow(camera, hero, renderer, world, distance);
    log::info!("hero at {}", hero.position);
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
    world: &World,
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
    let animations = renderer.hero_animations.clone();
    let configs = model::bone_configs(assets.data_dir(), &renderer.hero_morphs);
    renderer.hero_animator = skin.and_then(|mut skin| {
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
            Err(e) => log::warn!("kick: {e}"),
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
fn follow(
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
                    app.hour = args.hour;
                    app.brightness = args.brightness;
                    app.highlights = args.highlights;
                    app.auto_walk = args.walk_to.map(|[x, y, z]| {
                        let height = (!z.is_nan()).then_some(z);
                        walk::Follow::to_floor(glam::Vec2::new(x, y), height)
                    });
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
    /// Sound output and the voices of lines, if they could be opened.
    audio: Option<audio::Audio>,
    voices: Option<audio::Voices>,
    music: soundscape::Music,
    ambience: soundscape::Ambience,
    /// Volumes set in the side panel: overall, voices, music, ambience.
    volumes: [f32; 4],
    /// Time of day in hours, and whether it moves on by itself.
    hour: f32,
    clock: bool,
    brightness: f32,
    highlights: f32,
    /// Animations, opened when first needed (gestures).
    anim_bank: Option<skin::AnimBank>,
    /// A mouth moving to a voice: the creature, the line's id, its lip sync
    /// track, and seconds since the voice began.
    lip: Option<(usize, u32, fable_data::lipsync::LipSync, f32)>,
    /// Someone gesturing as they speak: the creature, the line's id, the
    /// gestures to pick from, and seconds since the line began; and a counter
    /// for picking.
    talking: Option<(usize, u32, Vec<u32>, f32)>,
    /// The conversation camera: how far the view has eased into it (0..=1),
    /// and its shot (kept while easing back out).
    conversation: f32,
    conversation_camera: Option<OrbitCamera>,
    gesture_picks: u64,
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
    /// The game's archives, opened once; background loads take clones.
    assets: AssetCache,
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
    /// Black over the view, fading out after a night passes (1 opaque).
    fade: f32,
    /// Turns frame times into fixed simulation ticks.
    time: engine_core::time::Clock,
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
        let assets = AssetCache::new(&data_dir);
        let (mut scene, camera, status) =
            Scene::load(&assets, &wld, &subject, &sections, &rs.device, &rs.queue)?;
        // The region's quest, if it has one, with its people placed.
        let story = match &subject {
            Subject::Region(name) => story::Story::for_region(name, &sections),
            Subject::Mesh(_) => None,
        };
        let mut story = story;
        if let (Some(story), Scene::Terrain(_renderer, world)) = (&mut story, &mut scene) {
            story.place(&mut world.npcs, &assets);
        }
        let list = match subject {
            Subject::Region(_) => ListMode::Regions,
            Subject::Mesh(_) => ListMode::Meshes,
        };
        let mut textures = model::TextureBank::open(&assets);
        let text = fable_data::strings::TextBank::open(&data_dir, "English")
            .map_err(|e| log::warn!("text: {e}"))
            .ok();
        let audio = audio::Audio::start()
            .map_err(|e| log::warn!("audio: {e}"))
            .ok();
        let voices = text.as_ref().and_then(|t| {
            audio::Voices::open(&data_dir, t)
                .map_err(|e| log::warn!("voices: {e}"))
                .ok()
        });
        if let Some(v) = &voices {
            log::info!("voices: {} lines voiced", v.len());
        }
        let text_names = text
            .as_ref()
            .map(|t| t.names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        let gtg = load_gtg(&data_dir)?;
        let texture_names = textures.names();
        Ok(Self {
            assets,
            wld,
            mesh_names,
            texture_names,
            texture_view: None,
            text,
            text_names,
            text_view: None,
            walker: None,
            fade: 0.0,
            time: engine_core::time::Clock::new(),
            follow_distance: 6.0,
            gtg,
            pending_exit: None,
            loading: None,
            story,
            subs: talk::Subtitles::default(),
            audio,
            voices,
            music: Default::default(),
            ambience: Default::default(),
            anim_bank: None,
            lip: None,
            talking: None,
            conversation: 0.0,
            conversation_camera: None,
            gesture_picks: 0,
            hour: DEFAULT_HOUR,
            clock: false,
            brightness: lighting::BRIGHTNESS,
            highlights: lighting::HIGHLIGHT_LIMIT,
            volumes: [
                1.0,
                1.0,
                soundscape::MUSIC_VOLUME,
                soundscape::AMBIENCE_VOLUME,
            ],
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
        let (assets, wld, device, queue) = (
            self.assets.clone(),
            self.wld.clone(),
            rs.device.clone(),
            rs.queue.clone(),
        );
        let job = subject.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Loaded, Error> {
                let (mut scene, mut camera, status) =
                    Scene::load(&assets, &wld, &job, &sections, &device, &queue)?;
                let mut hero = None;
                if let (Some((at, forward)), Scene::Terrain(renderer, world)) =
                    (arrival, &mut scene)
                {
                    let mut h = spawn_hero(
                        renderer,
                        world,
                        &assets,
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
                if let (Some(story), Scene::Terrain(_renderer, world)) =
                    (&mut self.story, &mut self.scene)
                {
                    story.place(&mut world.npcs, &self.assets);
                }
                if let (Some(hero), Scene::Terrain(renderer, world)) =
                    (loaded.hero, &mut self.scene)
                {
                    follow(
                        &mut self.camera,
                        &hero,
                        renderer,
                        world,
                        self.follow_distance,
                    );
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
        if let Scene::Terrain(renderer, world) = &mut self.scene {
            ui.checkbox(&mut renderer.show_nav, "Show navigation");
            ui.checkbox(&mut self.want_walk, "Walk (WASD, Shift runs)");
            egui::CollapsingHeader::new("Time of day").show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.hour, 0.0..=23.99)
                        .text("Hour")
                        .custom_formatter(|h, _| {
                            format!("{:02}:{:02}", h as u32, (h.fract() * 60.0) as u32)
                        }),
                );
                ui.checkbox(&mut self.clock, "Clock runs (a day in 24 minutes)");
                ui.add(egui::Slider::new(&mut self.brightness, 0.5..=2.5).text("Brightness"));
                ui.add(egui::Slider::new(&mut self.highlights, 0.8..=3.0).text("Highlight limit"));
                if let Some(l) = &world.lighting {
                    ui.label(format!("Environment: {}", l.outside.0));
                }
            });
            if let Some(audio) = &self.audio {
                egui::CollapsingHeader::new("Sound").show(ui, |ui| {
                    let labels = ["Overall", "Voices", "Music", "Ambience"];
                    for (k, label) in labels.iter().enumerate() {
                        let slider = egui::Slider::new(&mut self.volumes[k], 0.0..=1.0)
                            .text(*label)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                        if ui.add(slider).changed() {
                            let v = self.volumes[k];
                            match k {
                                0 => audio.set_volume(v),
                                1 => audio.set_channel_volume(engine_audio::Channel::Voice, v),
                                2 => audio.set_channel_volume(engine_audio::Channel::Music, v),
                                _ => audio.set_channel_volume(engine_audio::Channel::Ambience, v),
                            }
                        }
                    }
                });
            }
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
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        match (self.want_walk, self.walker.is_some()) {
            (true, false) => {
                match spawn_hero(
                    renderer,
                    world,
                    &self.assets,
                    &rs.device,
                    &rs.queue,
                    self.camera.target.truncate(),
                    None,
                ) {
                    Ok(Some(hero)) => {
                        follow(
                            &mut self.camera,
                            &hero,
                            renderer,
                            world,
                            self.follow_distance,
                        );
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
    /// One simulation tick ([`TICK`] seconds): the Hero, what is being said
    /// and its effects, gestures and mouths, creatures, doors, the clock and
    /// particles.
    fn tick(&mut self, rs: &egui_wgpu::RenderState, actions: &mut Actions) {
        let dt = TICK;
        self.walk(actions, dt, &rs.queue);
        let mut effects = Vec::new();
        self.subs.step(dt, &mut effects);
        self.gesture(dt);
        self.move_mouth(dt);
        for effect in effects {
            self.apply(rs, effect);
        }
        if self.clock {
            self.hour = (self.hour + dt * CLOCK_RATE).rem_euclid(24.0);
        }
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        let (_layers, _heights) = (&world.walk, world.heights.as_ref());
        let hero = self.walker.as_ref().map(|h| h.position.truncate());
        world.step_creatures(dt, hero, PERSON_RADIUS);
        world.tick(dt);
        let focus = world.focus;
        if let Some(p) = &mut renderer.particles {
            p.system.update(dt, focus);
        }
    }

    /// One simulation tick of the walking Hero: `input` this frame's actions
    /// (presses are taken, so a frame running two ticks acts once), `dt` the
    /// tick's length.
    fn walk(&mut self, input: &mut Actions, dt: f32, queue: &wgpu::Queue) {
        if self.loading.is_some() {
            return;
        }
        let (Some(hero), Scene::Terrain(renderer, world)) = (&mut self.walker, &mut self.scene)
        else {
            return;
        };
        let (walk, Some(heights)) = (&world.walk, &world.heights) else {
            return;
        };
        let (forward, right, run) = (input.forward, input.right, input.run);
        let view = (self.camera.target - self.camera.eye())
            .truncate()
            .normalize_or_zero();
        let side = glam::Vec2::new(view.y, -view.x);
        let mut direction = view * forward + side * right;
        // A walk to a point (`--walk-to`) along a route, while no key steers.
        // In a conversation or a quest's closing scene the Hero stays put.
        let held = self.subs.in_scene();
        if held {
            direction = glam::Vec2::ZERO;
        }
        if direction == glam::Vec2::ZERO
            && !held
            && let Some(follow) = &mut self.auto_walk
        {
            let (doors, npcs) = (&**world, &world.npcs);
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
                    let goal = follow.goal;
                    self.auto_walk = None;
                    let at = hero.position.truncate();
                    log::info!(
                        "walk to {goal}: ended at {} ({:.1} away)",
                        hero.position,
                        at.distance(goal)
                    );
                    if std::mem::take(&mut self.auto_talk)
                        && let (Some(text), Some(story)) = (&self.text, &mut self.story)
                        && let Some(i) =
                            story::nearest_person(&world.npcs, at, story::TALK_REACH, None)
                    {
                        story.talk(&world.npcs, i, at, text, &mut self.subs);
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
        let (talk_key, kick_key, answer_key) = (
            std::mem::take(&mut input.use_),
            std::mem::take(&mut input.kick),
            input.answer.take(),
        );
        let mut use_door = None;
        // E: go on with what is being said, else talk to someone near, else use a door.
        if talk_key && !self.subs.advance() && self.subs.asking().is_none() {
            let near = story::nearest_person(&world.npcs, at, story::TALK_REACH, None);
            if let (Some(story), Some(text), Some(i)) = (&mut self.story, &self.text, near) {
                story.talk(&world.npcs, i, at, text, &mut self.subs);
            } else {
                // Used at the end of the step, once the walk grids are free.
                use_door = world.nearest_door(hero.position);
            }
        }
        // Y/N (or a click): answer the question on screen.
        if let Some(yes) = answer_key.or(self.clicked_answer.take())
            && self.subs.asking().is_some()
            && let (Some(story), Some(text)) = (&mut self.story, &self.text)
        {
            story.answer(yes, &world.npcs, at, text, &mut self.subs);
        }
        // F: kick whoever is in front.
        if kick_key
            && !held
            && !self.kicking
            && let (Some(kick), Some(animator)) = (renderer.hero_kick, &mut renderer.hero_animator)
        {
            animator.play_once(kick, 0.0);
            self.kicking = true;
            let facing = glam::Vec2::new(-hero.heading.sin(), hero.heading.cos());
            if let Some(i) = story::nearest_person(&world.npcs, at, story::KICK_REACH, Some(facing))
            {
                world.npcs[i].flinch();
                if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
                    story.hit(&world.npcs, i, at, text, &mut self.subs);
                }
            }
        }
        if self.kicking {
            direction = glam::Vec2::ZERO;
        }
        // Walking away ends a conversation.
        if let Some(i) = self.subs.waiting_on()
            && world
                .npcs
                .get(i)
                .is_none_or(|n| n.position().distance(at) > 4.0)
        {
            self.subs.walked_away();
        }
        if let (Some(story), Some(text)) = (&mut self.story, &self.text) {
            let height = hero.position.z;
            story.update(dt, &world.npcs, (at, height), text, &mut self.subs);
        }
        let (doors, npcs) = (&**world, &world.npcs);
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
        renderer.update_interior(hero.position);
        follow(
            &mut self.camera,
            hero,
            renderer,
            world,
            self.follow_distance,
        );
        match exit_at(renderer, hero) {
            Some(connection) if self.exits_armed => self.pending_exit = Some(connection),
            Some(_) => {}
            None => self.exits_armed = true,
        }
        if let Some(d) = use_door {
            world.toggle_door(d);
        }
    }

    /// Plays the region's music and the ambience where the Hero (or, when not
    /// walking, the camera) is.
    fn play_soundscape(&mut self, dt: f32) {
        let Some(audio) = &self.audio else {
            return;
        };
        // While a region loads, what plays carries on.
        if self.loading.is_some() {
            return;
        }
        let (music, bank, night, indoors) = match &self.scene {
            Scene::Terrain(r, world) => {
                let at = self
                    .walker
                    .as_ref()
                    .map_or(self.camera.target.truncate(), |h| h.position.truncate());
                // Night when the sun's light is low (as the lighting reckons it).
                let sun = world.light.sun.iter().sum::<f32>() / 3.0;
                (
                    r.soundscape.music.clone(),
                    r.soundscape.bank_at(at).cloned(),
                    world.lighting.is_some() && sun < 0.3,
                    r.inside.is_some(),
                )
            }
            Scene::Model(_) => (None, None, false, false),
        };
        self.music.update(music.as_deref(), audio);
        self.ambience
            .update(dt, bank.as_deref(), night, indoors, audio);
    }

    /// Picks what glows as usable: whoever E would talk to (someone the quest
    /// has something for, in reach), else the door E would use. Green, gently
    /// pulsing; nothing during a scene or when not walking. Also tells the
    /// renderer where the camera is.
    fn choose_highlight(&mut self, time: f32) {
        let shown = self.shown_camera();
        let Scene::Terrain(renderer, world) = &mut self.scene else {
            return;
        };
        renderer.eye = shown.eye();
        world.focus = shown.target;
        renderer.highlight = None;
        let Some(hero) = &self.walker else {
            return;
        };
        if self.subs.in_scene() || self.subs.asking().is_some() {
            return;
        }
        let at = hero.position.truncate();
        let person = self.story.as_ref().and_then(|story| {
            story::nearest_person(&world.npcs, at, story::TALK_REACH, None).filter(|&i| {
                world.npcs[i]
                    .script_name
                    .as_deref()
                    .is_some_and(|r| story.knows(r))
            })
        });
        let what = match person {
            Some(i) => Some(render::Highlight::Npc(i)),
            None => world
                .nearest_door(hero.position)
                .map(render::Highlight::Door),
        };
        let strength = 0.5 + 0.15 * (time * 4.0).sin();
        renderer.highlight = what.map(|w| (w, [0.3, 1.0, 0.4, strength]));
    }

    /// The camera the view uses: the orbit or follow camera, eased towards the
    /// conversation camera while someone talks to the Hero.
    fn shown_camera(&self) -> OrbitCamera {
        match &self.conversation_camera {
            Some(shot) => {
                conversation::blend(&self.camera, shot, conversation::ease(self.conversation))
            }
            None => self.camera,
        }
    }

    /// While a conversation line or question is on screen: the shot its line
    /// names (or over the Hero's shoulder onto the speaker), easing in over
    /// half a second, with the speaker and the Hero turned to each other.
    /// Afterwards it eases back to the follow camera.
    fn frame_conversation(&mut self, dt: f32) {
        const EASE: f32 = 0.5;
        let active = match (self.subs.waiting_on(), &mut self.walker, &mut self.scene) {
            (Some(talked), Some(hero), Scene::Terrain(renderer, world)) => {
                let speaker = self.subs.speaker().unwrap_or(talked);
                let tags = self
                    .subs
                    .line()
                    .map(|(l, _)| l.tags.clone())
                    .unwrap_or_default();
                match world.npcs.get_mut(speaker) {
                    Some(npc) => {
                        npc.turn_towards(hero.position.truncate(), dt);
                        // The Hero turns to the speaker too.
                        let d = npc.position() - hero.position.truncate();
                        if d.length() > 0.1 {
                            let want = (-d.x).atan2(d.y);
                            let diff = (want - hero.heading + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI;
                            hero.heading += diff.clamp(-6.0 * dt, 6.0 * dt);
                            if let Some((_, transform)) = &mut renderer.hero {
                                *transform = hero_transform(hero);
                            }
                        }
                        let speaker = conversation::Actor {
                            feet: npc.feet(),
                            face_height: npc.face_height(),
                        };
                        let speakee = conversation::Actor {
                            feet: hero.position,
                            face_height: 1.05,
                        };
                        let shot = conversation::Shot::from_tags(&tags)
                            .unwrap_or(conversation::Shot::DEFAULT);
                        self.conversation_camera = Some(shot.camera(speaker, speakee));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        };
        if active {
            self.conversation = (self.conversation + dt / EASE).min(1.0);
        } else {
            self.conversation = (self.conversation - dt / EASE).max(0.0);
            if self.conversation == 0.0 {
                self.conversation_camera = None;
            }
        }
    }

    /// Has whoever says the line on screen gesture while they say it: a gesture
    /// the line asks for, or one for its mood, then another as each ends, until
    /// the line is done.
    fn gesture(&mut self, dt: f32) {
        let Scene::Terrain(_renderer, world) = &mut self.scene else {
            return;
        };
        let line = self.subs.line().map(|(l, _)| (l.id, l.tags.clone()));
        let speaker = self.subs.speaker();
        // A new line: who says it, and the gestures to pick from.
        if let (Some((id, tags)), Some(i)) = (&line, speaker)
            && self.talking.as_ref().is_none_or(|t| t.1 != *id)
            && let Some(npc) = world.npcs.get(i)
        {
            let (slot, mood) = talk::gesture_of(tags);
            self.talking = Some((i, *id, npc.gesture_ids(slot, mood), 0.0));
            // A fresh line starts a fresh gesture.
            self.gesture_picks += 1;
        }
        let saying = self.subs.saying_time();
        let Some((i, id, choices, since)) = &mut self.talking else {
            return;
        };
        if line.as_ref().is_none_or(|(l, _)| l != id) {
            self.talking = None;
            return;
        }
        // No new gesture once the line has been said (a line waiting for E stays
        // on screen after).
        *since += dt;
        if *since >= saying {
            return;
        }
        let Some(npc) = world.npcs.get_mut(*i) else {
            return;
        };
        if choices.is_empty() || npc.gesturing() {
            return;
        }
        if self.anim_bank.is_none() {
            self.anim_bank = skin::AnimBank::open(&self.assets)
                .map_err(|e| log::warn!("animations: {e}"))
                .ok();
        }
        let Some(bank) = &mut self.anim_bank else {
            return;
        };
        self.gesture_picks = self
            .gesture_picks
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let pick = choices[(self.gesture_picks >> 33) as usize % choices.len()];
        npc.gesture(bank, pick);
    }

    /// Speaks a line newly on screen (holding it there while it is said), and
    /// stops speaking when no line is.
    fn speak(&mut self) {
        let Some(audio) = &self.audio else {
            return;
        };
        if let Some(id) = self.subs.next_to_voice() {
            match self.voices.as_mut().and_then(|v| v.sound(id)) {
                Some(sound) => {
                    self.subs.hold_for(sound.seconds() + 0.3);
                    audio.say(sound);
                    // The speaker's mouth follows the voice's lip sync track.
                    let track = self.voices.as_mut().and_then(|v| v.lipsync(id));
                    self.stop_mouth();
                    self.lip = self
                        .subs
                        .speaker()
                        .zip(track)
                        .map(|(who, track)| (who, id, track, 0.0));
                }
                None => audio.hush(),
            }
        } else if self.subs.line().is_none() {
            audio.hush();
        }
    }

    /// Releases the mouth of whoever was speaking.
    fn stop_mouth(&mut self) {
        if let (Some((who, ..)), Scene::Terrain(_renderer, world), Some(bank)) =
            (self.lip.take(), &mut self.scene, &mut self.anim_bank)
            && let Some(npc) = world.npcs.get_mut(who)
        {
            npc.mouth(bank, &[]);
        }
    }

    /// Moves the speaker's mouth along their line's lip sync track, `dt`
    /// seconds on; stops at its end or when the line goes.
    fn move_mouth(&mut self, dt: f32) {
        let Some((_, id, track, since)) = &mut self.lip else {
            return;
        };
        let on_screen = self.subs.line().is_some_and(|(l, _)| l.id == *id);
        *since += dt;
        if !on_screen || *since > track.seconds() {
            self.stop_mouth();
            return;
        }
        let shapes: Vec<(String, f32)> = track
            .weights_at(*since)
            .into_iter()
            .map(|(n, w)| (n.to_owned(), w))
            .collect();
        let who = self.lip.as_ref().map(|l| l.0);
        if self.anim_bank.is_none() {
            self.anim_bank = skin::AnimBank::open(&self.assets)
                .map_err(|e| log::warn!("animations: {e}"))
                .ok();
        }
        if let (Some(who), Scene::Terrain(_renderer, world), Some(bank)) =
            (who, &mut self.scene, &mut self.anim_bank)
            && let Some(npc) = world.npcs.get_mut(who)
        {
            let shapes: Vec<(&str, f32)> = shapes.iter().map(|(n, w)| (n.as_str(), *w)).collect();
            npc.mouth(bank, &shapes);
        }
    }

    /// Applies a quest beat the subtitles reached: someone leaving or coming back,
    /// or the quest ending (the region reloads in the next quest state, the Hero
    /// where he stands).
    fn apply(&mut self, rs: &egui_wgpu::RenderState, beat: engine_script::Beat) {
        use engine_script::Beat;
        match beat {
            Beat::Leave(role) | Beat::Return(role) => {
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    for npc in &mut world.npcs {
                        if npc.script_name.as_deref() == Some(role) {
                            npc.hidden = matches!(beat, Beat::Leave(_));
                        }
                    }
                }
            }
            Beat::Walk { who, to } => {
                log::info!("quest: {who} walks to {to:?}");
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    for npc in world
                        .npcs
                        .iter_mut()
                        .filter(|n| n.script_name.as_deref() == Some(who))
                    {
                        if !npc.walk_to(glam::Vec3::from_array(to)) {
                            log::warn!("{who} cannot walk");
                        }
                    }
                }
            }
            Beat::Place {
                who,
                at,
                facing: [fx, fy],
            } => {
                if let Scene::Terrain(_renderer, world) = &mut self.scene {
                    let (layers, heights) = (&world.walk, world.heights.as_ref());
                    for npc in world
                        .npcs
                        .iter_mut()
                        .filter(|n| n.script_name.as_deref() == Some(who))
                    {
                        npc.hidden = false;
                        let at = glam::Vec3::from_array(at);
                        npc.place(at, glam::Vec2::new(fx, fy), layers, heights);
                    }
                }
            }
            Beat::Sleep {
                hour,
                at,
                facing: [fx, fy],
            } => {
                log::info!("quest: night passes, waking at {at:?} at {hour}:00");
                // Black at once, fading back in on the new day.
                self.hour = hour;
                self.fade = 1.0 + NIGHT_HOLD;
                if let (Scene::Terrain(renderer, world), Some(hero)) =
                    (&mut self.scene, &mut self.walker)
                    && let Some(heights) = &world.heights
                {
                    let at = glam::Vec3::from_array(at);
                    if let Some((layer, spot, z)) =
                        walk::stand_at(&world.walk, heights, at.truncate(), Some(at.z))
                    {
                        hero.position = spot.extend(z);
                        hero.layer = layer;
                    }
                    let to = glam::Vec2::new(fx, fy) - hero.position.truncate();
                    hero.heading = (-to.x).atan2(to.y);
                    renderer.update_interior(hero.position);
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
                // The simulation runs in fixed ticks; what is only seen
                // (camera framing, highlight, fades) runs per frame.
                let frame_dt = ui.input(|i| i.stable_dt.min(0.1));
                let ticks = self.time.advance(frame_dt);
                let mut actions = ui.input(Actions::read);
                for _ in 0..ticks {
                    self.tick(&rs, &mut actions);
                }
                self.speak();
                self.frame_conversation(frame_dt);
                self.choose_highlight(ui.input(|i| i.time) as f32);
                self.play_soundscape(frame_dt);
                if self.walker.is_some() || ticks > 0 {
                    ui.ctx().request_repaint();
                }
                if let Scene::Terrain(_renderer, world) = &mut self.scene
                    && !world.npcs.is_empty()
                {
                    ui.ctx().request_repaint();
                }

                let ppp = ui.ctx().pixels_per_point();
                let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
                let view_proj = self
                    .shown_camera()
                    .view_proj(rect.width() / rect.height().max(1.0));
                if self.clock {
                    ui.ctx().request_repaint();
                }
                if let Scene::Terrain(renderer, world) = &mut self.scene {
                    world.set_hour(self.hour, self.brightness, self.highlights);
                    if renderer.particles.is_some() {
                        ui.ctx().request_repaint();
                    }
                }
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
                // Letterbox bars while the conversation camera is on.
                let bar = rect.height() * 0.11 * conversation::ease(self.conversation);
                if bar > 0.5 {
                    let black = egui::Color32::BLACK;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar)),
                        0.0,
                        black,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(rect.min.x, rect.max.y - bar),
                            egui::vec2(rect.width(), bar),
                        ),
                        0.0,
                        black,
                    );
                }
                // A night passing: black, fading back in.
                if self.fade > 0.0 {
                    let alpha = (conversation::ease(self.fade.min(1.0)) * 255.0) as u8;
                    ui.painter()
                        .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(alpha));
                    self.fade -= ui.input(|i| i.stable_dt.min(0.1)) / NIGHT_FADE;
                    ui.ctx().request_repaint();
                }
                // What is being said, notices, and the Hero's gold and items.
                if let Some(answer) = self.subs.show(ui.ctx(), rect) {
                    self.clicked_answer = Some(answer);
                }
                if let (Some(story), Some(text), true) = (
                    &self.story,
                    &self.text,
                    self.walker.is_some() && self.conversation == 0.0,
                ) {
                    story.hud(ui.ctx(), rect, text);
                }
            });
        ui.ctx().request_repaint();
    }
}
