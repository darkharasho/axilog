//! The damage-modifier DEFINITION table (M16, Task 2).
//!
//! **Generated** by `scripts/gen_damage_mod_catalog.py`; re-run it and
//! `git diff` to re-verify against a GW2EI checkout. Hand edits are lost.
//!
//! # What is in here
//!
//! GW2EI's definition set, grouped exactly as GW2EI groups it: the three
//! shared tables (`CommonDamageModifiers/{Item,Gear,Shared}DamageModifiers.cs`)
//! are transcribed COMPLETE -- every WvW-expressible member, observed in
//! the reference capture or not -- and the per-profession files contribute
//! the definitions whose ids appear in that capture's `damageModMap`.
//!
//! **239 statements considered = 205 transcribed + 34 skipped.** Nothing is dropped silently; the skipped table
//! below is exhaustive and each row carries every reason that applies.
//!
//! Every entry cites the `file:line` of the C# statement it came from, and
//! carries GW2EI's own era windows verbatim: a reworked trait is several
//! entries sharing one id with disjoint `[min, max)` build ranges and/or
//! disjoint modes, exactly as upstream writes it. Selecting between them is
//! `DamageModifierDef::available` + `DamageModifierDef::keep`, not this
//! table -- so the catalog is era-agnostic and a pre-rework log picks its
//! own variant. `no_ambiguous_definition_for_any_build_and_mode` proves
//! the windows really are disjoint.
//!
//! # Transcription rules
//!
//! - `CounterOn{Actor,Foe}DamageModifier` passes a hardcoded `gainPerStack`
//!   of `100.0` to its base ctor (`CounterOnActorDamageModifier.cs:9-16`),
//!   so counters are transcribed with `gain_per_stack: 100.0` +
//!   `is_counter: true`, never with the "percent" the name suggests.
//! - `SkillDamageModifier` passes `int.MaxValue`
//!   (`SkillDamageModifier.cs:27`) -- a sentinel, since its gain is
//!   hardcoded to `1.0`; transcribed literally so nothing silently trips
//!   `DamageModifierDef::validate`'s non-zero rule.
//! - `NumberOfBoons` (`SkillIDs.cs:20`, id `-3`) is not a real buff: it is
//!   the PRESENCE MERGE of every `BuffClassification.Boon` graph
//!   (`SingleActorBuffsHelper.cs:963-1040`, `MergePresenceInto`). A
//!   `BuffsTrackerSingle` over it therefore returns "how many distinct
//!   boons are up", which is EXACTLY what a `BuffTracker` with
//!   `multi: true` over the twelve boon ids computes
//!   (`BuffsTrackerMulti.cs:7-15`) -- so those definitions are transcribed
//!   as multi trackers rather than needing a synthetic graph.
//! - GW2EI's `Source` enum only decides which spec a modifier is offered
//!   to; the engine never branches on it, so it is carried as a label
//!   (`ModSource`).
//!
//! # Deliberately NOT transcribed
//!
//! Each line is a GW2EI statement this project cannot evaluate faithfully.
//! None is silently approximated -- the definition is simply absent, and
//! ALL of the reasons that apply to it are recorded here:
//!
//! | id | symbol | file:line | why |
//! | --- | --- | --- | --- |
//! | 5 | `Mod_ThiefRune` | `GearDamageModifiers.cs:25` | checker `(x, log) => x.IsFlanking \|\| x.To.GetCurrentBreakbarState(log, x.Time) != BreakbarState.None` |
//! | 8 | `Mod_RuneOfMercy` | `GearDamageModifiers.cs:95` | checker `(x, log) => log.CombatData.GetAnimatedCastData(Resurrect).Any(y => y.Caster.Is(x.To) && y.IntersectsActualCastWindow(x.Time, 0))` |
//! | 9 | `Mod_RuneOfTheScrapper` | `GearDamageModifiers.cs:97` | checker `(x,log) => ProfHelper.TargetOutsideRangeChecker(x, log, 600)` |
//! | 25 | `Mod_HuntersTactics` | `RangerHelper.cs:480` | checker `(x, log) => x.IsFlanking \|\| x.To.GetCurrentBreakbarState(log, x.Time) != BreakbarState.None` |
//! | 25 | `Mod_HuntersTactics` | `RangerHelper.cs:482` | checker `(x, log) => x.IsFlanking \|\| x.To.GetCurrentBreakbarState(log, x.Time) != BreakbarState.None` |
//! | 57 | `Mod_Vulnerability` | `SharedDamageModifiers.cs:57` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 58 | `Mod_Protection` | `SharedDamageModifiers.cs:40` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 59 | `Mod_Resolution` | `SharedDamageModifiers.cs:41` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 72 | `Mod_StoneFlesh` | `ElementalistHelper.cs:220` | EI-synthetic buff id FireEarthAttunement = -7 (a computed graph, not a wire buff) |
//! | 87 | `Mod_HuntersFortification` | `DragonhunterHelper.cs:110` | NumberOfConditions pseudo-buff graph (no condition-buff id table in this project) |
//! | 215 | `Mod_Distortion` | `MesmerHelper.cs:242` | UsingHitAndAbsorbedDamageEvents (absorbed hits not classified) |
//! | 215 | `Mod_Distortion` | `MesmerHelper.cs:245` | UsingHitAndAbsorbedDamageEvents (absorbed hits not classified) |
//! | 219 | `Mod_FlameLegionRune` | `GearDamageModifiers.cs:31` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 220 | `Mod_SpellbreakerRune` | `GearDamageModifiers.cs:33` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 221 | `Mod_IceRune` | `GearDamageModifiers.cs:35` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 222 | `Mod_RuneOfMesmer` | `GearDamageModifiers.cs:39` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 223 | `Mod_ImpactSigil` | `GearDamageModifiers.cs:42` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 224 | `Mod_RelicOfTheDragonhunter` | `GearDamageModifiers.cs:44` | WithBuffOnFoeFromActor (per-applier stacks not modelled); UsingEarlyExit (early-exit actor checker not modelled); BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 225 | `Mod_RelicOfIsgarren` | `GearDamageModifiers.cs:48` | WithBuffOnFoeFromActor (per-applier stacks not modelled); UsingEarlyExit (early-exit actor checker not modelled); BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 226 | `Mod_RelicOfPeitha` | `GearDamageModifiers.cs:52` | WithBuffOnFoeFromActor (per-applier stacks not modelled); UsingEarlyExit (early-exit actor checker not modelled); BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 227 | `Mod_RuneOfPerplexity` | `GearDamageModifiers.cs:100` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 229 | `Mod_FlowLikeWater10` | `ElementalistHelper.cs:166` | checker `FromHPChecker(50)` |
//! | 252 | `Mod_ExposedStrike` | `SharedDamageModifiers.cs:45` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 252 | `Mod_ExposedStrike` | `SharedDamageModifiers.cs:49` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 253 | `Mod_OldExposedStrike` | `SharedDamageModifiers.cs:53` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 254 | `Mod_ExposedCondition` | `SharedDamageModifiers.cs:47` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 254 | `Mod_ExposedCondition` | `SharedDamageModifiers.cs:51` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 255 | `Mod_OldExposedCondition` | `SharedDamageModifiers.cs:55` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 256 | `Mod_Exposed` | `SharedDamageModifiers.cs:43` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//! | 327 | `Mod_BeastlyWarden_Pet` | `RangerHelper.cs:502` | checker `(x, log) => IsJuvenileUrsinePet(x.From) \|\| IsJuvenilePorcinePet(x.From)`; UsingEarlyExit (early-exit actor checker not modelled) |
//! | 327 | `Mod_BeastlyWarden_Pet` | `RangerHelper.cs:507` | checker `(x, log) => IsJuvenileUrsinePet(x.From) \|\| IsJuvenilePorcinePet(x.From)`; UsingEarlyExit (early-exit actor checker not modelled) |
//! | 327 | `Mod_BeastlyWarden_Pet` | `RangerHelper.cs:512` | checker `(x, log) => IsJuvenileUrsinePet(x.From) \|\| IsJuvenilePorcinePet(x.From)`; UsingEarlyExit (early-exit actor checker not modelled) |
//! | 328 | `Mod_EmpoweredIllusions` | `MesmerHelper.cs:154` | checker `IllusionsChecker`; UsingEarlyExit (early-exit actor checker not modelled) |
//! | 374 | `Mod_RadiantArmamentsHammer` | `LuminaryHelper.cs:56` | BuffOnFoe family: GW2EI drops it outright in WvW/sPvP (BuffOnFoeDamageModifier.cs:83-91) -- definitionally inert here |
//!
//! The `BuffOnFoe` rows are not a gap in this port: GW2EI itself returns
//! `false` from `BuffOnFoeDamageModifier.Keep` for every WvW and sPvP log
//! before consulting anything else (`:83-91`), so those modifiers are
//! definitionally inert in this project's only parse mode. They are listed
//! for completeness, and transcribing them would add dead entries that
//! `DamageModifierDef::keep` drops anyway.

