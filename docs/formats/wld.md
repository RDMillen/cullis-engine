# Wld world file

`Levels/FinalAlbion.wld`. Parser: `fable_data::wld`.

Text, CRLF line endings. Every non-blank line is one statement, `Key value;` or `Key[arg] value;`. Values are integers, floats, quoted strings, `TRUE`/`FALSE` or bare identifiers. The parser rejects unknown keys so new ones get noticed.

## Top level

```
START_INITIAL_QUESTS;
<QuestName>;            one per line
END_INITIAL_QUESTS;
MapUIDCount <int>;
ThingManagerUIDCount <int>;
NewMap ... EndMap;       repeated
NewRegion ... EndRegion; repeated
```

## Map block

| Key | Value |
| --- | --- |
| `NewMap` | Map number |
| `MapX`, `MapY` | World position (integers) |
| `LevelName` | Lev path relative to `Data\Levels\`, e.g. `"FinalAlbion\LookoutPoint.lev"`. Always present in the wad |
| `LevelScriptName` | Script name; several maps can share one |
| `MapUID` | Unique id |
| `IsSea` | `TRUE`/`FALSE` |
| `LoadedOnPlayerProximity` | `TRUE`/`FALSE` |

## Region block

| Key | Value |
| --- | --- |
| `NewRegion` | Region number |
| `RegionName` | e.g. `"LookoutPoint"`, `"OakBay"`, `"Filler_Snowspire_01"` |
| `NewDisplayName` | Text key such as `"TXT_REGION_LOOKOUT_POINT"`; filler regions repeat their name |
| `RegionDef` | Def name such as `"REGION_LOOKOUT_POINT"`; empty for filler and sea regions |
| `AppearOnWorldMap` | Flag, no value |
| `MiniMapGraphic` | Bare identifier, e.g. `MINIMAP_LOOKOUTPOINT` (optional) |
| `MiniMapScale`, `MiniMapOffsetX/Y`, `WorldMapOffsetX/Y`, `NameGraphicOffsetX/Y` | Floats |
| `MiniMapRegionExitTextOffsetX[<Region>]`, `...Y[<Region>]` | Float; label offset for the exit to `<Region>` |
| `ContainsMap` | Lev path of a map in this region (repeated) |
| `SeesMap` | Lev path of a map outside the region that is visible from it (repeated) |

A map can appear in more than one region, and a region can list a map as both contained and seen. The Prologue's Oakvale is region `StartOakVale` (`REGION_OAK_VALE_INTRO`); the adult Oakvale is `OakBay` (`REGION_OAK_VALE`).
