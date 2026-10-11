//! Compiled game definitions: `CompiledDefs/names.bin` and `CompiledDefs/game.bin`.
//! See `docs/formats/defs.md`.
//!
//! Decoded: the name table, the definition types, the directory linking each
//! definition to its name, and the zlib-compressed records. Record contents are only
//! searched for known fields (such as `Graphic`), not decoded in general.

use std::collections::HashMap;

use crate::reader::Reader;
use crate::{Error, Result};

/// Name and field hash: the CRC-32 polynomial (reflected) with an initial value of 0
/// and no final inversion.
pub fn name_hash(name: &str) -> u32 {
    let mut crc = 0u32;
    for b in name.bytes() {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Names are addressed by byte offset from this point in `names.bin`.
const NAMES_BASE: usize = 16;
const NAMES_HEADER_LEN: usize = 20;

/// `names.bin`: hashes and names of definitions, fields and types.
#[derive(Debug, Clone, Default)]
pub struct Names {
    /// Matches the second header word of the `game.bin` that uses these names.
    pub id: u32,
    by_offset: HashMap<u32, (u32, String)>,
}

impl Names {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let _magic = r.u32()?;
        let id = r.u32()?;
        let count = r.u32()? as usize;
        let mut by_offset = HashMap::with_capacity(count);
        let mut r = Reader::new(data.get(NAMES_HEADER_LEN..).unwrap_or_default());
        for _ in 0..count {
            let offset = (NAMES_HEADER_LEN + r.pos() - NAMES_BASE) as u32;
            let hash = r.u32()?;
            let name = r.c_string()?;
            by_offset.insert(offset, (hash, name));
        }
        Ok(Self { id, by_offset })
    }

    /// The name stored at a `game.bin` name offset.
    pub fn at(&self, offset: u32) -> Option<&str> {
        self.by_offset.get(&offset).map(|(_, n)| n.as_str())
    }

    pub fn len(&self) -> usize {
        self.by_offset.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_offset.is_empty()
    }

    /// Every (hash, name) pair.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.by_offset.values().map(|(h, n)| (*h, n.as_str()))
    }

    /// Checks every stored hash against its name.
    pub fn hashes_match(&self) -> bool {
        self.by_offset.values().all(|(h, n)| *h == name_hash(n))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefType {
    pub name: String,
    /// Name of the type's null definition, e.g. `NULLDEF_BUILDING`.
    pub null_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// Index into [`Defs::types`].
    pub def_type: usize,
    /// `None` for definitions with no name in `names.bin`.
    pub name: Option<String>,
    /// The definition's record, not decoded in general.
    pub record: Vec<u8>,
}

/// Field of `ENGINE_THEME` records holding the cliff (steep slope) texture, e.g.
/// `UNASSIGNED_CLIFF_LAYERED_01`. Its name is not known; another field
/// (`0x99b491a3`) holds the same id in every theme seen.
const CLIFF_TEXTURE_FIELD: u32 = 0x7a1c_6732;

/// Texture bank (`GBANK_MAIN_PC`) ids used by a terrain theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeTextures {
    /// Flat ground, e.g. `UNASSIGNED_GRASS_PLAIN`.
    pub base: u32,
    /// Steep slopes, e.g. `UNASSIGNED_CLIFF_LAYERED_01`.
    pub cliff: u32,
    pub base_bump: u32,
    pub cliff_bump: u32,
}

/// Value type tag that precedes graphic (mesh) references in records: 5 for most
/// objects and buildings (static meshes), 4 for creatures and animated objects
/// (beds, doors), whose `Graphic` is a skinned mesh.
const GRAPHIC_VALUE_TAG: u32 = 5;
const SKINNED_GRAPHIC_VALUE_TAG: u32 = 4;

impl Def {
    /// Finds `field` (by name) with value tag `tag` and returns the `u32` after it.
    /// Records are searched rather than parsed, since their layout is not decoded.
    pub fn find_u32(&self, field: &str, tag: u32) -> Option<u32> {
        let mut pattern = name_hash(field).to_le_bytes().to_vec();
        pattern.extend_from_slice(&tag.to_le_bytes());
        let at = self.record.windows(8).position(|w| w == pattern)?;
        let v = self.record.get(at + 8..at + 12)?;
        Some(u32::from_le_bytes(v.try_into().unwrap()))
    }

    /// The `u32` right after the first occurrence of a field hash. Theme records are a
    /// 5-byte header then (field hash, `u32` value) pairs.
    pub fn value_after(&self, field_hash: u32) -> Option<u32> {
        let at = self
            .record
            .windows(4)
            .position(|w| w == field_hash.to_le_bytes())?;
        let v = self.record.get(at + 4..at + 8)?;
        Some(u32::from_le_bytes(v.try_into().unwrap()))
    }

    /// Texture ids of an `ENGINE_THEME` definition (0 where unset).
    pub fn theme_textures(&self) -> ThemeTextures {
        let get = |h: u32| self.value_after(h).unwrap_or(0);
        ThemeTextures {
            base: get(name_hash("BaseTexture")),
            cliff: get(CLIFF_TEXTURE_FIELD),
            base_bump: get(name_hash("BaseBumpMap")),
            cliff_bump: get(name_hash("CliffBumpMap")),
        }
    }

    /// The global definition index of this definition's component of type
    /// `component` (`CMultiStaticMeshDef`, `CAppearanceDef`, …). Entity records
    /// list components as (type name hash, `u32` definition index, `u32`).
    pub fn component(&self, component: &str) -> Option<usize> {
        self.value_after(name_hash(component)).map(|v| v as usize)
    }

    /// Every `u32` that follows the field `field` in the record, in order.
    pub fn values_after(&self, field: &str) -> Vec<u32> {
        let hash = name_hash(field).to_le_bytes();
        self.record
            .windows(8)
            .filter(|w| w[..4] == hash)
            .map(|w| u32::from_le_bytes(w[4..].try_into().unwrap()))
            .collect()
    }

    /// Mesh bank entry ids of a `CMultiStaticMeshDef` (its `Mesh` fields): a
    /// building's exterior and interior (`MESH_OAKVALE_HOUSE_03` and
    /// `MESH_OAKVALE_HOUSE_INSIDE_03` for `BUILDING_OAKVALE_HOUSE_LARGE`).
    pub fn static_meshes(&self) -> Vec<u32> {
        self.static_mesh_entries().iter().map(|e| e.mesh).collect()
    }

    /// The `Mesh` entries of a `CMultiStaticMeshDef` with their exterior and
    /// interior flags (each a field hash and a byte, within the next 20 bytes).
    pub fn static_mesh_entries(&self) -> Vec<StaticMeshEntry> {
        let r = &self.record;
        let flag = |from: usize, field: u32| {
            let hash = field.to_le_bytes();
            let end = (from + 24).min(r.len());
            r.get(from..end)?
                .windows(5)
                .find(|w| w[..4] == hash)
                .map(|w| w[4] == 1)
        };
        let hash = name_hash("Mesh").to_le_bytes();
        (0..r.len().saturating_sub(7))
            .filter(|&i| r[i..i + 4] == hash)
            .map(|i| StaticMeshEntry {
                mesh: u32::from_le_bytes(r[i + 4..i + 8].try_into().unwrap()),
                exterior: flag(i + 8, EXTERIOR_FLAG_FIELD).unwrap_or(false),
                interior: flag(i + 8, INTERIOR_FLAG_FIELD).unwrap_or(false),
            })
            .collect()
    }

    /// Mesh bank entry id from the `Graphic` field, if the definition has one
    /// (value tag 5, or 4 for skinned meshes such as creatures, beds and doors).
    pub fn graphic(&self) -> Option<u32> {
        self.find_u32("Graphic", GRAPHIC_VALUE_TAG)
            .or_else(|| self.find_u32("Graphic", SKINNED_GRAPHIC_VALUE_TAG))
    }
}

/// One entry of a `CAppearanceDef`'s animation table: what a creature plays for a
/// slot (`ST_IDLE_SUBTLE`, `MOVEMENT_MEDIUM_RIGHT`, `CS_SHIFT_WEIGHT3`, …), maybe
/// only with a modifier (`SWORD`, `BOW`, `CROUCH`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationSlot {
    pub slot: String,
    /// `MBANK_ALLMESHES` entry id of the animation (`ANIM_HERO_IDLE`).
    pub animation: u32,
    pub modifier: Option<String>,
}

