# World things (`Levels/FinalAlbion.gtg`)

Text, in the Tng format ([tng.md](tng.md)), split into one block per map:

```
NEWMAP <map number in the wld>
Version 2;
XXXSectionStart NULL;
NewThing ...; ... EndThing;
XXXSectionEnd;
ENDMAP
```

151 blocks; many are empty. Parser: `fable_data::gtg`. Positions are relative to the map's origin in the wld, like Tng positions.

Things: `REGION_ENTRANCE_POINT` (187, with a `CTCDRegionEntrance` component: `Active`), `HOLY_SITE_PLAYER_START` (129), `REGION_KICK_TO_POINT`, `MARKER_GLOBAL`, `GLOBAL_TELEPORT_MARKER`, camera points.

## Region exits and entrances

A region exit is a Tng thing (`REGION_EXIT_POINT`) with a `CTCDRegionExit` component: `Active`, `Radius` (2–3), `MessageRadius` (7.5–8), `ReversedOnMiniMap`, `HiddenOnMiniMap` and `EntranceConnectedToUID`.

`EntranceConnectedToUID` names the entrance it leads to:

- bits 40–63: the destination map's `MapUID` in the wld (`0x027a8900_0000006d` → 162441, `LookoutPoint.lev`);
- bits 0–39: the low 40 bits of the entrance thing's `UID` in that map's gtg block (thing UIDs carry `0xfffffe` in their top bits).

The low bits alone are not unique across maps. The destination region is the one whose `ContainsMap` lists the map. All 129 exits in the game's regions resolve to a `REGION_ENTRANCE_POINT` (`tests/real_data.rs`). The 10 exits in `creature_hub.lev` (region `CreatureHub`: no `RegionDef`, an untranslated display name) point at maps that no longer exist.

In the viewer, walking within `Radius` of an exit loads the destination region and puts the Hero at the entrance, facing its forward vector: Oak Bay → Barrow Fields → Haunted House.

Not decoded: what `MessageRadius` shows (probably the "exit to …" prompt), the mini-map flags, `REGION_KICK_TO_POINT`.
