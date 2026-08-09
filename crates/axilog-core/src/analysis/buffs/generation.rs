//! Per-source boon-generation attribution (self/group/squad rollups), M3
//! Task 4.
//!
//! GW2EI's own semantics (verified against source, not guessed -- see
//! citations below): `Player.ComputeBuffs` (`GW2EIEvtcParser/EIData/Actors/
//! Player.cs`) computes three different "generation" views per (player,
//! boon), depending which JSON array they land in:
//!
//! - **`selfBuffs`** (`BuffEnum.Self`, the default arm) ->
//!   `BuffStatistics.GetBuffsForSelf`: for this player as BOTH source and
//!   target, "how much of the boon-time I HELD did I generate for MYSELF"
//!   -- `generationValue = buffDistribution.GetGeneration(buff.ID,
//!   dstActor.AgentItem)` where `buffDistribution` is the player's OWN
//!   received-boon distribution and `dstActor.AgentItem` (the player
//!   itself) is used as the SOURCE key.
//! - **`groupBuffs`** (`BuffEnum.Group`) ->
//!   `BuffStatistics.GetBuffsForPlayers(log.PlayerList.Where(p.Group ==
//!   Group && p != this), ..., srcActor: this)`: averaged, over every OTHER
//!   player in this player's own subgroup, of "how much boon-time did I
//!   (`this`, as source) generate for them".
//! - **`squadBuffs`** (`BuffEnum.Squad`) ->
//!   `BuffStatistics.GetBuffsForPlayers(log.PlayerList.Where(p != this),
//!   ..., srcActor: this)`: the same average, but over the WHOLE squad
//!   (every other `log.PlayerList` member) excluding self -- this is the
//!   brief's "squad-generation" calibration target.
//!
//! Both `GetBuffsForPlayers`'s per-target `generation` term and
//! `GetBuffsForSelf`'s own are `BuffDistribution.GetGeneration(buffID,
//! srcAgent)`, i.e. `BuffDistributionItem.Value` -- populated per
//! (target, source) pair by each buff-stack simulation segment
//! (`BuffSimulationItem{Duration,Intensity}.SetBuffDistributionItem`,
//! `GW2EIEvtcParser/EIData/Buffs/BuffSimulators/BuffSimulationItems/*.cs`):
//! for a DURATION-type (Queue) boon, only the segment's ACTIVE stack
//! (`Stacks[0]`) contributes -- exactly the source occupying the "ticking"
//! slot our own `simulator::run_duration` already models. For an
//! INTENSITY-type boon (Might, Stability), EVERY concurrently-held stack
//! contributes its own held duration to ITS OWN source, regardless of how
//! many other stacks (from other sources) are held at the same time.
//! `phaseDuration` is the log's absolute `[start, end)` window (same as
//! `uptime.rs`'s own denominator -- `BuffStatistics.cs`'s `long
//! phaseDuration = end - start`, never per-player-active-clamped for the
//! non-`*Active` arrays this task consumes). Scale matches `uptime.rs`
//! exactly: duration boons are `*100` (a genuine 0-100 percentage);
//! intensity boons are NOT `*100` (a raw average-concurrent-stack-count
//! number, same convention as `BoonUptime::avg_stacks`).
//!
//! This module re-simulates the SAME per-(target, buff) event streams
//! `simulator::run` already consumes (Task 1/2's verified apply/remove/
//! extend stack machine), but tracks each held stack's SOURCE (the
//! applier's account-representative addr) and accumulates generated-ms per
//! source instead of a stack-count timeline -- see `run_duration_ms`/
//! `run_intensity_ms` below, which mirror `simulator::run_duration`/
//! `run_intensity` branch-for-branch (reusing the same removal-matching
//! helper, `simulator::find_single_removal_match`) so any future fix to
//! the count-timeline model doesn't silently diverge from this one.

use super::events::{self, BuffEvent, BuffEventKind};
use super::simulator::{self, find_single_removal_match};
use super::BOON_IDS;
use crate::evtc::RawLog;
use crate::model::{Encounter, Player};
use std::collections::{BTreeMap, BTreeSet};

