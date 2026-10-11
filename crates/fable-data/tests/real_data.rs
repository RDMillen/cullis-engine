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

#[test]
#[ignore]
fn lev_heightmaps_match_stb_tiles() {
    use fable_data::lev::Lev;
    use fable_data::stb::{COMMON_HEADER_NAME, CommonHeader, StaticMap};

    let wad_path = data_dir().join("Levels/FinalAlbion.wad");
    let stb_path = data_dir().join("Levels/FinalAlbion_RT.stb");
    let wad = BbbbFile::open(&wad_path).unwrap();
    let stb = BbbbFile::open(&stb_path).unwrap();
    let mut wad_file = BufReader::new(File::open(&wad_path).unwrap());
    let mut stb_file = BufReader::new(File::open(&stb_path).unwrap());
    let common = stb
        .index
        .by_name(COMMON_HEADER_NAME)
        .unwrap()
        .read_data(&mut stb_file)
        .unwrap();
    let common = CommonHeader::parse(&common).unwrap();

    let (mut tiles, mut min_matches, mut contained) = (0, 0, 0);
    for header in &common.maps {
        let name = &header.level_path;
        let lev = wad
            .index
            .by_name(name)
            .unwrap()
            .read_data(&mut wad_file)
            .unwrap();
        let lev = Lev::parse(&lev).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            (lev.width, lev.height),
            (header.width, header.height),
            "{name} size"
        );
        let stb_value = u32::from_le_bytes(header.raw[0x34..0x38].try_into().unwrap());
        assert_eq!(
            lev.sound_value.unwrap_or(0),
            stb_value,
            "{name} sound value"
        );

        let entry = stb
            .index
            .by_name(name)
            .unwrap()
            .read_data(&mut stb_file)
            .unwrap();
        for tile in StaticMap::parse(&entry).unwrap().tiles {
            let x0 = tile.min[0] as u32 - header.x;
            let y0 = tile.min[1] as u32 - header.y;
            let heights: Vec<f32> = (0..=16)
                .flat_map(|j| (0..=16).map(move |i| (x0 + i, y0 + j)))
                .map(|(x, y)| lev.cell(x, y).unwrap().height())
                .collect();
            let lo = heights.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            tiles += 1;
            min_matches += usize::from((lo - tile.min[2]).abs() < 0.01);
            contained += usize::from(lo >= tile.min[2] - 0.01 && hi <= tile.max[2] + 0.01);
        }
    }
    eprintln!(
        "{tiles} tiles: {min_matches} minimum heights match, {contained} contain their terrain"
    );
    assert!(
        min_matches * 100 >= tiles * 99,
        "only {min_matches} of {tiles} tile minimums match"
    );
}

#[test]
#[ignore]
fn every_texture_unpacks_exactly() {
    use fable_data::texture::{Texture, TextureFormat, TextureInfo};

    let (mut unpacked, mut skipped) = (0, 0);
    for name in ["graphics/pc/textures.big", "graphics/pc/frontend.big"] {
        let path = data_dir().join(name);
        let mut file = BufReader::new(File::open(&path).unwrap());
        let big = BigFile::read(&mut file).unwrap();
        for bank in &big.banks {
            for entry in bank.read_index(&mut file).unwrap().entries {
                let info = TextureInfo::parse(&entry.info)
                    .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
                if let TextureFormat::Other(_) = info.format {
                    skipped += 1;
                    continue;
                }
                let data = entry.read_data(&mut file).unwrap();
                let texture = Texture::unpack(&info, entry.entry_type, &data).unwrap_or_else(|e| {
                    panic!("{} type {} {info:?}: {e}", entry.name, entry.entry_type)
                });
                texture.to_rgba(0, 0).unwrap();
                unpacked += 1;
            }
        }
    }
    eprintln!("{unpacked} textures unpacked, {skipped} in unsupported formats");
    assert!(skipped <= 1);
}

