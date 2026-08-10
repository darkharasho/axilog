//! `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs` -- the Tempest definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_TranscendentTempest = 74` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:29`.
pub static D74_0: DamageModifierDef = DamageModifierDef {
    id: 74,
    name: "Transcendent Tempest",
    icon: "https://render.guildwars2.com/file/0C79A761B446E53CAB6DA4B320D12D656B5E1151/2207773.png",
    description: "7% after overload",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[31353], multi: false }, from_foe: false },
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
    min_gw2_build: 99526,
    max_gw2_build: 133322,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_TranscendentTempest = 74` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:31`.
pub static D74_1: DamageModifierDef = DamageModifierDef {
    id: 74,
    name: "Transcendent Tempest",
    icon: "https://render.guildwars2.com/file/0C79A761B446E53CAB6DA4B320D12D656B5E1151/2207773.png",
    description: "7% after overload",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[31353], multi: false }, from_foe: false },
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
    min_gw2_build: 133322,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_TranscendentTempest = 74` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:33`.
pub static D74_2: DamageModifierDef = DamageModifierDef {
    id: 74,
    name: "Transcendent Tempest",
    icon: "https://render.guildwars2.com/file/0C79A761B446E53CAB6DA4B320D12D656B5E1151/2207773.png",
    description: "15% after overload",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[31353], multi: false }, from_foe: false },
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
    min_gw2_build: 133322,
    max_gw2_build: 150431,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_TranscendentTempest = 74` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:35`.
pub static D74_3: DamageModifierDef = DamageModifierDef {
    id: 74,
    name: "Transcendent Tempest",
    icon: "https://render.guildwars2.com/file/0C79A761B446E53CAB6DA4B320D12D656B5E1151/2207773.png",
    description: "25% after overload",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 25.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[31353], multi: false }, from_foe: false },
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
    min_gw2_build: 150431,
    max_gw2_build: 198816,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_TempestuousAria = 75` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:42`.
pub static D75_0: DamageModifierDef = DamageModifierDef {
    id: 75,
    name: "Tempestuous Aria",
    icon: "https://render.guildwars2.com/file/0C2A72B848A7FBB596DE6096B4C45814D3266EEB/1029946.png",
    description: "7% after giving aura",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[69427], multi: false }, from_foe: false },
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
    min_gw2_build: 147734,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_TempestuousAria = 75` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:44`.
pub static D75_1: DamageModifierDef = DamageModifierDef {
    id: 75,
    name: "Tempestuous Aria",
    icon: "https://render.guildwars2.com/file/0C2A72B848A7FBB596DE6096B4C45814D3266EEB/1029946.png",
    description: "10% after giving aura",
    source: ModSource::Spec("Tempest"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[69427], multi: false }, from_foe: false },
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
    min_gw2_build: 147734,
    max_gw2_build: 159951,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_HardyConduit = 78` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/TempestHelper.cs:55`.
pub static D78_0: DamageModifierDef = DamageModifierDef {
    id: 78,
    name: "Hardy Conduit",
    icon: "https://render.guildwars2.com/file/F61B0203A07CDF17A3613731EE93F6C09410E825/1029953.png",
    description: "20% extra protection effectiveness",
    source: ModSource::Spec("Tempest"),
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

/// Every definition in this group (7).
pub static DEFS: &[&DamageModifierDef] = &[
    &D74_0,
    &D74_1,
    &D74_2,
    &D74_3,
    &D75_0,
    &D75_1,
    &D78_0,
];
