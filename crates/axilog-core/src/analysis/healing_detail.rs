//! Per-ally, per-skill and per-second healing/barrier from the arcdps
//! healing extension (MEIGAP Task 3a) -- GW2EI's
//! `extHealingStats.outgoingHealingAllies` / `.totalHealingDist` /
//! `.healing1S` and `extBarrierStats.outgoingBarrierAllies` /
//! `.totalBarrierDist`.
//!
//! ## Relationship to [`crate::analysis::healing`]
//!
//! M10's `healing` module owns the hard part -- decoding the extension's
//! byte/flag layout, resolving healer and target by instid, GW2EI's
//! all-or-nothing `SanitizeForSrc` peer sanitization, the pet/minion
//! `src_master_instid` owner fold and the account fold. None of that is
//! repeated here. This module consumes
//! [`crate::analysis::healing::attributed_events`] -- the single shared
//! producer both it and `healing::apply` reduce -- and does nothing but
//! GROUP the resulting rows three ways:
//!
//! | this module | grouping |
//! |---|---|
//! | `ally_healing` / `ally_barrier` | by recipient (positional over `enc.players`) |
//! | `healing_dist` / `barrier_dist` | by `skill_id` |
//! | `healing_1s` | by 1-second bucket, cumulative |
//!
//! Because there is one producer, `extHealingStats.outgoingHealing[0]
//! .healing` (from `healing::HealingMetrics`) and the sum of
//! `totalHealingDist`, and the last element of `healing1S`, are the same
//! number by construction rather than by test.
//!
//! ## GW2EI's shapes, transcribed
//!
//! Builder:
//! `GW2EIBuilders/JsonModels/JsonActorUtilities/JsonExtensions/EXTHealingStats/EXTJsonPlayerHealingStatsBuilder.cs`.
//!
//! - **`outgoingHealingAllies[allyIndex][phase]`** (`:73`): one row per
//!   `log.Friendlies` entry, in that order, from
//!   `a.EXTHealing.GetOutgoingHealStats(friendly, ...)`. The healer appears
//!   as one of its own "allies", at its own index -- there is no
//!   self-excluded scalar in EI's output. axilog emits it positionally over
//!   `enc.players`, which is exactly the array axibridge indexes it against
//!   (`computePlayerAggregation.ts:952-960`'s `addHealingByCategory` does
//!   `players[allyIdx]`, then classifies self / group / squad / off-squad
//!   from that entry). Fields emitted: `healing` and `downedHealing`
//!   (barrier: `barrier`) -- the only three any axibridge reader touches
//!   (`combatMetrics.ts:131-158`, `computePlayerAggregation.ts:952-966`).
//!   EI's other seventeen per-ally fields
//!   (`healingPowerHealing`/`conversionHealing`/`hybridHealing`/`actor*`
//!   and their `hps` twins) split healing by the SOURCE ATTRIBUTE that
//!   produced it, which the extension's wire format does not carry and
//!   this project has never modelled -- omitted, not faked, exactly as the
//!   scalar `outgoingHealing[0]` block already omits them.
//! - **`totalHealingDist[phase]`** (`:118`): `GetOutgoingHealEvents(null,
//!   ...).GroupBy(x => x.SkillID)` through
//!   `EXTJsonHealingStatsBuilderCommons.BuildHealingDist`, which
//!   accumulates `Hits++`, `TotalHealing += HealingDone`,
//!   `TotalDownedHealing` (when `AgainstDowned`), `Min`, `Max`, and sets
//!   `IndirectHealing = list.Exists(x => x is EXTNonDirectHealingEvent)`.
//!   Note there is no `HasHit` gate anywhere in the healing dist -- unlike
//!   the damage dist, EVERY event in the group counts toward `hits`, `min`
//!   and `max`. Reproduced verbatim.
//! - **`healing1S[phase]`** (`:105`):
//!   `EXTSingleActorHealingHelper.ComputeHealingGraph` (`:84-110`), which
//!   is the same `InterpolatedGraph` length rule and the same
//!   `Math.Ceiling((t - start) / 1000.0)` bucket index MEIGAP Task 2's
//!   `timeseries::ei_grid`/`ei_bucket` already transcribes, over a
//!   CUMULATIVE fill-forward array. Same helpers, so the healing series and
//!   the damage series cannot land on different grids.
//!
//! ## Scope
//!
//! `healingReceived1S` and the per-receiver barrier series ARE produced
//! here (see `healing_received_1s`/`barrier_received_1s`), added for the
//! AxiPulse native cutover, which renders both. `alliedHealing1S`,
//! `alliedHealingDist`, `totalIncomingHealingDist` and every
//! `*PowerHealing*` / `*Conversion*` / `*Hybrid*` variant EI also emits
//! are still NOT produced: no consumer reads them, and `alliedHealing1S`
//! alone would be `players x players x seconds`.
//!
//! ## The outgoing `barrier1S` is deliberately absent while `healing1S` is present
//!
//! That asymmetry is the audit's, not this module's: its gap rows name
//! `extHealingStats.healing1S` and stop there. `healing1S` is also unread
//! by axibridge today (it is declared in `dpsReportTypes.ts:98` but no
//! reader touches it) -- it ships because the audit names it and because it
//! is the cheapest of the five rows, not because a consumer needs it.
//!
//! ## Gating
//!
//! **Standalone, NOT wired into `analyze()`** -- opt-in like
//! `buffs::states` / `timeseries::build_enemy_series` /
//! `target_conditions`. The adapter gates the three families separately,
//! each behind the flag GW2EI itself gates the same field with:
//!
//! | family | GW2EI gate | axilog flag |
//! |---|---|---|
//! | `healing1S` | `settings.RawFormatTimelineArrays` (`:30`) | `--timeseries` |
//! | `outgoingHealingAllies` | none (always emitted) | always-on, measured |
//! | `totalHealingDist` | none (always emitted) | `--skill-damage` (payload only) |
//!
//! See the adapter and `docs/BENCHMARKS.md` for the measured sizes behind
//! the two non-obvious choices.
//!
//! ## Two shape divergences from GW2EI, both id-keyed-invisible
//!
//! 1. **Dist row ORDER.** GW2EI emits `totalHealingDist` in
//!    `GroupBy(x => x.SkillID)` order (first-event order); this pass emits
//!    ascending skill id, because it accumulates into a `BTreeMap`. Every
//!    axibridge reader keys by `entry.id`
//!    (`computePlayerAggregation.ts:1075-1097`), as does the calibration,
//!    so nothing observes it -- but a byte-diff against a real export
//!    would.
//! 2. **Indirect healing ids resolve in no map.** GW2EI's
//!    `BuildHealingDist` routes an `IndirectHealing` row's id into
//!    `buffMap` rather than `skillMap`. This adapter's `buffMap` carries
//!    the 12 tracked boons (plus Task 2d's conditions) and a
//!    healing-over-time id is neither, so such a row ships a correct
//!    `indirectHealing: true` alongside an id the consumer will render as
//!    `"Skill <id>"` -- the same fallback the always-on `skillMap` already
//!    documents for unresolvable ids.
//!
//! ## Everything here is extension-gated already
//!
//! [`build`] returns `None` on a log with no healing extension, which is
//! the same signal `Metrics::has_healing_extension` carries and the same
//! condition under which the adapter omits `extHealingStats` entirely. No
//! zero-filled arrays are ever produced for a log that has no data.

