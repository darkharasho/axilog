//! Per-player per-second series + dpsTargets (M12, Task 2): `damage`/
//! `damage_taken`/`per_target` cumulative-per-second buckets, plus a
//! per-enemy dps/damage summary (`dps_targets`).
//!
//! ## Cumulative vs instant (GW2EI citation)
//!
//! GW2EI's `players[].damage1S`/`damageTaken1S`/`targetDamage1S` are
//! **cumulative** per-second running totals, NOT per-second deltas --
//! verified directly against a real dps.report WvW export
//! (`axibridge/test-fixtures/boon/20260117-181030.json`, the same source
//! `skill_damage`'s calibration and `fixtures/wvw-small.ei.json`'s golden
//! extraction use): every player's `damage1S[0]` array is monotonic
//! non-decreasing and its LAST element equals `dpsAll[0].damage` exactly
//! (e.g. player index 0: `damage1S[0][-1] == 96084 == dpsAll[0].damage`);
//! `damageTaken1S[0]`'s last element likewise equals `defenses[0].
//! damageTaken` exactly. This matches `GW2EIEvtcParser`'s
//! `Get1SDamageList`-family helpers, which build these arrays as running
//! sums over the actor's damage log, not fixed-window deltas. This
//! module's `build` therefore accumulates the SAME per-bucket delta
//! `cc::timeline` computes for its squad-wide `squad_damage` (an instant,
//! per-second amount), then converts each player's own delta series to a
//! running (cumulative) prefix sum before returning it -- so the final
//! element of `damage`/`damage_taken`/each `per_target[].damage` always
//! equals that player's whole-fight total, exactly, by construction (see
//! `tests/timeseries_golden.rs`'s exact-equality assertions).
//!
//! ## Predicate family
//!
//! Every predicate here mirrors `analysis::damage`'s established
//! definitions EXACTLY (statechange/activation/buffremove excluded;
//! `result == CROWD_CONTROL` excluded since `value`/`buff_dmg` there carry
//! CC duration, not damage; `buff == 1 -> buff_dmg` else `value`, clamped
//! `>= 0`), plus the same time-aware `InstidRegistry`-based friendly pet/
//! minion credit `damage::pet_credit_events`/`cc::timeline` already use --
//! this module reuses `pet_credit_events` directly rather than
//! re-implementing pet resolution a third time. Bucketing is GW2EI's own 1S grid --
//! see [`ei_grid`]/[`ei_bucket`] for the two citations
//! (`InterpolatedGraph.cs:13-21` for the length,
//! `SingleActorGraphsHelper.cs:118` for the CEILING bucket index) and for
//! why, as of MEIGAP Task 2, it is deliberately NOT the same grid
//! `cc::timeline` uses for the native `Timeline.squad_damage` (that one
//! keeps its floor-bucketed `duration/1000 + 1` scheme). A native consumer
//! overlaying the two must account for the ceiling offset and the extra
//! trailing bucket; the whole-fight totals still agree, only the alignment
//! differs.
//!
//! ## `dps_targets`
//!
//! Derived from the SAME per-(player, enemy) delta map `per_target` is
//! built from (just reduced to a single final `{damage, dps}` pair per
//! enemy instead of a full per-second series), so `sum(dps_targets[*].
//! damage) == PlayerMetrics::per_enemy`'s own total EXACTLY for every
//! player -- an internal invariant asserted directly in this module's unit
//! tests and in the golden test. This can only be checked for INTERNAL
//! consistency against GW2EI's own `dpsTargets`, not matched value-for-
//! value: GW2EI's non-`detailedWvW` WvW export collapses every enemy
//! PLAYER into one synthetic `"Enemy Players"` target and silently drops
//! damage dealt to non-player-enemy NPCs (siege equipment, dolyaks,
//! guards) from `targets[]`/`dpsTargets` entirely -- still counted in the
//! player's own `damage1S`/`dpsAll[0].damage` scalar, just not
//! attributable to any target row (verified on the same source fixture:
//! player index 0's `targetDamage1S`/`dpsTargets` sum to `71362` while
//! `damage1SFinal`/`dpsAll[0].damage` is `96084`, a `24722` gap -- see
//! `fixtures/wvw-small.ei.json`'s `_note` for the full writeup). This
//! project's own `enemies`/`per_enemy` DO enumerate every NPC the log
//! tracks (`Metrics::combat_participant_enemies`'s doc comment), so this
//! module's `dps_targets` is a strict superset of what GW2EI's own
//! `dpsTargets` can show on this fixture -- a real per-enemy value match is
//! only meaningful against a `detailedWvW` export, which this project does
//! not have (same limitation `skill_damage`'s `per_target` module doc
//! already documents for the exact same reason).

