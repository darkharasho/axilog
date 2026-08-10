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

/// `CBTS_STACKACTIVE`. Its only role in these tests is to satisfy GW2EI's
/// `CombatData.HasStackIDs` precondition (`CombatData.cs:610`,
/// `buffEvents.Any(x => x is BuffStackActiveEvent || x is
/// BuffStackDeactiveEvent)`), which gates the whole
/// `StackingConditionalLoss` band aid (`BuffsContainer.cs:197`). Placed on
/// an unrelated buff id and a different owner so it contributes nothing to
/// the `totalDuration` reconstruction under test.
fn stack_active_marker(time: u64) -> RawEvent {
    RawEvent { skillid: 9_999_999, src_agent: 77, dst_agent: 1, ..ev(time, sc::STACK_ACTIVE) }
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
/// rewritten to the REMAINING duration, so it matches the held stack and
/// removes it (MBUFFSIM Task 2, rule 2 — flipped).
#[test]
fn stability_strip_reporting_the_original_duration_is_rewritten() {
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, STABILITY, 1, 9, 6000, 7),
        // Real strip at t=2000 (remover present, so NOT OverstackOrNaturalEnd)
        // but `value` is the ORIGINAL 6000, not the remaining 4000.
        remove_single(2000, STABILITY, 1, 42, 6000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    assert!(
        matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 4000 }),
        "6000 - activeTime(0) - elapsed(2000) = 4000 (BuffsContainer.cs:241-246): {:?}",
        evs[1].kind
    );
    let states = simulator::run(evs, 25, true, 20_000);
    assert_eq!(states, vec![(0, 1), (2000, 0)]);
}

/// The band aid is gated on `CombatData.HasStackIDs`
/// (`CombatData.cs:610` -> `BuffsContainer.cs:197`): a log carrying no
/// `BuffStackActive`/`BuffStackDeactive` row at all must be left alone,
/// even though the removal would otherwise qualify. Same fixture as above
/// minus the marker row.
#[test]
fn band_aid_does_not_run_without_stack_ids() {
    let log = raw(vec![
        apply(0, STABILITY, 1, 9, 6000, 7),
        remove_single(2000, STABILITY, 1, 42, 6000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    assert!(matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 6000 }));
    assert_eq!(simulator::run(evs, 25, true, 20_000), vec![(0, 1), (6000, 0)]);
}

/// `Stacking` (Might) is in the band aid's `stackTypeBuffs` filter but only
/// qualifies for removals reporting `RemovedDuration == int.MaxValue`
/// (`BuffsContainer.cs:202,210`). An ordinary finite Might strip must NOT
/// be rewritten.
#[test]
fn band_aid_skips_plain_stacking_with_a_finite_removed_duration() {
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, MIGHT, 1, 9, 6000, 7),
        remove_single(2000, MIGHT, 1, 42, 6000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, MIGHT);
    assert!(
        matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 6000 }),
        "Might is BuffStackType.Stacking: only int.MaxValue removals qualify"
    );
}

/// ...and the `int.MaxValue` sentinel DOES qualify a `Stacking` buff. The
/// rewrite clamps at 0 (`BuffRemoveSingleEvent.cs:40-43`,
/// `Math.Max(removedDuration, 0)`) — here `i32::MAX - 0 - 2000` stays
/// positive, so this also pins the arithmetic.
#[test]
fn band_aid_applies_to_stacking_with_the_infinite_sentinel() {
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, MIGHT, 1, 9, i32::MAX, 7),
        remove_single(2000, MIGHT, 1, 42, i32::MAX, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, MIGHT);
    assert!(
        matches!(
            evs[1].kind,
            BuffEventKind::RemoveSingle { removed_duration_ms } if removed_duration_ms == (i32::MAX - 2000) as u32
        ),
        "{:?}",
        evs[1].kind
    );
}

/// The `Math.Max(x, 0)` clamp: a strip long after the apply would otherwise
/// produce a negative remaining duration.
#[test]
fn band_aid_clamps_a_negative_rewrite_to_zero() {
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, STABILITY, 1, 9, 6000, 7),
        remove_single(20_000, STABILITY, 1, 42, 6000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    assert!(
        matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 0 }),
        "6000 - 0 - 20000 clamps to 0, not a u32 wrap: {:?}",
        evs[1].kind
    );
}

/// The band aid pairs a removal with its apply by `BuffInstance`: a strip
/// carrying a DIFFERENT instance id finds no apply and is left alone.
#[test]
fn band_aid_pairs_by_buff_instance() {
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, STABILITY, 1, 9, 6000, 7),
        remove_single(2000, STABILITY, 1, 42, 6000, IFF_FRIEND, 8),
    ]);
    let evs = extract(&log, STABILITY);
    assert!(matches!(evs[1].kind, BuffEventKind::RemoveSingle { removed_duration_ms: 6000 }));
}

/// An extension inside `[apply, remove]` raises the reconstructed
/// `totalDuration`, so the gate now matches a LARGER reported value
/// (`BuffsContainer.cs:227-230`).
#[test]
fn band_aid_totals_include_extensions() {
    let mut extend = apply(1000, STABILITY, 1, 9, 2000, 7);
    extend.is_statechange = sc::BUFF_CHANGE;
    extend.overstack = 7000;
    let log = raw(vec![
        stack_active_marker(0),
        apply(0, STABILITY, 1, 9, 6000, 7),
        extend,
        // totalDuration = 6000 + 2000 = 8000
        remove_single(2000, STABILITY, 1, 42, 8000, IFF_FRIEND, 7),
    ]);
    let evs = extract(&log, STABILITY);
    let rm = evs.iter().find(|e| matches!(e.kind, BuffEventKind::RemoveSingle { .. })).unwrap();
    assert!(
        matches!(rm.kind, BuffEventKind::RemoveSingle { removed_duration_ms: 6000 }),
        "8000 - 0 - 2000 = 6000: {:?}",
        rm.kind
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
/// the band aid (`BuffsContainer.cs:206-210`), and `BuffEvent` now carries
/// it (MBUFFSIM Task 2 — flipped).
#[test]
fn buff_events_carry_the_buff_instance_id() {
    let log = raw(vec![apply(0, STABILITY, 1, 9, 6000, 7)]);
    let evs = extract(&log, STABILITY);
    assert_eq!(evs[0].buff_instance, 7);
}
