//! Prints every text string as `id<TAB>name<TAB>bank<TAB>text` (for scripts).
//! Usage: `cargo run --example text_dump -- <FABLE_DIR>/data`

use fable_data::strings::TextBank;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: text_dump <data dir>")?;
    let text = TextBank::open(std::path::Path::new(&dir), "English")?;
    for name in text.names() {
        if let Some(id) = text.id(name)
            && let Some(e) = text.strings.get(&id)
        {
            let clean: String = e
                .text
                .chars()
                .map(|c| if c == '\t' || c == '\n' { ' ' } else { c })
                .collect();
            println!("{id}\t{name}\t{}\t{clean}", e.sound_bank);
        }
    }
    Ok(())
}
