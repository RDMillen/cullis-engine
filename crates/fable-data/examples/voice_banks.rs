//! Counts the text bank's spoken lines per voice bank, in index order, and
//! prints the first few of each: `cargo run --example voice_banks -- <FABLE_DIR>/data`.

use std::collections::BTreeMap;

use fable_data::strings::TextBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: voice_banks <data dir>")?;
    let text = TextBank::open(std::path::Path::new(&dir), "English")?;
    let mut banks: BTreeMap<&str, Vec<(u32, &str)>> = BTreeMap::new();
    for name in text.names() {
        if let Some(id) = text.id(name)
            && let Some(e) = text.strings.get(&id)
            && !e.sound_bank.is_empty()
        {
            banks.entry(&e.sound_bank).or_default().push((id, name));
        }
    }
    for (bank, lines) in &banks {
        println!("{bank}: {} lines", lines.len());
        for (id, name) in lines.iter().take(3) {
            println!("  {id} {name}");
        }
    }
    Ok(())
}