#[test]
#[ignore]
fn static_meshes_parse_and_reference_real_textures() {
    use fable_data::mesh::{Mesh, STATIC_MESH_TYPE};

    let textures = {
        let path = data_dir().join("graphics/pc/textures.big");
        let mut file = BufReader::new(File::open(&path).unwrap());
        let big = BigFile::read(&mut file).unwrap();
        let bank = big.bank("GBANK_MAIN_PC").unwrap();
        let index = bank.read_index(&mut file).unwrap();
        index.entries.iter().map(|e| e.id).collect::<HashSet<u32>>()
    };
    let path = data_dir().join("graphics/graphics.big");
    let mut file = BufReader::new(File::open(&path).unwrap());
    let big = BigFile::read(&mut file).unwrap();
    let index = big
        .bank("MBANK_ALLMESHES")
        .unwrap()
        .read_index(&mut file)
        .unwrap();
    let (mut parsed, mut failed, mut triangles) = (0, Vec::new(), 0usize);
    for entry in index
        .entries
        .iter()
        .filter(|e| e.entry_type == STATIC_MESH_TYPE)
    {
        let data = entry.read_data(&mut file).unwrap();
        match Mesh::parse_static(&entry.info, &data) {
            Ok(mesh) => {
                for lod in &mesh.lods {
                    for m in &lod.materials {
                        for t in [m.texture, m.second_texture] {
                            assert!(
                                t == 0 || textures.contains(&t),
                                "{} material {} texture {t}",
                                entry.name,
                                m.name
                            );
                        }
                    }
                    triangles += lod
                        .chunks
                        .iter()
                        .flat_map(|c| c.sub_meshes.iter().map(|s| c.triangles(s).len()))
                        .sum::<usize>();
                }
                parsed += 1;
            }
            Err(e) => failed.push(format!("{}: {e}", entry.name)),
        }
    }
    eprintln!(
        "{parsed} static meshes parsed ({triangles} triangles), {} failed, e.g. {:?}",
        failed.len(),
        &failed[..failed.len().min(3)]
    );
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
#[ignore]
fn skinned_meshes_parse_with_consistent_skeletons() {
    use fable_data::mesh::{Mesh, SKINNED_MESH_TYPE, mul_affine};

    let path = data_dir().join("graphics/graphics.big");
    let mut file = BufReader::new(File::open(&path).unwrap());
    let big = BigFile::read(&mut file).unwrap();
    let index = big
        .bank("MBANK_ALLMESHES")
        .unwrap()
        .read_index(&mut file)
        .unwrap();
    let (mut meshes, mut lods, mut worst_bind) = (0, 0, 0.0f32);
    let (mut unit_normals, mut normals) = (0usize, 0usize);
    for entry in index
        .entries
        .iter()
        .filter(|e| e.entry_type == SKINNED_MESH_TYPE)
    {
        let data = entry.read_data(&mut file).unwrap();
        let mesh = Mesh::parse_skinned(&entry.info, &data)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        for lod in &mesh.lods {
            let skeleton = lod.skeleton.as_ref().unwrap();
            assert!(!skeleton.bones.is_empty(), "{}", entry.name);
            // The composed rest pose undoes each inverse bind matrix.
            for (world, bone) in skeleton.rest_pose().iter().zip(&skeleton.bones) {
                let m = mul_affine(world, &bone.inverse_bind);
                for (i, row) in m.iter().enumerate() {
                    for (j, &v) in row.iter().enumerate() {
                        let want = if i == j { 1.0 } else { 0.0 };
                        let scale = if j == 3 { 100.0 } else { 1.0 };
                        worst_bind = worst_bind.max((v - want).abs() / scale);
                    }
                }
            }
            for chunk in &lod.chunks {
                assert!(chunk.skinned);
                for n in chunk.normals() {
                    let len = n.iter().map(|x| x * x).sum::<f32>().sqrt();
                    unit_normals += usize::from((0.9..1.1).contains(&len));
                    normals += 1;
                }
                let weights = chunk.bone_weights();
                for sub in &chunk.sub_meshes {
                    for t in chunk.triangles(sub) {
                        for v in t {
                            let (bones, w) = weights[usize::from(v)];
                            assert_eq!(w.iter().map(|&x| u32::from(x)).sum::<u32>(), 255);
                            for (b, w) in bones.iter().zip(w) {
                                assert!(
                                    w == 0 || usize::from(*b) < sub.bones.len(),
                                    "{} bone {b} of {}",
                                    entry.name,
                                    sub.bones.len()
                                );
                            }
                        }
                    }
                    assert!(
                        sub.bones
                            .iter()
                            .all(|&b| usize::from(b) < skeleton.bones.len())
                    );
                }
            }
            lods += 1;
        }
        meshes += 1;
    }
    eprintln!(
        "{meshes} skinned meshes, {lods} levels of detail; worst bind-pose error {worst_bind}; \
         {unit_normals} of {normals} normals unit length"
    );
    assert!(unit_normals * 100 >= normals * 99);
    assert_eq!(meshes, 504);
    assert!(worst_bind < 0.01, "{worst_bind}");
}

#[test]
#[ignore]
fn animations_parse_with_unit_quaternions() {
    use fable_data::anim::{ANIMATION_TYPES, AnimInfo, Animation};

    let path = data_dir().join("graphics/graphics.big");
    let mut file = BufReader::new(File::open(&path).unwrap());
    let big = BigFile::read(&mut file).unwrap();
    let index = big
        .bank("MBANK_ALLMESHES")
        .unwrap()
        .read_index(&mut file)
        .unwrap();
    let (mut parsed, mut tracks, mut failed) = (0, 0, Vec::new());
    let (mut unit, mut keys) = (0usize, 0usize);
    for entry in index
        .entries
        .iter()
        .filter(|e| ANIMATION_TYPES.contains(&e.entry_type))
    {
        let info = AnimInfo::parse(&entry.info).unwrap();
        let data = entry.read_data(&mut file).unwrap();
        let anim = match Animation::parse(&data) {
            Ok(a) => a,
            Err(e) => {
                failed.push(format!("{}: {e}", entry.name));
                continue;
            }
        };
        for track in anim.objects.iter().flat_map(|o| &o.tracks) {
            // Durations agree with the frame counts at the track's rate.
            assert!(
                (info.duration - track.frames as f32 / track.fps).abs() < 0.02 || track.frames <= 1,
                "{} {}: {} frames at {} fps, info {}",
                entry.name,
                track.name,
                track.frames,
                track.fps,
                info.duration
            );
            for q in &track.rotations {
                let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
                unit += usize::from((len - 1.0).abs() < 0.01);
                keys += 1;
            }
            for frames in [&track.rotation_frames, &track.position_frames] {
                assert!(frames.is_empty() || frames.len() == track.frames as usize);
            }
            tracks += 1;
        }
        parsed += 1;
    }
    eprintln!(
        "{parsed} animations ({tracks} tracks) parsed, {} failed, e.g. {:?}; \
         {unit} of {keys} rotation keys unit length",
        failed.len(),
        &failed[..failed.len().min(3)]
    );
    assert!(failed.is_empty());
    assert!(unit * 1000 >= keys * 999);
}

#[test]
#[ignore]
fn every_region_exit_leads_to_an_entrance() {
    use fable_data::gtg::{Gtg, split_connected_uid};
    use fable_data::tng::Tng;
    use fable_data::wld::Wld;

    let wld = Wld::open(data_dir().join("Levels/FinalAlbion.wld")).unwrap();
    let gtg_bytes = std::fs::read(data_dir().join("Levels/FinalAlbion.gtg")).unwrap();
    let gtg = Gtg::parse(&gtg_bytes.iter().map(|&b| b as char).collect::<String>()).unwrap();
    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut exits, mut unresolved) = (0, Vec::new());
    for map in &wld.maps {
        // Development regions (CreatureHub: no RegionDef, untranslated name)
        // have exits to maps that are gone.
        if !wld
            .regions
            .iter()
            .any(|r| !r.region_def.is_empty() && r.contains_maps.contains(&map.level_name))
        {
            continue;
        }
        let name = format!("Data\\Levels\\{}", map.level_name.replace(".lev", ".tng"));
        let Some(entry) = wad.index.by_name(&name) else {
            continue;
        };
        let text: String = entry
            .read_data(&mut file)
            .unwrap()
            .iter()
            .map(|&b| b as char)
            .collect();
        for thing in Tng::parse(&text).unwrap().things {
            let Some(exit) = thing.component("CTCDRegionExit") else {
                continue;
            };
            let connected: u64 = exit
                .fields
                .iter()
                .find(|f| f.key == "EntranceConnectedToUID")
                .unwrap()
                .value
                .parse()
                .unwrap();
            let (map_uid, local) = split_connected_uid(connected);
            let entrance = wld
                .maps
                .iter()
                .find(|m| m.uid == map_uid)
                .and_then(|dest| gtg.map(dest.number))
                .and_then(|m| m.thing(local));
            match entrance {
                Some(e) => assert!(
                    e.component("CTCDRegionEntrance").is_some(),
                    "{} -> {:?}",
                    map.level_name,
                    e.definition()
                ),
                None => unresolved.push(format!("{} -> map UID {map_uid}", map.level_name)),
            }
            exits += 1;
        }
    }
    eprintln!(
        "{exits} region exits, {} unresolved: {unresolved:?}",
        unresolved.len()
    );
    assert!(exits > 100 && unresolved.is_empty());
}

