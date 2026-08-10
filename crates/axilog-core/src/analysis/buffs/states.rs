//! GW2EI-shape boon stack timelines -- `buffUptimes[].states` and
//! `buffUptimes[].statesPerSource` (MEIGAP Task 1b).
//!
//! ## What GW2EI emits
//!
//! `JsonBuffsUptime.States` is `long[][]`
//! (`GW2EIJSON/JsonActorUtilities/JsonBuffsUptime.cs:77`), built by
//! `JsonBuffsUptimeBuilder.GetBuffStates`
//! (`GW2EIBuilders/JsonModels/JsonActorUtilities/JsonBuffsUptimeBuilder.cs:68-76`):
//!
//! ```text
//! bgm.Values.Select(x => new List<long>() { x.Start, (int)x.Value })
//! ```
//!
//! i.e. one `[segment start, stack count]` pair per segment of the actor's
//! fused buff graph -- transition points only, the value holding until the
//! next pair, the stack count TRUNCATED to `int`. Three structural
//! properties, all verified against this project's reference export:
//!
//! 1. **A leading `[0, 0]` is always present.**
//!    `SingleActorBuffsHelper.SimulateBuffsAndComputeGraphs`
//!    (`GW2EIEvtcParser/EIData/Actors/ActorsHelper/SingleActorBuffsHelper.cs:1005-1030`)
//!    prepends `Segment(LogStart, firstSegment.Start, 0)`. Measured: 0 of
//!    the export's 2,200 `buffUptimes` entries start with anything else --
//!    including 7 whose SECOND pair is also at time 0 (a buff applied on
//!    the very first tick), so the zero-length leading segment survives
//!    `FuseSegments` rather than being dropped.
//! 2. **Times are LOG-RELATIVE** (GW2EI's own `LogStart`-based timescale),
//!    where this crate's [`super::BoonTimeline`] carries absolute arcdps
//!    `timeGetTime()` values. The offset applied here is
//!    `raw.events.first().time`, the same log-start anchor
//!    `buffs::simulate_boon_uptimes`/`uptime::compute` already use as their
//!    window floor.
//! 3. **A trailing zero is emitted when the buff runs out before log end**
//!    (the appended `Segment(last.End, LogEnd, 0)` at `:1023-1026`), and
//!    NOT when it is still up -- that appended segment is zero-length and
//!    fuses away. This crate's simulator already behaves that way (it
//!    emits a `0` transition at each expiry and leaves a still-held stack
//!    un-flushed past `log_end`, per GW2EI's own `Trim(logEnd)`), so no
//!    adjustment is needed here.
//!
//! `StatesPerSource` (`JsonBuffsUptime.cs:86`, built at
//! `JsonBuffsUptimeBuilder.cs:55-63`) is the same shape per SOURCE, keyed
//! by `source.Character` -- the source actor's character name, the same key
//! space as the `generated`/`wasted` dictionaries on `buffData`. Its values
//! come from the per-source graph at `SingleActorBuffsHelper.cs:472-520`,
//! built from `simul.ToSegment(by)`, where a segment contributes the number
//! of ITS stacks owned by that source (`BuffSimulationItemBase.GetStacks`,
//! `.../Base/BuffSimulationItemBase.cs:47-50`). That is exactly the overlap
//! count of [`generation::HeldSegment`]s belonging to one source, which is
//! what [`build`] computes.
//!
//! ## Duration boons are 0/1, not queue depth
//!
//! GW2EI's stack count comes from `BuffSimulationItem.GetStacks()`, which
//! for a DURATION-type (Queue/Force/Healing) buff is the single active
//! stack -- `BuffSimulationItemDuration` models one ticking stack plus a
//! frozen queue, and the queue contributes nothing to the graph. Measured
//! across the whole reference export: every one of the ten duration boons
//! tops out at `states`/`statesPerSource` value **1**, while the two
//! intensity boons (Might, Stability) reach 25 and 17.
//!
//! This crate's [`super::BoonTimeline`] instead carries the QUEUE DEPTH for
//! duration boons (`simulator::run_duration`'s frozen-queue model -- the
//! thing `uptime::compute` reduces with a `> 0` presence test, so the queue
//! depth never leaks into an uptime number). [`build`] therefore clamps
//! duration boons to `0`/`1` on the way out, which is what makes the
//! emitted graph mean the same thing GW2EI's does. The per-source side
//! needs no clamp: only one source can own the active slot at a time, so
//! its overlap count is already 0 or 1 by construction.
//!
//! ## Gating
//!
//! Both arrays sit inside `if (settings.RawFormatTimelineArrays)`
//! (`JsonBuffsUptimeBuilder.cs:52`) in GW2EI itself -- the same setting
//! axibridge maps onto axilog's `--timeseries`. So this pass is opt-in and
//! standalone (not wired into `analyze()`), like `replay`/`missiles`/
//! `damage_mods`, and the ei-json adapter emits the two keys exactly when
//! the caller supplies its result.

