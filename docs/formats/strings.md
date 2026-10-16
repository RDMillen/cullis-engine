# Text (`lang/<language>/text.big`)

A `BIGB` archive ([big.md](big.md)) with one bank, `TEXT_ENGLISH_MAIN`: 28,913 entries named like `TXT_REGION_OAKVALE` or `TEXT_QST_067_INFECTED_BANTER_30`. Parser: `fable_data::strings`; all entries decode (`tests/real_data.rs`): 26,807 strings, 2,105 groups, 379 speakers, and every region display name in the wld (94) has its text.

## Entry types

| Type | Count | What |
|---|---|---|
| 0 | 26,807 | a string |
| 1 | 2,105 | a group: alternatives for one line (`TEXT_AI_GUARD_DEMAND_REBRIBE_SECOND` → 4 strings) |
| 2 | 1 | `[NarratorList]`: the speakers |

The string entries' 4-byte info block is a small number (0 on-screen help, 1 banter, 2 and 4 cutscene lines, 3 GUI and region names): probably a category; not decoded.

## String (type 0)

| Field | Type | Notes |
|---|---|---|
| text | UTF-16LE, NUL-terminated | `Oakvale`; button names in brackets (`[B]`) |
| voice bank | u32 length + chars | `Dialogue.lug`, `ScriptDialogue.lug`, `ScriptDialogue2.lug`; empty for unspoken text |
| speaker | u32 length + chars | a `[NarratorList]` name (`INFECTED_TRADER`, `THERESA`); empty if nobody |
| identifier | u32 length + chars | the entry's own name |
| tag count | u32 | |
| tags | count x (u32 0, NUL-terminated name) | `CONVERSATION_ATTITUDE_ANGER`, `…_SADNESS` |

Tags (on about a quarter of the spoken lines) say how a line is delivered:

- `CONVERSATION_ATTITUDE_<mood>`: `ANGER` (508 lines), `FEAR` (234), `SADNESS` (136), `HAPPY` (37), `GLEE`, `NEUTRAL`, `RIDICULE`, `LOVE`, `FRIENDLY`, `BELLIGERENT`, `HOLDING_SWORD`. Creatures' animation tables have `CONV_GENERAL` gestures under the modifiers `CONVERSATION_HAPPY`, `_ANGRY`, `_SAD`, `_FEAR`, `_BELLIGERENT` (four each for the villagers).
- `ANIM:<slot>`: an animation table slot to play with the line (`ST_OPINION_DISAPPROVAL_POINT_AT`, `CS_CONV_HAPPY_BR1`, `STANDARD_HEAD_SCRATCH`, `GIVE_MONEY`).
- `CAM:(<who>,<where>)(<who>,<where>)`: camera framing for the line, as two pairs, apparently where the camera stands and what it looks at (`(SPEAKER,IN_FRONT_OF_FACE)(SPEAKER,FOCUS_FACE)`, `(SPEAKER,BEHIND_RIGHT_SHOULDER_NEAR)(SPEAKEE,FOCUS_FACE)`).

## Group (type 1)

`u32` count, then that many `u32` string entry ids.

## `[NarratorList]` (type 2)

No data; its info block is a `u32` count (379) and NUL-terminated speaker names: `NONE`, `THERESA`, `GUILD_MASTER`, `HERO FATHER`, `BANDIT1`, ….

The voice lines themselves are in `Dialogue.lut`, `ScriptDialogue.lut` and `ScriptDialogue2.lut` (the `.lug` names); see `lut.md`.
