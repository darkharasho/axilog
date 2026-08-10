//! `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs` -- the Reaper definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_ReapersShroud = 128` -- `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:64`.
pub static D128_0: DamageModifierDef = DamageModifierDef {
    id: 128,
    name: "Reaper's Shroud",
    icon: "https://render.guildwars2.com/file/795CD8A855B81908E6D37DFE039DE25762DAAAE9/1012930.png",
    description: "-33%",
    source: ModSource::Spec("Reaper"),
    spec_specific_shared: false,
    gain_per_stack: -33.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[29446], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: START_OF_LIFE,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_ReapersShroud = 128` -- `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:65`.
pub static D128_1: DamageModifierDef = DamageModifierDef {
    id: 128,
    name: "Reaper's Shroud",
    icon: "https://render.guildwars2.com/file/795CD8A855B81908E6D37DFE039DE25762DAAAE9/1012930.png",
    description: "-50%",
    source: ModSource::Spec("Reaper"),
    spec_specific_shared: false,
    gain_per_stack: -50.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[29446], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::SPvPWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: START_OF_LIFE,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_InfusingTerror = 129` -- `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:67`.
pub static D129_0: DamageModifierDef = DamageModifierDef {
    id: 129,
    name: "Infusing Terror",
    icon: "https://render.guildwars2.com/file/9D599AB94261527E79969E35AB6519D9C0396102/1012935.png",
    description: "-20%",
    source: ModSource::Spec("Reaper"),
    spec_specific_shared: false,
    gain_per_stack: -20.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[30129], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: START_OF_LIFE,
    max_gw2_build: 145038,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_InfusingTerror = 129` -- `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:69`.
pub static D129_1: DamageModifierDef = DamageModifierDef {
    id: 129,
    name: "Infusing Terror",
    icon: "https://render.guildwars2.com/file/9D599AB94261527E79969E35AB6519D9C0396102/1012935.png",
    description: "-20%",
    source: ModSource::Spec("Reaper"),
    spec_specific_shared: false,
    gain_per_stack: -20.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[30129], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::SPvPWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 145038,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_InfusingTerror = 129` -- `GW2EIEvtcParser/EIData/ProfHelpers/Necromancer/ReaperHelper.cs:71`.
pub static D129_2: DamageModifierDef = DamageModifierDef {
    id: 129,
    name: "Infusing Terror",
    icon: "https://render.guildwars2.com/file/9D599AB94261527E79969E35AB6519D9C0396102/1012935.png",
    description: "-66%",
    source: ModSource::Spec("Reaper"),
    spec_specific_shared: false,
    gain_per_stack: -66.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[30129], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 145038,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (5).
pub static DEFS: &[&DamageModifierDef] = &[
    &D128_0,
    &D128_1,
    &D129_0,
    &D129_1,
    &D129_2,
];
