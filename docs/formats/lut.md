# Voice banks (`lang/<language>/*.lut`)

Spoken lines: `Dialogue.lut` (12,134 sounds; villager and guard chatter), `Dialogue2.lut` (1), `ScriptDialogue.lut` (5,310; quests and cutscenes) and `ScriptDialogue2.lut` (2,769, numbered up to 3,060; The Lost Chapters' additions). Parser: `fable_data::lut`; decoder: `fable_data::adpcm`; which line is which sound: `fable_data::voices`.

## Layout

All little-endian. The file starts with the 8 bytes `LiOnHeAd`, then chunks. Each chunk is a 32-byte zero-padded name, a u32 length, then that many bytes.

1. `LHAudioBankCompData`: the sounds, back to back. Each is a 36-byte header then a complete RIFF WAVE file.
2. `LHAudioBankLookupTable`: u32 (1000; 500 in `Dialogue2.lut`), u32 count, then per sound 12 bytes: u32 id, u32 length (header and WAVE), u32 offset (from the start of the first chunk's data). Ids ascend; `ScriptDialogue2.lut` skips ids (see below).
3. `LHFileSegmentBankInfo` (524 bytes): the text `\Generated bank for Fable`, later `NoFPNameYet`, zero padding, and a final u32 4.

### Sound header (36 bytes)

| Offset | Type | Observed |
| --- | --- | --- |
| 0 | u32 | the sound's id |
| 4 | u16 | 1 |
| 6 | u16 | 22050 (the sample rate) |
| 8 | u16 | 40000 |
| 10 | u16 | 0 or 1 |
| 12 | u16 | 0 |
| 14 | u8 × 4 | 1, 1, 6, then 0x7f (0x58 in `Dialogue2.lut`) |
| 18 | u16 | 100 |
| 20 | f32 | 5.0 (1.5) |
| 24 | f32 | 25.0 (18.0) |
| 28 | u32 | 1000 (500) |
| 32 | i32 | -1 |

Apart from the id and the 0/1 flag, every header seen is the same; perhaps playback settings (volume, distances, priority). Kept as `unknown_header`.

### Audio

The WAVE files use format 0x69, Xbox ADPCM: mono, 22,050 Hz, 4 bits, 36-byte blocks, fmt extra u16 64 (samples per block). A block holds per channel a 4-byte header (i16 first sample, u8 step index, u8 reserved), then the channels' 4-bit codes in turns of 4 bytes per channel, low nibble first. Codes follow the standard IMA ADPCM tables, with the step change computed as `((2 * magnitude + 1) * step) >> 3`. A block gives 64 samples: the header sample, then the first 63 codes (the last code is not used). `fable_data::adpcm` matches ffmpeg's `adpcm_ima_xbox` decoder sample for sample, and decoded lengths match the lipsync durations below.

## Lipsync (`lang/<language>/dialogue.big`)

A BIG archive with banks `LIPSYNC_ENGLISH_MAIN`, `_MAIN_2`, `_SCRIPT`, `_SCRIPT_2`, one entry per sound named `<bank>_<id>` (`ScriptDialogue_112`). Each entry's sources are `["", "SPEAKER_FEMALE1"]` for every entry, and its 4-byte info is the sound's length in seconds (f32). Its data is a viseme timeline: a u32 count, then that many viseme names (`AH`, `EE`, `OH`, `SZ`), then keyframes (not decoded). Ids with no recording are entries named `<bank>_<id>[Empty]` with no data: 291 in `ScriptDialogue2`, matching the ids its `.lut` skips.

`Defs/RetailHeaders/*_lipsync.h` only number the sounds (`ScriptDialogue_1 = 1`).

## Which line is which sound

Nothing in the data links them directly. A text entry (`text.big`) names its voice bank (`ScriptDialogue.lug`) but no sound, and its index info is the speaker's number. What holds, by measurement:

- Within a bank, sounds follow the bank's spoken lines in name order compared without case (so `_` sorts before letters: `TEXT_QST_048_DAD_DO_MORE` before `TEXT_QST_048_DAD_DONE_NOTHING_YET`). Spoken means a speaker other than none, `NONE` or `UNSPOKEN`; the banks also tag menu texts and object descriptions that have no sound.
- Lines were added and cut after the banks were built, so the two lists drift apart by a few to a few dozen places, both ways.

`fable_data::voices` rebuilds the link by aligning the two lists (dynamic programming within ±300 places): a line pairs with a sound whose length fits the line's (about 0.4 s plus 14 characters a second), and a line or a sound may go unpaired at a cost. In `Dialogue` and `ScriptDialogue`, 95% or more of spoken lines get a sound and about 87% fit its length, against about 52% (`ScriptDialogue`) when each line is paired with the sound five places on. Lines whose fit looks poor are mostly grunts, whispers and crying, which run long.

`ScriptDialogue2` does not follow name order (nor text id order); its lines are left unvoiced until its order is known.
