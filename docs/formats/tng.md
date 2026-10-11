# Tng thing placement

One `.tng` per map in `Levels/FinalAlbion.wad`, named like the map's Lev. Parser: `fable_data::tng`. All 398 parse: 21,800 things, matching a raw count of `NewThing` lines (`tests/real_data.rs`).

Text in the same `Key value;` format as [wld.md](wld.md), CRLF line endings. Keys may contain brackets and dots (`KeyCameras[0].Position`, `LookDirection.X`); only a trailing `[arg]` is treated as an argument.

## Structure

```
Version 2;
XXXSectionStart NULL;        sections group things; most are NULL
NewThing <kind>;
  fields...
  StartCTC<Component>;       components, never nested
    fields...
  EndCTC<Component>;
EndThing;
XXXSectionEnd;
```

Thing kinds: `Object` (11,964), `Marker` (5,177), `Thing` (3,049), `AICreature` (819), `TrackNode` (319), `Building` (291), `Holy Site` (129), `Village` (51), `Switch` (1). There are 60 component types; the common ones are `CTCEditor`, `CTCPhysicsStandard`, `CTCOwnedEntity`, `CTCDCameraPoint`, `CTCTargeted`, `CTCVillageMember`, `CTCLight`, `CTCEnemy`.

## Common fields

| Field | Example | Notes |
| --- | --- | --- |
| `Player` | `4` | |
| `UID` | `18446741874686296296` | u64 |
| `DefinitionType` | `"BUILDING_TOWN_WAREHOUSE_02"` | A game definition name (see below) |
| `ScriptName` | `MK_OIBH_HERO` or `NULL` | Name scripts refer to |
| `ScriptData` | `"NULL"` | |
| `ThingGamePersistent`, `ThingLevelPersistent` | `TRUE`/`FALSE` | |

`CTCPhysicsStandard` holds `PositionX/Y/Z`, `RHSetForwardX/Y/Z` and `RHSetUpX/Y/Z`.

## Coordinates

Positions are relative to the map's origin (its `.wld` `MapX`, `MapY`), in the same units as the terrain, z up. Checked two ways:

- Across 12,255 buildings and objects, the median distance between a thing's z and the Lev ground height at its x, y is 0.12; 88% are within 2 units.
- Scripted camera points store `CoordBase` as minus (map origin + position), e.g. (-3603.29, -856.69) for a point at (19.29, 88.69) in a map at (3584, 768).

## Definitions

`DefinitionType` names a definition in the compiled defs (`CompiledDefs/game.bin`). `CompiledDefs/names.bin` maps name hashes to names: a header of five `u32` (the third is the count, 13,593), then records of `u32` hash + NUL-terminated name. The hash is the reflected CRC-32 polynomial with initial value 0 and no final inversion (`fable_data::defs::name_hash`). See [defs.md](defs.md) for how definitions lead to meshes.

## Creatures

`AICreature` things (`CREATURE_HERO_FATHER`, `CREATURE_OAKVALE_VILLAGER_MALE_UNEMPLOYED`, chickens, …) keep their placement in `CTCPhysicsNavigator` instead of `CTCPhysicsStandard`, with the same `Position*`, `RHSetForward*` and `RHSetUp*` fields. They also carry `InitialPosX/Y/Z`, in world coordinates (map origin plus position), and behaviour flags (`Health`, `OverridingBrainName`, `CanBeMarried`, …), with components such as `CTCTalk`, `CTCVillageMember` and `CTCEnemy`. Their definitions' `Graphic` is a skinned mesh (value tag 4; [defs.md](defs.md)). Their `ScriptName` names a role for the quest that drives them: in the Prologue's Oakvale every creature but the chickens has one, `NOVI_Villager` for background villagers and a role of its own for each quest character (`NOVI_LiveFather`, `NOVI_Theresa`, `NOVI_AffairWife`, …).

## Sections (quest states)

Things sit between `XXXSectionStart <name>;` and `XXXSectionEnd;`. `NULL` holds what is always there; other sections are states that quests and village events switch on: in StartOakVale `Q_NewOakValeIntro_PreAttack` (villagers, the Hero's father and sister, 9 objects) and `Q__OakValeIntro_PostAttack` (44 objects: the burnt village, charred villagers; the father again), in Oak Bay `Q_CinemaTest` (two bandits), `V_FisticuffsClubOnlyAtNight`, `V_SickChild` and others. Showing every section at once puts states on top of each other (bandits among villagers, intact and burnt objects in one place). The viewer shows `NULL` plus a default per region (the Prologue's Oakvale before the raid) and lets you switch sections on (`--section`, or the side panel).
