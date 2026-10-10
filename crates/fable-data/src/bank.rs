//! The entry index shared by `BBBB` bank files (`.wad`, `.stb`) and by each bank
//! inside a `BIGB` archive (`.big`). See `docs/formats/bank-index.md`.

use std::io::{Read, Seek, SeekFrom};

use crate::reader::{Reader, put_len_string, put_u32};
use crate::{Error, Result};

/// Value that starts every entry record.
pub const ENTRY_MARKER: u32 = 0x2a;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryIndex {
    /// How many entries there are of each type, in file order.
    pub type_counts: Vec<TypeCount>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeCount {
    pub entry_type: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    pub entry_type: u32,
    pub size: u32,
    /// Absolute offset of the entry's data in the containing file.
    pub offset: u32,
    /// Unknown. Zero in `BBBB` banks; varies per bank in `BIGB` archives.
    pub unknown_a: u32,
    pub name: String,
    /// Unknown. Zero in `BBBB` banks; varies per entry in `BIGB` archives.
    pub unknown_b: u32,
    /// Strings recorded by the original tools, usually one source path. May be empty.
    pub sources: Vec<String>,
    /// Type-specific info block, not yet decoded.
    pub info: Vec<u8>,
}

impl EntryIndex {
    /// Parses an index from the start of `data`. Trailing bytes are ignored;
    /// [`EntryIndex::encoded_len`] says how many were used.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let type_count = r.u32()?;
        let mut type_counts = Vec::new();
        for _ in 0..type_count {
            type_counts.push(TypeCount {
                entry_type: r.u32()?,
                count: r.u32()?,
            });
        }
        let total: usize = type_counts.iter().map(|t| t.count as usize).sum();
        let mut entries = Vec::with_capacity(total.min(data.len() / 32));
        for _ in 0..total {
            entries.push(Entry::parse(&mut r)?);
        }
        Ok(Self {
            type_counts,
            entries,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_u32(&mut out, self.type_counts.len() as u32);
        for t in &self.type_counts {
            put_u32(&mut out, t.entry_type);
            put_u32(&mut out, t.count);
        }
        for e in &self.entries {
            e.write(&mut out);
        }
        out
    }

    pub fn encoded_len(&self) -> usize {
        self.to_bytes().len()
    }

    pub fn by_name(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

impl Entry {
    fn parse(r: &mut Reader) -> Result<Self> {
        let at = r.pos();
        let marker = r.u32()?;
        if marker != ENTRY_MARKER {
            return Err(Error::Invalid {
                offset: at,
                reason: format!("entry marker {marker:#x}, expected {ENTRY_MARKER:#x}"),
            });
        }
        let id = r.u32()?;
        let entry_type = r.u32()?;
        let size = r.u32()?;
        let offset = r.u32()?;
        let unknown_a = r.u32()?;
        let name = r.len_string()?;
        let unknown_b = r.u32()?;
        let source_count = r.u32()?;
        let mut sources = Vec::new();
        for _ in 0..source_count {
            sources.push(r.len_string()?);
        }
        let info_len = r.u32()? as usize;
        let info = r.bytes(info_len)?.to_vec();
        Ok(Self {
            id,
            entry_type,
            size,
            offset,
            unknown_a,
            name,
            unknown_b,
            sources,
            info,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        put_u32(out, ENTRY_MARKER);
        put_u32(out, self.id);
        put_u32(out, self.entry_type);
        put_u32(out, self.size);
        put_u32(out, self.offset);
        put_u32(out, self.unknown_a);
        put_len_string(out, &self.name);
        put_u32(out, self.unknown_b);
        put_u32(out, self.sources.len() as u32);
        for s in &self.sources {
            put_len_string(out, s);
        }
        put_u32(out, self.info.len() as u32);
        out.extend_from_slice(&self.info);
    }

    /// Reads this entry's data from the file that holds the index.
    pub fn read_data<R: Read + Seek>(&self, r: &mut R) -> Result<Vec<u8>> {
        r.seek(SeekFrom::Start(self.offset.into()))?;
        let mut data = vec![0; self.size as usize];
        r.read_exact(&mut data)?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EntryIndex {
        EntryIndex {
            type_counts: vec![
                TypeCount {
                    entry_type: 0,
                    count: 1,
                },
                TypeCount {
                    entry_type: 3,
                    count: 1,
                },
            ],
            entries: vec![
                Entry {
                    id: 1,
                    entry_type: 0,
                    size: 100,
                    offset: 0x800,
                    unknown_a: 0,
                    name: "Data\\Levels\\Test.lev".into(),
                    unknown_b: 0,
                    sources: vec![String::new()],
                    info: vec![1, 2, 3, 4],
                },
                Entry {
                    id: 2,
                    entry_type: 3,
                    size: 7,
                    offset: 0x1000,
                    unknown_a: 0xdead_beef,
                    name: "MESH_TEST".into(),
                    unknown_b: 5,
                    sources: vec!["\\Dev\\test.bbm".into(), "two".into()],
                    info: vec![],
                },
            ],
        }
    }

    #[test]
    fn round_trips() {
        let index = sample();
        let bytes = index.to_bytes();
        assert_eq!(EntryIndex::parse(&bytes).unwrap(), index);
    }

    #[test]
    fn rejects_bad_marker() {
        let mut bytes = sample().to_bytes();
        bytes[20] = 0x2b; // first entry's marker
        assert!(matches!(
            EntryIndex::parse(&bytes),
            Err(Error::Invalid { offset: 20, .. })
        ));
    }

    #[test]
    fn rejects_truncated_index() {
        let bytes = sample().to_bytes();
        assert!(matches!(
            EntryIndex::parse(&bytes[..bytes.len() - 1]),
            Err(Error::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn finds_by_name_ignoring_case() {
        assert_eq!(sample().by_name("mesh_test").unwrap().id, 2);
    }
}
