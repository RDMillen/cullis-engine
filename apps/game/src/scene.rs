//! What is on screen: a region (its renderer and world) or a single mesh,
//! and finding where region exits lead.

use crate::*;

/// What a window shows: a region, or (in the viewer) a single mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Region(String),
    Mesh(String),
}

/// What is on screen: a region's terrain or a single mesh.
pub enum Scene {
    /// A region: its renderer and its world.
    Terrain(Box<TerrainRenderer>, Box<engine_world::world::World>),
    Model(Box<ModelRenderer>),
}

impl Scene {
    pub fn load(
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
                let geometry = engine_render::terrain::TerrainGeometry {
                    vertices: &mesh.vertices,
                    indices: &mesh.indices,
                    themes: mesh.themes.as_ref(),
                    sea: mesh.sea_quad(),
                    nav_vertices: &mesh.nav_vertices,
                    nav_indices: &mesh.nav_indices,
                };
                let mut renderer = TerrainRenderer::new(device, queue, &geometry, themes.as_ref());
                let mut world = World {
                    walk: mesh.walk.clone(),
                    heights: mesh.heights.clone(),
                    point_lights: mesh.lights.clone(),
                    exits: mesh.exits.clone(),
                    sections: mesh.sections.iter().cloned().collect(),
                    ..World::default()
                };
                if let Some(r) = wld.region(name) {
                    world.soundscape = soundscape::Soundscape::resolve(
                        data_dir,
                        &r.region_def,
                        &defs,
                        std::mem::take(&mut mesh.sound_areas),
                    );
                    log::info!(
                        "sound: music {:?}, ambience {:?}",
                        world.soundscape.music.as_ref().and_then(|m| m.file_name()),
                        world.soundscape.bank_names()
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
                world.hero_animations = HeroAnimations::from_slots(&hero_slots);
                world.hero_morphs = hero_morphs;
                let resolve = |object: &str| {
                    defs.get(object)
                        .map(|d| defs.meshes_of(d))
                        .unwrap_or_default()
                };
                let (things, buildings) =
                    model::load_placements(assets, name, &placements, &resolve)?;
                world.interiors = buildings
                    .iter()
                    .map(|b| engine_world::world::Interior {
                        lo: b.interior_lo,
                        hi: b.interior_hi,
                    })
                    .collect();
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

    pub fn render(
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

    pub fn target(&self) -> Option<&Target> {
        match self {
            Self::Terrain(r, _) => r.target.as_ref(),
            Self::Model(r) => r.target.as_ref(),
        }
    }
}

/// Reads `Levels/FinalAlbion.gtg` (region entrances).
pub fn load_gtg(data_dir: &Path) -> Result<Gtg, Error> {
    let bytes = std::fs::read(data_dir.join("Levels/FinalAlbion.gtg"))?;
    Ok(Gtg::parse(
        &bytes.iter().map(|&b| b as char).collect::<String>(),
    )?)
}

/// Where a region exit leads: the region to load, and the entrance's world
/// position and facing.
pub fn resolve_exit(
    wld: &Wld,
    gtg: &Gtg,
    connection: u64,
) -> Option<(String, glam::Vec3, glam::Vec3)> {
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
pub fn exit_at(world: &World, hero: &walk::Hero) -> Option<u64> {
    world
        .exits
        .iter()
        .find(|e| {
            e.position.truncate().distance(hero.position.truncate()) < e.radius
                && (e.position.z - hero.position.z).abs() < 4.0
        })
        .map(|e| e.connection)
}
