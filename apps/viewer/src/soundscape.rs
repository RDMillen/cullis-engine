//! Music and ambience for where the Hero is.
//!
//! A region's music is its definition's music set (`MUSIC_SET_OAKVALE` →
//! `Sound/OAKVALE.OGG`), looped. Its ambience comes from the sound map: each
//! 4x4-unit block plays a sound theme, whose atmosphere names an ambient bank
//! (`SOUND_THEME_COUNTRY` → `ATMOS_COUNTRY` → `Sound/country.met`). A bank's
//! looping beds play while the Hero is in its blocks, and its day sounds (birds)
//! now and then, picked by their weights and panned at random. How often spot
//! sounds play is a stand-in: what the `.met` playback fields mean is not
//! known yet.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use engine_audio::{Channel, Settings, Sound, SoundId};
use fable_data::atmos::{self, Enums};
use fable_data::defs::Defs;
use fable_data::lug::AmbientBank;
use glam::Vec2;

use crate::audio::Audio;
use crate::terrain::SoundArea;

/// Seconds music crossfades over, ambience beds fade over, and that a new
/// ambience must be wanted before it takes over (so a path along a boundary
/// does not flip between two).
const MUSIC_FADE: f32 = 2.0;
const BED_FADE: f32 = 1.5;
const AMBIENCE_SETTLE: f32 = 0.75;

/// Mix levels of the channels against the voices (1).
pub const MUSIC_VOLUME: f32 = 0.35;
pub const AMBIENCE_VOLUME: f32 = 0.5;

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

/// Plays a region's music, decoding it in the background, and crossfading
/// from one piece to the next.
#[derive(Default)]
pub struct Music {
    playing: Option<PathBuf>,
    id: Option<SoundId>,
    pending: Option<(PathBuf, Receiver<Result<Sound, String>>)>,
}

impl Music {
    /// Plays `wanted` (looped), or stops when it is None. A piece already
    /// playing carries on.
    pub fn update(&mut self, wanted: Option<&Path>, audio: &Audio) {
        if let Some((path, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(sound)) => {
                    if wanted == Some(path.as_path()) {
                        self.id = audio.play(
                            sound,
                            Channel::Music,
                            Settings {
                                looping: true,
                                fade_in: MUSIC_FADE,
                                ..Settings::default()
                            },
                        );
                        self.playing = Some(path.clone());
                    }
                    self.pending = None;
                }
                Ok(Err(e)) => {
                    eprintln!("music {}: {e}", path.display());
                    self.playing = Some(path.clone());
                    self.pending = None;
                }
                Err(_) => {}
            }
        }
        if wanted == self.playing.as_deref()
            || self
                .pending
                .as_ref()
                .is_some_and(|(p, _)| Some(p.as_path()) == wanted)
        {
            return;
        }
        if let Some(id) = self.id.take() {
            audio.fade_out(id, MUSIC_FADE);
        }
        self.playing = None;
        let Some(path) = wanted else {
            return;
        };
        let (tx, rx) = channel();
        let file = path.to_owned();
        std::thread::spawn(move || {
            let result = std::fs::read(&file)
                .map_err(|e| e.to_string())
                .and_then(|bytes| engine_audio::ogg::decode(&bytes));
            let _ = tx.send(result);
        });
        self.pending = Some((path.to_owned(), rx));
    }
}

/// A decoded ambient bank: looping beds with their volumes, and spot sounds
/// with their weights and volumes.
struct Bank {
    beds: Vec<(Sound, f32)>,
    spots: Vec<(Sound, u32, f32)>,
}

