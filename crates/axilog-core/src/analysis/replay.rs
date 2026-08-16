//! Combat replay position tracks (M9 Task 1): per-squad-player and
//! per-enemy-player-representative position samples, downsampled to a fixed
//! polling interval, plus down/dead intervals. Standalone from
//! [`crate::analysis::analyze`] -- `Metrics` does not grow by default; a
//! caller opts in by calling [`build_replay`] separately (CLI wiring for
//! `--replay` is M9 Task 2).
//!
//! ## Ordinal verification (project law: curl + hand-count the arcdps
//! README enum, never `WebFetch` it)
//!
//! `curl https://www.deltaconnected.com/arcdps/evtc/README.txt`
//! (2026-08-08), hand-counting `enum cbtstatechange` entries from
//! `CBTS_COMBAT = 0`: `CBTS_POSITION` is the 20th entry (index 19),
//! `CBTS_VELOCITY` the 21st (index 20), `CBTS_FACING` the 22nd (index 21).
//! Cross-checked against GW2EI's `ArcDPSEnums.StateChange.Position = 19` /
//! `.Velocity = 20` / `.Rotation = 21`
//! (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:279-281`). Independently
//! confirmed by an event-count fingerprint against this project's real
//! post-rework local fixture (`fixtures/local/wvw-postrework.zevtc`):
//! statechange 19/20/21 carry exactly 61336/57333/50182 events (position
//! most frequent, facing least -- consistent with how often each kind of
//! movement telemetry changes), matching the plan's documented hint. See
//! `crate::evtc::event::sc::POSITION`'s doc comment for the full payload
//! citation (GW2EI's `MovementEvent.PackMovementData`/`UnpackMovementData`
//! is the arbiter for the packed-float layout, since the arcdps reference
//! text alone doesn't spell out where `z` lives).
//!
//! ## Down/dead interval construction
//!
//! `CBTS_CHANGEDOWN` (5, `sc::CHANGE_DOWN`) opens a down interval;
//! `CBTS_CHANGEDEAD` (4, `sc::CHANGE_DEAD`) closes an open down interval and
//! opens a dead interval; `CBTS_CHANGEUP` (3, `sc::CHANGE_UP`) closes
//! whichever interval (down or dead) is currently open. Verified end-to-end
//! against BOTH golden fixtures by reproducing GW2EI's own exported
//! `combatReplayData.down`/`.dead` arrays exactly:
//! - `fixtures/local/wvw-postrework.zevtc` vs
//!   `fixtures/local/wvw-postrework.ei.json`: account `doodlejump.4250`'s
//!   raw events are `CHANGE_DOWN@300150, CHANGE_DEAD@304801,
//!   CHANGE_UP@310123` -- golden `down=[[300150,304801]]`,
//!   `dead=[[304801,310123]]`, an exact match. Account `harasho.4281`'s
//!   raw events (`CHANGE_DOWN@183019, CHANGE_UP@185181, CHANGE_DOWN@188193,
//!   CHANGE_UP@199231`) exactly reproduce golden
//!   `down=[[183019,185181],[188193,199231]]`.
//! - `fixtures/local/wvw-small.zevtc` vs the axibridge golden JSON (via
//!   reverse account-obfuscation lookup): the one player with a non-empty
//!   `down` array (`players[35]`/`Anon130.5810`, golden `down=[[12642,15512]]`) has
//!   raw events `CHANGE_DOWN@12642, CHANGE_UP@15512` -- exact match.
//!
//! All times above are log-relative ms (`event.time - t0`, `t0 =
//! raw.events.first().time` -- the same convention already used by
//! `analysis::cc::timeline`, `analysis::buffs`, and `wvw::markers`).
//!
//! ## Downsampling
//!
//! Mirrors GW2EI's own combat-replay polling
//! (`GW2EIEvtcParser/EIData/CombatReplay/CombatReplay.cs`,
//! `PollingRate`/`HandlePosition`): samples land on a grid of multiples of
//! `poll_ms` (default 300, matching GW2EI's
//! `ParserHelper.CombatReplayPollingRate`), with each grid point computed
//! by linearly interpolating between the two raw `CBTS_POSITION` events
//! bracketing it (holding the nearest endpoint's position outside the
//! track's own observed time range). This project's grid starts at the
//! first multiple of `poll_ms` at or after the track's own "first aware"
//! time (see `build_track`'s doc comment -- the earliest event of ANY kind
//! for that agent, NOT its first `CBTS_POSITION` event specifically) and
//! ends at the last multiple at or before its last observed position --
//! deliberately narrower at the END than GW2EI's own internal
//! representation (which holds the last known position all the way to log
//! end, later trimmed per-actor); this project only ever emits samples
//! backed by real bracketing position data, which is what the M9 Task 1
//! calibration gate checks against GW2EI's exported per-sample positions on
//! the overlapping window. Calibrated to ≥95% of samples within 1.0
//! map-pixel per tracked player on both golden fixtures
//! (`crates/axilog-core/tests/replay_golden.rs`) -- in practice 99.77-100%
//! (see that test file's module doc for the full numbers).

