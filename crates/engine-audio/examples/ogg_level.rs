//! Prints an Ogg Vorbis file's length, average level and peak:
//! `cargo run --example ogg_level -- <file.ogg>`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: ogg_level <file.ogg>")?;
    let sound = engine_audio::ogg::decode(&std::fs::read(&path)?)?;
    let n = sound.samples.len().max(1) as f64;
    let rms = (sound
        .samples
        .iter()
        .map(|&s| f64::from(s).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    let peak = sound
        .samples
        .iter()
        .map(|&s| i32::from(s).abs())
        .max()
        .unwrap_or(0);
    println!(
        "{path}: {:.1} s, {} Hz, {} channel(s), rms {rms:.0}, peak {peak}",
        sound.seconds(),
        sound.sample_rate,
        sound.channels
    );
    Ok(())
}
