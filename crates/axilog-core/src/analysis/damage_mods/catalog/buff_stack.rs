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
//! `stack_type` is GW2EI's `BuffStackType` verbatim (`ArcDPSEnums.cs:384-393`)
//! -- MBUFFSIM Task 2 promoted this field from the `intensity: bool` M16
//! carried, because rule 2's band aid (`EIData/Buffs/BuffsContainer.cs:196-252`)
//! gates on `StackingConditionalLoss` vs `Stacking` specifically. Read
//! [`crate::analysis::buffs::BuffStackType::is_intensity`] for the old
//! bool's meaning (`Queue`/`Regeneration`/`Force` are duration buffs).
//! `capacity` is GW2EI's own, used only as the fallback when the log carries
//! no `CBTS_BUFFINFO` row for the buff.

use crate::analysis::buffs::BuffStackType;

/// One row of GW2EI's buff catalog: `(id, stack_type, capacity)`.
pub struct BuffStackInfo {
    pub id: u32,
    pub stack_type: BuffStackType,
    pub capacity: u32,
}

/// Sorted by id ([`stack_info`] binary-searches it). 91 entries.
pub static BUFF_STACK_INFO: &[BuffStackInfo] = &[
    // Protection -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:18)
    BuffStackInfo { id: 717, stack_type: BuffStackType::Queue, capacity: 5 },
    // Regeneration -- BuffStackType.Regeneration, 5
    // (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:21). NOTE: GW2EI carries
    // TWO catalogue rows for Regeneration and M16 transcribed the wrong one
    // -- `CommonBuffs.cs:19` is `BuffStackType.Queue` but is build-gated to
    // `[StartOfLife, February2018Balance)`; every log this project will ever
    // see takes the `:21` row. Corrected in MBUFFSIM Task 2. Behaviourally
    // inert TODAY (both types are `BuffType.Duration`, so
    // `is_intensity()` is false either way and the simulator dispatch is
    // unchanged) -- it makes the deferred `HealingLogic` gap visible in the
    // type system instead of hidden behind a mis-citation.
    BuffStackInfo { id: 718, stack_type: BuffStackType::Regeneration, capacity: 5 },
    // Swiftness -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:26)
    BuffStackInfo { id: 719, stack_type: BuffStackType::Queue, capacity: 9 },
    // Fury -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:15)
    BuffStackInfo { id: 725, stack_type: BuffStackType::Queue, capacity: 9 },
    // Vigor -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:23)
    BuffStackInfo { id: 726, stack_type: BuffStackType::Queue, capacity: 5 },
    // Vulnerability -- BuffStackType.Stacking, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:49)
    BuffStackInfo { id: 738, stack_type: BuffStackType::Stacking, capacity: 25 },
    // Might -- BuffStackType.Stacking, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:14)
    BuffStackInfo { id: 740, stack_type: BuffStackType::Stacking, capacity: 25 },
    // Aegis -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:24)
    BuffStackInfo { id: 743, stack_type: BuffStackType::Queue, capacity: 9 },
    // Death Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/NecromancerHelper.cs:150)
    BuffStackInfo { id: 790, stack_type: BuffStackType::Force, capacity: 1 },
    // Retaliation -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:27)
    BuffStackInfo { id: 873, stack_type: BuffStackType::Queue, capacity: 5 },
    // Stability -- BuffStackType.StackingConditionalLoss, 25 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:25)
    BuffStackInfo { id: 1122, stack_type: BuffStackType::StackingConditionalLoss, capacity: 25 },
    // Quickness -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:16)
    BuffStackInfo { id: 1187, stack_type: BuffStackType::Queue, capacity: 5 },
    // Frost Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:100)
    BuffStackInfo { id: 5579, stack_type: BuffStackType::Force, capacity: 1 },
    // Fire Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:99)
    BuffStackInfo { id: 5677, stack_type: BuffStackType::Force, capacity: 1 },
    // Chaos Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:98)
    BuffStackInfo { id: 10332, stack_type: BuffStackType::Force, capacity: 1 },
    // Force of Nature -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/RangerHelper.cs:617)
    BuffStackInfo { id: 12579, stack_type: BuffStackType::Force, capacity: 1 },
    // Persisting Flames -- BuffStackType.Stacking, 10 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs:292)
    BuffStackInfo { id: 13342, stack_type: BuffStackType::Stacking, capacity: 10 },
    // Light Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:101)
    BuffStackInfo { id: 25518, stack_type: BuffStackType::Force, capacity: 1 },
    // Resistance -- BuffStackType.Queue, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:28)
    BuffStackInfo { id: 26980, stack_type: BuffStackType::Queue, capacity: 5 },
    // Reaper's Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:78)
    BuffStackInfo { id: 29446, stack_type: BuffStackType::Force, capacity: 1 },
    // Berserk -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/BerserkerHelper.cs:63)
    BuffStackInfo { id: 29502, stack_type: BuffStackType::Force, capacity: 1 },
    // Infusing Terror -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:79)
    BuffStackInfo { id: 30129, stack_type: BuffStackType::Force, capacity: 1 },
    // Alacrity -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:17)
    BuffStackInfo { id: 30328, stack_type: BuffStackType::Queue, capacity: 9 },
    // Harmonious Conduit -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:62)
    BuffStackInfo { id: 31353, stack_type: BuffStackType::Force, capacity: 1 },
    // Fractal Defensive -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:82)
    BuffStackInfo { id: 32134, stack_type: BuffStackType::Stacking, capacity: 5 },
    // Fractal Offensive -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:83)
    BuffStackInfo { id: 32473, stack_type: BuffStackType::Stacking, capacity: 5 },
    // Bowl of Mussel Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:109)
    BuffStackInfo { id: 33148, stack_type: BuffStackType::Force, capacity: 1 },
    // Writ of Masterful Strength -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:101)
    BuffStackInfo { id: 33297, stack_type: BuffStackType::Force, capacity: 1 },
    // Bowl of Curry Mussel Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:114)
    BuffStackInfo { id: 33337, stack_type: BuffStackType::Force, capacity: 1 },
    // Plate of Mussels Gnashblade -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:30)
    BuffStackInfo { id: 33476, stack_type: BuffStackType::Force, capacity: 1 },
    // Bowl of Lemongrass Mussel Pasta -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:108)
    BuffStackInfo { id: 33574, stack_type: BuffStackType::Force, capacity: 1 },
    // Writ of Masterful Malice -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/UtilityBuffs.cs:113)
    BuffStackInfo { id: 33836, stack_type: BuffStackType::Force, capacity: 1 },
    // Oysters with Pesto Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:113)
    BuffStackInfo { id: 39042, stack_type: BuffStackType::Force, capacity: 1 },
    // Oysters Gnashblade -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:111)
    BuffStackInfo { id: 39067, stack_type: BuffStackType::Force, capacity: 1 },
    // Fried Oysters -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:105)
    BuffStackInfo { id: 39302, stack_type: BuffStackType::Force, capacity: 1 },
    // Oysters With Cocktail Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:107)
    BuffStackInfo { id: 39341, stack_type: BuffStackType::Force, capacity: 1 },
    // Oysters with Zesty Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:112)
    BuffStackInfo { id: 39344, stack_type: BuffStackType::Force, capacity: 1 },
    // Fried Oyster Sandwich -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:106)
    BuffStackInfo { id: 39348, stack_type: BuffStackType::Force, capacity: 1 },
    // Oysters with Spicy Sauce -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:110)
    BuffStackInfo { id: 39500, stack_type: BuffStackType::Force, capacity: 1 },
    // Dark Aura -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:104)
    BuffStackInfo { id: 39978, stack_type: BuffStackType::Force, capacity: 1 },
    // Desert / Sandstorm Shroud -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ScourgeHelper.cs:97)
    BuffStackInfo { id: 40052, stack_type: BuffStackType::Queue, capacity: 9 },
    // Berserker's Power -- BuffStackType.Stacking, 3 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/WarriorHelper.cs:226)
    BuffStackInfo { id: 42539, stack_type: BuffStackType::Stacking, capacity: 3 },
    // Can of Stewed Oysters -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:104)
    BuffStackInfo { id: 53384, stack_type: BuffStackType::Force, capacity: 1 },
    // Soul Barbs -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/NecromancerHelper.cs:191)
    BuffStackInfo { id: 53489, stack_type: BuffStackType::Force, capacity: 1 },
    // Symbolic Avenger -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/GuardianHelper.cs:256)
    BuffStackInfo { id: 56890, stack_type: BuffStackType::Stacking, capacity: 5 },
    // Peppercorn-Crusted Sous-Vide Steak -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:409)
    BuffStackInfo { id: 57051, stack_type: BuffStackType::Force, capacity: 1 },
    // Spiced Pepper Creme Brulee -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:446)
    BuffStackInfo { id: 57067, stack_type: BuffStackType::Force, capacity: 1 },
    // Plate of Peppercorn-Spiced Beef Carpaccio -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:423)
    BuffStackInfo { id: 57114, stack_type: BuffStackType::Force, capacity: 1 },
    // Peppered Cured Meat Flatbread -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:411)
    BuffStackInfo { id: 57127, stack_type: BuffStackType::Force, capacity: 1 },
    // Spiced Peppercorn Cheesecake -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:447)
    BuffStackInfo { id: 57129, stack_type: BuffStackType::Force, capacity: 1 },
    // Plate of Peppered Clear Truffle Ravioli -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:426)
    BuffStackInfo { id: 57155, stack_type: BuffStackType::Force, capacity: 1 },
    // Spherified Peppercorn-Spiced Oyster Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:444)
    BuffStackInfo { id: 57165, stack_type: BuffStackType::Force, capacity: 1 },
    // Peppercorn-Spiced Eggs Benedict -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:410)
    BuffStackInfo { id: 57210, stack_type: BuffStackType::Force, capacity: 1 },
    // Plate of Peppercorn-Spiced Coq Au Vin -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:424)
    BuffStackInfo { id: 57260, stack_type: BuffStackType::Force, capacity: 1 },
    // Bowl of Spiced Fruit Salad -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:393)
    BuffStackInfo { id: 57276, stack_type: BuffStackType::Force, capacity: 1 },
    // Plate of Peppercorn-Spiced Poultry Aspic -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:425)
    BuffStackInfo { id: 57299, stack_type: BuffStackType::Force, capacity: 1 },
    // Peppercorn and Veggie Flatbread -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:408)
    BuffStackInfo { id: 57382, stack_type: BuffStackType::Force, capacity: 1 },
    // Weight of the World -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:379)
    BuffStackInfo { id: 58512, stack_type: BuffStackType::Force, capacity: 1 },
    // Inspiring Virtue -- BuffStackType.Queue, 99 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/GuardianHelper.cs:260)
    BuffStackInfo { id: 59592, stack_type: BuffStackType::Queue, capacity: 99 },
    // Harbinger Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/HarbingerHelper.cs:93)
    BuffStackInfo { id: 59964, stack_type: BuffStackType::Force, capacity: 1 },
    // Pet Unleashed -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:125)
    BuffStackInfo { id: 63145, stack_type: BuffStackType::Force, capacity: 1 },
    // Forest's Fortification -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:127)
    BuffStackInfo { id: 63240, stack_type: BuffStackType::Force, capacity: 1 },
    // Unleashed -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:124)
    BuffStackInfo { id: 63317, stack_type: BuffStackType::Force, capacity: 1 },
    // Emboldened -- BuffStackType.Stacking, 5 (GW2EIEvtcParser/EIData/Buffs/EncounterBuffs.cs:60)
    BuffStackInfo { id: 68087, stack_type: BuffStackType::Stacking, capacity: 5 },
    // Mists-Infused Spherified Peppercorn-Spiced Oyster Soup -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:456)
    BuffStackInfo { id: 69124, stack_type: BuffStackType::Force, capacity: 1 },
    // Mists-Infused Peppercorn-Crusted Sous-Vide Steak -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/FoodBuffs.cs:455)
    BuffStackInfo { id: 69141, stack_type: BuffStackType::Force, capacity: 1 },
    // Tempestuous Aria -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:68)
    BuffStackInfo { id: 69427, stack_type: BuffStackType::Queue, capacity: 9 },
    // Relic of Fireworks -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:196)
    BuffStackInfo { id: 69855, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of the Deadeye -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:198)
    BuffStackInfo { id: 70282, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of the Weaver -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:202)
    BuffStackInfo { id: 70390, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of the Thief -- BuffStackType.StackingConditionalLoss, 5 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:195)
    BuffStackInfo { id: 70767, stack_type: BuffStackType::StackingConditionalLoss, capacity: 5 },
    // Relic of the Brawler -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:194)
    BuffStackInfo { id: 70913, stack_type: BuffStackType::Force, capacity: 1 },
    // Nourys's Hunger (Damage Buff) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:213)
    BuffStackInfo { id: 71431, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of the Claw -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:217)
    BuffStackInfo { id: 73955, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of Sorrow -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:215)
    BuffStackInfo { id: 74410, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of Mount Balrior -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:218)
    BuffStackInfo { id: 74793, stack_type: BuffStackType::Force, capacity: 1 },
    // Illusionary Membrane -- BuffStackType.Queue, 9 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:286)
    BuffStackInfo { id: 76074, stack_type: BuffStackType::Queue, capacity: 9 },
    // Bloodstone Fervor -- BuffStackType.Stacking, 3 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:223)
    BuffStackInfo { id: 76326, stack_type: BuffStackType::Stacking, capacity: 3 },
    // Vow of the Untamed (Biorhythm) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Ranger/UntamedHelper.cs:133)
    BuffStackInfo { id: 76502, stack_type: BuffStackType::Force, capacity: 1 },
    // Harp Playing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:62)
    BuffStackInfo { id: 76624, stack_type: BuffStackType::Force, capacity: 1 },
    // Altered Chord -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:67)
    BuffStackInfo { id: 76759, stack_type: BuffStackType::Force, capacity: 1 },
    // Chant of Action -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/ParagonHelper.cs:83)
    BuffStackInfo { id: 76865, stack_type: BuffStackType::Force, capacity: 1 },
    // Willing Host -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:47)
    BuffStackInfo { id: 76885, stack_type: BuffStackType::Force, capacity: 1 },
    // Ritualist Shroud -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/RitualistHelper.cs:93)
    BuffStackInfo { id: 76958, stack_type: BuffStackType::Force, capacity: 1 },
    // Plasmatic State -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:59)
    BuffStackInfo { id: 77052, stack_type: BuffStackType::Force, capacity: 1 },
    // Radiant Armaments (Hammer) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:108)
    BuffStackInfo { id: 77207, stack_type: BuffStackType::Force, capacity: 1 },
    // Lute Playing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/TroubadourHelper.cs:66)
    BuffStackInfo { id: 77297, stack_type: BuffStackType::Force, capacity: 1 },
    // Luminary's Blessing -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:125)
    BuffStackInfo { id: 77333, stack_type: BuffStackType::Force, capacity: 1 },
    // Radiant Armaments (Hammer Lingering) -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Guardian/LuminaryHelper.cs:109)
    BuffStackInfo { id: 77360, stack_type: BuffStackType::Force, capacity: 1 },
    // Chant of Recuperation -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/ProfHelpers/Warrior/ParagonHelper.cs:84)
    BuffStackInfo { id: 77378, stack_type: BuffStackType::Force, capacity: 1 },
    // Relic of the Director -- BuffStackType.Force, 1 (GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:230)
    BuffStackInfo { id: 79640, stack_type: BuffStackType::Force, capacity: 1 },
];

/// The catalogued row for a buff id, if this project knows it.
pub fn stack_info(id: u32) -> Option<&'static BuffStackInfo> {
    BUFF_STACK_INFO.binary_search_by_key(&id, |b| b.id).ok().map(|i| &BUFF_STACK_INFO[i])
}

/// `BuffStackType` is intensity-stacking. Unknown ids are treated as
/// duration buffs, which is GW2EI's own default (`Buff.cs:120`).
pub fn is_intensity(id: u32) -> bool {
    stack_info(id).is_some_and(|b| b.stack_type.is_intensity())
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