/// Self/group/squad generation rollups for one (source player, boon) pair,
/// on the same 0-100 (duration boons) / raw-average-stack-count (intensity
/// boons, no `*100`) scale as `BoonUptime` -- see module docs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GenerationStats {
    /// How much of the player's OWN boon-time they generated for
    /// themselves (`GetBuffsForSelf`).
    pub self_pct: f64,
    /// Averaged, over every OTHER player in the source's own subgroup, of
    /// how much boon-time the source generated for them
    /// (`GetBuffsForPlayers`, `BuffEnum.Group`).
    pub group_pct: f64,
    /// Same average, but over the whole squad excluding self
    /// (`GetBuffsForPlayers`, `BuffEnum.Squad`).
    pub squad_pct: f64,
}

// ---------------------------------------------------------------------
// Per-(target, buff) simulation: generated-ms per source.
// ---------------------------------------------------------------------

/// A duration-type (Queue) boon's held stack, carrying its source alongside
/// the remaining-ms value `simulator::run_duration`'s `DurationStack`
/// already tracks (see that function's doc comment for the frozen-queue
/// tick model this mirrors).
#[derive(Debug, Clone, Copy)]
struct DSlot {
    remaining: u64,
    source: u64,
}

/// Mirrors `simulator::advance_duration` exactly, but credits elapsed
/// active-slot ms to `gen_ms[stack[0].source]` as it consumes them --
/// GW2EI only ever attributes DURATION-type generation to whichever
/// source's stack currently occupies the active slot (`Stacks[0]`), never
/// to a frozen queued stack.
fn advance_duration_ms(
    stack: &mut Vec<DSlot>,
    clock: &mut u64,
    to_t: u64,
    gen_ms: &mut BTreeMap<u64, u64>,
) {
    loop {
        if *clock >= to_t {
            break;
        }
        let Some(active) = stack.first().copied() else {
            *clock = to_t;
            break;
        };
        let budget = to_t - *clock;
        if active.remaining > budget {
            *gen_ms.entry(active.source).or_default() += budget;
            stack[0].remaining -= budget;
            *clock = to_t;
            break;
        }
        *gen_ms.entry(active.source).or_default() += active.remaining;
        *clock += active.remaining;
        stack.remove(0);
    }
}

