//! The arcdps-methodology down/CC/strip/movement-impair contribution family
//! (M11 Task 2) -- the project's founding differentiator, re-expressed
//! (never transcribed) from the dev-relayed methodology
//! (`docs/arcdps-dev-notes.md` #11,
//! `docs/superpowers/specs/2026-08-09-m11-contribution-family-design.md`).
//! Replaces the M1-era `downs::apply` 10-second-window approximation
//! (retired this task) with the real arcdps algorithm: EI's own
//! `downContribution` uses a DIFFERENT algorithm by design (see
//! `axilog_ei`'s doc note on its `downContribution` mapping) -- there is no
//! EI golden to calibrate this engine against, only synthetic unit tests
//! (one per documented nuance below) and real-log sanity checks
//! (`tests/contribution_fixture.rs`).
//!
//! ## The methodology (normative; see the spec/dev-notes for the full text)
//!
//! On a downing blow against a player (an ordinary combat event, `result ==
//! DOWNED`, per this project's already-established down-event shape --
//! `downs::apply`'s own `result::DOWNED` predicate):
//! - **Window**: `[max(anchor(target) - 2000ms, log_start, previous_reset
//!   (target)), down_time]` -- `anchor(target)` is the target's own "over-99
//!   anchor" (`health::HealthTracker::last_time_at_or_above`, `None` treated
//!   as `log_start` per that module's own documented convention). After
//!   processing a down, `previous_reset(target)` becomes `down_time +
//!   2100ms` (the downstate-invulnerability guard: a subsequent down of the
//!   SAME target cannot attribute into the prior burst or the invuln
//!   window). This per-target reset state is kept HERE, not in `health.rs`
//!   (which stays a pure step-series query, per that module's own doc note).
//! - **Both bounds are INCLUSIVE** (`lo <= e.time <= down_time`) -- a
//!   deliberate choice, not inherited from the retired `downs::apply`
//!   10s-window code (which used an exclusive lower bound,
//!   `e.time > lo`). "From X to Y" reads naturally as inclusive on both
//!   ends, and an exclusive lower bound would silently exclude the very
//!   first event of the log from ever being creditable whenever a down's
//!   window floor lands exactly on `log_start` (a real, testable edge case
//!   -- see `window_clamps_to_log_start_inclusive` below). The upper bound
//!   being inclusive is required regardless: the downing blow's own damage
//!   (`e.time == down_time`) must itself be creditable.
//! - **Credit resolution**: every credit resolves its contributor's raw
//!   agent addr via the SAME time-aware `damage::InstidRegistry` every
//!   other pet-crediting pass in this codebase already uses, but with an
//!   extra **id-consistency guard** this engine is the first to need: the
//!   event's own `*_instid` field must resolve (at the event's own time) to
//!   the SAME raw addr the event's own `*_agent` field already claims --
//!   if the registry has no registration yet, or resolves to a DIFFERENT
//!   addr (e.g. instid reuse racing a still-in-flight event), the credit is
//!   dropped rather than guessed (see `resolve_contributor` below). Only
//!   after that guard passes does the contributor fold to its ultimate
//!   master (pet -> owner, self when there is no master) via the existing
//!   `*_master_instid` -> `InstidRegistry::resolve_at` pattern
//!   (`damage::pet_credit_events`/`cc::pet_credit_cc_events`/
//!   `buffs::events::resolve_agent` all already do this same single-hop
//!   fold). A folded contributor is only credited when it is NOT on the
//!   down TARGET's own side -- using this project's already-established
//!   binary `squad`/`enemies` sets (not a richer 3-team WvW model; see
//!   `apply`'s doc comment for why that's the right granularity here).
//! - **Four stats** (`ContributionMetrics`): `damage` (sum, CC-excluded,
//!   the same predicate `damage::accumulate` already uses), `cc` (+1 per
//!   `cc::is_cc` application on the target -- reusing that module's own
//!   era-gated predicate, not re-deriving it), `strips` (+1 per hostile
//!   `BUFFREMOVE_ALL` of a boon-category buff off the target, with the
//!   stability >1-stack carve-out -- see `credit_window`'s strips branch
//!   for the full `RemovedStacks` field citation), `movement_impairing`
//!   (impairment amount credited to the IMPAIRER via a packed instid in a
//!   pre-era SINGLE-removal row's `overstack` field -- see that same
//!   function's movement-impairing branch for the full, honestly-caveated
//!   citation trail).
//!
//! Computed for BOTH directions in one pass over every down in the log,
//! processed in time order (a single global `reset` map, keyed by target
//! raw addr, correctly serializes same-target reset dependencies while
//! leaving different-target windows independent): `downs_contribution`
//! (outgoing -- a squad player's own credit toward downing an enemy PLAYER)
//! and `downed_by` (incoming -- the mirror: what non-squad contributors did
//! to a squad player before THEIR down, aggregated onto the victim's own
//! row, not broken down by attacker).

