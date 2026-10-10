//! Builds a terrain mesh for a region from its maps' Lev heightmaps.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use fable_data::bbbb::BbbbFile;
use fable_data::lev::Lev;
use fable_data::wld::Wld;
use glam::Vec3;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// World units: x and y as in the `.wld`, z is height.
#[derive(Debug, Default)]
pub struct TerrainMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub maps: usize,
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
) -> Result<TerrainMesh, Box<dyn std::error::Error>> {
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
    }
    Ok(mesh)
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
    fn indices_offset_for_second_map() {
        let mut mesh = TerrainMesh::default();
        mesh.add_map(&flat_lev(1, 1, |_, _| 0.0), 0.0, 0.0);
        mesh.add_map(&flat_lev(1, 1, |_, _| 0.0), 1.0, 0.0);
        assert_eq!(mesh.maps, 2);
        assert_eq!(*mesh.indices.iter().max().unwrap(), 7);
    }
}