//! ## The POWER split (MEIGAP Task 2a/2b)
//!
//! GW2EI emits a three-way `All`/`Power`/`Condition` split of every one of
//! these series: `damageTaken1S`/`powerDamageTaken1S`/
//! `conditionDamageTaken1S` (`GW2EIBuilders/JsonModels/JsonActors/
//! JsonPlayerBuilder.cs:75-78`), `targetDamage1S`/`targetPowerDamage1S`/
//! `targetConditionDamage1S` (`:99-101`) and, on the NPC side,
//! `damage1S`/`powerDamage1S`/`conditionDamage1S`
//! (`JsonActorBuilder.cs:108-110`). All three come from the same
//! `GetDamageGraph`/`GetDamageTakenGraph` call with a different
//! `ParserHelper.DamageType`, and the filter itself is a one-liner
//! (`EIData/Actors/Actor.cs:449-451`):
//!
//! ```text
//! case ParserHelper.DamageType.Power:
//!     dls.RemoveAll(x => x.ConditionDamageBased(log));
//!     break;
//! ```
//!
//! So **POWER == every damage row whose skill id is NOT in GW2EI's
//! Condition buff catalog** -- strike damage AND life-leech AND the
//! "fourth bucket" (`buff == 1`, non-catalogued, non-life-leech), exactly
//! the bucket `defenses::DefenseStats::power_damage` already carries, and
//! decided by exactly the predicate `analysis::condition_catalog::
//! is_condition_damage_based` already implements. It is emphatically NOT
//! `strike + life_leech` (see `defenses`'s module doc for the MCONDCAT
//! measurement that separated those two readings, up to 51.4% relative on
//! a real capture).
//!
//! `conditionDamage1S` is the complement and is deliberately NOT emitted:
//! the audit's gap rows name only the `power` half, and axibridge reads
//! only the `power` half (`computeIncomingStrikeDamageData.ts:101,108`,
//! `computeCommanderStats.ts:536`). Adding the third series would be pure
//! payload for no consumer.
//!
//! ## Enemy-side series ([`EnemySeries`], MEIGAP Task 2b)
//!
//! `targets[].damage1S`/`.powerDamage1S` are built by the SHARED actor
//! builder (`JsonActorBuilder.FillJsonActor`, which `JsonNPCBuilder.
//! BuildJsonNPC:20` calls first thing), so on an NPC they mean exactly what
//! they mean on a player: **damage dealt BY that actor**, to anyone
//! (`GetDamageGraph(log, start, end, null, ...)` -- a `null` target, not a
//! null source). Direction independently confirmed on the consumer side:
//! axibridge's `computeIncomingStrikeDamageData.ts:100-104` reads
//! `target.powerDamage1S` as the per-second strike damage that enemy class
//! PUT OUT, and falls back to summing squad `powerDamageTaken1S` when it is
//! missing (`:361-364`) -- i.e. the two are the same quantity seen from
//! opposite ends.
//!
//! Minion scope matches EI's: `SingleActor.InitDamageEvents`
//! (`EIData/Actors/SingleActor.cs:735-740`) folds `mins.GetDamageEvents`
//! into the actor's damage list, so an NPC's `damage1S` INCLUDES its
//! minions -- the mirror image of the friendly pet-credit fold this module
//! already applies on the squad side, and reproduced here with the same
//! `InstidRegistry` master resolution. (Contrast `totalDamageDist`, which
//! uses `GetJustActorDamageEvents` and is actor-only -- see
//! `skill_damage::build_enemy_dist`.)

use super::damage;
use crate::analysis::condition_catalog;
use crate::evtc::{result, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One enemy's cumulative per-second outgoing-damage series (sorted by
/// `enemy_id` ascending in the returned [`TimeseriesMetrics::per_target`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetSeries {
    pub enemy_id: u64,
    pub damage: Vec<u64>,
    /// The non-condition half of `damage` (`targetPowerDamage1S`) -- same
    /// buckets, same cumulative shape, filtered by
    /// `condition_catalog::is_condition_damage_based`. Always
    /// element-wise `<= damage`.
    pub power_damage: Vec<u64>,
}

/// One enemy's whole-fight dps/damage summary (`dpsTargets`, sorted by
/// `enemy_id` ascending). `damage` is the same final value `per_target`'s
/// matching series ends on; `dps` divides it by the encounter duration in
/// seconds (same `secs = (duration_ms / 1000.0).max(1.0)` convention
/// `analysis::mod::analyze`'s own `PlayerMetrics::dps` uses). No `Eq`
/// derive here (unlike its siblings in this module) since `dps` is a plain
/// `f64` -- matches `PlayerMetrics::dps`'s own existing convention of not
/// deriving `PartialEq`/`Eq` at all for an `f64`-carrying struct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DpsTargetEntry {
    pub enemy_id: u64,
    pub damage: u64,
    pub dps: f64,
}

