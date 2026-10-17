# Ambience and music (`Sound/*.lug`, `*.met`, `*.ogg`)

Parsers: `fable_data::lug` (ambient banks), `fable_data::atmos` (which file plays where). Music is Ogg Vorbis, decoded by `engine_audio::ogg` (the `lewton` crate).

## Which plays where

`Defs/atmos_types.h` is a plain C header shipped with the game. Its enums number the music sets (`EMusicSetType`: `MUSIC_SET_NULL = 1`, …, `MUSIC_SET_OAKVALE` = 19) and the atmospheres (`EAtmosType`: `ATMOS_NULL = 0`, `ATMOS_INN = NO_SOUND_TYPES` (6), `ATMOS_NIGHT`, `ATMOS_VILLAGE`, `ATMOS_WOODLAND`, `ATMOS_COUNTRY` = 10, …). `fable_data::atmos::Enums` reads it, values that name other constants included.

- **Music.** A `REGION` definition (named by the `.wld` region's `RegionDef`) holds its music set after field hash `0xcab1fa53` (name unknown; found by matching 34 of 47 regions to the music their names suggest). `REGION_OAK_VALE_INTRO`, `REGION_OAK_VALE` and `REGION_OAK_VALE_MEMORIAL_GARDEN` have 19, `MUSIC_SET_OAKVALE`. The file is the set's name without `MUSIC_SET_`, compared without case or underscores: `Sound/OAKVALE.OGG`. Sets with no such file: `MUSIC_SET_DANGER_ONLY` (32; quiet until danger), `MUSIC_SET_CAVES`, `MUSIC_SET_DRAGON`.
- **Ambience.** Each map's Lev has a sound map: one record per 4x4-unit block (see `lev.md`) whose last byte counts from 1 into the map's list of sound theme names. In `StartOakValeWest` 9 is `SOUND_THEME_COUNTRY` (most of the village), 21 `SOUND_THEME_OCEAN`, 37 `SOUND_THEME_SHORE`; in the memorial garden, whose list is two names shorter, 16 is `SOUND_THEME_RIVER`. A `SOUND_THEME` definition holds its atmosphere after field hash `0xe001b756` (`SOUND_THEME_COUNTRY`: 10, `ATMOS_COUNTRY`; `SOUND_THEME_MEADOW` and `SOUND_THEME_GUILD_GARDEN` share it). Its record also holds the strings `DAY` and `NIGHT` (not decoded). The bank is the atmosphere's name without `ATMOS_`, compared without case or underscores: `Sound/country.met` and `.lug`. Atmospheres with no bank: `ATMOS_SECRETPASSAGE`, `ATMOS_KRAKENCHAMBER`, `ATMOS_RAIN`, and `ATMOS_NULL` (the Chapel of Evil and Temple of Light themes).

## `.lug`

Starts with `LiOnHeAd` and an `LHFileSegmentBankInfo` chunk (32-byte name, u32 length 0x208, the text `Big Blue Box - Fable - Countryside Ambience`, …), then an `LHAudioWaveData` chunk. The sounds are complete RIFF WAVE files at the offsets the `.met` gives (the first at 600): Xbox ADPCM (format 0x69, see `lut.md`), 44,100 or 22,050 Hz, mono or stereo (beds), with a `fact` chunk. Decoding matches ffmpeg sample for sample (checked on `country`'s stereo `garden.wav`).

## `.met`

All little-endian.

```
u32 version        1
u32 count
count x sound:
  u32 index        1, 2, …
  u32 length, the original path (Latin-1)   S:\BigBlueBox\Fable atmos\Country\1644XBADPCM\garden.wav
  u32 size         bytes of the WAVE file in the .lug
  u32 offset       of the WAVE file in the .lug
  u16 channels
  u16 format       0x69
  u32 sample rate
  i32 loop start, i32 loop end   (-1 for none; 0 and the length for some birds)
u32 count          playback records
u32                1 in every file
count x playback:
  u32 sound        a sound's index; a sound may have several records
  u32 length, a name   usually empty; "Black and White Shoreline" in shore.met
  40 bytes:
    u16 group      1 day, 2 night (0 seen)
    u16            1 in some of Graveyard.met, else 0
    u32 repeats    9999: a looping bed (garden.wav, oceanloop.wav); 1: played once each time
    i32            -1 for beds, 0 otherwise
    u32            0
    u16            7, 25, 40, 45, 60, 90, 127 seen
    u16 volume     percent of the recording (100, 70, 50, 40, 140 seen)
    u16            0, 10, 20
    u16            12, 8, 4
    8 bytes        zero
    u32 frequency  1 for beds; 2000 or 4000 for birds, 400 for waves, 20 for river sounds, 800 for night creatures
    u32            usually the next record's number counting from 1 (0 on the last), but not always
                   (Lost_Bay.met: record 4 has 6, record 9 has 13 of 10)
```

The groups hold what their names suggest: in `country.met` group 1 is the `garden.wav` bed and 106 bird calls, group 2 the `nightloop4.wav` bed and 15 `NightCreature_*` calls (`examples/lug_plays.rs` lists a bank's records by group). The viewer plays a bank's day records by day and its night records at night (when the sun's light is low), crossfading the beds as day turns to night, and indoors eases the beds down to 30% and plays no spot sounds (by eye; how the game treats interiors is not known).

`Northern_Waste_Archons_Shrine.met` plays `mm_wind_lost_bay.wav` twice, by day at 50% and by night at 40%.

## How the viewer plays it (stand-in)

The music loops, crossfading over 2 s when it changes. The beds of the bank under the Hero loop (crossfading over 1.5 s when the bank changes, once the new one has been wanted for 0.75 s), and every 1.5 to 6 s a day spot sound plays, picked by `frequency` as a weight, at its volume scaled by 0.5 to 1 and panned at random. Night sounds are left out (no time of day yet). Mix: voices 1, ambience 0.5, music 0.35. What the unknown fields do (timing, distance, priority) is not known, so the timing is a stand-in.