use super::generation::{self, HeldSegment};
use super::{BoonTimeline, BOON_IDS};
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeMap;

/// GW2EI's own placeholder for an actor it cannot name -- what the
/// reference export shows for every unresolved boon source (e.g.
/// `"statesPerSource": {"UNKNOWN": [...]}`). Reused verbatim rather than
/// inventing a key, so a consumer keyed on character names sees exactly the
/// string EI would have written.
pub const UNKNOWN_SOURCE: &str = "UNKNOWN";

/// One `[[time_ms_from_log_start, stacks], ...]` step timeline.
pub type StateTimeline = Vec<(u64, u32)>;

/// Per-(player representative addr, buff id) state timelines, split by
/// source CHARACTER name.
pub type PerSourceTimelines = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;

/// One log's EI-shape boon stack timelines, keyed by
/// `(player representative addr, buff id)`.
#[derive(Debug, Clone, Default)]
pub struct BoonStates {
    /// `buffUptimes[].states`.
    pub total: BTreeMap<(u64, u32), StateTimeline>,
    /// `buffUptimes[].statesPerSource`: the same, per source CHARACTER name
    /// ([`UNKNOWN_SOURCE`] for a source that is not a recorded player).
    pub per_source: PerSourceTimelines,
}

/// Turns an absolute-time `(time, value)` step list into GW2EI's
/// log-relative shape with its leading `[0, 0]` (see this module's doc,
/// point 1).
///
/// A transition at or before `log_start` clamps to relative time 0, where
/// it lands after the mandatory leading pair -- the same order GW2EI's own
/// fused graph produces for a buff already up at log start.
pub(crate) fn to_ei_states(
    steps: impl Iterator<Item = (u64, u32)>,
    log_start: u64,
) -> Vec<(u64, u32)> {
    let mut out: Vec<(u64, u32)> = vec![(0u64, 0u32)];
    for (t, v) in steps {
        let t = t.saturating_sub(log_start);
        // GW2EI's `FuseSegments` (`GW2EIEvtcParser/EIData/MathUtils/
        // StateGraph.cs:24-28`) runs over the graph before it is
        // serialized, so the emitted list never carries two pairs at the
        // same timestamp (only the last value at that instant survives) nor
        // two consecutive pairs with the same value. This project's own
        // stack-count simulator emits a raw transition per state change,
        // several of which can land on the same millisecond (a batch of
        // stacks expiring together), so the same fusion is applied here.
        //
        // The one pair deliberately NOT fused away is the mandatory leading
        // `[0, 0]`: GW2EI keeps it even when a real transition lands at
        // time 0 (measured: 7 such entries in the reference export, all
        // shaped `[[0, 0], [0, n], ...]`), so a first real transition at 0
        // is APPENDED rather than merged.
        if out.len() > 1 {
            let last = *out.last().expect("out is non-empty");
            if last.0 == t {
                out.last_mut().expect("out is non-empty").1 = v;
                continue;
            }
            if last.1 == v {
                continue;
            }
        }
        out.push((t, v));
    }
    out
}