use super::damage::InstidRegistry;
use super::healing::{self, AttributedHeal};
use crate::analysis::timeseries::{ei_bucket, ei_grid};
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One (player, ally) cell of `outgoingHealingAllies` / of
/// `outgoingBarrierAllies` (the latter uses only `healing`, renamed
/// `barrier` on the wire -- see [`PlayerHealingDetail::ally_barrier`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllyHealing {
    /// EI's `healing`.
    pub healing: u64,
    /// EI's `downedHealing` -- the subset of `healing` landing on an ally
    /// who was downed at the time (`EXTHealingEvent.AgainstDowned`).
    pub downed_healing: u64,
}

/// One skill id's row of `totalHealingDist` / `totalBarrierDist`.
///
/// `hits`/`min`/`max` count EVERY event in the group: GW2EI's healing dist
/// has no `HasHit` equivalent (`BuildHealingDist` accumulates
/// unconditionally), unlike its damage dist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealDistEntry {
    pub skill_id: u32,
    /// EI's `totalHealing` / `totalBarrier`.
    pub total: u64,
    /// EI's `totalDownedHealing`. Always 0 on a barrier row: GW2EI's
    /// `EXTJsonBarrierDist` has no such field at all.
    pub total_downed: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    /// EI's `indirectHealing` / `indirectBarrier` -- the group contains at
    /// least one `EXTNonDirectHealingEvent` (a `buff != 0` extension row).
    pub indirect: bool,
}

