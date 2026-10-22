//! Loads a static mesh with its textures and draws it into an offscreen target.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use fable_data::big::BigFile;
use fable_data::mesh::{Mesh, SKINNED_MESH_TYPE, STATIC_MESH_TYPE, Skeleton};
use fable_data::texture::{Texture, TextureInfo};
use glam::{Mat4, Vec3};

use crate::lighting::Light;
use crate::render::{COLOR_FORMAT, DEPTH_FORMAT, SKY, Target, create_target};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// An RGBA8 image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A range of `indices` drawn with one texture.
pub struct Draw {
    pub first: u32,
    pub count: u32,
    pub image: usize,
    /// Its material is a roof (named `Roof`, `roof`, `Roof fringe`, `Fringe`):
    /// hidden while the Hero is inside the building.
    pub roof: bool,
}

pub struct ModelData {
    pub name: String,
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<Draw>,
    pub images: Vec<Image>,
    pub lo: Vec3,
    pub hi: Vec3,
    /// Particle emitters started by the meshes (`CREATEPARTICLE` dummies) and
    /// by what is placed (chimney smoke, attachers), and where, in the model's
    /// space.
    pub particles: Vec<(crate::particles::EmitterRef, Vec3)>,
}

type Error = Box<dyn std::error::Error>;

/// Whether a placed mesh is part of a building, and which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshRole {
    Plain,
    /// A building's outer shell (walls and roof), hidden while the Hero is inside
    /// that building (by its Tng UID).
    Exterior(u64),
    /// A building's interior (floors, stairs, inner walls).
    Interior(u64),
}

/// A mesh placed in the world.
#[derive(Debug, Clone)]
pub struct Placement {
    pub role: MeshRole,
    /// Mesh bank entry id.
    pub mesh: u32,
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    /// Particle emitters its definition starts (chimney smoke, attachers): the
    /// emitter's number and the mesh helper it sits at (a name, or the start of
    /// one, `focus_chimney`; empty for the mesh's origin).
    pub effects: Vec<(u32, String)>,
}

impl Placement {
    /// Meshes face their local -y: the Oakvale trader bridge only spans its two
    /// plateaus this way round, and house doors then face the paths (checked against
    /// the game). So local -y maps to `forward`, -x to `right` and z to `up`.
    /// Meshes are in hundredths of a world unit (a chair is 119 units tall, a
    /// map 96 units wide), so placed meshes are scaled by [`MESH_SCALE`].
    pub fn transform(&self) -> Mat4 {
        let up = self.up.normalize_or(Vec3::Z);
        let forward = self.forward.normalize_or(Vec3::Y);
        let right = forward.cross(up).normalize_or(Vec3::X);
        Mat4::from_cols(
            (-right * MESH_SCALE).extend(0.0),
            (-forward * MESH_SCALE).extend(0.0),
            (up * MESH_SCALE).extend(0.0),
            self.position.extend(1.0),
        )
    }
}

/// World units per mesh unit.
pub const MESH_SCALE: f32 = 0.01;

/// A mesh's most detailed level, in its own space, with image indices into the model.
struct LocalMesh {
    vertices: Vec<ModelVertex>,
    indices: Vec<u32>,
    draws: Vec<Draw>,
    /// Skinned meshes: per vertex, 4 skeleton bones and weights, and the skeleton.
    skin: Option<Skin>,
    /// Objects the mesh creates (`CREATEOBJECT` dummies): definition name and
    /// where, in the mesh's own space.
    creates: Vec<(String, Mat4)>,
    /// Particle emitters it starts (`CREATEPARTICLE` dummies), likewise.
    particles: Vec<(String, Vec3)>,
    /// Its helpers (attachment points: `focus_chimney_01`, `FLARE_POINT`) and
    /// where, likewise.
    helpers: Vec<(String, Vec3)>,
}

/// What a skinned mesh needs to be posed: per vertex, 4 skeleton bone indices and
/// weights (summing to 1), and the skeleton.
#[derive(Clone)]
pub struct Skin {
    pub influences: Vec<([u16; 4], [f32; 4])>,
    pub skeleton: Skeleton,
    /// Per bone, a scale from the creature's bone configs (`Bones/*.bncfg`):
    /// along the bone and across it. Empty for none.
    pub morph: Vec<[f32; 3]>,
}

