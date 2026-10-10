//! Tests against a real install. Run with `FABLE_DIR=... cargo test -- --ignored`.
//! Assertions check internal consistency, not sizes or counts that may differ
//! between releases.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use fable_data::bank::EntryIndex;
use fable_data::bbbb::BbbbFile;
use fable_data::big::BigFile;
use fable_data::wld::Wld;

fn data_dir() -> PathBuf {
    let dir = std::env::var_os("FABLE_DIR").expect("set FABLE_DIR to the game install");
    PathBuf::from(dir).join("data")
}

fn check_bank_layout(index: &EntryIndex, data_start: u64, data_end: u64, block_size: u32) {
    let total: u32 = index.type_counts.iter().map(|t| t.count).sum();
    assert_eq!(index.entries.len(), total as usize);
    let mut spans: Vec<_> = index
        .entries
        .iter()
        .map(|e| (u64::from(e.offset), u64::from(e.size), &e.name))
        .collect();
    spans.sort();
    let mut previous_end = data_start;
    for (offset, size, name) in spans {
        assert_eq!(
            offset % u64::from(block_size),
            0,
            "{name} is not block-aligned"
        );
        assert!(offset >= previous_end, "{name} overlaps the previous entry");
        previous_end = offset + size;
    }
    assert!(previous_end <= data_end, "entry data runs past the index");
}

fn check_bbbb(name: &str) -> BbbbFile {
    let path = data_dir().join("Levels").join(name);
    let bank = BbbbFile::open(&path).unwrap();
    let h = &bank.header;
    assert_eq!(bank.index.entries.len(), h.entry_count_b as usize);
    assert_eq!(
        bank.index.to_bytes(),
        bank.index_bytes,
        "index does not round-trip"
    );
    check_bank_layout(
        &bank.index,
        u64::from(h.block_size),
        h.index_offset.into(),
        h.block_size,
    );
    bank
}

#[test]
#[ignore]
fn wad_index_round_trips_and_pairs_lev_with_tng() {
    let wad = check_bbbb("FinalAlbion.wad");
    let names: HashSet<String> = wad
        .index
        .entries
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .collect();
    for name in &names {
        if let Some(stem) = name.strip_suffix(".lev") {
            assert!(names.contains(&format!("{stem}.tng")), "{name} has no tng");
        }
    }
}

#[test]
#[ignore]
fn stb_index_round_trips() {
    check_bbbb("FinalAlbion_RT.stb");
}

#[test]
#[ignore]
fn every_big_bank_index_parses_and_round_trips() {
    let mut paths = Vec::new();
    let mut dirs = vec![data_dir()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("big"))
            {
                paths.push(path);
            }
        }
    }
    assert!(!paths.is_empty());
    for path in paths {
        let mut file = BufReader::new(File::open(&path).unwrap());
        let big = BigFile::read(&mut file).unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        assert_eq!(
            u64::from(big.bank_table_offset + big.bank_table_len),
            len,
            "{}",
            path.display()
        );
        for bank in &big.banks {
            let bytes = bank.read_index_bytes(&mut file).unwrap();
            let index = EntryIndex::parse(&bytes).unwrap();
            assert_eq!(
                index.entries.len(),
                bank.entry_count as usize,
                "{}",
                bank.name
            );
            assert_eq!(
                index.to_bytes(),
                bytes,
                "{} index does not round-trip",
                bank.name
            );
        }
    }
}

#[test]
#[ignore]
fn wld_maps_exist_in_wad_and_regions_reference_known_maps() {
    let wld = Wld::open(data_dir().join("Levels/FinalAlbion.wld")).unwrap();
    let wad = BbbbFile::open(data_dir().join("Levels/FinalAlbion.wad")).unwrap();
    assert!(!wld.maps.is_empty() && !wld.regions.is_empty());
    for map in &wld.maps {
        let wad_name = format!("Data\\Levels\\{}", map.level_name);
        assert!(
            wad.index.by_name(&wad_name).is_some(),
            "{wad_name} missing from wad"
        );
    }
    for region in &wld.regions {
        for level in region.contains_maps.iter().chain(&region.sees_maps) {
            assert!(
                wld.map(level).is_some(),
                "region {} names unknown map {level}",
                region.name
            );
        }
    }
}

#[test]
#[ignore]
fn stb_maps_match_wld_and_every_tile_decompresses() {
    use fable_data::stb::{COMMON_HEADER_NAME, CommonHeader, StaticMap};

    let path = data_dir().join("Levels/FinalAlbion_RT.stb");
    let stb = BbbbFile::open(&path).unwrap();
    let wld = Wld::open(data_dir().join("Levels/FinalAlbion.wld")).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());

    let common = stb
        .index
        .by_name(COMMON_HEADER_NAME)
        .unwrap()
        .read_data(&mut file)
        .unwrap();
    let common = CommonHeader::parse(&common).unwrap();
    assert_eq!(common.maps.len(), wld.maps.len());

    let mut tile_count = 0;
    for header in &common.maps {
        let level = header.level_path.strip_prefix("Data\\Levels\\").unwrap();
        let map = wld
            .map(level)
            .unwrap_or_else(|| panic!("{level} not in wld"));
        assert_eq!(
            (header.x as i32, header.y as i32),
            (map.x, map.y),
            "{level} position"
        );

        let entry = stb
            .index
            .by_name(&header.level_path)
            .unwrap()
            .read_data(&mut file)
            .unwrap();
        let stat = StaticMap::parse(&entry).unwrap();
        let (x0, y0) = (header.x as f32, header.y as f32);
        let (x1, y1) = (x0 + header.width as f32, y0 + header.height as f32);
        for tile in &stat.tiles {
            assert_eq!(tile.max[0] - tile.min[0], 16.0, "{level} tile width");
            assert_eq!(tile.max[1] - tile.min[1], 16.0, "{level} tile height");
            assert!(
                tile.min[0] >= x0 && tile.max[0] <= x1 && tile.min[1] >= y0 && tile.max[1] <= y1,
                "{level} tile outside map"
            );
            tile.decompress(&entry)
                .unwrap_or_else(|e| panic!("{level}: {e}"));
        }
        tile_count += stat.tiles.len();
    }
    assert!(tile_count > 0);
}