impl HealDistEntry {
    fn record(&mut self, amount: u64, against_downed: bool, is_direct: bool) {
        self.min = if self.hits == 0 { amount } else { self.min.min(amount) };
        self.max = self.max.max(amount);
        self.hits += 1;
        self.total += amount;
        if against_downed {
            self.total_downed += amount;
        }
        if !is_direct {
            self.indirect = true;
        }
    }
}

/// One squad player's healing/barrier detail. Every array is positionally
/// joined to `enc.players` (`ally_*`) or sorted ascending by skill id
/// (`*_dist`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerHealingDetail {
    /// `outgoingHealingAllies`, one entry per `enc.players` entry, in that
    /// order -- INCLUDING this player's own index (a self-heal), matching
    /// GW2EI's `log.Friendlies` loop.
    pub ally_healing: Vec<AllyHealing>,
    /// `outgoingBarrierAllies`, same indexing. Only the `healing` field of
    /// each cell is meaningful; it renders as EI's `barrier`.
    pub ally_barrier: Vec<u64>,
    /// `totalHealingDist[0]`, sorted by skill id ascending.
    pub healing_dist: Vec<HealDistEntry>,
    /// `totalBarrierDist[0]`, sorted by skill id ascending.
    pub barrier_dist: Vec<HealDistEntry>,
    /// `healing1S[0]` -- CUMULATIVE outgoing healing per 1s bucket, on
    /// `timeseries::ei_grid`'s grid.
    pub healing_1s: Vec<u64>,
    /// CUMULATIVE **incoming** healing per 1s bucket, on
    /// `timeseries::ei_grid`'s grid -- the receiver-indexed transpose of
    /// `healing_1s`. GW2EI calls this `healingReceived1S`.
    ///
    /// Unlike `healing_1s`, this is ALLY-ATTRIBUTED: a heal only lands
    /// here when its recipient is one of the enumerated players, because
    /// there is no row to put it on otherwise.
    pub healing_received_1s: Vec<u64>,
    /// CUMULATIVE **incoming** barrier per 1s bucket, same grid and same
    /// ally attribution. GW2EI's counterpart is
    /// `extBarrierStats.barrierReceived1S`, whose attribution differs --
    /// see the ally-attribution note on `healing_received_1s`.
    pub barrier_received_1s: Vec<u64>,
}

/// Per-player healing detail, positionally joined to `enc.players`.
pub type HealingDetail = Vec<PlayerHealingDetail>;