/// Reads bone configs by file name from `Bones/` (unreadable ones are skipped
/// with a message).
pub fn bone_configs(data_dir: &Path, files: &[String]) -> Vec<fable_data::bncfg::BoneConfig> {
    files
        .iter()
        .filter_map(|f| {
            let text = std::fs::read(data_dir.join("Bones").join(f))
                .map_err(|e| eprintln!("{f}: {e}"))
                .ok()?;
            fable_data::bncfg::BoneConfig::parse(&String::from_utf8_lossy(&text))
                .map_err(|e| eprintln!("{f}: {e}"))
                .ok()
        })
        .collect()
}

impl Skin {
    /// How much the morph lowers the hips: the pelvis's rest height with the morph
    /// over without (1 with no morph). Animations made for full-size skeletons
    /// cover that much less ground.
    pub fn stride_ratio(&self) -> f32 {
        let pelvis = |morph: &[[f32; 3]]| {
            let i = self
                .skeleton
                .bones
                .iter()
                .position(|b| b.name == "Bip01 Pelvis")?;
            let m = crate::skin::pose(&self.skeleton, None, 0.0, morph)[i];
            // The bone's origin: the inverse bind maps it from model space.
            let bind = crate::skin::bind_origin(&self.skeleton.bones[i]);
            Some(m.transform_point3(bind).z)
        };
        match (pelvis(&self.morph), pelvis(&[])) {
            (Some(a), Some(b)) if b.abs() > 1e-3 => a / b,
            _ => 1.0,
        }
    }

    /// Applies bone configs: each bone's scale is the product of the configs'
    /// scales for it.
    pub fn set_morph(&mut self, configs: &[fable_data::bncfg::BoneConfig]) {
        self.morph = self
            .skeleton
            .bones
            .iter()
            .map(|b| {
                configs
                    .iter()
                    .filter_map(|c| c.scale(&b.name))
                    .fold([1.0; 3], |a, s| [a[0] * s[0], a[1] * s[1], a[2] * s[2]])
            })
            .collect();
    }
}

/// Accumulates meshes, placed or not, into one [`ModelData`], loading each mesh
/// and texture once.
pub struct ModelBuilder {
    file: BufReader<File>,
    entries: HashMap<u32, fable_data::bank::Entry>,
    textures: TextureBank,
    images: HashMap<u32, usize>,
    meshes: HashMap<u32, Option<LocalMesh>>,
    model: ModelData,
    /// Textures to use in place of those materials name (creature part options).
    swaps: HashMap<u32, u32>,
}

impl ModelBuilder {
    pub fn open(data_dir: &Path, name: &str) -> Result<Self, Error> {
        let mut file = BufReader::new(File::open(data_dir.join("graphics/graphics.big"))?);
        let big = BigFile::read(&mut file)?;
        let index = big
            .bank("MBANK_ALLMESHES")
            .ok_or("no MBANK_ALLMESHES")?
            .read_index(&mut file)?;
        let entries = index
            .entries
            .into_iter()
            .filter(|e| [STATIC_MESH_TYPE, SKINNED_MESH_TYPE].contains(&e.entry_type))
            .map(|e| (e.id, e))
            .collect();
        Ok(Self {
            file,
            entries,
            textures: TextureBank::open(data_dir)?,
            images: HashMap::new(),
            meshes: HashMap::new(),
            swaps: HashMap::new(),
            model: ModelData {
                name: name.to_owned(),
                vertices: Vec::new(),
                indices: Vec::new(),
                draws: Vec::new(),
                images: vec![Image {
                    width: 1,
                    height: 1,
                    rgba: vec![255; 4],
                }],
                lo: Vec3::splat(f32::INFINITY),
                hi: Vec3::splat(f32::NEG_INFINITY),
                particles: Vec::new(),
            },
        })
    }

