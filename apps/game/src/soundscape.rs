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
use fable_data::lug::AmbientBank;

use crate::audio::Audio;
pub use engine_world::region::Soundscape;

/// Seconds music crossfades over, ambience beds fade over, and that a new
/// ambience must be wanted before it takes over (so a path along a boundary
/// does not flip between two).
const MUSIC_FADE: f32 = 2.0;
const BED_FADE: f32 = 1.5;
const AMBIENCE_SETTLE: f32 = 0.75;

/// Mix levels of the channels against the voices (1). Ambience sits just
/// under the music; at 0.5 it drowned the music and voices out.
pub const MUSIC_VOLUME: f32 = 0.25;
pub const AMBIENCE_VOLUME: f32 = 0.2;

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
                    log::warn!("music {}: {e}", path.display());
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
/// with their weights and volumes, each with when it plays.
struct Bank {
    beds: Vec<(Sound, f32, When)>,
    spots: Vec<(Sound, u32, f32, When)>,
}

/// When a playback record plays, from its group: 1 by day, 2 at night, else
/// always.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Day,
    Night,
    Always,
}

impl When {
    fn of(group: u16) -> Self {
        match group {
            1 => When::Day,
            2 => When::Night,
            _ => When::Always,
        }
    }

    fn now(self, night: bool) -> bool {
        match self {
            When::Day => !night,
            When::Night => night,
            When::Always => true,
        }
    }
}

/// How loud the outdoor ambience is indoors (by eye), and how fast it eases
/// there and back (per second).
const INDOOR_AMBIENCE: f32 = 0.3;
const MUFFLE_RATE: f32 = 1.5;

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
        let when = When::of(play.group);
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
            out.beds.push((sound, volume, when));
        } else {
            out.spots.push((sound, play.frequency.max(1), volume, when));
        }
    }
    Ok(out)
}

/// Plays the ambience of the block the Hero is in.
#[derive(Default)]
pub struct Ambience {
    /// The bank playing, and whether its night sounds are.
    current: Option<(PathBuf, bool)>,
    /// A different bank wanted, and for how long.
    wanting: Option<(PathBuf, f32)>,
    /// The beds playing and their own volumes, and how loud the ambience is
    /// now (1 outdoors, easing to [`INDOOR_AMBIENCE`] indoors).
    beds: Vec<(SoundId, f32)>,
    muffle: f32,
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

    /// Advances `dt` seconds with `wanted` the bank to play (None: silence),
    /// its night or day sounds, quieter `indoors`.
    pub fn update(
        &mut self,
        dt: f32,
        wanted: Option<&Path>,
        night: bool,
        indoors: bool,
        audio: &Audio,
    ) {
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
                    log::warn!("ambience {}: {e}", p.display());
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
            self.switch(None, false, audio);
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
        // Indoors the outdoor ambience eases down, and back outside.
        let target = if indoors { INDOOR_AMBIENCE } else { 1.0 };
        let step = MUFFLE_RATE * dt;
        let muffle = self.muffle + (target - self.muffle).clamp(-step, step);
        if muffle != self.muffle {
            self.muffle = muffle;
            for &(id, volume) in &self.beds {
                audio.set_sound_volume(id, volume * muffle);
            }
        }
        let current_path = self.current.as_ref().map(|(p, _)| p.as_path());
        if current_path == Some(path) && self.current.as_ref().is_some_and(|c| c.1 != night) {
            // Day turned to night or back: the other beds.
            self.switch(Some((path, &bank)), night, audio);
        } else if current_path != Some(path) {
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
                self.switch(Some((path, &bank)), night, audio);
            }
        } else {
            self.wanting = None;
        }
        // Now and then a spot sound of the time of day, picked by weight,
        // somewhere to the side; none indoors.
        self.next_spot -= dt;
        if self.next_spot <= 0.0 {
            self.next_spot = 1.5 + 4.5 * self.random();
            let spots: Vec<_> = bank.spots.iter().filter(|s| s.3.now(night)).collect();
            let total: u32 = spots.iter().map(|s| s.1).sum();
            if total > 0 && !indoors {
                let mut pick = (self.random() * total as f32) as u32;
                let chosen = spots.iter().find(|s| {
                    if pick < s.1 {
                        true
                    } else {
                        pick -= s.1;
                        false
                    }
                });
                if let Some((sound, _, volume, _)) = chosen {
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

    /// Stops the beds playing and starts `to`'s for the time of day.
    fn switch(&mut self, to: Option<(&Path, &Bank)>, night: bool, audio: &Audio) {
        for (id, _) in self.beds.drain(..) {
            audio.fade_out(id, BED_FADE);
        }
        self.current = to.map(|(p, _)| (p.to_owned(), night));
        if let Some((p, bank)) = to {
            log::info!(
                "ambience: {} by {} ({} beds, {} spot sounds)",
                p.file_stem().and_then(|s| s.to_str()).unwrap_or("?"),
                if night { "night" } else { "day" },
                bank.beds.iter().filter(|b| b.2.now(night)).count(),
                bank.spots.iter().filter(|s| s.3.now(night)).count()
            );
        }
        if let Some((_, bank)) = to {
            for (bed, volume, _) in bank.beds.iter().filter(|b| b.2.now(night)) {
                let settings = Settings {
                    volume: *volume * self.muffle,
                    looping: true,
                    fade_in: BED_FADE,
                    ..Settings::default()
                };
                if let Some(id) = audio.play(bed.clone(), Channel::Ambience, settings) {
                    self.beds.push((id, *volume));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_sounds_by_day_and_night_sounds_at_night() {
        assert!(When::of(1).now(false) && !When::of(1).now(true));
        assert!(When::of(2).now(true) && !When::of(2).now(false));
        assert!(When::of(0).now(true) && When::of(0).now(false));
    }
}
