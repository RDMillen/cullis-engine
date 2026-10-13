//! Tests whether a text entry's index info (a u32) is its sound's number in its
//! voice bank: per bank, whether the numbers are unique and in range, and how
//! well line length follows the sound's duration (from the lipsync bank).
//!
//! Usage: `cargo run --example voice_map -- <FABLE_DIR>/data`

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;
use fable_data::strings::TextEntry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: voice_map <data dir>")?,
    );
    let lang = dir.join("lang/English");
    // Durations per lipsync entry name (`ScriptDialogue_112`).
    let mut durations: HashMap<String, f32> = HashMap::new();
    let mut f = BufReader::new(File::open(lang.join("dialogue.big"))?);
    let big = BigFile::read(&mut f)?;
    for bank in &big.banks {
        for e in bank.read_index(&mut f)?.entries {
            if let Ok(b) = <[u8; 4]>::try_from(e.info.as_slice()) {
                durations.insert(e.name.clone(), f32::from_le_bytes(b));
            }
        }
    }
    let mut f = BufReader::new(File::open(lang.join("text.big"))?);
    let big = BigFile::read(&mut f)?;
    let bank = &big.banks[0];
    let index = bank.read_index(&mut f)?;
    // Per voice bank: (info number, text length, duration of that sound).
    let mut per: HashMap<String, Vec<(u32, usize, Option<f32>)>> = HashMap::new();
    for e in index.entries.iter().filter(|e| e.entry_type == 0) {
        let Ok(info) = <[u8; 4]>::try_from(e.info.as_slice()) else {
            continue;
        };
        let n = u32::from_le_bytes(info);
        let entry = TextEntry::parse(&e.read_data(&mut f)?)?;
        let Some(bank) = entry.sound_bank.strip_suffix(".lug") else {
            continue;
        };
        let d = durations.get(&format!("{bank}_{n}")).copied();
        per.entry(bank.to_owned())
            .or_default()
            .push((n, entry.text.chars().count(), d));
    }
    // Alternative: the k-th line of a bank (in some order) is that bank's
    // k-th sound (in id order, as the lookup table lists them).
    let mut lines: HashMap<String, Vec<(u32, String, usize, usize)>> = HashMap::new();
    for (pos, e) in index
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.entry_type == 0)
    {
        let entry = TextEntry::parse(&e.read_data(&mut f)?)?;
        let spoken = !entry.speaker.is_empty()
            && !["NONE", "UNSPOKEN"].contains(&entry.speaker.to_ascii_uppercase().as_str());
        if !spoken {
            continue;
        }
        if let Some(bank) = entry.sound_bank.strip_suffix(".lug") {
            lines.entry(bank.to_owned()).or_default().push((
                e.id,
                e.name.clone(),
                pos,
                entry.text.chars().count(),
            ));
        }
    }
    for (bank, rows) in &mut lines {
        let mut sounds: Vec<(u32, f32)> = durations
            .iter()
            .filter_map(|(name, &d)| {
                let n = name
                    .strip_prefix(bank.as_str())?
                    .strip_prefix('_')?
                    .parse()
                    .ok()?;
                Some((n, d))
            })
            .collect();
        sounds.sort_by_key(|s| s.0);
        for order in ["name", "lowercase name"] {
            match order {
                "name" => rows.sort_by(|a, b| a.1.cmp(&b.1)),
                _ => rows.sort_by_key(|r| r.1.to_ascii_lowercase()),
            }
            // Correlation in windows of 500, to see where an alignment drifts.
            let windows: Vec<String> = rows
                .iter()
                .zip(&sounds)
                .map(|(r, s)| (r.3 as f64, s.1 as f64))
                .collect::<Vec<_>>()
                .chunks(500)
                .map(|w| format!("{:.2}", correlation(w)))
                .collect();
            println!("{bank} by {order}: {}", windows.join(" "));
        }
        // The best shift (sound = line + shift) per window of 250 lines.
        rows.sort_by(|a, b| a.1.cmp(&b.1));
        let mut shifts = Vec::new();
        for start in (0..rows.len()).step_by(250) {
            let best = (-60i64..=60)
                .map(|shift| {
                    let pairs: Vec<(f64, f64)> = (start..(start + 250).min(rows.len()))
                        .filter_map(|i| {
                            let s = sounds.get(usize::try_from(i as i64 + shift).ok()?)?;
                            Some((rows[i].3 as f64, s.1 as f64))
                        })
                        .collect();
                    (shift, correlation(&pairs))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            shifts.push(format!("{}:{:+}({:.2})", start, best.0, best.1));
        }
        println!("{bank} shifts: {}", shifts.join(" "));
    }
    for (bank, rows) in &per {
        let unique: HashSet<u32> = rows.iter().map(|r| r.0).collect();
        let (lo, hi) = (
            rows.iter().map(|r| r.0).min().unwrap_or(0),
            rows.iter().map(|r| r.0).max().unwrap_or(0),
        );
        let pairs: Vec<(f64, f64)> = rows
            .iter()
            .filter_map(|&(_, len, d)| Some((len as f64, d? as f64)))
            .collect();
        println!(
            "{bank}: {} lines, {} distinct numbers, {lo}..{hi}, {} with a sound, length/duration correlation {:.2}",
            rows.len(),
            unique.len(),
            pairs.len(),
            correlation(&pairs)
        );
    }
    Ok(())
}

fn correlation(pairs: &[(f64, f64)]) -> f64 {
    let n = pairs.len() as f64;
    let (mx, my) = (
        pairs.iter().map(|p| p.0).sum::<f64>() / n,
        pairs.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let cov: f64 = pairs.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let vx: f64 = pairs.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let vy: f64 = pairs.iter().map(|p| (p.1 - my).powi(2)).sum();
    cov / (vx * vy).sqrt()
}