pub mod buff_stack;
pub mod amalgam;
pub mod berserker;
pub mod elementalist;
pub mod engineer;
pub mod gear;
pub mod guardian;
pub mod item;
pub mod luminary;
pub mod mesmer;
pub mod necromancer;
pub mod paragon;
pub mod ranger;
pub mod reaper;
pub mod shared;
pub mod tempest;
pub mod troubadour;
pub mod untamed;
pub mod warrior;

/// GW2EI `Mod_MovingBonus = 10` -- kept under its Task 1 name because the
/// calibration harness and several unit tests refer to it by that name.
pub use item::D10_0 as MOVING_BONUS;

use super::model::DamageModifierDef;

/// Every definition this project knows about.
///
/// Ordering is the engine's iteration order and therefore part of its
/// determinism contract -- but the engine's output is a `BTreeMap` keyed by
/// `(player, id)`, so catalog order cannot affect results either way.
pub static CATALOG: &[&DamageModifierDef] = &[
    &amalgam::D371_0,
    &amalgam::D371_1,
    &amalgam::D372_0,
    &amalgam::D372_1,
    &berserker::D170_0,
    &berserker::D170_1,
    &berserker::D170_2,
    &berserker::D170_3,
    &berserker::D170_4,
    &berserker::D170_5,
    &berserker::D170_6,
    &berserker::D170_7,
    &berserker::D170_8,
    &elementalist::D67_0,
    &elementalist::D67_1,
    &elementalist::D11_0,
    &engineer::D98_0,
    &engineer::D99_0,
    &gear::D3_0,
    &gear::D3_1,
    &gear::D4_0,
    &gear::D4_1,
    &gear::D5_0,
    &gear::D41_0,
    &gear::D42_0,
    &gear::D43_0,
    &gear::D44_0,
    &gear::D312_0,
    &gear::D312_1,
    &gear::D312_2,
    &gear::D46_0,
    &gear::D47_0,
    &gear::D48_0,
    &gear::D45_0,
    &gear::D45_1,
    &gear::D49_0,
    &gear::D314_0,
    &gear::D318_0,
    &gear::D318_1,
    &gear::D430_0,
    &gear::D344_0,
    &gear::D344_1,
    &gear::D344_2,
    &gear::D344_3,
    &gear::D344_4,
    &gear::D319_0,
    &gear::D6_0,
    &gear::D7_0,
    &gear::D45_2,
    &gear::D50_0,
    &gear::D389_0,
    &gear::D431_0,
    &gear::D431_1,
    &gear::D431_2,
    &guardian::D107_0,
    &guardian::D107_1,
    &guardian::D107_2,
    &guardian::D403_0,
    &guardian::D403_1,
    &guardian::D403_2,
    &guardian::D108_0,
    &guardian::D108_1,
    &guardian::D109_0,
    &guardian::D109_1,
    &guardian::D18_0,
    &guardian::D109_2,
    &guardian::D18_1,
    &guardian::D109_3,
    &guardian::D18_2,
    &guardian::D313_0,
    &guardian::D111_0,
    &guardian::D111_1,
    &guardian::D111_2,
    &item::D10_0,
    &item::D51_0,
    &item::D211_0,
    &item::D212_0,
    &item::D52_0,
    &item::D53_0,
    &item::D54_0,
    &item::D55_0,
    &luminary::D374_0,
    &luminary::D374_1,
    &luminary::D374_2,
    &luminary::D368_0,
    &luminary::D368_1,
    &luminary::D368_2,
    &luminary::D368_3,
    &luminary::D368_4,
    &luminary::D390_0,
    &luminary::D390_1,
    &luminary::D376_0,
    &mesmer::D119_0,
    &mesmer::D119_1,
    &mesmer::D119_2,
    &mesmer::D119_3,
    &mesmer::D119_4,
    &mesmer::D119_5,
    &necromancer::D21_0,
    &necromancer::D124_0,
    &necromancer::D124_1,
    &necromancer::D125_0,
    &necromancer::D125_1,
    &necromancer::D125_2,
    &necromancer::D126_0,
    &necromancer::D126_1,
    &necromancer::D336_0,
    &necromancer::D336_1,
    &paragon::D369_0,
    &paragon::D369_1,
    &paragon::D369_2,
    &paragon::D369_3,
    &paragon::D369_4,
    &paragon::D369_5,
    &paragon::D369_6,
    &paragon::D370_0,
    &paragon::D370_1,
    &paragon::D370_2,
    &paragon::D370_3,
    &ranger::D25_0,
    &ranger::D25_1,
    &ranger::D25_2,
    &ranger::D334_0,
    &ranger::D131_0,
    &ranger::D131_1,
    &ranger::D131_2,
    &ranger::D131_3,
    &ranger::D132_0,
    &reaper::D128_0,
    &reaper::D128_1,
    &reaper::D129_0,
    &reaper::D129_1,
    &reaper::D129_2,
    &shared::D422_0,
    &shared::D423_0,
    &shared::D424_0,
    &shared::D429_0,
    &shared::D56_0,
    &shared::D428_0,
    &shared::D427_0,
    &shared::D426_0,
    &shared::D425_0,
    &shared::D58_0,
    &shared::D59_0,
    &shared::D57_0,
    &shared::D60_0,
    &shared::D61_0,
    &shared::D62_0,
    &tempest::D74_0,
    &tempest::D74_1,
    &tempest::D74_2,
    &tempest::D74_3,
    &tempest::D75_0,
    &tempest::D75_1,
    &tempest::D78_0,
    &troubadour::D362_0,
    &troubadour::D362_1,
    &troubadour::D361_0,
    &troubadour::D361_1,
    &troubadour::D364_0,
    &troubadour::D364_1,
    &troubadour::D364_2,
    &troubadour::D411_0,
    &troubadour::D411_1,
    &untamed::D93_0,
    &untamed::D93_1,
    &untamed::D93_2,
    &untamed::D93_3,
    &untamed::D93_4,
    &untamed::D93_5,
    &untamed::D93_6,
    &untamed::D93_7,
    &untamed::D93_8,
    &untamed::D93_9,
    &untamed::D93_10,
    &untamed::D94_0,
    &untamed::D94_1,
    &untamed::D94_2,
    &untamed::D93_11,
    &untamed::D93_12,
    &untamed::D93_13,
    &untamed::D93_14,
    &untamed::D93_15,
    &untamed::D93_16,
    &untamed::D93_17,
    &untamed::D93_18,
    &untamed::D93_19,
    &warrior::D172_0,
    &warrior::D172_1,
    &warrior::D172_2,
    &warrior::D172_3,
    &warrior::D173_0,
    &warrior::D173_1,
    &warrior::D173_2,
    &warrior::D173_3,
    &warrior::D174_0,
    &warrior::D36_0,
    &warrior::D36_1,
    &warrior::D36_2,
    &warrior::D36_3,
    &warrior::D175_0,
    &warrior::D175_1,
    &warrior::D175_2,
    &warrior::D176_0,
    &warrior::D176_1,
];

