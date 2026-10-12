//! Lists text entries whose name contains every given word (case-insensitive),
//! with speaker and text.
//!
//! Usage: `cargo run --example text_find -- <FABLE_DIR>/data NOVI THERESA`

use fable_data::strings::TextBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .ok_or("usage: text_find <data dir> <words>...")?;
    let words: Vec<String> = args.map(|w| w.to_ascii_uppercase()).collect();
    let text = TextBank::open(std::path::Path::new(&dir), "English")?;
    for name in text.names() {
        if !words.iter().all(|w| name.contains(w.as_str())) {
            continue;
        }
        match text.id(name).and_then(|i| text.strings.get(&i)) {
            Some(e) => println!("{name} [{}] {}", e.speaker, e.text),
            None => {
                let n = text
                    .id(name)
                    .and_then(|i| text.groups.get(&i))
                    .map_or(0, Vec::len);
                println!(
                    "{name} (group of {n}) e.g. {}",
                    text.get(name).unwrap_or("")
                );
            }
        }
    }
    Ok(())
}
