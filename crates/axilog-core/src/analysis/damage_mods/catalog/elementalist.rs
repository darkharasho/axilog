//! `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs` -- the Elementalist definitions observed in the WvW reference capture.
//!
//! Machine-transcribed from GW2EI; every entry carries the `file:line` of
//! the C# statement it came from. See `super`'s module doc for the
//! transcription rules and the skipped-definition list.

#![allow(clippy::excessive_precision)]

use super::super::model::*;

/// GW2EI `Mod_PersistingFlames = 67` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs:102`.
pub static D67_0: DamageModifierDef = DamageModifierDef {
    id: 67,
    name: "Persisting Flames",
    icon: "https://render.guildwars2.com/file/38496307B7C27BCB08D6800F2A699B4998DD1935/1012312.png",
    description: "2% per stack",
    source: ModSource::Spec("Elementalist"),
    spec_specific_shared: false,
    gain_per_stack: 2.0,
    gain: GainComputer::ByStack,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[13342], multi: false }, from_foe: false },
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
    min_gw2_build: 175086,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_PersistingFlames = 67` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs:104`.
pub static D67_1: DamageModifierDef = DamageModifierDef {
    id: 67,
    name: "Persisting Flames",
    icon: "https://render.guildwars2.com/file/38496307B7C27BCB08D6800F2A699B4998DD1935/1012312.png",
    description: "1% per stack",
    source: ModSource::Spec("Elementalist"),
    spec_specific_shared: false,
    gain_per_stack: 1.0,
    gain: GainComputer::ByStack,
    trigger: Trigger::BuffOnActor { tracker: BuffTracker { ids: &[13342], multi: false }, from_foe: false },
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
    min_gw2_build: 104844,
    max_gw2_build: 175086,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// GW2EI `Mod_BoltToTheHeart = 11` -- `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/ElementalistHelper.cs:121`.
pub static D11_0: DamageModifierDef = DamageModifierDef {
    id: 11,
    name: "Bolt to the Heart",
    icon: "https://render.guildwars2.com/file/3E4E727D6D23DFEF4205F272A15E2136799DE291/1012276.png",
    description: "20% if target <50% HP",
    source: ModSource::Spec("Elementalist"),
    spec_specific_shared: false,
    gain_per_stack: 20.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::Hit,
    src_type: DamageType::Strike,
    compare_type: DamageType::All,
    dmg_src: DamageSource::NoPets,
    checks: &[HitCheck::AgainstUnderFifty],
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

/// Every definition in this group (3).
pub static DEFS: &[&DamageModifierDef] = &[
    &D67_0,
    &D67_1,
    &D11_0,
];