/// A player's full per-second detail: cumulative `damage`/`damage_taken`
/// (whole-fight, any source/target), a cumulative `per_target` breakdown,
/// and the reduced `dps_targets` summary. All three of `damage`/
/// `damage_taken`/every `per_target[].damage` share the same length
/// (`buckets`, one entry per second) and the same bucket alignment as
/// `cc::timeline`'s `Timeline.squad_damage`. No `Eq` derive (see
/// `DpsTargetEntry`'s doc comment: it carries a plain `f64`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeseriesMetrics {
    pub damage: Vec<u64>,
    pub damage_taken: Vec<u64>,
    /// The non-condition half of `damage_taken` (`powerDamageTaken1S`,
    /// MEIGAP Task 2a) -- see this module's POWER-split section. Always
    /// element-wise `<= damage_taken`, and its final element always equals
    /// `DefenseStats::power_damage` for the same player (both are the same
    /// `!is_condition_damage_based` filter over the same incoming rows;
    /// asserted in `tests/timeseries_golden.rs`).
    pub power_damage_taken: Vec<u64>,
    pub per_target: Vec<TargetSeries>,
    pub dps_targets: Vec<DpsTargetEntry>,
}

/// One ENEMY's cumulative per-second OUTGOING damage series -- GW2EI's
/// `targets[].damage1S` / `targets[].powerDamage1S` (MEIGAP Task 2b). See
/// this module's enemy-side section for the direction citation and the
/// minion-inclusive scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnemySeries {
    /// The enemy's representative id (`Enemy::id`), same key space
    /// `TargetSeries::enemy_id` and `Report::all_enemies` use.
    pub enemy_id: u64,
    pub damage: Vec<u64>,
    pub power_damage: Vec<u64>,
}

/// GW2EI's 1S grid, reproduced exactly (MEIGAP Task 2).
///
/// **Length** -- `InterpolatedGraph`'s ctor
/// (`GW2EIEvtcParser/EIData/MathUtils/InterpolatedGraph.cs:13-21`):
///
/// ```text
/// int durationInMS = (int)(last - first);
/// int durationInS  = durationInMS / 1000;
/// _values = durationInS * 1000 != durationInMS ? new T[durationInS + 2]
///                                              : new T[durationInS + 1];
/// ```
///
/// i.e. `+2` slots whenever the log length is not a whole number of
/// seconds, `+1` when it is. On this project's post-rework capture
/// (`durationMS = 348362`) that is **350**, where the pre-MEIGAP formula
/// (`duration/1000 + 1`) produced 349.
///
/// **Bucket index** -- `ComputeDamageGraph`
/// (`EIData/Actors/ActorsHelper/SingleActorGraphsHelper.cs:118`):
/// `(int)Math.Ceiling((dl.Time - start) / 1000.0)` -- a CEILING, so a hit
/// at `t = 1ms` lands in bucket 1, not bucket 0, and only a hit exactly on
/// a second boundary stays in its own. The pre-MEIGAP formula floored,
/// which put every mid-second hit one bucket early.
///
/// Both corrections were found while calibrating `powerDamageTaken1S`
/// against the reference export and apply to the WHOLE `*1S` family
/// (`damage1S`/`damageTaken1S`/`targetDamage1S` included, which M12 only
/// ever calibrated on their FINAL element -- a value neither correction
/// moves, which is why the divergence survived until now).
///
/// This grid is deliberately NOT shared with `cc::timeline`'s native
/// `Timeline.squad_damage`, which keeps its own floor-bucketed
/// `duration/1000 + 1` scheme: that is a native surface with its own
/// consumers, not an EI mirror. The two are no longer bucket-aligned, and
/// this module's doc comment says so.
fn ei_grid(duration_ms: u64) -> usize {
    let secs = duration_ms / 1000;
    if secs * 1000 != duration_ms { (secs + 2) as usize } else { (secs + 1) as usize }
}

/// `Math.Ceiling((t - t0) / 1000.0)`, clamped to the grid (see [`ei_grid`]).
fn ei_bucket(t: u64, t0: u64, buckets: usize) -> Option<usize> {
    let rel = t.saturating_sub(t0);
    let b = (rel / 1000 + u64::from(rel % 1000 != 0)) as usize;
    if b < buckets {
        Some(b)
    } else {
        None
    }
}

/// Add `src`'s per-bucket deltas into `dst` element-wise. Both are always
/// the same `buckets`-length by construction (every delta vec allocated via
/// `vec![0u64; buckets]` before any entry is ever written to it).
fn add_into(dst: &mut [u64], src: &[u64]) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d += s;
    }
}

