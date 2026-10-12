//! Prints the bank id, voice bank and text of entries whose name contains every
//! given word: `cargo run --example text_ids -- <FABLE_DIR>/data DAD_DONE`.

use fable_data::strings::TextBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().ok_or("usage: text_ids <data dir> <words>...")?;
    let words: Vec<String> = args.map(|w| w.to_ascii_uppercase()).collect();
    let text = TextBank::open(std::path::Path::new(&dir), "English")?;
    for name in text.names() {
        if words.iter().all(|w| name.contains(w.as_str()))
            && let Some(id) = text.id(name)
            && let Some(e) = text.strings.get(&id)
        {
            println!("{id} {name} [{}] {}", e.sound_bank, e.text);
        }
    }
    Ok(())
}
