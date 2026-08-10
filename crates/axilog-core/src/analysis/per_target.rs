//! Per-(player, target) offensive splits (MEIGAP Task 1d) -- the
//! `statsTargets[i][0]` fields axibridge reads that whole-fight rollups
//! cannot answer.
//!
//! GW2EI builds `statsTargets[targetIndex][phaseIndex]` from
//! `player.GetOffensiveStats(target, log, phase.Start, phase.End)`
//! (`GW2EIBuilders/JsonModels/JsonActors/JsonPlayerBuilder.cs:122`), i.e.
//! `OffensiveStatistics` -- the SAME class that builds the non-target
//! `statsAll`, only with a non-null `target` argument
//! (`GW2EIEvtcParser/EIData/Actors/SingleActor.cs:680-691`). Every field
//! this module computes is therefore the already-calibrated whole-fight
//! statistic, restricted to one enemy:
//!
//! - **`connectedDamageCount`** (`OffensiveStatistics.cs:81-84`,
//!   `JsonStatisticsBuilder.cs:92`): `DamageCount`, incremented once per
//!   `dl.HasHit` event. Reuses `hit_stats::classify`, which IS this
//!   project's `HasHit` decision (era-gated, `pub(crate)` for exactly this
//!   kind of second reader) -- so the per-target counts sum to
//!   `hit_stats::HitStats::connected_count` by construction.
//! - **`againstDownedCount`** (`OffensiveStatistics.cs:159-163`):
//!   `AgainstDownedDamageCount`, connecting hits landed while the TARGET
//!   was downed -- `Classified::is_against_downed`.
//! - **`killed`/`downed`** (`OffensiveStatistics.cs:190-197`): counts of
//!   `dl.HasKilled`/`dl.HasDowned`, which are set exclusively on
//!   `NoDamageHealthDamageEvent`s carrying `DamageResult.KillingBlow` /
//!   `DamageResult.Downed` (`ParsedData/CombatEvents/DamageEvents/
//!   NoDamageHealthDamageEvent.cs:9-10`, created at
//!   `CombatEventFactory.cs:814-817`) -- i.e. LAST-HIT attribution, exactly
//!   the `result::DOWNED`/`result::KILLING_BLOW` predicate
//!   `downs::apply_with_registry` already uses for the whole-fight
//!   `downs_dealt`/`kills_dealt`, including its minion fold.
//! - **`interrupts`** (`OffensiveStatistics.cs`'s `InterruptCount`,
//!   `JsonStatisticsBuilder.cs`'s `Interrupts`): the outgoing mirror of
//!   `defenses::DefenseStats::interrupted_count`, i.e.
//!   `result::INTERRUPT` rows landed on this target. Not in the audit's
//!   own gap row but read by axibridge alongside `againstDownedCount`
//!   (`packages/bridge-metrics/src/dashboardMetrics.ts`'s
//!   `getTargetStatTotal`), and free on this same scan.
//!
//! **Scope, per field, mirroring each rollup exactly.** GW2EI's
//! `GetDamageEvents` folds minion damage in (`SingleActor.cs:734-739`), but
//! all of `OffensiveStatistics`' hit-quality fields sit INSIDE a
//! `dl.From.Is(actor.AgentItem)` guard (`:68`) that re-narrows them to the
//! actor -- which is why this project's `hit_stats` (M13 Task 1) is
//! actor-only and calibrates exactly. `HasKilled`/`HasDowned` are the
//! exception: their branch sits OUTSIDE that guard, so a pet's or minion's
//! finishing blow counts for its owner. `downs::credited_squad_source`
//! carries that fold (and the measurement that forced it) and is reused
//! verbatim here, so each column sums to its own rollup:
//! `connected_hits`/`against_downed_count` to `hit_stats`, `downed`/
//! `killed` to `downs_dealt`/`kills_dealt`.
//!
//! **`downContribution` is deliberately NOT here.** EI's per-target
//! `downContribution` (`OffensiveStatistics.cs:85-108`) is damage dealt to
//! that target inside its own 90%-to-downstate window -- a DIFFERENT
//! algorithm from this project's arcdps-methodology contribution family by
//! design. Its per-target split lives on
//! `contribution::ContributionMetrics`' own per-target map
//! (`PlayerMetrics::downs_contribution_per_target`), for the same reason
//! and with the same honesty caveat the whole-fight
//! `statsAll[0].downContribution` mapping already carries.

use crate::analysis::damage::InstidRegistry;
use crate::analysis::downs::credited_squad_source as downs_credited_source;
use crate::analysis::hit_stats::classify;
use crate::evtc::{result, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// One `(player, enemy)` pair's offensive split. Only pairs with at least
/// one qualifying event exist in [`build`]'s output (a sparse map: a real
/// WvW log enumerates hundreds of targets, almost all of which any given
/// player never touched).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerTargetOffense {
    /// EI's `connectedDamageCount` (its `connectedHits` in axibridge's
    /// fallback chain).
    pub connected_hits: u32,
    /// EI's `connectedDmg`. Not read by axibridge (its `totalDmg` fallback
    /// already resolves), but free here and the natural pair for the count
    /// -- and the invariant that makes the count auditable.
    pub connected_damage: u64,
    /// EI's `againstDownedCount`.
    pub against_downed_count: u32,
    /// EI's `downed` -- times this player landed the DOWNING blow on this
    /// target.
    pub downed: u32,
    /// EI's `killed` -- times this player landed the KILLING blow on this
    /// target.
    pub killed: u32,
    /// EI's `interrupts`.
    pub interrupts: u32,
}

impl PerTargetOffense {
    fn is_empty(&self) -> bool {
        *self == PerTargetOffense::default()
    }
}

