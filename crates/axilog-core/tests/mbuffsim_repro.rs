//! MBUFFSIM Task 1 — synthetic reproductions of the two diagnosed rules.
//!
//! **These tests deliberately assert the CURRENT (wrong) behaviour.**
//! `MBUFFSIM Task 2 flips this` marks every assertion Task 2 must invert;
//! each one carries the GW2EI rule it violates with a `file:line` citation,
//! so the flip is mechanical.
//!
//! Both rules live in the buff-event PIPELINE (`analysis::buffs::events`),
//! not in `analysis::buffs::simulator`. Task 1 established that by porting
//! GW2EI's whole `BuffSimulatorNoID` family statement-for-statement
//! (`tests/common/eiref.rs`) and finding it reproduces this project's
//! current numbers to five decimals on all 14 diagnosed buffs — the
//! stacking logic is already faithful; the EVENT LIST fed to it is not. See
//! `.superpowers/sdd/2026-08-09-mbuffsim/task-1-report.md`.

use axilog_core::analysis::buffs::events::{
    extract_buff_events_with_registry, BuffEventKind,
};
use axilog_core::analysis::buffs::simulator;
use axilog_core::analysis::damage::InstidRegistry;
use axilog_core::evtc::{buff_remove, sc, RawEvent, RawHeader, RawLog};
use std::collections::BTreeSet;

/// `ArcDPSEnums.IFF.Unknown` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:618-624`).
const IFF_UNKNOWN: u8 = 2;
const IFF_FRIEND: u8 = 0;

const MIGHT: u32 = 740;
const STABILITY: u32 = 1122;
/// Relic of Fireworks — `BuffStackType.Force`, capacity 1.
const FIREWORKS: u32 = 69855;

fn ev(time: u64, is_statechange: u8) -> RawEvent {
    RawEvent {
        time,
        src_agent: 0,
        dst_agent: 0,
        value: 0,
        buff_dmg: 0,
        overstack: 0,
        skillid: 0,
        src_instid: 0,
        dst_instid: 0,
        src_master_instid: 0,
        dst_master_instid: 0,
        iff: IFF_FRIEND,
        buff: 1,
        result: 0,
        is_activation: 0,
        is_buffremove: 0,
        is_ninety: 0,
        is_fifty: 0,
        is_moving: 0,
        is_statechange,
        is_flanking: 0,
        is_shields: 0,
        is_offcycle: 0,
        pad: 0,
    }
}

/// Post-era (`>= 20260501`) `CBTS_BUFFAPPLY`: owner = `dst_agent`.
fn apply(time: u64, buff: u32, owner: u64, applier: u64, duration: i32, inst: u32) -> RawEvent {
    RawEvent {
        skillid: buff,
        src_agent: applier,
        dst_agent: owner,
        value: duration,
        is_shields: 1,
        pad: inst,
        ..ev(time, sc::BUFF_APPLY)
    }
}

/// Post-era `CBTS_BUFFREMOVE_SINGLE`: owner = `src_agent`, remover =
/// `dst_agent`. `dst_agent == 0` + `iff == Unknown` is GW2EI's
/// `OverstackOrNaturalEnd`.
fn remove_single(
    time: u64,
    buff: u32,
    owner: u64,
    remover: u64,
    removed_duration: i32,
    iff: u8,
    inst: u32,
) -> RawEvent {
    RawEvent {
        skillid: buff,
        src_agent: owner,
        dst_agent: remover,
        value: removed_duration,
        is_buffremove: buff_remove::SINGLE,
        iff,
        pad: inst,
        ..ev(time, sc::BUFF_REMOVE_SINGLE)
    }
}

fn raw(events: Vec<RawEvent>) -> RawLog {
    RawLog {
        header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
        agents: vec![],
        skills: vec![],
        events,
        guid_map: vec![],
    }
}

fn extract(log: &RawLog, buff: u32) -> Vec<axilog_core::analysis::buffs::events::BuffEvent> {
    let ids: BTreeSet<u32> = [buff].into_iter().collect();
    let registry = InstidRegistry::build(log);
    extract_buff_events_with_registry(log, &registry, &ids)
}