use crate::analysis::buffs::{BOON_IDS, STABILITY};
use crate::analysis::cc::is_cc;
use crate::analysis::damage::InstidRegistry;
use crate::analysis::health::HealthTracker;
use crate::analysis::PlayerMetrics;
use crate::evtc::{buff_remove, result, sc, RawEvent, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// The over-99 anchor's health-percent threshold (methodology-normative).
const ANCHOR_THRESHOLD_PCT: f32 = 99.0;
/// The window's lead-in before the anchor (methodology-normative).
const LEAD_IN_MS: u64 = 2000;
/// The downstate-invuln reset gap applied after each processed down
/// (methodology-normative).
const RESET_GAP_MS: u64 = 2100;

/// One player's four-stat contribution total (either direction -- see
/// `PlayerMetrics::downs_contribution`/`downed_by`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContributionMetrics {
    /// Sum of in-window damage credited (CC-excluded; the same predicate
    /// `damage::accumulate` uses).
    pub damage: u64,
    /// Count of in-window `cc::is_cc` applications credited.
    pub cc: u32,
    /// Count of in-window hostile boon-category `BUFFREMOVE_ALL` events
    /// credited (the stability >1-stack carve-out applies here).
    pub strips: u32,
    /// Sum of in-window movement-impairment amounts credited to the
    /// resolved impairer (pre-era-only; see `credit_window`'s
    /// movement-impairing branch).
    pub movement_impairing: u64,
}

impl ContributionMetrics {
    fn merge(&mut self, other: ContributionMetrics) {
        self.damage += other.damage;
        self.cc += other.cc;
        self.strips += other.strips;
        self.movement_impairing += other.movement_impairing;
    }
}

/// Which of `PlayerMetrics`'s two contribution fields a processed down
/// feeds.
enum Direction {
    /// Target is an enemy player: contributors are squad players, credited
    /// onto their own `downs_contribution`.
    Outgoing,
    /// Target is a squad player: ALL non-squad contributors are aggregated
    /// onto the victim's own `downed_by` (not broken down by attacker).
    Incoming,
}

struct DownEvent {
    time: u64,
    target: u64,
    dir: Direction,
}

/// Computes both `PlayerMetrics::downs_contribution` (outgoing) and
/// `PlayerMetrics::downed_by` (incoming) for every squad player in
/// `players`, mutating in place -- mirrors `downs::apply`/`cc::apply_cc`'s
/// shape (an `agent_addr -> index` map + `addr_to_rep` relog-folding
/// closure), the established convention every other per-player pass in
/// this module already follows.
///
/// **Why the binary `squad`/`enemies` sets, not a 3-team WvW model**: this
/// project's `analyze()` already reduces every other per-player pass
/// (`damage`, `downs`, `cc`, `support`) to this same binary split --
/// "our recorded squad" vs "everyone else" -- with no richer red/green/blue
/// distinction threaded through any of them. A down between two OTHER
/// WvW teams (neither our squad) can still appear in this log's events, but
/// since only squad players have a `PlayerMetrics` row to credit at all,
/// treating "everyone but the target's own recorded side" as eligible and
/// then only crediting rows that actually resolve into `players` produces
/// the identical observable result as a richer 3-team model would, with far
/// less surface area -- see `credit_window`'s doc comment for the one place
/// this matters (the `exclude_side` parameter).
pub fn apply(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    enc: &Encounter,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) {
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    // "each down of an enemy PLAYER" (methodology scope) -- excludes enemy
    // NPCs/gadgets, which `enemies` itself doesn't distinguish.
    let enemy_player_addrs: BTreeSet<u64> = enc
        .enemies
        .iter()
        .filter(|e| e.is_player)
        .flat_map(|e| e.agent_addrs.iter().copied())
        .collect();

    let mut downs: Vec<DownEvent> = raw
        .events
        .iter()
        .filter(|e| e.is_statechange == 0 && e.result == result::DOWNED)
        .filter_map(|e| {
            if enemy_player_addrs.contains(&e.dst_agent) {
                Some(DownEvent { time: e.time, target: e.dst_agent, dir: Direction::Outgoing })
            } else if squad.contains(&e.dst_agent) {
                Some(DownEvent { time: e.time, target: e.dst_agent, dir: Direction::Incoming })
            } else {
                None
            }
        })
        .collect();
    // Global time order (not per-target) -- a single `reset` map, keyed by
    // target raw addr, correctly serializes same-target dependencies while
    // leaving different-target windows independent regardless of
    // interleaving. Real captures are already chronological (matching
    // every other pass in this codebase); `sort_by_key` is a stable,
    // defensive no-op on already-sorted input.
    downs.sort_by_key(|d| d.time);

    let health = HealthTracker::build(raw);
    let registry = InstidRegistry::build(raw);
    let post_era = raw.header.is_post_buff_rework();
    let boon_ids: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let log_start = raw.events.first().map(|e| e.time).unwrap_or(0);

    // Per-target downstate-invuln reset floor (methodology-normative) --
    // kept HERE, not in `health.rs` (which stays a pure query module).
    let mut reset: BTreeMap<u64, u64> = BTreeMap::new();

    for down in &downs {
        let target = down.target;
        let t_down = down.time;
        let anchor =
            health.last_time_at_or_above(target, ANCHOR_THRESHOLD_PCT, t_down).unwrap_or(log_start);
        let prev_reset = reset.get(&target).copied().unwrap_or(0);
        let lo = anchor.saturating_sub(LEAD_IN_MS).max(log_start).max(prev_reset);
        // Reset applies to whatever THIS down processes next, regardless of
        // direction -- a target can only ever be one side (squad XOR
        // enemy), so there's no cross-direction collision on this map.
        reset.insert(target, t_down + RESET_GAP_MS);

        let exclude_side: &BTreeSet<u64> = match down.dir {
            Direction::Outgoing => enemies,
            Direction::Incoming => squad,
        };
        let credits =
            credit_window(&raw.events, &registry, lo, t_down, target, exclude_side, &boon_ids, post_era);

        match down.dir {
            Direction::Outgoing => {
                for (contributor, c) in credits {
                    if let Some(&i) = idx.get(&rep(contributor)) {
                        players[i].downs_contribution.merge(c);
                    }
                }
            }
            Direction::Incoming => {
                // Aggregated onto the victim's own row -- not broken down
                // by attacker (methodology: "Aggregate per player ...
                // downed_by { ... }").
                if let Some(&i) = idx.get(&rep(target)) {
                    for (_contributor, c) in credits {
                        players[i].downed_by.merge(c);
                    }
                }
            }
        }
    }
}