/// Duration-type (Queue) boon variant: returns generated-ms per source for
/// one (target, buff) event stream, over `[.., log_end_ms)` (there is no
/// explicit start clamp -- see `simulator::run_duration`'s doc comment;
/// `events` is never earlier than the caller's window start by
/// construction, since it's a subset of the same raw event stream the
/// window itself is derived from).
fn run_duration_ms(mut events: Vec<BuffEvent>, capacity: u32, log_end_ms: u64) -> BTreeMap<u64, u64> {
    events.sort_by_key(|e| e.time);
    let mut stack: Vec<DSlot> = Vec::new();
    let mut clock = events.first().map(|e| e.time).unwrap_or(0);
    let mut gen_ms: BTreeMap<u64, u64> = BTreeMap::new();

    for e in &events {
        advance_duration_ms(&mut stack, &mut clock, e.time, &mut gen_ms);
        match e.kind {
            BuffEventKind::Apply { duration_ms, is_shields } => {
                let duration_ms = duration_ms as u64;
                let was_full = stack.len() as u32 >= capacity;
                let inserted_idx = if !was_full {
                    stack.push(DSlot { remaining: duration_ms, source: e.agent });
                    stack.len() - 1
                } else if stack.len() > 1 {
                    let (idx, _) =
                        stack.iter().enumerate().skip(1).min_by_key(|&(_, s)| s.remaining).unwrap();
                    stack[idx] = DSlot { remaining: duration_ms, source: e.agent };
                    idx
                } else {
                    stack[0] = DSlot { remaining: duration_ms, source: e.agent };
                    0
                };
                if is_shields {
                    let val = stack.remove(inserted_idx);
                    stack.insert(0, val);
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                if let Some(idx) = find_single_removal_match(
                    stack.iter().map(|s| s.remaining as i64),
                    removed_duration_ms as i64,
                ) {
                    stack.remove(idx);
                }
            }
            BuffEventKind::RemoveAll => {
                stack.clear();
            }
            BuffEventKind::Extend { extended_ms, new_duration_ms } => {
                let extended = extended_ms as i64;
                let old_value = new_duration_ms as i64 - extended;
                let is_full = stack.len() as u32 >= capacity;
                if (!stack.is_empty() && old_value > 0) || is_full {
                    if let Some(active) = stack.first_mut() {
                        active.remaining = (active.remaining as i64 + extended).max(0) as u64;
                    }
                } else {
                    let duration_ms = (old_value + extended).max(0) as u64;
                    stack.push(DSlot { remaining: duration_ms, source: e.agent });
                    let val = stack.pop().unwrap();
                    stack.insert(0, val);
                }
            }
        }
    }
    advance_duration_ms(&mut stack, &mut clock, log_end_ms, &mut gen_ms);
    gen_ms
}

/// An intensity-type (Might, Stability) boon's held stack, carrying its
/// source alongside the same `(start, duration)` fields
/// `simulator::run_intensity`'s `Stack` already tracks.
#[derive(Debug, Clone, Copy)]
struct IStack {
    start: u64,
    duration: u64,
    source: u64,
}

impl IStack {
    fn expiry(&self) -> u64 {
        self.start + self.duration
    }
    fn remaining_at(&self, t: u64) -> i64 {
        self.expiry() as i64 - t as i64
    }
}

/// Credits `s`'s held lifetime, from its own `start` up to `end_t`
/// (`end_t` is always >= `s.start` at every call site: expiry/removal
/// times are always >= the stack's own apply time, and the final
/// still-held sweep clamps at `log_end_ms`, which is >= every event's
/// time by construction), to `gen_ms[s.source]`.
fn credit(gen_ms: &mut BTreeMap<u64, u64>, s: &IStack, end_t: u64) {
    let ms = end_t.saturating_sub(s.start);
    *gen_ms.entry(s.source).or_default() += ms;
}

/// Mirrors `simulator::run_intensity`'s `flush_expiries`, additionally
/// crediting each naturally-expired stack's full lifetime (`start` to its
/// own `expiry()`, NOT `upto`) to its source.
fn flush_expiries_ms(stacks: &mut Vec<IStack>, gen_ms: &mut BTreeMap<u64, u64>, upto: u64) {
    loop {
        let next = stacks
            .iter()
            .enumerate()
            .filter(|(_, s)| s.expiry() <= upto)
            .min_by_key(|(_, s)| s.expiry())
            .map(|(i, s)| (i, s.expiry()));
        match next {
            Some((i, exp)) => {
                let s = stacks.remove(i);
                credit(gen_ms, &s, exp);
            }
            None => break,
        }
    }
}

/// Intensity-type boon variant: unlike the duration path, EVERY
/// concurrently-held stack contributes its own lifetime to its own
/// source (GW2EI's `BuffSimulationItemIntensity.SetBuffDistributionItem`
/// credits every `RegroupedStack`, not just an "active" one -- see module
/// docs).
fn run_intensity_ms(mut events: Vec<BuffEvent>, capacity: u32, log_end_ms: u64) -> BTreeMap<u64, u64> {
    events.sort_by_key(|e| e.time);
    let mut stacks: Vec<IStack> = Vec::new();
    let mut gen_ms: BTreeMap<u64, u64> = BTreeMap::new();

    for e in &events {
        flush_expiries_ms(&mut stacks, &mut gen_ms, e.time);
        match e.kind {
            BuffEventKind::Apply { duration_ms, .. } => {
                let new_stack = IStack { start: e.time, duration: duration_ms as u64, source: e.agent };
                if (stacks.len() as u32) < capacity {
                    stacks.push(new_stack);
                } else if let Some((i, _)) =
                    stacks.iter().enumerate().min_by_key(|(_, s)| s.remaining_at(e.time))
                {
                    let evicted = stacks[i];
                    credit(&mut gen_ms, &evicted, e.time);
                    stacks[i] = new_stack;
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                let mut order: Vec<usize> = (0..stacks.len()).collect();
                order.sort_by_key(|&i| stacks[i].remaining_at(e.time));
                if let Some(pos) = find_single_removal_match(
                    order.iter().map(|&i| stacks[i].remaining_at(e.time)),
                    removed_duration_ms as i64,
                ) {
                    let s = stacks.remove(order[pos]);
                    credit(&mut gen_ms, &s, e.time);
                }
            }
            BuffEventKind::RemoveAll => {
                for s in stacks.drain(..) {
                    credit(&mut gen_ms, &s, e.time);
                }
            }
            BuffEventKind::Extend { extended_ms, new_duration_ms } => {
                let extended = extended_ms as i64;
                let old_value = new_duration_ms as i64 - extended;
                let is_full = (stacks.len() as u32) >= capacity;
                if (!stacks.is_empty() && old_value > 0) || is_full {
                    if let Some((i, _)) = stacks
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, s)| (s.remaining_at(e.time) - old_value).abs())
                    {
                        stacks[i].duration = (stacks[i].duration as i64 + extended).max(0) as u64;
                    }
                } else {
                    let duration_ms = (old_value + extended).max(0) as u64;
                    stacks.push(IStack { start: e.time, duration: duration_ms, source: e.agent });
                }
            }
        }
    }
    flush_expiries_ms(&mut stacks, &mut gen_ms, log_end_ms);
    // Anything still held past `log_end_ms` is credited only up to
    // `log_end_ms` -- mirrors GW2EI's `GetClampedDuration(start, end)`
    // (phase-clamped generation), same convention `uptime::compute` already
    // uses for its own window.
    for s in &stacks {
        credit(&mut gen_ms, s, log_end_ms);
    }
    gen_ms
}

