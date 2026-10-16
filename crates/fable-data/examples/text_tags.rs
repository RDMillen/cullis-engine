//! Counts the tags on text strings: `cargo run --example text_tags -- <FABLE_DIR>/data`.

use std::collections::BTreeMap;

use fable_data::strings::TextBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: text_tags <data dir>")?;
    let text = TextBank::open(std::path::Path::new(&dir), "English")?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut untagged_spoken = 0;
    for e in text.strings.values() {
        for t in &e.tags {
            *counts.entry(t).or_default() += 1;
        }
        if e.tags.is_empty() && !e.sound_bank.is_empty() {
            untagged_spoken += 1;
        }
    }
    for (t, n) in counts {
        println!("{n:6} {t}");
    }
    println!("{untagged_spoken} spoken strings without tags");
    Ok(())
}