/// One mesh of a `CMultiStaticMeshDef` (a building's exterior or interior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticMeshEntry {
    pub mesh: u32,
    /// Field `0x7ca90715`: 1 on the exterior (`MESH_OAKVALE_HOUSE_03`), 0 on the
    /// interior. Presumably what is hidden when the player goes inside.
    pub exterior: bool,
    /// Field `0x97595fc1`: 1 on the interior (`MESH_OAKVALE_HOUSE_INSIDE_03`).
    pub interior: bool,
}

/// Field hashes after each `Mesh` of a `CMultiStaticMeshDef` (names unknown).
const EXTERIOR_FLAG_FIELD: u32 = 0x7ca9_0715;
const INTERIOR_FLAG_FIELD: u32 = 0x9759_5fc1;

/// One choice for a body part of a built-up creature (villagers, guards,
/// children): a skinned mesh and texture swaps for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartOption {
    pub mesh: u32,
    /// 1 in every option looked at.
    pub unknown: u32,
    /// (texture the mesh names, texture to use instead, unknown 0): e.g.
    /// `TEXTURE_BOWER_MALE_MIDDLE_FACE_01` -> `TEXTURE_OV_MALE_MIDDLE_FACE_01` for
    /// an Oakvale villager.
    pub texture_swaps: Vec<(u32, u32, u32)>,
    pub unknown_flag: u8,
}