/// Simulates every tracked boon's generated-ms breakdown for every squad
/// player as TARGET, keyed by `(target representative addr, buff id) ->
/// (source representative addr -> generated ms)`. Both target and source
/// addrs are folded onto their account representative exactly like
/// `simulate_boons` (relog/build-swap addrs collapse onto one entry); a
/// source that never resolves to a known squad addr (an enemy, NPC, or
/// unrecognized applier) keeps its raw addr as the map key -- it simply
/// never matches any squad player's `agent_addr` when `rollup` below
/// enumerates sources, so it contributes to the target's overall boon-time
/// (already reflected in `simulate_boons`/`boon_uptime`) without being
/// double-counted or misattributed here.
pub(crate) fn simulate_boon_generation_ms(
    raw: &RawLog,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), BTreeMap<u64, u64>> {
    simulate_boon_generation_ms_with_registry(
        raw,
        &crate::analysis::damage::InstidRegistry::build(raw),
        enc,
    )
}

/// [`simulate_boon_generation_ms`] against a caller-supplied, already-built
/// [`crate::analysis::damage::InstidRegistry`] (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for test callers.
pub(crate) fn simulate_boon_generation_ms_with_registry(
    raw: &RawLog,
    registry: &crate::analysis::damage::InstidRegistry,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), BTreeMap<u64, u64>> {
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let boon_ids: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let intensity_ids: BTreeSet<u32> =
        BOON_IDS.iter().filter(|&&(_, _, is_intensity)| is_intensity).map(|&(id, _, _)| id).collect();

    let raw_events = events::extract_buff_events_with_registry(raw, registry, &boon_ids);
    let arcdps_capacities = events::extract_buff_capacities(raw, &boon_ids);

    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for mut e in raw_events {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        // Fold a relogged source onto its account representative too, so a
        // player who relogged mid-fight still gets ONE source key instead
        // of splitting their generation credit across pre/post-relog addrs
        // (same relog-fold reasoning as `owner` above). A source that
        // isn't a known squad addr at all (enemy/NPC/unrecognized) keeps
        // its raw addr -- see this function's doc comment.
        e.agent = addr_to_rep.get(&e.agent).copied().unwrap_or(e.agent);
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    grouped
        .into_iter()
        .map(|((rep, buff_id), evs)| {
            let capacity = arcdps_capacities
                .get(&buff_id)
                .copied()
                .unwrap_or_else(|| simulator::capacity_for(buff_id));
            let is_intensity = intensity_ids.contains(&buff_id);
            let gen_ms = if is_intensity {
                run_intensity_ms(evs, capacity, log_end_ms)
            } else {
                run_duration_ms(evs, capacity, log_end_ms)
            };
            ((rep, buff_id), gen_ms)
        })
        .collect()
}

// ---------------------------------------------------------------------
// Self/group/squad rollup.
// ---------------------------------------------------------------------

fn ms_to_pct(ms: u64, phase_ms: f64, is_intensity: bool) -> f64 {
    let scale = if is_intensity { 1.0 } else { 100.0 };
    scale * ms as f64 / phase_ms
}

/// `target_gen[(target, buff)][source]` -> that source's generated-ms for
/// `target`'s copy of `buff`, converted to EI's percentage/avg-stacks scale
/// and averaged over `targets` (GW2EI's `totalGeneration / playerCount`,
/// per-target-summed-then-divided -- see module docs; algebraically
/// equivalent to averaging each target's own already-scaled pct, which is
/// what this does).
fn avg_pct(
    targets: &[&Player],
    boon_id: u32,
    source_addr: u64,
    target_gen: &BTreeMap<(u64, u32), BTreeMap<u64, u64>>,
    phase_ms: f64,
    is_intensity: bool,
) -> f64 {
    if targets.is_empty() {
        return 0.0;
    }
    let sum: f64 = targets
        .iter()
        .map(|t| {
            let ms = target_gen
                .get(&(t.agent_addr, boon_id))
                .and_then(|m| m.get(&source_addr))
                .copied()
                .unwrap_or(0);
            ms_to_pct(ms, phase_ms, is_intensity)
        })
        .sum();
    sum / targets.len() as f64
}

/// Reduces `simulate_boon_generation_ms`'s per-target breakdown into
/// self/group/squad rollups, keyed by `(source representative addr, buff
/// id)`. Only REAL recorded squad members (`Player::subgroup != 0`) are
/// considered as sources OR as group/squad targets -- mirrors GW2EI's own
/// `log.PlayerList`, which the Task 3 finding already established excludes
/// "Non Squad Player" friendlies (`subgroup == 0` is that same finding's
/// reusable signal) from exactly this kind of player-to-player rollup.
/// `[log_start_ms, log_end_ms)` must be the SAME absolute window
/// `simulate_boons`/`uptime::compute` already use (see their own doc
/// comments for why: the log's absolute event span, not
/// `Encounter::duration_ms`).
pub fn rollup(
    target_gen: &BTreeMap<(u64, u32), BTreeMap<u64, u64>>,
    enc: &Encounter,
    log_start_ms: u64,
    log_end_ms: u64,
) -> BTreeMap<(u64, u32), GenerationStats> {
    let phase_ms = (log_end_ms.saturating_sub(log_start_ms) as f64).max(1.0);
    let squad_players: Vec<&Player> = enc.players.iter().filter(|p| p.subgroup != 0).collect();

    let mut out: BTreeMap<(u64, u32), GenerationStats> = BTreeMap::new();
    for &(boon_id, _, is_intensity) in BOON_IDS.iter() {
        for &source in &squad_players {
            let source_addr = source.agent_addr;

            let self_ms = target_gen
                .get(&(source_addr, boon_id))
                .and_then(|m| m.get(&source_addr))
                .copied()
                .unwrap_or(0);
            let self_pct = ms_to_pct(self_ms, phase_ms, is_intensity);

            let group_targets: Vec<&Player> = squad_players
                .iter()
                .filter(|p| p.subgroup == source.subgroup && p.agent_addr != source_addr)
                .copied()
                .collect();
            let group_pct =
                avg_pct(&group_targets, boon_id, source_addr, target_gen, phase_ms, is_intensity);

            let squad_targets: Vec<&Player> =
                squad_players.iter().filter(|p| p.agent_addr != source_addr).copied().collect();
            let squad_pct =
                avg_pct(&squad_targets, boon_id, source_addr, target_gen, phase_ms, is_intensity);

            out.insert((source_addr, boon_id), GenerationStats { self_pct, group_pct, squad_pct });
        }
    }
    out
}

/// Convenience wrapper: simulates + rolls up in one call, over the same
/// absolute `[raw.events.first().time, raw.events.last().time)` window
/// `simulate_boon_uptimes` uses. `analysis::analyze` computes this from the
/// already-simulated per-target generation map instead (see `Metrics`'s doc
/// comment) to avoid re-running the simulator a second time; this exists
/// for standalone callers.
pub fn simulate_boon_generation(raw: &RawLog, enc: &Encounter) -> BTreeMap<(u64, u32), GenerationStats> {
    let target_gen = simulate_boon_generation_ms(raw, enc);
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    rollup(&target_gen, enc, log_start_ms, log_end_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn player(addr: u64, subgroup: u8) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup,
            in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![addr],
        }
    }

    fn enc(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "20260114".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], tick_rate: None,
        }
    }

    fn apply_ev(time: u64, buff_id: u32, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: duration_ms, buff_dmg: 0, overstack: 0,
            skillid: buff_id, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 1, result: 0, is_activation: 0, is_buffremove: 0,
            is_ninety: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// Duration boon (Fury): a single 10s apply spanning the whole window,
    /// from src=2 to dst=1, must credit ALL of it to source 2 -- 100% of a
    /// 10_000ms window.
    #[test]
    fn duration_generation_full_window_credited_to_applier() {
        let e = enc(vec![player(1, 1), player(2, 1)]);
        let raw = raw_from(vec![
            apply_ev(0, super::super::FURY, 2, 1, 10_000),
            // Trailing non-boon event so `log_end_ms` (derived from
            // `raw.events.last().time`) extends past the applied stack's
            // natural expiry -- see the identical pattern/comment in
            // `buffs::mod`'s own `simulate_boons_folds_relog_addrs_onto_representative`.
            apply_ev(12_000, 999_999, 0, 0, 0),
        ]);
        let target_gen = simulate_boon_generation_ms(&raw, &e);
        let ms = target_gen[&(1, super::super::FURY)][&2];
        assert_eq!(ms, 10_000);
    }

    /// Intensity boon (Might): two DIFFERENT sources each holding a
    /// concurrent stack on the same target must BOTH get full credit for
    /// their own stack's lifetime -- unlike duration boons, holding
    /// multiple stacks isn't "only the active one counts".
    #[test]
    fn intensity_generation_credits_every_concurrent_source() {
        let e = enc(vec![player(1, 1), player(2, 1), player(3, 1)]);
        let raw = raw_from(vec![
            apply_ev(0, super::super::MIGHT, 2, 1, 5000),
            apply_ev(0, super::super::MIGHT, 3, 1, 5000),
            apply_ev(6000, 999_999, 0, 0, 0), // trailing event extending log_end_ms
        ]);
        let target_gen = simulate_boon_generation_ms(&raw, &e);
        assert_eq!(target_gen[&(1, super::super::MIGHT)][&2], 5000);
        assert_eq!(target_gen[&(1, super::super::MIGHT)][&3], 5000);
    }

    /// Self/group/squad rollup: player 1 (subgroup 1) generates Fury for
    /// player 2 (own subgroup) and player 3 (other subgroup), plus for
    /// themselves. `self_pct` only reflects player 1's own held Fury from
    /// player 1; `group_pct` averages over subgroup-1 teammates excluding
    /// self (just player 2 here); `squad_pct` averages over the whole
    /// squad excluding self (players 2 and 3).
    #[test]
    fn rollup_self_group_squad_scopes() {
        let e = enc(vec![player(1, 1), player(2, 1), player(3, 2)]);
        let raw = raw_from(vec![
            apply_ev(0, super::super::FURY, 1, 1, 10_000), // self
            apply_ev(0, super::super::FURY, 1, 2, 5_000),  // to group-mate (subgroup 1)
            apply_ev(0, super::super::FURY, 1, 3, 2_500),  // to squad-mate (subgroup 2)
            apply_ev(10_000, 999_999, 0, 0, 0), // trailing event extending log_end_ms to 10_000
        ]);
        let target_gen = simulate_boon_generation_ms(&raw, &e);
        // Window is exactly [0, 10_000): the longest applied stack is 10s
        // and there's no trailing event, so this is constructed explicitly
        // (mirrors what `raw.events.first()/last().time` would derive).
        let stats = rollup(&target_gen, &e, 0, 10_000);
        let s = stats[&(1, super::super::FURY)];
        assert_eq!(s.self_pct, 100.0, "self: 10_000ms / 10_000ms window = 100%");
        assert_eq!(s.group_pct, 50.0, "group: only player 2 in-group, 5000/10000 = 50%");
        // squad average over BOTH other players (2 and 3): (50% + 25%) / 2 = 37.5%
        assert_eq!(s.squad_pct, 37.5);
    }

    /// A `subgroup == 0` player ("Non Squad Player" friendly, Task 3
    /// finding) must not appear as a source key in the rollup at all --
    /// out of scope per this module's doc comment.
    #[test]
    fn non_squad_friendly_excluded_as_source() {
        let mut friendly = player(9, 0);
        friendly.account = ":Friendly.0001".into();
        let e = enc(vec![player(1, 1), friendly]);
        let raw = raw_from(vec![apply_ev(0, super::super::FURY, 9, 1, 5000)]);
        let target_gen = simulate_boon_generation_ms(&raw, &e);
        let stats = rollup(&target_gen, &e, 0, 5000);
        assert!(!stats.contains_key(&(9, super::super::FURY)), "non-squad friendly must not be a source key");
    }
}