use crate::evtc::{sc, RawEvent, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// GW2EI's own combat-replay polling interval
/// (`ParserHelper.CombatReplayPollingRate`), used as this project's default
/// `poll_ms` for CLI/SDK callers that don't override it (M9 Task 2).
pub const DEFAULT_POLL_MS: u64 = 300;

/// One downsampled position sample. `t_ms` is log-relative
/// (`event.time - t0`). `z` is retained (world height) even though the
/// schema/HTML layers (M9 Tasks 2-3) only surface `x`/`y` -- kept here so a
/// future 3D-aware consumer (or a more faithful map elevation overlay)
/// doesn't need a second decode pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub t_ms: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A closed `[start_ms, end_ms)` interval, log-relative ms, matching the
/// half-open convention GW2EI's own exported `down`/`dead` arrays use (the
/// end timestamp is the closing transition's own time, not included in the
/// interval).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// One agent's combat-replay track: a squad player, or an enemy-player
/// representative (folded across relog/build-swap addrs the same way
/// `analysis::analyze` folds damage -- see `Encounter::players`'/
/// `Encounter::enemies`' `agent_addrs` doc comments).
#[derive(Debug, Clone)]
pub struct Track {
    /// The representative raw agent addr (first of `agent_addrs`) -- stable
    /// join key back to `Encounter::players`/`Encounter::enemies`.
    pub agent_addr: u64,
    /// Display name: `Player::character` for squad players, `Enemy::name`
    /// for enemy-player representatives (M9 Task 2 wires the schema's
    /// richer display-field precedence if any is needed; this stays the
    /// existing per-model name field, matching how every other native
    /// output already labels agents).
    pub name: String,
    pub team: String,
    /// Commander-tag presence. For squad players, `Player::commander`
    /// directly. For enemy-player representatives (no dedicated commander
    /// field on `Enemy`), derived from `Enemy::marker` being a
    /// commander-tag variant name (`*-commander`/`*-catmander`, the same
    /// naming `wvw::markers::COMMANDER_TAG_VARIANTS` produces).
    pub commander: bool,
    pub is_squad: bool,
    /// Downsampled position samples, in ascending `t_ms` order. Empty when
    /// this agent has no `CBTS_POSITION` events at all (e.g. present too
    /// briefly to be tracked, or a build/log without position telemetry).
    pub samples: Vec<Sample>,
    pub down_intervals: Vec<Interval>,
    pub dead_intervals: Vec<Interval>,
    /// Disconnect/not-yet-spawned windows (`CBTS_DESPAWN` to the matching
    /// `CBTS_SPAWN`). Not mutually exclusive with `down_intervals`/
    /// `dead_intervals` -- an agent can despawn while dead.
    pub dc_intervals: Vec<Interval>,
}

/// A full combat replay: every squad player's track plus every
/// enemy-player representative's track, at a fixed `poll_ms` polling
/// interval. NPC/gadget enemies are not tracked (per the M9 Task 1 brief:
/// "squad players AND enemy-player representatives" only).
#[derive(Debug, Clone)]
pub struct Replay {
    pub poll_ms: u64,
    pub tracks: Vec<Track>,
    /// GW2EI's `distToCom`/`stackDist` per tracked agent, keyed by
    /// [`Track::agent_addr`] (Phase B Task 7). Rides on the replay rather
    /// than on `Metrics` because it is a reduction OVER `tracks` and cannot
    /// be computed without them -- a caller who did not ask for a replay
    /// cannot have these numbers at all. See
    /// [`crate::analysis::distance`] for the semantics, which are exacting.
    ///
    /// Every track has an entry; a track with no qualifying poll carries
    /// [`crate::analysis::distance::NO_DISTANCE`], never a missing key.
    pub distance: BTreeMap<u64, crate::analysis::distance::DistanceScalars>,
}

/// Build a [`Replay`] from a decoded log. Standalone -- does not read or
/// alter anything [`crate::analysis::analyze`] computes; a caller that
/// wants both simply calls both.
///
/// `poll_ms` is clamped to a minimum of 1 (a `0` interval would loop
/// forever); pass [`DEFAULT_POLL_MS`] to match GW2EI's own polling rate.
pub fn build_replay(raw: &RawLog, enc: &Encounter, poll_ms: u64) -> Replay {
    let poll_ms = poll_ms.max(1);
    let t0 = raw.log_start_ms();

    let mut tracks = Vec::with_capacity(enc.players.len() + enc.enemies.len());
    for p in &enc.players {
        let roster = RosterEntry {
            addrs: &p.agent_addrs,
            name: p.character.clone(),
            team: p.team.clone(),
            commander: p.commander,
            is_squad: true,
        };
        tracks.push(build_track(raw, t0, poll_ms, roster));
    }
    for en in &enc.enemies {
        if !en.is_player {
            continue; // only enemy-player representatives, per the Task 1 brief
        }
        let commander = en.marker.as_deref().map(is_commander_marker).unwrap_or(false);
        let roster = RosterEntry {
            addrs: &en.agent_addrs,
            name: en.name.clone(),
            team: en.team.clone(),
            commander,
            is_squad: false,
        };
        tracks.push(build_track(raw, t0, poll_ms, roster));
    }
    let mut replay = Replay { poll_ms, tracks, distance: BTreeMap::new() };
    replay.distance = crate::analysis::distance::build(raw, &replay, enc);
    replay
}

/// Whether a resolved marker name is a commander-tag variant, matching the
/// naming `wvw::markers::COMMANDER_TAG_VARIANTS` (`"<color>-commander"`) and
/// its cat-tag counterpart (`"<color>-catmander"`) produce.
fn is_commander_marker(marker: &str) -> bool {
    marker.ends_with("-commander") || marker.ends_with("-catmander")
}

/// The roster-derived (non-position) fields needed to build one [`Track`] --
/// bundled purely to keep `build_track`'s argument count down; not a public
/// type.
struct RosterEntry<'a> {
    addrs: &'a [u64],
    name: String,
    team: String,
    commander: bool,
    is_squad: bool,
}

