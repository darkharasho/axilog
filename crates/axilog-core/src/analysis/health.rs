//! Per-agent health-percent tracking (M11 Task 1): decodes
//! `CBTS_HEALTHPCTUPDATE` (`sc::HEALTH_UPDATE`) statechanges into a
//! step-series per raw agent addr, and answers the "over-99 anchor" query
//! the M11 contribution-family methodology needs
//! (`docs/superpowers/specs/2026-08-09-m11-contribution-family-design.md`):
//! window `from max(last_time_target_was_above_99%_health − 2000ms,
//! log_start) to the down`.
//!
//! ## Ordinal + payload verification (project law: curl + hand-count the
//! arcdps README enum, never trust memory or a third-party writeup)
//!
//! `curl https://www.deltaconnected.com/arcdps/evtc/README.txt`
//! (2026-08-09), hand-counting `enum cbtstatechange` entries from
//! `CBTS_COMBAT = 0`: `CBTS_HEALTHPCTUPDATE` is the 9th entry (index 8),
//! immediately after `CBTS_SPAWN` (6)/`CBTS_DESPAWN` (7) and before
//! `CBTS_SQCOMBATSTART` (9) -- this project's OWN `sc::LOG_START` (9,
//! already independently verified) and `sc::LOG_END`/`CBTS_SQCOMBATEND`
//! (10), and further out `sc::MAX_HEALTH`/`CBTS_MAXHEALTHUPDATE` (12) and
//! `sc::POINT_OF_VIEW`/`CBTS_POINTOFVIEW` (13) -- both already-verified
//! constants in this same file -- land exactly 3/4/5 entries later in the
//! same hand-count, a strong internal cross-check. Cross-checked against
//! GW2EI's `ArcDPSEnums.StateChange.HealthUpdate = 8`
//! (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:268`). Exhaustively
//! checked GW2EI's `ArcDPSBuilds` table for any version gate on this
//! statechange (the way `BuffAppliesAndRemovesAsStateChanges`/
//! `AnimationAsStateChanges` gate the buff-apply/cast-start pairs this
//! project's `evtc::header::is_post_buff_rework` already handles) -- there
//! is none. This is one stable wire shape across every log era this
//! project supports; `HealthTracker::build` does not era-dispatch.
//!
//! Payload, per the arcdps reference (`CBTS_HEALTHPCTUPDATE` block):
//! ```text
//! CBTS_HEALTHPCTUPDATE, // agent health percentage changed
//! // src_agent: relates to agent
//! // dst_agent: percent * 10000 eg. 99.5% will be 9950
//! ```
//! Read literally, the comment text is self-contradictory (`* 10000` would
//! make 99.5% become 995000, not 9950) -- GW2EI's `HealthUpdateEvent` is the
//! arbiter for the real scale: `HealthPercent = Math.Round(DstAgent / 100.0,
//! 2)` (`HealthUpdateEvent.cs`), i.e. `dst_agent` is percent times 100
//! (`9950 / 100.0 = 99.5`), which matches the reference's own worked example
//! exactly even though its prose doesn't. `EvtcParser.cs`'s own combat-event
//! filter independently confirms the same scale in its own words: "DstAgent
//! should be target health % times 100, values higher than 10000 are
//! unlikely" -- and drops any row whose decoded percent exceeds 200 as a
//! garbage read (`if (IsStateChange == HealthUpdate &&
//! GetHealthPercent(combatItem) > 200) { ignore }`, `EvtcParser.cs:935`).
//! `HealthUpdateEvent`'s own ctor additionally clamps any decoded percent
//! over 100.0 down to exactly 100.0 before storing it. [`HealthTracker::
//! build`] mirrors both: rows decoding to a percent `> 200.0` are dropped
//! entirely (never a real health value), and survivors are clamped to
//! `[0.0, 100.0]`.
//!
//! `src_agent` is the agent whose health this reading is FOR: GW2EI's
//! shared `StatusEvent` base ctor (`HealthUpdateEvent`'s only ctor param
//! beyond the raw `CombatItem`) reads `Src = agentData.GetAgent(evtcItem
//! .SrcAgent, evtcItem.Time)` unconditionally. No pet/minion-owner (
//! `src_master_instid`) folding applies here -- a pet has its own health bar
//! and its own agent addr, so this module intentionally stays keyed by RAW
//! agent addr, not account/ultimate-master representative (see "Design"
//! below and the M11 Task 1 brief: "keep it pure -- no down-reset logic
//! here").
//!
//! ## Design: standalone, not threaded through `Metrics`
//!
//! Like [`crate::analysis::replay::build_replay`] and
//! [`crate::analysis::missiles::build_missiles`] (both standalone from
//! [`crate::analysis::analyze`]), and like `damage::InstidRegistry` (rebuilt
//! fresh by each of `downs`/`cc`/`healing` rather than computed once and
//! threaded through `Metrics`), [`HealthTracker::build`] is a standalone
//! pure function over `&RawLog` -- it is NOT wired into `analyze`/`Metrics`.
//! M11 Task 2's contribution engine (`analysis::contribution`, not yet
//! written as of this task) builds its own `HealthTracker` directly from the
//! same `raw: &RawLog` it already holds, exactly as `downs`/`cc` already
//! build their own `InstidRegistry` on demand. This keeps `health.rs`
//! reusable without growing `Metrics`'s surface for a value this task's own
//! brief says NOT to expose in the native schema yet ("do NOT add schema
//! output this task").
//!
//! ## Time domain
//!
//! `t_ms` is the RAW, absolute `RawEvent::time` (arcdps's own
//! recording-relative millisecond clock) -- NOT re-based to `event.time -
//! t0` the way `analysis::replay`/`analysis::buffs` do for schema/HTML
//! output. This matches `analysis::downs`/`analysis::cc`'s existing
//! convention (their down/CC window math -- e.g. `downs::WINDOW_MS` --
//! operates on raw `RawEvent::time` values directly), which is what M11
//! Task 2's window math (anchor − 2000ms, down_time + 2100ms) needs to stay
//! consistent with. "Log start" (the methodology's `max(..., log_start)`
//! clamp) is an `Encounter`/`RawLog`-level concept this pure module
//! deliberately does not know about -- [`HealthTracker::
//! last_time_at_or_above`] returns `None` when no qualifying step exists,
//! and documents that the caller (M11 Task 2) is the one that applies the
//! log-start default.

