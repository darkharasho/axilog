//! `buff id -> (stacking kind, capacity)`, for every buff the catalog
//! watches (M16, Task 2).
//!
//! Stack type is a property of the BUFF, not of the definition that reads
//! it -- GW2EI keeps it on its `Buff` catalog (`EIData/Buffs/`,
//! `new Buff(name, id, source, BuffStackType.X, capacity, ...)`; the short
//! ctor overload defaults to `BuffStackType.Force, 1`, `Buff.cs:120-125`).
//! This project has no full buff catalog, so the subset the damage-modifier
//! catalog needs is transcribed here rather than being declared per
//! definition: a multi-buff tracker over the twelve boons mixes intensity
//! (Might, Stability) and duration (the other ten) ids, so one flag per
//! TRACKER cannot be right -- it silently simulated Fury, Protection and
//! Resolution as stacking buffs, which the calibration caught.
//!
//! `intensity` is `BuffStackType` in
//! `{Stacking, StackingConditionalLoss, StackingUniquePerSrc}`
//! (`ArcDPSEnums.cs:384-393`); `Queue`/`Regeneration`/`Force` are duration
//! buffs. `capacity` is GW2EI's own, used only as the fallback when the log
//! carries no `CBTS_BUFFINFO` row for the buff.

/// One row of GW2EI's buff catalog: `(id, intensity, capacity)`.
pub struct BuffStackInfo {
    pub id: u32,
    pub intensity: bool,
    pub capacity: u32,
}

