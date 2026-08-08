//! Per-(agent, boon) uptime aggregation from a `BoonTimeline` (M3, Task 2).
//!
//! Turns the Task 1 stack-count step timeline into the two summary numbers
//! GW2EI's own `buffUptimes[].buffData[0]` JSON exposes per player per
//! tracked boon: `presence_pct` (% of the fight with >=1 held stack) and
//! `avg_stacks` (time-weighted mean stack count over the fight).
//!
//! **EI field-meaning mapping (verified against GW2EI source, not guessed —
//! see citations below and the `fixtures/wvw-small.ei.json` `_note`, which
//! documents the same mapping from the calibration-data side with a live
//! numeric example)**: GW2EI's `BuffStatistics` (`GW2EIEvtcParser/EIData/
//! Statistics/BuffStatistics.cs`, `GetBuffsForSelf`) computes two different
//! things into the SAME `Uptime`/`Presence` fields depending on `Buff.Type`:
//!
//! - **Duration-type** boons (everything here except Might/Stability --
//!   `BuffType.Duration`): `uptime.Uptime = 100 * uptimeValue /
//!   phaseDuration` where `uptimeValue` is the buff DISTRIBUTION's summed
//!   held-time (`BuffDistribution.GetUptime`, `EIData/Buffs/
//!   BuffDistribution.cs`) -- i.e. literally the percentage of the phase
//!   with the buff active. `uptime.Presence` is NEVER set for this branch
//!   (stays at its zero default) -- so for duration boons, EI's `uptime`
//!   field IS our `presence_pct`, and EI's `presence` field is meaningless
//!   (always 0, confirmed empirically against every duration-boon row in
//!   `fixtures/wvw-small.ei.json`).
//! - **Intensity-type** boons (Might, Stability -- `BuffType.Intensity`):
//!   `uptime.Uptime = uptimeValue / phaseDuration` (note: NO `* 100` here,
//!   unlike the duration branch) -- `uptimeValue` here is a time-INTEGRAL
//!   of stack count (stack-ms), so dividing by `phaseDuration` (ms) yields
//!   a genuine time-weighted average stack count, i.e. our `avg_stacks`.
//!   `uptime.Presence = 100 * presenceValueBoon / phaseDuration`, computed
//!   from a separate boolean (`stacks > 0`) time-integral
//!   (`SingleActor.GetBuffPresence`) -- our `presence_pct`.
//!
//! `phaseDuration = end - start` uses the PHASE's absolute window (here:
//! the whole log, `log.LogData.LogStart`..`LogEnd`), NOT clamped to the
//! player's own active/aware time -- that per-player-active-time clamp only
//! applies to the SEPARATE `buffUptimesActive[]` JSON array (same source
//! file's `uptimeActive` local, divided by `playerActiveDuration` instead),
//! which this task does not consume. So the denominator here is simply the
//! fight's absolute `[start_ms, end_ms)` window -- matching
//! `simulate_boons`'s own `log_end_ms` derivation
//! (`raw.events.last().time`), not `Encounter::duration_ms` (which is
//! START-RELATIVE: `last.time - first.time` -- see `model::resolve`). Using
//! the SAME absolute window `simulate_boons` already ticks its timelines
//! against keeps `compute`'s denominator consistent with the timeline it's
//! summarizing without re-deriving or re-exposing a relative-vs-absolute
//! conversion here.

use super::BoonTimeline;

/// One boon's fight-long uptime summary for one player, using EI's own
/// per-stack-type field semantics (see module docs): `presence_pct` is
/// always "% of the fight with >=1 held stack" (0-100); `avg_stacks` is
/// always "time-weighted mean held-stack count" (0 for duration-type boons,
/// since EI never reports intensity for them -- callers of `compute` should
/// read `presence_pct` for duration boons and `avg_stacks` for intensity
/// boons, mirroring which field EI itself populates for that boon type).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoonUptime {
    pub presence_pct: f64,
    pub avg_stacks: f64,
}

