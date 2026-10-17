//! Builds a terrain mesh for a region from its maps' Lev heightmaps.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use fable_data::bbbb::BbbbFile;
use fable_data::defs::Defs;
use fable_data::lev::Lev;
use fable_data::nav::Navigation;
use fable_data::tng::Tng;
use fable_data::wld::Wld;
use glam::{Vec2, Vec3};

use crate::model::{MeshRole, Placement};
use crate::walk::{HeightGrid, WalkGrid};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// Sea surface height in world units. Across all maps, 731 stb tiles that rise above
/// their terrain top out at exactly this height (`docs/formats/stb.md`).
pub const SEA_LEVEL: f32 = 3.0;

/// Regions with water have an stb entry with this prefix followed by the region name.
/// Its contents (compressed, format unknown) may hold per-region water heights; until
/// then such regions get a plane at [`SEA_LEVEL`] if their ground dips below it.
const SEA_BANK_PREFIX: &str = "__ENGINE_SEA_STATIC_MAP_BANK_FILE__";

/// One map's sound map: per 4x4-unit block, row by row from the map's origin,
/// the sound theme it plays (`SOUND_THEME_COUNTRY`). A block's value counts
/// from 1 into the map's list of sound themes.
#[derive(Debug, Clone)]
pub struct SoundArea {
    pub origin: Vec2,
    /// Blocks per row.
    pub columns: usize,
    pub themes: Vec<Option<String>>,
}

/// Side of a sound map block in world units.
pub const SOUND_BLOCK: f32 = 4.0;

impl SoundArea {
    /// The theme of the block at world point `p`, if `p` is on this map.
    pub fn theme_at(&self, p: Vec2) -> Option<&str> {
        let local = (p - self.origin) / SOUND_BLOCK;
        if local.x < 0.0 || local.y < 0.0 || self.columns == 0 {
            return None;
        }
        let (x, y) = (local.x as usize, local.y as usize);
        if x >= self.columns {
            return None;
        }
        self.themes.get(y * self.columns + x)?.as_deref()
    }
}

/// World units: x and y as in the `.wld`, z is height.
#[derive(Debug, Default)]
pub struct TerrainMesh {
    /// Every quest-state section the region's things use, besides `NULL`.
    pub sections: BTreeSet<String>,
    /// Region exits in the maps the region contains.
    pub exits: Vec<RegionExit>,
    /// Creatures placed in the maps the region contains.
    pub creatures: Vec<CreaturePlacement>,
    /// Doors in the maps the region contains.
    pub doors: Vec<DoorPlacement>,
    /// Lit lamps, fires and light markers.
    pub lights: Vec<PointLight>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub maps: usize,
    /// Height of a sea plane covering the terrain bounds, if the region has sea.
    pub sea_level: Option<f32>,
    /// Which themes cover the ground, if loaded.
    pub themes: Option<ThemeGrid>,
    /// Navigation leaves as coloured quads; `normal` holds the colour.
    pub nav_vertices: Vec<Vertex>,
    pub nav_indices: Vec<u32>,
    /// Navigation leaves: world centre, side and layer (0 = the ground).
    pub nav_cells: Vec<(Vec2, f32, usize)>,
    /// The sound map of each map the region contains: which sound theme each
    /// 4x4-unit block plays.
    pub sound_areas: Vec<SoundArea>,
    /// Where a Hero can walk, one grid per navigation layer, and the ground height,
    /// once the region is loaded. Layers above the ground get their surface heights
    /// later, from the placed meshes.
    pub walk: Vec<WalkGrid>,
    pub heights: Option<HeightGrid>,
}

/// Terrain themes across a region: per grid point, two palette layers and weights.
#[derive(Debug, Default)]
pub struct ThemeGrid {
    /// World position of grid point (0, 0).
    pub origin: [f32; 2],
    pub size: [u32; 2],
    /// Row-major: [first layer, second layer, first weight, second weight].
    pub cells: Vec<[u8; 4]>,
    /// Layer 0 is "no theme"; the rest are named themes with their texture ids.
    pub layers: Vec<ThemeLayer>,
}

