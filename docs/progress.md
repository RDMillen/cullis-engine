# Progress log

Newest first. One entry per session: what changed, what was learned, what is next.

## 2026-10-17: brighter light

- Ross found all the lighting too dark, even at noon. With the table's colours added and capped at 1, a sunlit surface showed its bare texture and everything facing away dropped to the ambient level (about 60% at noon). The light is now scaled by a brightness factor before going linear, so it can go past 1 (overbright, as was common in engines of the time; the dim noon ambient suggests it): 1.6 by default, by eye, with a slider in the Time of day panel and `--brightness` for screenshots. Measured on a noon screenshot, walls went from 137 to about 200 (of 255) and ground from 54 to about 90. Interior window light went back to half strength, since the factor lifts it too.
- Note for screenshot checks: ffmpeg's side-by-side images can look the same when the inputs differ; compare single images or measure pixels.

## 2026-10-17: light inside houses

- Ross found house interiors too dark. The indoor theme (`ENVIRONMENT_INTERNALS`) has no sun and a dim ambient (about 30% grey). Its row 7, near white and filled in every theme, now lights interiors at 65% from high up on the sun's side, standing for daylight through windows and doors (`rows::KEY`). Checked in headless screenshots inside a house at noon and at night; the indoor theme barely changes through the day, so nights indoors stay lit (fires and candles are not placed lights yet).

## 2026-10-16 (later): lighting

- Light comes from environment themes (`ENVIRONMENT_THEME_DAY` definitions) and `LightingTable/lighting_colours.tga`, now read by `fable_data::environment` (`docs/formats/environment.md`). A region names its outdoor and indoor themes; a theme is keyframes by hour, each with sky textures, plus the first column of its run in the colour table, one column per keyframe and 21 colour rows. All 39 themes parse, and every theme's run is exactly as long as its keyframes (real-data test).
- Field names found by hashing guesses with the definitions' CRC-32: `EnvironmentTheme`, `InternalEnvironmentTheme`, `Time`, `TimeOfDay`, `SkyTexture0/1`, `SunHeight`, `MoonHeight`, `MistAlpha`, `RainStrength`, `SnowStrength`, `LightningFrequency`, `ShadowFactor`, `GlowBloomScale`. The table's rows are not named; from how they change through the day, row 0 is taken as ambient, row 8 as the sun and row 9 as the sky's tint.
- The viewer lights terrain and models with ambient plus sun (added in gamma space, as fixed-function lighting did), the sun moving east to west through the day. A sky pass draws the theme's sky textures (panoramas, blended between keyframes and tinted) behind everything. Inside a building the region's internal theme lights it. Oakvale after the raid uses `ENVIRONMENT_OV_POSTATTACK` (red and orange). The window has a time-of-day slider and a clock (a day in 24 minutes, a choice); `--hour` sets it for screenshots. Model lighting now turns the sun into each model's own space (it was used in world space before).
- Checked in headless screenshots of Oakvale at 06:00, 10:00, 14:00, 19:00 and 23:00, after the raid and inside the Guild.
- Not yet: clouds (two cloud textures per keyframe), placed lights (torches, `CLightDef`), the meaning of most table rows, and how the game maps its sky textures. After the raid the sky is nearly black: a dark texture under an orange tint, so the colour probably comes from the clouds.
- Next: compare with the original at a few times of day; then clouds and placed lights.

## 2026-10-16 (later): lip sync

- Ross asked whether mouths can move properly. They can: every voice sound has a lip sync track in `lang/English/dialogue.big` (`fable_data::lipsync`, documented in `docs/formats/lut.md`): mouth shapes (`AH`, `EE`, `OH`, `SZ`) with a weight each, 43 frames a second. Every track parses and lasts as long as its sound (real-data test).
- The shapes are single-frame animations, `ANIM_<set>_PHONEME_<shape>`, posing the face bones below `Bip01 Head` (chin, lips, mouth corners, cheeks, brows, eyelids). The animator gained a face layer: those bones take the weighted average of the body's pose and the shapes'. Creatures load their set on first use: the boy's for children, the bandit's for guards and bandits, else the villagers' (male or female) - a guess from the sets' names.
- While a voiced line plays, the speaker's mouth follows its track, blending between frames; it is released when the line or the track ends.
- Checked: headless close-ups of the father with the new `--mouth NOVI_LiveFather:<shape>` option show four distinct mouths (MM closed, AH open, OH round, EE wide).
- Next: lighting (Ross).

## 2026-10-16 (later): the interaction highlight

- Whoever E would talk to (someone in reach the current quest has something for, `Story::knows`), else the door E would use, glows green and gently pulses; nothing during a conversation or a quest's closing scene. The model shader adds a rim glow (strongest where the surface turns away from the camera) and a faint tint, from a per-model highlight colour and the camera's position in the model's space (`ModelRenderer::prepare_placed`). Strength set by eye after a first try that tinted the whole figure.
- Checked in the window: walking up to the father, he glows at his edges.
- Not yet: the original's red highlight on someone about to be hit (there is no targeting yet).
- That completes issue #1's three items (gestures, the conversation camera, the highlight). Next: lighting, the issue's most visible gap.

## 2026-10-16 (later): the conversation camera

