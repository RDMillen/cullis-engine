//! Writes one sound from a voice bank as a 16-bit PCM WAVE, and its original
//! WAVE file as stored: `cargo run --example voice_wav -- <bank.lut> <id> <out prefix>`.

use std::fs::File;
use std::io::{BufReader, Write};

use fable_data::lut::SoundBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: voice_wav <bank.lut> <id> <out prefix>";
    let (path, id, out) = (
        args.next().ok_or(usage)?,
        args.next().ok_or(usage)?,
        args.next().ok_or(usage)?,
    );
    let mut file = BufReader::new(File::open(&path)?);
    let bank = SoundBank::read(&mut file)?;
    let entry = bank.entry(id.parse()?).ok_or("no such sound")?;
    let sound = bank.read_sound(&mut file, entry)?;
    let w = &sound.wave;
    println!(
        "sound {}: format {:#x}, {} channel(s), {} Hz, block {}, {} bits, extra {:02x?}, {} data bytes",
        sound.id,
        w.format,
        w.channels,
        w.sample_rate,
        w.block_align,
        w.bits_per_sample,
        w.extra,
        w.data.len()
    );
    // The stored file, for comparing with other decoders.
    let mut raw = vec![0u8; entry.length as usize - 36];
    {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = File::open(&path)?;
        f.seek(SeekFrom::Start(44 + u64::from(entry.offset) + 36))?;
        f.read_exact(&mut raw)?;
    }
    File::create(format!("{out}.orig.wav"))?.write_all(&raw)?;
    let pcm = w.decode()?;
    let data: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&w.channels.to_le_bytes());
    wav.extend_from_slice(&w.sample_rate.to_le_bytes());
    wav.extend_from_slice(&(w.sample_rate * 2 * u32::from(w.channels)).to_le_bytes());
    wav.extend_from_slice(&(2 * w.channels).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    File::create(format!("{out}.pcm.wav"))?.write_all(&wav)?;
    println!(
        "{} samples ({:.2} s)",
        pcm.len(),
        pcm.len() as f32 / w.sample_rate as f32
    );
    Ok(())
}