#[derive(Debug, Clone, Default)]
pub struct ThemeLayer {
    pub name: String,
    pub base: u32,
    pub cliff: u32,
}

impl ThemeGrid {
    fn new(origin: [f32; 2], size: [u32; 2]) -> Self {
        Self {
            origin,
            size,
            cells: vec![[0; 4]; size[0] as usize * size[1] as usize],
            layers: vec![ThemeLayer::default()],
        }
    }

    fn layer(&mut self, name: &str, defs: &Defs) -> u8 {
        if let Some(i) = self.layers.iter().position(|l| l.name == name) {
            return i as u8;
        }
        let t = defs.get(name).map(|d| d.theme_textures());
        let cliff = t.map_or(0, |t| t.cliff);
        // Water themes (base texture LANDSCAPE_WATER and similar) paint thin strips
        // beside paths and on slopes; in the game no water shows there. They
        // probably mark ground meant to lie under a water surface,
        // which is not drawn yet, so their flat ground uses the cliff texture.
        let base = if name.starts_with("WATER_") {
            cliff
        } else {
            t.map_or(0, |t| t.base)
        };
        self.layers.push(ThemeLayer {
            name: name.to_owned(),
            base,
            cliff,
        });
        (self.layers.len() - 1).min(255) as u8
    }

    /// Writes one map's theme points at world position (`x`, `y`).
    ///
    /// Some used slots have no name (all of StartOakVale_Filler_05's points use one).
    /// How the game resolves them is not known; as a visual stand-in they get the
    /// map's first real named theme.
    fn add_map(&mut self, lev: &Lev, x: f32, y: f32, defs: &Defs) {
        let stand_in = lev
            .themes
            .iter()
            .map(|t| t.name.as_str())
            .find(|n| !n.is_empty() && *n != "INVALID_THEME_STANDIN");
        let mut slot_layer = [0u8; 256];
        for t in &lev.themes {
            let name = match (t.name.as_str(), stand_in) {
                ("", Some(s)) => s,
                ("", None) => continue,
                (n, _) => n,
            };
            slot_layer[t.slot as usize] = self.layer(name, defs);
        }
        let (ox, oy) = ((x - self.origin[0]) as u32, (y - self.origin[1]) as u32);
        for j in 0..=lev.height {
            for i in 0..=lev.width {
                let Some(cell) = lev.cell(i, j) else { continue };
                let ([t0, t1, _], [w0, w1]) = cell.themes();
                let (gx, gy) = (ox + i, oy + j);
                if gx < self.size[0] && gy < self.size[1] {
                    self.cells[(gy * self.size[0] + gx) as usize] =
                        [slot_layer[t0 as usize], slot_layer[t1 as usize], w0, w1];
                }
            }
        }
    }
}