fn build_track(raw: &RawLog, t0: u64, poll_ms: u64, roster: RosterEntry<'_>) -> Track {
    let RosterEntry { addrs, name, team, commander, is_squad } = roster;
    let addr_set: BTreeSet<u64> = addrs.iter().copied().collect();
    let agent_addr = addrs.first().copied().unwrap_or(0);

    let mut positions: Vec<(u64, f32, f32, f32)> = raw
        .events
        .iter()
        .filter(|e| e.is_statechange == sc::POSITION && addr_set.contains(&e.src_agent))
        .map(|e| decode_position(e, t0))
        .collect();
    positions.sort_by_key(|p| p.0);

    // The polling grid is anchored to this agent's "first aware" time --
    // the earliest event of ANY kind (not just `CBTS_POSITION`) where
    // `src_agent` is this addr -- per the arcdps EVTC reference's own
    // agent-table processing recipe: "set agents[x].first_aware = time on
    // first event" (`README.txt`, the un-numbered section right after the
    // header/agent-table notes). This is USUALLY earlier than the addr's
    // first `CBTS_POSITION` event (position telemetry can start a little
    // after an agent is first tracked) -- using the first position event's
    // own time as the anchor instead (an earlier version of this function
    // did) silently shifted the whole grid one poll step late whenever that
    // gap crossed a `poll_ms` boundary. Verified against both golden
    // fixtures: real agent addr 2000 (`fixtures/local/wvw-small.zevtc`,
    // account `harasho.4281`) has its first event of any kind at
    // log-relative t=0 but its first `CBTS_POSITION` at t=23 -- GW2EI's own
    // exported `combatReplayData.start` for that account is `0`, matching
    // "first aware" exactly and NOT `ceil(23/300)*300=300`, which is what
    // anchoring off the first position event would (wrongly) produce.
    let first_aware_t = raw
        .events
        .iter()
        .filter(|e| addr_set.contains(&e.src_agent))
        .map(|e| e.time.saturating_sub(t0))
        .min();

    let samples = downsample(&positions, first_aware_t, poll_ms);
    let (down_intervals, dead_intervals, dc_intervals) = build_intervals(raw, t0, &addr_set);

    Track {
        agent_addr,
        name,
        team,
        commander,
        is_squad,
        samples,
        down_intervals,
        dead_intervals,
        dc_intervals,
    }
}

/// Decode a `CBTS_POSITION` event's packed payload (see this module's doc
/// comment / `sc::POSITION`'s doc comment for the byte-layout citation):
/// `x`/`y` are two little-endian f32s packed into `dst_agent`'s 8 bytes
/// (low half = x, high half = y); `z` is `value`'s bits reinterpreted as an
/// f32 (NOT a numeric `i32 -> f32` conversion).
fn decode_position(e: &RawEvent, t0: u64) -> (u64, f32, f32, f32) {
    let dst = e.dst_agent.to_le_bytes();
    let x = f32::from_le_bytes(dst[0..4].try_into().unwrap());
    let y = f32::from_le_bytes(dst[4..8].try_into().unwrap());
    let z = f32::from_bits(e.value as u32);
    (e.time.saturating_sub(t0), x, y, z)
}

/// Downsample raw (already time-sorted) position events onto a `poll_ms`
/// grid via linear interpolation between the two bracketing events, per
/// this module's doc comment. The grid starts at the first multiple of
/// `poll_ms` at or after `first_aware_t` (this agent's own "first aware"
/// time -- see `build_track`'s doc comment on why that, and not the first
/// position event's own time, is the correct anchor) and ends at the last
/// multiple at or before the last observed position. Empty `positions`
/// yields empty output (no fabricated samples) regardless of
/// `first_aware_t`.
fn downsample(positions: &[(u64, f32, f32, f32)], first_aware_t: Option<u64>, poll_ms: u64) -> Vec<Sample> {
    if positions.is_empty() {
        return Vec::new();
    }
    let last_t = positions[positions.len() - 1].0;
    // `first_aware_t` is always `Some` whenever `positions` is non-empty
    // (a position event is itself an event for this addr), but fall back
    // to the first position's own time defensively rather than panic.
    let anchor_t = first_aware_t.unwrap_or(positions[0].0);
    let grid_start = anchor_t.div_ceil(poll_ms) * poll_ms;

    let mut out = Vec::new();
    let mut t = grid_start;
    loop {
        let (x, y, z) = interp_at(positions, t);
        out.push(Sample { t_ms: t, x, y, z });
        if t >= last_t {
            break;
        }
        t += poll_ms;
    }
    out
}

/// Linear interpolation of position at time `t`, given ascending,
/// already-sorted `positions`. Clamps (holds) at the first/last sample
/// outside the observed range, matching GW2EI's own `HandlePosition`
/// fallback behavior at the ends of a track.
fn interp_at(positions: &[(u64, f32, f32, f32)], t: u64) -> (f32, f32, f32) {
    debug_assert!(!positions.is_empty());
    if t <= positions[0].0 {
        let p = positions[0];
        return (p.1, p.2, p.3);
    }
    let last = positions[positions.len() - 1];
    if t >= last.0 {
        return (last.1, last.2, last.3);
    }
    // First index whose time is > t (partition_point requires the ascending
    // sort already established above).
    let idx = positions.partition_point(|p| p.0 <= t);
    let a = positions[idx - 1];
    let b = positions[idx];
    if b.0 == a.0 {
        return (a.1, a.2, a.3);
    }
    let ratio = (t - a.0) as f32 / (b.0 - a.0) as f32;
    (
        a.1 + (b.1 - a.1) * ratio,
        a.2 + (b.2 - a.2) * ratio,
        a.3 + (b.3 - a.3) * ratio,
    )
}