/// The id-consistency guard + ultimate-master fold every credit in this
/// engine resolves through: `addr`/`instid`/`master_instid` are an event's
/// own agent/instid/master-instid TRIPLE for whichever role is being
/// credited (e.g. `src_agent`/`src_instid`/`src_master_instid` for an
/// ordinary strike's damage source, or the `dst_*` triple for a
/// `BUFFREMOVE_ALL` row's remover -- roles are inverted there, see
/// `credit_window`'s strips branch).
///
/// **Guard**: when `instid != 0`, the registry's OWN record for that instid
/// at this event's time must agree with the event's own `addr` field --
/// disagreement (a different addr) OR no registration at all drops the
/// credit entirely (`None`) rather than guessing. `instid == 0` skips the
/// guard (treated as trivially consistent), mirroring this codebase's
/// established "0 = no link" convention for `*_master_instid` (never a
/// failure trigger on its own) -- real captures always assign live agents a
/// nonzero instid, so this only matters for the synthetic
/// instid-recycling-collision case the guard exists to catch (see
/// `instid_mismatch_is_dropped` below).
///
/// **Fold**: once the guard passes (or was skipped), `master_instid != 0`
/// resolves through the SAME registry (falling back to `addr` itself if
/// that particular resolution fails) -- the identical single-hop
/// pet-owner-fold pattern `damage::pet_credit_events`/`cc::
/// pet_credit_cc_events`/`buffs::events::resolve_agent` already use
/// (`self` when `master_instid == 0`, i.e. "no master" -- credit lands on
/// the contributor itself).
///
/// **addr-0 guard (review fix round 1)**: `addr == 0` is never a real,
/// creditable contributor -- arcdps never assigns a live agent address `0`
/// (it's the wire's own "no agent" sentinel, e.g. a ground-targeted skill's
/// unset `dst_agent`). Without this guard, a synthetic/degenerate
/// all-zero-fields row (`addr == instid == master_instid == 0`) would
/// resolve to `Some(0)` (the trivial "no instid, no master -> self" path
/// falls straight through) -- harmless for the OUTGOING direction (an addr
/// of `0` can never be a squad player's `agent_addr`, so the later `idx`
/// lookup silently drops it), but a real footgun for INCOMING: that
/// direction aggregates every surviving credit in the window onto the
/// victim's `downed_by` unconditionally, with no equivalent per-contributor
/// identity lookup to filter it back out (see `apply`'s `Direction::
/// Incoming` arm). Dropping `addr == 0` here, once, before either the guard
/// or the fold runs, closes that gap for every credit type and both
/// directions uniformly.
fn resolve_contributor(
    registry: &InstidRegistry,
    addr: u64,
    instid: u16,
    master_instid: u16,
    time: u64,
) -> Option<u64> {
    if addr == 0 {
        return None;
    }
    if instid != 0 {
        match registry.resolve_at(instid, time) {
            Some(resolved) if resolved == addr => {}
            _ => return None,
        }
    }
    if master_instid != 0 {
        Some(registry.resolve_at(master_instid, time).unwrap_or(addr))
    } else {
        Some(addr)
    }
}