/// Sorted by id ([`stack_info`] binary-searches it). 91 entries.
pub static BUFF_STACK_INFO: &[BuffStackInfo] = &[
    // Protection -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:18)
    BuffStackInfo { id: 717, intensity: false, capacity: 5 },
    // Regeneration -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:19)
    BuffStackInfo { id: 718, intensity: false, capacity: 5 },
    // Swiftness -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:26)
    BuffStackInfo { id: 719, intensity: false, capacity: 9 },
    // Fury -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:15)
    BuffStackInfo { id: 725, intensity: false, capacity: 9 },
    // Vigor -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:23)
    BuffStackInfo { id: 726, intensity: false, capacity: 5 },
    // Vulnerability -- BuffStackType.Stacking, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:49)
    BuffStackInfo { id: 738, intensity: true, capacity: 25 },
    // Might -- BuffStackType.Stacking, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:14)
    BuffStackInfo { id: 740, intensity: true, capacity: 25 },
    // Aegis -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:24)
    BuffStackInfo { id: 743, intensity: false, capacity: 9 },
    // Death Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/NecromancerHelper.cs:150)
    BuffStackInfo { id: 790, intensity: false, capacity: 1 },
    // Retaliation -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:27)
    BuffStackInfo { id: 873, intensity: false, capacity: 5 },
    // Stability -- BuffStackType.StackingConditionalLoss, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:25)
    BuffStackInfo { id: 1122, intensity: true, capacity: 25 },
    // Quickness -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:16)
    BuffStackInfo { id: 1187, intensity: false, capacity: 5 },
    // Frost Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:100)
    BuffStackInfo { id: 5579, intensity: false, capacity: 1 },
    // Fire Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:99)
    BuffStackInfo { id: 5677, intensity: false, capacity: 1 },
    // Chaos Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:98)
    BuffStackInfo { id: 10332, intensity: false, capacity: 1 },
    // Force of Nature -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/RangerHelper.cs:617)
    BuffStackInfo { id: 12579, intensity: false, capacity: 1 },
    // Persisting Flames -- BuffStackType.Stacking, 10 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs:292)
    BuffStackInfo { id: 13342, intensity: true, capacity: 10 },
    // Light Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:101)
    BuffStackInfo { id: 25518, intensity: false, capacity: 1 },
    // Resistance -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:28)
    BuffStackInfo { id: 26980, intensity: false, capacity: 5 },
    // Reaper's Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:78)
    BuffStackInfo { id: 29446, intensity: false, capacity: 1 },
    // Berserk -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/BerserkerHelper.cs:63)
    BuffStackInfo { id: 29502, intensity: false, capacity: 1 },
    // Infusing Terror -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:79)
    BuffStackInfo { id: 30129, intensity: false, capacity: 1 },
    // Alacrity -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:17)
    BuffStackInfo { id: 30328, intensity: false, capacity: 9 },
    // Harmonious Conduit -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:62)
    BuffStackInfo { id: 31353, intensity: false, capacity: 1 },
    // Fractal Defensive -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:82)
    BuffStackInfo { id: 32134, intensity: true, capacity: 5 },
    // Fractal Offensive -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:83)
    BuffStackInfo { id: 32473, intensity: true, capacity: 5 },
    // Bowl of Mussel Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:109)
    BuffStackInfo { id: 33148, intensity: false, capacity: 1 },
    // Writ of Masterful Strength -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:101)
    BuffStackInfo { id: 33297, intensity: false, capacity: 1 },
    // Bowl of Curry Mussel Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:114)
    BuffStackInfo { id: 33337, intensity: false, capacity: 1 },
    // Plate of Mussels Gnashblade -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:30)
    BuffStackInfo { id: 33476, intensity: false, capacity: 1 },
    // Bowl of Lemongrass Mussel Pasta -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:108)
    BuffStackInfo { id: 33574, intensity: false, capacity: 1 },
    // Writ of Masterful Malice -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:113)
    BuffStackInfo { id: 33836, intensity: false, capacity: 1 },
    // Oysters with Pesto Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:113)
    BuffStackInfo { id: 39042, intensity: false, capacity: 1 },
    // Oysters Gnashblade -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:111)
    BuffStackInfo { id: 39067, intensity: false, capacity: 1 },
    // Fried Oysters -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:105)
    BuffStackInfo { id: 39302, intensity: false, capacity: 1 },
    // Oysters With Cocktail Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:107)
    BuffStackInfo { id: 39341, intensity: false, capacity: 1 },
    // Oysters with Zesty Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:112)
    BuffStackInfo { id: 39344, intensity: false, capacity: 1 },
    // Fried Oyster Sandwich -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:106)
    BuffStackInfo { id: 39348, intensity: false, capacity: 1 },
    // Oysters with Spicy Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:110)
    BuffStackInfo { id: 39500, intensity: false, capacity: 1 },
    // Dark Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:104)
    BuffStackInfo { id: 39978, intensity: false, capacity: 1 },
    // Desert / Sandstorm Shroud -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ScourgeHelper.cs:97)
    BuffStackInfo { id: 40052, intensity: false, capacity: 9 },
    // Berserker's Power -- BuffStackType.Stacking, 3 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/WarriorHelper.cs:226)
    BuffStackInfo { id: 42539, intensity: true, capacity: 3 },
    // Can of Stewed Oysters -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:104)
    BuffStackInfo { id: 53384, intensity: false, capacity: 1 },
    // Soul Barbs -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/NecromancerHelper.cs:191)
    BuffStackInfo { id: 53489, intensity: false, capacity: 1 },
    // Symbolic Avenger -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/GuardianHelper.cs:256)
    BuffStackInfo { id: 56890, intensity: true, capacity: 5 },
    // Peppercorn-Crusted Sous-Vide Steak -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:409)
    BuffStackInfo { id: 57051, intensity: false, capacity: 1 },
    // Spiced Pepper Creme Brulee -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:446)
    BuffStackInfo { id: 57067, intensity: false, capacity: 1 },
    // Plate of Peppercorn-Spiced Beef Carpaccio -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:423)
    BuffStackInfo { id: 57114, intensity: false, capacity: 1 },
    // Peppered Cured Meat Flatbread -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:411)
    BuffStackInfo { id: 57127, intensity: false, capacity: 1 },
    // Spiced Peppercorn Cheesecake -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:447)
    BuffStackInfo { id: 57129, intensity: false, capacity: 1 },
    // Plate of Peppered Clear Truffle Ravioli -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:426)
    BuffStackInfo { id: 57155, intensity: false, capacity: 1 },
    // Spherified Peppercorn-Spiced Oyster Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:444)
    BuffStackInfo { id: 57165, intensity: false, capacity: 1 },
    // Peppercorn-Spiced Eggs Benedict -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:410)
    BuffStackInfo { id: 57210, intensity: false, capacity: 1 },
    // Plate of Peppercorn-Spiced Coq Au Vin -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:424)
    BuffStackInfo { id: 57260, intensity: false, capacity: 1 },
    // Bowl of Spiced Fruit Salad -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:393)
    BuffStackInfo { id: 57276, intensity: false, capacity: 1 },
    // Plate of Peppercorn-Spiced Poultry Aspic -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:425)
    BuffStackInfo { id: 57299, intensity: false, capacity: 1 },
    // Peppercorn and Veggie Flatbread -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:408)
    BuffStackInfo { id: 57382, intensity: false, capacity: 1 },
    // Weight of the World -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:379)
    BuffStackInfo { id: 58512, intensity: false, capacity: 1 },
    // Inspiring Virtue -- BuffStackType.Queue, 99 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/GuardianHelper.cs:260)
    BuffStackInfo { id: 59592, intensity: false, capacity: 99 },
    // Harbinger Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/HarbingerHelper.cs:93)
    BuffStackInfo { id: 59964, intensity: false, capacity: 1 },
    // Pet Unleashed -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:125)
    BuffStackInfo { id: 63145, intensity: false, capacity: 1 },
    // Forest's Fortification -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:127)
    BuffStackInfo { id: 63240, intensity: false, capacity: 1 },
    // Unleashed -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:124)
    BuffStackInfo { id: 63317, intensity: false, capacity: 1 },
    // Emboldened -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/EncounterBuffs.cs:60)
    BuffStackInfo { id: 68087, intensity: true, capacity: 5 },
    // Mists-Infused Spherified Peppercorn-Spiced Oyster Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:456)
    BuffStackInfo { id: 69124, intensity: false, capacity: 1 },
    // Mists-Infused Peppercorn-Crusted Sous-Vide Steak -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:455)
    BuffStackInfo { id: 69141, intensity: false, capacity: 1 },
    // Tempestuous Aria -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:68)
    BuffStackInfo { id: 69427, intensity: false, capacity: 9 },
    // Relic of Fireworks -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:196)
    BuffStackInfo { id: 69855, intensity: false, capacity: 1 },
    // Relic of the Deadeye -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:198)
    BuffStackInfo { id: 70282, intensity: false, capacity: 1 },
    // Relic of the Weaver -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:202)
    BuffStackInfo { id: 70390, intensity: false, capacity: 1 },
    // Relic of the Thief -- BuffStackType.StackingConditionalLoss, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:195)
    BuffStackInfo { id: 70767, intensity: true, capacity: 5 },
    // Relic of the Brawler -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:194)
    BuffStackInfo { id: 70913, intensity: false, capacity: 1 },
    // Nourys's Hunger (Damage Buff) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:213)
    BuffStackInfo { id: 71431, intensity: false, capacity: 1 },
    // Relic of the Claw -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:217)
    BuffStackInfo { id: 73955, intensity: false, capacity: 1 },
    // Relic of Sorrow -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:215)
    BuffStackInfo { id: 74410, intensity: false, capacity: 1 },
    // Relic of Mount Balrior -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:218)
    BuffStackInfo { id: 74793, intensity: false, capacity: 1 },
    // Illusionary Membrane -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:286)
    BuffStackInfo { id: 76074, intensity: false, capacity: 9 },
    // Bloodstone Fervor -- BuffStackType.Stacking, 3 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:223)
    BuffStackInfo { id: 76326, intensity: true, capacity: 3 },
    // Vow of the Untamed (Biorhythm) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:133)
    BuffStackInfo { id: 76502, intensity: false, capacity: 1 },
    // Harp Playing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:62)
    BuffStackInfo { id: 76624, intensity: false, capacity: 1 },
    // Altered Chord -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:67)
    BuffStackInfo { id: 76759, intensity: false, capacity: 1 },
    // Chant of Action -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/ParagonHelper.cs:83)
    BuffStackInfo { id: 76865, intensity: false, capacity: 1 },
    // Willing Host -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:47)
    BuffStackInfo { id: 76885, intensity: false, capacity: 1 },
    // Ritualist Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/RitualistHelper.cs:93)
    BuffStackInfo { id: 76958, intensity: false, capacity: 1 },
    // Plasmatic State -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:59)
    BuffStackInfo { id: 77052, intensity: false, capacity: 1 },
    // Radiant Armaments (Hammer) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:108)
    BuffStackInfo { id: 77207, intensity: false, capacity: 1 },
    // Lute Playing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:66)
    BuffStackInfo { id: 77297, intensity: false, capacity: 1 },
    // Luminary's Blessing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:125)
    BuffStackInfo { id: 77333, intensity: false, capacity: 1 },
    // Radiant Armaments (Hammer Lingering) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:109)
    BuffStackInfo { id: 77360, intensity: false, capacity: 1 },
    // Chant of Recuperation -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/ParagonHelper.cs:84)
    BuffStackInfo { id: 77378, intensity: false, capacity: 1 },
    // Relic of the Director -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:230)
    BuffStackInfo { id: 79640, intensity: false, capacity: 1 },
];

