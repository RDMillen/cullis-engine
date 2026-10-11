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

Not decoded in general. Records hold fields as a name hash, a value type tag and a value. A mesh reference is `Graphic` (hash `0x2e6b63c8`), tag 5 (static meshes) or 4 (skinned meshes: creatures, beds, doors), then the `u32` mesh bank entry id (`MBANK_ALLMESHES`, numbered as in `Defs/RetailHeaders/meshdata.h`). `Def::graphic` searches a record for that pattern.

Doors and beds, which seemed to have no `Graphic`, have one with tag 4 (a skinned mesh: `OBJECT_KHG_BED_01` → `MESH_KHG_BED_01`).

Entity definitions (`BUILDING`, `CREATURE`, `OBJECT`, …) list components as (component type name hash, `u32` global definition index, `u32`): `BUILDING_OAKVALE_HOUSE_LARGE` has `CCameraCollisionDef`, `CAppearanceDef`, `CVillageMemberDef`, `CPhysicsDef`, `CSmokeGeneratorDef`, `CHitLocationsDef`, `CMultiStaticMeshDef` and `CBuyableHouseDef`; `CREATURE_HERO_CHILD`'s `CAppearanceDef` holds its animation slots. `Def::component` finds one.

A building's `CMultiStaticMeshDef` lists its meshes as `Mesh` fields (hash `0x60194a74`) followed by the mesh id and flag fields: field `0x7ca90715` (a byte) is 1 on the exterior, field `0x97595fc1` is 1 on the interior, and field `0x15dc93e9` (a float) is 40.0 on the interior and 0 on the exterior (meaning unknown). `MESH_OAKVALE_HOUSE_INSIDE_03` is (0, 1, 40.0), `MESH_OAKVALE_HOUSE_03` (1, 0, 0); the tavern's pair is the same. `Def::static_mesh_entries` reads them; the viewer hides the exterior's roof materials (named `Roof`, `roof`, `Roof fringe`, `Fringe`) while the Hero is inside the building; the exterior walls, which carry the door frames, stay. The `Graphic` field names only the exterior; the interior (floors, stairs, inner walls) comes from this list (`Defs::meshes_of`). Every Oakvale building has one, e.g. the tavern's `MESH_OAKVALE_TAVERN_INSIDE_01`.

## Terrain themes

`ENGINE_THEME` records are a 5-byte header (`01 00 01 00 00`) then pairs of (`u32` field hash, `u32` value). Texture fields hold `GBANK_MAIN_PC` ids:

| Field | Example (`GROUND_GRASS_GREEN`) |
| --- | --- |
| `BaseTexture` | `UNASSIGNED_GRASS_PLAIN` (flat ground) |
| `0xf6ff82d3` (name unknown) | same id as `BaseTexture` in the themes seen |
| `0x7a1c6732` (name unknown) | `UNASSIGNED_CLIFF_LAYERED_01` (steep slopes) |
| `0x99b491a3` (name unknown) | same id as the cliff texture in the themes seen |
| `BaseBumpMap` | 0, or e.g. `UNASSIGNED_SAND_FINE_BUMP` |
| `CliffBumpMap` | e.g. `UNASSIGNED_CLIFF_LAYERED_01_BUMP` |

Of the 344 theme names used by Lev files, 343 have a definition with a base texture; `SLIDING_BLOCK_ROCKS_ET` has no definition. `Def::theme_textures` reads these fields.

Water themes (`WATER_GREYCLIFF_ET`, `WATER_BWLAKE_*`, …) have a water texture as their base (`LANDSCAPE_WATER`, opaque teal). In StartOakValeWest they cover thin strips beside paths and on slopes at full weight, where the game shows no water (Ross, comparing). They probably mark ground meant to lie under a water surface, likely described by the stb sea banks, which are not decoded. The viewer draws them with their cliff texture until then.

