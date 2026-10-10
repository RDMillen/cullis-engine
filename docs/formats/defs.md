# Compiled definitions

`CompiledDefs/names.bin` and `CompiledDefs/game.bin`. Parser: `fable_data::defs`. All 14,761 definitions in `game.bin` load; every hash in `names.bin` matches its name; every placed building and object in every Tng names a definition, and 11,677 of 12,255 placements resolve to a mesh through its `Graphic` field (`tests/real_data.rs`).

## Name hash

`fable_data::defs::name_hash`: the reflected CRC-32 polynomial (`0xedb88320`) with initial value 0 and no final inversion. Used for definition names and record field names (`Graphic` hashes to `0x2e6b63c8`).

## names.bin

```
u32 magic (0x7ab1e), u32 id (0xa8e36c34, repeated in game.bin), u32 count, u32 data length, u32 0
count x { u32 hash, NUL-terminated name }
```

`game.bin` refers to names by byte offset counted from byte 16 of `names.bin` (so the first record, at byte 20, is offset 4). Names are grouped by type, each group starting with `NULLDEF_<type>`.

## game.bin

```
u8 0, u32 checksum?, u32 names id (must equal names.bin's), u32 definition count (14,761)
type table      rows of (u32 type name, u32 null definition name, u32 0); 249 rows
directory       rows of (u32 type name, u32 definition name or 0xffffffff, u32 index within type);
                definition count - type count rows (14,512)
chunk table     u32 count, count x (u32 first definition, u32 compressed offset); the last pair is a
                sentinel (definition count, total compressed length)
u32 total compressed length
zlib streams    one per chunk, at base + compressed offset
```

Name fields are `names.bin` offsets. The type table ends where a row's third word stops being 0. Definitions without a name (5,873) still have records.

**Global definition order**: the 249 null definitions in type-table order, then the directory in file order. Definition *i* is the *i*-th record across the chunks. (Checked: `BUILDING_OAKVALE_TAVERN` lands on the record holding `MESH_OAKVALE_TAVERN_01`'s id; other orders tried do not.)

**Chunk**: a zlib stream that decompresses to at most 32 KB: `u16` offsets for each of its definitions, then their records back to back.

## Records

Not decoded in general. Records hold fields as a name hash, a value type tag and a value. A mesh reference is `Graphic` (hash `0x2e6b63c8`), tag 5, then the `u32` mesh bank entry id (`MBANK_ALLMESHES`, numbered as in `Defs/RetailHeaders/meshdata.h`). `Def::graphic` searches a record for that pattern.

Placed things whose definition has no `Graphic` field include animated birds, doors and beds; their graphics are probably elsewhere (another field or an inherited definition).