/// The catalogued row for a buff id, if this project knows it.
pub fn stack_info(id: u32) -> Option<&'static BuffStackInfo> {
    BUFF_STACK_INFO.binary_search_by_key(&id, |b| b.id).ok().map(|i| &BUFF_STACK_INFO[i])
}

/// `BuffStackType` is intensity-stacking. Unknown ids are treated as
/// duration buffs, which is GW2EI's own default (`Buff.cs:120`).
pub fn is_intensity(id: u32) -> bool {
    stack_info(id).is_some_and(|b| b.intensity)
}

/// Every id any catalog tracker watches must be in the table, or its
/// stack simulation silently falls back to "duration, capacity 5".
#[cfg(test)]
#[test]
fn every_tracked_buff_has_a_stack_type() {
    let mut ids: Vec<u32> = super::CATALOG
        .iter()
        .flat_map(|d| d.trackers().into_iter().flat_map(|t| t.ids.iter().copied()))
        .chain(super::CATALOG.iter().flat_map(|d| d.checks.iter().filter_map(|c| c.buff_id())))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let missing: Vec<u32> = ids.into_iter().filter(|&i| stack_info(i).is_none()).collect();
    assert!(missing.is_empty(), "buff ids with no stack type: {missing:?}");
}

/// The table must stay sorted -- [`stack_info`] binary-searches it.
#[cfg(test)]
#[test]
fn table_is_sorted_by_id() {
    assert!(BUFF_STACK_INFO.windows(2).all(|w| w[0].id < w[1].id));
}