/// The step timeline of how many of `segments` are simultaneously held,
/// as `(absolute time, count)` transitions.
///
/// A sweep over `+1` at each segment start and `-1` at each end. Equal
/// timestamps are collapsed into ONE transition carrying the count AFTER
/// every change at that instant, which is what `FuseSegments`
/// (`GW2EIEvtcParser/EIData/MathUtils/StateGraph.cs:24-28`) produces for
/// coincident segment boundaries, and consecutive equal counts are dropped
/// for the same reason. Zero-length segments therefore vanish entirely,
/// exactly as they do in GW2EI.
pub(crate) fn overlap_steps(segments: &[HeldSegment]) -> Vec<(u64, u32)> {
    let mut deltas: Vec<(u64, i32)> = Vec::with_capacity(segments.len() * 2);
    for s in segments {
        if s.end <= s.start {
            continue;
        }
        deltas.push((s.start, 1));
        deltas.push((s.end, -1));
    }
    deltas.sort_unstable();

    let mut out: Vec<(u64, u32)> = Vec::new();
    let mut count: i32 = 0;
    let mut i = 0usize;
    while i < deltas.len() {
        let t = deltas[i].0;
        while i < deltas.len() && deltas[i].0 == t {
            count += deltas[i].1;
            i += 1;
        }
        let value = count.max(0) as u32;
        if out.last().map(|&(_, v)| v) != Some(value) {
            out.push((t, value));
        }
    }
    out
}

/// Build both timelines for every tracked (player, boon).
///
/// `boons` is `analyze()`'s already-computed stack-count timeline map
/// (`Metrics::boons`) -- reused rather than recomputed, so `states` is a
/// pure reshaping of the numbers `buffUptimes[].uptime`/`presence` are
/// already derived from. `per_source` needs the source-tracking simulation,
/// which `analyze()` keeps only in summed form, so it is re-run here (see
/// `generation::simulate_boon_held_segments`'s doc comment).
pub fn build(
    raw: &RawLog,
    enc: &Encounter,
    boons: &BTreeMap<(u64, u32), BoonTimeline>,
) -> BoonStates {
    let log_start = raw.events.first().map(|e| e.time).unwrap_or(0);
    let name_of: BTreeMap<u64, &str> =
        enc.players.iter().map(|p| (p.agent_addr, p.character.as_str())).collect();

    let is_intensity = |buff_id: u32| {
        BOON_IDS.iter().any(|&(id, _, intensity)| id == buff_id && intensity)
    };

    let total: BTreeMap<(u64, u32), StateTimeline> = boons
        .iter()
        .map(|(&key, tl)| {
            let clamp = !is_intensity(key.1);
            let steps = tl
                .states
                .iter()
                .map(move |&(t, v)| (t, if clamp { v.min(1) } else { v }));
            (key, to_ei_states(steps, log_start))
        })
        .collect();

    let mut per_source: PerSourceTimelines = BTreeMap::new();
    for (key, segments) in generation::simulate_boon_held_segments(raw, enc) {
        let mut by_source: BTreeMap<u64, Vec<HeldSegment>> = BTreeMap::new();
        for s in segments {
            by_source.entry(s.source).or_default().push(s);
        }
        let mut named: BTreeMap<String, StateTimeline> = BTreeMap::new();
        for (source, segs) in by_source {
            let steps = overlap_steps(&segs);
            if steps.is_empty() {
                continue;
            }
            let name = name_of.get(&source).copied().unwrap_or(UNKNOWN_SOURCE).to_string();
            // Several unresolved sources collapse onto the one `UNKNOWN`
            // key, exactly as they do in GW2EI (whose dictionary is keyed
            // by `Character` too); merge rather than overwrite.
            match named.entry(name) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(to_ei_states(steps.into_iter(), log_start));
                }
                std::collections::btree_map::Entry::Occupied(mut o) => {
                    let merged = merge_step_timelines(o.get(), &to_ei_states(steps.into_iter(), log_start));
                    o.insert(merged);
                }
            }
        }
        if !named.is_empty() {
            per_source.insert(key, named);
        }
    }

    BoonStates { total, per_source }
}

