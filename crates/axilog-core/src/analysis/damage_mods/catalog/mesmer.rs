//! `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs` -- the Mesmer definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:225`.
pub static D119_0: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "10% under regeneration",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[718], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 115190,
    max_gw2_build: 154949,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:227`.
pub static D119_1: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "10% under chaos aura",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[10332], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 154949,
    max_gw2_build: 157732,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:229`.
pub static D119_2: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "10% under chaos aura",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[10332], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::SPvPWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 157732,
    max_gw2_build: 178947,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:231`.
pub static D119_3: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "7% under chaos aura",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[10332], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 157732,
    max_gw2_build: 178947,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:233`.
pub static D119_4: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "10%",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 10.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[76074], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::SPvPWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 178947,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_IllusionaryMembrane = 119` -- `GW2EIEvtcParser/EIData/ProfHelpers/Mesmer/MesmerHelper.cs:235`.
pub static D119_5: DamageModifierDef = DamageModifierDef {
    id: 119,
    name: "Illusionary Membrane",
    icon: "https://render.guildwars2.com/file/0DF6A27A24B01D069DCD7609ADD305C7C557A82A/1012472.png",
    description: "7%",
    source: ModSource::Spec("Mesmer"),
    spec_specific_shared: false,
    gain_per_stack: 7.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[76074], multi: false }, from_foe: false },
    src_type: DamageType::Condition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 178947,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (6).
pub static DEFS: &[&DamageModifierDef] = &[
    &D119_0,
    &D119_1,
    &D119_2,
    &D119_3,
    &D119_4,
    &D119_5,
];
