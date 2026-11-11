//! `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/ConduitHelper.cs` -- the Conduit definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_ShieldingHands = 373` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/ConduitHelper.cs:43`.
pub static D373_0: DamageModifierDef = DamageModifierDef {
    id: 373,
    name: "Shielding Hands",
    icon: "https://render.guildwars2.com/file/274EEBFD624D0DA55A0C7F7566D41A060E48B4BA/3680199.png",
    description: "-75%",
    source: ModSource::Spec("Conduit"),
    spec_specific_shared: false,
    gain_per_stack: -75.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[77322], multi: false }, from_foe: false },
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
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_DeterminedResolution_NuminousGift = 409` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/ConduitHelper.cs:45`.
pub static D409_0: DamageModifierDef = DamageModifierDef {
    id: 409,
    name: "Determined Resolution (Numinous Gift)",
    icon: "https://render.guildwars2.com/file/F3DB5663A9C9C8630247D463050B05A12C69E542/1012699.png",
    description: "-12% under resolution",
    source: ModSource::Spec("Conduit"),
    spec_specific_shared: false,
    gain_per_stack: -12.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[873], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::SPvP,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 186019,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_DeterminedResolution_NuminousGift = 409` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/ConduitHelper.cs:47`.
pub static D409_1: DamageModifierDef = DamageModifierDef {
    id: 409,
    name: "Determined Resolution (Numinous Gift)",
    icon: "https://render.guildwars2.com/file/F3DB5663A9C9C8630247D463050B05A12C69E542/1012699.png",
    description: "-15% under resolution",
    source: ModSource::Spec("Conduit"),
    spec_specific_shared: false,
    gain_per_stack: -15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[873], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::PvEWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 186019,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (3).
pub static DEFS: &[&DamageModifierDef] = &[
    &D373_0,
    &D409_0,
    &D409_1,
];
