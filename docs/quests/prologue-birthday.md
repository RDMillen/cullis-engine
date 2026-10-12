# Prologue: Theresa's birthday

Region `StartOakVale`, quest state (Tng section) `Q_NewOakValeIntro_PreAttack`. Code: `crates/engine-script/src/prologue.rs`. Text: the `TEXT_QST_048_*` entries in `lang/English/text.big` (the quest's number in the text is 048).

The young Hero must buy his sister Theresa a birthday present. His father pays a gold piece for each good deed done around Oakvale; the Trader sells a box of sweets for three gold pieces. Giving them to Theresa ends the quest, and the bandits arrive.

## People

All placed in the Prologue state, found by their Tng `ScriptName`:

| Role | Who | Where (world x, y) |
| --- | --- | --- |
| `NOVI_LiveFather` | the Hero's father | 3495, 867 |
| `NOVI_Theresa` | his sister, by the Barrow Fields gate | 3534, 933 |
| `NOVI_BookTrader` | the travelling Trader | 3541, 832 |
| `NOVI_Bully`, `NOVI_Victim` | a bully and the boy he picks on | 3603, 859 |
| `NOVI_TeddyGirl` | the girl who lost her teddy bear, Rosie | 3494, 852 |
| `NOVI_BarrelMan`, `NOVI_BarrelThug` | a warehouse worker and a boy who wants his stock smashed | 3576, 823 and 3593, 823 |
| `NOVI_AffairMan`, `NOVI_AffairWoman`, `NOVI_AffairWife` | a husband with another woman, and his wife | 3480, 826 and 3551, 854 |
| `NOVI_Guard` (3) | town guards | |
| `NOVI_Villager` (8) | villagers | |

## Deeds

| Deed | How | Good or bad | Gold |
| --- | --- | --- | --- |
| Drive off the bully | Kick him three times | good | from the father |
| Return Rosie | The victim hands her over once the bully is gone (or when hit); give her to the girl | good | from the father |
| Give Rosie to the bully | Talk to him with the teddy, answer yes | bad | 1 from the bully |
| Watch the stock | Talk to the worker; stay within 5 m of his spot while he is away (40 s) | good (bad if away more than 8 s) | from the father |
| Take the husband's bribe | Talk to him, answer yes | bad | 1 |
| Tell the wife | Talk to her after meeting the husband, answer yes | good | from the father |
| Hit anyone else | First hit on each role | bad | |

The father pays for good deeds when talked to, and scolds the Hero for new bad deeds. A guard, talked to after bad deeds, reads out the list of crimes (`GUARD_CRIME_*`). Villagers greet the Hero by what he has done, in male or female lines (`VILLAGER_SPOKEN_TO_*`, `VILLAGER_DONE_GOOD_DEEDS_*`, ...).

The ending: Theresa, given the sweets, says `THERESA_HELLO` and `THERESA_BANDITS`, and the region moves to `Q__OakValeIntro_PostAttack`.

## Not checked against the game (guesses)

- The bully gives up after three kicks; the bully pays 1 gold for the teddy.
- The worker is away 40 s; the Hero must stay within 5 m and may be away 8 s; the thug taunts every 5.5 s.
- Hitting the victim makes him give up the teddy.
- The tutorial notices shown after the father's first speech (`INSTRUCTION_HIGHLIGHTING_70`, `_60`, `_40`, `_50`).
- The father's opening speech (`FATHER_INTRO`, in the game a cutscene when the Prologue starts) is said on first talking to him.

## Not yet

- Smashing the worker's barrels (barrels are not breakable objects yet), so `BARRELMAN_LETDOWN_BROKEN` and `GUARD_CRIME_BARREL_BREAKING` go unused.
- People walking where the story sends them: the bully and the worker vanish and reappear; the wife does not go to her husband.
- The cutscenes (`CS_009`, `CS_048_OIF`), the diary and other readable objects, voices.
