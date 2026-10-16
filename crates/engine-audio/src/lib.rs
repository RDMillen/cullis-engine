//! Mixing decoded sounds for an output device.
//!
//! The mixer knows nothing of devices: the app asks it to [`Mixer::fill`] each
//! output buffer (from its audio callback) and starts and stops sounds. Sounds
//! play on channels: [`Channel::Voice`] plays one sound at a time (a new line
//! of dialogue cuts off the last), [`Channel::Music`] and
//! [`Channel::Ambience`] any number, so pieces can crossfade. Each sound has a
//! volume, a pan, may loop and may fade in or out; each channel has a volume
//! of its own.
//!
//! No platform dependency, and no allocation while mixing.

use std::sync::Arc;

pub mod ogg;

/// Decoded 16-bit samples, shared so a sound can play while still cached.
#[derive(Debug, Clone)]
pub struct Sound {
    /// Interleaved samples.
    pub samples: Arc<[i16]>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Sound {
    pub fn new(samples: Vec<i16>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples: samples.into(),
            sample_rate,
            channels: channels.max(1),
        }
    }

    /// Frames (samples per channel).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Length in seconds.
    pub fn seconds(&self) -> f32 {
        self.frames() as f32 / self.sample_rate as f32
    }
}

/// What a sound is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Spoken lines, one at a time.
    Voice,
    /// Background music (crossfaded by whoever plays it).
    Music,
    /// Ambient beds and spot sounds, any number at once.
    Ambience,
}

impl Channel {
    const ALL: [Channel; 3] = [Channel::Voice, Channel::Music, Channel::Ambience];

    fn index(self) -> usize {
        self as usize
    }

    /// Whether starting a sound stops the one playing.
    fn exclusive(self) -> bool {
        matches!(self, Channel::Voice)
    }
}

/// How a sound plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// 0..=1.
    pub volume: f32,
    /// -1 (left) ..= 1 (right); for mono sounds.
    pub pan: f32,
    /// Starts again from the beginning at its end, until stopped.
    pub looping: bool,
    /// Seconds to fade in from silence (0: starts at full volume).
    pub fade_in: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            looping: false,
            fade_in: 0.0,
        }
    }
}

/// A sound started by [`Mixer::play`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundId(u64);

struct Playing {
    id: SoundId,
    channel: Channel,
    sound: Sound,
    /// Position in source frames.
    position: f64,
    settings: Settings,
    /// The fade's gain (0..=1) and how fast it moves, per second: up while
    /// fading in, down (then the sound stops) while fading out.
    fade: f32,
    fade_rate: f32,
}

/// Level below which the output passes unchanged; above it, peaks are rounded
/// off towards full scale instead of clipped.
const KNEE: f32 = 0.75;

/// A soft limiter: unchanged up to [`KNEE`], then a smooth curve (a scaled
/// `tanh`, with the same slope at the knee) that never quite reaches 1.
fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        return x;
    }
    let room = 1.0 - KNEE;
    (KNEE + room * ((a - KNEE) / room).tanh()).copysign(x)
}

/// Mixes playing sounds to an output rate.
pub struct Mixer {
    output_rate: u32,
    playing: Vec<Playing>,
    next: u64,
    channel_volume: [f32; Channel::ALL.len()],
    /// Overall volume, 0..=1.
    pub volume: f32,
}

impl Mixer {
    pub fn new(output_rate: u32) -> Self {
        Self {
            output_rate: output_rate.max(1),
            playing: Vec::with_capacity(32),
            next: 0,
            channel_volume: [1.0; Channel::ALL.len()],
            volume: 1.0,
        }
    }

    /// Starts `sound` on `channel` at full volume, once.
    pub fn play(&mut self, sound: Sound, channel: Channel) -> SoundId {
        self.play_with(sound, channel, Settings::default())
    }

    /// Starts `sound` on `channel`; on a one-at-a-time channel, stops what it
    /// was playing.
    pub fn play_with(&mut self, sound: Sound, channel: Channel, settings: Settings) -> SoundId {
        if channel.exclusive() {
            self.stop_channel(channel);
        }
        self.next += 1;
        let id = SoundId(self.next);
        let fading_in = settings.fade_in > 0.0;
        self.playing.push(Playing {
            id,
            channel,
            sound,
            position: 0.0,
            settings,
            fade: if fading_in { 0.0 } else { 1.0 },
            fade_rate: if fading_in {
                1.0 / settings.fade_in
            } else {
                0.0
            },
        });
        id
    }