impl TerrainMesh {
    /// Adds one map's heightmap with its origin at (`x`, `y`).
    pub fn add_map(&mut self, lev: &Lev, x: f32, y: f32) {
        let (w, h) = (lev.width, lev.height);
        let height = |i: u32, j: u32| lev.cell(i.min(w), j.min(h)).map_or(0.0, |c| c.height());
        let base = self.vertices.len() as u32;
        for j in 0..=h {
            for i in 0..=w {
                // Central differences, clamped at the map edge.
                let dx = (height((i + 1).min(w), j) - height(i.saturating_sub(1), j))
                    / ((i + 1).min(w) - i.saturating_sub(1)) as f32;
                let dy = (height(i, (j + 1).min(h)) - height(i, j.saturating_sub(1)))
                    / ((j + 1).min(h) - j.saturating_sub(1)) as f32;
                self.vertices.push(Vertex {
                    position: [x + i as f32, y + j as f32, height(i, j)],
                    normal: Vec3::new(-dx, -dy, 1.0).normalize().to_array(),
                });
            }
        }
        let row = w + 1;
        for j in 0..h {
            for i in 0..w {
                let a = base + j * row + i;
                let (b, c, d) = (a + 1, a + row, a + row + 1);
                self.indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        self.maps += 1;
    }

    /// A quad at sea level over the terrain's x/y bounds, as 4 vertices and 6 indices.
    pub fn sea_quad(&self) -> Option<([Vertex; 4], [u32; 6])> {
        let z = self.sea_level?;
        let (lo, hi) = self.bounds();
        let v = |x: f32, y: f32| Vertex {
            position: [x, y, z],
            normal: [0.0, 0.0, 1.0],
        };
        Some((
            [v(lo.x, lo.y), v(hi.x, lo.y), v(hi.x, hi.y), v(lo.x, hi.y)],
            [0, 1, 2, 0, 2, 3],
        ))
    }

    /// Adds the map's default navigation (the `NULL` section, else the first) as
    /// quads just above the ground: red on paths, green on other walkable ground,
    /// yellow on further layers (bridge decks, raised floors).
    fn add_navigation(&mut self, lev: &Lev, data: &[u8], x: f32, y: f32) {
        let Ok(tail) = lev.parse_tail(data) else {
            return;
        };
        let Some(section) = tail
            .sections
            .iter()
            .find(|s| s.name == "NULL")
            .or(tail.sections.first())
        else {
            return;
        };
        let Ok(nav) = Navigation::parse(&section.data, lev.width, lev.height) else {
            return;
        };
        for (layer, node, leaf) in nav.leaves() {
            let colour = match (layer, leaf.kind) {
                (0, 0) => [0.85, 0.15, 0.15],
                (0, _) => [0.2, 0.85, 0.2],
                _ => [1.0, 0.85, 0.0],
            };
            let half = node.size() / 2.0 * 0.9;
            let [cx, cy] = node.centre;
            let ground = |gx: f32, gy: f32| {
                lev.cell(gx.round().max(0.0) as u32, gy.round().max(0.0) as u32)
                    .map_or(0.0, |c| c.height())
            };
            self.nav_cells.push((
                Vec2::new(x + node.centre[0], y + node.centre[1]),
                node.size(),
                layer,
            ));
            let base = self.nav_vertices.len() as u32;
            for (dx, dy) in [(-half, -half), (half, -half), (half, half), (-half, half)] {
                let (px, py) = (cx + dx, cy + dy);
                self.nav_vertices.push(Vertex {
                    position: [x + px, y + py, ground(px, py) + 0.15],
                    normal: colour,
                });
            }
            self.nav_indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base,
                base + 2,
                base + 3,
            ]);
        }
    }

