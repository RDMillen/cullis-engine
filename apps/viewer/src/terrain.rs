//! Builds a terrain mesh for a region from its maps' Lev heightmaps.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use fable_data::bbbb::BbbbFile;
use fable_data::defs::Defs;
use fable_data::lev::Lev;
use fable_data::tng::Tng;
use fable_data::wld::Wld;
use glam::Vec3;

use crate::model::Placement;

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

/// World units: x and y as in the `.wld`, z is height.
#[derive(Debug, Default)]
pub struct TerrainMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub maps: usize,
    /// Height of a sea plane covering the terrain bounds, if the region has sea.
    pub sea_level: Option<f32>,
    /// Which themes cover the ground, if loaded.
    pub themes: Option<ThemeGrid>,
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
        // beside paths and on slopes; in the game no water shows there (Ross,
        // comparing). They probably mark ground meant to lie under a water surface,
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

/// Loads every map a region contains or sees.
pub fn load_region(
    data_dir: &Path,
    wld: &Wld,
    region: &str,
    defs: &Defs,
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
        let lev = Lev::parse(&entry.read_data(&mut file)?)?;
        mesh.add_map(&lev, map.x as f32, map.y as f32);
        maps.push((lev, map.x as f32, map.y as f32));
        let origin = Vec3::new(map.x as f32, map.y as f32, 0.0);
        placements.extend(load_placements(&wad, &mut file, level, origin, defs)?);
    }
    if !maps.is_empty() {
        let (lo, hi) = mesh.bounds();
        let size = [(hi.x - lo.x) as u32 + 1, (hi.y - lo.y) as u32 + 1];
        let mut grid = ThemeGrid::new([lo.x, lo.y], size);
        for (lev, x, y) in &maps {
            grid.add_map(lev, *x, *y, defs);
        }
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
fn load_placements(
    wad: &BbbbFile,
    file: &mut BufReader<File>,
    level: &str,
    origin: Vec3,
    defs: &Defs,
) -> Result<Vec<Placement>, Box<dyn std::error::Error>> {
    let tng_name = format!("Data\\Levels\\{}", level.replace(".lev", ".tng"));
    let Some(entry) = wad.index.by_name(&tng_name) else {
        return Ok(Vec::new());
    };
    let text: String = entry.read_data(file)?.iter().map(|&b| b as char).collect();
    let tng = Tng::parse(&text)?;
    Ok(tng
        .things
        .iter()
        .filter(|t| t.kind == "Building" || t.kind == "Object")
        .filter_map(|t| {
            let mesh = defs.get(t.definition()?)?.graphic()?;
            Some(Placement {
                mesh,
                position: origin + Vec3::from_array(t.position()?),
                forward: Vec3::from_array(t.forward()?),
                up: Vec3::from_array(t.up()?),
            })
        })
        .collect())
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
}
