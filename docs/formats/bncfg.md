# Bone configs (`Bones/*.bncfg`)

60 text files (CRLF) that reshape a skeleton for one kind of character: `hero_young_set.bncfg` (the Prologue Hero), `young_sister_set.bncfg`, `bs_male_tall.bncfg`, `hero_fat.bncfg`, `bandit_short.bncfg`, … Parser: `fable_data::bncfg`; all parse (`tests/real_data.rs`).

```
Creature_type: CREATURE_HERO_CHILD_02;
#Start_group_settings
clavs: "Bip01 L Clavicle", "Bip01 R Clavicle";
hands: "Bip01 L Hand", "Bip01 L Finger0", …;
#End_group_settings
#Start_Bone_data
Bip01 L Thigh: 0.6, 0.75, 0.75;
Sub_movement_dummy: 1.0, 1.0, 0.75;
#End_bone_data
```

`Creature_type` names a creature definition (informational). Groups name sets of bones (their use is not known). Each bone line gives three scales in the bone's own frame: x along the bone (Biped bones point along +x, so x is its length) and y, z across it. Values seen run from 0.6 to 4.05 (`hero_berserk.bncfg`'s neck).

A creature picks its files through its `CSkeletalMorphDef` component: field `0xcbf27745`, a `u32` count, then that many `names.bin` offsets of file names (`CREATURE_HERO_CHILD` → `hero_young_set.bncfg`; [defs.md](defs.md)). Villagers have none.

How the viewer applies them (matches the files' own consistency, not checked against the game): a bone's scale shapes its own vertices (in bone space) and scales its children's offsets from it, but is not inherited by them. With `hero_young_set.bncfg` the young Hero's thighs shorten to 0.6, his spine segments to 0.65–0.75, and `Sub_movement_dummy`'s 0.75 lowers the whole body over the root so the feet stay on the ground: his hips end at 0.75 of the adult height, the same ratio the leg scales give. Walk and run cycles made for full-size skeletons then cover 0.75 as much ground, so the viewer scales his speed by that ratio (2.0 m/s walking).