/// Integrates a `BoonTimeline`'s step function over `[start_ms, end_ms)`
/// into `BoonUptime`. `start_ms`/`end_ms` are on the same absolute
/// `RawEvent::time` timescale `BoonTimeline::states` already uses (see
/// module docs) -- callers pass `simulate_boons`'s own window
/// (`raw.events.first().time`, `raw.events.last().time`), not
/// `Encounter::duration_ms`.
///
/// A step at `states[i] = (t, c)` holds count `c` until `states[i+1].0` (or
/// `end_ms` for the last entry); no entry before the first one, or before
/// `start_ms`, means count 0 (matches `BoonTimeline`'s own doc comment).
/// Entries at or after `end_ms` are ignored (mirrors GW2EI's `Trim`-then-
/// integrate-over-the-phase-window approach -- the simulator already stops
/// producing new transitions past `log_end_ms` per Task 1, so this is
/// belt-and-braces rather than load-bearing for the real fixture).
pub fn compute(tl: &BoonTimeline, start_ms: u64, end_ms: u64) -> BoonUptime {
    if end_ms <= start_ms {
        return BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 };
    }
    let window_ms = (end_ms - start_ms) as f64;
    let mut present_ms: u128 = 0;
    let mut stack_ms: u128 = 0;

    let mut iter = tl.states.iter().peekable();
    while let Some(&(t, c)) = iter.next() {
        let seg_start = t.max(start_ms);
        let seg_end = iter.peek().map(|&&(next_t, _)| next_t).unwrap_or(end_ms).min(end_ms);
        if seg_end <= seg_start || t >= end_ms {
            continue;
        }
        let dt = (seg_end - seg_start) as u128;
        if c > 0 {
            present_ms += dt;
        }
        stack_ms += dt * c as u128;
    }

    BoonUptime {
        presence_pct: 100.0 * present_ms as f64 / window_ms,
        avg_stacks: stack_ms as f64 / window_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(states: Vec<(u64, u32)>) -> BoonTimeline {
        BoonTimeline { states }
    }

    #[test]
    fn empty_timeline_is_zero() {
        let u = compute(&tl(vec![]), 0, 10_000);
        assert_eq!(u, BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 });
    }

    #[test]
    fn full_uptime_single_stack_whole_fight() {
        // Held at count 1 for the entire [0, 10_000) window: 100% presence,
        // 1.0 average stacks.
        let u = compute(&tl(vec![(0, 1)]), 0, 10_000);
        assert_eq!(u.presence_pct, 100.0);
        assert_eq!(u.avg_stacks, 1.0);
    }

    #[test]
    fn half_uptime_half_the_window() {
        // 0->1 for [0,5000), then 1->0 for [5000,10000): 50% presence.
        let u = compute(&tl(vec![(0, 1), (5000, 0)]), 0, 10_000);
        assert_eq!(u.presence_pct, 50.0);
        assert_eq!(u.avg_stacks, 0.5);
    }

    #[test]
    fn intensity_avg_stacks_time_weighted() {
        // count=2 for [0,2500) then count=4 for [2500,10000): time-weighted
        // mean = (2500*2 + 7500*4) / 10000 = 3.5. Presence is 100% the
        // whole time (never drops to 0).
        let u = compute(&tl(vec![(0, 2), (2500, 4)]), 0, 10_000);
        assert_eq!(u.presence_pct, 100.0);
        assert_eq!(u.avg_stacks, 3.5);
    }

    #[test]
    fn states_before_start_ms_are_clamped_into_the_window() {
        // A stack held since before the fight window (t=0 state) but the
        // caller's window starts at 1000 -- the whole [1000, 10000) window
        // should count as present, not just [0,10000).
        let u = compute(&tl(vec![(0, 1)]), 1000, 10_000);
        assert_eq!(u.presence_pct, 100.0);
    }

    #[test]
    fn states_at_or_after_end_ms_are_ignored() {
        // A transition that happens to land exactly at (or after) the
        // window's end must not contribute -- only [0, 10000) counts.
        let u = compute(&tl(vec![(0, 1), (10_000, 5), (12_000, 0)]), 0, 10_000);
        assert_eq!(u.presence_pct, 100.0);
        assert_eq!(u.avg_stacks, 1.0);
    }

    #[test]
    fn gap_before_first_state_counts_as_zero() {
        // No entry until t=3000 -- [0,3000) is implicitly 0 stacks.
        let u = compute(&tl(vec![(3000, 1)]), 0, 10_000);
        assert_eq!(u.presence_pct, 70.0);
        assert_eq!(u.avg_stacks, 0.7);
    }
}