/// Reads a list of part options at `at`: a `u32` count, then per option `u32`
/// mesh, `u32` 1, `u32` swap count n, `u32` 12n, n x (3 x `u32`), `u8`.
fn part_list(record: &[u8], at: usize) -> Option<(Vec<PartOption>, usize)> {
    let word = |o: usize| {
        record
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let count = word(at)?;
    if !(1..=64).contains(&count) {
        return None;
    }
    let mut q = at + 4;
    let mut options = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (mesh, unknown, n, len) = (word(q)?, word(q + 4)?, word(q + 8)?, word(q + 12)?);
        if mesh == 0 || unknown != 1 || n > 8 || len != 12 * n {
            return None;
        }
        let texture_swaps = (0..n as usize)
            .map(|k| {
                let o = q + 16 + 12 * k;
                Some((word(o)?, word(o + 4)?, word(o + 8)?))
            })
            .collect::<Option<Vec<_>>>()?;
        q += 16 + len as usize;
        let unknown_flag = *record.get(q)?;
        q += 1;
        options.push(PartOption {
            mesh,
            unknown,
            texture_swaps,
            unknown_flag,
        });
    }
    Some((options, q))
}

impl Def {
    /// The animation table of a `CAppearanceDef`: entries of `u32` slot name hash,
    /// `u32` animation id, `u32` slot name (a `names.bin` offset), `u32` modifier
    /// name offset or `0xffffffff`. Found by that layout (the slot's name must hash
    /// to the hash before it), since the record is not decoded.
    pub fn animation_slots(&self, names: &Names) -> Vec<AnimationSlot> {
        let r = &self.record;
        let word = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        (0..r.len().saturating_sub(15))
            .filter_map(|i| {
                let (hash, animation, name, modifier) =
                    (word(i), word(i + 4), word(i + 8), word(i + 12));
                let slot = names.at(name).filter(|n| name_hash(n) == hash)?;
                let modifier = match modifier {
                    0xffff_ffff => None,
                    m => Some(names.at(m)?.to_owned()),
                };
                Some(AnimationSlot {
                    slot: slot.to_owned(),
                    animation,
                    modifier,
                })
            })
            .collect()
    }

    /// The part lists of a `CCreatureDef` (heads, torsos, legs, in that order for
    /// Oakvale villagers), found by their layout since the record is not decoded.
    /// Empty for creatures drawn with their `Graphic` alone.
    pub fn part_lists(&self) -> Vec<Vec<PartOption>> {
        let mut lists = Vec::new();
        let mut at = 0;
        while at + 4 <= self.record.len() {
            match part_list(&self.record, at) {
                Some((list, end)) => {
                    lists.push(list);
                    at = end;
                }
                None => at += 1,
            }
        }
        lists
    }
}