Lake themes (`WATER_BWLAKE_N`, `WATER_WWLAKE_N`, `WATER_KRAKEN_LAKE_N`, …) also have `WaterType` = 1 and `WaterHeight` = N as a float (`WATER_BWLAKE_4` has 4.0, `_16` has 16.0); `WATER_GREYCLIFF_ET` has neither. `WaterHeight` is not a world height: lake themes cover whole background hillsides (up to 160 m) in filler maps, and in StartOakValeWest `WATER_BWLAKE_2` covers 21 points at x 3541–3544, y 901–907 where the ground is 12.5–12.9 m. Water surfaces are therefore described somewhere else (candidates: the stb sea banks, or the Lev sections after the grid). Field names found by hashing: `WaterHeight`, `WaterType`, `MaterialDef`.

## Creatures

A creature's `Graphic` (tag 4) is its whole body for single-mesh creatures (`CREATURE_HERO_FATHER` → `MESH_HERO_FATHER`, chickens, `CREATURE_YOUNG_SISTER`). Built-up creatures (villagers, guards, children, traders) name an "unclothed" base (`MESH_BS_MALE_MIDDLE_UNCLOTHED_01`); their `CCreatureDef` component holds three part lists, heads, torsos and legs, each a `u32` count followed by options:

| Field | Type | Notes |
|---|---|---|
| mesh | u32 | skinned mesh (`MESH_BS_MALE_MIDDLE_HEAD_01`, `…_TORSO_02`, `…_LEGS_02`) |
| | u32 | 1 |
| swap count n | u32 | |
| | u32 | 12n |
| swaps | n x (u32, u32, u32) | texture the mesh names, texture to use, 0 (`TEXTURE_BOWER_MALE_MIDDLE_FACE_01` → `TEXTURE_OV_MALE_MIDDLE_FACE_01` for Oakvale) |
| | u8 | 0 or 1 |

`CREATURE_OAKVALE_VILLAGER_MALE_UNEMPLOYED` has 8 heads, 4 torsos and 4 legs. The lists are found by this layout (`Def::part_lists`, `Defs::creature_parts`); the fields around them are not decoded, nor how the game picks an option (presumably at random).

## Animation tables

A creature's `CAppearanceDef` (`CREATURE_HERO_CHILD`'s has 871 entries, a villager's 792, a chicken's 25) maps animation slots to animations. Each entry: `u32` slot name hash, `u32` `MBANK_ALLMESHES` id of the animation, `u32` the slot's name (a `names.bin` offset), `u32` a modifier's name offset or `0xffffffff`. Entries are found by that layout, checking that the name at the offset hashes to the hash before it (`Def::animation_slots`, `Defs::animation_slots`); the bytes between entries are not decoded.

| Slot | Modifier | Animation (young Hero) |
|---|---|---|
| `ST_IDLE_SUBTLE` | `STAND` | `ANIM_HERO_IDLE` (the breathing loop) |
| `ST_IDLE` | `STAND` | `ANIM_YOUNGHERO_SCRATCH_EAR`, `…_KICK_FLOOR`, `…_LOOK_BACK`, `…_SCRATCH_ASS` (fidgets) |
| `ST_WALK`, `ST_RUN`, `ST_SLOW_WALK` | `STAND` | `ANIM_HERO_WALK`, `ANIM_HERO_RUN`, `ANIM_HERO_SLOW_WALK` |
| `ST_IDLE_SUBTLE` | `SWORD`, `CROSSBOW`, `TOO_HEAVY`, `DRAINED`, … | situation variants |

Villagers: `ST_IDLE_SUBTLE` → `ANIM_VILLAGER_MALE_IDLE_BREATHE_01`, `ST_IDLE` → gazing at the sky, scratching the head. Other slots cover combat, emotes (`ST_THANKS`), cutscenes (`CS_SHIFT_WEIGHT3` → `ANIM_HERO_IDLE_3`), objects (`LIGHT_LAMP` with `TABLELIGHT`, `CUPBOARDLIGHT`). How the game chooses among several entries for one slot is not known.

## Skeletal morphs

`CSkeletalMorphDef`: field `0xcbf27745`, a `u32` count, then that many `names.bin` offsets naming bone config files (`hero_young_set.bncfg`, `young_sister_set.bncfg`, a guard's; [bncfg.md](bncfg.md)). `Def::skeletal_morphs`, `Defs::skeletal_morphs`.
