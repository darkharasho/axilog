//! `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs` -- the Vindicator definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_ForerunnerOfDeath = 159` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:33`.
pub static D159_0: DamageModifierDef = DamageModifierDef {
    id: 159,
    name: "Forerunner of Death",
    icon: "https://render.guildwars2.com/file/2864D963D3FC9156E6F52FA95DD34C2DE30306BE/2491537.png",
    description: "15%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: 15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62811], multi: false }, from_foe: false },
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
    min_gw2_build: 119939,
    max_gw2_build: 151966,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_ForerunnerOfDeath = 159` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:35`.
pub static D159_1: DamageModifierDef = DamageModifierDef {
    id: 159,
    name: "Forerunner of Death",
    icon: "https://render.guildwars2.com/file/2864D963D3FC9156E6F52FA95DD34C2DE30306BE/2491537.png",
    description: "15%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: 15.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62811], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::SPvPWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 151966,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_ForerunnerOfDeath = 159` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:37`.
pub static D159_2: DamageModifierDef = DamageModifierDef {
    id: 159,
    name: "Forerunner of Death",
    icon: "https://render.guildwars2.com/file/2864D963D3FC9156E6F52FA95DD34C2DE30306BE/2491537.png",
    description: "25%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: 25.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62811], multi: false }, from_foe: false },
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[],
    mode: ModifierMode::PvE,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 151966,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_UrnOfSaintViktor = 160` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:44`.
pub static D160_0: DamageModifierDef = DamageModifierDef {
    id: 160,
    name: "Urn of Saint Viktor",
    icon: "https://render.guildwars2.com/file/317EB806A8487B77040BC0C673A59096F396EEE4/2491625.png",
    description: "-50%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: -50.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62864], multi: false }, from_foe: false },
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
    max_gw2_build: 163141,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_UrnOfSaintViktor = 160` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:46`.
pub static D160_1: DamageModifierDef = DamageModifierDef {
    id: 160,
    name: "Urn of Saint Viktor",
    icon: "https://render.guildwars2.com/file/317EB806A8487B77040BC0C673A59096F396EEE4/2491625.png",
    description: "-33%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: -33.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62864], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::SPvP,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 163141,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_UrnOfSaintViktor = 160` -- `GW2EIEvtcParser/EIData/ProfHelpers/Revenant/VindicatorHelper.cs:48`.
pub static D160_2: DamageModifierDef = DamageModifierDef {
    id: 160,
    name: "Urn of Saint Viktor",
    icon: "https://render.guildwars2.com/file/317EB806A8487B77040BC0C673A59096F396EEE4/2491625.png",
    description: "-50%",
    source: ModSource::Spec("Vindicator"),
    spec_specific_shared: false,
    gain_per_stack: -50.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[62864], multi: false }, from_foe: false },
    src_type: DamageType::StrikeAndCondition,
    compare_type: DamageType::All,
    dmg_src: DamageSource::Incoming,
    checks: &[],
    mode: ModifierMode::PvEWvW,
    approximate: false,
    is_counter: false,
    actor_always_master: false,
    foe_always_master: false,
    with_absorbed_damage_events: false,
    min_gw2_build: 163141,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition in this group (6).
pub static DEFS: &[&DamageModifierDef] = &[
    &D159_0,
    &D159_1,
    &D159_2,
    &D160_0,
    &D160_1,
    &D160_2,
];