/// Pointwise sum of two already-relative step timelines (both start with
/// the mandatory `[0, 0]`), used only to collapse several unresolved
/// sources onto the single [`UNKNOWN_SOURCE`] key.
fn merge_step_timelines(a: &[(u64, u32)], b: &[(u64, u32)]) -> Vec<(u64, u32)> {
    let mut times: Vec<u64> = a.iter().chain(b).map(|&(t, _)| t).collect();
    times.sort_unstable();
    times.dedup();
    let value_at = |tl: &[(u64, u32)], t: u64| -> u32 {
        match tl.binary_search_by(|probe| probe.0.cmp(&t)) {
            Ok(i) => tl[i].1,
            Err(0) => 0,
            Err(i) => tl[i - 1].1,
        }
    };
    let mut out: Vec<(u64, u32)> = Vec::with_capacity(times.len());
    for t in times {
        let v = value_at(a, t) + value_at(b, t);
        if out.last().map(|&(_, x)| x) != Some(v) {
            out.push((t, v));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_states_are_log_relative_and_lead_with_a_zero_pair() {
        let tl = BoonTimeline { states: vec![(1_000, 1), (3_000, 0)] };
        let got = to_ei_states(tl.states.iter().copied(), 1_000);
        assert_eq!(got, vec![(0, 0), (0, 1), (2_000, 0)]);
    }

    /// GW2EI serializes a FUSED graph: never two pairs at one timestamp,
    /// never two consecutive pairs with the same value.
    #[test]
    fn coincident_and_repeated_transitions_are_fused() {
        let steps = [(0u64, 3u32), (100, 2), (100, 1), (100, 0), (200, 0), (300, 4)];
        assert_eq!(
            to_ei_states(steps.into_iter(), 0),
            vec![(0, 0), (0, 3), (100, 0), (300, 4)]
        );
    }

    /// A buff already up at log start still gets the leading `[0, 0]` --
    /// the reference export has 7 such entries (see this module's doc).
    #[test]
    fn a_transition_before_log_start_clamps_to_zero_after_the_leading_pair() {
        let got = to_ei_states([(500u64, 3u32)].into_iter(), 1_000);
        assert_eq!(got, vec![(0, 0), (0, 3)]);
    }

    #[test]
    fn overlap_counts_concurrent_stacks_and_collapses_coincident_boundaries() {
        let segs = vec![
            HeldSegment { source: 7, start: 0, end: 100 },
            HeldSegment { source: 7, start: 50, end: 150 },
            // Zero-length: must vanish, as it does in GW2EI's fused graph.
            HeldSegment { source: 7, start: 80, end: 80 },
        ];
        assert_eq!(overlap_steps(&segs), vec![(0, 1), (50, 2), (100, 1), (150, 0)]);
    }

    /// One stack ending exactly where the next begins is a single fused
    /// run at count 1, not a 1 -> 0 -> 1 flicker.
    #[test]
    fn back_to_back_segments_do_not_flicker() {
        let segs = vec![
            HeldSegment { source: 7, start: 0, end: 100 },
            HeldSegment { source: 7, start: 100, end: 200 },
        ];
        assert_eq!(overlap_steps(&segs), vec![(0, 1), (200, 0)]);
    }

    #[test]
    fn unknown_sources_merge_rather_than_overwrite() {
        let a = vec![(0u64, 0u32), (10, 1), (20, 0)];
        let b = vec![(0u64, 0u32), (15, 1), (30, 0)];
        assert_eq!(
            merge_step_timelines(&a, &b),
            vec![(0, 0), (10, 1), (15, 2), (20, 1), (30, 0)]
        );
    }
}