    /// Id of the first mesh named `needle`, or containing it (case-insensitive).
    pub fn find(&self, needle: &str) -> Option<(u32, String)> {
        let needle = needle.to_ascii_uppercase();
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|e| e.id);
        entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&needle))
            .or_else(|| entries.iter().find(|e| e.name.contains(&needle)))
            .map(|e| (e.id, e.name.clone()))
    }

    /// Adds mesh `id` transformed by `transform`. Returns false if the mesh could
    /// not be loaded. Skinned meshes are drawn in their bind pose.
    pub fn add(&mut self, id: u32, transform: Mat4) -> bool {
        if !self.meshes.contains_key(&id) {
            let local = self.load(id).map_err(|e| eprintln!("mesh {id}: {e}")).ok();
            self.meshes.insert(id, local);
        }
        let Some(Some(local)) = self.meshes.get(&id) else {
            return false;
        };
        let base = self.model.vertices.len() as u32;
        let first = self.model.indices.len() as u32;
        self.model.vertices.extend(local.vertices.iter().map(|v| {
            ModelVertex {
                position: transform
                    .transform_point3(Vec3::from_array(v.position))
                    .to_array(),
                normal: transform
                    .transform_vector3(Vec3::from_array(v.normal))
                    .normalize_or_zero()
                    .to_array(),
                uv: v.uv,
            }
        }));
        self.model
            .indices
            .extend(local.indices.iter().map(|i| base + i));
        self.model
            .particles
            .extend(local.particles.iter().map(|(name, p)| {
                (
                    crate::particles::EmitterRef::Name(name.clone()),
                    transform.transform_point3(*p),
                )
            }));
        self.model.draws.extend(local.draws.iter().map(|d| Draw {
            first: first + d.first,
            count: d.count,
            image: d.image,
            roof: d.roof,
        }));
        true
    }

    fn load(&mut self, id: u32) -> Result<LocalMesh, Error> {
        let entry = self
            .entries
            .get(&id)
            .ok_or("not a static or skinned mesh")?;
        let data = entry.read_data(&mut self.file)?;
        let mesh = if entry.entry_type == SKINNED_MESH_TYPE {
            Mesh::parse_skinned(&entry.info, &data)?
        } else {
            Mesh::parse_static(&entry.info, &data)?
        };
        let creates = mesh
            .skeleton()
            .map(|s| {
                s.created_objects()
                    .into_iter()
                    .map(|(name, d)| {
                        let r = d.rotation;
                        let m = Mat4::from_cols(
                            Vec3::new(r[0][0], r[1][0], r[2][0]).extend(0.0),
                            Vec3::new(r[0][1], r[1][1], r[2][1]).extend(0.0),
                            Vec3::new(r[0][2], r[1][2], r[2][2]).extend(0.0),
                            Vec3::from_array(d.position).extend(1.0),
                        );
                        (name.to_owned(), m)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let particles = mesh
            .skeleton()
            .map(|s| {
                s.created_particles()
                    .into_iter()
                    .map(|(name, d)| (name.to_owned(), Vec3::from_array(d.position)))
                    .collect()
            })
            .unwrap_or_default();
        let helpers = mesh
            .skeleton()
            .map(|s| {
                s.helpers
                    .iter()
                    .filter_map(|h| {
                        let name = s
                            .helper_names
                            .iter()
                            .find(|n| fable_data::defs::name_hash(n) == h.name_hash)?;
                        Some((name.clone(), Vec3::from_array(h.position)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let lod = &mesh.lods[0];
        let mut local = LocalMesh {
            creates,
            particles,
            helpers,
            vertices: Vec::new(),
            indices: Vec::new(),
            draws: Vec::new(),
            // Rigged static meshes have a skeleton but no vertex weights.
            skin: lod
                .skeleton
                .clone()
                .filter(|_| lod.chunks.iter().any(|c| c.skinned))
                .map(|skeleton| Skin {
                    influences: Vec::new(),
                    skeleton,
                    morph: Vec::new(),
                }),
        };
        for chunk in &lod.chunks {
            let base = local.vertices.len() as u32;
            if let Some(skin) = &mut local.skin {
                // Palette indices are per sub-mesh; resolve them through the
                // sub-mesh that uses each vertex.
                let weights = chunk.bone_weights();
                let mut resolved = vec![([0u16; 4], [1.0, 0.0, 0.0, 0.0]); weights.len()];
                for sub in &chunk.sub_meshes {
                    for v in chunk.triangles(sub).into_iter().flatten() {
                        let (bones, w) = weights[usize::from(v)];
                        resolved[usize::from(v)] = (
                            bones.map(|b| {
                                u16::from(sub.bones.get(usize::from(b)).copied().unwrap_or(0))
                            }),
                            w.map(|x| f32::from(x) / 255.0),
                        );
                    }
                }
                skin.influences.extend(resolved);
            }
            let (positions, normals, uvs) = (chunk.positions(), chunk.normals(), chunk.uvs());
            local
                .vertices
                .extend((0..positions.len()).map(|i| ModelVertex {
                    position: positions[i],
                    normal: normals[i],
                    uv: uvs[i],
                }));
            for sub in &chunk.sub_meshes {
                let material = &lod.materials[sub.material as usize];
                let texture = self
                    .swaps
                    .get(&material.texture)
                    .copied()
                    .unwrap_or(material.texture);
                let image = self.image(texture, &material.name);
                let first = local.indices.len() as u32;
                for t in chunk.triangles(sub) {
                    local.indices.extend(t.map(|i| base + u32::from(i)));
                }
                let name = material.name.to_ascii_lowercase();
                local.draws.push(Draw {
                    first,
                    count: local.indices.len() as u32 - first,
                    image,
                    roof: name.contains("roof") || name.contains("fringe"),
                });
            }
        }
        Ok(local)
    }

    /// The objects mesh `id` creates (if it was added), with their transforms in
    /// the mesh's space.
    /// Where mesh `id` (if added) has the helpers named `name` or starting with
    /// it, in its own space; its origin for an empty name.
    fn helper_points(&self, id: u32, name: &str) -> Vec<Vec3> {
        let Some(Some(mesh)) = self.meshes.get(&id) else {
            return Vec::new();
        };
        if name.is_empty() {
            return vec![Vec3::ZERO];
        }
        mesh.helpers
            .iter()
            .filter(|(n, _)| n.starts_with(name))
            .map(|(_, p)| *p)
            .collect()
    }

    fn creates(&self, id: u32) -> Vec<(String, Mat4)> {
        self.meshes
            .get(&id)
            .and_then(Option::as_ref)
            .map(|m| m.creates.clone())
            .unwrap_or_default()
    }

    /// Adds the objects mesh `id`, placed with `transform`, creates, and theirs
    /// (two levels), resolving definition names to meshes with `resolve`.
    fn add_created(
        &mut self,
        id: u32,
        transform: Mat4,
        resolve: &dyn Fn(&str) -> Vec<u32>,
        depth: u32,
    ) -> usize {
        if depth == 0 {
            return 0;
        }
        let mut added = 0;
        for (name, local) in self.creates(id) {
            for mesh in resolve(&name) {
                let t = transform * local;
                if self.add(mesh, t) {
                    added += 1 + self.add_created(mesh, t, resolve, depth - 1);
                }
            }
        }
        added
    }

    /// The skin of mesh `id`, if it was added and is skinned. Its influences follow
    /// the mesh's own vertex order, as added by [`ModelBuilder::add`].
    pub fn skin(&self, id: u32) -> Option<Skin> {
        self.meshes.get(&id)?.as_ref()?.skin.clone()
    }

    /// Index of a texture in the model's images (0, plain white, for none).
    fn image(&mut self, id: u32, material: &str) -> usize {
        if id == 0 {
            return 0;
        }
        if let Some(&i) = self.images.get(&id) {
            return i;
        }
        let i = match self.textures.load(id) {
            Ok(img) => {
                self.model.images.push(img);
                self.model.images.len() - 1
            }
            Err(e) => {
                eprintln!("texture {id} for {material}: {e}");
                0
            }
        };
        self.images.insert(id, i);
        i
    }

    pub fn finish(mut self) -> ModelData {
        for v in &self.model.vertices {
            self.model.lo = self.model.lo.min(Vec3::from_array(v.position));
            self.model.hi = self.model.hi.max(Vec3::from_array(v.position));
        }
        self.model
    }
}

/// Loads the most detailed level of the first static mesh whose name contains
/// `needle` (case-insensitive), with its materials' main textures.
pub fn load_mesh(data_dir: &Path, needle: &str) -> Result<ModelData, Error> {
    let probe = ModelBuilder::open(data_dir, needle)?;
    let (id, name) = probe
        .find(needle)
        .ok_or_else(|| format!("no static mesh matching {needle}"))?;
    let mut builder = probe;
    builder.model.name = name;
    if !builder.add(id, Mat4::IDENTITY) {
        return Err(format!("could not load mesh {id}").into());
    }
    Ok(builder.finish())
}

/// The Prologue Hero's mesh (face, torso, mouth and hair materials).
const YOUNG_HERO_MESH: &str = "MESH_YOUNGHERO_02";

/// Takes the young Hero's mesh to his local space: feet at the origin, facing +y
/// (meshes face their local -y, so he is turned round), in world units.
pub fn hero_local() -> Mat4 {
    Mat4::from_scale(Vec3::splat(MESH_SCALE)) * Mat4::from_rotation_z(std::f32::consts::PI)
}

/// The young Hero in his bind pose, in mesh units (see [`hero_local`]), with his
/// skin for animation.
pub fn hero_model(data_dir: &Path) -> Result<(ModelData, Option<Skin>), Error> {
    let builder = ModelBuilder::open(data_dir, "Hero")?;
    let (id, _) = builder
        .find(YOUNG_HERO_MESH)
        .ok_or_else(|| format!("no {YOUNG_HERO_MESH}"))?;
    skinned_model(data_dir, id, &[])
}

/// Mesh `id` in its own units with its textures (with `swaps` applied: texture
/// named -> texture used), and its skin if it has one.
pub fn skinned_model(
    data_dir: &Path,
    id: u32,
    swaps: &[(u32, u32)],
) -> Result<(ModelData, Option<Skin>), Error> {
    let mut builder = ModelBuilder::open(data_dir, &id.to_string())?;
    builder.swaps = swaps.iter().copied().collect();
    if !builder.add(id, Mat4::IDENTITY) {
        return Err(format!("could not load mesh {id}").into());
    }
    let skin = builder.skin(id);
    Ok((builder.finish(), skin))
}

/// Loads every placement into one model in world space.
/// `resolve` gives the meshes of an object definition, for the objects meshes
/// create at their `CREATEOBJECT` dummies (a building's door frames and windows).
pub fn load_placements(
    data_dir: &Path,
    name: &str,
    placements: &[Placement],
    resolve: &dyn Fn(&str) -> Vec<u32>,
) -> Result<(ModelData, Vec<Building>), Error> {
    let mut builder = ModelBuilder::open(data_dir, name)?;
    let mut buildings: HashMap<u64, Building> = HashMap::new();
    let mut placed = 0;
    // Plain meshes' draws and centres, to find those standing indoors.
    let mut plain: Vec<(std::ops::Range<usize>, Vec3)> = Vec::new();
    for p in placements {
        let (draws, vertices) = (builder.model.draws.len(), builder.model.vertices.len());
        if !builder.add(p.mesh, p.transform()) {
            continue;
        }
        placed += 1;
        for (emitter, helper) in &p.effects {
            for at in builder.helper_points(p.mesh, helper) {
                let world = p.transform().transform_point3(at);
                builder
                    .model
                    .particles
                    .push((crate::particles::EmitterRef::Id(*emitter), world));
            }
        }
        let added = &builder.model.vertices[vertices..];
        let drawn = draws..builder.model.draws.len();
        match p.role {
            MeshRole::Plain => {
                let (lo, hi) = added.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(lo, hi), v| {
                        let v = Vec3::from_array(v.position);
                        (lo.min(v), hi.max(v))
                    },
                );
                plain.push((drawn, (lo + hi) * 0.5));
            }
            MeshRole::Exterior(uid) => {
                let b = buildings.entry(uid).or_default();
                for (i, d) in builder.model.draws.iter().enumerate().skip(draws) {
                    if d.roof {
                        b.roof_draws.push(i);
                        b.roof_triangles.push(d.first / 3..(d.first + d.count) / 3);
                    }
                }
            }
            MeshRole::Interior(uid) => {
                let b = buildings.entry(uid).or_default();
                b.indoor_draws.extend(drawn);
                for v in added {
                    let v = Vec3::from_array(v.position);
                    b.interior_lo = b.interior_lo.min(v);
                    b.interior_hi = b.interior_hi.max(v);
                }
            }
        }
    }
    let mut created = 0;
    for p in placements {
        created += builder.add_created(p.mesh, p.transform(), resolve, 2);
    }
    eprintln!(
        "{name}: placed {placed} of {} meshes, and {created} they create",
        placements.len()
    );
    let mut buildings: Vec<Building> = buildings
        .into_values()
        .filter(|b| !b.roof_draws.is_empty() && b.interior_lo.x <= b.interior_hi.x)
        .collect();
    // Furniture and lamps inside a building are lit as its interior.
    for (drawn, centre) in plain {
        if let Some(b) = buildings.iter_mut().find(|b| b.contains(centre)) {
            b.indoor_draws.extend(drawn);
        }
    }
    Ok((builder.finish(), buildings))
}

/// A building with an interior: its exterior's draws in the placed model, and the
/// box its interior mesh spans.
#[derive(Debug, Clone)]
pub struct Building {
    /// Its exterior's roof draws in the placed model (walls, door frames and
    /// the rest of the exterior stay).
    pub roof_draws: Vec<usize>,
    /// The exterior's triangles in the placed model (camera rays skip them while
    /// it is hidden).
    pub roof_triangles: Vec<std::ops::Range<u32>>,
    pub interior_lo: Vec3,
    pub interior_hi: Vec3,
    /// Draws lit as indoors: the interior meshes and what stands inside.
    pub indoor_draws: Vec<usize>,
}

impl Default for Building {
    fn default() -> Self {
        Self {
            indoor_draws: Vec::new(),
            roof_draws: Vec::new(),
            roof_triangles: Vec::new(),
            interior_lo: Vec3::splat(f32::INFINITY),
            interior_hi: Vec3::splat(f32::NEG_INFINITY),
        }
    }
}

impl Building {
    /// Whether `p` is inside the interior's box, a little in from its walls.
    pub fn contains(&self, p: Vec3) -> bool {
        let margin = Vec3::new(0.3, 0.3, 0.0);
        let (lo, hi) = (self.interior_lo + margin, self.interior_hi - margin);
        p.x > lo.x && p.y > lo.y && p.x < hi.x && p.y < hi.y && p.z > lo.z - 1.0 && p.z < hi.z
    }
}

/// The uniforms for a model placed by `model` (`view_proj` already includes
/// it), seen from `eye` in the model's own space, lit by `light`.
fn uniforms(
    view_proj: Mat4,
    model: Mat4,
    eye: Vec3,
    highlight: [f32; 4],
    light: &Light,
) -> Uniforms {
    // Normals are in the model's own space: so is the sun.
    let sun = model
        .inverse()
        .transform_vector3(light.sun_dir)
        .normalize_or(light.sun_dir);
    let shadow_proj = light.shadow_proj * model;
    let [sr, sg, sb] = light.sun;
    let [ar, ag, ab] = light.ambient;
    Uniforms {
        view_proj: view_proj.to_cols_array_2d(),
        sun: sun.extend(0.0).to_array(),
        eye: eye.extend(1.0).to_array(),
        highlight,
        sun_colour: [sr, sg, sb, light.highlight_limit],
        ambient: [ar, ag, ab, light.brightness],
        shadow_proj: shadow_proj.to_cols_array_2d(),
        shadow: light.shadow,
        model: model.to_cols_array_2d(),
        points: light.points,
        point_count: [light.point_count as f32, 0.0, 0.0, 0.0],
    }
}

pub(crate) struct TextureBank {
    file: BufReader<File>,
    entries: HashMap<u32, fable_data::bank::Entry>,
}

impl TextureBank {
    pub(crate) fn open(data_dir: &Path) -> Result<Self, Error> {
        let mut file = BufReader::new(File::open(data_dir.join("graphics/pc/textures.big"))?);
        let big = BigFile::read(&mut file)?;
        let index = big
            .bank("GBANK_MAIN_PC")
            .ok_or("no GBANK_MAIN_PC")?
            .read_index(&mut file)?;
        let entries = index.entries.into_iter().map(|e| (e.id, e)).collect();
        Ok(Self { file, entries })
    }

    /// Every texture's id and short name (the source file name, without its path).
    pub(crate) fn names(&self) -> Vec<(u32, String)> {
        let mut names: Vec<(u32, String)> = self
            .entries
            .values()
            .map(|e| {
                let short = e.name.rsplit('\\').next().unwrap_or(&e.name);
                (e.id, short.trim_end_matches(']').to_owned())
            })
            .collect();
        names.sort_by(|a, b| a.1.cmp(&b.1));
        names
    }

    pub(crate) fn load(&mut self, id: u32) -> Result<Image, Error> {
        let entry = self
            .entries
            .get(&id)
            .ok_or_else(|| format!("no texture {id}"))?;
        let info = TextureInfo::parse(&entry.info)?;
        let texture = Texture::unpack(&info, entry.entry_type, &entry.read_data(&mut self.file)?)?;
        Ok(Image {
            width: texture.width,
            height: texture.height,
            rgba: texture.to_rgba(0, 0)?,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    sun: [f32; 4],
    eye: [f32; 4],
    highlight: [f32; 4],
    sun_colour: [f32; 4],
    ambient: [f32; 4],
    shadow_proj: [[f32; 4]; 4],
    shadow: [f32; 4],
    model: [[f32; 4]; 4],
    points: [[f32; 4]; 2 * crate::lighting::MAX_POINTS],
    point_count: [f32; 4],
}

pub struct ModelRenderer {
    pipeline: wgpu::RenderPipeline,
    /// Depth from the sun: its pipeline and uniforms (the sun's view), and an
    /// empty shadow map for drawing alone.
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_uniforms: wgpu::Buffer,
    shadow_group: wgpu::BindGroup,
    no_shadow: crate::shadow::ShadowMap,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_groups: Vec<wgpu::BindGroup>,
    draws: Vec<(u32, u32, usize)>,
    /// Draws skipped (an exterior hidden while the Hero is inside).
    hidden: Vec<usize>,
    /// Per draw, whether it is lit as indoors, by the indoor uniforms.
    indoor: Vec<bool>,
    indoor_uniforms: wgpu::Buffer,
    indoor_group: wgpu::BindGroup,
    pub target: Option<Target>,
}

impl ModelRenderer {
    /// Replaces the vertices (same count and order as the model it was made from).
    pub fn set_vertices(&self, queue: &wgpu::Queue, vertices: &[ModelVertex]) {
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, model: &ModelData) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("model.wgsl"));
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let indoor_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model indoor uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let indoor_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model indoor uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: indoor_uniforms.as_entire_binding(),
            }],
        });
        let shadow_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model shadow uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model shadow uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: shadow_uniforms.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_groups = model
            .images
            .iter()
            .map(|img| {
                let texture = device.create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some("model texture"),
                        size: wgpu::Extent3d {
                            width: img.width,
                            height: img.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &img.rgba,
                );
                let view = texture.create_view(&Default::default());
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model texture"),
                    layout: &texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            })
            .collect();
        let shadow_layout = crate::shadow::layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[
                Some(&uniform_layout),
                Some(&texture_layout),
                Some(&shadow_layout),
            ],
            ..Default::default()
        });
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("model shadow"),
                bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
                ..Default::default()
            });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<ModelVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(crate::shadow::depth_state()),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<ModelVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            // Winding is not known yet, so draw both sides.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Self {
            pipeline,
            shadow_pipeline,
            shadow_uniforms,
            shadow_group,
            no_shadow: crate::shadow::ShadowMap::none(device),
            vertices: buffer(
                "model vertices",
                bytemuck::cast_slice(&model.vertices),
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ),
            indices: buffer(
                "model indices",
                bytemuck::cast_slice(&model.indices),
                wgpu::BufferUsages::INDEX,
            ),
            uniforms,
            uniform_group,
            texture_groups,
            draws: model
                .draws
                .iter()
                .map(|d| (d.first, d.count, d.image))
                .collect(),
            hidden: Vec::new(),
            indoor: Vec::new(),
            indoor_uniforms,
            indoor_group,
            target: None,
        }
    }

    /// Renders into the target texture, recreating it if the size changed.
    /// Returns true if the target was recreated.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        view_proj: Mat4,
    ) -> bool {
        let size = [size[0].max(1), size[1].max(1)];
        let recreated = self.target.as_ref().is_none_or(|t| t.size != size);
        if recreated {
            self.target = Some(create_target(device, size));
        }
        let target = self.target.as_ref().unwrap();
        self.prepare(queue, view_proj, &Light::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("model"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(2, &self.no_shadow.group, &[]);
            self.draw(&mut pass);
        }
        queue.submit([encoder.finish()]);
        recreated
    }

    /// Writes the uniforms for this frame; call before [`ModelRenderer::draw`].
    pub fn prepare(&self, queue: &wgpu::Queue, view_proj: Mat4, light: &Light) {
        self.write(
            queue,
            view_proj,
            Mat4::IDENTITY,
            Vec3::ZERO,
            [0.0; 4],
            light,
        );
    }

    /// As [`ModelRenderer::prepare`] for a model placed by `model`, seen from
    /// `eye` (world space), glowing with `highlight` (rgb, strength; strength 0
    /// for none).
    pub fn prepare_placed(
        &self,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        model: Mat4,
        eye: Vec3,
        highlight: [f32; 4],
        light: &Light,
    ) {
        let local_eye = model.inverse().transform_point3(eye);
        self.write(queue, view_proj * model, model, local_eye, highlight, light);
    }

    /// Writes the light for this model's indoor draws (see
    /// [`ModelRenderer::set_indoor`]); call after the outdoor `prepare`.
    pub fn prepare_indoor(&self, queue: &wgpu::Queue, view_proj: Mat4, light: &Light) {
        if self.indoor.iter().any(|&i| i) {
            let uniforms = uniforms(view_proj, Mat4::IDENTITY, Vec3::ZERO, [0.0; 4], light);
            queue.write_buffer(&self.indoor_uniforms, 0, bytemuck::bytes_of(&uniforms));
        }
    }

    /// Marks the draws lit as indoors (empty for none).
    pub fn set_indoor(&mut self, draws: &[usize]) {
        self.indoor = (0..self.draws.len()).map(|i| draws.contains(&i)).collect();
    }

    fn write(
        &self,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        model: Mat4,
        eye: Vec3,
        highlight: [f32; 4],
        light: &Light,
    ) {
        let mut uniforms = uniforms(view_proj, model, eye, highlight, light);
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        if light.shadow[1] > 0.0 {
            // The shadow pass sees the model from the sun (even when lit
            // indoors, so it casts its shadow where it stands).
            uniforms.view_proj = uniforms.shadow_proj;
            queue.write_buffer(&self.shadow_uniforms, 0, bytemuck::bytes_of(&uniforms));
        }
    }

    /// Records the model's draws into a pass using the shared colour and depth formats.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut indoor = false;
        for (i, &(first, count, image)) in self.draws.iter().enumerate() {
            if self.hidden.contains(&i) {
                continue;
            }
            let here = self.indoor.get(i).copied().unwrap_or(false);
            if here != indoor {
                let group = if here {
                    &self.indoor_group
                } else {
                    &self.uniform_group
                };
                pass.set_bind_group(0, group, &[]);
                indoor = here;
            }
            pass.set_bind_group(1, &self.texture_groups[image], &[]);
            pass.draw_indexed(first..first + count, 0, 0..1);
        }
    }

    /// Records the model's depth into a shadow pass.
    pub fn draw_shadow(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.shadow_pipeline);
        pass.set_bind_group(0, &self.shadow_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (i, &(first, count, image)) in self.draws.iter().enumerate() {
            if self.hidden.contains(&i) {
                continue;
            }
            pass.set_bind_group(1, &self.texture_groups[image], &[]);
            pass.draw_indexed(first..first + count, 0, 0..1);
        }
    }

    /// Skips the draws listed (empty to show everything).
    pub fn set_hidden(&mut self, draws: Vec<usize>) {
        self.hidden = draws;
    }
}
