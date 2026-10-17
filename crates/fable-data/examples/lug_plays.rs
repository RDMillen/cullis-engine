//! Prints an ambient bank's playback records by group (1 day, 2 night):
//! `cargo run --example lug_plays -- <FABLE_DIR>/data/Sound/country.met`.

use fable_data::lug::AmbientBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: lug_plays <file.met>")?;
    let bank = AmbientBank::parse_met(&std::fs::read(path)?)?;
    for p in &bank.plays {
        let kind = if p.repeats >= 9999 { "bed" } else { "spot" };
        println!("group {} {kind:4} sound {:3} {}", p.group, p.sound, p.name);
    }
    Ok(())
}