// ---------------------------------------------------------------------
// Rule 1 — `BuffRemoveSingleEvent.OverstackOrNaturalEnd`
//
//   GW2EIEvtcParser/ParsedData/CombatEvents/BuffEvents/BuffRemoves/
//   BuffRemoveSingleEvent.cs:11
//     internal bool OverstackOrNaturalEnd =>
//         (IFF == IFF.Unknown && CreditedBy.IsUnknown && !_byShouldntBeUnknown);
//     // ctor:26  _byShouldntBeUnknown = evtcItem.DstAgent != 0;
//   BuffRemoveSingleEvent.cs:26-38
//     IsBuffSimulatorCompliant(false) => !OverstackOrNaturalEnd
//   GW2EIEvtcParser/EIData/Buffs/BuffDictionary.cs:83-86
//     if (!buffEvent.IsBuffSimulatorCompliant(...)) { return; }   // never
//                                                                 // reaches
//                                                                 // the sim
//
// i.e. a SINGLE removal with `dst_agent == 0` AND `iff == Unknown` is arcdps
// REPORTING a stack that ended on its own (natural expiry) or was
// overstacked. The simulator already models that expiry from the apply's
// duration, so replaying the event double-counts it and strips a stack that
// should still be held.
//
// In the post-era WvW reference capture, 51863 of 52116 (99.5%) SINGLE
// removals over the 14 diagnosed buffs are of this kind.
// ---------------------------------------------------------------------

/// A natural-expiry SINGLE removal is DROPPED before it reaches the
/// simulator (MBUFFSIM Task 2, rule 1 — flipped).
#[test]
fn overstack_or_natural_end_removal_is_dropped() {
    let log = raw(vec![
        apply(0, FIREWORKS, 1, 9, 6000, 7),
        // arcdps's "this stack was overstacked" notification: no remover
        // agent, IFF unknown, and `value` = the duration that was dropped.
        remove_single(0, FIREWORKS, 1, 0, 6000, IFF_UNKNOWN, 7),
    ]);
    let evs = extract(&log, FIREWORKS);
    assert_eq!(
        evs.len(),
        1,
        "GW2EI drops OverstackOrNaturalEnd removals before they reach the simulator \
         (BuffRemoveSingleEvent.cs:11,26-38 + BuffDictionary.cs:83-86): {evs:?}"
    );
    assert!(matches!(evs[0].kind, BuffEventKind::Apply { duration_ms: 6000, .. }));
}

/// The behavioural consequence for a `BuffStackType.Force` buff (`d312`
/// Relic of Fireworks, `d369` Chant of Action): the apply/overstack-removal
/// pair the game emits on every re-trigger currently CANCELS the buff, so
/// its presence collapses. Measured on the reference capture: mean presence
/// error 8.38pp on `b69855` (ours 38.6% vs EI 51.0% on the worst account),
/// which is 0.00029pp after the fix (axilog-measured, not extrapolated).
#[test]
fn force_buff_survives_its_own_overstack_removal() {
    let log = raw(vec![
        apply(0, FIREWORKS, 1, 9, 6000, 7),
        remove_single(0, FIREWORKS, 1, 0, 6000, IFF_UNKNOWN, 7),
    ]);
    let evs = extract(&log, FIREWORKS);
    let states = simulator::run(evs, 1, false, 20_000);
    assert_eq!(
        states,
        vec![(0, 1), (6000, 0)],
        "the buff must survive its own overstack notification and expire on its own"
    );
}

/// The behavioural consequence for an intensity buff (`d422` Might 25): a
/// stack that arcdps reports as naturally ended is removed EARLY (at the
/// report, not at its own expiry), so the held-stack count runs
/// systematically low without ever affecting presence — exactly `d422`'s
/// signature (hitCount −64% at saturation, `totalHitCount` exact).
///
/// Here stack B (applied at t=0, 10s) is reported as ended at t=1000 while
/// stack A (5s) is still up: presence is unchanged either way, but the
/// integral of the stack count is not.
///
/// Fixed: Might's mean relative average-stack error on the committed
/// fixture went 0.007259 -> 0.000035, and 0.15542 -> 0.00411 on the local
/// post-era capture.
#[test]
fn intensity_buff_keeps_its_stack_through_a_natural_end_report() {
    let log = raw(vec![
        apply(0, MIGHT, 1, 9, 5000, 1),  // A
        apply(0, MIGHT, 1, 9, 10_000, 2), // B
        // arcdps reports B as naturally ended at t=1000 (no remover, IFF
        // unknown). B's remaining at that instant is 9000.
        remove_single(1000, MIGHT, 1, 0, 9000, IFF_UNKNOWN, 2),
    ]);
    let evs = extract(&log, MIGHT);
    let states = simulator::run(evs, 25, true, 20_000);
    assert_eq!(states, vec![(0, 1), (0, 2), (5000, 1), (10_000, 0)]);
}

/// A REAL strip (a remover agent is present, so `_byShouldntBeUnknown`) must
/// keep working after the Task 2 fix — this test must NOT change.
#[test]
fn a_real_strip_with_a_remover_agent_is_kept() {
    let log = raw(vec![
        apply(0, MIGHT, 1, 9, 5000, 1),
        remove_single(1000, MIGHT, 1, 42, 4000, IFF_FRIEND, 1),
    ]);
    let evs = extract(&log, MIGHT);
    let states = simulator::run(evs, 25, true, 20_000);
    assert_eq!(states, vec![(0, 1), (1000, 0)], "a real strip must stay a strip");
}