/// Assert every catalog entry is a combination GW2EI itself could produce
/// (`DamageModifierDef::validate`) -- the transcription guard for the
/// generated table.
#[cfg(test)]
#[test]
fn catalog_definitions_are_valid() {
    for d in CATALOG {
        d.validate().unwrap_or_else(|e| panic!("{e}"));
    }
    assert!(!CATALOG.is_empty());
}

/// The one invariant duplicate ids must respect.
///
/// A reworked modifier is several entries sharing one `json_id` with
/// disjoint build windows and/or disjoint modes (GW2EI writes them that
/// way, e.g. `Mod_BloodyRoar` has ten). What must NEVER happen is two of
/// them being live at once: [`super::evaluate`] resolves a running key back
/// to its definition by `json_id`, and GW2EI asserts the same uniqueness
/// when it builds `OutgoingDamageModifiersByID`
/// (`DamageModifiersContainer.cs:111-117`).
///
/// So this sweeps every build boundary the catalog mentions (plus one below
/// and one above each, to catch an off-by-one in a half-open window) across
/// every `(ParseMode, SkillMode)` pair, and asserts the surviving set is
/// id-unique. That is a far stronger transcription guard than a flat
/// duplicate check, and it is what actually caught window typos while this
/// table was being written.
#[cfg(test)]
#[test]
fn no_ambiguous_definition_for_any_build_and_mode() {
    use super::model::{ParseMode, SkillMode, END_OF_LIFE};
    use std::collections::{BTreeMap, BTreeSet};

    let mut builds: BTreeSet<u64> = BTreeSet::new();
    builds.insert(0);
    for d in CATALOG {
        for b in [d.min_gw2_build, d.max_gw2_build] {
            if b == END_OF_LIFE {
                continue;
            }
            builds.insert(b.saturating_sub(1));
            builds.insert(b);
            builds.insert(b + 1);
        }
    }
    let modes = [
        (ParseMode::WvW, SkillMode::WvW),
        (ParseMode::SPvP, SkillMode::SPvP),
        (ParseMode::Instanced, SkillMode::PvE),
        (ParseMode::OpenWorld, SkillMode::PvE),
        (ParseMode::Unknown, SkillMode::PvE),
    ];
    let mut failures: Vec<String> = Vec::new();
    for &b in &builds {
        for (pm, sm) in modes {
            let mut seen: BTreeMap<i32, &str> = BTreeMap::new();
            for d in CATALOG {
                if !d.available(Some(b), None) || !d.keep(pm, sm) {
                    continue;
                }
                if let Some(prev) = seen.insert(d.json_id(), d.name) {
                    failures.push(format!(
                        "build {b} {pm:?}/{sm:?}: id {} claimed by both {prev:?} and {:?}",
                        d.json_id(),
                        d.name
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "ambiguous definitions:\n{}", failures.join("\n"));
}

/// The catalog must not contain a definition the engine would silently
/// drop: transcription is only useful if [`super::is_supported`] accepts it.
#[cfg(test)]
#[test]
fn every_catalog_definition_is_supported_by_the_engine() {
    for d in CATALOG {
        assert!(
            super::is_supported(d),
            "{}: transcribed but rejected by is_supported -- it belongs in the \
             skipped list in this module's doc instead",
            d.name
        );
    }
}