use crate::evtc::{sc, RawLog};
use std::collections::BTreeMap;

/// One health-percent step: at `t_ms` (raw `RawEvent::time`), the agent's
/// health became `percent` (already clamped to `[0.0, 100.0]` -- see module
/// doc). Holds until the next step (or indefinitely if it's the last one) --
/// a step function, matching how arcdps itself only emits a row on change,
/// not a continuous curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthStep {
    pub t_ms: u64,
    pub percent: f32,
}

/// Per-agent (raw addr -- no relog/account or pet/owner folding; see module
/// doc) health step-series, decoded from `sc::HEALTH_UPDATE` rows.
#[derive(Debug, Clone, Default)]
pub struct HealthTracker {
    by_agent: BTreeMap<u64, Vec<HealthStep>>,
}

impl HealthTracker {
    /// Build from a decoded log. Per-agent steps are kept sorted ascending
    /// by `t_ms` via a sorted insert -- defensive: real captures are already
    /// chronological (matching every other pass in this codebase), but this
    /// doesn't assume it, mirroring `damage::InstidRegistry::build`'s
    /// same not-necessarily-in-order robustness. Rows decoding to an
    /// implausible percent (`> 200.0`, arcdps's/GW2EI's own garbage-read
    /// threshold) are dropped entirely rather than clamped -- see module
    /// doc.
    pub fn build(raw: &RawLog) -> Self {
        let mut by_agent: BTreeMap<u64, Vec<HealthStep>> = BTreeMap::new();
        for e in &raw.events {
            if e.is_statechange != sc::HEALTH_UPDATE {
                continue;
            }
            // `dst_agent` carries the encoded percent (percent * 100); see
            // module doc for the full citation. This project's `RawEvent::
            // dst_agent` is `u64` (the generic field decode), but the real
            // wire-level values this statechange emits are always small
            // (well within `u32`) -- the `> 200.0` filter below catches any
            // garbage/oversized read regardless.
            let raw_pct = e.dst_agent as f64 / 100.0;
            if raw_pct > 200.0 {
                continue; // garbage read, per GW2EI's own EvtcParser filter
            }
            let percent = raw_pct.clamp(0.0, 100.0) as f32;
            let steps = by_agent.entry(e.src_agent).or_default();
            let pos = steps.partition_point(|s| s.t_ms <= e.time);
            steps.insert(pos, HealthStep { t_ms: e.time, percent });
        }
        HealthTracker { by_agent }
    }

