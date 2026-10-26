//! The terrain as the renderer takes it: vertices and indices, which themes
//! cover the ground, the sea plane and the navigation overlay.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
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

/// A region's terrain, borrowed from whatever loaded it.
pub struct TerrainGeometry<'a> {
    pub vertices: &'a [Vertex],
    pub indices: &'a [u32],
    pub themes: Option<&'a ThemeGrid>,
    /// A sea plane, if the region has sea.
    pub sea: Option<([Vertex; 4], [u32; 6])>,
    /// Navigation leaves as coloured quads (`normal` holds the colour).
    pub nav_vertices: &'a [Vertex],
    pub nav_indices: &'a [u32],
}