#[test]
#[ignore]
fn every_bone_config_parses_with_sane_scales() {
    use fable_data::bncfg::BoneConfig;

    let mut files = 0;
    for entry in std::fs::read_dir(data_dir().join("Bones")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "bncfg") {
            continue;
        }
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let config = BoneConfig::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!config.bones.is_empty(), "{}", path.display());
        for (bone, scale) in &config.bones {
            assert!(
                scale.iter().all(|s| (0.05..10.0).contains(s)),
                "{} {bone} {scale:?}",
                path.display()
            );
        }
        files += 1;
    }
    assert!(files > 50, "{files}");
}

#[test]
#[ignore]
fn every_tng_parses_and_places_its_buildings() {
    use fable_data::tng::Tng;

    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut files, mut things) = (0, 0);
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".tng"))
    {
        let bytes = entry.read_data(&mut file).unwrap();
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let tng = Tng::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        let raw = text
            .lines()
            .filter(|l| l.trim_start().starts_with("NewThing "))
            .count();
        assert_eq!(tng.things.len(), raw, "{}", entry.name);
        for t in tng
            .things
            .iter()
            .filter(|t| t.kind == "Building" || t.kind == "Object")
        {
            assert!(
                t.position().is_some(),
                "{} {:?} has no position",
                entry.name,
                t.definition()
            );
            assert!(
                t.definition().is_some(),
                "{} thing without a definition",
                entry.name
            );
        }
        files += 1;
        things += tng.things.len();
    }
    eprintln!("{files} tng files, {things} things");
    assert!(files > 0);
}