    /// The step-function health percent effective at time `t` for `agent`
    /// (the latest step with `t_ms <= t`). `None` if the agent has no health
    /// data at all, or `t` is strictly before that agent's first recorded
    /// step.
    pub fn percent_at(&self, agent: u64, t: u64) -> Option<f32> {
        let steps = self.by_agent.get(&agent)?;
        let idx = steps.partition_point(|s| s.t_ms <= t);
        if idx == 0 {
            return None;
        }
        Some(steps[idx - 1].percent)
    }

    /// The M11 over-99 anchor query: the latest step time `<= before_t`
    /// whose percent is `>= threshold_pct` (an exact-threshold percent
    /// counts as a hit -- `>=`, not `>`). `None` when the agent has no
    /// health data at all, OR no qualifying step exists at/before
    /// `before_t` (e.g. `before_t` precedes every recorded step for this
    /// agent, or every step at/before it reads below `threshold_pct`).
    ///
    /// The M11 Task 2 contribution engine treats `None` as "log start" per
    /// the methodology's `max(anchor − 2000ms, log_start)` clamp -- this
    /// pure module does not know what "log start" is (see module doc's
    /// "Time domain" section).
    pub fn last_time_at_or_above(
        &self,
        agent: u64,
        threshold_pct: f32,
        before_t: u64,
    ) -> Option<u64> {
        let steps = self.by_agent.get(&agent)?;
        let upper = steps.partition_point(|s| s.t_ms <= before_t);
        steps[..upper]
            .iter()
            .rev()
            .find(|s| s.percent >= threshold_pct)
            .map(|s| s.t_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{decode_events, RawEvent, RawHeader};

    fn health_event(time: u64, src: u64, dst_pct_encoded: u64) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: dst_pct_encoded,
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
            is_ninety: 0, is_moving: 0,
            is_statechange: sc::HEALTH_UPDATE,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    /// Byte-level decode probe: a `CBTS_HEALTHPCTUPDATE` row's `src_agent`
    /// (offset 8) and `dst_agent` (offset 16, the encoded percent) must
    /// decode from the RIGHT offsets, not a neighbor -- guards against a
    /// future struct-layout regression silently reading the wrong bytes
    /// (the same style of probe `event.rs::tests::decodes_strike` already
    /// runs for the generic combat-event fields).
    #[test]
    fn decodes_health_update_from_correct_offsets() {
        let mut b = vec![0u8; crate::evtc::EVENT_SIZE_REV1];
        b[0..8].copy_from_slice(&12345u64.to_le_bytes()); // time
        b[8..16].copy_from_slice(&0xAAAAu64.to_le_bytes()); // src_agent
        b[16..24].copy_from_slice(&9950u64.to_le_bytes()); // dst_agent: 99.5%
        b[56] = sc::HEALTH_UPDATE; // is_statechange
        let ev = decode_events(&b, 1).unwrap();
        let e = &ev[0];
        assert_eq!(e.time, 12345);
        assert_eq!(e.src_agent, 0xAAAA, "src_agent must decode from offset 8, not a neighbor");
        assert_eq!(e.dst_agent, 9950, "dst_agent (encoded percent) must decode from offset 16, not a neighbor");
        assert_eq!(e.is_statechange, sc::HEALTH_UPDATE);
    }

    /// Scale verification: `dst_agent / 100.0` is the correct decode (per
    /// the arcdps reference's own worked example, 9950 -> 99.5%, and GW2EI's
    /// `HealthUpdateEvent.GetHealthPercent`) -- a wrong-scale decode (e.g.
    /// `/ 10000.0`, taking the reference's literal but self-contradictory
    /// prose at face value) would produce 0.995%, not 99.5%. This test fails
    /// under either wrong-offset OR wrong-scale regressions.
    #[test]
    fn decode_scale_is_percent_times_100_not_times_10000() {
        let raw = raw_from(vec![health_event(0, 1, 9950)]);
        let tracker = HealthTracker::build(&raw);
        let pct = tracker.percent_at(1, 0).expect("health data for agent 1");
        assert!((pct - 99.5).abs() < 0.01, "expected 99.5%, got {pct} (wrong-scale regression?)");
    }

    /// A garbage read (encoded percent > 200%, i.e. `dst_agent > 20000`) is
    /// dropped entirely, per GW2EI's own `EvtcParser` filter -- the agent
    /// ends up with NO health data at all (not a clamped-but-wrong 100%).
    #[test]
    fn garbage_read_above_200_percent_is_dropped() {
        let raw = raw_from(vec![health_event(0, 1, 20050)]); // 200.5%
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.percent_at(1, 0), None, "a >200% garbage row must be dropped, not stored");
    }

    /// A percent between 100% and 200% (not garbage, but over the real max)
    /// is clamped to exactly 100.0, per GW2EI's `HealthUpdateEvent` ctor.
    #[test]
    fn percent_over_100_is_clamped_to_100() {
        let raw = raw_from(vec![health_event(0, 1, 10050)]); // 100.5%
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.percent_at(1, 0), Some(100.0));
    }

