//! Prints each sound of an ambient bank with how it plays and its level:
//! `cargo run --example lug_levels -- <FABLE_DIR>/data/Sound/country.met`.

use std::fs::File;
use std::io::BufReader;

use fable_data::lug::AmbientBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let met = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: lug_levels <bank.met>")?,
    );
    let bank = AmbientBank::parse_met(&std::fs::read(&met)?)?;
    let mut lug = BufReader::new(File::open(met.with_extension("lug"))?);
    for s in &bank.sounds {
        let samples = bank.read_samples(&mut lug, s)?;
        let n = samples.len().max(1) as f64;
        let rms = (samples.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / n).sqrt();
        println!(
            "{:3} {:24} {:5.1} s rms {rms:5.0}",
            s.index,
            s.file_name(),
            samples.len() as f32 / f32::from(s.channels.max(1)) / s.sample_rate as f32
        );
    }
    for p in &bank.plays {
        println!(
            "play sound {:3} group {} repeats {:5} volume {:3} frequency {:5} {:?}",
            p.sound, p.group, p.repeats, p.volume, p.frequency, p.name
        );
    }
    Ok(())
}
