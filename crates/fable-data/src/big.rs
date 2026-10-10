//! `BIGB` archives (`.big`): a table of named banks, each with its own entry index.
//! See `docs/formats/big.md`.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::bank::EntryIndex;
use crate::reader::Reader;
use crate::{Error, Result};

pub const MAGIC: [u8; 4] = *b"BIGB";
pub const HEADER_LEN: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigFile {
    pub version: u32,
    pub bank_table_offset: u32,
    /// Equal to the bank table's length in every file seen so far.
    pub bank_table_len: u32,
    pub banks: Vec<BigBank>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigBank {
    /// For example `MBANK_ALLMESHES` or `GBANK_MAIN_PC`; `banks_dvd.ini` maps names to files.
    pub name: String,
    /// Unknown. Same value for every bank in one file.
    pub unknown: u32,
    pub entry_count: u32,
    pub index_offset: u32,
    pub index_len: u32,
    pub block_size: u32,
}

impl BigFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::read(&mut BufReader::new(File::open(path)?))
    }

    pub fn read<R: Read + Seek>(r: &mut R) -> Result<Self> {
        let mut head = [0; HEADER_LEN];
        r.read_exact(&mut head)?;
        let mut h = Reader::new(&head);
        let found = h.array()?;
        if found != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                found,
            });
        }
        let version = h.u32()?;
        let bank_table_offset = h.u32()?;
        let bank_table_len = h.u32()?;

        r.seek(SeekFrom::Start(bank_table_offset.into()))?;
        let mut table = Vec::new();
        r.read_to_end(&mut table)?;
        let mut t = Reader::new(&table);
        let bank_count = t.u32()?;
        let mut banks = Vec::new();
        for _ in 0..bank_count {
            banks.push(BigBank {
                name: t.c_string()?,
                unknown: t.u32()?,
                entry_count: t.u32()?,
                index_offset: t.u32()?,
                index_len: t.u32()?,
                block_size: t.u32()?,
            });
        }
        Ok(Self {
            version,
            bank_table_offset,
            bank_table_len,
            banks,
        })
    }

    pub fn bank(&self, name: &str) -> Option<&BigBank> {
        self.banks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
    }
}

impl BigBank {
    pub fn read_index_bytes<R: Read + Seek>(&self, r: &mut R) -> Result<Vec<u8>> {
        r.seek(SeekFrom::Start(self.index_offset.into()))?;
        let mut data = vec![0; self.index_len as usize];
        r.read_exact(&mut data)?;
        Ok(data)
    }

    pub fn read_index<R: Read + Seek>(&self, r: &mut R) -> Result<EntryIndex> {
        EntryIndex::parse(&self.read_index_bytes(r)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_bank_table() {
        let mut data = MAGIC.to_vec();
        for v in [100u32, 16, 0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(b"MBANK_TEST\0");
        for v in [0xcdu32, 2, 0x800, 0x40, 0x800] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let big = BigFile::read(&mut Cursor::new(data)).unwrap();
        assert_eq!(big.version, 100);
        let bank = big.bank("mbank_test").unwrap();
        assert_eq!(
            (bank.entry_count, bank.index_offset, bank.index_len),
            (2, 0x800, 0x40)
        );
    }
}
