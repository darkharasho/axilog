//! The damage-modifier DEFINITION table (M16).
//!
//! **Task 1 deliberately ships exactly ONE definition.** The full WvW-
//! relevant catalog (the ~40 shared/gear/item modifiers plus the per-spec
//! tables the reference export's `damageModMap` shows) is Task 2's job;
//! this single entry exists to prove the plumbing end-to-end -- model ->
//! engine -> calibration against a real export -- before a large hand-
//! transcribed table lands on top of it.
//!
//! `Moving Bonus` was chosen because it is the simplest possible real
//! definition: no buff state, no minions, no era gating, no approximation,
//! a single per-hit flag that arcdps writes directly onto the damage row.
//! If its four fields match the reference export, the eligibility rule, the
//! gain formula, the rounding and the denominator selection are all
//! demonstrably right.

use super::model::*;

/// GW2EI `Mod_MovingBonus = 10`
/// (`GW2EIEvtcParser/ParserHelpers/IDs/DamageModifierIDs.cs:23`) -> JSON
/// key `"d10"`.
///
/// Definition, verbatim from
/// `GW2EIEvtcParser/EIData/DamageModifiers/CommonDamageModifiers/
/// ItemDamageModifiers.cs:13`:
///
/// ```text
/// new DamageLogDamageModifier(Mod_MovingBonus, "Moving Bonus",
///     "Seaweed Salad (and the likes) – 5% while moving",
///     DamageSource.NoPets, 5.0, DamageType.Strike, DamageType.Strike,
///     Source.Item, ItemImages.BowlOfSeaweedSalad,
///     (x, log) => x.IsMoving, DamageModifierMode.All),
/// ```
///
/// Consequences of it being a `DamageLogDamageModifier`
/// (`DamageModifierDescriptors/DamageLogDamageModifier.cs`): the gain
/// computer is hardcoded [`GainComputer::ByPresence`] (`:10`) and the gain
/// is evaluated ONCE at `stack = 1` (`:15-18`), i.e. a constant
/// `5.0 / 105.0 = 0.047619047619...` for every qualifying hit; no buff
/// timeline is ever consulted.
///
/// The `icon` is GW2EI's `ItemImages.BowlOfSeaweedSalad`; the URL below is
/// the one the reference export actually emits for `damageModMap.d10.icon`.
pub static MOVING_BONUS: DamageModifierDef = DamageModifierDef {
    id: 10,
    name: "Moving Bonus",
    icon: "https://render.guildwars2.com/file/0D442C30D4E29832725800E22990BA111D05E0BE/219455.png",
    // The en dash is GW2EI's own (`–`, U+2013), reproduced verbatim so the
    // composed tooltip matches byte-for-byte.
    description: "Seaweed Salad (and the likes) – 5% while moving",
    source: ModSource::Item,
    gain_per_stack: 5.0,
    gain: GainComputer::ByPresence,
    trigger: Trigger::Hit,
    src_type: DamageType::Strike,
    compare_type: DamageType::Strike,
    dmg_src: DamageSource::NoPets,
    checks: &[HitCheck::SrcMoving],
    mode: ModifierMode::All,
    approximate: false,
    is_counter: false,
    min_gw2_build: START_OF_LIFE,
    max_gw2_build: END_OF_LIFE,
    min_evtc_build: EVTC_START_OF_LIFE,
    max_evtc_build: EVTC_END_OF_LIFE,
};

/// Every definition this project currently knows about (Task 1: one).
///
/// Ordering is the engine's iteration order and therefore part of its
/// determinism contract -- but the engine's output is a `BTreeMap` keyed by
/// `(player, id)`, so catalog order cannot affect results either way.
pub static CATALOG: &[&DamageModifierDef] = &[&MOVING_BONUS];