    /// The walk grids as quads at their walking heights: green on the terrain,
    /// blue on floors of the ground layer, yellow on later layers, and red on
    /// cells of later layers that `navigation` (the grids before surfaces were
    /// assigned) marks walkable but that got no height.
    pub fn walk_overlay(
        navigation: &[WalkGrid],
        layers: &[WalkGrid],
        ground: &HeightGrid,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let (mut vertices, mut indices) = (Vec::new(), Vec::new());
        let mut quad = |c: Vec2, z: f32, colour: [f32; 3]| {
            let base = vertices.len() as u32;
            let h = crate::walk::CELL * 0.45;
            for (dx, dy) in [(-h, -h), (h, -h), (h, h), (-h, h)] {
                vertices.push(Vertex {
                    position: [c.x + dx, c.y + dy, z + 0.1],
                    normal: colour,
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        for (l, (nav, layer)) in navigation.iter().zip(layers).enumerate() {
            for c in nav.cell_centres() {
                match layer.height(c, ground) {
                    Some(z) if l > 0 => quad(c, z, [1.0, 0.85, 0.0]),
                    Some(z) if layer.on_surface(c) => quad(c, z, [0.2, 0.4, 1.0]),
                    Some(z) => quad(c, z, [0.2, 0.85, 0.2]),
                    None => quad(c, ground.height(c), [0.9, 0.1, 0.1]),
                }
            }
        }
        (vertices, indices)
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), v| {
                let p = Vec3::from_array(v.position);
                (lo.min(p), hi.max(p))
            },
        )
    }
}

/// A map's placed meshes, region exits, creatures and doors.
type MapThings = (
    Vec<Placement>,
    Vec<RegionExit>,
    Vec<CreaturePlacement>,
    Vec<DoorPlacement>,
);

/// A door (a thing with a `CTCDoor`): a skinned mesh that swings open.
#[derive(Debug, Clone)]
pub struct DoorPlacement {
    pub definition: String,
    /// `CTCDoor`'s `Open`.
    pub open: bool,
    /// `DOOR_OPEN` and `DOOR_CLOSE` animations from its table (filled in after
    /// loading).
    pub animations: [Option<u32>; 2],
    pub placement: Placement,
}

/// A creature placed in a map (an `AICreature` thing).
#[derive(Debug, Clone)]
pub struct CreaturePlacement {
    pub definition: String,
    pub uid: u64,
    /// The `Graphic` mesh (`placement.mesh`) is used alone when this is empty;
    /// otherwise one option from each list (heads, torsos, legs) is.
    pub parts: Vec<Vec<fable_data::defs::PartOption>>,
    /// Its idle animations from its `CAppearanceDef` (filled in after loading).
    pub idles: IdleAnimations,
    /// Its bone config files (`CSkeletalMorphDef`; filled in after loading).
    pub morphs: Vec<String>,
    /// Its whole animation table, for gestures and other one-off animations.
    pub slots: std::sync::Arc<Vec<fable_data::defs::AnimationSlot>>,
    /// A quest character (a `ScriptName` naming a role, such as the Prologue's
    /// `NOVI_AffairWife`), which its quest moves, so it does not wander. Generic
    /// background roles (`NOVI_Villager`) do not count.
    pub scripted: bool,
    /// Its `ScriptName` (its role), if any.
    pub script_name: Option<String>,
    pub placement: Placement,
}

/// A creature's standing animations, as `MBANK_ALLMESHES` ids: the loop it
/// breathes in (`ST_IDLE_SUBTLE`) and the fidgets it sometimes plays (`ST_IDLE`).
#[derive(Debug, Clone, Default)]
pub struct IdleAnimations {
    pub base: Option<u32>,
    pub fidgets: Vec<u32>,
    /// Its walk (`ST_WALK`).
    pub walk: Option<u32>,
    /// Its flinch when hit (`RESPONSE_GENERIC_HIT_RIGHT`).
    pub hit: Option<u32>,
    /// Variants of its idle by animation table modifier (`MORTALLY_WOUNDED`),
    /// for quests that pose it.
    pub moods: Vec<(String, u32)>,
}

impl IdleAnimations {
    /// From an animation table: the plain or `STAND` variants of the two slots.
    pub fn from_slots(slots: &[fable_data::defs::AnimationSlot]) -> Self {
        let plain = |slot: &'static str| {
            slots
                .iter()
                .filter(move |s| {
                    s.slot == slot && s.modifier.as_deref().is_none_or(|m| m == "STAND")
                })
                .map(|s| s.animation)
        };
        let mut fidgets: Vec<u32> = Vec::new();
        for a in plain("ST_IDLE") {
            if !fidgets.contains(&a) {
                fidgets.push(a);
            }
        }
        let base = plain("ST_IDLE_SUBTLE").next();
        fidgets.retain(|&a| Some(a) != base);
        let walk = plain("ST_WALK").next();
        let hit = plain("RESPONSE_GENERIC_HIT_RIGHT").next();
        let moods = slots
            .iter()
            .filter(|s| s.slot == "ST_IDLE_SUBTLE")
            .filter_map(|s| Some((s.modifier.clone().filter(|m| m != "STAND")?, s.animation)))
            .collect();
        Self {
            base,
            fidgets,
            walk,
            hit,
            moods,
        }
    }
}

/// A region exit (`CTCDRegionExit`): walking within `radius` of `position` leads to
/// the entrance named by `connection` (see `fable_data::gtg::split_connected_uid`).
#[derive(Debug, Clone, Copy)]
pub struct RegionExit {
    pub position: Vec3,
    pub radius: f32,
    pub connection: u64,
}

/// Loads every map a region contains or sees.
/// The quest-state sections a region shows by default, besides `NULL`. Tng things
/// belong to sections: `NULL` (always there), quest states
/// (`Q_NewOakValeIntro_PreAttack`, `Q__OakValeIntro_PostAttack`, the raid's
/// bandits) and village events (`V_FisticuffsClubOnlyAtNight`). Which are active
/// is the quest system's business; until there is one, the Prologue's Oakvale
/// starts before the raid and other regions show `NULL` only.
pub fn default_sections(region: &str) -> Vec<String> {
    match region {
        "StartOakVale" => vec!["Q_NewOakValeIntro_PreAttack".into()],
        _ => Vec::new(),
    }
}

/// Whether a thing in `section` is shown with `active` sections.
fn section_active(section: &str, active: &[String]) -> bool {
    section.is_empty() || section == "NULL" || active.iter().any(|a| a == section)
}

/// Loads every map a region contains or sees, with things from `NULL` and the
/// `sections` given (see [`default_sections`]).
pub fn load_region(
    data_dir: &Path,
    wld: &Wld,
    region: &str,
    defs: &Defs,
    sections: &[String],
) -> Result<(TerrainMesh, Vec<Placement>), Box<dyn std::error::Error>> {
    let region = wld
        .region(region)
        .ok_or_else(|| format!("no region named {region:?}"))?;
    let levels: BTreeSet<&String> = region
        .contains_maps
        .iter()
        .chain(&region.sees_maps)
        .collect();
    let wad_path = data_dir.join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&wad_path)?;
    let mut file = BufReader::new(File::open(&wad_path)?);
    let mut mesh = TerrainMesh::default();
    let mut placements = Vec::new();
    let mut maps = Vec::new();
    for level in levels {
        let map = wld
            .map(level)
            .ok_or_else(|| format!("{level} is not in the wld"))?;
        let entry = wad
            .index
            .by_name(&format!("Data\\Levels\\{level}"))
            .ok_or_else(|| format!("{level} is not in the wad"))?;
        let data = entry.read_data(&mut file)?;
        let lev = Lev::parse(&data)?;
        mesh.add_map(&lev, map.x as f32, map.y as f32);
        mesh.add_navigation(&lev, &data, map.x as f32, map.y as f32);
        maps.push((lev, map.x as f32, map.y as f32));
        let origin = Vec3::new(map.x as f32, map.y as f32, 0.0);
        let (placed, exits, creatures, doors) =
            load_placements(&wad, &mut file, level, origin, defs, sections, &mut mesh)?;
        placements.extend(placed);
        if region.contains_maps.contains(level) {
            let lev = &maps.last().expect("just pushed").0;
            if let Ok(tail) = lev.parse_tail(&data) {
                mesh.sound_areas.push(SoundArea {
                    origin: origin.truncate(),
                    columns: (lev.width / 4) as usize,
                    themes: tail
                        .sound_map
                        .iter()
                        .map(|b| {
                            let i = usize::from(b.value()).checked_sub(1)?;
                            lev.sound_themes.get(i).cloned()
                        })
                        .collect(),
                });
            }
            mesh.exits.extend(exits);
            mesh.creatures.extend(creatures);
            mesh.doors.extend(doors);
        }
    }
    if !maps.is_empty() {
        let (lo, hi) = mesh.bounds();
        let size = [(hi.x - lo.x) as u32 + 1, (hi.y - lo.y) as u32 + 1];
        let mut grid = ThemeGrid::new([lo.x, lo.y], size);
        for (lev, x, y) in &maps {
            grid.add_map(lev, *x, *y, defs);
        }
        let mut heights = HeightGrid::new(Vec2::new(lo.x, lo.y), size);
        for (lev, x, y) in &maps {
            let (ox, oy) = ((x - lo.x) as u32, (y - lo.y) as u32);
            for j in 0..=lev.height {
                for i in 0..=lev.width {
                    if let Some(c) = lev.cell(i, j) {
                        heights.set(ox + i, oy + j, c.height());
                    }
                }
            }
        }
        let layers = mesh.nav_cells.iter().map(|c| c.2 + 1).max().unwrap_or(0);
        mesh.walk = (0..layers)
            .map(|_| WalkGrid::new(Vec2::new(lo.x, lo.y), Vec2::new(hi.x, hi.y)))
            .collect();
        for (centre, side, layer) in &mesh.nav_cells {
            mesh.walk[*layer].mark(*centre, *side);
        }
        mesh.heights = Some(heights);
        mesh.themes = Some(grid);
    }
    let stb = BbbbFile::open(data_dir.join("Levels/FinalAlbion_RT.stb"))?;
    if stb
        .index
        .by_name(&format!("{SEA_BANK_PREFIX}{}", region.name))
        .is_some()
    {
        // Inland water regions sit far above sea level; a plane there would only
        // show as a skirt under the terrain edges.
        if mesh.bounds().0.z < SEA_LEVEL {
            mesh.sea_level = Some(SEA_LEVEL);
        }
    }
    Ok((mesh, placements))
}