/// `game.bin` (or another compiled definition file using the same names).
#[derive(Debug, Clone)]
pub struct Defs {
    pub types: Vec<DefType>,
    /// Every definition: each type's null definition first (in type order), then the
    /// directory's definitions in directory order.
    pub defs: Vec<Def>,
    by_name: HashMap<String, usize>,
}

const HEADER_LEN: usize = 13;
const ROW_LEN: usize = 12;

impl Defs {
    pub fn parse(data: &[u8], names: &Names) -> Result<Self> {
        let mut r = Reader::new(data);
        let _flag: [u8; 1] = r.array()?;
        let _checksum = r.u32()?;
        let names_id = r.u32()?;
        if names_id != names.id {
            return Err(Error::Invalid {
                offset: 5,
                reason: format!(
                    "names id {names_id:#x} does not match names.bin ({:#x})",
                    names.id
                ),
            });
        }
        let def_count = r.u32()? as usize;
        let missing = |offset: u32| Error::Invalid {
            offset: offset as usize,
            reason: format!("no names.bin name at offset {offset:#x}"),
        };

        // Types: (name, null name, 0) rows until the third word becomes a 1-based index.
        let mut types = Vec::new();
        let mut type_index = HashMap::new();
        let mut pos = HEADER_LEN;
        loop {
            let mut row = Reader::new(data.get(pos..pos + ROW_LEN).unwrap_or_default());
            let (name, null_name, zero) = (row.u32()?, row.u32()?, row.u32()?);
            if zero != 0 {
                break;
            }
            type_index.insert(name, types.len());
            types.push(DefType {
                name: names.at(name).ok_or_else(|| missing(name))?.to_owned(),
                null_name: names
                    .at(null_name)
                    .ok_or_else(|| missing(null_name))?
                    .to_owned(),
            });
            pos += ROW_LEN;
        }
        let mut entries: Vec<(usize, Option<String>)> = types
            .iter()
            .enumerate()
            .map(|(i, t)| (i, Some(t.null_name.clone())))
            .collect();

        // Directory: (type name, definition name or 0xffffffff, index within type).
        let directory_len = def_count.checked_sub(types.len()).ok_or(Error::Invalid {
            offset: pos,
            reason: format!("{} types for {def_count} definitions", types.len()),
        })?;
        for _ in 0..directory_len {
            let mut row = Reader::new(data.get(pos..pos + ROW_LEN).unwrap_or_default());
            let (type_name, name, _index) = (row.u32()?, row.u32()?, row.u32()?);
            let def_type = *type_index
                .get(&type_name)
                .ok_or_else(|| missing(type_name))?;
            let name = match name {
                0xffff_ffff => None,
                n => Some(names.at(n).ok_or_else(|| missing(n))?.to_owned()),
            };
            entries.push((def_type, name));
            pos += ROW_LEN;
        }

        // Chunk table: (first definition, compressed offset) pairs ending with a
        // sentinel, then the compressed length and the zlib streams.
        let mut r = Reader::new(&data[pos..]);
        let chunk_count = r.u32()? as usize;
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            chunks.push((r.u32()? as usize, r.u32()? as usize));
        }
        let _compressed_len = r.u32()?;
        let base = pos + r.pos();
        let mut records = Vec::with_capacity(def_count);
        for w in chunks.windows(2) {
            let ((first, start), (next, end)) = (w[0], w[1]);
            let compressed = data
                .get(base + start..base + end)
                .ok_or(Error::UnexpectedEof {
                    offset: base + start,
                    wanted: end.saturating_sub(start),
                })?;
            let chunk = miniz_oxide::inflate::decompress_to_vec_zlib(compressed).map_err(|e| {
                Error::Invalid {
                    offset: base + start,
                    reason: format!("zlib: {e:?}"),
                }
            })?;
            let count = next - first;
            let mut offsets = Reader::new(&chunk);
            let mut starts = (0..count)
                .map(|_| {
                    offsets
                        .array::<2>()
                        .map(|b| usize::from(u16::from_le_bytes(b)))
                })
                .collect::<Result<Vec<_>>>()?;
            starts.push(chunk.len());
            for s in starts.windows(2) {
                let record = chunk.get(s[0]..s[1]).ok_or(Error::Invalid {
                    offset: base + start,
                    reason: format!(
                        "record {}..{} outside a {}-byte chunk",
                        s[0],
                        s[1],
                        chunk.len()
                    ),
                })?;
                records.push(record.to_vec());
            }
        }
        if records.len() != def_count || entries.len() != def_count {
            return Err(Error::Invalid {
                offset: base,
                reason: format!(
                    "{} records and {} directory entries for {def_count} definitions",
                    records.len(),
                    entries.len()
                ),
            });
        }
        let defs: Vec<Def> = entries
            .into_iter()
            .zip(records)
            .map(|((def_type, name), record)| Def {
                def_type,
                name,
                record,
            })
            .collect();
        let by_name = defs
            .iter()
            .enumerate()
            .filter_map(|(i, d)| Some((d.name.clone()?, i)))
            .collect();
        Ok(Self {
            types,
            defs,
            by_name,
        })
    }

    /// The meshes an entity is drawn with: those of its `CMultiStaticMeshDef`
    /// component if it has one (buildings: exterior and interior), else its
    /// `Graphic`.
    pub fn meshes_of(&self, def: &Def) -> Vec<u32> {
        self.mesh_entries_of(def).iter().map(|e| e.mesh).collect()
    }

    /// As [`Defs::meshes_of`], with each mesh's exterior/interior flags (both
    /// false for a lone `Graphic`).
    pub fn mesh_entries_of(&self, def: &Def) -> Vec<StaticMeshEntry> {
        let multi = def
            .component("CMultiStaticMeshDef")
            .and_then(|i| self.defs.get(i))
            .filter(|c| self.type_name(c) == "CMultiStaticMeshDef")
            .map(Def::static_mesh_entries)
            .unwrap_or_default();
        if multi.is_empty() {
            def.graphic()
                .into_iter()
                .map(|mesh| StaticMeshEntry {
                    mesh,
                    exterior: false,
                    interior: false,
                })
                .collect()
        } else {
            multi
        }
    }

    /// A creature's animation table, from its `CAppearanceDef` component.
    pub fn animation_slots(&self, def: &Def, names: &Names) -> Vec<AnimationSlot> {
        def.component("CAppearanceDef")
            .and_then(|i| self.defs.get(i))
            .filter(|c| self.type_name(c) == "CAppearanceDef")
            .map(|c| c.animation_slots(names))
            .unwrap_or_default()
    }

    /// A creature's body part lists, from its `CCreatureDef` component.
    pub fn creature_parts(&self, def: &Def) -> Vec<Vec<PartOption>> {
        def.component("CCreatureDef")
            .and_then(|i| self.defs.get(i))
            .filter(|c| self.type_name(c) == "CCreatureDef")
            .map(Def::part_lists)
            .unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<&Def> {
        self.by_name.get(name).map(|&i| &self.defs[i])
    }

    pub fn type_name(&self, def: &Def) -> &str {
        &self.types[def.def_type].name
    }
}

