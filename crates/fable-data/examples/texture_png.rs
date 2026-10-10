//! Exports a texture from a `.big` archive to PNG.
//!
//! `cargo run --release -p fable-data --example texture_png -- ARCHIVE NAME_PART OUT.png [LEVEL] [IMAGE]`
//!
//! `NAME_PART` matches the first entry whose name contains it (case-insensitive).
//! Writes PNGs with the small encoder below, so it needs no extra dependencies.

use std::fs::File;
use std::io::{BufReader, Write};

use fable_data::big::BigFile;
use fable_data::texture::{Texture, TextureInfo};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [archive, needle, out, rest @ ..] = args.as_slice() else {
        return Err("usage: texture_png ARCHIVE NAME_PART OUT.png [LEVEL] [IMAGE]".into());
    };
    let level: u32 = rest.first().map_or(Ok(0), |s| s.parse())?;
    let frame: usize = rest.get(1).map_or(Ok(0), |s| s.parse())?;
    let needle = needle.to_ascii_uppercase();
    let mut file = BufReader::new(File::open(archive)?);
    let big = BigFile::read(&mut file)?;
    for bank in &big.banks {
        let index = bank.read_index(&mut file)?;
        let Some(entry) = index
            .entries
            .iter()
            .find(|e| e.name.to_ascii_uppercase().contains(&needle))
        else {
            continue;
        };
        let info = TextureInfo::parse(&entry.info)?;
        let texture = Texture::unpack(&info, entry.entry_type, &entry.read_data(&mut file)?)?;
        let rgba = texture.to_rgba(frame, level)?;
        let [w, h] = texture.level_size(level);
        File::create(out)?.write_all(&png(w, h, &rgba))?;
        eprintln!(
            "{} ({}, type {}, {:?}, {}x{}, {} levels, {} images) -> {out}",
            entry.name,
            bank.name,
            entry.entry_type,
            info.format,
            w,
            h,
            info.mip_count,
            texture.images.len()
        );
        return Ok(());
    }
    Err(format!("no entry matching {needle}").into())
}

/// Minimal PNG writer: RGBA8, stored (uncompressed) deflate blocks.
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&out[start..]).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &x in data {
        c ^= u32::from(x);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}