/// Buildings and objects on one map whose definition names a mesh.
/// A point light placed in a region: a lamp, a fire or a light marker.
#[derive(Debug, Clone, PartialEq)]
pub struct PointLight {
    pub position: Vec3,
    /// RGB 0..1, as stored (gamma space).
    pub colour: [f32; 3],
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub flicker: f32,
    pub flicker_speed: f32,
    /// Lit only at night (dimmed by daylight).
    pub night_only: bool,
}

impl PointLight {
    /// The light of a placed thing: its definition's `CLightDef`, switched by
    /// its `CTCLight` component's `Active` and with the values it overrides.
    /// Lamps whose definition is named unlit or off are skipped: they are
    /// placed active but show no flame (a guess from the names).
    fn from_thing(t: &fable_data::tng::Thing, defs: &Defs, origin: Vec3) -> Option<Self> {
        let component = t.component("CTCLight")?;
        let field = |k: &str| {
            component
                .fields
                .iter()
                .find(|f| f.key == k)
                .map(|f| f.value.as_str())
        };
        if field("Active") == Some("FALSE") {
            return None;
        }
        let definition = t.definition()?;
        if definition.contains("_UNLIT") || definition.ends_with("_OFF") {
            return None;
        }
        let def = defs.get(definition)?;
        let record = def
            .component("CLightDef")
            .and_then(|i| defs.defs.get(i))
            .filter(|c| defs.type_name(c) == "CLightDef")?;
        let light = fable_data::light::LightDef::parse(&record.record).ok()?;
        let overridden = field("Overridden") == Some("TRUE");
        let number = |k: &str, default: f32| {
            overridden
                .then(|| field(k)?.parse::<f32>().ok())
                .flatten()
                .unwrap_or(default)
        };
        let colour = overridden
            .then(|| fable_data::light::parse_tng_colour(field("Colour")?))
            .flatten()
            .unwrap_or(light.colour);
        Some(Self {
            position: origin + Vec3::from_array(t.position()?),
            colour: colour.map(|c| f32::from(c) / 255.0),
            inner_radius: number("InnerRadius", light.inner_radius),
            outer_radius: number("OuterRadius", light.outer_radius),
            flicker: number("Flicker", light.flicker),
            flicker_speed: light.flicker_speed,
            night_only: light.daylight_attenuator,
        })
        .filter(|l| l.outer_radius > 0.0 && l.colour.iter().any(|&c| c > 0.0))
    }
}

