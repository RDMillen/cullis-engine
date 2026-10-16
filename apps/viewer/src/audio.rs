//! Sound: the engine's mixer played through the default output device, and
//! spoken lines read from the voice banks on demand.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use engine_audio::{Channel, Mixer, Settings, Sound, SoundId};
use fable_data::lut::SoundBank;
use fable_data::strings::TextBank;
use fable_data::voices::VoiceIndex;

/// The output stream, mixing what plays.
pub struct Audio {
    mixer: Arc<Mutex<Mixer>>,
    _stream: cpal::Stream,
}

impl Audio {
    /// Opens the default output device: through PulseAudio where it runs
    /// (ALSA's own PulseAudio plugin underruns), else the platform's default.
    pub fn start() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        let host =
            cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|_| cpal::default_host());
        #[cfg(not(target_os = "linux"))]
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no audio output device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let channels = usize::from(config.channels());
        let mut m = Mixer::new(config.sample_rate());
        m.set_channel_volume(Channel::Music, crate::soundscape::MUSIC_VOLUME);
        m.set_channel_volume(Channel::Ambience, crate::soundscape::AMBIENCE_VOLUME);
        let mixer = Arc::new(Mutex::new(m));
        let shared = mixer.clone();
        let error = |e: cpal::Error| eprintln!("audio: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| match shared.lock() {
                    Ok(mut m) => m.fill(data, channels),
                    Err(_) => data.fill(0.0),
                },
                error,
                None,
            ),
            cpal::SampleFormat::I16 => {
                // Mixed as floats, kept between calls so mixing does not allocate.
                let mut mixed: Vec<f32> = Vec::new();
                device.build_output_stream(
                    config.into(),
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        mixed.resize(data.len(), 0.0);
                        if let Ok(mut m) = shared.lock() {
                            m.fill(&mut mixed, channels);
                        } else {
                            mixed.fill(0.0);
                        }
                        for (out, s) in data.iter_mut().zip(&mixed) {
                            *out = (s * 32767.0) as i16;
                        }
                    },
                    error,
                    None,
                )
            }
            f => return Err(format!("unsupported output format {f}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            mixer,
            _stream: stream,
        })
    }

    /// Starts `sound` on `channel`.
    pub fn play(&self, sound: Sound, channel: Channel, settings: Settings) -> Option<SoundId> {
        Some(self.mixer.lock().ok()?.play_with(sound, channel, settings))
    }

    /// Sets the overall volume (0..=1).
    pub fn set_volume(&self, volume: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.volume = volume;
        }
    }

    /// Sets a channel's volume (0..=1).
    pub fn set_channel_volume(&self, channel: Channel, volume: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.set_channel_volume(channel, volume);
        }
    }

    /// Fades a sound out over `seconds`, then stops it.
    pub fn fade_out(&self, id: SoundId, seconds: f32) {
        if let Ok(mut m) = self.mixer.lock() {
            m.fade_out(id, seconds);
        }
    }

    /// Speaks `sound`, cutting off any line still being said.
    pub fn say(&self, sound: Sound) {
        if let Ok(mut m) = self.mixer.lock() {
            m.play(sound, Channel::Voice);
        }
    }

    /// Stops whatever is being said.
    pub fn hush(&self) {
        if let Ok(mut m) = self.mixer.lock() {
            m.stop_channel(Channel::Voice);
        }
    }
}

/// The voices of lines of text, read from the banks when first wanted.
pub struct Voices {
    index: VoiceIndex,
    dir: PathBuf,
    banks: HashMap<String, Option<(BufReader<File>, SoundBank)>>,
}

impl Voices {
    pub fn open(data_dir: &Path, text: &TextBank) -> Result<Self, fable_data::Error> {
        Ok(Self {
            index: VoiceIndex::build(data_dir, "English", text)?,
            dir: data_dir.join("lang/English"),
            banks: HashMap::new(),
        })
    }

    /// Lines with a voice.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// The decoded voice of text entry `id`, if it has one.
    pub fn sound(&mut self, id: u32) -> Option<Sound> {
        let voice = self.index.get(id)?.clone();
        let dir = &self.dir;
        let bank = self
            .banks
            .entry(voice.bank.clone())
            .or_insert_with(|| {
                let path = dir.join(format!("{}.lut", voice.bank));
                let mut file = BufReader::new(File::open(&path).ok()?);
                let bank = SoundBank::read(&mut file)
                    .map_err(|e| eprintln!("{}: {e}", path.display()))
                    .ok()?;
                Some((file, bank))
            })
            .as_mut()?;
        let entry = bank.1.entry(voice.sound)?;
        let sound = bank
            .1
            .read_sound(&mut bank.0, entry)
            .map_err(|e| eprintln!("voice {} {}: {e}", voice.bank, voice.sound))
            .ok()?;
        let samples = sound.wave.decode().ok()?;
        Some(Sound::new(
            samples,
            sound.wave.sample_rate,
            sound.wave.channels,
        ))
    }
}
