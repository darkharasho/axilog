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
//! most of `OffensiveStatistics`' fields sit INSIDE a
//! `dl.From.Is(actor.AgentItem)` guard (`OffensiveStatistics.cs:67`) that
//! re-narrows them to the actor -- which is why this project's `hit_stats`
//! (M13 Task 1) is actor-only and calibrates exactly.
//!
//! **THREE fields are outside that guard** and are therefore
//! minion-inclusive: `HasInterrupted` (`:186-189`), `HasKilled` (`:190-193`)
//! and `HasDowned` (`:194-197`) -- a pet's or minion's interrupting,
//! finishing or downing blow counts for its owner.
//! `downs::credited_squad_source` carries that single-hop master fold (and
//! the measurement that forced it) and is reused verbatim for all three, so
//! each column sums to its own rollup: `connected_hits`/
//! `against_downed_count` to `hit_stats`, `downed`/`killed` to
//! `downs_dealt`/`kills_dealt`.
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
use crate::analysis::defenses::{classify_outcome, Outcome};
use crate::analysis::downs::credited_squad_source as downs_credited_source;
use crate::analysis::hit_stats::{can_crit, classify};
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
    /// EI's `directDmg` count pair. Named to match
    /// `hit_stats::HitStats::direct_count` so the per-target and
    /// whole-fight versions of one quantity are recognizably the same
    /// thing. NOT the same as the schema's `connected_direct_dmg`, which
    /// measures a different quantity -- see this plan's Task 4.
    pub direct_count: u32,
    /// EI's `directDmg`.
    pub direct_damage: u64,
    /// EI's `criticalRate` numerator. Gated behind `hit_stats::can_crit`
    /// exactly as the whole-fight counter is -- GW2EI gates
    /// `crit_count`/`critable_direct_count` behind `CanCrit` but NOT
    /// `direct_count`/`flank_count`/`glance_count`.
    pub crit_count: u32,
    /// EI's `criticalDmg`.
    pub crit_damage: u64,
    /// EI's `flankingRate` numerator.
    pub flank_count: u32,
    /// EI's `glanceRate` numerator.
    pub glance_count: u32,
    /// EI's `criticalRate` DENOMINATOR -- not `direct_count`. See
    /// `hit_stats`'s module doc, `critable_direct_count` section.
    pub critable_direct_count: u32,
    /// EI's `againstDownedDamage` -- the damage pair for the
    /// `against_downed_count` this struct already carried.
    pub against_downed_damage: u64,
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
    /// EI's `missed` -- arcdps `BLIND`. Comes from `defenses::classify_outcome`,
    /// not `hit_stats::classify`: the latter returns `None` for every non-hit
    /// outcome by design (it only ever answers "was this a connected hit?"),
    /// so the four mitigation outcomes are otherwise invisible to this scan.
    pub missed: u32,
    /// EI's `evaded` -- arcdps `EVADE`, an attack dodged outright.
    pub evaded: u32,
    /// EI's `blocked` -- arcdps `BLOCK`, stopped by an active defense like
    /// aegis or a shield skill rather than a dodge.
    pub blocked: u32,
    /// EI's `invulned` -- arcdps `ABSORB`/`INVERT`, an attack that landed
    /// against a target flagged unhittable (e.g. a defiance/invuln buff)
    /// rather than actively defended against.
    pub invulned: u32,
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
        if matches!(e.result, result::DOWNED | result::KILLING_BLOW | result::INTERRUPT) {
            // The three minion-INCLUSIVE fields (this module's scope note):
            // credited through `downs::apply_with_registry`'s own
            // credited-source fold, byte for byte -- which is what makes
            // `downed`/`killed` sum to `downs_dealt`/`kills_dealt`.
            let Some(owner) = downs_credited_source(e, registry, squad) else { continue };
            let src = addr_to_rep.get(&owner).copied().unwrap_or(owner);
            let s = out.entry((src, dst)).or_default();
            match e.result {
                result::DOWNED => s.downed += 1,
                result::KILLING_BLOW => s.killed += 1,
                _ => s.interrupts += 1,
            }
            continue;
        }

        // Every remaining field is ACTOR-ONLY (see this module's scope
        // note), so from here the source must be a squad player itself.
        if !squad.contains(&e.src_agent) {
            continue;
        }
        let src = addr_to_rep.get(&e.src_agent).copied().unwrap_or(e.src_agent);

        // A mitigated attempt is not a hit, so `classify` drops it -- but
        // GW2EI counts it per target, and a pair the player only ever
        // whiffed against has no other way to produce a row. Dispatch the
        // outcome first, then fall through to the hit path.
        let s = out.entry((src, dst)).or_default();
        match classify_outcome(e, post_era) {
            Some(Outcome::Blocked) => {
                s.blocked += 1;
                continue;
            }
            Some(Outcome::Evaded) => {
                s.evaded += 1;
                continue;
            }
            Some(Outcome::Missed) => {
                s.missed += 1;
                continue;
            }
            Some(Outcome::Invulned) => {
                s.invulned += 1;
                continue;
            }
            _ => {}
        }
        let Some(c) = classify(e, post_era) else { continue };
        s.connected_hits += 1;
        s.connected_damage += c.dmg;
        // Mirrors `hit_stats::accumulate`'s direct-hit branch byte for
        // byte, minus the condition/life-leech/above-90 buckets EI does not
        // split per target. Keeping the same order and the same `can_crit`
        // gate is what makes the per-target rows sum to the whole-fight
        // totals for every enumerated target.
        if c.is_direct_hit {
            if can_crit(e.skillid) {
                s.critable_direct_count += 1;
                if c.is_crit {
                    s.crit_count += 1;
                    s.crit_damage += c.dmg;
                }
            }
            s.direct_count += 1;
            s.direct_damage += c.dmg;
            if e.is_flanking != 0 {
                s.flank_count += 1;
            }
            if c.is_glance {
                s.glance_count += 1;
            }
        }
        if c.is_against_downed {
            s.against_downed_count += 1;
            s.against_downed_damage += c.dmg;
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
                connected_hits: 1, connected_damage: 100, direct_count: 1, direct_damage: 100,
                critable_direct_count: 1, downed: 1, interrupts: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            out[&(1, 10)],
            PerTargetOffense {
                connected_hits: 1, connected_damage: 250, direct_count: 1, direct_damage: 250,
                crit_count: 1, crit_damage: 250, critable_direct_count: 1, killed: 1,
                ..Default::default()
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

    /// `HasInterrupted` sits outside GW2EI's `From.Is(actor)` guard
    /// alongside `HasKilled`/`HasDowned`
    /// (`OffensiveStatistics.cs:186-197`), so a MINION's interrupt counts
    /// for its owner -- exactly like its finishing blow.
    #[test]
    fn minion_sourced_markers_credit_the_owner() {
        // Register minion instid 5 -> owner (squad player 1, instid 1).
        // A BLOCKED row -- it does not touch `classify`, but as of this
        // struct's four mitigation counters it now contributes its own
        // `blocked` count directly, since the owner IS the squad source.
        let mut owner_intro = base(1, 9);
        owner_intro.src_instid = 1;
        owner_intro.result = result::BLOCK;
        let minion = |result_: u8| {
            let mut e = base(50, 9); // src is the MINION, not a squad addr
            e.src_instid = 5;
            e.src_master_instid = 1;
            e.result = result_;
            e
        };
        let out = run(vec![
            owner_intro,
            minion(result::INTERRUPT),
            minion(result::DOWNED),
            minion(result::KILLING_BLOW),
        ]);
        assert_eq!(
            out[&(1, 9)],
            PerTargetOffense {
                interrupts: 1, downed: 1, killed: 1, blocked: 1,
                ..Default::default()
            },
            "all three of GW2EI's unguarded markers fold onto the owner"
        );
    }

    /// A minion's ordinary HIT does NOT credit the owner here -- hit
    /// quality sits INSIDE GW2EI's `From.Is(actor)` guard, which is what
    /// keeps this column summing to actor-only `hit_stats`.
    #[test]
    fn minion_sourced_hits_do_not_credit_the_owner() {
        let mut owner_intro = base(1, 9);
        owner_intro.src_instid = 1;
        owner_intro.result = result::BLOCK;
        let mut hit = base(50, 9);
        hit.src_instid = 5;
        hit.src_master_instid = 1;
        hit.result = result::NORMAL;
        hit.value = 100;
        let out = run(vec![owner_intro, hit]);
        // The owner's own BLOCK still produces a row (its `blocked` mitigation
        // counter), but the minion's ordinary hit must not add to that row's
        // `connected_hits` -- hit quality stays inside the actor-only guard.
        assert_eq!(
            out[&(1, 9)],
            PerTargetOffense { blocked: 1, ..Default::default() },
            "the minion's hit is not credited to the owner"
        );
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
    /// same `classify` decision `hit_stats` already calibrates. The row
    /// still exists (via the `blocked` mitigation counter), it just carries
    /// no hit-quality data.
    #[test]
    fn blocked_events_are_not_connected_hits() {
        let mut e = base(1, 9);
        e.result = result::BLOCK;
        e.value = 0;
        let out = run(vec![e]);
        assert_eq!(
            out[&(1, 9)],
            PerTargetOffense { blocked: 1, ..Default::default() },
            "a block is mitigation, not a connected hit"
        );
    }

    /// The eight hit-quality counters must split by target using the same
    /// `hit_stats::classify` decision the whole-fight totals already use.
    /// A crit on one target must not inflate the other target's row --
    /// which is exactly the error axibridge's `statsAll` fallback makes.
    #[test]
    fn splits_hit_quality_by_target() {
        let mut crit9 = base(1, 9);
        crit9.result = result::CRIT;
        crit9.value = 300;
        crit9.is_flanking = 1;
        let mut glance10 = base(1, 10);
        glance10.result = result::GLANCE;
        glance10.value = 50;
        let out = run(vec![crit9, glance10]);

        let t9 = &out[&(1, 9)];
        assert_eq!(t9.direct_count, 1);
        assert_eq!(t9.direct_damage, 300);
        assert_eq!(t9.crit_count, 1);
        assert_eq!(t9.crit_damage, 300);
        assert_eq!(t9.critable_direct_count, 1);
        assert_eq!(t9.flank_count, 1);
        assert_eq!(t9.glance_count, 0);

        let t10 = &out[&(1, 10)];
        assert_eq!(t10.direct_count, 1);
        assert_eq!(t10.direct_damage, 50);
        assert_eq!(t10.crit_count, 0, "target 9's crit must not leak onto target 10");
        assert_eq!(t10.glance_count, 1);
        assert_eq!(t10.flank_count, 0);
    }

    /// `against_downed_damage` is the damage pair for the count this struct
    /// already carries; EI reports both.
    #[test]
    fn accumulates_against_downed_damage_per_target() {
        let mut hit = base(1, 9);
        hit.result = result::NORMAL;
        hit.value = 120;
        hit.is_offcycle = 1;
        let out = run(vec![hit]);
        let t = &out[&(1, 9)];
        assert_eq!(t.against_downed_count, 1);
        assert_eq!(t.against_downed_damage, 120);
    }

    /// The four mitigation outcomes are the rows `hit_stats::classify`
    /// returns `None` for, so they need the widened `defenses` classifier.
    /// GW2EI reports them per target (`totalDamage: 0, hits: n` rows), and
    /// without them a target a player only ever whiffed against produces no
    /// row at all.
    #[test]
    fn splits_mitigation_outcomes_by_target() {
        let mut blocked = base(1, 9);
        blocked.result = result::BLOCK;
        let mut evaded = base(1, 9);
        evaded.result = result::EVADE;
        let mut blind = base(1, 10);
        blind.result = result::BLIND;
        let mut absorb = base(1, 10);
        absorb.result = result::ABSORB;
        let out = run(vec![blocked, evaded, blind, absorb]);

        let t9 = &out[&(1, 9)];
        assert_eq!(t9.blocked, 1);
        assert_eq!(t9.evaded, 1);
        assert_eq!(t9.missed, 0);
        assert_eq!(t9.invulned, 0);
        assert_eq!(t9.connected_hits, 0, "a mitigated attempt is not a connected hit");

        let t10 = &out[&(1, 10)];
        assert_eq!(t10.missed, 1, "BLIND is EI's `missed`");
        assert_eq!(t10.invulned, 1, "ABSORB is EI's `invulned`");
    }

    /// A pair the player only ever whiffed against must still produce a
    /// row. `is_empty` drives `retain`, so it has to see the new counters.
    #[test]
    fn mitigation_only_pair_still_produces_a_row() {
        let mut blocked = base(1, 9);
        blocked.result = result::BLOCK;
        let out = run(vec![blocked]);
        assert!(out.contains_key(&(1, 9)), "a blocked-only pair must not be retained away");
    }
}
