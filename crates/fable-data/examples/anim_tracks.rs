//! Lists an animation's bone tracks: `cargo run --example anim_tracks -- <FABLE_DIR>/data ANIM_VILLAGER_MALE_PHONEME_AH`.

use std::fs::File;
use std::io::BufReader;

use fable_data::anim::{ANIMATION_TYPES, Animation};
use fable_data::big::BigFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .ok_or("usage: anim_tracks <data dir> <animation>")?,
    );
    let name = args
        .next()
        .ok_or("usage: anim_tracks <data dir> <animation>")?;
    let mut file = BufReader::new(File::open(dir.join("graphics/graphics.big"))?);
    let big = BigFile::read(&mut file)?;
    let index = big
        .bank("MBANK_ALLMESHES")
        .ok_or("no bank")?
        .read_index(&mut file)?;
    let entry = index
        .entries
        .iter()
        .find(|e| e.name == name && ANIMATION_TYPES.contains(&e.entry_type))
        .ok_or("no such animation")?;
    let anim = Animation::parse(&entry.read_data(&mut file)?)?;
    println!(
        "{name}: {} object(s), duration {:?}",
        anim.objects.len(),
        anim.unknown_root
    );
    for o in &anim.objects {
        for t in &o.tracks {
            println!(
                "  {:24} frames {:3} fps {:4.1} layout {:?}",
                t.name, t.frames, t.fps, t.layout
            );
        }
    }
    Ok(())
}