/// One player's cheap activity data (M11 Task 3): down/dead intervals plus
/// first/last-aware bounds, WITHOUT the downsampled position track --
/// intervals (and the min/max event-time scan below) are cheap, positions
/// are the expensive part (decode + sort + interpolate every `CBTS_POSITION`
/// event), so this is computed unconditionally by [`build_activity_intervals`]
/// regardless of whether `--replay`/SDK `replay: true` was requested --
/// unlike [`Track`]'s `samples`, which stay behind that opt-in gate. See
/// `axilog_ei::to_ei_json`'s `combatReplayData`/`activeTimes` mapping for
/// the consumer.
#[derive(Debug, Clone)]
pub struct ActivityIntervals {
    /// The representative raw agent addr (first of `agent_addrs`) -- stable
    /// join key, matching [`Track::agent_addr`].
    pub agent_addr: u64,
    /// This agent's own "first aware" time -- earliest event of ANY kind,
    /// log-relative ms (same anchor `build_track` uses for the position
    /// polling grid -- see its doc comment for the citation trail).
    /// Mirrors GW2EI's `AgentItem.FirstAware` / exported
    /// `combatReplayData.start`.
    pub start_ms: u64,
    /// This agent's own "last aware" time -- latest event of ANY kind,
    /// log-relative ms. Mirrors GW2EI's `AgentItem.LastAware` / exported
    /// `combatReplayData.end`.
    pub end_ms: u64,
    pub down_intervals: Vec<Interval>,
    pub dead_intervals: Vec<Interval>,
    /// Disconnect/not-yet-spawned windows -- see [`Track::dc_intervals`]'s
    /// doc comment for the separate-slot rationale.
    pub dc_intervals: Vec<Interval>,
}

impl ActivityIntervals {
    /// GW2EI's `SingleActorStatusHelper.GetActiveDuration`
    /// (`GW2EIEvtcParser/EIData/Actors/ActorsHelper/
    /// SingleActorStatusHelper.cs`): for the whole-fight phase (`start=0`,
    /// `end=durationMS`), `activeDuration = (end - start) -
    /// sum(dead-segments ∩ [start,end]) - sum(dc-segments ∩ [start,end])`,
    /// where GW2EI's own `dc` ("disconnected"/not-yet-spawned) segments are
    /// exactly `[MinValue, FirstAware)` and `(LastAware, MaxValue]`
    /// (`SingleActorStatusHelper.FillStatus`) in the common case (no mid-
    /// fight despawn/respawn). Down time is deliberately NOT subtracted --
    /// verified against the real EI export this project's golden fixture
    /// derives from (`axibridge/test-fixtures/boon/20260117-181030.json`):
    /// player `players[35]`/`Anon130.5810` has a real 2870ms down interval
    /// (`combatReplayData.down = [[12642, 15512]]`) yet
    /// `activeTimes[0] = 49265` exactly equals `end - start` (`49266 - 1`),
    /// no down-time deduction; cross-checked against all 41 players in that
    /// export (every one satisfies `activeTimes[0] == end - start -
    /// dead_ms` exactly, `dead_ms` always 0 in that particular log).
    ///
    /// This reduces to `(end_ms - start_ms) - dead_ms` here because
    /// `start_ms`/`end_ms` are already this agent's own first/last-aware
    /// bounds (GW2EI's `dc` boundary segments, by construction, fall
    /// entirely outside `[start_ms, end_ms]`) -- algebraically identical to
    /// GW2EI's own formula whenever `dc` only occurs at those two log-
    /// boundary edges. This project doesn't track GW2EI's rarer mid-log
    /// despawn/respawn `dc` segments (out of scope for this cheap tier-1
    /// win -- a genuine WvW relog is folded across `agent_addrs` into one
    /// continuous track instead, per `build_track`'s own folding
    /// convention), so a player with a real mid-fight disconnect gap would
    /// diverge slightly from GW2EI's own number here; the M11 Task 3 gate
    /// is a 0.5% tolerance, not byte-exact, for exactly this reason.
    pub fn active_ms(&self) -> u64 {
        let span = self.end_ms.saturating_sub(self.start_ms);
        let dead_ms: u64 = self
            .dead_intervals
            .iter()
            .map(|iv| iv.end_ms.saturating_sub(iv.start_ms))
            .sum();
        span.saturating_sub(dead_ms)
    }
}

/// Build [`ActivityIntervals`] for every player in `enc.players`, in that
/// same order (positional join key back to `axilog_schema::Report::players`
/// -- both are built by iterating `enc.players` 1:1, see
/// `axilog_schema::build_report`'s own player loop). Cheap: no position
/// decode/sort/interpolation, just a min/max scan over each player's own
/// events plus the existing [`build_intervals`] down/dead scan -- safe to
/// call unconditionally (M11 Task 3), unlike [`build_replay`].
pub fn build_activity_intervals(raw: &RawLog, enc: &Encounter) -> Vec<ActivityIntervals> {
    let t0 = raw.log_start_ms();
    enc.players
        .iter()
        .map(|p| {
            let addr_set: BTreeSet<u64> = p.agent_addrs.iter().copied().collect();
            let agent_addr = p.agent_addrs.first().copied().unwrap_or(p.agent_addr);
            let mut start_ms = u64::MAX;
            let mut end_ms = 0u64;
            for e in raw.events.iter().filter(|e| addr_set.contains(&e.src_agent)) {
                let t = e.time.saturating_sub(t0);
                start_ms = start_ms.min(t);
                end_ms = end_ms.max(t);
            }
            if start_ms == u64::MAX {
                start_ms = 0; // no events at all for this player -- degenerate, avoid u64::MAX leaking out
            }
            let (down_intervals, dead_intervals, dc_intervals) = build_intervals(raw, t0, &addr_set);
            ActivityIntervals {
                agent_addr,
                start_ms,
                end_ms,
                down_intervals,
                dead_intervals,
                dc_intervals,
            }
        })
        .collect()
}