fn load_placements(
    wad: &BbbbFile,
    file: &mut BufReader<File>,
    level: &str,
    origin: Vec3,
    defs: &Defs,
    sections: &[String],
    // Gains the level's quest-state sections and lights.
    mesh: &mut TerrainMesh,
) -> Result<MapThings, Box<dyn std::error::Error>> {
    let tng_name = format!("Data\\Levels\\{}", level.replace(".lev", ".tng"));
    let Some(entry) = wad.index.by_name(&tng_name) else {
        return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    };
    let text: String = entry.read_data(file)?.iter().map(|&b| b as char).collect();
    let mut tng = Tng::parse(&text)?;
    for t in &tng.things {
        if !t.section.is_empty() && t.section != "NULL" {
            mesh.sections.insert(t.section.clone());
        }
    }
    tng.things.retain(|t| section_active(&t.section, sections));
    mesh.lights.extend(
        tng.things
            .iter()
            .filter_map(|t| PointLight::from_thing(t, defs, origin)),
    );
    let exits = tng
        .things
        .iter()
        .filter_map(|t| {
            let exit = t.component("CTCDRegionExit")?;
            let field = |k: &str| {
                exit.fields
                    .iter()
                    .find(|f| f.key == k)
                    .map(|f| f.value.as_str())
            };
            if field("Active") == Some("FALSE") {
                return None;
            }
            Some(RegionExit {
                position: origin + Vec3::from_array(t.position()?),
                radius: field("Radius")?.parse().ok()?,
                connection: field("EntranceConnectedToUID")?.parse().ok()?,
            })
        })
        .collect();
    let creatures = tng
        .things
        .iter()
        .filter(|t| t.kind == "AICreature")
        .filter_map(|t| {
            let definition = t.definition()?;
            let def = defs.get(definition)?;
            Some(CreaturePlacement {
                definition: definition.to_owned(),
                uid: t.uid().unwrap_or(0),
                parts: defs.creature_parts(def),
                idles: IdleAnimations::default(),
                morphs: Vec::new(),
                slots: Default::default(),
                scripted: t.script_name().is_some_and(|n| !n.ends_with("_Villager")),
                script_name: t.script_name().map(str::to_owned),
                placement: Placement {
                    role: MeshRole::Plain,
                    mesh: defs.get(definition)?.graphic()?,
                    position: origin + Vec3::from_array(t.position()?),
                    forward: Vec3::from_array(t.forward()?),
                    up: Vec3::from_array(t.up()?),
                },
            })
        })
        .collect();
    let doors = tng
        .things
        .iter()
        .filter_map(|t| {
            let door = t.component("CTCDoor")?;
            let definition = t.definition()?;
            Some(DoorPlacement {
                definition: definition.to_owned(),
                open: door
                    .fields
                    .iter()
                    .any(|f| f.key == "Open" && f.value == "TRUE"),
                animations: [None, None],
                placement: Placement {
                    role: MeshRole::Plain,
                    mesh: defs.get(definition)?.graphic()?,
                    position: origin + Vec3::from_array(t.position()?),
                    forward: Vec3::from_array(t.forward()?),
                    up: Vec3::from_array(t.up()?),
                },
            })
        })
        .collect();
    let placements = tng
        .things
        .iter()
        .filter(|t| {
            (t.kind == "Building" || t.kind == "Object") && t.component("CTCDoor").is_none()
        })
        .filter_map(|t| {
            let def = defs.get(t.definition()?)?;
            let (position, forward, up) = (t.position()?, t.forward()?, t.up()?);
            let uid = t.uid().unwrap_or(0);
            Some(
                defs.mesh_entries_of(def)
                    .into_iter()
                    .map(move |e| Placement {
                        role: match (e.exterior, e.interior) {
                            (true, _) => MeshRole::Exterior(uid),
                            (_, true) => MeshRole::Interior(uid),
                            _ => MeshRole::Plain,
                        },
                        mesh: e.mesh,
                        position: origin + Vec3::from_array(position),
                        forward: Vec3::from_array(forward),
                        up: Vec3::from_array(up),
                    }),
            )
        })
        .flatten()
        .collect();
    Ok((placements, exits, creatures, doors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fable_data::lev::{Cell, HEIGHT_SCALE};

    fn flat_lev(width: u32, height: u32, z: impl Fn(u32, u32) -> f32) -> Lev {
        let mut cells = Vec::new();
        for j in 0..=height {
            for i in 0..=width {
                let mut raw = [0; 21];
                raw[..5].copy_from_slice(&[0x15, 0, 0, 0, 7]);
                raw[5..9].copy_from_slice(&(z(i, j) / HEIGHT_SCALE).to_le_bytes());
                cells.push(Cell { raw });
            }
        }
        Lev {
            width,
            height,
            themes: vec![],
            sound_value: None,
            sound_themes: vec![],
            cells,
            grid_end: 0,
        }
    }

    #[test]
    fn builds_grid_at_map_origin() {
        let mut mesh = TerrainMesh::default();
        mesh.add_map(&flat_lev(2, 1, |i, _| i as f32), 100.0, 200.0);
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 2 * 6);
        assert_eq!(mesh.vertices[5].position, [102.0, 201.0, 2.0]);
        let (lo, hi) = mesh.bounds();
        assert_eq!(
            (lo, hi),
            (Vec3::new(100.0, 200.0, 0.0), Vec3::new(102.0, 201.0, 2.0))
        );
    }

    #[test]
    fn normals_tilt_away_from_slope() {
        let mut mesh = TerrainMesh::default();
        mesh.add_map(&flat_lev(2, 2, |i, _| i as f32), 0.0, 0.0);
        let n = Vec3::from_array(mesh.vertices[4].normal);
        assert!(
            (n - Vec3::new(-1.0, 0.0, 1.0).normalize()).length() < 1e-6,
            "{n}"
        );
    }

    #[test]
    fn sea_quad_covers_bounds_at_sea_level() {
        let mut mesh = TerrainMesh::default();
        mesh.add_map(&flat_lev(2, 3, |_, _| 1.0), 10.0, 20.0);
        assert!(mesh.sea_quad().is_none());
        mesh.sea_level = Some(SEA_LEVEL);
        let (vertices, _) = mesh.sea_quad().unwrap();
        assert_eq!(vertices[0].position, [10.0, 20.0, SEA_LEVEL]);
        assert_eq!(vertices[2].position, [12.0, 23.0, SEA_LEVEL]);
    }

    #[test]
    fn indices_offset_for_second_map() {
        let mut mesh = TerrainMesh::default();
        mesh.add_map(&flat_lev(1, 1, |_, _| 0.0), 0.0, 0.0);
        mesh.add_map(&flat_lev(1, 1, |_, _| 0.0), 1.0, 0.0);
        assert_eq!(mesh.maps, 2);
        assert_eq!(*mesh.indices.iter().max().unwrap(), 7);
    }

    #[test]
    fn sound_blocks_are_four_units_row_by_row() {
        let area = SoundArea {
            origin: Vec2::new(100.0, 200.0),
            columns: 2,
            themes: vec![Some("A".into()), Some("B".into()), None, Some("D".into())],
        };
        assert_eq!(area.theme_at(Vec2::new(101.0, 201.0)), Some("A"));
        assert_eq!(area.theme_at(Vec2::new(105.0, 203.9)), Some("B"));
        assert_eq!(area.theme_at(Vec2::new(103.0, 205.0)), None);
        assert_eq!(area.theme_at(Vec2::new(107.0, 207.0)), Some("D"));
        assert_eq!(area.theme_at(Vec2::new(109.0, 201.0)), None, "past the row");
        assert_eq!(area.theme_at(Vec2::new(99.0, 201.0)), None);
    }
}
