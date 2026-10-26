//! A region's data beyond its geometry: its exits, what its creatures stand
//! in, and what it sounds like (its music and which ambient bank plays where).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fable_data::atmos::{self, Enums};
use fable_data::defs::Defs;
use glam::{Vec2, Vec3};

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

/// What plays where in a region.
#[derive(Debug, Clone, Default)]
pub struct Soundscape {
    /// The region's music file.
    pub music: Option<PathBuf>,
    areas: Vec<SoundArea>,
    /// Sound theme name to its ambient bank (`.met`).
    banks: HashMap<String, PathBuf>,
}

impl Soundscape {
    /// Works out region `region_def`'s music and its sound map's banks.
    pub fn resolve(data_dir: &Path, region_def: &str, defs: &Defs, areas: Vec<SoundArea>) -> Self {
        let sound_dir = data_dir.join("Sound");
        let Ok(enums) = Enums::open(data_dir) else {
            return Self::default();
        };
        let music = defs
            .get(region_def)
            .and_then(atmos::region_music_set)
            .and_then(|set| enums.name("EMusicSetType", i64::from(set)))
            .and_then(|name| atmos::music_file(&sound_dir, name));
        let mut banks = HashMap::new();
        for theme in areas.iter().flat_map(|a| a.themes.iter().flatten()) {
            if banks.contains_key(theme) {
                continue;
            }
            if let Some(bank) = defs
                .get(theme)
                .and_then(atmos::theme_atmos)
                .and_then(|a| enums.name("EAtmosType", i64::from(a)))
                .and_then(|name| atmos::atmos_bank(&sound_dir, name))
            {
                banks.insert(theme.clone(), bank);
            }
        }
        Self {
            music,
            areas,
            banks,
        }
    }

    /// The ambient bank of the block at `p`.
    pub fn bank_at(&self, p: Vec2) -> Option<&PathBuf> {
        let theme = self.areas.iter().find_map(|a| a.theme_at(p))?;
        self.banks.get(theme)
    }

    /// Names of the banks the region uses (for the log).
    pub fn bank_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .banks
            .values()
            .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
            .collect();
        names.sort();
        names.dedup();
        names
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