    /// Fades a sound out over `seconds`, then stops it.
    pub fn fade_out(&mut self, id: SoundId, seconds: f32) {
        if seconds <= 0.0 {
            self.stop(id);
        } else if let Some(p) = self.playing.iter_mut().find(|p| p.id == id) {
            p.fade_rate = -1.0 / seconds;
        }
    }

    pub fn stop(&mut self, id: SoundId) {
        self.playing.retain(|p| p.id != id);
    }

    pub fn stop_channel(&mut self, channel: Channel) {
        self.playing.retain(|p| p.channel != channel);
    }

    pub fn is_playing(&self, id: SoundId) -> bool {
        self.playing.iter().any(|p| p.id == id)
    }

    /// Sets one sound's volume (0..=1), as it plays.
    pub fn set_volume(&mut self, id: SoundId, volume: f32) {
        if let Some(p) = self.playing.iter_mut().find(|p| p.id == id) {
            p.settings.volume = volume;
        }
    }

    /// Sets a channel's volume (0..=1).
    pub fn set_channel_volume(&mut self, channel: Channel, volume: f32) {
        self.channel_volume[channel.index()] = volume;
    }

    /// Mixes the next `out.len() / channels` frames into `out` (interleaved,
    /// -1..=1), replacing what it held. Mono sounds are panned across the first
    /// two output channels; sounds resample linearly to the output rate.
    /// Finished sounds are dropped.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        let channels = channels.max(1);
        let frames = out.len() / channels;
        for p in &mut self.playing {
            let src_channels = usize::from(p.sound.channels);
            let total = p.sound.frames();
            if total == 0 {
                continue;
            }
            let step = f64::from(p.sound.sample_rate) / f64::from(self.output_rate);
            let gain =
                p.settings.volume * self.channel_volume[p.channel.index()] * self.volume / 32768.0;
            let mut fade_step = p.fade_rate / self.output_rate as f32;
            // Constant-power pan for mono sounds into stereo.
            let angle = (p.settings.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            let (left, right) = if src_channels == 1 && channels >= 2 && p.settings.pan != 0.0 {
                (
                    angle.cos() * std::f32::consts::SQRT_2,
                    angle.sin() * std::f32::consts::SQRT_2,
                )
            } else {
                (1.0, 1.0)
            };
            for frame in 0..frames {
                let mut at = p.position as usize;
                if at >= total {
                    if !p.settings.looping {
                        break;
                    }
                    p.position -= total as f64;
                    at = p.position as usize;
                }
                let t = (p.position - at as f64) as f32;
                let next = if at + 1 < total {
                    at + 1
                } else if p.settings.looping {
                    0
                } else {
                    at
                };
                for c in 0..channels {
                    let sc = c.min(src_channels - 1);
                    let a = f32::from(p.sound.samples[at * src_channels + sc]);
                    let b = f32::from(p.sound.samples[next * src_channels + sc]);
                    let side = match c {
                        0 => left,
                        1 => right,
                        _ => 1.0,
                    };
                    out[frame * channels + c] += (a + (b - a) * t) * gain * side * p.fade;
                }
                p.position += step;
                if fade_step != 0.0 {
                    p.fade = (p.fade + fade_step).clamp(0.0, 1.0);
                    if p.fade >= 1.0 && fade_step > 0.0 {
                        // Faded in: stays at full volume.
                        p.fade_rate = 0.0;
                        fade_step = 0.0;
                    }
                }
            }
        }
        self.playing.retain(|p| {
            let faded_out = p.fade_rate < 0.0 && p.fade <= 0.0;
            !faded_out && (p.settings.looping || (p.position as usize) < p.sound.frames())
        });
        for s in out.iter_mut() {
            *s = limit(*s);
        }
    }

    /// A channel's volume.
    pub fn channel_volume(&self, channel: Channel) -> f32 {
        self.channel_volume[channel.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_a_sound_to_its_end_on_every_channel() {
        let mut mixer = Mixer::new(10);
        let id = mixer.play(Sound::new(vec![16384, -16384, 0], 10, 1), Channel::Voice);
        let mut out = [9.0f32; 8];
        mixer.fill(&mut out, 2);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5, 0.0, 0.0, 0.0, 0.0]);
        assert!(!mixer.is_playing(id), "finished");
    }

    #[test]
    fn resamples_to_the_output_rate() {
        // A 10 Hz ramp played at 20 Hz: every other output frame is between.
        let mut mixer = Mixer::new(20);
        mixer.play(Sound::new(vec![0, 16384, 32767], 10, 1), Channel::Voice);
        let mut out = [0.0f32; 4];
        mixer.fill(&mut out, 1);
        assert!((out[1] - 0.25).abs() < 1e-4, "{out:?}");
        assert!((out[2] - 0.5).abs() < 1e-4, "{out:?}");
    }