// ---------------------------------------------------------------------
// Rule 2 — the `StackingConditionalLoss` `RemovedDuration` band aid
//
//   GW2EIEvtcParser/EIData/Buffs/BuffsContainer.cs:196-252
//     // Band aid for the stack type situation with fake inactive/infinite
//     // durations
//     if (combatData.HasStackIDs) {
//       var stackTypeBuffs = currentBuffs.Where(x =>
//           x.StackType == BuffStackType.StackingConditionalLoss ||
//           x.StackType == BuffStackType.Stacking);
//       ... foreach real (non-OverstackOrNaturalEnd) BuffRemoveSingleEvent,
//           grouped by (To, BuffInstance), find the last BuffApplyEvent with
//           Time <= remove.Time, accumulate
//             totalDuration = apply.OriginalAppliedDuration
//                           + sum(BuffExtensionEvent.ExtendedDuration)
//                           - sum(gaps before each BuffStackActiveEvent)
//           and IF totalDuration == remove.RemovedDuration:
//             int activeTime  = apply.OriginalAppliedDuration - apply.AppliedDuration;
//             int elapsedTime = (int)(remove.Time - apply.Time);
//             remove.OverrideRemovedDuration(
//                 remove.RemovedDuration - activeTime - elapsedTime);
//     }
//
// i.e. when arcdps reports the stack's ORIGINAL duration instead of its
// REMAINING duration on a conditional-loss strip, GW2EI converts it to the
// remaining duration BEFORE the simulator's 15ms `BuffStack` match runs.
// Without that conversion the match fails and the stack is never removed —
// Stability sits systematically high.
//
// Measured on the reference capture: 181 of Stability's 253 real SINGLE
// removals hit this rewrite; applying it takes the mean average-stack error
// from 0.0412 to 0.0005 (43 of 44 accounts become exact to three decimals).
// ---------------------------------------------------------------------

/// A conditional-loss strip that reports the ORIGINAL applied duration is
/// currently passed through verbatim, so it matches nothing and the stack
/// survives.
///
/// **MBUFFSIM Task 2 flips this**: the expected timeline becomes
/// `[(0, 1), (2000, 0)]`.
#[test]
fn stability_strip_reporting_the_original_duration_currently_matches_nothing() {
    let log = raw(vec![
        apply(0, STABILITY, 1, 9, 6000, 7),
        // Real strip at t=2000 (remover present, so NOT OverstackOrNaturalEnd)
        // but `value` is the ORIGINAL 6000, not the remaining 4000.
        remove_single(2000, STABILITY, 1, 42, 6000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    assert!(
        matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 6000 }),
        "the extractor passes arcdps's raw `value` through today"
    );
    let states = simulator::run(evs, 25, true, 20_000);
    assert_eq!(
        states,
        vec![(0, 1), (6000, 0)],
        "MBUFFSIM Task 2 flips this -- expected [(0, 1), (2000, 0)]: GW2EI rewrites \
         RemovedDuration to 6000 - 0 - 2000 = 4000 first (BuffsContainer.cs:240-246), \
         which then matches the held stack's 4000ms remaining"
    );
}

/// The band aid is gated on `totalDuration == remove.RemovedDuration`: a
/// strip that already reports the REMAINING duration must be left alone.
/// This test must NOT change in Task 2.
#[test]
fn stability_strip_reporting_the_remaining_duration_already_works() {
    let log = raw(vec![
        apply(0, STABILITY, 1, 9, 6000, 7),
        remove_single(2000, STABILITY, 1, 42, 4000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    let states = simulator::run(evs, 25, true, 20_000);
    assert_eq!(states, vec![(0, 1), (2000, 0)]);
}

/// `BuffInstance` (`RawEvent::pad`) is what pairs a strip with its apply in
/// the band aid. It is decoded on the wire but `BuffEvent` does not carry
/// it, so Task 2 has to thread it through (or do the rewrite during
/// extraction, where the raw row is still in hand).
///
/// **MBUFFSIM Task 2 flips this** only in the sense that the rewrite must
/// become possible; the assertion below documents today's gap.
#[test]
fn buff_events_currently_carry_no_buff_instance_id() {
    let log = raw(vec![apply(0, STABILITY, 1, 9, 6000, 7)]);
    let evs = extract(&log, STABILITY);
    let fields = format!("{:?}", evs[0]);
    assert!(
        !fields.contains("instance"),
        "MBUFFSIM Task 2 flips this -- BuffEvent has no BuffInstance field yet, but \
         BuffsContainer.cs:206-210 groups by (To, BuffInstance): {fields}"
    );
}
