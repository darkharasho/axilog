//! `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/EngineerHelper.cs` -- the Engineer definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_ExcessiveEnergy = 98` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/EngineerHelper.cs:254`.
pub static D98_0: DamageModifierDef = DamageModifierDef {
    id: 98,
    name: "Excessive Energy",
    icon: "https://render.guildwars2.com/file/B9CAA4643E9B7BD8CF7D61F67CB7C2C8F3FCEE07/1012389.png",
    description: "10% under vigor",
    source: ModSource::Spec("Engineer"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[726], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::All,
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

/// GW2EI `Mod_OverShield = 99` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/EngineerHelper.cs:263`.
pub static D99_0: DamageModifierDef = DamageModifierDef {
    id: 99,
    name: "Over Shield",
    icon: "https://render.guildwars2.com/file/F90CB6E3E23B6ECB761C062D92507799694CF4D2/1012366.png",
    description: "20% extra protection effectiveness",
    source: ModSource::Spec("Engineer"),
    spec_specific_shared: false,
    gain_per_stack: -9.85074626865673,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[717], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
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
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (2).
pub static DEFS: &[&DamageModifierDef] = &[
    &D98_0,
    &D99_0,
];