#[test]
#[ignore]
fn placed_things_stand_on_the_lev_ground() {
    use fable_data::lev::Lev;
    use fable_data::tng::Tng;

    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let mut diffs = Vec::new();
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".tng"))
    {
        let lev_name = entry.name.replace(".tng", ".lev");
        let lev = wad
            .index
            .by_name(&lev_name)
            .unwrap()
            .read_data(&mut file)
            .unwrap();
        let lev = Lev::parse(&lev).unwrap();
        let text: String = entry
            .read_data(&mut file)
            .unwrap()
            .iter()
            .map(|&b| b as char)
            .collect();
        for t in Tng::parse(&text).unwrap().things {
            if t.kind != "Building" && t.kind != "Object" {
                continue;
            }
            let [x, y, z] = t.position().unwrap();
            let (cx, cy) = (x.round(), y.round());
            if cx < 0.0 || cy < 0.0 {
                continue;
            }
            if let Some(cell) = lev.cell(cx as u32, cy as u32) {
                diffs.push((z - cell.height()).abs());
            }
        }
    }
    diffs.sort_by(f32::total_cmp);
    let within = |d: f32| diffs.iter().filter(|&&v| v <= d).count() * 100 / diffs.len();
    eprintln!(
        "{} things: median |z - ground| {:.3}; within 0.5: {}%, within 2: {}%",
        diffs.len(),
        diffs[diffs.len() / 2],
        within(0.5),
        within(2.0)
    );
    assert!(
        diffs[diffs.len() / 2] < 0.5,
        "positions do not line up with the Lev ground"
    );
}

#[test]
#[ignore]
fn game_defs_parse_and_resolve_placed_meshes() {
    use fable_data::defs::{Defs, Names};
    use fable_data::tng::Tng;

    let names =
        Names::parse(&std::fs::read(data_dir().join("CompiledDefs/names.bin")).unwrap()).unwrap();
    assert!(
        names.hashes_match(),
        "a names.bin hash does not match its name"
    );
    let defs = Defs::parse(
        &std::fs::read(data_dir().join("CompiledDefs/game.bin")).unwrap(),
        &names,
    )
    .unwrap();
    // Buildings are drawn with every mesh of their CMultiStaticMeshDef component:
    // the exterior and an interior with its floors.
    let house = defs.get("BUILDING_OAKVALE_HOUSE_LARGE").unwrap();
    let meshes = defs.meshes_of(house);
    assert_eq!(meshes.len(), 2, "{meshes:?}");
    assert!(meshes.contains(&house.graphic().unwrap()));
    let tavern = defs
        .get("BUILDING_OAKVALE_TAVERN")
        .expect("tavern definition");
    assert_eq!(defs.type_name(tavern), "BUILDING");

    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut placed, mut resolved) = (0, 0);
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".tng"))
    {
        let text: String = entry
            .read_data(&mut file)
            .unwrap()
            .iter()
            .map(|&b| b as char)
            .collect();
        for t in Tng::parse(&text).unwrap().things {
            if t.kind != "Building" && t.kind != "Object" {
                continue;
            }
            placed += 1;
            let def = defs
                .get(t.definition().unwrap())
                .unwrap_or_else(|| panic!("{} has no definition", t.definition().unwrap()));
            resolved += usize::from(def.graphic().is_some());
        }
    }
    eprintln!(
        "{} types, {} definitions, {} names; {resolved} of {placed} placed buildings and objects have a Graphic",
        defs.types.len(),
        defs.defs.len(),
        names.len()
    );
    assert!(resolved * 100 >= placed * 90);
}