/// Builds a `names.bin` by hand for tests; returns it with each name's offset.
#[cfg(test)]
fn names_bin(id: u32, names: &[&str]) -> (Vec<u8>, Vec<u32>) {
    let mut out = Vec::new();
    for v in [0x7ab1e_u32, id, names.len() as u32, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let mut offsets = Vec::new();
    for n in names {
        offsets.push((out.len() - NAMES_BASE) as u32);
        out.extend_from_slice(&name_hash(n).to_le_bytes());
        out.extend_from_slice(n.as_bytes());
        out.push(0);
    }
    (out, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_part_lists() {
        let mut record = vec![1, 0, 1, 0xaa];
        let words = |r: &mut Vec<u8>, w: &[u32]| {
            w.iter().for_each(|v| r.extend_from_slice(&v.to_le_bytes()))
        };
        // Two heads (the second with one texture swap), then one torso.
        words(&mut record, &[2, 5150, 1, 0, 0]);
        record.push(0);
        words(&mut record, &[5151, 1, 1, 12, 1968, 3253, 0]);
        record.push(1);
        words(&mut record, &[1, 5163, 1, 0, 0]);
        record.push(0);
        let def = Def {
            def_type: 0,
            name: None,
            record,
        };
        let lists = def.part_lists();
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].len(), 2);
        assert_eq!(lists[0][1].texture_swaps, [(1968, 3253, 0)]);
        assert_eq!(lists[1][0].mesh, 5163);
    }

    #[test]
    fn finds_animation_slots() {
        let (names_data, off) = names_bin(1, &["ST_IDLE_SUBTLE", "SWORD"]);
        let names = Names::parse(&names_data).unwrap();
        let mut record = vec![1, 0, 1, 7];
        for w in [name_hash("ST_IDLE_SUBTLE"), 3041, off[0], 0xffff_ffff] {
            record.extend_from_slice(&w.to_le_bytes());
        }
        for w in [name_hash("ST_IDLE_SUBTLE"), 3120, off[0], off[1]] {
            record.extend_from_slice(&w.to_le_bytes());
        }
        // A hash that does not match the name at the offset is not a slot.
        for w in [0x1234_5678, 99, off[0], 0xffff_ffff] {
            record.extend_from_slice(&w.to_le_bytes());
        }
        let def = Def {
            def_type: 0,
            name: None,
            record,
        };
        let slots = def.animation_slots(&names);
        assert_eq!(slots.len(), 2);
        assert_eq!(
            (
                slots[0].slot.as_str(),
                slots[0].animation,
                slots[0].modifier.as_deref()
            ),
            ("ST_IDLE_SUBTLE", 3041, None)
        );
        assert_eq!(slots[1].modifier.as_deref(), Some("SWORD"));
    }

    #[test]
    fn hashes_names_like_names_bin() {
        assert_eq!(name_hash("NULLDEF_FRONT_END"), 0xe34b_378d);
        assert_eq!(name_hash("BUILDING_TOWN_WAREHOUSE_02"), 0x647c_b337);
        assert_eq!(name_hash("Graphic"), 0x2e6b_63c8);
    }

    #[test]
    fn parses_types_directory_and_records() {
        let (names_data, off) = names_bin(
            0xa8e3_6c34,
            &["BUILDING", "NULLDEF_BUILDING", "BUILDING_HUT"],
        );
        let names = Names::parse(&names_data).unwrap();
        assert!(names.hashes_match());

        let mut g = vec![0u8];
        for v in [0x1234_5678_u32, 0xa8e3_6c34, 3] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        for v in [off[0], off[1], 0] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        for v in [off[0], off[2], 1] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        // A second directory entry without a name.
        for v in [off[0], 0xffff_ffff, 2] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        // One chunk holding 3 records: empty, a Graphic field, and 2 bytes.
        let mut chunk = Vec::new();
        let hut: Vec<u8> = [name_hash("Graphic"), GRAPHIC_VALUE_TAG, 6913]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        for o in [6u16, 6, 6 + hut.len() as u16] {
            chunk.extend_from_slice(&o.to_le_bytes());
        }
        chunk.extend_from_slice(&hut);
        chunk.extend_from_slice(&[7, 7]);
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&chunk, 6);
        for v in [2u32, 0, 0, 3, z.len() as u32, z.len() as u32] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        g.extend_from_slice(&z);

        let defs = Defs::parse(&g, &names).unwrap();
        assert_eq!(defs.types[0].name, "BUILDING");
        assert_eq!(defs.defs.len(), 3);
        assert_eq!(defs.defs[0].name.as_deref(), Some("NULLDEF_BUILDING"));
        let hut = defs.get("BUILDING_HUT").unwrap();
        assert_eq!(defs.type_name(hut), "BUILDING");
        assert_eq!(hut.graphic(), Some(6913));
        assert_eq!(defs.defs[2].name, None);
        assert_eq!(defs.defs[2].record, [7, 7]);
    }

    #[test]
    fn rejects_mismatched_names() {
        let (names_data, _) = names_bin(1, &[]);
        let names = Names::parse(&names_data).unwrap();
        let mut g = vec![0u8];
        for v in [0u32, 2, 0] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        assert!(Defs::parse(&g, &names).is_err());
    }
}