/// Per-(player representative addr, enemy representative id) offensive
/// splits over one log. One linear scan, reusing the squad/enemy membership
/// predicate and `hit_stats::classify` decision every other damage-side
/// pass already shares.
pub fn build(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<(u64, u64), PerTargetOffense> {
    let post_era = raw.header.is_post_buff_rework();
    let mut out: BTreeMap<(u64, u64), PerTargetOffense> = BTreeMap::new();

    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if !enemies.contains(&e.dst_agent) {
            continue;
        }
        let dst = enemy_addr_to_rep.get(&e.dst_agent).copied().unwrap_or(e.dst_agent);

        // The no-damage MARKER results (`NoDamageHealthDamageEvent`)
        // are mutually exclusive with a hit, so they're dispatched first
        // and `classify` never sees them (it returns `None` for all three
        // anyway -- this is an early-out, not a behaviour change).
        //
        // Deliberately NOT gated on `buff == 0`, matching `downs::apply`'s
        // own already-calibrated whole-fight predicate byte for byte --
        // which is what makes these per-target counts sum to `downs_dealt`/
        // `kills_dealt` exactly. Pre-era, a `buff == 1` row's result byte
        // decodes as the retired `ConditionResult` enum, whose values stop
        // at 4 (`hit_stats::classify`'s pre-era branch: ">= 5 collapses to
        // Unknown, silently dropped by GW2EI"), so a 5/8/9 there would be a
        // malformed row rather than a real marker; post-era every row uses
        // the unified `DamageResult` enum and the ungated read is simply
        // correct.
        if e.result == result::DOWNED || e.result == result::KILLING_BLOW {
            // Minion-inclusive, matching `downs::apply_with_registry`'s own
            // credited-source fold byte for byte (see its
            // `credited_squad_source` doc comment for the
            // `OffensiveStatistics.cs:190-197` citation) -- which is what
            // makes this column sum to `downs_dealt`/`kills_dealt`.
            let Some(owner) = downs_credited_source(e, registry, squad) else { continue };
            let src = addr_to_rep.get(&owner).copied().unwrap_or(owner);
            let s = out.entry((src, dst)).or_default();
            if e.result == result::DOWNED {
                s.downed += 1;
            } else {
                s.killed += 1;
            }
            continue;
        }

        // Every remaining field is ACTOR-ONLY (see this module's scope
        // note), so from here the source must be a squad player itself.
        if !squad.contains(&e.src_agent) {
            continue;
        }
        let src = addr_to_rep.get(&e.src_agent).copied().unwrap_or(e.src_agent);
        if e.result == result::INTERRUPT {
            out.entry((src, dst)).or_default().interrupts += 1;
            continue;
        }

        let Some(c) = classify(e, post_era) else { continue };
        let s = out.entry((src, dst)).or_default();
        s.connected_hits += 1;
        s.connected_damage += c.dmg;
        if c.is_against_downed {
            s.against_downed_count += 1;
        }
    }

    out.retain(|_, v| !v.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader};

    fn base(src: u64, dst: u64) -> RawEvent {
        RawEvent {
            time: 0, src_agent: src, dst_agent: dst, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 1, src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 1, buff: 0, result: 0, is_activation: 0, is_buffremove: 0, is_ninety: 0,
            is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0,
            is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn run(events: Vec<RawEvent>) -> BTreeMap<(u64, u64), PerTargetOffense> {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64, 10].into_iter().collect();
        let raw = raw_from(events);
        let registry = InstidRegistry::build(&raw);
        build(&raw, &registry, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new())
    }

    #[test]
    fn splits_hits_downs_kills_and_interrupts_by_target() {
        let mut hit9 = base(1, 9);
        hit9.result = result::NORMAL;
        hit9.value = 100;
        let mut hit10 = base(1, 10);
        hit10.result = result::CRIT;
        hit10.value = 250;
        let mut down9 = base(1, 9);
        down9.result = result::DOWNED;
        let mut kill10 = base(1, 10);
        kill10.result = result::KILLING_BLOW;
        let mut interrupt9 = base(1, 9);
        interrupt9.result = result::INTERRUPT;

        let out = run(vec![hit9, hit10, down9, kill10, interrupt9]);
        assert_eq!(
            out[&(1, 9)],
            PerTargetOffense {
                connected_hits: 1, connected_damage: 100, downed: 1, interrupts: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            out[&(1, 10)],
            PerTargetOffense {
                connected_hits: 1, connected_damage: 250, killed: 1, ..Default::default()
            }
        );
    }

    /// `againstDownedCount` follows `hit_stats`' own `is_offcycle == 1`
    /// against-downed flag, per target.
    #[test]
    fn against_downed_hits_are_split_by_target() {
        let mut e = base(1, 9);
        e.result = result::NORMAL;
        e.value = 50;
        e.is_offcycle = 1;
        let out = run(vec![e]);
        assert_eq!(out[&(1, 9)].against_downed_count, 1);
        assert_eq!(out[&(1, 9)].connected_hits, 1);
    }

    /// A pair with no qualifying event never appears (the map is sparse --
    /// a real log enumerates hundreds of targets per player).
    #[test]
    fn pairs_with_no_events_are_absent() {
        let mut e = base(1, 9);
        e.result = result::NORMAL;
        e.value = 1;
        let out = run(vec![e]);
        assert!(!out.contains_key(&(1, 10)));
        assert_eq!(out.len(), 1);
    }

    /// Non-hit outcomes (block/evade/miss) are not connected hits -- the
    /// same `classify` decision `hit_stats` already calibrates.
    #[test]
    fn blocked_events_are_not_connected_hits() {
        let mut e = base(1, 9);
        e.result = result::BLOCK;
        e.value = 0;
        let out = run(vec![e]);
        assert!(out.is_empty());
    }
}
