//! Aligns each voice bank's spoken lines (in name order) with its sounds (in id
//! order) by how well line length predicts sound duration, allowing skipped
//! lines and sounds, and reports how well each bank aligns.
//!
//! Usage: `cargo run --example voice_align -- <FABLE_DIR>/data [line name]`

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use fable_data::big::BigFile;
use fable_data::strings::TextBank;

/// Expected duration (seconds) of a line of `chars` characters.
fn expected(chars: usize) -> f64 {
    0.4 + chars as f64 / 14.0
}

fn cost(chars: usize, duration: f64) -> f64 {
    ((duration + 0.5) / (expected(chars) + 0.5)).ln().abs()
}

/// Global alignment: returns, per line, the index of its sound (or None).
fn align(lines: &[usize], sounds: &[Option<f64>], skip: f64) -> Vec<Option<usize>> {
    let (n, m) = (lines.len(), sounds.len());
    // dp[i][j]: best cost aligning the first i lines with the first j sounds.
    let mut dp = vec![f64::INFINITY; (n + 1) * (m + 1)];
    let mut from = vec![0u8; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    dp[0] = 0.0;
    for i in 0..=n {
        for j in 0..=m {
            let here = dp[at(i, j)];
            if !here.is_finite() {
                continue;
            }
            // An empty sound slot ([Empty]) costs nothing to skip.
            let sound_skip = if j < m && sounds[j].is_none() {
                0.0
            } else {
                skip
            };
            let mut relax = |i2: usize, j2: usize, c: f64, how: u8| {
                if here + c < dp[at(i2, j2)] {
                    dp[at(i2, j2)] = here + c;
                    from[at(i2, j2)] = how;
                }
            };
            if i < n
                && j < m
                && let Some(d) = sounds[j]
            {
                relax(i + 1, j + 1, cost(lines[i], d), 1);
            }
            if i < n {
                relax(i + 1, j, skip, 2);
            }
            if j < m {
                relax(i, j + 1, sound_skip, 3);
            }
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        match from[at(i, j)] {
            1 => {
                out[i - 1] = Some(j - 1);
                i -= 1;
                j -= 1;
            }
            2 => i -= 1,
            _ => j -= 1,
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage: voice_align <data dir> [line]")?);
    let show = args.next();
    let lang = dir.join("lang/English");
    // Sound slots per bank in id order: Some(duration), or None for [Empty].
    let mut slots: HashMap<String, Vec<(u32, Option<f64>)>> = HashMap::new();
    let mut f = BufReader::new(File::open(lang.join("dialogue.big"))?);
    let big = BigFile::read(&mut f)?;
    for bank in &big.banks {
        for e in bank.read_index(&mut f)?.entries {
            let name = e.name.trim_end_matches("[Empty]");
            let Some((b, n)) = name.rsplit_once('_') else {
                continue;
            };
            let Ok(n) = n.parse::<u32>() else { continue };
            let d = <[u8; 4]>::try_from(e.info.as_slice())
                .ok()
                .map(|x| f64::from(f32::from_le_bytes(x)));
            slots.entry(b.to_owned()).or_default().push((n, d));
        }
    }
    let text = TextBank::open(&dir, "English")?;
    let mut lines: HashMap<String, Vec<(&str, usize)>> = HashMap::new();
    for name in text.names() {
        let Some(e) = text.id(name).and_then(|i| text.strings.get(&i)) else {
            continue;
        };
        let spoken = !e.speaker.is_empty()
            && !["NONE", "UNSPOKEN"].contains(&e.speaker.to_ascii_uppercase().as_str());
        if let (true, Some(bank)) = (spoken, e.sound_bank.strip_suffix(".lug")) {
            lines
                .entry(bank.to_owned())
                .or_default()
                .push((name, e.text.chars().count()));
        }
    }
    for (bank, ls) in &mut lines {
        let Some(ss) = slots.get_mut(bank) else {
            continue;
        };
        ss.sort_by_key(|s| s.0);
        match std::env::var("ORDER").as_deref() {
            Ok("id") => ls.sort_by_key(|l| text.id(l.0)),
            _ => ls.sort_by_key(|l| l.0.to_ascii_lowercase()),
        }
        let lens: Vec<usize> = ls.iter().map(|l| l.1).collect();
        let durs: Vec<Option<f64>> = ss.iter().map(|s| s.1).collect();
        let matched = align(&lens, &durs, 1.2);
        let pairs: Vec<f64> = matched
            .iter()
            .zip(&lens)
            .filter_map(|(m, &len)| Some(cost(len, durs[(*m)?]?)))
            .collect();
        let good = pairs.iter().filter(|&&c| c < 0.4).count();
        // Baseline: the same lines paired with the sound five places on.
        let shifted = matched
            .iter()
            .zip(&lens)
            .filter_map(|(m, &len)| Some(cost(len, (*durs.get((*m)? + 5)?)?)))
            .filter(|&c| c < 0.4)
            .count();
        println!("{bank}: shifted by five, {shifted} within 40%");
        println!(
            "{bank}: {} lines, {} sound slots ({} empty), {} matched, {} within 40% of the expected duration",
            ls.len(),
            ss.len(),
            ss.iter().filter(|s| s.1.is_none()).count(),
            pairs.len(),
            good
        );
        if std::env::var_os("SIDE").is_some_and(|v| v == bank.as_str()) {
            for k in 0..60 {
                let l = ls
                    .get(k)
                    .map(|l| format!("{} ({})", l.0, l.1))
                    .unwrap_or_default();
                let s = ss
                    .get(k)
                    .map(|s| format!("{} {:?}", s.0, s.1.map(|d| (d * 10.0).round() / 10.0)))
                    .unwrap_or_default();
                println!("    {l:60} | {s}");
            }
        }
        if let Some(show) = &show {
            for (k, (name, len)) in ls.iter().enumerate() {
                if name.contains(show.as_str()) {
                    let s = matched[k].map(|j| ss[j]);
                    println!("  {name} ({len} chars) -> {s:?}");
                }
            }
        }
    }
    Ok(())
}