fn load_bank(met: &Path) -> Result<Bank, String> {
    let bank = AmbientBank::parse_met(&std::fs::read(met).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut lug = BufReader::new(File::open(met.with_extension("lug")).map_err(|e| e.to_string())?);
    let mut out = Bank {
        beds: Vec::new(),
        spots: Vec::new(),
    };
    let mut decoded: HashMap<u32, Sound> = HashMap::new();
    for play in &bank.plays {
        // Day sounds only for now (group 2 is night).
        if play.group == 2 {
            continue;
        }
        let sound = match decoded.get(&play.sound) {
            Some(s) => s.clone(),
            None => {
                let Some(info) = bank.sounds.iter().find(|s| s.index == play.sound) else {
                    continue;
                };
                let samples = bank
                    .read_samples(&mut lug, info)
                    .map_err(|e| e.to_string())?;
                let sound = Sound::new(samples, info.sample_rate, info.channels);
                decoded.insert(play.sound, sound.clone());
                sound
            }
        };
        let volume = f32::from(play.volume) / 100.0;
        if play.repeats >= 9999 {
            out.beds.push((sound, volume));
        } else {
            out.spots.push((sound, play.frequency.max(1), volume));
        }
    }
    Ok(out)
}

/// Plays the ambience of the block the Hero is in.
#[derive(Default)]
pub struct Ambience {
    current: Option<PathBuf>,
    /// A different bank wanted, and for how long.
    wanting: Option<(PathBuf, f32)>,
    beds: Vec<SoundId>,
    loaded: HashMap<PathBuf, Arc<Bank>>,
    pending: HashMap<PathBuf, Receiver<Result<Bank, String>>>,
    /// Seconds to the next spot sound.
    next_spot: f32,
    rng: u64,
}

impl Ambience {
    fn random(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Advances `dt` seconds with `wanted` the bank to play (None: silence).
    pub fn update(&mut self, dt: f32, wanted: Option<&Path>, audio: &Audio) {
        // Banks that finished loading.
        let done: Vec<PathBuf> = self
            .pending
            .iter()
            .filter_map(|(p, rx)| match rx.try_recv() {
                Ok(Ok(bank)) => {
                    self.loaded.insert(p.clone(), Arc::new(bank));
                    Some(p.clone())
                }
                Ok(Err(e)) => {
                    eprintln!("ambience {}: {e}", p.display());
                    self.loaded.insert(
                        p.clone(),
                        Arc::new(Bank {
                            beds: Vec::new(),
                            spots: Vec::new(),
                        }),
                    );
                    Some(p.clone())
                }
                Err(_) => None,
            })
            .collect();
        for p in done {
            self.pending.remove(&p);
        }
        let Some(path) = wanted else {
            self.switch(None, audio);
            return;
        };
        let Some(bank) = self.loaded.get(path).cloned() else {
            if !self.pending.contains_key(path) {
                let (tx, rx) = channel();
                let met = path.to_owned();
                std::thread::spawn(move || {
                    let _ = tx.send(load_bank(&met));
                });
                self.pending.insert(path.to_owned(), rx);
            }
            return;
        };
        if self.current.as_deref() != Some(path) {
            // Switch at once into silence; else once the new bank has been
            // wanted for a moment.
            let waited = match &mut self.wanting {
                Some((p, t)) if p.as_path() == path => {
                    *t += dt;
                    *t
                }
                _ => {
                    self.wanting = Some((path.to_owned(), 0.0));
                    0.0
                }
            };
            if self.current.is_none() || waited >= AMBIENCE_SETTLE {
                self.wanting = None;
                self.switch(Some((path, &bank)), audio);
            }
        } else {
            self.wanting = None;
        }
        // Now and then a spot sound, picked by weight, somewhere to the side.
        self.next_spot -= dt;
        if self.next_spot <= 0.0 {
            self.next_spot = 1.5 + 4.5 * self.random();
            let total: u32 = bank.spots.iter().map(|s| s.1).sum();
            if total > 0 {
                let mut pick = (self.random() * total as f32) as u32;
                let chosen = bank.spots.iter().find(|s| {
                    if pick < s.1 {
                        true
                    } else {
                        pick -= s.1;
                        false
                    }
                });
                if let Some((sound, _, volume)) = chosen {
                    let settings = Settings {
                        volume: volume * (0.5 + 0.5 * self.random()),
                        pan: self.random() * 1.6 - 0.8,
                        ..Settings::default()
                    };
                    audio.play(sound.clone(), Channel::Ambience, settings);
                }
            }
        }
    }

    /// Stops the beds playing and starts `to`'s.
    fn switch(&mut self, to: Option<(&Path, &Bank)>, audio: &Audio) {
        for id in self.beds.drain(..) {
            audio.fade_out(id, BED_FADE);
        }
        self.current = to.map(|(p, _)| p.to_owned());
        if let Some((p, bank)) = to {
            eprintln!(
                "ambience: {} ({} beds, {} spot sounds)",
                p.file_stem().and_then(|s| s.to_str()).unwrap_or("?"),
                bank.beds.len(),
                bank.spots.len()
            );
        }
        if let Some((_, bank)) = to {
            for (bed, volume) in &bank.beds {
                let settings = Settings {
                    volume: *volume,
                    looping: true,
                    fade_in: BED_FADE,
                    ..Settings::default()
                };
                if let Some(id) = audio.play(bed.clone(), Channel::Ambience, settings) {
                    self.beds.push(id);
                }
            }
        }
    }
}
