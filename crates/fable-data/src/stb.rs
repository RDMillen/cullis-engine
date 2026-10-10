//! Static map data inside `Levels/FinalAlbion_RT.stb`. See `docs/formats/stb.md`.
//!
//! The stb is a `BBBB` bank (see [`crate::bbbb`]). Its `__STATIC_MAP_COMMON_HEADER__`
//! entry holds a header per map; each map's own entry, named like the map's Lev,
//! holds a table of LZO-compressed tiles followed by sections not yet decoded.

use crate::reader::Reader;
use crate::{Error, Result, lzo};

pub const COMMON_HEADER_NAME: &str = "__STATIC_MAP_COMMON_HEADER__";

/// Offset of the tile table in a map entry.
pub const TILE_TABLE_OFFSET: usize = 0x800;
const TILE_RECORD_LEN: usize = 36;

/// The `__STATIC_MAP_COMMON_HEADER__` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct CommonHeader {
    pub maps: Vec<MapHeader>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapHeader {
    /// Lev path as named in the wad and stb, e.g. `Data\Levels\FinalAlbion\Arena.lev`.
    pub level_path: String,
    /// Offset of this header within the common header entry.
    pub offset: u32,
    /// Always 1 so far.
    pub unknown_0: u32,
    pub unknown_1: u32,
    pub unknown_2: u32,
    /// Map size in world units.
    pub width: u32,
    pub height: u32,
    /// Map position in world units; equal to the `.wld` `MapX` and `MapY`.
    pub x: u32,
    pub y: u32,
    /// The whole header, from `offset` to the next header (or the end of the entry).
    pub raw: Vec<u8>,
}

impl CommonHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let count = r.u32()?;
        let mut table = Vec::new();
        for _ in 0..count {
            table.push((r.c_string()?, r.u32()?));
        }
        let mut starts: Vec<u32> = table.iter().map(|&(_, o)| o).collect();
        starts.sort_unstable();
        let mut maps = Vec::with_capacity(table.len());
        for (level_path, offset) in table {
            let start = offset as usize;
            let i = starts.partition_point(|&s| s <= offset);
            let end = starts.get(i).map_or(data.len(), |&s| s as usize);
            let raw = data.get(start..end).ok_or_else(|| Error::UnexpectedEof {
                offset: start,
                wanted: end.saturating_sub(start),
            })?;
            let mut h = Reader::new(raw);
            maps.push(MapHeader {
                level_path,
                offset,
                unknown_0: h.u32()?,
                unknown_1: h.u32()?,
                unknown_2: h.u32()?,
                width: h.u32()?,
                height: h.u32()?,
                x: h.u32()?,
                y: h.u32()?,
                raw: raw.to_vec(),
            });
        }
        Ok(Self { maps })
    }

    pub fn map(&self, level_path: &str) -> Option<&MapHeader> {
        self.maps
            .iter()
            .find(|m| m.level_path.eq_ignore_ascii_case(level_path))
    }
}

/// One map's entry: its tile table. The data after the tiles is not decoded yet.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticMap {
    /// Always 1 so far.
    pub unknown_0: u32,
    pub tiles: Vec<Tile>,
}

/// A 16x16-unit square of the map. Empty squares have no tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    /// Offset of the tile's compressed block within the map entry.
    pub offset: u32,
    /// Length of the compressed block, including its 8-byte size header.
    pub size: u32,
    /// Bounding box in world units; z is height.
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub unknown: u32,
}

impl StaticMap {
    pub fn parse(entry: &[u8]) -> Result<Self> {
        let unknown_0 = Reader::new(entry).u32()?;
        let table = entry
            .get(TILE_TABLE_OFFSET..)
            .ok_or_else(|| Error::UnexpectedEof {
                offset: entry.len(),
                wanted: TILE_TABLE_OFFSET.saturating_sub(entry.len()),
            })?;
        let mut r = Reader::new(table);
        let mut tiles = Vec::new();
        // The table ends at an all-zero record, or where the first tile's data starts.
        let mut end = table.len();
        while r.pos() + TILE_RECORD_LEN <= end {
            let offset = r.u32()?;
            let size = r.u32()?;
            if offset == 0 && size == 0 {
                break;
            }
            let mut f = || r.u32().map(f32::from_bits);
            let min = [f()?, f()?, f()?];
            let max = [f()?, f()?, f()?];
            let unknown = r.u32()?;
            end = end.min((offset as usize).saturating_sub(TILE_TABLE_OFFSET));
            tiles.push(Tile {
                offset,
                size,
                min,
                max,
                unknown,
            });
        }
        Ok(Self { unknown_0, tiles })
    }
}

impl Tile {
    /// Decompresses the tile's data. Its format is not decoded yet.
    pub fn decompress(&self, entry: &[u8]) -> Result<Vec<u8>> {
        let start = self.offset as usize;
        let block = entry
            .get(start..start + self.size as usize)
            .ok_or(Error::UnexpectedEof {
                offset: start,
                wanted: self.size as usize,
            })?;
        let mut r = Reader::new(block);
        let out_len = r.u32()? as usize;
        let in_len = r.u32()? as usize;
        if in_len + 8 != block.len() {
            return Err(Error::Invalid {
                offset: start + 4,
                reason: format!(
                    "compressed length {in_len} does not fill the {}-byte tile",
                    block.len()
                ),
            });
        }
        lzo::decompress(&block[8..], out_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(out: &mut Vec<u8>, words: &[u32]) {
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    #[test]
    fn parses_common_header() {
        let mut data = Vec::new();
        put(&mut data, &[2]);
        data.extend_from_slice(b"B.lev\0");
        put(&mut data, &[0x40]);
        data.extend_from_slice(b"A.lev\0");
        put(&mut data, &[0x20]);
        data.resize(0x20, 0);
        put(&mut data, &[1, 0, 0, 96, 128, 3584, 768, 0]);
        put(&mut data, &[1, 0, 0, 64, 64, 2272, 4000]);
        let h = CommonHeader::parse(&data).unwrap();
        let a = h.map("a.lev").unwrap();
        assert_eq!(
            (a.width, a.height, a.x, a.y, a.raw.len()),
            (96, 128, 3584, 768, 0x20)
        );
        let b = h.map("B.lev").unwrap();
        assert_eq!((b.offset, b.x, b.raw.len()), (0x40, 2272, 28));
    }

    #[test]
    fn parses_tiles_and_decompresses() {
        let mut entry = Vec::new();
        put(&mut entry, &[1]);
        entry.resize(TILE_TABLE_OFFSET, 0);
        // One tile record, then the zero terminator.
        let block = [5u32, 9];
        put(&mut entry, &[0x1000, 17]);
        for v in [16.0f32, 32.0, 1.5, 32.0, 48.0, 2.5] {
            put(&mut entry, &[v.to_bits()]);
        }
        put(&mut entry, &[7, 0, 0]);
        entry.resize(0x1000, 0);
        put(&mut entry, &block);
        entry.extend_from_slice(&[22, b'h', b'e', b'l', b'l', b'o', 0x11, 0, 0]);

        let map = StaticMap::parse(&entry).unwrap();
        assert_eq!(map.tiles.len(), 1);
        let tile = map.tiles[0];
        assert_eq!(
            (tile.min, tile.max, tile.unknown),
            ([16.0, 32.0, 1.5], [32.0, 48.0, 2.5], 7)
        );
        assert_eq!(tile.decompress(&entry).unwrap(), b"hello");
    }
}