    #[test]
    fn a_new_line_cuts_off_the_last() {
        let mut mixer = Mixer::new(10);
        let first = mixer.play(Sound::new(vec![1; 100], 10, 1), Channel::Voice);
        let second = mixer.play(Sound::new(vec![2; 100], 10, 1), Channel::Voice);
        assert!(!mixer.is_playing(first));
        assert!(mixer.is_playing(second));
        mixer.stop(second);
        let mut out = [1.0f32; 4];
        mixer.fill(&mut out, 1);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn ambience_layers_loop_and_pan() {
        let mut mixer = Mixer::new(10);
        let looping = Settings {
            looping: true,
            ..Settings::default()
        };
        let bed = mixer.play_with(Sound::new(vec![8192, 0], 10, 1), Channel::Ambience, looping);
        let left = Settings {
            pan: -1.0,
            ..Settings::default()
        };
        let spot = mixer.play_with(Sound::new(vec![4096], 10, 1), Channel::Ambience, left);
        assert!(mixer.is_playing(bed) && mixer.is_playing(spot), "both play");
        let mut out = [0.0f32; 8];
        mixer.fill(&mut out, 2);
        // Frame 0: bed 0.25 on both sides (centred), spot 0.125 * sqrt(2) left
        // only (below the limiter's knee).
        assert!(
            (out[0] - (0.25 + 0.125 * std::f32::consts::SQRT_2)).abs() < 1e-4,
            "{out:?}"
        );
        assert!((out[1] - 0.25).abs() < 1e-4, "{out:?}");
        // The bed loops: frame 2 is its first sample again.
        assert!((out[4] - 0.25).abs() < 1e-4, "{out:?}");
        assert!(mixer.is_playing(bed) && !mixer.is_playing(spot));
        mixer.set_channel_volume(Channel::Ambience, 0.0);
        mixer.fill(&mut out, 2);
        assert_eq!(out, [0.0; 8]);
    }

    #[test]
    fn sounds_fade_in_and_out() {
        let mut mixer = Mixer::new(10);
        let fading = Settings {
            looping: true,
            fade_in: 0.4,
            ..Settings::default()
        };
        let id = mixer.play_with(Sound::new(vec![16384], 10, 1), Channel::Music, fading);
        let mut out = [0.0f32; 6];
        mixer.fill(&mut out, 1);
        // 0.4 s at 10 Hz: up by a quarter each frame from silence.
        let expect = [0.0, 0.125, 0.25, 0.375, 0.5, 0.5];
        for (o, e) in out.iter().zip(expect) {
            assert!((o - e).abs() < 1e-4, "{out:?}");
        }
        mixer.fade_out(id, 0.2);
        mixer.fill(&mut out, 1);
        assert!(
            (out[0] - 0.5).abs() < 1e-4 && (out[1] - 0.25).abs() < 1e-4,
            "{out:?}"
        );
        assert_eq!(out[2..], [0.0; 4]);
        assert!(!mixer.is_playing(id), "stopped once faded out");
    }

    #[test]
    fn music_pieces_overlap_for_a_crossfade() {
        let mut mixer = Mixer::new(10);
        let a = mixer.play(Sound::new(vec![1; 10], 10, 1), Channel::Music);
        let b = mixer.play(Sound::new(vec![1; 10], 10, 1), Channel::Music);
        assert!(mixer.is_playing(a) && mixer.is_playing(b));
    }

    #[test]
    fn loud_mixes_are_rounded_off_not_clipped() {
        assert_eq!(limit(0.5), 0.5, "quiet passes unchanged");
        assert_eq!(limit(-0.75), -0.75);
        let mut last = KNEE;
        for k in 1..=40 {
            let x = KNEE + k as f32 * 0.1;
            let y = limit(x);
            assert!(y >= last && y <= 1.0, "{x} -> {y}");
            assert_eq!(limit(-x), -y);
            last = y;
        }
        // Two full-scale sounds together stay below full scale.
        let mut mixer = Mixer::new(10);
        mixer.play(Sound::new(vec![32767], 10, 1), Channel::Music);
        mixer.play(Sound::new(vec![32767], 10, 1), Channel::Ambience);
        let mut out = [0.0f32; 1];
        mixer.fill(&mut out, 1);
        assert!(out[0] > 0.9 && out[0] <= 1.0, "{out:?}");
    }
}
