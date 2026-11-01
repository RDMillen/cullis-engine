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

## Walking in the Guild

Fixed 2026-10-31. Both problems below came from two causes: the Guild's wings and the hall's floors are buildings created by `CREATEBUILDING` dummies, which were not placed ([mesh.md](../formats/mesh.md)), and the dormitory's navigation layer started from the ground floor under it ([nav.md](../formats/nav.md)). Every navigation cell of the Guild now has a height. Headless walks (`--walk-to`) that now reach their goals: from the hall (`TheRealGuildmaster`) to `MK_GTP_BED_GM1` and `MK_GTP_BED_HERO2` (layer 2, 30.1 m); from `MK_GTP_END_GM` down to the Map Room (`MK_GTP_LATE_W2`); from the Map Room east and out to `M_MeleeTeacherStand`.

- Was: the dormitory floor not walkable (no mesh floor at the bedsides, the sequence stopped at "follow me").
- Was: no route leaving the Map Room (the hall's floor at 23.1 m came from the entrance hall's created parts).

## Into the training ring

The ring's fence (`OBJECT_GUILD_WALL_*`) has a gate, `OBJECT_OAKVALE_FENCEGATE_02` at (4761.7, 3644.3): a plain object (no `CTCDoor`, no script name) whose definition has `CActionUseDef`, and whose mesh is one static leaf 2.76 m wide hinged at its origin. The navigation inside and outside the fence meets only there and no cells cross it, so nothing can walk in. In the original the Hero does not walk through it either (observed by Ross): after "Get in the ring" (`TEXT_CS_028_PREMELEE_PUNCH_10`) the cutscene puts him inside. The quest does the same with `Beat::Cut`, to `MK_GTP_HERO2` (4767.7, 3640.8), 2 m from the dummy, facing it; `MK_GTP_HERO1` (4760.2, 3644.1), outside by the Guildmaster, is his spot before.

Whisper waits outside beside the Guildmaster, at the ring's `PreMeleeChatMarker` (4758.2, 3644.1), a guess. `WhisperMeleeStart`, inside the ring (in `Q_GuildTraining`, beside `HeroMeleeStart`), is probably for the apprentice melee lessons, whose markers include `MK_GTM_WHISPERGATE` and `MK_GTM_HEROGATE` just inside the gate; how they get in then is not observed.

## Unarmed fighting

What the dummy lesson's first part needs is in (`engine_world::combat`): F strikes, and pressing it again during a strike chains the next; Q locks on to the nearest foe in front (a dummy or a person), the Hero then facing it and stepping in his stance (`STRAFE_IDLE`, `STRAFE_MOVEMENT_FORWARD`, `_BACK`, `_LEFT`, `_RIGHT`, with the `STAND` modifier). The Guild's 13 dummies (`OBJECT_STRAW_DUMMY_01`, the lesson's `PreMeleeDummy` among them) are world entities: a blow reaching one plays its `GET_HIT` (the last of a chain its `GET_HIT_SPIN`), then `WOBBLE`, then its still `DEFAULT`; they count their hits and stop the Hero like people. People struck flinch and the quest hears of it, as the kick did. Checked headless (`--strike 3`): from 1 m, a chain of three lands three blows and the dummy spins; from the cutscene's spot (`MK_GTP_HERO2`, 2.1 m) the first punch falls short.

Guesses, to check against the original:

- **The chains.** Two chains of three punches (`ANIM_HERO_UNARMED_STRIKE_RIGHT_2_1` to `_2_3`, then the `LEFT` ones; chains take turns). The names suggest the order; the table also has eight kicks and turning variants (`C90`, `AC90`, `C180`, `AC180`), probably for striking a foe to the side or behind, not used yet.
- **Reach and lock.** A blow lands on a foe whose edge is within 1 m of the Hero and within 60 degrees of straight ahead; a lock reaches 10 m, preferring foes in front, and is lost at 15 m. The lock key (Q) is ours: the game's PC binding (`GAME_ACTION_LOCK_TARGET`) is not known.
- **The dummy's reactions.** Which blows spin it, and that it wobbles after each.

## Punching the dummy

After the cut into the ring and "Just keep going 'til I tell you to stop", the punch instructions show (`TEXT_QST_028_PREMELEE_INSTRUCTIONS_PUNCH_PC`, with `[GAME_ACTION_LOCK_TARGET]` and `[GAME_ACTION_ATTACK]` filled in as Q and F). The quest counts blows on the lesson's dummy (`PreMeleeDummy`, by script name; the other dummies do not count): after 5 the Guildmaster says "Not making much of an impact there, are you? Here, try with this." (`TEXT_CS_028_PREMELEE_STICK_10`, `_20`). With no punch for 20 s he urges "Come on, boy, you can hit it. Walk right up to it and give it a thump." (`TEXT_QST_028_GUILDMASTER_PREMELEE_HIT_DUMMY`). Both numbers are guesses. Checked in the window (`--guild-at-ring`).

## Not yet

- Not yet watched in the window: the Guildmaster leading the Hero up to the dormitory, the morning walk to the Map Room, and the cut into the ring, as quest beats.
- Closed doors no longer block routes: routes go through them and whoever follows a route opens a closed door in their way (`World::follow_route`, creatures' errands).
- The rest of the dummy lesson: the stick (`STANDARD_UNSHEATHE_STICK`; its strikes are probably the `SWORD` set; its instructions `TEXT_QST_028_PREMELEE_INSTRUCTIONS_STICK_PC` and nags `_STICK_USAGE`, `_STICK_REPEAT`), the dummy breaking (`TEXT_CS_028_PREMELEE_PASSED`) and its experience orb (`_INSTRUCTIONS_XP_PC`, `_XP_REPEAT`). Then the beetles and the end question.