/// Build the healing/barrier detail for every squad player, or `None` when
/// the log carries no healing extension at all.
pub fn build(raw: &RawLog, enc: &Encounter) -> Option<HealingDetail> {
    // Check the extension gate BEFORE paying for a registry build -- the
    // same cold-path hoist `healing::apply` documents. Most logs have no
    // healing extension, and this is an always-on ei-json pass, so the
    // scan-for-nothing would show up in the pipeline benchmark.
    if !crate::evtc::ext_healing::healing_extension_present(raw) {
        return None;
    }
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> Option<HealingDetail> {
    // The extension gate, decided ONCE for this pass (review fix). It has
    // to live here rather than only in `build`, because `None` must mean
    // "no extension" and not "an extension that produced no rows" -- but it
    // used to be asked twice more below (once directly, once inside
    // `healing::attributed_events`). `healing_extension_present` is an
    // `.any()` over registration rows, so on the PRESENT path it
    // short-circuits within the first few events and the remaining check
    // inside `attributed_events` is free; on the ABSENT path (which is most
    // logs) it is a full scan, and this is now the only one that runs
    // before the early return.
    if !crate::evtc::ext_healing::healing_extension_present(raw) {
        return None;
    }
    // Same two folds `analysis::mod::analyze` builds for every other pass.
    let squad: BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();

    let events = healing::attributed_events(raw, registry, &squad, &addr_to_rep);

    let index_of: BTreeMap<u64, usize> =
        enc.players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let n = enc.players.len();
    let buckets = ei_grid(enc.duration_ms);
    let t0 = raw.log_start_ms();

    let mut out: HealingDetail = (0..n)
        .map(|_| PlayerHealingDetail {
            ally_healing: vec![AllyHealing::default(); n],
            ally_barrier: vec![0u64; n],
            healing_dist: Vec::new(),
            barrier_dist: Vec::new(),
            // Per-bucket DELTAS while accumulating; turned into GW2EI's
            // cumulative fill-forward graph in the final pass below.
            healing_1s: vec![0u64; buckets],
            healing_received_1s: vec![0u64; buckets],
            barrier_received_1s: vec![0u64; buckets],
        })
        .collect();
    let mut healing_dist: Vec<BTreeMap<u32, HealDistEntry>> = vec![BTreeMap::new(); n];
    let mut barrier_dist: Vec<BTreeMap<u32, HealDistEntry>> = vec![BTreeMap::new(); n];

    for AttributedHeal {
        time,
        healer_rep,
        target_rep,
        skill_id,
        amount,
        is_direct,
        is_barrier,
        against_downed,
    } in events
    {
        let Some(&hi) = index_of.get(&healer_rep) else { continue };
        let ally = index_of.get(&target_rep).copied();
        if is_barrier {
            if let Some(ai) = ally {
                out[hi].ally_barrier[ai] += amount;
                if let Some(b) = ei_bucket(time, t0, buckets) {
                    out[ai].barrier_received_1s[b] += amount;
                }
            }
            barrier_dist[hi]
                .entry(skill_id)
                .or_insert(HealDistEntry { skill_id, ..Default::default() })
                // GW2EI's `EXTJsonBarrierDist` carries no downed field, so
                // the barrier row's `total_downed` stays 0 by construction.
                .record(amount, false, is_direct);
            continue;
        }
        if let Some(ai) = ally {
            let cell = &mut out[hi].ally_healing[ai];
            cell.healing += amount;
            if against_downed {
                cell.downed_healing += amount;
            }
            if let Some(b) = ei_bucket(time, t0, buckets) {
                out[ai].healing_received_1s[b] += amount;
            }
        }
        healing_dist[hi]
            .entry(skill_id)
            .or_insert(HealDistEntry { skill_id, ..Default::default() })
            .record(amount, against_downed, is_direct);
        // `ComputeHealingGraph` runs over `GetOutgoingHealEvents(null, ..)`
        // -- every outgoing heal, whether or not its recipient is one of
        // the enumerated allies. So this is deliberately outside the
        // `ally` match, and `healing_1s.last()` therefore equals
        // `HealingMetrics::healing_out_total`, not the ally-array sum.
        if let Some(b) = ei_bucket(time, t0, buckets) {
            out[hi].healing_1s[b] += amount;
        }
    }

    for (i, p) in out.iter_mut().enumerate() {
        p.healing_dist = std::mem::take(&mut healing_dist[i]).into_values().collect();
        p.barrier_dist = std::mem::take(&mut barrier_dist[i]).into_values().collect();
        // Deltas -> GW2EI's cumulative fill-forward graph
        // (`ComputeHealingGraph`'s `graph[i] = graph[previousTime]` loops).
        let mut running = 0u64;
        for v in p.healing_1s.iter_mut() {
            running += *v;
            *v = running;
        }
        let mut running = 0u64;
        for v in p.healing_received_1s.iter_mut() {
            running += *v;
            *v = running;
        }
        let mut running = 0u64;
        for v in p.barrier_received_1s.iter_mut() {
            running += *v;
            *v = running;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{ext_healing, sc, RawEvent, RawHeader};
    use crate::model::{Player, Team};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn registration() -> RawEvent {
        let src_agent = (ext_healing::HEALING_SIGNATURE as u64) | (1u64 << 32);
        RawEvent { is_statechange: sc::EXTENSION, pad: 0, src_agent, ..base_event() }
    }

    fn instid_reg(time: u64, addr: u64, instid: u16) -> RawEvent {
        RawEvent {
            time, src_agent: addr, src_instid: instid, dst_agent: addr, dst_instid: instid,
            iff: 1, ..base_event()
        }
    }

    /// A direct heal (`buff == 0`, negative `value`).
    fn heal(time: u64, src: u16, dst: u16, skillid: u32, amount: i32, downed: bool) -> RawEvent {
        RawEvent {
            is_statechange: sc::EXTENSION_COMBAT,
            pad: ext_healing::HEALING_SIGNATURE,
            time, src_instid: src, dst_instid: dst, skillid,
            value: -amount,
            is_offcycle: if downed { 1 << 5 } else { 0 },
            iff: 0,
            ..base_event()
        }
    }

    /// A barrier grant (`is_shields`).
    fn barrier(time: u64, src: u16, dst: u16, skillid: u32, amount: i32) -> RawEvent {
        RawEvent { is_shields: 1, ..heal(time, src, dst, skillid, amount, false) }
    }

    /// An INDIRECT heal (`buff != 0`, negative `buff_dmg`, `value == 0`) --
    /// GW2EI's `EXTNonDirectHealingEvent`.
    fn indirect_heal(time: u64, src: u16, dst: u16, skillid: u32, amount: i32) -> RawEvent {
        RawEvent {
            is_statechange: sc::EXTENSION_COMBAT,
            pad: ext_healing::HEALING_SIGNATURE,
            time, src_instid: src, dst_instid: dst, skillid,
            buff: 1, value: 0, buff_dmg: -amount,
            iff: 0,
            ..base_event()
        }
    }

    fn player(addr: u64, account: &str) -> Player {
        Player {
            agent_addr: addr, account: account.into(), character: account.into(),
            profession: "Guardian".into(), elite_spec: String::new(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None,
            commander_tag: None, guild_id: None, agent_addrs: vec![addr],
        }
    }

    fn enc_with(players: Vec<Player>, duration_ms: u64) -> Encounter {
        Encounter {
            kind: "wvw".into(), pve: None, map: String::new(), duration_ms,
            build: String::new(), revision: 1, recorded_by: None,
            teams: vec![Team { color: "red".into(), team_id: 1, guid: None, shard_id: None }],
            players, enemies: Vec::new(), markers: Vec::new(), ground_markers: Vec::new(), tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// No extension registration row -> `None`, never zero-filled arrays.
    #[test]
    fn returns_none_without_the_extension() {
        let raw = raw_from(vec![instid_reg(0, 1, 10)]);
        let enc = enc_with(vec![player(1, "A")], 1000);
        assert!(build(&raw, &enc).is_none());
    }

    /// The three groupings, on one small stream: two heals to an ally, one
    /// self-heal, one barrier, one indirect heal.
    #[test]
    fn groups_by_ally_by_skill_and_by_second() {
        let raw = raw_from(vec![
            registration(),
            instid_reg(0, 1, 10),
            instid_reg(0, 2, 20),
            heal(0, 10, 20, 100, 500, false),
            heal(1_500, 10, 20, 100, 300, true),
            heal(1_500, 10, 10, 200, 250, false),
            barrier(2_000, 10, 20, 300, 400),
            indirect_heal(2_400, 10, 20, 400, 50),
        ]);
        let enc = enc_with(vec![player(1, "A"), player(2, "B")], 2_400);
        let d = build(&raw, &enc).expect("extension present");
        assert_eq!(d.len(), 2);
        let a = &d[0];

        // Per-ally: index 1 is B (500 + 300 + 50 healing, 300 downed);
        // index 0 is A's own self-heal (250). Barrier is its own array.
        assert_eq!(a.ally_healing[1], AllyHealing { healing: 850, downed_healing: 300 });
        assert_eq!(a.ally_healing[0], AllyHealing { healing: 250, downed_healing: 0 });
        assert_eq!(a.ally_barrier[1], 400);
        assert_eq!(a.ally_barrier[0], 0);

        // Per-skill: 100 -> 800 over 2 hits (min 300, max 500, 300 downed);
        // 200 -> 250; 400 -> indirect. Barrier dist holds only skill 300.
        let s100 = a.healing_dist.iter().find(|e| e.skill_id == 100).unwrap();
        assert_eq!((s100.total, s100.hits, s100.min, s100.max, s100.total_downed), (800, 2, 300, 500, 300));
        assert!(!s100.indirect);
        let s400 = a.healing_dist.iter().find(|e| e.skill_id == 400).unwrap();
        assert!(s400.indirect, "a buff!=0 row is EI's EXTNonDirectHealingEvent");
        assert_eq!(a.barrier_dist.len(), 1);
        assert_eq!(a.barrier_dist[0].skill_id, 300);
        assert_eq!(a.barrier_dist[0].total, 400);
        assert_eq!(a.barrier_dist[0].total_downed, 0);

        // Skill ids ascending.
        assert!(a.healing_dist.windows(2).all(|w| w[0].skill_id < w[1].skill_id));

        // Series: EI's grid for 2400ms is 4 buckets, ceiling-indexed, and
        // CUMULATIVE. t=0 -> bucket 0 (500); t=1500 -> bucket 2 (300+250);
        // t=2400 -> bucket 3 (50). Barrier is not in the healing graph.
        assert_eq!(a.healing_1s, vec![500, 500, 1050, 1100]);

        // B healed nobody.
        assert!(d[1].healing_dist.is_empty());
        assert!(d[1].healing_1s.iter().all(|&v| v == 0));
    }

    /// `healing_1s`'s last element must equal the scalar
    /// `healing_out_total` -- the shared-producer invariant this module's
    /// doc comment claims. Includes a heal to an agent that is NOT one of
    /// the enumerated players, which the ally array cannot represent.
    #[test]
    fn series_total_matches_the_scalar_even_with_a_non_player_recipient() {
        let raw = raw_from(vec![
            registration(),
            instid_reg(0, 1, 10),
            instid_reg(0, 2, 20),
            // 99 is a pet-shaped agent: registered, but not in `enc.players`.
            instid_reg(0, 99, 90),
            heal(0, 10, 20, 100, 500, false),
            heal(500, 10, 90, 100, 700, false),
        ]);
        let enc = enc_with(vec![player(1, "A"), player(2, "B")], 1_000);
        let d = build(&raw, &enc).expect("extension present");
        let a = &d[0];
        assert_eq!(*a.healing_1s.last().unwrap(), 1200);
        assert_eq!(a.healing_dist.iter().map(|e| e.total).sum::<u64>(), 1200);
        // ...while the ally array only sees the 500 that landed on a player.
        assert_eq!(a.ally_healing.iter().map(|c| c.healing).sum::<u64>(), 500);

        // Cross-check against the scalar pass over the same log.
        let mut players = vec![
            crate::analysis::PlayerMetrics { agent_addr: 1, ..Default::default() },
            crate::analysis::PlayerMetrics { agent_addr: 2, ..Default::default() },
        ];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        healing::apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 1200);
    }

    /// Incoming is the transpose of outgoing: every healing/barrier amount
    /// that lands on an enumerated ally must appear in that ALLY's
    /// received series, on the same grid, cumulative in the same way.
    /// Three players (A, B, C) plus a pet-shaped agent (99, registered but
    /// not in `enc.players`) exercise the transpose across two buckets:
    /// A heals B (bucket 0) and downed-heals C (bucket 2); B heals A
    /// (bucket 2); A barriers B and C barriers A (both bucket 2); and A
    /// heals the pet (bucket 3) -- a heal with no ally row to land on.
    #[test]
    fn received_series_is_the_transpose_of_outgoing() {
        let raw = raw_from(vec![
            registration(),
            instid_reg(0, 1, 10),
            instid_reg(0, 2, 20),
            instid_reg(0, 3, 30),
            // 99 is a pet-shaped agent: registered, but not in `enc.players`.
            instid_reg(0, 99, 90),
            heal(0, 10, 20, 100, 500, false),      // A -> B
            heal(1_500, 10, 30, 100, 300, true),   // A -> C, downed
            heal(1_500, 20, 10, 200, 250, false),  // B -> A
            barrier(2_000, 10, 20, 300, 400),      // A -> B
            barrier(2_000, 30, 10, 300, 150),      // C -> A
            heal(2_400, 10, 90, 100, 700, false),  // A -> pet (no ally row)
        ]);
        let enc = enc_with(vec![player(1, "A"), player(2, "B"), player(3, "C")], 2_400);
        let d = build(&raw, &enc).expect("extension present");
        let buckets = d[0].healing_1s.len();

        for p in &d {
            assert_eq!(p.healing_received_1s.len(), buckets, "same grid as healing_1s");
            assert_eq!(p.barrier_received_1s.len(), buckets, "same grid as healing_1s");
            assert!(p.healing_received_1s.windows(2).all(|w| w[1] >= w[0]));
            assert!(p.barrier_received_1s.windows(2).all(|w| w[1] >= w[0]));
        }

        // A received B's 250 heal in bucket 2, and C's 150 barrier in
        // bucket 2. A's own outgoing heal to the pet does NOT appear here.
        assert_eq!(d[0].healing_received_1s, vec![0, 0, 250, 250]);
        assert_eq!(d[0].barrier_received_1s, vec![0, 0, 150, 150]);
        // B received A's 500 heal in bucket 0 and 400 barrier in bucket 2.
        assert_eq!(d[1].healing_received_1s, vec![500, 500, 500, 500]);
        assert_eq!(d[1].barrier_received_1s, vec![0, 0, 400, 400]);
        // C received A's 300 downed heal in bucket 2, no barrier.
        assert_eq!(d[2].healing_received_1s, vec![0, 0, 300, 300]);
        assert_eq!(d[2].barrier_received_1s, vec![0, 0, 0, 0]);

        // The pet-heal proves the ally-attribution boundary: it inflates
        // A's OUTGOING healing_1s (last element 1500 = 500 + 300 + 700)
        // but has no receiver row to land on, so it is invisible to every
        // player's healing_received_1s.
        assert_eq!(*d[0].healing_1s.last().unwrap(), 1500);

        // Squad-wide, the last cumulative value of every RECEIVED series
        // must equal the summed ally_healing across every healer --
        // received is ally-attributed, unlike healing_1s which counts
        // every outgoing heal whether or not the recipient is an
        // enumerated ally.
        let received_total: u64 = d.iter().map(|p| *p.healing_received_1s.last().unwrap()).sum();
        let ally_total: u64 =
            d.iter().flat_map(|p| p.ally_healing.iter().map(|c| c.healing)).sum();
        assert_eq!(received_total, ally_total);

        let barrier_received_total: u64 =
            d.iter().map(|p| *p.barrier_received_1s.last().unwrap()).sum();
        let barrier_total: u64 = d.iter().flat_map(|p| p.ally_barrier.iter().copied()).sum();
        assert_eq!(barrier_received_total, barrier_total);
    }
}
