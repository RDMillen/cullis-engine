//! Lev map files, stored in `Levels/FinalAlbion.wad`. See `docs/formats/lev.md`.
//!
//! Decoded so far: map size, texture themes, sound themes and the heightmap grid.
//! Everything after the grid (navigation and more) is not decoded yet.

use crate::reader::{Reader, latin1};
use crate::{Error, Result};

/// Heights are stored as small floats; multiplying by this gives world units,
/// the same units as the stb tile bounds and the `.wld` map positions.
pub const HEIGHT_SCALE: f32 = 2048.0;

const SIZE_OFFSET: usize = 0x26;
const THEMES_OFFSET: usize = 0xaf;
const THEME_SLOTS: usize = 255;
const THEME_NAME_LEN: usize = 128;
const SOUND_COUNT_OFFSET: usize = 0x8433;
const SOUND_THEMES_OFFSET: usize = 0x10837;
const CELL_LEN: usize = 21;
const CELL_PREFIX: [u8; 5] = [0x15, 0, 0, 0, 7];

#[derive(Debug, Clone, PartialEq)]
pub struct Lev {
    /// Map size in world units; the grid has `(width + 1) * (height + 1)` points.
    pub width: u32,
    pub height: u32,
    /// Texture themes, by slot id. Empty slots are left out.
    pub themes: Vec<Theme>,
    /// A per-map value that the stb common header repeats. Absent (and zero in the
    /// stb) in one map, `HauntedHouse_Filler_01`.
    pub sound_value: Option<u32>,
    pub sound_themes: Vec<String>,
    /// Row-major grid points, `width + 1` per row, starting at the map's origin.
    pub cells: Vec<Cell>,
    /// Offset of the first byte after the grid.
    pub grid_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The 21 bytes as stored. Byte 4 is always 7; bytes 5..9 are the height.
    pub raw: [u8; CELL_LEN],
}

impl Cell {
    /// Height in world units.
    pub fn height(&self) -> f32 {
        f32::from_le_bytes(self.raw[5..9].try_into().unwrap()) * HEIGHT_SCALE
    }
}

impl Lev {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let at = |offset: usize| Reader::new(data.get(offset..).unwrap_or_default());
        let mut r = at(SIZE_OFFSET);
        let width = r.u32()?;
        let height = r.u32()?;

        let mut themes = Vec::new();
        let mut r = at(THEMES_OFFSET);
        for _ in 0..THEME_SLOTS {
            let id = r.u32()?;
            let name = r.bytes(THEME_NAME_LEN)?;
            let len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            if len > 0 {
                themes.push(Theme {
                    id,
                    name: latin1(&name[..len]),
                });
            }
        }

        // The count includes the optional sound value as an entry.
        let entries = at(SOUND_COUNT_OFFSET).u32()? as usize;
        let mut r = at(SOUND_THEMES_OFFSET);
        let sound_value = if looks_like_string(data, SOUND_THEMES_OFFSET) {
            None
        } else {
            Some(r.u32()?)
        };
        let mut sound_themes = Vec::new();
        for _ in 0..entries.saturating_sub(1) {
            sound_themes.push(r.len_string()?);
        }

        let start = SOUND_THEMES_OFFSET + r.pos();
        let count = (width as usize + 1) * (height as usize + 1);
        let grid_end = start + count * CELL_LEN;
        let grid = data.get(start..grid_end).ok_or(Error::UnexpectedEof {
            offset: start,
            wanted: count * CELL_LEN,
        })?;
        let mut cells = Vec::with_capacity(count);
        for (i, chunk) in grid.chunks_exact(CELL_LEN).enumerate() {
            if chunk[..5] != CELL_PREFIX {
                return Err(Error::Invalid {
                    offset: start + i * CELL_LEN,
                    reason: format!("grid cell {i} starts {:02x?}", &chunk[..5]),
                });
            }
            cells.push(Cell {
                raw: chunk.try_into().unwrap(),
            });
        }
        Ok(Self {
            width,
            height,
            themes,
            sound_value,
            sound_themes,
            cells,
            grid_end,
        })
    }

    /// Grid point at (x, y) from the map's origin, in world units (0..=width, 0..=height).
    pub fn cell(&self, x: u32, y: u32) -> Option<&Cell> {
        if x > self.width || y > self.height {
            return None;
        }
        self.cells.get((y * (self.width + 1) + x) as usize)
    }
}

/// A `u32` length followed by that many printable ASCII bytes.
fn looks_like_string(data: &[u8], offset: usize) -> bool {
    let Some(len) = data
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
    else {
        return false;
    };
    (1..256).contains(&len)
        && data
            .get(offset + 4..offset + 4 + len)
            .is_some_and(|s| s.iter().all(|&b| b.is_ascii_graphic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data: &mut [u8], offset: usize, bytes: &[u8]) {
        data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    fn sample(sound_value: Option<u32>) -> Vec<u8> {
        let mut data = vec![0; SOUND_THEMES_OFFSET];
        put(&mut data, SIZE_OFFSET, &[1, 0, 0, 0, 2, 0, 0, 0]);
        put(&mut data, THEMES_OFFSET + 132, &7u32.to_le_bytes());
        put(&mut data, THEMES_OFFSET + 136, b"GROUND_GRASS");
        put(&mut data, SOUND_COUNT_OFFSET, &2u32.to_le_bytes());
        if let Some(v) = sound_value {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"WIND");
        for i in 0..6u8 {
            let mut cell = [0; CELL_LEN];
            cell[..5].copy_from_slice(&CELL_PREFIX);
            cell[5..9].copy_from_slice(&(f32::from(i) / HEIGHT_SCALE).to_le_bytes());
            data.extend_from_slice(&cell);
        }
        data
    }

    #[test]
    fn parses_with_and_without_sound_value() {
        for value in [Some(0x79ea_86d2), None] {
            let lev = Lev::parse(&sample(value)).unwrap();
            assert_eq!((lev.width, lev.height), (1, 2));
            assert_eq!(
                lev.themes,
                [Theme {
                    id: 7,
                    name: "GROUND_GRASS".into()
                }]
            );
            assert_eq!(lev.sound_value, value);
            assert_eq!(lev.sound_themes, ["WIND"]);
            assert_eq!(lev.cells.len(), 6);
            assert_eq!(lev.cell(1, 2).unwrap().height(), 5.0);
            assert_eq!(lev.cell(0, 1).unwrap().height(), 2.0);
            assert!(lev.cell(2, 0).is_none());
        }
    }

    #[test]
    fn rejects_bad_cell() {
        let mut data = sample(None);
        let last = data.len() - CELL_LEN;
        data[last + 4] = 8;
        assert!(matches!(Lev::parse(&data), Err(Error::Invalid { .. })));
    }
}
