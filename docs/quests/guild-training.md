# The Guild training (quest 028)

The Hero's childhood and apprenticeship at the Heroes' Guild, region `HeroGuildComplexInside` (map origin 4576, 3552), quest state `Q_GuildTraining` (with `Q_GuildTrainingPreMelee`, `_Melee`, `_Will`, `_Departure` for its later stages). Quest code: `engine_script::guild` (`GuildTraining`). Text: `TEXT_QST_028_*` (512 entries) and the cutscene lines `TEXT_CS_028_*`.

## Stages, from the text

1. **Arrival** (`CS_028_PREMELEE_INTRO`). Maze: "I have a new student for you… Put him in the dorm upstairs with the girl." Guildmaster: "Well, follow me then. You don't look much like Hero material to me." He leads the boy to the dormitory: "You'll be sharing this room with Whisper… She's playing in the Woods right now" (`_BOOHOO`), "For now you should get some sleep. Your training starts tomorrow" (`_INTRO_123`).
2. **Morning** (`CS_028_PREMELEE_WAKEUP`). Whisper wakes him ("You must be my room-mate"), leads him to the Map Room ("the Guildmaster is waiting for us"), finds him gone ("looks like the Guildmaster got tired of waiting") and heads for the training grounds across the river.
3. **The dummy** (`CS_028_PREMELEE_PUNCH`, `_STICK`, `_PASSED`; `QST_028_GUILDMASTER_PREMELEE_*`, `QST_028_PREMELEE_INSTRUCTIONS_*`). "Get in the ring… hit that dummy as hard as you can." Punching barely marks it; the Guildmaster hands him a stick. The dummy breaks and drops an experience orb: "Now pick it up."
4. **The alarm** (`CS_028_PREMELEE_ALARM`). Something loose in the Guild Woods (beetles); the Guildmaster waits at the woods' entrance.
5. **Leaving childhood** (`CS_028_PREMELEE_OUTRO`, `QST_028_GUILDMASTER_PREMELEE_END_*`): "Do you wish to leave your childhood behind, and begin Apprentice training?"

The apprentice stages (melee, skill and Will training, grades A+ to F, the departure ceremony) follow.

## Where (quest markers, world coordinates)

| Marker | Where | Use |
| --- | --- | --- |
| `TheRealGuildmaster`, `PreMeleeMaze` | 4646.3, 3670.2 (hall) | the arrival |
| `MK_GTP_BED_GM1`, `MK_GTP_BED_HERO2` | 4671.9, 3676.0 and 4678.2, 3667.9, z 30.1 | the dormitory, upstairs |
| `MK_GTP_END_GM` | 4679.6, 3667.4, z 30.1 | where Whisper wakes him |
| `MK_GTP_LATE_W2`, `MK_GTP_LATE_H2` | 4645.2, 3668.8; 4645.6, 3667.1 (z 21.4) | the Map Room |
| `M_MeleeTeacherStand`, `WhisperMeleeStart` | 4759.6, 3642.6; 4767.5, 3637.9 (z 17) | the training ring |
| `PreMeleeDummy` (`OBJECT_STRAW_DUMMY_01`) | 4769.7, 3640.1 | the dummy |
| `CAM_GTP_*` | | the stage's camera points (bed shots, the dummy, the experience orb) |

## Implemented

The quest plays stages 1–3 up to the Guildmaster's punch instructions: lines in order, the Guildmaster walking off towards the dormitory, the night passing (a fade, the clock to 07:00, the Hero placed by his bed, the Guildmaster moved to the ring and Whisper beside the bed), Whisper's wake-up lines and her walk to the Map Room, then to the ring. Quest beats `Walk`, `Place` and `Sleep` were added to the quest API; people walk by route and goals carry a floor height where floors are stacked.

## Not yet

- **The dormitory floor is not walkable.** The mesh has the floor at 30.1 m, but no layer of the decoded navigation data covers it, so neither the Guildmaster nor the Hero can walk up to it and the sequence stops at "follow me". Routes reach the stairs (about 27 m on layer 1).
- **No route leaves the Map Room.** Routes from the Map Room marker (4645.6, 3667.1, z 21.4) fail even 10 m east, with or without a goal height; walking straight east climbs onto the edge of a staircase (z 23–25 on navigation layer 1). The hall's walkable cells seem not to connect to the rest of the building, likely the same navigation gap as the dormitory floor. Routes around the training ring work. A goal given without a height goes on the first layer walkable there, which inside the Guild is the ground under the building.
- Closed doors no longer block routes: routes go through them and whoever follows a route opens a closed door in their way (`World::follow_route`, creatures' errands).
- The dummy lesson itself (targeting, punching, the stick, the dummy breaking, experience orbs), the beetles and the end question.