    /// Step-series construction: out-of-order input events (not
    /// chronological -- defensive per `InstidRegistry::build`'s own
    /// robustness) must still produce an ascending-by-time series, so
    /// `percent_at`'s step-function lookup is correct regardless of input
    /// order.
    #[test]
    fn step_series_sorts_out_of_order_events() {
        let raw = raw_from(vec![
            health_event(2000, 1, 8000), // 80% at t=2000
            health_event(0, 1, 10000),   // 100% at t=0 (arrives second, but earlier time)
            health_event(1000, 1, 9000), // 90% at t=1000
        ]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.percent_at(1, 0), Some(100.0));
        assert_eq!(tracker.percent_at(1, 500), Some(100.0), "holds until next step");
        assert_eq!(tracker.percent_at(1, 1000), Some(90.0));
        assert_eq!(tracker.percent_at(1, 1999), Some(90.0));
        assert_eq!(tracker.percent_at(1, 2000), Some(80.0));
        assert_eq!(tracker.percent_at(1, 5000), Some(80.0), "holds indefinitely after the last step");
    }

    #[test]
    fn percent_at_before_first_step_is_none() {
        let raw = raw_from(vec![health_event(1000, 1, 10000)]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.percent_at(1, 999), None);
    }

    #[test]
    fn percent_at_unknown_agent_is_none() {
        let raw = raw_from(vec![health_event(1000, 1, 10000)]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.percent_at(2, 5000), None);
    }

    /// `last_time_at_or_above`: an exact-threshold percent counts as a hit
    /// (`>=`, not `>`).
    #[test]
    fn last_time_at_or_above_exact_threshold_hits() {
        let raw = raw_from(vec![health_event(1000, 1, 9900)]); // exactly 99.0%
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 5000), Some(1000));
    }

    /// `before_t` strictly precedes every recorded step for the agent ->
    /// `None` (no qualifying step exists yet).
    #[test]
    fn last_time_at_or_above_before_first_event_is_none() {
        let raw = raw_from(vec![health_event(1000, 1, 10000)]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 500), None);
    }

    /// An agent with zero health data at all -> `None`.
    #[test]
    fn last_time_at_or_above_unknown_agent_is_none() {
        let raw = raw_from(vec![health_event(1000, 1, 10000)]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.last_time_at_or_above(2, 99.0, 5000), None);
    }

    /// Multiple over-99/under-99 crossings: must return the LATEST
    /// qualifying time, not the first, and must correctly skip over a
    /// disqualifying dip that happens to be more recent than an earlier
    /// qualifying step but still below threshold.
    #[test]
    fn last_time_at_or_above_returns_latest_crossing_not_first() {
        let raw = raw_from(vec![
            health_event(0, 1, 10000),    // 100% at t=0 -- qualifies
            health_event(1000, 1, 5000),  // dips to 50% at t=1000 -- disqualifies
            health_event(2000, 1, 9950),  // back to 99.5% at t=2000 -- qualifies (latest)
            health_event(3000, 1, 3000),  // dips to 30% at t=3000 -- disqualifies again
        ]);
        let tracker = HealthTracker::build(&raw);
        // Querying before the final dip: latest qualifying step is t=2000,
        // not t=0.
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 2500), Some(2000));
        // Querying after the final dip (still below threshold at t=3000):
        // still resolves to the same t=2000 qualifying step, not None --
        // the most recent step being a disqualifying one doesn't erase an
        // earlier qualifying one within the window.
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 4000), Some(2000));
        // Querying exactly at the first dip (t=1000, itself disqualifying):
        // only the t=0 step qualifies at/before this time.
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 1000), Some(0));
    }

    /// Multiple agents tracked independently -- one agent's crossings must
    /// not affect another's query.
    #[test]
    fn tracks_multiple_agents_independently() {
        let raw = raw_from(vec![
            health_event(0, 1, 10000), // agent 1: 100%
            health_event(0, 2, 5000),  // agent 2: 50%
        ]);
        let tracker = HealthTracker::build(&raw);
        assert_eq!(tracker.last_time_at_or_above(1, 99.0, 1000), Some(0));
        assert_eq!(tracker.last_time_at_or_above(2, 99.0, 1000), None);
    }
}