- While a conversation line or question is on screen, the view eases (half a second) from the follow camera into a framed shot, letterbox bars slide in, the HUD hides, and the speaker and the Hero turn to face each other; afterwards it eases back. Module `apps/viewer/src/conversation.rs`.
- Framings come from the lines' `CAM:` tags where they have them (18 lines): `(SPEAKER,IN_FRONT_OF_FACE)(SPEAKER,FOCUS_FACE)` (a close-up), `(SPEAKER,BEHIND_RIGHT_SHOULDER_NEAR)(SPEAKEE,FOCUS_FACE)` (over the speaker's shoulder onto the Hero), `(SPEAKEE,IN_FRONT_OF_FACE)(SPEAKEE,FOCUS_FACE)`. Other lines use over the Hero's right shoulder onto the speaker. Distances and face heights (adults 1.6, children 1.05) are set by eye, to compare with the original.
- Creatures can turn (`Npc::turn_towards`; their heading is kept from placement), which also stops a stroll.
- Checked in the window: talking to the father frames him over the Hero's shoulder, facing the Hero, mid-gesture, between the bars, with the subtitle in the lower bar.
- Ross: the Hero should not move during these scenes. Now, while a conversation line or question is on screen, or lines lead up to a quest's end (Theresa's raid lines, Maze's rescue), WASD, the walk to a point and the kick are ignored; E and Y/N still work (`Subtitles::in_scene`). Unit-tested; key presses could not be sent to the window here to try it.
- Next: the interaction highlight (the last of issue #1's items).

## 2026-10-16 (later): gestures while talking

- Lines carry delivery tags (now in `docs/formats/strings.md`): `CONVERSATION_ATTITUDE_<mood>` on about 1,000 spoken lines, `ANIM:<slot>` naming an animation to play, and `CAM:(…)(…)` camera framing (for the conversation camera next). Creatures' tables have `CONV_GENERAL` gestures per mood modifier (`CONVERSATION_HAPPY`, `_ANGRY`, `_SAD`, `_FEAR`, `_BELLIGERENT`; four each for villagers).
- Viewer: whoever says the line on screen gestures while saying it: the slot the line names, else a random `CONV_GENERAL` gesture for its mood (untagged lines, most of them, count as happy: a guess), another as each ends, until the line has been said (its voice's length, or a reading time). Gestures load on first use from one animation bank; the speaker then returns to their idle. Creatures keep their whole animation table for this.
- Checked: headless screenshots with the new `--gesture NOVI_LiveFather` option show the father raising his arm through the gesture and back in his idle by 6 s; in the window, the log showed him playing a happy gesture as his first line started (the scene is too dark to see it there).
- Not yet: speakers turning to face the Hero; the Hero's own reactions.
- Next: the conversation camera with letterbox bars (using the `CAM:` tags where lines have them).

## 2026-10-16: feedback from issue #1

- Issue #1 (Romane Donnet): CullisEngine builds unchanged on macOS on Apple Silicon (Metal), and every test passes against the Steam release, so the parsers handle it and an ARM CPU. Their notes taken up:
  - Volume: the viewer's side panel has a "Sound" section with overall, voice, music and ambience sliders (`Audio::set_volume`, `set_channel_volume`). Music's default is down from 35% to 25%, since it felt loud.
  - The mixer had a hard clip at full scale; it now has a soft limiter (unchanged up to 0.75, then a scaled `tanh` that never exceeds full scale).
  - README: `FABLE_DIR` is the install folder, not its `data` folder; the Steam release works; SteamCMD on macOS (quarantine, Windows depot); platforms tested.
  - Roadmap (PLAN.md, phase 4): the conversation camera with letterbox bars, speakers gesturing while they talk, and the interaction highlight. Checked: the `highlight` and `outline` the report found in the code are tutorial text names and navigation outlines, so there is no highlight yet.
- Checked in the window: the Sound section's sliders, with the new defaults. Clicks sent with xdotool do not reach the window (pointer movement does), so the sliders were captured with the section opened by default in a test build.

## 2026-10-13 (later): music and ambience

- `Defs/atmos_types.h` (a C header shipped with the game) numbers the music sets and atmospheres; `fable_data::atmos` reads it. Region definitions hold their music set after field hash `0xcab1fa53` (found by matching regions to their music: 34 of 47); sound themes their atmosphere after `0xe001b756`. Oakvale: `MUSIC_SET_OAKVALE` → `Sound/OAKVALE.OGG`.
- The Lev sound map's block value counts from 1 into the map's sound theme names: Oakvale's village is `SOUND_THEME_COUNTRY`, its north edge `SOUND_THEME_SHORE` and `SOUND_THEME_OCEAN`, the memorial garden's stream `SOUND_THEME_RIVER`.
- `fable_data::lug` reads the ambient banks: `.met` lists the sounds (each a complete WAVE file in the `.lug`, Xbox ADPCM, matching ffmpeg) and playback records (sound, day or night group, looping bed or one-shot, volume, a frequency weight). All 50 banks parse and every sound decodes (real-data test). First tries got this wrong twice: the `.lug` sounds are WAVE files, not raw ADPCM, and playback records are not one per sound and carry names.
- `engine-audio`: music and ambience channels (any number at once), per-sound volume, pan and looping, fades, and Ogg Vorbis decoding (`lewton`, pure Rust). The viewer loops the region's music (crossfading over 2 s) and plays the ambience of the block under the Hero: its beds, and day spot sounds now and then, picked by weight and panned at random (a stand-in; what the unknown fields mean is not known). Changes of ambience fade over 1.5 s once wanted for 0.75 s.
- Checked: walking from the father to Theresa, the ambience goes country → shore → country → shore → ocean; the output (recorded from the sound card's monitor) never clips; levels follow the music and the ambience.
- Also found: `Defs/*dialoguesnds*.bin` map 32-bit keys to voice sound ids, but no hash of names, text or ids tried matches the keys (see `lut.md`); the voice alignment stays.
- Not yet: night sounds, positioned sounds (fountains, the windmill), danger music, the raid's ambience (`SOUND_THEME_OAKVALE_RAID` is in the theme list but no block uses it).
- Next: Ross to listen and judge the mix; then the Guild training or the raid's look.

## 2026-10-13: voices

- `fable_data::lut` reads the voice banks (`lang/English/*.lut`: chunks, lookup table, 36-byte sound headers, RIFF WAVE files) and `fable_data::adpcm` decodes their Xbox ADPCM (64 samples per 36-byte block: the header sample and 63 codes). Checked sample for sample against ffmpeg's `adpcm_ima_xbox`; decoded lengths match the lipsync durations in `dialogue.big`. Format page: `docs/formats/lut.md`.
- Which line is which sound is not in the data. Measured instead: within a bank, sounds follow the spoken lines in case-insensitive name order, with gaps where lines were added or cut. `fable_data::voices` aligns the two lists by how well each line's length fits a sound's duration (banded dynamic programming). 17,403 lines voiced (Dialogue and ScriptDialogue); about 87% fit their sound against about 52% for a deliberately wrong pairing. `ScriptDialogue2` (The Lost Chapters) follows neither name nor id order and is left unvoiced.
- Ruled out on the way: the text entry's index info (it is the speaker's number), the lipsync headers (they only number sounds), the sound headers (identical apart from the id), text id order.
- New crate `engine-audio`: a mixer with no platform dependency (one voice at a time, linear resampling, no allocation while mixing). The viewer plays it through cpal, preferring PulseAudio (ALSA's PulseAudio plugin underran), and speaks each line as its subtitle appears, keeping remarks up while they are said.
- Checked: the father's first line, recorded from the sound card's monitor, matches the decoded sound (correlation 0.87, against 0.10 for another sound). Key presses sent with xdotool no longer reach the window on this display (mouse events do), so later lines were not checked in the window; the queue's voicing is unit-tested.
- New examples: `big_entry`, `voice_banks`, `voice_map`, `voice_align`, `voice_wav`.
- Next: Ross to listen (are the right lines spoken by the right voices?); ScriptDialogue2's order; ambient sound banks (`Sound/*.lug`, `*.met`) and music (`Sound/*.ogg`).

## 2026-10-12 (later): voices, first look (unfinished)

- `lang/English/*.lut` (Dialogue, Dialogue2, ScriptDialogue, ScriptDialogue2) are chunked audio banks. Each chunk is a 32-byte zero-padded name then a u32 size:
  - `LiOnHeAdLHAudioBankCompData` (at 0): the sounds back to back, each a 36-byte header then a complete RIFF WAVE file. Header: u32 sound id, u16 1, u16 22050, u32 (40000 or 105536), bytes 01 01 06 xx, u16 100, f32, f32, u32 (500 or 1000), i32 -1; meanings not yet known.
  - The WAVE files use format tag 0x69 (Xbox ADPCM): mono, 22,050 Hz, 4-bit, 36-byte blocks of 64 samples (fmt extra = 64).
  - `LHAudioBankLookupTable`: u32 (1000 or 500), u32 count, then count records of (u32 sound id, u32 length, u32 offset into the data chunk). ScriptDialogue: 5,310 sounds with ids 1..5310.
  - `LHFileSegmentBankInfo` (0x20c bytes): "\Generated bank for Fable", "NoFPNameYet", then u32 4.
- Text entries name only their voice bank (`ScriptDialogue.lug`), and their ids (such as 8828) are not sound ids. `Defs/RetailHeaders/*_lipsync.h` just number the sounds (`ScriptDialogue_1 = 1`). `lang/English/dialogue.big` has lipsync banks (`LIPSYNC_ENGLISH_MAIN`, `_SCRIPT`, ...), one entry per sound (`ScriptDialogue_1`) with a speaker source (`SPEAKER_FEMALE1`) and a 4-byte float info (probably the duration). The link from a text line to its sound id is not found yet.
- New examples: `text_ids` (text entries with their ids and voice banks), `big_list` (a BIG archive's banks and entries).
- Next: find the text-to-sound link (look inside the lipsync entries' data, and the text bank's entries for a sound index); write the `.lut` parser and an Xbox ADPCM decoder, checked against ffmpeg's decoding of the same RIFF files; then an `engine-audio` crate and playback in the viewer.

## 2026-10-12 (later): routes on foot

- The stop at y≈908 north of the village centre is a fence: the game's navigation leaves the cells beyond it unwalkable (a rail at 14.3 above ground at 13.0). It only stopped straight-line test walks; Theresa is reachable round it, over the covered bridge.
- `walk::route`: A* over the walk cells of every layer, with the walking step rule (each move within 0.6 of the last, no corner cutting past unwalkable cells, layers joined where their surfaces meet). It avoids closed doors and people, keeping a margin round them, and ends beside a goal that someone is standing on. `walk::steer` aims at the farthest next point in clear sight; a point counts as reached only at the Hero's height (within 0.2), so a steep climb is not skipped. `walk::Follow` plans the route and plans again from where the Hero is after a second without progress.
- `--walk-to` follows routes, headless and in the window. From the father, all nine Prologue characters are reached on foot (Theresa, the Trader, the bully, the teddy girl, the warehouse worker and the thug, the husband, the wife, a guard); in the window the Hero walked himself to Theresa and her introduction started.
- Fixed on the way: walks grazed people the route passed close by, and the follower dropped route points on a ramp he had not climbed yet.
- Next: voices (`Dialogue.lut`), the Guild training, or the raid's look.

## 2026-10-12 (later): stairs, and the README

- Ross saw the Hero catch on house steps in Oakvale. Cause: walk cells were 0.5 units, wider than a stair tread, so a cell could skip a tread and sit two risers (about 0.65) above its neighbour, over the 0.6 step limit. Walk cells are now 0.25 units, and each cell samples surfaces at its centre and four nearby points (a ray at the centre alone slipped through a seam in one step's mesh, leaving a cell at ground height). Walking straight up and down the Hero's house steps on three lines: the old grid stopped partway on three of six walks, the new one on none. Oakvale still loads in about 2 s.
- New diagnostic `--ledges`: prints walk-grid ledges (neighbouring walkable cells more than a step apart), and with `--camera` the cell heights and surfaces around that point. The remaining small ledges in Oakvale are straight platform edges at one height (an upper floor at 3486, 829; a dock at 3600, 816), left as they are.
- README restructured: status as a list, the crates, and the viewer's keys and options as tables.

## 2026-10-12 (later): the end of the Prologue

- `engine-script`: `prologue::Aftermath` for the after-the-raid state: reaching the dying father plays his moan and the rescue cutscene's lines (a bandit, then Maze), then a new `Beat::Travel` takes the Hero to the Heroes' Guild (`HeroGuildComplexInside`, `Q_GuildTraining`), arriving facing the Guildmaster and Maze. The birthday ending now includes the raid cutscene's lines (`CS_048_OIF`). A quest can pose people (`Quest::mood`: an animation table modifier).
- Learned: the father after the raid is `OVI_DeadFather` with `Health 12`; his table's `ST_IDLE_SUBTLE` has a `MORTALLY_WOUNDED` variant (`CS_GUILDMASTER_DYING_CYCLE`, a kneeling dying loop). The Guild's `Q_GuildTraining` state places the Guildmaster and Maze together in the hall.
- Viewer: creatures carry their idle's modifier variants and can be given one (`Npc::set_mood`); screenshot mode now places the region's quest too.
- Walking: floors raised over the terrain with no step up from open ground (the Guild hall, 1.3 above the terrain) were walked under; the ground layer now seeds those cells from the floor itself. Spawning picks the walk layer nearest a given height (arrivals and region exits). Oakvale walks are unchanged.
- Checked in the window: the father kneeling, the rescue lines, the Guild loading and the Hero arriving on the hall floor facing the Guildmaster and Maze.
- Known: walking straight north from the village centre stops at y≈908 (before this change too).
- Next: the Guild training (`Q_GuildTraining`), or the raid's look (night, fires).

## 2026-10-12 (later): the first Prologue quest

- New crate `engine-script` (no rendering dependency): the quest API (events in: talk, hit, answer, time; `Beat`s out: say, ask, notice, leave, return, finish) and the Prologue birthday quest (`prologue::Birthday`), written from how the Prologue plays. Expected behaviour and the values not yet checked against the game: `docs/quests/prologue-birthday.md`.
- Viewer: the quest runs in `StartOakVale`. E talks (and goes on), F kicks, Y/N answer questions. Subtitles wait for E; remarks (a kicked bully, the thug's taunts) time out; deed notices show at the top; gold, items and deeds in the top right. People the story sends away vanish and come back. Giving Theresa the sweets reloads Oakvale in the after-the-raid quest state, the Hero where he stood.
- The boy Hero kicks (`ANIM_HERO_UNARMED_KICK_RIGHT_2_1`, from his `STRIKE_MEDIUM_FRONT` slot); creatures flinch with their `RESPONSE_GENERIC_HIT_RIGHT` animation.
- Fixed: a clip played once never counted as finished (its time holds at the last frame, short of the clip's length).
- Tests: the quest's paths (good, bad, leaving the stock, dropped questions); an ignored real-data test plays 2000 random event sequences and checks every text entry the quest names is reached and exists. It found a line that could never be said (the victim's thanks after being hit).
- Checked in the window, driven with xdotool: the father's speech and tutorial notices; three kicks drive the bully off (good deed, the victim hands over Rosie); the Trader's question with Y; the ending with `--with-present` (a testing option: start with the sweets bought).
- Not done: smashing barrels, people walking where the story sends them, cutscenes, voices, the raid's look (the post-attack state renders in daylight).
- Next: compare with the game (timings and amounts above), then the rest of the Prologue (the raid and the escape).

## 2026-10-12: talking to people

- Viewer: **E** near a Prologue character starts a conversation, shown as a subtitle (speaker and line; E for the next line, walking away ends it). E still uses doors when no one is in reach. `--talk N` prints what the nearest character says N times (screenshot mode) or starts a conversation once `--walk-to` arrives (window).
- What each role (`ScriptName`) says is a table in `apps/viewer/src/talk.rs`, written from how the Prologue plays: the father's birthday speech (`TEXT_QST_048_DAD_THERESA_BIRTHDAY_INTRO`, four lines), Theresa's introduction, the bully, the victim, the teddy girl, the barrel man and thug, the trader, the affair trio, guards and villagers (`TEXT_AI_OPINION_ACT_GREET_TO_HERO_ASBOY`). Talking again gives a short follow-up where the text has one. All 13 roles placed in `StartOakVale`'s Prologue state are covered; an ignored real-data test checks every line exists.
- Learned: generic lines are spoken by voice sets (speaker codes such as `LEV1BDT1`, `CM1`), so they name no character; the viewer names those speakers by role ("Villager"). A group of alternatives mixes voices, so a villager may answer in a voice that doesn't match them.
- Not yet: voice audio (`.lut`), choices (`[Unspoken]`/`[NONE]` question lines such as "Give the chocolates to your sister?"), and the quest state that changes what people say.
- Next: the first Prologue quest (good deeds for the father's gold, the trader's chocolates, Theresa), which needs quest state, gold and a choice prompt.

## 2026-10-10 (later): textured terrain

- Grid point bytes 10–14 are two theme slots and their weights (plus a third slot); themes are `ENGINE_THEME` definitions naming a base (flat) and a cliff texture (`Def::theme_textures`). Every grid point uses a listed slot; 343 of 344 themes have textures.
- The viewer textures terrain: a region-wide grid texture of (layer, layer, weight, weight), base and cliff texture arrays (512x512, box-filtered mips), bilinear blending between grid points in the shader, cliff texture by slope (projected sideways). StartOakVale reads as Oakvale: grass, mud paths, the stream bed (`LANDSCAPE_WATER`), ploughed fields, earthy cliffs.
- Unnamed theme slots: a key-based lookup did not hold (the same key names different themes even within a region), so they get the map's first real named theme as a labelled stand-in.
- Set by eye, to compare with the game: texture tiling (12 world units per repeat), the slope range for cliffs.
- Ross saw water drawn on some paths. Water themes paint thin full-weight strips beside paths and on slopes; the viewer now draws them with their cliff texture (see defs.md). The grid's weight bytes are also not fully understood: where bytes 11 and 12 name the same theme, bytes 13 and 14 do not add to 255, so there may be three weights.
- Water surfaces: Ross sees water across the path under the bridge. Lake themes carry `WaterType` and `WaterHeight` fields, but `WaterHeight` is not a world height and lake themes also cover dry hillsides, so surfaces live elsewhere. Not in the Tng files (no water things). The stb sea banks use an unidentified compression (not LZO, not deflate); the Lev data after the grid is undecoded. Candidate patch for the bridge water: `WATER_BWLAKE_2` at x 3541–3544, y 901–907.
- Next: decide with Ross between decoding water surfaces now or moving on.

## 2026-10-11 (later): the game's text

- `lang/English/text.big` decoded (`fable_data::strings`, `docs/formats/strings.md`): 26,807 strings (UTF-16 text, voice bank, speaker, identifier, tags), 2,105 groups of alternatives, the 379-name `[NarratorList]`. All region display names resolve.
- Viewer: regions show the game's names ("Oakvale (StartOakVale)", "Bowerstone South (BowerstoneSlums)"), including on the loading screen; a Text tab searches every string by name or content and shows its text, speaker, voice bank and tags.

## 2026-10-11 (later): animation blending

- The animator crossfades over 0.25 s when it switches clips (smoothstep weight; rotations interpolated the short way, positions linearly), so the Hero, the creatures and doors no longer snap between idle, walk, run and fidgets. A door's resting pose at load still jumps straight to its state.

## 2026-10-11 (later): door frames and windows created by buildings

- Ross saw doorways without frames. The door mesh is only the leaves; buildings create their door frames and windows at `CREATEOBJECT <definition>` dummies in their mesh header (`docs/formats/mesh.md`). Static mesh headers now parse (all 2,583) with their dummies; the viewer places the objects they create (148 in StartOakVale): carved frames around every door, glazed windows in the walls.

## 2026-10-11 (later): doors and collisions

- Doors (`CTCDoor` things) are skinned and posed by their `DOOR_OPEN`/`DOOR_CLOSE` animations (played once, holding the last frame), starting in their `Open` state. A closed door blocks a 0.5-unit strip across its doorway; **E** opens or closes the nearest door within 2 units. Checked on the Inn door: the Hero stops at it closed, it swings open inward, he walks through.
- People block each other: the Hero cannot come closer than 0.7 to a creature, wandering creatures avoid the Hero and closed doors. `Hero::step_blocked` takes the blocking test.
- `--use` (with `--walk-to`) uses the nearest door and walks on, for screenshots.
- Fix (Ross): closed doors swung open by themselves on load, because they were posed by the opening animation and left playing. Doors now start holding the last frame of their closing (or, if open, opening) animation.

## 2026-10-11 (later): door frames indoors

- Ross saw doors standing without frames indoors. Doors (`MESH_VILLAGE_DOOR_TYPE_03`) are just the leaves; the frames are part of the building's exterior walls, which were hidden along with the roof. Now only the exterior's roof materials (`Roof`, `roof`, `Roof fringe`, `Fringe`) are hidden indoors and skipped by camera rays; walls and frames stay.

## 2026-10-11 (later): wandering villagers

- Every Prologue creature has a `ScriptName` naming its role in the Prologue quest (`NOVI_Villager` for background villagers; `NOVI_AffairWife`, `NOVI_Bully`, `NOVI_TeddyGirl`, `NOVI_BookTrader`, `NOVI_Theresa`, … for quest characters). Background villagers and unscripted creatures (chickens) now stroll within 5 m of where they are placed, at their `ST_WALK` clip's own speed, resting 8–22 s between strolls; quest characters and guards stay put until quests exist. Stand-in behaviour: the game's creature AI is not decoded.
- `--npc-time T` advances the creatures before a screenshot.

## 2026-10-11 (later): bone configs, the young Hero's real size

- `Bones/*.bncfg` give per-bone scales (along, across) that reshape a skeleton; a creature's `CSkeletalMorphDef` names its files (`CREATURE_HERO_CHILD` → `hero_young_set.bncfg`). Parser `fable_data::bncfg`, docs `docs/formats/bncfg.md`.
- Viewer: skins apply their creature's morph (own vertices scaled in bone space, children's offsets scaled, not inherited). The young Hero now has a child's proportions from the data; the guessed 0.65 scale (and the sister's) is gone. His speed scales by how much the morph lowers his hips (0.75: 2.0 m/s walking, 4.9 running).

## 2026-10-11 (later): animation tables

- `CAppearanceDef` holds each creature's animation table: slot hash, animation bank id, slot name offset, modifier name offset (`docs/formats/defs.md`). The young Hero's `ST_IDLE_SUBTLE`/`ST_WALK`/`ST_RUN` are exactly the clips chosen by hand before.
- Viewer: the Hero takes idle, walk and run from `CREATURE_HERO_CHILD`'s table and, after standing 10 s, plays one of his fidgets (`ST_IDLE`: kick the floor, scratch his ear, …). Creatures loop their own `ST_IDLE_SUBTLE` and every 6–14 s play one of their `ST_IDLE` fidgets; the by-kind guess remains only as a fallback.
- New example `anim_slots` prints a creature's table; `def_dump` resolves `names.bin` offsets.

## 2026-10-11 (later): every static mesh parses

- 29 static (type 1) meshes, among them the burnt Oakvale's charred villagers and the statues, use the skinned layout (levels from the start of the entry, each with a skeleton) with static chunks: `Mesh::parse_static` falls back to it. The chunk end word is a count of physics blocks (4 on washing lines). All 2,583 static meshes parse; the real-data test now requires it.
- Stream lengths and bone counts are bounded before allocating (a misread length had asked for 1.2 TB).

## 2026-10-11 (later): quest-state sections, depth fix

- Ross saw bandits among the villagers and meshes changing as the camera or Hero moved.
- Tng sections are quest states and village events (`docs/formats/tng.md`); the viewer was showing all of them at once. It now shows `NULL` plus a default per region (StartOakVale: `Q_NewOakValeIntro_PreAttack`); the side panel's "Quest state" list and `--section` switch others on (the raid's aftermath, Oak Bay's events). Pre-raid Oakvale: 630 meshes, 29 creatures, all load.
- The camera's far plane was 10x its distance, so in walk mode (about 6 m, less after camera collision) meshes beyond 60 m, or a few metres, were clipped and popped in and out as the distance changed. Now fixed planes (0.05 to 5000) with reversed-Z depth (`Depth32Float` cleared to 0, `Greater`).

## 2026-10-11 (later): loading screen

- The viewer loads regions (and meshes) on a background thread and shows "Loading <region>…" with a spinner meanwhile; for a region exit the Hero is placed at the entrance on that thread too. Checked in the window (captured with `xwd`): Oak Bay -> loading screen -> Barrow Fields, 4.4 s. A queued exit or `--walk-to` target does not carry into the new region.
- `--camera`, `--walk` and `--walk-to` now also work in the window (the Hero walks himself to the point), for testing.
- Mesh chunk end word: any non-zero value is followed by a physics block (4 on `MESH_BS_WASHINGLINE_05`, whose later data still does not parse).

## 2026-10-11 (later): roofs hidden indoors

- A building's `CMultiStaticMeshDef` flags each mesh as exterior or interior (`docs/formats/defs.md`). The viewer records each building's exterior draws and triangles and its interior's box; while the Hero is inside the box, the exterior (walls and roof) is not drawn and camera rays ignore it, so the camera looks into single-storey houses from above. In two-storey buildings (the Inn) the upper floor, part of the interior, still keeps the camera low.

## 2026-10-11 (later): NPCs

- Creatures (`AICreature` things) keep their placement in `CTCPhysicsNavigator`; `Thing::position` now reads it. Their `Graphic` uses value tag 4 (skinned meshes), as do beds and doors, which now also render.
- Built-up creatures (villagers, guards, children) are a head, a torso and legs chosen from part lists in their `CCreatureDef`, each with texture swaps (Bowerstone textures -> Oakvale ones); `docs/formats/defs.md`.
- Viewer: all 30 creatures in StartOakVale stand in place, each part skinned on the CPU and playing an idle (chosen by kind as a stand-in: chickens, children, women, men), loops staggered. The UID picks each villager's parts, in place of the game's random choice.
- Not yet: creature AI or movement, which animation the game picks, scale of the young sister (0.65 like the Hero).

## 2026-10-11 (later): region transitions

- Region exits decoded (`docs/formats/gtg.md`): a `REGION_EXIT_POINT` thing's `EntranceConnectedToUID` is the destination map's `MapUID` << 40 plus the low 40 bits of a `REGION_ENTRANCE_POINT`'s UID in `Levels/FinalAlbion.gtg` (Tng text per map, now parsed by `fable_data::gtg`). All 129 exits in real regions resolve.
- Viewer: walking into an exit's radius loads the destination region and places the Hero at its entrance (exits re-arm once he has left them). Checked in screenshots: Oak Bay → Barrow Fields → Haunted House, the Phase 3 gate's three linked regions. Each region loads whole, in a few seconds; there is no loading screen or streaming yet.
- New example `region_dump` (a region's maps and things; `UID:` lookups).

## 2026-10-11 (later): building interiors

- Ross noticed the Hero walked on the ground under the Inn's furniture: building interiors were never placed. A building's `CMultiStaticMeshDef` component lists its meshes (exterior and interior); `Graphic` names only the exterior. `Def::component`, `Def::static_meshes`, `Defs::meshes_of`; the viewer places all of them (7 more meshes in StartOakVale).
- The interiors carry the floors: in StartOakVale the second navigation layer now gets heights on 1,210 of 1,220 cells (was 342, the bridge only), and the Hero walks on the Inn's and the houses' floors. The spawn point now uses the walk layer's height, not the terrain's.
- New example `mesh_dump` prints a mesh's levels, materials and chunks.

## 2026-10-11 (later): animations, the young Hero walks and runs

- Animations (bank types 6, 7, 9; 3,435 entries, 210,743 bone tracks) all parse: `fable_data::anim`, `docs/formats/anim.md`. Each entry is a u32 length and one LZO stream of chunks (`3DAF`, `ANRT` > `AOBJ` > `XSEQ` per bone, `HLPR` > `MVEC`, `XALO`). Tracks hold float quaternion keys and i16 position keys, each either constant, per frame or a palette with per-frame indices.
- Conventions checked against `MESH_YOUNGHERO_02`'s skeleton: rotations match the rest-pose convention; positions are directions scaled so the largest component is 1, their length comes from the skeleton (so adult Hero animations fit the young Hero).
- `MVEC` starts with the distance an animation carries the character per loop: `ANIM_HERO_WALK` 266.67 mesh units per second, `ANIM_HERO_RUN` 390 per 0.6 s. Walk cycles stay in place.
- Viewer: the Hero is skinned on the CPU each frame and plays `ANIM_HERO_IDLE` standing, `ANIM_HERO_WALK` walking and `ANIM_HERO_RUN` with Shift; his speed comes from the clips' `MVEC` (1.73 and 4.22 m/s at the young Hero's estimated 0.65 scale), so the feet keep pace. `--anim NAME --anim-time T` poses him for screenshots.
- Not yet: which animation the game uses for what (`CAppearanceDef` slot values are not bank ids), blending between clips, the young Hero's true scale.

## 2026-10-11: skinned meshes, the real young Hero

- Skinned meshes (bank entry type 5, 504 entries, 1431 levels of detail) all parse: skeleton (bone names, parents, rest poses, inverse bind matrices), skinned chunks with per-sub-mesh bone lists, 4 bone indices and weights per vertex, and the optional physics stream after a chunk (`docs/formats/mesh.md`). The rest pose composed down the hierarchy reproduces every inverse bind matrix (worst error 4e-6), which fixes the matrix and quaternion conventions.
- Static meshes: the chunk end flag and physics stream apply to them too; 2,549 of 2,583 now parse (was 2,519).
- `game.bin`: creature definitions are lists of components (`CAppearanceDef`, `CCreatureDef`, `CHeroMorphDef`, …) pointing at component definitions by global index. `CREATURE_HERO_CHILD`'s `CAppearanceDef` holds its animation table (`ST_IDLE_SUBTLE`, `CS_SHIFT_WEIGHT3`, …). New example `def_dump` prints a definition with known name hashes resolved.
- Viewer camera collision in walk mode: the follow camera casts rays from the Hero's chest to the camera and to eight points 0.5 m around it, against the placed meshes' triangles (bucketed in 4 m cells) and the heightmap, and pulls in in front of the nearest hit. The scroll wheel sets the preferred distance (2–15 m).
- Viewer upper walking levels: one walk grid per navigation layer; each cell's height is chosen among the terrain and the placed meshes' upward-facing surfaces by growing out from known heights (step 0.6, highest surface within a step, terrain covered lower than 1.6 not walked on). The Hero changes layer where surfaces meet within a step. The Oakvale trader bridge works end to end (`--walk-to 3560,884.5` from `--camera 3526,884.5,21,5,0,0.3`); three other second-layer patches stay unwalkable (see `docs/formats/nav.md`). "Show navigation" now draws the walk grids at their heights.
- Viewer: skinned meshes appear in the Meshes list and draw in their bind pose; walk mode now uses the real `MESH_YOUNGHERO_02` (bind pose, scaled by an estimated 0.65, since the mesh is adult-sized and the scale in the data is not found yet).

## 2026-10-10 (later): placeholder Hero and Hero textures

- Hero textures: the Prologue Hero is `MESH_YOUNGHERO_02` (mesh id 4300, type 5, skinned); its info block lists `YOUNGHERO_SKIN_FACE`, `YOUNGHERO_HAIR`, `YOUNGHERO_SKIN_TORSO` (outfit atlas) and `HERO_SKIN_MOUTH`; all decode correctly (checked by eye). Type-5 meshes (504, the character parts) share the static-mesh info layout. Creature definitions (`CREATURE_HERO_CHILD`, …) do not reference meshes through `Graphic`; how they pick body parts is not decoded.
- Viewer: a Textures tab lists every texture by name and shows the selected one.
- Viewer walk mode ("Walk" checkbox, or `--walk` for screenshots): a blocky placeholder Hero textured with the young Hero's textures, on a 0.5 m walkability grid from the ground-layer navigation leaves, height from the heightmaps, WASD relative to the camera, Shift to run, sliding along blocked edges, follow camera.
- Ross confirmed walk mode and the texture browser work in the window (2026-10-11).
- Not yet: camera collision (the camera can end up inside houses), walking on upper navigation layers (bridge decks), skinned meshes and animation.

## 2026-10-10 (later): the rest of the Lev

- Ross chose to move on from water. Added `Lev::parse_tail`: sound map (one record per 4x4 units), a fixed 33,801-byte block, and named sections, one per map state (`NULL`, quest states like `Q_NewOakValeIntro_PreAttack`). All 398 maps parse; filler maps have no sections.
- The section records look like a navigation subdivision (8 levels, node positions at half-size steps). Decoding them is the next step for walking and collision (Phase 3).
- Decoded them: `fable_data::nav` parses every section exactly (outline points, then layers of 32-unit root quadtrees down to 0.5 units; leaves carry a kind, a value, neighbour ids). Drawn in the viewer (`--nav`, or the "Show navigation" checkbox), StartOakValeWest's leaves follow the village paths and walkable ground, with holes at buildings; layer 1 is the bridge deck and raised floors.
- Next: Phase 3 proper, a walkable Hero: movement constrained to navigation leaves, ground height from the Lev, a follow camera.

## 2026-10-10 (later): definitions and placed meshes

- Added `fable_data::defs`: `names.bin` and `game.bin` (type table, directory, zlib chunks, records), with `Def::graphic` finding the mesh id. 95% of placed buildings and objects resolve to a mesh.
- The viewer now places every building and object of a region on its terrain (602 of 629 meshes in `StartOakVale`; the rest are skinned). Meshes are scaled by 0.01: mesh units are centimetres if world units are metres (a chair is 119 units tall, a map 96 wide). Fences and walls join up end to end, which suggests orientation (local x right, y forward, z up) is right.
- `--camera x,y,z,distance,yaw,pitch` overrides the screenshot camera.
- Orientation fix after Ross compared with the game (tavern in the right place, but the bridge did not span its two high points and some house doors faced the cliffs): meshes face their local -y. The bridge test confirmed it from the terrain heights: only that way round do both ends rest on the 21 m plateaus.
- Clean-room note: screenshots of the game as Ross plays it count as observed gameplay. They are fine to look at but are kept out of the repo.
- Not yet: textured terrain; skinned meshes; definitions without a `Graphic`.
- Next: terrain texturing from the Lev themes, then compare against the game for Milestone A.

## 2026-10-10 (later): Tng placement

- Added `fable_data::tng` (things, components, typed accessors for definition, position, orientation) on a shared `text` tokenizer (moved out of `wld`). All 398 Tng files parse: 21,800 things.
- Confirmed the coordinate frame: buildings and objects sit at the Lev ground height (median gap 0.12 over 12,255 things); positions are map-relative, z up.
- Found that `names.bin` maps JAMCRC hashes to definition names (13,593 names). `game.bin` does not reference names by hash; its layout (offset tables, records at odd byte offsets) needs its own session.
- Next: decode `game.bin` enough to map a definition (e.g. `BUILDING_TOWN_WAREHOUSE_02`) to its mesh, then place meshes on the terrain for Milestone A.

## 2026-10-10 (later): static meshes

- Added `fable_data::mesh`: info block, LOD blocks (materials, geometry chunks, sub-meshes with strips or lists, packed or float positions, packed normals, UVs, compressed or raw streams). 2,519 of 2,583 static meshes parse (10.1 million triangles); all their texture ids exist.
- The viewer gained `--mesh NAME` and a searchable mesh list. Checked by eye: chalk circle (alpha), quest card (UVs), stump chair, Oakvale house and tavern (multi-chunk, 5 textures) all render with their original textures.
- Learned: LOD blocks fill the end of the entry, so the header can be skipped; a LOD can hold up to 6 geometry chunks; vertex format flag 16 means float positions; meshes use the same compressed-stream scheme as textures.
- Not yet: skinned meshes (bones), the mesh header (helpers, bones), the 8 extra bytes in formats 6 and 22, triangle winding (drawn double-sided for now).
- Next: place meshes in a region from the Tng files (Milestone A needs Oakvale's buildings standing on its terrain).

## 2026-10-10 (later): textures, and desktop-only scope

- Added `fable_data::dxt` (DXT1/3/5 decoding) and `fable_data::texture`. All 6,717 textures in `textures.big` and `frontend.big` unpack exactly (one 16-bit texture unsupported). PNG export via the `texture_png` example; DXT1, DXT3 and DXT5 samples checked by eye.
- Learned: the top level is LZO1X-compressed except its last 3 bytes, which follow raw (or the whole level is raw when the length is 0); the rest of the mip chain is raw; levels are padded to powers of two; levels below 4x4 are stored at bits-per-pixel size; bank type 1 is a frame sequence, other types one image; `0x20` is DXT3, `0x23` DXT5.
- Scope change (Ross): this repository is the desktop reimplementation only. The PS Vita version will be a separate repository, an ARM port that reuses these crates. PLAN.md, README, CLAUDE.md and the Nix shell were updated; Vita budgets and tooling moved to PLAN.md's "Future: PS Vita port" section, and portability rules were added.
- Next: Bbm meshes from `graphics.big`, shown textured in the viewer.

## 2026-10-10 (later): sea plane

- The viewer draws a translucent sea plane at height 3.0 for regions that have an stb sea bank and whose ground dips below 3.0.
- Learned: sea level is 3.0 (731 raised stb tiles top out there); the `.wld` `IsSea` flag does not mean "has sea"; the 25 stb sea banks are per region and compressed in an unidentified format; inland lakes have their own heights (10.5 to 78).
- Next: lake water heights (probably from the sea banks), texture themes per grid point, then `textures.big` and `graphics.big`.

## 2026-10-10 (later): first terrain in the viewer

- Added `apps/viewer` (eframe + wgpu): loads every map a region contains or sees, builds one terrain mesh from the Lev heightmaps, draws it with placeholder height/slope colours into an offscreen texture shown in an egui window. A region list switches regions. `--screenshot` renders headlessly to a PNG.
- `StartOakVale` (12 maps) and `OakBay` (16 maps) both render as one seamless coastline; the two are the same place remodelled, which is a good sanity check.
- Ross confirmed the window opens and the terrain matches the game's orientation (world x and y as stored, z up).
- Not yet: no water plane (the sea floor shows at height 0); in the VM, rendering uses llvmpipe (software OpenGL), fine for correctness only.
- Next: water plane at the sea height, texture themes per grid point (Lev bytes 10–20 probably), then meshes and textures from `graphics.big` and `textures.big`.

## 2026-10-10 (later): Lev heightmap matches the Stb

- Added `fable_data::lev`: map size, texture theme slots, sound themes and the heightmap grid. Tested on all 398 maps against the stb (size, per-map value, tile heights).
- Learned:
  - The Lev heightmap is `(w+1)*(h+1)` 21-byte points with an unaligned `f32` height at byte 5; world height = value * 2048.
  - Stb tile bounds contain the Lev terrain in 10,253 of 10,257 tiles; the stb tile vertices carry the same heights, so stb tiles are pre-built terrain meshes from the Lev.
  - One map (`HauntedHouse_Filler_01`) leaves out an optional per-map value before its sound themes.
  - EgoCore is MIT and Windows-only, but its README cites reverse engineering, it installs Fable Script Extender, and its Def compiler is jamen/fable-defs. Black-box use only; recorded in PLAN.md and CLAUDE.md.
- Next: render the Lev heightmap in the viewer as the first terrain (it needs only the grid), then decode the rest of the stb vertex record and the grid point bytes 10–20.

## 2026-10-10 (later): Stb tiles and LZO

- Added `fable_data::lzo` (LZO1X decompression, written from the kernel's `lzo.rst` bitstream description) and `fable_data::stb` (common header, per-map headers, tile tables, tile decompression). Added `README.md`.
- Learned:
  - The stb's common header has one header per map giving its size and position (matching `.wld` for all 398 maps).
  - Each map entry holds a grid of 16x16-unit tiles, LZO1X-compressed, with empty tiles left out. Every tile in the install decompresses; output matches the reference LZO library.
  - After the tiles comes a section that looks like a spatial tree. Not decoded.
- Next: decode the decompressed tile format (terrain vertices?) and compare it with the Lev heightmap for the same map; then the tree section.

## 2026-10-10: workspace and data survey

- Created the Cargo workspace (`crates/fable-data`), pinned `nightly-2026-10-10`, added GPLv3 `LICENSE` and `CLAUDE.md`.
- Parsers with tests: the shared bank index (`bank`), `BBBB` banks (`bbbb`), `BIGB` archive bank tables (`big`) and the world file (`wld`). Every index in the install round-trips byte-for-byte.
- Wrote `docs/inventory.md`, `docs/survey.md` and format pages for the bank index, `BBBB`, `BIGB` and `Wld`.
- Learned:
  - `.wld` defines 141 regions explicitly, each with `ContainsMap` and `SeesMap` lists, so the set of maps loaded together is known.
  - The Stb has one entry per Lev (566 MB in total, about 3.8 times the Levs); it is the biggest part of every region's map data.
  - Heaviest region by Lev + Tng + Stb is `ExecutionTree` at about 83 MB, almost all from maps it sees. The Prologue's `StartOakVale` is about 33 MB.
  - The Prologue's Oakvale is region `StartOakVale`; the adult Oakvale is `OakBay`.
- Removed the openalbion clone (sourced from decompiled and leaked material; see PLAN.md).
- Next: decode a Stb per-map entry and a Lev header; list mesh and texture entries with names from `Defs/RetailHeaders`; Vita memory test once the Vita is set up.
