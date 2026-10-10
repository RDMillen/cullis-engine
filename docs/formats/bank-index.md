# Bank entry index

Shared by `BBBB` bank files (`.wad`, `.stb`, see [bbbb.md](bbbb.md)) and by each bank inside a `BIGB` archive (see [big.md](big.md)). Parser: `fable_data::bank::EntryIndex`. Every index in the reference install round-trips byte-for-byte (`tests/real_data.rs`).

All integers are little-endian `u32`. Strings are a `u32` length then that many bytes, no terminator; treated as Latin-1.

## Layout

```
u32 type_count
type_count x { u32 entry_type, u32 count }
sum(count) x entry
```

Entries appear in type order: all entries of the first listed type, then the next.

## Entry

| Field | Type | Notes |
| --- | --- | --- |
| marker | u32 | Always `0x2a` |
| id | u32 | 1, 2, 3, … in the entries looked at so far |
| entry_type | u32 | Matches the type table |
| size | u32 | Bytes of entry data |
| offset | u32 | Absolute offset in the containing file; a multiple of the bank's block size (`0x800`) |
| unknown_a | u32 | 0 in `BBBB` banks. In `BIGB` banks, the same value across the first entries of a bank (perhaps a hash); not yet checked across whole banks |
| name | string | `BBBB`: a path such as `Data\Levels\FinalAlbion\LookoutPoint.lev`. `BIGB`: a symbol such as `MESH_OBJECT_STATUE_BEAR` or a bracketed source path |
| unknown_b | u32 | 0 in `BBBB` banks; varies per entry in `BIGB` banks (perhaps a checksum) |
| source_count | u32 | Usually 1 |
| sources | source_count x string | Original tool paths such as `\Dev\BBBProjects\Fable1_5Release\...`. Empty string in the wad; repeats the name in the stb entries looked at |
| info_len | u32 | |
| info | info_len bytes | Type-specific, not yet decoded. Lengths seen in the first entries: wad 88 bytes (three timestamps, see below), stb 16, mesh bank 84, main texture bank 34 |

### Wad info block

Three 7-`u32` timestamps (year, month, day, hour, minute, second, millisecond), all in 2005, then one more `u32`. Which timestamp is which (created, modified, accessed) is unknown.