/// Turn a per-bucket delta series into GW2EI's cumulative running-total
/// shape (see this module's doc comment for the citation).
fn cumulative(deltas: &[u64]) -> Vec<u64> {
    let mut running = 0u64;
    deltas
        .iter()
        .map(|&d| {
            running += d;
            running
        })
        .collect()
}

/// Compute per-squad-player per-second series (account-folded, enemy-
/// representative-folded) plus `dps_targets`. `addr_to_rep`/
/// `enemy_addr_to_rep`/`friendly_team`/`agent_team` are the same values
/// `analysis::mod::analyze`'s other per-player passes (`skill_damage::
/// build`, `damage::accumulate_pet_credit`) already thread through --
/// this fn needs one more parameter than `skill_damage::build` (`enc`,
/// purely for `enc.duration_ms`'s bucket-count derivation), which is what
/// pushes it past clippy's default 7-argument threshold; same accepted
/// shape this crate's `contribution::credit_window` already carries
/// (8 args, no lint suppression there either) -- every parameter here is
/// load-bearing, not incidental, so this is suppressed rather than forced
/// into an artificial options-struct wrapper.
#[allow(clippy::too_many_arguments)]
pub fn build(
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, TimeseriesMetrics> {
    build_with_registry(
        enc,
        raw,
        &damage::InstidRegistry::build(raw),
        squad,
        enemies,
        addr_to_rep,
        enemy_addr_to_rep,
        friendly_team,
        agent_team,
    )
}

/// [`build`] against a caller-supplied, already-built
/// [`damage::InstidRegistry`] (MPERF Task 2) -- see
/// [`damage::accumulate_pet_credit_with_registry`]'s doc comment for why the
/// registry is threaded rather than rebuilt per consumer. The `raw`-only
/// wrapper above stays for standalone/test callers.
#[allow(clippy::too_many_arguments)]
pub fn build_with_registry(
    enc: &Encounter,
    raw: &RawLog,
    registry: &damage::InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, TimeseriesMetrics> {
    let buckets = ei_grid(enc.duration_ms);
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);
    let bucket_of = |t: u64| -> Option<usize> { ei_bucket(t, t0, buckets) };

    // Per-bucket DELTAS, keyed by raw addr first (folded to account/enemy
    // representatives afterward, same two-pass shape `skill_damage::build`
    // uses).
    let mut dmg_delta: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut taken_delta: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut power_taken_delta: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut target_delta: BTreeMap<u64, BTreeMap<u64, Vec<u64>>> = BTreeMap::new();
    let mut target_power_delta: BTreeMap<u64, BTreeMap<u64, Vec<u64>>> = BTreeMap::new();

    // Direct squad<->enemy events -- mirrors `damage::accumulate`'s
    // (outgoing) and `damage::accumulate_damage_taken`'s (incoming)
    // predicates exactly, evaluated together in one pass since both read
    // the same `dmg`/exclusions off the same event.
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 {
            continue;
        }
        let Some(b) = bucket_of(e.time) else { continue };
        // GW2EI's `DamageType.Power` filter, verbatim: everything that is
        // not `ConditionDamageBased` (see this module's POWER-split
        // section). One `const`-array membership test per already-filtered
        // damage row.
        let is_power = !condition_catalog::is_condition_damage_based(e.skillid);

        if squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            dmg_delta.entry(e.src_agent).or_insert_with(|| vec![0u64; buckets])[b] += dmg;
            target_delta
                .entry(e.src_agent)
                .or_default()
                .entry(e.dst_agent)
                .or_insert_with(|| vec![0u64; buckets])[b] += dmg;
            if is_power {
                target_power_delta
                    .entry(e.src_agent)
                    .or_default()
                    .entry(e.dst_agent)
                    .or_insert_with(|| vec![0u64; buckets])[b] += dmg;
            }
        }
        if squad.contains(&e.dst_agent) {
            taken_delta.entry(e.dst_agent).or_insert_with(|| vec![0u64; buckets])[b] += dmg;
            if is_power {
                power_taken_delta.entry(e.dst_agent).or_insert_with(|| vec![0u64; buckets])[b] +=
                    dmg;
            }
        }
    }

    // Friendly pet/minion damage credited to the owning squad player --
    // reuses `damage::pet_credit_events` directly (same time-aware
    // `InstidRegistry` owner resolution `cc::timeline`'s own `squad_damage`
    // loop and `skill_damage`'s pet-credit pass already use), so this
    // module never re-derives pet ownership independently.
    for (time, owner, dst, dmg, skill_id) in
        damage::pet_credit_events_with_skill(raw, registry, squad, friendly_team, agent_team)
    {
        let Some(b) = bucket_of(time) else { continue };
        dmg_delta.entry(owner).or_insert_with(|| vec![0u64; buckets])[b] += dmg;
        target_delta
            .entry(owner)
            .or_default()
            .entry(dst)
            .or_insert_with(|| vec![0u64; buckets])[b] += dmg;
        if !condition_catalog::is_condition_damage_based(skill_id) {
            target_power_delta
                .entry(owner)
                .or_default()
                .entry(dst)
                .or_insert_with(|| vec![0u64; buckets])[b] += dmg;
        }
    }

    // Fold every source addr onto its account representative (relog/build-
    // swap safe, same as every other per-player pass in this crate), and
    // every destination addr onto its enemy representative.
    let mut dmg_by_rep: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (addr, deltas) in &dmg_delta {
        let rep = addr_to_rep.get(addr).copied().unwrap_or(*addr);
        add_into(dmg_by_rep.entry(rep).or_insert_with(|| vec![0u64; buckets]), deltas);
    }
    let mut taken_by_rep: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (addr, deltas) in &taken_delta {
        let rep = addr_to_rep.get(addr).copied().unwrap_or(*addr);
        add_into(taken_by_rep.entry(rep).or_insert_with(|| vec![0u64; buckets]), deltas);
    }
    let mut power_taken_by_rep: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (addr, deltas) in &power_taken_delta {
        let rep = addr_to_rep.get(addr).copied().unwrap_or(*addr);
        add_into(power_taken_by_rep.entry(rep).or_insert_with(|| vec![0u64; buckets]), deltas);
    }
    let fold_targets = |src: &BTreeMap<u64, BTreeMap<u64, Vec<u64>>>| {
        let mut out: BTreeMap<u64, BTreeMap<u64, Vec<u64>>> = BTreeMap::new();
        for (addr, by_dst) in src {
            let rep = addr_to_rep.get(addr).copied().unwrap_or(*addr);
            let entry = out.entry(rep).or_default();
            for (dst, deltas) in by_dst {
                let dst_rep = enemy_addr_to_rep.get(dst).copied().unwrap_or(*dst);
                add_into(entry.entry(dst_rep).or_insert_with(|| vec![0u64; buckets]), deltas);
            }
        }
        out
    };
    let target_by_rep = fold_targets(&target_delta);
    let target_power_by_rep = fold_targets(&target_power_delta);

    // Every squad account representative gets a full, zero-filled entry
    // (not just the ones that dealt/took damage) -- `addr_to_rep`'s VALUE
    // set is exactly the set of `Player.agent_addr`s (`analysis::mod::
    // analyze`'s own construction: every one of a player's `agent_addrs`
    // maps to that same single `p.agent_addr`), matching GW2EI's own
    // `damage1S`/`damageTaken1S`, which are always full-length arrays even
    // for a player who never dealt or took any damage.
    let all_reps: BTreeSet<u64> = addr_to_rep.values().copied().collect();
    // One shared all-zero row for every "this player/enemy has no series"
    // fallback below. Hoisted out of the loop deliberately: `per_target`'s
    // POWER fallback fires once per (player, enemy) PAIR, and on a real
    // WvW log (44 players x 624 enemies x 350 buckets) a per-call
    // `vec![0; buckets]` there was measurable in `analyze`.
    let zeros: Vec<u64> = vec![0u64; buckets];
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut result: BTreeMap<u64, TimeseriesMetrics> = BTreeMap::new();
    for rep in all_reps {
        let damage = cumulative(dmg_by_rep.get(&rep).map(Vec::as_slice).unwrap_or(&zeros));
        let damage_taken =
            cumulative(taken_by_rep.get(&rep).map(Vec::as_slice).unwrap_or(&zeros));
        let power_damage_taken =
            cumulative(power_taken_by_rep.get(&rep).map(Vec::as_slice).unwrap_or(&zeros));

        // The power series is keyed by the SAME enemy set as `damage`, not
        // by its own (narrower) key set: GW2EI emits `targetDamage1S` and
        // `targetPowerDamage1S` as two arrays of identical outer length,
        // both indexed by `targets[]` position, so an enemy this player hit
        // with conditions only must still get a full-length all-zero power
        // series rather than being dropped.
        let empty = BTreeMap::new();
        let power_for = target_power_by_rep.get(&rep).unwrap_or(&empty);
        let mut per_target: Vec<TargetSeries> = target_by_rep
            .get(&rep)
            .map(|m| {
                m.iter()
                    .map(|(&enemy_id, deltas)| TargetSeries {
                        enemy_id,
                        damage: cumulative(deltas),
                        power_damage: cumulative(
                            power_for.get(&enemy_id).map(Vec::as_slice).unwrap_or(&zeros),
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        per_target.sort_by_key(|t| t.enemy_id);

        let mut dps_targets: Vec<DpsTargetEntry> = per_target
            .iter()
            .map(|t| {
                let total = t.damage.last().copied().unwrap_or(0);
                DpsTargetEntry { enemy_id: t.enemy_id, damage: total, dps: total as f64 / secs }
            })
            .collect();
        dps_targets.sort_by_key(|d| d.enemy_id);

        result.insert(
            rep,
            TimeseriesMetrics {
                damage,
                damage_taken,
                power_damage_taken,
                per_target,
                dps_targets,
            },
        );
    }
    result
}

/// Per-ENEMY cumulative outgoing-damage series -- GW2EI's
/// `targets[].damage1S` / `targets[].powerDamage1S` (MEIGAP Task 2b). See
/// this module's enemy-side section for the direction citation
/// (`JsonActorBuilder.FillJsonActor:108-110` over an NPC actor) and the
/// minion-inclusive scope (`SingleActor.cs:735-740`).
///
/// **Standalone, NOT wired into `analyze()`** -- the same opt-in convention
/// `replay`/`missiles`/`damage_mods`/`buffs::states` use. It is a second
/// full pass over `raw.events` whose only consumer is the ei-json adapter's
/// `targets[]` block, which is itself gated on `--timeseries` (GW2EI's own
/// `RawFormatTimelineArrays` gate on these two arrays), so a flagless parse
/// must not pay for it.
///
/// `enemies`/`enemy_addr_to_rep` are the FULL roster
/// (`Encounter::enemies`), matching `Report::all_enemies` -- the same set
/// the adapter's `targets[]` array is built from, so every emitted target
/// has a series and the two stay positionally aligned.
///
/// Returned entries are sorted by `enemy_id` ascending (a `BTreeMap`
/// collect), and only enemies that actually dealt damage appear; the
/// adapter zero-fills the rest.
pub fn build_enemy_series(
    enc: &Encounter,
    raw: &RawLog,
    registry: &damage::InstidRegistry,
    enemies: &BTreeSet<u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, EnemySeries> {
    let buckets = ei_grid(enc.duration_ms);
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);

    let mut dmg: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut power: BTreeMap<u64, Vec<u64>> = BTreeMap::new();

    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        let dmg_v = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg_v == 0 {
            continue;
        }
        // The enemy this row is credited to. The MASTER is tried FIRST, and
        // that order is load-bearing: this project's `Encounter::enemies` is
        // the full unfiltered agent roster, so an enemy's minion is itself
        // a tracked "enemy" -- crediting the source to itself first would
        // leave every minion's damage on the minion's own row and the
        // master's `damage1S` short by exactly its minions' output, which is
        // what GW2EI's `InitDamageEvents` fold (`SingleActor.cs:735-740`)
        // puts there. Measured on the reference export before the order was
        // fixed: the ONLY three joined targets that disagreed were the only
        // three with minions (two Virtuosos' Illusionary Berserkers, a
        // Druid's two juvenile pets), each short by precisely its minion
        // total.
        //
        // No double-count results: a folded row is credited once, to the
        // master, and the minion's own target row simply reports nothing
        // for it -- the same thing GW2EI's curated `targets[]` shows, which
        // never lists the minion as a target at all.
        let credited = match registry.resolve_at(e.src_master_instid, e.time) {
            Some(addr) if e.src_master_instid != 0 && enemies.contains(&addr) => addr,
            _ if enemies.contains(&e.src_agent) => e.src_agent,
            _ => continue,
        };
        let Some(b) = ei_bucket(e.time, t0, buckets) else { continue };
        let rep = enemy_addr_to_rep.get(&credited).copied().unwrap_or(credited);
        dmg.entry(rep).or_insert_with(|| vec![0u64; buckets])[b] += dmg_v;
        if !condition_catalog::is_condition_damage_based(e.skillid) {
            power.entry(rep).or_insert_with(|| vec![0u64; buckets])[b] += dmg_v;
        }
    }

    dmg.into_iter()
        .map(|(enemy_id, deltas)| {
            let power_damage = cumulative(
                power.get(&enemy_id).map(Vec::as_slice).unwrap_or(&vec![0u64; buckets]),
            );
            (enemy_id, EnemySeries { enemy_id, damage: cumulative(&deltas), power_damage })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;
    use crate::model::{Encounter, Player};

    fn strike(time: u64, src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: dmg, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn enc_with_duration(duration_ms: u64) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms, build: "".into(), revision: 1,
            recorded_by: None, teams: vec![], players: vec![], enemies: vec![],
            markers: vec![], tick_rate: None,
        }
    }

    /// Two hits landing in two different 1000ms buckets must accumulate:
    /// bucket 0 shows the first hit's value, bucket 1 shows BOTH hits
    /// summed (cumulative, per the module doc's GW2EI citation), and every
    /// later bucket repeats the final total.
    #[test]
    fn buckets_are_cumulative_across_second_boundaries() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![strike(0, 1, 9, 50), strike(1200, 1, 9, 30)]);
        // duration 2500 is not a whole number of seconds, so EI's
        // `InterpolatedGraph` allocates `2 + 2 == 4` slots (see `ei_grid`).
        let enc = enc_with_duration(2500);

        let result = build(
            &enc, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), None, &BTreeMap::new(),
        );
        let m = result.get(&1).expect("player present");
        // t=0 -> ceil(0/1000) == bucket 0; t=1200 -> ceil(1.2) == bucket 2
        // (NOT bucket 1: EI's grid CEILS, see `ei_bucket`).
        assert_eq!(m.damage, vec![50, 50, 80, 80], "cumulative running total, not per-bucket delta");
        assert_eq!(m.damage.last().copied().unwrap(), 80, "final bucket == whole-fight total");
    }

    /// `damage_taken` mirrors `damage::accumulate_damage_taken`'s
    /// predicate: ANY source, restricted only by `dst` being in `squad`.
    #[test]
    fn damage_taken_any_source_cumulative() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![strike(0, 9, 1, 40), strike(500, 10, 1, 20)]);
        let enc = enc_with_duration(1500);

        let result = build(
            &enc, &raw, &squad, &BTreeSet::new(), &addr_to_rep, &BTreeMap::new(), None,
            &BTreeMap::new(),
        );
        let m = result.get(&1).unwrap();
        // duration 1500 -> 3 slots; t=0 -> bucket 0, t=500 -> ceil(0.5) == 1.
        assert_eq!(m.damage_taken, vec![40, 60, 60]);
    }

    /// `per_target`/`dps_targets` split correctly per enemy, and
    /// `sum(dps_targets[*].damage) == sum(per_target[*].damage.last())`
    /// (trivially, by construction) `== damage_total`-equivalent
    /// (`m.damage.last()`) -- the internal invariant this module's golden
    /// test re-verifies against real per-enemy totals.
    #[test]
    fn per_target_and_dps_targets_split_and_sum_correctly() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64, 10u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![strike(0, 1, 9, 50), strike(0, 1, 10, 20), strike(1200, 1, 9, 10)]);
        let enc = enc_with_duration(2000);

        let result = build(
            &enc, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), None, &BTreeMap::new(),
        );
        let m = result.get(&1).unwrap();
        assert_eq!(m.per_target.len(), 2);
        // duration_ms=2000 is a whole number of seconds -> `2 + 1 == 3`
        // slots. Both t=0 hits land in bucket 0; t=1200 ceils to bucket 2.
        let t9 = m.per_target.iter().find(|t| t.enemy_id == 9).unwrap();
        assert_eq!(t9.damage, vec![50, 50, 60]);
        let t10 = m.per_target.iter().find(|t| t.enemy_id == 10).unwrap();
        assert_eq!(t10.damage, vec![20, 20, 20]);

        let dps_sum: u64 = m.dps_targets.iter().map(|d| d.damage).sum();
        assert_eq!(dps_sum, 80);
        assert_eq!(m.damage.last().copied().unwrap(), 80);
        let d9 = m.dps_targets.iter().find(|d| d.enemy_id == 9).unwrap();
        assert_eq!(d9.damage, 60);
        assert!((d9.dps - 30.0).abs() < 1e-9, "60 damage / 2s duration == 30 dps");
    }

    /// Pet/minion damage must fold onto the owner's series via the same
    /// `InstidRegistry` resolution `damage::pet_credit_events` provides --
    /// mirrors `skill_damage`'s equivalent pet-credit test.
    #[test]
    fn pet_damage_folds_onto_owner() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        let friendly_team = Some(10u32);
        let agent_team: BTreeMap<u64, u32> = [(1u64, 10u32), (300u64, 10u32)].into_iter().collect();

        fn ev(time: u64, src: u64, src_instid: u16, master: u16, dst: u64, v: i32) -> RawEvent {
            RawEvent {
                time, src_agent: src, dst_agent: dst, value: v, buff_dmg: 0, overstack: 0,
                skillid: 0, src_instid, dst_instid: 0, src_master_instid: master,
                dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
                is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0,
                is_offcycle: 0, pad: 0,
            }
        }
        let raw = raw_from(vec![
            ev(0, 1, 11, 0, 9, 40),   // player registers instid 11 (own attack)
            ev(0, 300, 77, 11, 9, 15), // pet attack, owner resolves to addr 1
        ]);
        let enc = enc_with_duration(1000);
        let result = build(
            &enc, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), friendly_team,
            &agent_team,
        );
        let m = result.get(&1).unwrap();
        // duration_ms=1000 is whole -> 2 slots; both events at time 0.
        assert_eq!(m.damage, vec![55, 55], "player's own + pet's damage folded onto owner");
    }

    /// A squad player who never dealt or took any damage still gets a
    /// full, zero-filled entry (not omitted) -- matches GW2EI's own
    /// always-present `damage1S`/`damageTaken1S` arrays.
    #[test]
    fn silent_player_gets_full_zero_series() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> =
            [(1u64, 1u64), (2u64, 2u64)].into_iter().collect();
        let raw = raw_from(vec![strike(0, 1, 9, 50)]);
        let enc = enc_with_duration(2000);
        let result = build(
            &enc, &raw, &squad, &BTreeSet::new(), &addr_to_rep, &BTreeMap::new(), None,
            &BTreeMap::new(),
        );
        let m = result.get(&2).expect("silent player still present");
        // duration_ms=2000 is whole -> 3 slots.
        assert_eq!(m.damage, vec![0, 0, 0]);
        assert_eq!(m.damage_taken, vec![0, 0, 0]);
        assert!(m.per_target.is_empty());
        assert!(m.dps_targets.is_empty());
    }

    /// Account fold: a relogged squad member's outgoing damage (two raw
    /// addrs) must sum onto the single representative's series.
    #[test]
    fn relog_folds_outgoing_across_account_addrs() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64), (2u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![strike(0, 1, 9, 50), strike(0, 2, 9, 30)]);
        let enc = enc_with_duration(1000);
        let result = build(
            &enc, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), None, &BTreeMap::new(),
        );
        assert_eq!(result.len(), 1);
        // duration_ms=1000 is whole -> 2 slots.
        assert_eq!(result.get(&1).unwrap().damage, vec![80, 80]);
    }

    /// Crowd-control application rows must be excluded from every series,
    /// same as `damage`/`skill_damage`'s own predicate.
    #[test]
    fn excludes_crowd_control_rows() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        let mut cc = strike(0, 1, 9, 5000);
        cc.result = result::CROWD_CONTROL;
        let raw = raw_from(vec![strike(0, 1, 9, 100), cc]);
        let enc = enc_with_duration(1000);
        let result = build(
            &enc, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), None, &BTreeMap::new(),
        );
        // duration_ms=1000 is whole -> 2 slots.
        assert_eq!(result.get(&1).unwrap().damage, vec![100, 100]);
    }

    /// [`ei_grid`]/[`ei_bucket`] reproduce GW2EI's own 1S grid exactly --
    /// the two corrections MEIGAP Task 2 found while calibrating
    /// `powerDamageTaken1S` (see their doc comments for the citations).
    #[test]
    fn ei_grid_and_bucket_match_gw2ei() {
        // `InterpolatedGraph.cs:18-20`: `+2` slots on a partial second,
        // `+1` on a whole one. The reference capture's 348362 ms -> 350.
        assert_eq!(ei_grid(348_362), 350);
        assert_eq!(ei_grid(2_500), 4);
        assert_eq!(ei_grid(2_000), 3);
        assert_eq!(ei_grid(1_000), 2);
        assert_eq!(ei_grid(0), 1);

        // `SingleActorGraphsHelper.cs:118`: `Math.Ceiling((t - start)/1000)`.
        let n = ei_grid(2_500);
        assert_eq!(ei_bucket(0, 0, n), Some(0), "exactly at start stays in bucket 0");
        assert_eq!(ei_bucket(1, 0, n), Some(1), "1ms in already CEILS to bucket 1");
        assert_eq!(ei_bucket(1_000, 0, n), Some(1), "a whole second stays in its own bucket");
        assert_eq!(ei_bucket(1_001, 0, n), Some(2));
        assert_eq!(ei_bucket(2_500, 0, n), Some(3), "the last slot the +2 allocation exists for");
        assert_eq!(ei_bucket(9_999, 0, n), None, "past the grid is dropped, not clamped");
        // The offset is taken from the log's first event, not from 0.
        assert_eq!(ei_bucket(5_001, 5_000, n), Some(1));
    }

    /// Sanity: a full `Encounter`/`Player` roster shape (not just bare
    /// addr maps) still resolves the same way `analysis::mod::analyze`
    /// would set it up -- guards against `addr_to_rep`'s value-set
    /// assumption (`all_reps`) silently drifting from `Player.agent_addr`.
    #[test]
    fn all_reps_matches_player_agent_addr_convention() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![1, 2], // relogged: two raw addrs, one rep
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![player],
            enemies: vec![], markers: vec![], tick_rate: None,
        };
        let addr_to_rep: BTreeMap<u64, u64> = enc.players.iter()
            .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
            .collect();
        let squad: BTreeSet<u64> = enc.players.iter()
            .flat_map(|p| p.agent_addrs.iter().copied())
            .collect();
        let raw = raw_from(vec![]);
        let result = build(
            &enc, &raw, &squad, &BTreeSet::new(), &addr_to_rep, &BTreeMap::new(), None,
            &BTreeMap::new(),
        );
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&1));
    }
}
