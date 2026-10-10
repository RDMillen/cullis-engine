//! `BBBB` bank files: `Levels/FinalAlbion.wad` and `Levels/FinalAlbion_RT.stb`.
//! See `docs/formats/bbbb.md`.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::bank::EntryIndex;
use crate::reader::Reader;
use crate::{Error, Result};

pub const MAGIC: [u8; 4] = *b"BBBB";
pub const HEADER_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BbbbHeader {
    pub version: u32,
    /// Unknown. 2 in the wad, 0xae in the stb.
    pub unknown_a: u32,
    /// Unknown. 1 in both files seen.
    pub unknown_b: u32,
    /// Entry data starts on multiples of this.
    pub block_size: u32,
    /// Equal to the index's entry count in the wad, one more in the stb.
    pub entry_count_a: u32,
    /// Equal to the index's entry count in both files seen.
    pub entry_count_b: u32,
    pub index_offset: u32,
}

impl BbbbHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let found = r.array()?;
        if found != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                found,
            });
        }
        Ok(Self {
            version: r.u32()?,
            unknown_a: r.u32()?,
            unknown_b: r.u32()?,
            block_size: r.u32()?,
            entry_count_a: r.u32()?,
            entry_count_b: r.u32()?,
            index_offset: r.u32()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BbbbFile {
    pub header: BbbbHeader,
    pub index: EntryIndex,
    /// Raw index bytes, from `index_offset` to the end of the file.
    pub index_bytes: Vec<u8>,
}

impl BbbbFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::read(&mut BufReader::new(File::open(path)?))
    }

    pub fn read<R: Read + Seek>(r: &mut R) -> Result<Self> {
        let mut head = [0; HEADER_LEN];
        r.read_exact(&mut head)?;
        let header = BbbbHeader::parse(&head)?;
        r.seek(SeekFrom::Start(header.index_offset.into()))?;
        let mut index_bytes = Vec::new();
        r.read_to_end(&mut index_bytes)?;
        let index = EntryIndex::parse(&index_bytes)?;
        Ok(Self {
            header,
            index,
            index_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header() {
        let mut data = MAGIC.to_vec();
        for v in [0x73u32, 2, 1, 0x800, 3, 3, 0x1800] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let h = BbbbHeader::parse(&data).unwrap();
        assert_eq!(
            (h.version, h.block_size, h.entry_count_b, h.index_offset),
            (0x73, 0x800, 3, 0x1800)
        );
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(
            BbbbHeader::parse(&[b'B', b'I', b'G', b'B', 0, 0, 0, 0]),
            Err(Error::BadMagic { .. })
        ));
    }
}
