//! `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs` -- the Amalgam definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_WillingHost_StrikeCondition = 371` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:24`.
pub static D371_0: DamageModifierDef = DamageModifierDef {
    id: 371,
    name: "Willing Host",
    icon: "https://render.guildwars2.com/file/49DEE2C90E04E876DE2517C201060E330A7D9FDE/3679954.png",
    description: "15%",
    source: ModSource::Spec("Amalgam"),
    spec_specific_shared: false,
    gain_per_stack: 15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[76885], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 186019,
    max_gw2_build: 190000,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_WillingHost_StrikeCondition = 371` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:34`.
pub static D371_1: DamageModifierDef = DamageModifierDef {
    id: 371,
    name: "Willing Host",
    icon: "https://render.guildwars2.com/file/49DEE2C90E04E876DE2517C201060E330A7D9FDE/3679954.png",
    description: "10%",
    source: ModSource::Spec("Amalgam"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[76885], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
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

/// GW2EI `Mod_PlasmaticState = 372` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:36`.
pub static D372_0: DamageModifierDef = DamageModifierDef {
    id: 372,
    name: "Plasmatic State",
    icon: "https://render.guildwars2.com/file/2AA6D1CCEE723B7415B60DBDBAB55275EA562D29/3680138.png",
    description: "15%",
    source: ModSource::Spec("Amalgam"),
    spec_specific_shared: false,
    gain_per_stack: 15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[77052], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 186019,
    max_gw2_build: 190000,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_PlasmaticState = 372` -- `GW2EIEvtcParser/EIData/ProfHelpers/Engineer/AmalgamHelper.cs:38`.
pub static D372_1: DamageModifierDef = DamageModifierDef {
    id: 372,
    name: "Plasmatic State",
    icon: "https://render.guildwars2.com/file/2AA6D1CCEE723B7415B60DBDBAB55275EA562D29/3680138.png",
    description: "7%",
    source: ModSource::Spec("Amalgam"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[77052], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 190000,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (4).
pub static DEFS: &[&DamageModifierDef] = &[
    &D371_0,
    &D371_1,
    &D372_0,
    &D372_1,
];