/// One down's backward scan, re-expressed as a single windowed filter pass
/// over every event (semantically identical to walking the window
/// backward from the down -- these are pure sums, so scan direction can't
/// change the result; a forward filter avoids re-deriving arcdps's own
/// live ring-buffer walk, staying a re-expression of the METHODOLOGY, not
/// the mechanism). Both window bounds are INCLUSIVE -- see `apply`'s doc
/// comment for why.
fn credit_window(
    events: &[RawEvent],
    registry: &InstidRegistry,
    lo: u64,
    hi: u64,
    target: u64,
    exclude_side: &BTreeSet<u64>,
    boon_ids: &BTreeSet<u32>,
    post_era: bool,
) -> BTreeMap<u64, ContributionMetrics> {
    let mut out: BTreeMap<u64, ContributionMetrics> = BTreeMap::new();
    for e in events {
        if e.time < lo || e.time > hi {
            continue;
        }

        // damage + cc: an ordinary (non-statechange, non-activation,
        // non-buffremove) combat event aimed at the target. Damage predicate
        // mirrors `damage::accumulate` exactly (CC-excluded, buff_dmg vs
        // value selection); `cc` reuses `cc::is_cc`'s own era-gated
        // predicate rather than re-deriving it.
        if e.is_statechange == 0 && e.is_activation == 0 && e.is_buffremove == 0 && e.dst_agent == target {
            let cc_row = is_cc(e, post_era);
            if cc_row || e.result != result::CROWD_CONTROL {
                if let Some(c) =
                    resolve_contributor(registry, e.src_agent, e.src_instid, e.src_master_instid, e.time)
                {
                    if !exclude_side.contains(&c) {
                        let entry = out.entry(c).or_default();
                        if cc_row {
                            entry.cc += 1;
                        } else {
                            let dmg =
                                if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
                            entry.damage += dmg;
                        }
                    }
                }
            }
        }

        // strips: hostile BUFFREMOVE_ALL of a boon-category buff off the
        // target. TWO wire shapes credit here, unlike movement_impairing
        // below: pre-era, an ordinary combat event (`is_statechange == 0`)
        // with `is_buffremove == ALL`; post-era, its OWN dedicated
        // statechange (`sc::BUFF_REMOVE_ALL = 72`) -- verified against
        // `evtc::event::BUFF_REMOVE_ALL`'s doc comment: `CombatEventFactory.
        // AddBuffRemoveEvent`'s post-era branch routes a `BuffRemoveAll`-
        // statechange row to `BuffRemoveAllEvent` UNCONDITIONALLY (`is_
        // buffremove`'s value is NOT consulted for that statechange, unlike
        // `BUFF_REMOVE_SINGLE`), so the post-era arm below does not also
        // require `is_buffremove == ALL`.
        //
        // Role inversion (owner = src_agent, remover = dst_agent) verified
        // against `AbstractBuffRemoveEvent`'s ctor (both eras share it) --
        // `AbstractBuffRemoveEvent(CombatItem evtcItem, ...)`: `By =
        // agentData.GetAgent(evtcItem.DstAgent, ...)`, `To = agentData.
        // GetAgent(evtcItem.SrcAgent, ...)` (decompiled `GW2EIEvtcParser.
        // ParsedData.AbstractBuffRemoveEvent`, 2026-08-09) -- `To` (the buff
        // owner, i.e. our `target`) is `SrcAgent`; `By` (the remover, i.e.
        // our contributor) is `DstAgent`.
        //
        // Stacks-removed field: `BuffRemoveAllEvent(CombatItem evtcItem,
        // ...)`: `RemovedStacks = evtcItem.Result` (decompiled
        // `GW2EIEvtcParser.ParsedData.BuffRemoveAllEvent`, 2026-08-09) --
        // this project's `RawEvent::result` (`u8`) decodes from the SAME
        // fixed wire offset regardless of what a given row's `result` byte
        // means semantically (a strike's `cbtresult` enum vs a removal's
        // stack count), so `e.result as u32` is directly the stacks-removed
        // count here. Confirmed field-identical in BOTH eras:
        // `CombatEventFactory.AddBuffRemoveEvent`'s pre-era `BuffRemove.All`
        // branch and post-era `IsStateChange == BuffRemoveAll` branch both
        // construct `new BuffRemoveAllEvent(buffEvent, agentData,
        // skillData)` -- the identical `(CombatItem, ...)` ctor, so no era
        // dispatch is needed for this field mapping (`CombatEventFactory.cs`
        // lines 671-679/700-702, 2026-08-09).
        let is_strip_row = (e.is_statechange == 0 && e.is_buffremove == buff_remove::ALL)
            || e.is_statechange == sc::BUFF_REMOVE_ALL;
        if is_strip_row && e.src_agent == target && boon_ids.contains(&e.skillid) {
            let stacks_removed = e.result as u32;
            // Stability counts only when MORE THAN ONE stack was removed
            // (single-stack loss is self-consumption, not a strip) --
            // methodology-normative; every other boon-category buff always
            // counts.
            if e.skillid != STABILITY || stacks_removed > 1 {
                if let Some(c) =
                    resolve_contributor(registry, e.dst_agent, e.dst_instid, e.dst_master_instid, e.time)
                {
                    if !exclude_side.contains(&c) {
                        out.entry(c).or_default().strips += 1;
                    }
                }
            }
        }

        // movement_impairing: the dev-relayed "is_shields single-remove
        // form". `is_statechange == 0 && is_buffremove == SINGLE` is
        // PRE-ERA-ONLY BY CONSTRUCTION -- the post-rework wire shape moves
        // single-buff-stack removal to its own dedicated statechange
        // (`sc::BUFF_REMOVE_SINGLE = 71`, i.e. `is_statechange == 71`, never
        // `0`), so this predicate structurally never matches a post-era
        // log's rows for the same logical event; `movement_impairing`
        // gracefully stays 0 there rather than needing an explicit era
        // branch. This is also independently the ONLY era the arcdps
        // reference documents `is_shields`/`overstack_value` as populated
        // fields at all: fetching the live reference (`curl
        // https://www.deltaconnected.com/arcdps/evtc/README.txt`,
        // 2026-08-09) and reading its dedicated `CBTS_BUFFREMOVE_SINGLE`
        // block (the post-era statechange) directly shows NEITHER
        // `is_shields` NOR `overstack_value` in that block's own documented
        // field list -- only `src_agent`/`dst_agent`/`value`/`skillid`/
        // `iff`/`is_buffremove`/`is_ninety`/`is_fifty`/`is_moving`/
        // `is_flanking`/`pad61`. Pre-era, this row instead reuses the
        // generic `CBTS_COMBAT` (`is_statechange == 0`) struct, where
        // `is_shields`/`overstack_value` are always physically present
        // slots (their DEFAULT meaning there, per the same reference's
        // `CBTS_COMBAT` block, is "damage partially/wholly absorbed by
        // barrier" / "shield damage" -- NOT applicable to a buff-remove-
        // shaped row; this repurposing for movement-impairment on a
        // buff-remove row is GW2EI-independent (searched the full GW2EI
        // source tree, 2026-08-09: zero references to any
        // movement-impairment concept anywhere -- EI does not compute this
        // stat at all, so there is no GW2EI arbiter for this specific
        // field's repurposed meaning, only the arcdps dev's own relayed
        // description, re-expressed here per this task's LAW).
        //
        // Packed `overstack` layout: `[amount: u16 (low)][applier_instid:
        // u16 (high)]`, taken directly in the order the dev-relayed
        // description names the two values ("amount u16 + applier instid
        // u16") -- this specific bit ordering has no independent source to
        // cross-check against (see above: GW2EI doesn't model this stat at
        // all, and the current arcdps reference doesn't document
        // `overstack_value` for this row at all in either era's own
        // section), so it is taken as-given rather than guessed a different
        // way; documented here as an explicit, isolated assumption rather
        // than silently baked in.
        //
        // No id-consistency guard is possible for the resolved applier (there
        // is no companion raw-agent field on this row to cross-check the
        // packed instid against, unlike every other credit type above) --
        // and no further pet-owner fold either (the packed field carries only
        // ONE instid, not an instid + master-instid pair) -- both are
        // documented, isolated limitations of this one stat, not a broader
        // engine gap.
        if e.is_statechange == 0
            && e.is_buffremove == buff_remove::SINGLE
            && e.src_agent == target
            && e.is_shields != 0
        {
            let amount = (e.overstack & 0xFFFF) as u64;
            let applier_instid = (e.overstack >> 16) as u16;
            if let Some(applier) = registry.resolve_at(applier_instid, e.time) {
                if !exclude_side.contains(&applier) {
                    out.entry(applier).or_default().movement_impairing += amount;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{sc, RawHeader};

    fn base_event(time: u64, src: u64, dst: u64) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 1, buff: 0, result: 0, is_activation: 0, is_buffremove: 0, is_statechange: 0,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn dmg(time: u64, src: u64, dst: u64, value: i32) -> RawEvent {
        let mut e = base_event(time, src, dst);
        e.value = value;
        e
    }

    fn down(time: u64, src: u64, dst: u64) -> RawEvent {
        let mut e = base_event(time, src, dst);
        e.result = result::DOWNED;
        e
    }

    fn health(time: u64, agent: u64, pct_times_100: u64) -> RawEvent {
        let mut e = base_event(time, agent, 0);
        e.dst_agent = pct_times_100;
        e.src_agent = agent;
        e.is_statechange = sc::HEALTH_UPDATE;
        e
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn enc_with(squad_addrs: Vec<u64>, enemy_player_addrs: Vec<u64>) -> Encounter {
        use crate::model::{Enemy, Player};
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 100_000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![],
            players: squad_addrs.iter().map(|&a| Player {
                agent_addr: a, account: format!(":P{a}.1"), character: format!("P{a}"),
                profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
                subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
                agent_addrs: vec![a],
            }).collect(),
            enemies: enemy_player_addrs.iter().map(|&a| Enemy {
                id: a, instid: 0, name: format!("E{a}"), team: "blue".into(), is_player: true,
                marker: None, agent_addrs: vec![a],
            }).collect(),
            markers: vec![], tick_rate: None,
        }
    }

    fn run(events: Vec<RawEvent>, squad_addrs: Vec<u64>, enemy_addrs: Vec<u64>) -> Vec<PlayerMetrics> {
        let enc = enc_with(squad_addrs.clone(), enemy_addrs.clone());
        let raw = raw_from(events);
        let squad: BTreeSet<u64> = squad_addrs.into_iter().collect();
        let enemies: BTreeSet<u64> = enemy_addrs.into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = squad.iter().map(|&a| (a, a)).collect();
        let mut players: Vec<PlayerMetrics> =
            enc.players.iter().map(|p| PlayerMetrics { agent_addr: p.agent_addr, ..Default::default() }).collect();
        apply(&mut players, &raw, &enc, &squad, &enemies, &addr_to_rep);
        players
    }

    /// Anchor math: no health data for the target at all -> `None` treated
    /// as `log_start` (per `HealthTracker::last_time_at_or_above`'s own
    /// documented convention). An event exactly AT `log_start` must still
    /// be creditable (inclusive lower bound).
    #[test]
    fn anchor_none_treated_as_log_start_inclusive() {
        let events = vec![
            dmg(100, 1, 9, 500), // the very first event -- also log_start
            down(5000, 1, 9),
        ];
        let players = run(events, vec![1], vec![9]);
        assert_eq!(players[0].downs_contribution.damage, 500, "log_start-anchored window must include the log's own first event");
    }

    /// The `-2000ms` lead-in boundary is INCLUSIVE at the low end: an event
    /// exactly at `anchor - 2000` counts; one ms earlier does not.
    #[test]
    fn lead_in_boundary_is_inclusive_at_lo() {
        let events = vec![
            health(0, 9, 10000),    // target at 100% at t=0 -- sets log_start=0
            health(5000, 9, 10000), // target confirmed >=99% at t=5000 -- this is the anchor
            dmg(2999, 1, 9, 111),   // one ms before the window floor (3000) -- excluded
            dmg(3000, 1, 9, 222),   // exactly the window floor (anchor-2000) -- included
            down(10000, 1, 9),
        ];
        let players = run(events, vec![1], vec![9]);
        assert_eq!(players[0].downs_contribution.damage, 222, "only the event at the inclusive lo boundary should count");
    }

    /// Reset math: a second down of the SAME target must not reach back
    /// into the first down's own burst -- the 2100ms reset floor
    /// (`down_time + 2100`) wins over a stale/absent health anchor.
    #[test]
    fn reset_prevents_second_down_reaching_into_first_burst() {
        let events = vec![
            health(0, 9, 10000),   // sets log_start=0, target at 100%
            dmg(1000, 1, 9, 500),  // burst #1 -- must count for down #1 only
            down(5000, 1, 9),      // down #1 -- reset becomes 5000+2100=7100
            dmg(7200, 2, 9, 300),  // burst #2 -- must count for down #2 only
            down(8000, 2, 9),      // down #2
        ];
        let players = run(events, vec![1, 2], vec![9]);
        let p1 = players.iter().find(|p| p.agent_addr == 1).unwrap();
        let p2 = players.iter().find(|p| p.agent_addr == 2).unwrap();
        assert_eq!(p1.downs_contribution.damage, 500, "attacker 1's burst credits down #1 only");
        assert_eq!(p2.downs_contribution.damage, 300, "attacker 2's burst credits down #2 only, NOT reaching back into burst #1 (500 must not leak in)");
    }

    /// Stability: a single-stack removal is self-consumption, not a strip;
    /// a >1-stack removal on the SAME buff counts.
    #[test]
    fn stability_single_stack_not_credited_multi_stack_is() {
        // Role inversion (verified in `credit_window`'s strips branch doc
        // comment): owner (the target losing the buff) = `src_agent`,
        // remover (the contributor) = `dst_agent` -- so `src=9` (the enemy
        // target), `dst=2` (the squad attacker being credited).
        let mut one_stack = base_event(1000, 9, 2);
        one_stack.is_buffremove = buff_remove::ALL;
        one_stack.skillid = STABILITY;
        one_stack.result = 1; // RemovedStacks = 1

        let mut two_stack = base_event(2000, 9, 2);
        two_stack.is_buffremove = buff_remove::ALL;
        two_stack.skillid = STABILITY;
        two_stack.result = 2; // RemovedStacks = 2

        let events = vec![one_stack, two_stack, down(5000, 1, 9)];
        let players = run(events, vec![1, 2], vec![9]);
        let attacker = players.iter().find(|p| p.agent_addr == 2).unwrap();
        assert_eq!(attacker.downs_contribution.strips, 1, "only the >1-stack removal counts as a strip");
    }

    /// A `BUFFREMOVE_ALL` of a non-boon skill id (not in `BOON_IDS`) must be
    /// ignored entirely.
    #[test]
    fn non_boon_removal_is_ignored() {
        let mut e = base_event(1000, 9, 2); // owner=target(9), remover=attacker(2)
        e.is_buffremove = buff_remove::ALL;
        e.skillid = 999_999; // not a tracked boon id
        e.result = 5;
        let events = vec![e, down(5000, 1, 9)];
        let players = run(events, vec![1, 2], vec![9]);
        let attacker = players.iter().find(|p| p.agent_addr == 2).unwrap();
        assert_eq!(attacker.downs_contribution.strips, 0);
    }

    /// Post-era logs move `BUFFREMOVE_ALL` to its own dedicated statechange
    /// (`sc::BUFF_REMOVE_ALL = 72`), unlike movement-impairing's pre-era-only
    /// wire shape -- strips must be credited on EITHER era's row shape.
    /// `is_buffremove` is deliberately left unset on the post-era row
    /// (mirrors the real wire: `is_buffremove` is not consulted for this
    /// statechange, per `evtc::event::BUFF_REMOVE_ALL`'s doc comment).
    #[test]
    fn strips_credited_on_post_era_dedicated_statechange() {
        let mut e = base_event(1000, 9, 2); // owner=target(9), remover=attacker(2)
        e.is_statechange = sc::BUFF_REMOVE_ALL;
        e.skillid = STABILITY;
        e.result = 3; // RemovedStacks = 3 (>1, counts)
        let events = vec![e, down(5000, 1, 9)];
        let players = run(events, vec![1, 2], vec![9]);
        let attacker = players.iter().find(|p| p.agent_addr == 2).unwrap();
        assert_eq!(attacker.downs_contribution.strips, 1, "post-era BUFF_REMOVE_ALL statechange must credit a strip");
    }

    /// Pet folding: a pet's own damage must fold onto its owner via
    /// `*_master_instid` -> `InstidRegistry`, the same single-hop pattern
    /// every other pet-crediting pass in this codebase already uses.
    #[test]
    fn pet_damage_folds_onto_owner() {
        let mut seed = dmg(0, 1, 0, 0); // registers owner (addr 1) at instid 11
        seed.src_instid = 11;
        let mut pet_hit = dmg(1000, 300, 9, 400); // pet addr 300, instid 77
        pet_hit.src_instid = 77;
        pet_hit.src_master_instid = 11; // points back to owner's instid
        let events = vec![seed, pet_hit, down(5000, 1, 9)];
        let players = run(events, vec![1], vec![9]);
        assert_eq!(players[0].downs_contribution.damage, 400, "pet damage must fold onto the owning squad player");
    }

    /// Id-consistency guard: an instid that resolves to a DIFFERENT raw
    /// addr than the event's own claimed `src_agent` (e.g. instid reuse
    /// racing within the same tick) is dropped entirely, not guessed.
    ///
    /// `InstidRegistry::build` registers a candidate's OWN claimed
    /// `src_agent`/`src_instid` pairing too (that's how it can ever resolve
    /// a legitimate event at all), so a mismatch can only be manufactured
    /// with a SECOND, later-in-vec-order event claiming the same instid at
    /// the exact same timestamp -- `InstidRegistry::register`'s own tie-
    /// breaking (equal `time` never inserts-sorted, only ever pushes) makes
    /// the later-in-order registration win the `resolve_at` query for that
    /// tied instant, overriding the credited event's own self-registration.
    #[test]
    fn instid_mismatch_is_dropped() {
        let mut spoofed = dmg(1000, 2, 9, 500); // claims src_agent=2, instid=11
        spoofed.src_instid = 11;
        let mut override_reg = dmg(1000, 3, 0, 0); // same tick, addr 3 also claims instid 11
        override_reg.src_instid = 11;
        let events = vec![spoofed, override_reg, down(5000, 1, 9)];
        // Only addr 2 and addr 3 as candidate squad rows; neither should be
        // credited -- addr 3's own event never targets the down at all
        // (dst_agent=0), and addr 2's is dropped by the guard.
        let players = run(events, vec![2, 3], vec![9]);
        for p in &players {
            assert_eq!(p.downs_contribution.damage, 0, "instid-mismatched credit must be dropped entirely, addr={}", p.agent_addr);
        }
    }

    /// A contributor on the TARGET's own side (friendly fire / self-damage
    /// shape) must be excluded -- tested on the INCOMING direction, where
    /// `exclude_side` is `squad` itself (a squad member "damaging" another
    /// squad member must never land in `downed_by`).
    #[test]
    fn same_side_contributor_excluded() {
        let mut friendly_fire = dmg(1000, 2, 1, 999); // squad member 2 "hits" squad member 1
        friendly_fire.iff = 0;
        let events = vec![friendly_fire, down(5000, 2, 1)]; // squad member 1 downed
        let players = run(events, vec![1, 2], vec![]);
        let victim = players.iter().find(|p| p.agent_addr == 1).unwrap();
        assert_eq!(victim.downed_by.damage, 0, "a same-side contributor must never be credited into downed_by");
    }

    /// Review fix round 1: an all-zero-fields contributor (`addr == instid
    /// == master_instid == 0`, e.g. a degenerate/synthetic row) must be
    /// dropped by `resolve_contributor`'s addr-0 guard, not mis-credited.
    /// This specifically exercises the INCOMING direction (`downed_by`),
    /// which -- unlike outgoing -- has no per-contributor `idx` lookup to
    /// catch an addr-0 credit after the fact; it aggregates every surviving
    /// credit in the window unconditionally.
    #[test]
    fn addr_zero_contributor_is_dropped_not_credited() {
        let mut zero_strip = base_event(1000, 1, 0); // owner=target(1), remover addr=0
        zero_strip.is_buffremove = buff_remove::ALL;
        zero_strip.skillid = STABILITY;
        zero_strip.result = 2; // stacks removed > 1 -- would count if not for the addr-0 guard
        let events = vec![zero_strip, down(5000, 9, 1)]; // squad member 1 downed
        let players = run(events, vec![1], vec![]);
        let victim = players.iter().find(|p| p.agent_addr == 1).unwrap();
        assert_eq!(victim.downed_by.strips, 0, "an all-zero-fields contributor must be dropped, not inflate downed_by.strips");
    }

    /// Window clamp at log start: when `anchor - 2000` (or the `None`
    /// default) would land before the log's own first event, the window
    /// floor clamps up to `log_start`, not further back.
    #[test]
    fn window_clamps_to_log_start_inclusive() {
        let events = vec![
            dmg(500, 1, 9, 42), // log_start = 500 (the log's own first event)
            health(600, 9, 10000), // anchor at t=600, would want lo=600-2000 (underflow) -> clamps to 500
            down(10000, 1, 9),
        ];
        let players = run(events, vec![1], vec![9]);
        assert_eq!(players[0].downs_contribution.damage, 42, "window floor must clamp to log_start, not underflow/reach before it");
    }

    /// The mirror direction: a squad player's own down aggregates enemy
    /// contributors' credit onto `downed_by`, not broken down by attacker.
    #[test]
    fn incoming_direction_aggregates_onto_victim() {
        let events = vec![
            dmg(1000, 9, 1, 300),  // enemy 9 hits squad member 1
            dmg(1200, 10, 1, 200), // enemy 10 also hits squad member 1
            down(5000, 9, 1),      // squad member 1 downed
        ];
        let players = run(events, vec![1], vec![]); // no enemy PLAYER rows needed for incoming
        assert_eq!(players[0].downed_by.damage, 500, "both enemy contributors' damage aggregates onto the victim's own row");
    }

    /// Movement-impairing: pre-era `is_shields`-flagged SINGLE removal
    /// packs `[amount: u16][applier_instid: u16]` in `overstack`; the
    /// resolved applier is credited with the amount.
    #[test]
    fn movement_impairing_credits_the_packed_applier() {
        let mut seed = dmg(0, 2, 0, 0); // registers applier (addr 2) at instid 55
        seed.src_instid = 55;
        let mut removal = base_event(1000, 9, 9); // owner (target) = src_agent = 9
        removal.is_buffremove = buff_remove::SINGLE;
        removal.is_shields = 1;
        removal.overstack = 250u32 | (55u32 << 16); // amount=250, applier_instid=55
        let events = vec![seed, removal, down(5000, 1, 9)];
        let players = run(events, vec![1, 2], vec![9]);
        let applier = players.iter().find(|p| p.agent_addr == 2).unwrap();
        assert_eq!(applier.downs_contribution.movement_impairing, 250);
    }

    /// Post-era logs never produce the pre-era generic SINGLE-remove wire
    /// shape this stat needs (the post-era statechange doesn't carry
    /// `is_shields`/`overstack_value` at all per the live arcdps reference)
    /// -- `movement_impairing` gracefully stays 0 rather than crediting a
    /// spurious value from a structurally-different row shape.
    #[test]
    fn movement_impairing_gracefully_absent_post_era() {
        let mut removal = base_event(1000, 9, 9);
        removal.is_statechange = sc::BUFF_REMOVE_SINGLE; // post-era dedicated statechange
        removal.is_buffremove = buff_remove::SINGLE;
        removal.is_shields = 1; // even if a post-era row happened to carry this
        removal.overstack = 250u32 | (55u32 << 16);
        let events = vec![removal, down(5000, 1, 9)];
        let players = run(events, vec![1], vec![9]);
        assert_eq!(players[0].downs_contribution.movement_impairing, 0, "post-era rows never match the pre-era-only predicate");
    }
}
