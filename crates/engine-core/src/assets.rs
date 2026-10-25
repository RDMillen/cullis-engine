//! The asset cache: the game's archives, each opened and indexed once.
//!
//! An [`AssetCache`] clone shares the indexes read so far (behind an `Arc`)
//! and opens its own file handles, so threads loading in the background each
//! take a clone and need no locks.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fable_data::bank::Entry;
use fable_data::big::BigFile;

use crate::{Error, Result};

/// The archives and banks the engine reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Archive {
    /// `graphics/graphics.big`, `MBANK_ALLMESHES`: meshes and animations.
    Meshes,
    /// `graphics/pc/textures.big`, `GBANK_MAIN_PC`.
    Textures,
    /// `Misc/pc/effects.big`, `PARTICLE_MAIN_PC`: particle emitters.
    Effects,
}

impl Archive {
    /// The archive's file, under the data directory.
    pub fn file(self) -> &'static str {
        match self {
            Archive::Meshes => "graphics/graphics.big",
            Archive::Textures => "graphics/pc/textures.big",
            Archive::Effects => "Misc/pc/effects.big",
        }
    }

    /// The bank read from it.
    pub fn bank(self) -> &'static str {
        match self {
            Archive::Meshes => "MBANK_ALLMESHES",
            Archive::Textures => "GBANK_MAIN_PC",
            Archive::Effects => "PARTICLE_MAIN_PC",
        }
    }
}

/// One bank's entries, by id and by name.
#[derive(Debug, Default)]
pub struct Index {
    pub entries: Vec<Entry>,
    by_id: HashMap<u32, usize>,
    by_name: HashMap<String, usize>,
}

impl Index {
    fn new(entries: Vec<Entry>) -> Self {
        let by_id = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let by_name = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i))
            .collect();
        Self {
            entries,
            by_id,
            by_name,
        }
    }

    pub fn by_id(&self, id: u32) -> Option<&Entry> {
        self.entries.get(*self.by_id.get(&id)?)
    }

    pub fn by_name(&self, name: &str) -> Option<&Entry> {
        self.entries.get(*self.by_name.get(name)?)
    }
}

/// The game's archives, opened and indexed when first used.
pub struct AssetCache {
    data_dir: Arc<PathBuf>,
    indexes: Arc<Mutex<HashMap<Archive, Arc<Index>>>>,
    files: HashMap<Archive, BufReader<File>>,
}

impl Clone for AssetCache {
    /// Shares the indexes; file handles are opened anew when needed.
    fn clone(&self) -> Self {
        Self {
            data_dir: self.data_dir.clone(),
            indexes: self.indexes.clone(),
            files: HashMap::new(),
        }
    }
}

impl AssetCache {
    /// A cache over the game's data directory (`$FABLE_DIR/data`).
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: Arc::new(data_dir.to_owned()),
            indexes: Arc::default(),
            files: HashMap::new(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn file(&mut self, archive: Archive) -> Result<&mut BufReader<File>> {
        if !self.files.contains_key(&archive) {
            let file = BufReader::new(File::open(self.data_dir.join(archive.file()))?);
            self.files.insert(archive, file);
        }
        Ok(self.files.get_mut(&archive).expect("just opened"))
    }

    /// The archive's bank index, read on first use and shared by all clones.
    pub fn index(&mut self, archive: Archive) -> Result<Arc<Index>> {
        if let Some(index) = self
            .indexes
            .lock()
            .ok()
            .and_then(|m| m.get(&archive).cloned())
        {
            return Ok(index);
        }
        let file = self.file(archive)?;
        let big = BigFile::read(file)?;
        let bank = big.bank(archive.bank()).ok_or(Error::NoBank {
            archive: archive.file(),
            bank: archive.bank(),
        })?;
        let index = Arc::new(Index::new(bank.read_index(file)?.entries));
        log::debug!(
            "indexed {} ({} entries)",
            archive.file(),
            index.entries.len()
        );
        if let Ok(mut m) = self.indexes.lock() {
            m.insert(archive, index.clone());
        }
        Ok(index)
    }

    /// The data of entry `id`.
    pub fn read(&mut self, archive: Archive, id: u32) -> Result<(Entry, Vec<u8>)> {
        let index = self.index(archive)?;
        let entry = index.by_id(id).ok_or_else(|| Error::NoEntry {
            bank: archive.bank(),
            entry: id.to_string(),
        })?;
        let data = entry.read_data(self.file(archive)?)?;
        Ok((entry.clone(), data))
    }

    /// The id of the entry named `name`.
    pub fn id_of(&mut self, archive: Archive, name: &str) -> Option<u32> {
        Some(self.index(archive).ok()?.by_name(name)?.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_indexes_but_not_files() {
        let cache = AssetCache::new(Path::new("/nonexistent"));
        let mut a = cache.clone();
        assert!(a.index(Archive::Textures).is_err(), "no such file");
        let index = Arc::new(Index::new(Vec::new()));
        cache
            .indexes
            .lock()
            .unwrap()
            .insert(Archive::Textures, index.clone());
        // An index read by one clone serves the others.
        assert!(Arc::ptr_eq(&a.index(Archive::Textures).unwrap(), &index));
        assert!(a.files.is_empty());
    }
}