/// Build down/dead/dc intervals for one agent (folded across `addr_set`)
/// from `CBTS_CHANGEDOWN`/`CBTS_CHANGEDEAD`/`CBTS_CHANGEUP`/`CBTS_SPAWN`/
/// `CBTS_DESPAWN`, per this module's doc comment. Events across all of
/// `addr_set` are merged and time-sorted first, so a relog mid-down (a
/// genuinely obscure edge case) still closes correctly.
///
/// `dc` (disconnect/not-yet-spawned) tracking uses its own `dc_open` slot,
/// separate from the down/dead state machine's `open` slot -- an agent can
/// despawn while dead (dead THEN disconnect, without an intervening
/// `CHANGE_UP`), and collapsing both into one slot would silently drop
/// whichever interval was still open. A `dc` interval still open when the
/// event scan ends (the agent never re-spawned before the log ended) is
/// left unclosed rather than clamped to the log's last event time: the
/// caller already knows the log duration, and a clamped interval would be
/// indistinguishable from a real one that happened to end exactly there.
fn build_intervals(
    raw: &RawLog,
    t0: u64,
    addr_set: &BTreeSet<u64>,
) -> (Vec<Interval>, Vec<Interval>, Vec<Interval>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Down,
        Dead,
    }
    let mut events: Vec<(u64, u8)> = raw
        .events
        .iter()
        .filter(|e| addr_set.contains(&e.src_agent))
        .filter(|e| {
            matches!(
                e.is_statechange,
                sc::CHANGE_DOWN | sc::CHANGE_DEAD | sc::CHANGE_UP | sc::SPAWN | sc::DESPAWN
            )
        })
        .map(|e| (e.time.saturating_sub(t0), e.is_statechange))
        .collect();
    events.sort_by_key(|e| e.0);

    let mut down_intervals = Vec::new();
    let mut dead_intervals = Vec::new();
    let mut dc_intervals = Vec::new();
    let mut open: Option<(Kind, u64)> = None;
    let mut dc_open: Option<u64> = None;
    for (t, kind) in events {
        match kind {
            sc::CHANGE_DOWN => {
                open = Some((Kind::Down, t));
            }
            sc::CHANGE_DEAD => {
                if let Some((Kind::Down, start)) = open {
                    down_intervals.push(Interval { start_ms: start, end_ms: t });
                }
                open = Some((Kind::Dead, t));
            }
            sc::CHANGE_UP => {
                if let Some((k, start)) = open.take() {
                    match k {
                        Kind::Down => down_intervals.push(Interval { start_ms: start, end_ms: t }),
                        Kind::Dead => dead_intervals.push(Interval { start_ms: start, end_ms: t }),
                    }
                }
            }
            sc::DESPAWN => {
                if dc_open.is_none() {
                    dc_open = Some(t);
                }
            }
            sc::SPAWN => {
                if let Some(start) = dc_open.take() {
                    dc_intervals.push(Interval { start_ms: start, end_ms: t });
                }
            }
            _ => unreachable!(),
        }
    }
    (down_intervals, dead_intervals, dc_intervals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn pos_event(time: u64, src: u64, x: f32, y: f32, z: f32) -> RawEvent {
        let x_bytes = x.to_le_bytes();
        let y_bytes = y.to_le_bytes();
        let mut dst = [0u8; 8];
        dst[0..4].copy_from_slice(&x_bytes);
        dst[4..8].copy_from_slice(&y_bytes);
        RawEvent {
            time,
            src_agent: src,
            dst_agent: u64::from_le_bytes(dst),
            value: z.to_bits() as i32,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::POSITION,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0, pad: 0,
        }
    }

    fn state_event(time: u64, src: u64, statechange: u8) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: statechange,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0, pad: 0,
        }
    }

    fn player(addr: u64) -> Player {
        Player {
            agent_addr: addr,
            account: ":A.1".into(),
            character: "Alice".into(),
            profession: "Thief".into(),
            elite_spec: "".into(),
            team: "red".into(),
            subgroup: 1,
            in_squad: true,
            commander: true,
            marker: None,
            commander_tag: None, guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    fn empty_enc() -> Encounter {
        Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 0,
            build: "".into(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![],
            enemies: vec![],
            markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        }
    }

    /// Wrong-offset-fails decode probe: reading `x`/`y` from anywhere but
    /// `dst_agent`'s low/high f32 halves, or `z` from anywhere but a bit-
    /// reinterpret of `value`, must NOT reproduce the packed values.
    #[test]
    fn decodes_packed_position_payload() {
        let e = pos_event(1000, 5, 123.5, -67.25, 8.75);
        let (t, x, y, z) = decode_position(&e, 0);
        assert_eq!(t, 1000);
        assert_eq!(x, 123.5);
        assert_eq!(y, -67.25);
        assert_eq!(z, 8.75);

        // Wrong-offset probes: swapping x/y (reading high half as x) must
        // NOT accidentally match.
        assert_ne!(x, y);
        // z must come from `value`'s bit pattern, not a numeric cast of the
        // raw i32 (8.75 packed as bits is a large integer, nothing like a
        // plausible coordinate -- if a future edit swaps `f32::from_bits`
        // for `as f32`, this assertion catches it).
        assert_ne!(e.value as f32, z, "z must be a bit-reinterpret of `value`, not a numeric cast");
    }

    #[test]
    fn t_ms_is_relative_to_first_event_time() {
        let e = pos_event(5_000, 5, 1.0, 2.0, 0.0);
        let (t, ..) = decode_position(&e, 4_000);
        assert_eq!(t, 1_000);
    }

    #[test]
    fn downsample_interpolates_between_bracketing_samples() {
        let positions = vec![(0u64, 0.0f32, 0.0f32, 0.0f32), (300, 30.0, 60.0, 0.0)];
        let samples = downsample(&positions, Some(0), 100);
        // Grid: 0, 100, 200, 300.
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0], Sample { t_ms: 0, x: 0.0, y: 0.0, z: 0.0 });
        assert_eq!(samples[1], Sample { t_ms: 100, x: 10.0, y: 20.0, z: 0.0 });
        assert_eq!(samples[2], Sample { t_ms: 200, x: 20.0, y: 40.0, z: 0.0 });
        assert_eq!(samples[3], Sample { t_ms: 300, x: 30.0, y: 60.0, z: 0.0 });
    }

    #[test]
    fn downsample_grid_starts_at_first_multiple_at_or_after_first_aware() {
        // `first_aware_t` (t=50, not a multiple of the 300ms poll interval)
        // is EARLIER than the first real position sample (t=200) -- the
        // ordinary case (an agent is usually "aware" -- has SOME event --
        // slightly before its first position telemetry). The grid must
        // start at 300, the first multiple >= 50 (the anchor), NOT the
        // first multiple >= 200 (which would also be 300 here by
        // coincidence -- see the next test for a case that distinguishes
        // them), per GW2EI's own `ceil(start / rate) * rate` convention
        // (verified against the golden fixtures in
        // `tests/replay_golden.rs`).
        let positions = vec![(200u64, 1.0f32, 1.0f32, 0.0f32), (900, 5.0, 5.0, 0.0)];
        let samples = downsample(&positions, Some(50), 300);
        assert_eq!(samples[0].t_ms, 300);
        assert_eq!(samples.last().unwrap().t_ms, 900);
    }

    /// Distinguishes "anchor off first_aware" from "anchor off first
    /// position sample": `first_aware_t=23` and the first position sample
    /// is at `t=301` -- anchoring off the position sample would start the
    /// grid at 600 (`ceil(301/300)*300`), but the correct grid start (per
    /// `build_track`'s doc comment, and reproducing the real
    /// `wvw-small.zevtc` account `harasho.4281` finding that drove this
    /// fix) is 300 (`ceil(23/300)*300`).
    #[test]
    fn downsample_anchors_off_first_aware_not_first_position_sample() {
        let positions = vec![(301u64, 1.0f32, 1.0f32, 0.0f32), (900, 5.0, 5.0, 0.0)];
        let samples = downsample(&positions, Some(23), 300);
        assert_eq!(samples[0].t_ms, 300, "grid must anchor off first_aware_t (23), not the first position sample (301)");
    }

    #[test]
    fn downsample_holds_first_and_last_sample_outside_observed_range() {
        let positions = vec![(300u64, 10.0f32, 20.0f32, 0.0f32), (300, 10.0, 20.0, 0.0)];
        // Degenerate single-timestamp track (duplicate entries at the same
        // time -- a relog boundary edge case): must not panic or divide by
        // zero, and holds that single position.
        let samples = downsample(&positions, Some(300), 300);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0], Sample { t_ms: 300, x: 10.0, y: 20.0, z: 0.0 });
    }

    #[test]
    fn downsample_is_deterministic() {
        let positions = vec![
            (0u64, 0.0f32, 0.0f32, 0.0f32),
            (137, 5.0, -5.0, 1.0),
            (401, 12.0, 8.0, -1.0),
            (900, 0.0, 0.0, 0.0),
        ];
        let a = downsample(&positions, Some(0), 300);
        let b = downsample(&positions, Some(0), 300);
        assert_eq!(a, b);
    }

    /// Down interval: opened at `CHANGE_DOWN`, closed at the next
    /// `CHANGE_UP` (revived) -- exactly reproduces the
    /// `Anon130.5810`-style single-down-no-death shape
    /// found in both golden fixtures (see this module's doc comment).
    #[test]
    fn down_interval_closes_on_change_up() {
        let raw = raw_from(vec![
            state_event(12_642, 7, sc::CHANGE_DOWN),
            state_event(15_512, 7, sc::CHANGE_UP),
        ]);
        let addrs: BTreeSet<u64> = [7].into_iter().collect();
        let (down, dead, _dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(down, vec![Interval { start_ms: 12_642, end_ms: 15_512 }]);
        assert!(dead.is_empty());
    }

    /// Down interval closes at `CHANGE_DEAD` (killed while down), which
    /// simultaneously opens a dead interval closed by the eventual respawn
    /// `CHANGE_UP` -- exactly reproduces `doodlejump.4250`'s golden shape.
    #[test]
    fn down_then_dead_then_respawn() {
        let raw = raw_from(vec![
            state_event(300_150, 3, sc::CHANGE_DOWN),
            state_event(304_801, 3, sc::CHANGE_DEAD),
            state_event(310_123, 3, sc::CHANGE_UP),
        ]);
        let addrs: BTreeSet<u64> = [3].into_iter().collect();
        let (down, dead, _dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(down, vec![Interval { start_ms: 300_150, end_ms: 304_801 }]);
        assert_eq!(dead, vec![Interval { start_ms: 304_801, end_ms: 310_123 }]);
    }

    /// Two separate down cycles for the same agent both get captured, in
    /// order -- reproduces `harasho.4281`'s golden shape (down, up, down,
    /// up).
    #[test]
    fn multiple_down_cycles() {
        let raw = raw_from(vec![
            state_event(183_019, 9, sc::CHANGE_DOWN),
            state_event(185_181, 9, sc::CHANGE_UP),
            state_event(188_193, 9, sc::CHANGE_DOWN),
            state_event(199_231, 9, sc::CHANGE_UP),
        ]);
        let addrs: BTreeSet<u64> = [9].into_iter().collect();
        let (down, dead, _dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(
            down,
            vec![
                Interval { start_ms: 183_019, end_ms: 185_181 },
                Interval { start_ms: 188_193, end_ms: 199_231 },
            ]
        );
        assert!(dead.is_empty());
    }

    /// An interval left open at the end of the log (never revived/died in
    /// the recording -- e.g. the fight ends mid-down) is simply dropped,
    /// not emitted half-open -- matches this module's documented half-open
    /// `[start, end)` `Interval` contract (an interval needs a real closing
    /// event).
    #[test]
    fn unclosed_interval_at_log_end_is_dropped() {
        let raw = raw_from(vec![state_event(1_000, 4, sc::CHANGE_DOWN)]);
        let addrs: BTreeSet<u64> = [4].into_iter().collect();
        let (down, dead, _dc) = build_intervals(&raw, 0, &addrs);
        assert!(down.is_empty());
        assert!(dead.is_empty());
    }

    /// A `dc` interval covers the window an agent was not trackable --
    /// both the pre-spawn head of the log and any mid-fight despawn gap.
    /// EI's distance metrics null out positions across exactly these
    /// windows, which is why the replay block has to export them.
    #[test]
    fn build_intervals_reports_despawn_gaps_as_dc() {
        let events = vec![
            state_event(1000, 1, sc::DESPAWN),
            state_event(4000, 1, sc::SPAWN),
        ];
        let raw = raw_from(events);
        let addrs: BTreeSet<u64> = [1u64].into_iter().collect();
        let (down, dead, dc) = build_intervals(&raw, 0, &addrs);
        assert!(down.is_empty());
        assert!(dead.is_empty());
        assert_eq!(dc, vec![Interval { start_ms: 1000, end_ms: 4000 }]);
    }

    /// `dc` must not overlap `down` or `dead` -- a consumer intersecting
    /// the three to find "active" polls depends on them being disjoint.
    #[test]
    fn dc_does_not_overlap_down_or_dead() {
        let events = vec![
            state_event(1000, 1, sc::CHANGE_DOWN),
            state_event(2000, 1, sc::CHANGE_UP),
            state_event(3000, 1, sc::DESPAWN),
            state_event(5000, 1, sc::SPAWN),
        ];
        let raw = raw_from(events);
        let addrs: BTreeSet<u64> = [1u64].into_iter().collect();
        let (down, _dead, dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(down, vec![Interval { start_ms: 1000, end_ms: 2000 }]);
        assert_eq!(dc, vec![Interval { start_ms: 3000, end_ms: 5000 }]);
        for d in &down {
            for c in &dc {
                assert!(d.end_ms <= c.start_ms || c.end_ms <= d.start_ms, "overlap");
            }
        }
    }

    /// The entire justification for `dc_open` being a separate slot from
    /// the down/dead `open` slot: an agent can despawn WHILE dead (no
    /// intervening `CHANGE_UP`). If a future refactor collapsed the two
    /// slots, this despawn would either silently drop the still-open dead
    /// interval or fail to open `dc` at all -- this test pins both outputs
    /// as genuinely bounded values (not mere non-emptiness) so either
    /// failure mode is caught.
    #[test]
    fn dc_opens_while_dead_is_already_open() {
        let raw = raw_from(vec![
            state_event(1_000, 1, sc::CHANGE_DOWN),
            state_event(1_500, 1, sc::CHANGE_DEAD),
            state_event(2_000, 1, sc::DESPAWN), // despawns while still dead
            state_event(3_000, 1, sc::SPAWN),
            state_event(4_000, 1, sc::CHANGE_UP), // eventual respawn closes dead
        ]);
        let addrs: BTreeSet<u64> = [1].into_iter().collect();
        let (down, dead, dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(down, vec![Interval { start_ms: 1_000, end_ms: 1_500 }]);
        assert_eq!(dead, vec![Interval { start_ms: 1_500, end_ms: 4_000 }]);
        assert_eq!(dc, vec![Interval { start_ms: 2_000, end_ms: 3_000 }]);
        // The dc window sits entirely inside the still-open dead window --
        // exactly the simultaneity a shared slot could not represent.
        assert!(dc[0].start_ms >= dead[0].start_ms && dc[0].end_ms <= dead[0].end_ms);
    }

    /// A `dc` interval left open at the end of the log (despawned, never
    /// re-spawned before the recording ends) is dropped, not synthesized
    /// with a log-end closing bound -- the same convention already applied
    /// to unclosed down/dead intervals above.
    #[test]
    fn unclosed_dc_interval_at_log_end_is_dropped() {
        let raw = raw_from(vec![state_event(1_000, 4, sc::DESPAWN)]);
        let addrs: BTreeSet<u64> = [4].into_iter().collect();
        let (down, dead, dc) = build_intervals(&raw, 0, &addrs);
        assert!(down.is_empty());
        assert!(dead.is_empty());
        assert!(dc.is_empty(), "an unclosed dc interval must not be synthesized with a log-end bound");
    }

    /// Relog folding: position events and down/dead statechanges from
    /// EITHER of an account's raw addrs must land on the same single
    /// track, exactly like `analysis::analyze` folds damage across
    /// `agent_addrs` (Finding #1).
    #[test]
    fn folds_position_and_interval_events_across_relog_addrs() {
        let mut p = player(1);
        p.agent_addrs = vec![1, 2];
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            pos_event(0, 1, 0.0, 0.0, 0.0),   // pre-relog addr
            pos_event(300, 2, 30.0, 0.0, 0.0), // post-relog addr, same account
            state_event(100, 1, sc::CHANGE_DOWN),
            state_event(200, 2, sc::CHANGE_UP),
        ]);
        let replay = build_replay(&raw, &enc, 300);
        assert_eq!(replay.tracks.len(), 1);
        let track = &replay.tracks[0];
        assert_eq!(track.samples.len(), 2, "positions from both addrs folded onto one track");
        assert_eq!(track.down_intervals, vec![Interval { start_ms: 100, end_ms: 200 }]);
    }

    #[test]
    fn squad_player_track_carries_commander_and_team() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![pos_event(0, 1, 1.0, 2.0, 0.0)]);
        let replay = build_replay(&raw, &enc, 300);
        assert_eq!(replay.tracks.len(), 1);
        let t = &replay.tracks[0];
        assert!(t.is_squad);
        assert!(t.commander);
        assert_eq!(t.team, "red");
        assert_eq!(t.name, "Alice");
    }

    #[test]
    fn only_enemy_players_get_tracks_not_npcs_or_gadgets() {
        let enc = Encounter {
            enemies: vec![
                Enemy { id: 9, instid: 0, name: "Enemy Player".into(), team: "blue".into(),
                    is_player: true, marker: None,
                    profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                    agent_addrs: vec![9] },
                Enemy { id: 10, instid: 0, name: "Sentry".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![10] },
            ],
            ..empty_enc()
        };
        let raw = raw_from(vec![
            pos_event(0, 9, 1.0, 1.0, 0.0),
            pos_event(0, 10, 2.0, 2.0, 0.0),
        ]);
        let replay = build_replay(&raw, &enc, 300);
        assert_eq!(replay.tracks.len(), 1, "only the enemy-player representative gets a track");
        assert_eq!(replay.tracks[0].agent_addr, 9);
        assert!(!replay.tracks[0].is_squad);
    }

    #[test]
    fn enemy_commander_tag_marker_sets_commander_flag() {
        let enc = Encounter {
            enemies: vec![Enemy {
                id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true,
                marker: Some("red-commander".into()),
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                agent_addrs: vec![9],
            }],
            ..empty_enc()
        };
        let raw = raw_from(vec![pos_event(0, 9, 0.0, 0.0, 0.0)]);
        let replay = build_replay(&raw, &enc, 300);
        assert!(replay.tracks[0].commander);
    }

    #[test]
    fn track_with_no_position_events_has_empty_samples_not_a_panic() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![]);
        let replay = build_replay(&raw, &enc, 300);
        assert_eq!(replay.tracks.len(), 1);
        assert!(replay.tracks[0].samples.is_empty());
    }

    #[test]
    fn poll_ms_zero_is_clamped_to_one_not_an_infinite_loop() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![pos_event(0, 1, 0.0, 0.0, 0.0), pos_event(2, 1, 2.0, 0.0, 0.0)]);
        let replay = build_replay(&raw, &enc, 0);
        assert_eq!(replay.poll_ms, 1);
        assert_eq!(replay.tracks[0].samples.len(), 3); // t=0,1,2
    }

    // -- M11 Task 3: `build_activity_intervals`/`ActivityIntervals::active_ms` --

    #[test]
    fn activity_intervals_start_end_are_first_and_last_event_of_any_kind() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        // t0 = the log's first event's own time (50, per this module's
        // `t0 = raw.events.first().time` convention) -- relative times
        // below are each event's absolute time minus that 50.
        let raw = raw_from(vec![
            pos_event(50, 1, 0.0, 0.0, 0.0),
            state_event(200, 1, sc::CHANGE_DOWN),
            state_event(400, 1, sc::CHANGE_UP),
            pos_event(1000, 1, 1.0, 1.0, 0.0),
        ]);
        let activity = build_activity_intervals(&raw, &enc);
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].start_ms, 0, "earliest event of any kind, not just position events");
        assert_eq!(activity[0].end_ms, 950, "latest event of any kind, relative to t0=50");
        assert_eq!(activity[0].down_intervals, vec![Interval { start_ms: 150, end_ms: 350 }]);
        assert!(activity[0].dead_intervals.is_empty());
    }

    #[test]
    fn active_ms_subtracts_dead_but_not_down() {
        // 1000ms total span (0..1000); a 200ms down (200..400, NOT
        // subtracted) and a 100ms dead (600..700, subtracted).
        let iv = ActivityIntervals {
            agent_addr: 1,
            start_ms: 0,
            end_ms: 1000,
            down_intervals: vec![Interval { start_ms: 200, end_ms: 400 }],
            dead_intervals: vec![Interval { start_ms: 600, end_ms: 700 }],
            dc_intervals: vec![],
        };
        assert_eq!(iv.active_ms(), 900, "1000 - 100 dead, down NOT subtracted");
    }

    #[test]
    fn active_ms_with_no_dead_time_equals_the_full_span() {
        let iv = ActivityIntervals {
            agent_addr: 1,
            start_ms: 3,
            end_ms: 49_266,
            down_intervals: vec![Interval { start_ms: 12_642, end_ms: 15_512 }],
            dead_intervals: vec![],
            dc_intervals: vec![],
        };
        // Matches the real EI golden finding cited in `active_ms`'s doc
        // comment (`Anon130.5810`: end-start with no dead == activeTimes
        // exactly, down time not subtracted).
        assert_eq!(iv.active_ms(), 49_263);
    }

    #[test]
    fn build_activity_intervals_is_positionally_joined_to_enc_players() {
        let mut p1 = player(1);
        p1.character = "First".into();
        let mut p2 = player(2);
        p2.character = "Second".into();
        let enc = Encounter { players: vec![p1, p2], ..empty_enc() };
        let raw = raw_from(vec![
            state_event(0, 1, sc::CHANGE_DOWN),
            state_event(10, 1, sc::CHANGE_UP),
            state_event(0, 2, sc::CHANGE_DOWN),
            state_event(20, 2, sc::CHANGE_UP),
        ]);
        let activity = build_activity_intervals(&raw, &enc);
        assert_eq!(activity.len(), 2);
        // Index 0 <-> enc.players[0] (agent 1), index 1 <-> enc.players[1]
        // (agent 2) -- same order, no reordering/filtering.
        assert_eq!(activity[0].agent_addr, 1);
        assert_eq!(activity[0].down_intervals, vec![Interval { start_ms: 0, end_ms: 10 }]);
        assert_eq!(activity[1].agent_addr, 2);
        assert_eq!(activity[1].down_intervals, vec![Interval { start_ms: 0, end_ms: 20 }]);
    }

    #[test]
    fn build_activity_intervals_folds_across_relog_addrs() {
        let mut p = player(1);
        p.agent_addrs = vec![1, 2];
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            state_event(0, 1, sc::CHANGE_DOWN),   // pre-relog addr
            state_event(100, 2, sc::CHANGE_UP),   // post-relog addr, same account
        ]);
        let activity = build_activity_intervals(&raw, &enc);
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].start_ms, 0);
        assert_eq!(activity[0].end_ms, 100);
        assert_eq!(activity[0].down_intervals, vec![Interval { start_ms: 0, end_ms: 100 }]);
    }

    #[test]
    fn build_activity_intervals_handles_a_player_with_no_events_without_panicking() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![]);
        let activity = build_activity_intervals(&raw, &enc);
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0].start_ms, 0);
        assert_eq!(activity[0].end_ms, 0);
        assert_eq!(activity[0].active_ms(), 0);
    }
}
