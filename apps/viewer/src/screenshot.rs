//! The headless path: load a scene, place and walk the Hero, run creatures,
//! and write a screenshot (`--screenshot`).

use crate::*;

pub(crate) fn screenshot(
    data_dir: &Path,
    wld: &Wld,
    args: &Args,
    path: &Path,
) -> Result<(), Error> {
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
        pending_exit = exit_at(world, &hero);
        if pending_exit.is_none() {
            pose_and_frame(renderer, world, &assets, &mut camera, &hero, args)?;
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
        pose_and_frame(renderer, world, &assets, &mut camera, &hero, args)?;
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

/// Poses the Hero for a screenshot (`--anim`, or idle/walk) and frames him.
pub(crate) fn pose_and_frame(
    renderer: &mut TerrainRenderer,
    world: &mut World,
    assets: &AssetCache,
    camera: &mut OrbitCamera,
    hero: &walk::Hero,
    args: &Args,
) -> Result<(), Error> {
    let mut clip = usize::from(args.walk_to.is_some());
    if let (Some(name), Some(animator)) = (&args.anim, &mut world.hero_animator) {
        clip = match HERO_CLIPS.iter().position(|c| c == name) {
            Some(i) => i,
            None => {
                let clip = skin::Clip::load(assets, name, animator.skeleton())?;
                animator.add_clip(clip)
            }
        };
    }
    if let Some(animator) = &world.hero_animator {
        for (i, name) in HERO_CLIPS.iter().enumerate() {
            let speed =
                animator.speed(i).unwrap_or(0.0) * model::MESH_SCALE * animator.stride_ratio();
            log::info!("{name}: {speed:.2} m/s");
        }
    }
    animate_hero(world, clip, args.anim_time);
    if let Some(animator) = &world.hero_animator {
        log::info!(
            "posed with {:?} at {}s",
            animator.clip_name(),
            args.anim_time
        );
    }
    if world.update_interior(hero.position) {
        renderer.set_inside(world.inside);
    }
    if let Some(i) = world.inside {
        log::info!("inside building {i} (roof hidden)");
    }
    let distance = camera.distance.clamp(3.0, 12.0);
    follow(camera, hero, renderer, world, distance);
    log::info!("hero at {}", hero.position);
    Ok(())
}
