//! Mixing decoded sounds for an output device.
//!
//! The mixer knows nothing of devices: the app asks it to [`Mixer::fill`] each
//! output buffer (from its audio callback) and starts and stops sounds. Sounds
//! play on channels; a channel such as [`Channel::Voice`] plays one sound at a
//! time, so a new line of dialogue cuts off the last.
//!
//! No platform dependency, and no allocation while mixing.

use std::sync::Arc;

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

/// What a sound is for; each channel plays one sound at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Spoken lines.
    Voice,
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
    volume: f32,
}

/// Mixes playing sounds to an output rate.
pub struct Mixer {
    output_rate: u32,
    playing: Vec<Playing>,
    next: u64,
    /// Overall volume, 0..=1.
    pub volume: f32,
}

impl Mixer {
    pub fn new(output_rate: u32) -> Self {
        Self {
            output_rate: output_rate.max(1),
            playing: Vec::with_capacity(8),
            next: 0,
            volume: 1.0,
        }
    }

    /// Starts `sound` on `channel`, stopping what that channel was playing.
    pub fn play(&mut self, sound: Sound, channel: Channel) -> SoundId {
        self.stop_channel(channel);
        self.next += 1;
        let id = SoundId(self.next);
        self.playing.push(Playing {
            id,
            channel,
            sound,
            position: 0.0,
            volume: 1.0,
        });
        id
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

    /// Mixes the next `out.len() / channels` frames into `out` (interleaved,
    /// -1..=1), replacing what it held. Mono sounds play on every output
    /// channel; sounds resample linearly to the output rate. Finished sounds
    /// are dropped.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        let channels = channels.max(1);
        let frames = out.len() / channels;
        for p in &mut self.playing {
            let src_channels = usize::from(p.sound.channels);
            let total = p.sound.frames();
            let step = f64::from(p.sound.sample_rate) / f64::from(self.output_rate);
            let gain = p.volume * self.volume / 32768.0;
            for frame in 0..frames {
                let at = p.position as usize;
                if at >= total {
                    break;
                }
                let t = (p.position - at as f64) as f32;
                let next = (at + 1).min(total - 1);
                for c in 0..channels {
                    let sc = c.min(src_channels - 1);
                    let a = f32::from(p.sound.samples[at * src_channels + sc]);
                    let b = f32::from(p.sound.samples[next * src_channels + sc]);
                    out[frame * channels + c] += (a + (b - a) * t) * gain;
                }
                p.position += step;
            }
        }
        self.playing
            .retain(|p| (p.position as usize) < p.sound.frames());
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
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
}