#[test]
#[ignore]
fn every_used_lev_theme_has_textures() {
    use fable_data::defs::{Defs, Names};
    use fable_data::lev::Lev;

    let names =
        Names::parse(&std::fs::read(data_dir().join("CompiledDefs/names.bin")).unwrap()).unwrap();
    let defs = Defs::parse(
        &std::fs::read(data_dir().join("CompiledDefs/game.bin")).unwrap(),
        &names,
    )
    .unwrap();
    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut themes, mut textured, mut points, mut in_table) = (HashSet::new(), 0, 0, 0);
    let mut missing = Vec::new();
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".lev"))
    {
        let lev = Lev::parse(&entry.read_data(&mut file).unwrap()).unwrap();
        let slots: HashSet<u8> = lev.themes.iter().map(|t| t.slot).collect();
        for cell in &lev.cells {
            let ([t0, t1, _], [_, w1]) = cell.themes();
            points += 1;
            in_table += usize::from(slots.contains(&t0) && (w1 == 0 || slots.contains(&t1)));
        }
        for t in lev.themes.iter().filter(|t| !t.name.is_empty()) {
            if themes.insert(t.name.clone()) {
                let Some(def) = defs.get(&t.name) else {
                    missing.push(t.name.clone());
                    continue;
                };
                assert_eq!(defs.type_name(def), "ENGINE_THEME", "{}", t.name);
                textured += usize::from(def.theme_textures().base != 0);
            }
        }
    }
    eprintln!(
        "{} themes, {textured} with a base texture, no definition for {missing:?}; {in_table} of {points} grid points use listed slots",
        themes.len()
    );
    assert!(missing.len() * 20 <= themes.len());
    assert_eq!(in_table, points);
}

#[test]
#[ignore]
fn every_lev_tail_parses_into_sections() {
    use fable_data::lev::Lev;

    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut maps, mut sections, mut fixed_sizes) = (0, 0, HashSet::new());
    let mut without_null = Vec::new();
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".lev"))
    {
        let data = entry.read_data(&mut file).unwrap();
        let lev = Lev::parse(&data).unwrap();
        let tail = lev
            .parse_tail(&data)
            .unwrap_or_else(|e| panic!("{}: {e}", entry.name));
        if tail.sections.first().map(|s| s.name.as_str()) != Some("NULL") {
            without_null.push(entry.name.clone());
        }
        for s in &tail.sections {
            assert_eq!(
                s.size,
                [lev.width as f32, lev.height as f32],
                "{} {}",
                entry.name,
                s.name
            );
        }
        fixed_sizes.insert(tail.fixed_block.len());
        maps += 1;
        sections += tail.sections.len();
    }
    let no_sections = without_null.len();
    eprintln!(
        "{maps} maps, {sections} sections, fixed block sizes {fixed_sizes:?}; {no_sections} maps have no NULL first section"
    );
    assert_eq!(fixed_sizes.len(), 1);
}

#[test]
#[ignore]
fn every_navigation_section_parses_exactly() {
    use fable_data::lev::Lev;
    use fable_data::nav::Navigation;

    let path = data_dir().join("Levels/FinalAlbion.wad");
    let wad = BbbbFile::open(&path).unwrap();
    let mut file = BufReader::new(File::open(&path).unwrap());
    let (mut sections, mut leaves, mut layers) = (0, 0, 0);
    for entry in wad
        .index
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".lev"))
    {
        let data = entry.read_data(&mut file).unwrap();
        let lev = Lev::parse(&data).unwrap();
        for s in lev.parse_tail(&data).unwrap().sections {
            let nav = Navigation::parse(&s.data, lev.width, lev.height)
                .unwrap_or_else(|e| panic!("{} {}: {e}", entry.name, s.name));
            sections += 1;
            layers += nav.layers.len();
            leaves += nav.leaves().count();
        }
    }
    eprintln!("{sections} navigation sections, {layers} layers, {leaves} leaves");
    assert!(sections > 0);
}
