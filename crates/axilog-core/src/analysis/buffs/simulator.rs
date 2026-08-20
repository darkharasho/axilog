//! Per-(agent, buff) stack-count state machine (M3, Task 1; reworked in
//! Fix Round 1 -- see the module-level note below and the Task 1 report's
//! "Fix round 1" section for the citations that drove the rework).
//!
//! Verified against GW2EI's default ("NoID") buff simulator --
//! `GW2EIEvtcParser/EIData/Buffs/BuffSimulators/BuffSimulatorNoID/
//! {BuffSimulator,BuffSimulatorDuration,BuffSimulatorIntensity}.cs` and its
//! `StackingLogic` strategies (`EffectStackingLogic/{QueueLogic,
//! OverrideLogic}.cs`) -- which is what GW2EI uses for ordinary boon uptime
//! (the instance-id-based simulator is a separate, more precise mode used
//! selectively; our 12 tracked boons don't need it -- see
//! `BuffStackActiveEvent.IsBuffSimulatorCompliant`).
//!
//! **Fix Round 1 correction**: the original Task 1 implementation modeled
//! EVERY held stack (Queue-type/duration boons included) as continuously
//! ticking from its own apply time -- i.e. `expiry = apply_time +
//! duration_ms`, all concurrently. That is only correct for
//! INTENSITY-type boons (Might/Stability, `BuffStackType.Stacking`/
//! `StackingConditionalLoss` -- `BuffSimulatorIntensity.Update`, which
//! genuinely ticks every held stack down together). For the other 10
//! (Queue-type, `BuffStackType.Queue`) boons, GW2EI's
//! `BuffSimulatorDuration.Update` ticks down ONLY `BuffStack[0]` (the
//! active stack); every queued stack (index > 0) is FROZEN -- its
//! `Duration` field does not decrease -- until it is promoted to index 0
//! (on the active stack's expiry or removal). This module now implements
//! two distinct tick models (`run_duration` vs `run_intensity`) instead of
//! one shared one.

use super::events::{BuffEvent, BuffEventKind};

/// GW2EI's `ParserHelper.BuffSimulatorDelayConstant` (`GW2EIEvtcParser/
/// ParserHelpers/ParserHelper.cs`): the tolerance (ms) used to match a
/// `BuffRemove.Single` event's `removedDuration` against a held stack's
/// current remaining duration. **Fix Round 1**: verified the comparison in
/// `BuffSimulator.Remove` is a STRICT `<` (not `<=`), and it's a
/// first-match linear scan over `BuffStack` in LIST order (not a
/// globally-closest search) -- see `find_single_removal_match`.
pub(crate) const REMOVE_MATCH_TOLERANCE_MS: i64 = 15;

/// Per-boon stack capacity (max concurrent stacks, active + queued
/// combined), verified against GW2EI's `CommonBuffs.Boons` table
/// (`GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs`, lines 14-29) -- NOT the
/// Task 1 brief's "typically 5" guess, which undercounts several of these:
/// Might=25 (`BuffStackType.Stacking`), Fury=9, Quickness=5, Alacrity=9,
/// Protection=5, Regeneration=5, Vigor=5, Aegis=9,
/// Stability=25 (`BuffStackType.StackingConditionalLoss` -- an intensity
/// stack, same capacity field position as Might's `Stacking`; this
/// simplified machine treats it with the same "intensity" replace-lowest
/// policy as Might, which is a deliberate Task 1 simplification of EI's
/// real conditional-loss-on-CC semantics, documented in the Task 1 report),
/// Swiftness=9, Resistance=5, Resolution=5.
pub fn capacity_for(buff_id: u32) -> u32 {
    match buff_id {
        super::MIGHT => 25,
        super::STABILITY => 25,
        super::FURY => 9,
        super::ALACRITY => 9,
        super::AEGIS => 9,
        super::SWIFTNESS => 9,
        super::QUICKNESS => 5,
        super::PROTECTION => 5,
        super::REGENERATION => 5,
        super::VIGOR => 5,
        super::RESISTANCE => 5,
        super::RESOLUTION => 5,
        _ => 5, // unreachable for the 12 tracked boons; conservative default.
    }
}

fn push_state(states: &mut Vec<(u64, u32)>, t: u64, count: u32) {
    if states.last().map(|&(_, c)| c) != Some(count) {
        states.push((t, count));
    }
}

/// GW2EI's `BuffSimulator.Remove`, `BuffRemove.Single` case: a first-match
/// linear scan (NOT a globally-closest search) over the held stacks in
/// LIST order, removing the first one whose current remaining duration is
/// within a STRICT `< 15ms` tolerance of `removed_duration_ms`. `remaining`
/// must already be given in the same order GW2EI's `BuffStack` list would
/// be in at this instant (see call sites for how each stack type
/// satisfies that).
pub(crate) fn find_single_removal_match(
    remaining: impl Iterator<Item = i64>,
    removed_duration_ms: i64,
) -> Option<usize> {
    for (i, r) in remaining.enumerate() {
        if (r - removed_duration_ms).abs() < REMOVE_MATCH_TOLERANCE_MS {
            return Some(i);
        }
    }
    None
}

/// Runs the stack machine over one (agent, buff) event stream (already
/// filtered/grouped by caller -- see `super::simulate_boons`) and returns a
/// step timeline of `(time_ms, stack_count)` transitions. Only entries where
/// the count actually CHANGES are emitted (a compact step function, per
/// `BoonTimeline` semantics) -- no entry means "still whatever the previous
/// entry said", and the implicit count before the first entry is 0.
///
/// `is_intensity` selects the tick model (verified: `BuffStackType.Stacking`/
/// `StackingConditionalLoss` -> `BuffSimulatorIntensity`; everything else,
/// `BuffStackType.Queue` -> `BuffSimulatorDuration` -- see module docs for
/// the behavioral difference). `log_end_ms` bounds natural expiry, mirroring
/// GW2EI's `AbstractBuffSimulator.Simulate`/`Trim(logEnd)`.
pub fn run(events: Vec<BuffEvent>, capacity: u32, is_intensity: bool, log_end_ms: u64) -> Vec<(u64, u32)> {
    if is_intensity {
        run_intensity(events, capacity, log_end_ms)
    } else {
        run_duration(events, capacity, log_end_ms)
    }
}

// ---------------------------------------------------------------------
// Duration (Queue-type) boons: Fury, Regeneration, Vigor, Swiftness,
// Protection, Aegis, Resolution, Quickness, Resistance, Alacrity.
// ---------------------------------------------------------------------

/// `stack[0]` is the ACTIVE (currently ticking) stack's remaining ms, valid
/// as of `clock`; `stack[1..]` are QUEUED stacks, each holding its FROZEN
/// remaining-ms-once-promoted value (does not change while queued).
/// Mirrors GW2EI's `BuffStack: List<BuffStackItem>` for a
/// `BuffSimulatorDuration` exactly -- see `advance_duration`.
type DurationStack = Vec<u64>;

/// Advances the duration machine's clock from wherever it left off to
/// `to_t`: ticks down `stack[0]` and promotes queued items on expiry
/// (possibly several in one jump, if `to_t` is far enough ahead). Mirrors
/// GW2EI's `BuffSimulatorDuration.Update(timePassed)`, which recursively
/// consumes elapsed time against `BuffStack[0]` only -- `Shift`ing every
/// OTHER held stack's `Start` forward with `durationShift = 0` (frozen:
/// `Duration` unchanged) while the active one's `Duration` decreases by the
/// consumed amount.
fn advance_duration(stack: &mut DurationStack, clock: &mut u64, to_t: u64, states: &mut Vec<(u64, u32)>) {
    loop {
        if *clock >= to_t {
            break;
        }
        let Some(&active) = stack.first() else {
            *clock = to_t;
            break;
        };
        let budget = to_t - *clock;
        if active > budget {
            // Active stack survives past `to_t`: tick it down by the full
            // remaining budget, no expiry, no state change (count doesn't
            // change just from ticking).
            stack[0] = active - budget;
            *clock = to_t;
            break;
        }
        // Active stack expires exactly `active` ms from `clock`. Removing
        // index 0 promotes stack[1] (if any) to index 0 "for free" -- its
        // stored value already IS its remaining duration (frozen until
        // now), so it starts ticking correctly from this instant with no
        // further adjustment needed.
        *clock += active;
        stack.remove(0);
        push_state(states, *clock, stack.len() as u32);
        // Loop continues, consuming any leftover budget against the newly
        // promoted active stack -- mirrors GW2EI's recursive
        // `Update(leftOver)` call.
    }
}

fn run_duration(mut events: Vec<BuffEvent>, capacity: u32, log_end_ms: u64) -> Vec<(u64, u32)> {
    events.sort_by_key(|e| e.time);
    let mut stack: DurationStack = Vec::new();
    let mut clock = events.first().map(|e| e.time).unwrap_or(0);
    let mut states: Vec<(u64, u32)> = Vec::new();

    for e in &events {
        advance_duration(&mut stack, &mut clock, e.time, &mut states);
        match e.kind {
            BuffEventKind::Apply { duration_ms, is_shields } => {
                let duration_ms = duration_ms as u64;
                let was_full = stack.len() as u32 >= capacity;
                let inserted_idx = if !was_full {
                    // Verified: `QueueLogic.Add` just appends (its `Sort`
                    // override is a no-op for Queue-type boons).
                    stack.push(duration_ms);
                    push_state(&mut states, e.time, stack.len() as u32);
                    stack.len() - 1
                } else if stack.len() > 1 {
                    // At capacity: evict the min-duration item among the
                    // QUEUED (non-active) stacks and splice the new one
                    // into that slot -- verified: `QueueLogic.FindLowestValue`
                    // explicitly excludes `stacks[0]`
                    // (`stacks.Where(x => x != first).MinBy(TotalDuration)`).
                    // No net count change, so no new state.
                    let (idx, _) =
                        stack.iter().enumerate().skip(1).min_by_key(|&(_, &d)| d).unwrap();
                    stack[idx] = duration_ms;
                    idx
                } else {
                    // capacity == 1: unreachable for the 12 tracked boons
                    // (minimum real capacity 5) and for the 14 conditions
                    // (minimum 3), but LIVE since
                    // `analysis::self_effects` -- Stun and Daze are
                    // capacity 1, both by table and by arcdps's own
                    // `sc::BUFF_INFO` row.
                    //
                    // The unconditional overwrite is correct for them and
                    // not merely a fallback: both are
                    // `BuffStackType::Force`, whose `ForceOverrideLogic`
                    // has a new application REPLACE the active stack
                    // instead of being compared against it, and whose
                    // `IsFull => stacks.Count == 1` caps it at one stack
                    // regardless of the catalogued capacity. So the
                    // capacity-1 arm and Force semantics coincide exactly,
                    // and `run_segments`/`run_duration` need no notion of
                    // stack type to get Stun right.
                    stack[0] = duration_ms;
                    0
                };
                if is_shields {
                    // Verified: `BuffApplyEvent._addedActive =
                    // evtcItem.IsShields > 0`, and when set,
                    // `BuffSimulator.Add` calls `_logic.Activate(BuffStack,
                    // toAdd)` -> `QueueLogic.Activate`: `stacks.Remove(item);
                    // stacks.Insert(0, item);` -- forces the just-applied
                    // stack to become the active (ticking) one, demoting
                    // whatever was previously active to the front of the
                    // (still-frozen) queue.
                    let val = stack.remove(inserted_idx);
                    stack.insert(0, val);
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                // `stack` is already in GW2EI `BuffStack` list order
                // (active at index 0, queued after in queue order), and
                // every entry already holds its CURRENT remaining value
                // (frozen ones don't need adjustment; the active one was
                // just brought current by `advance_duration` above) --
                // so no extra "remaining at time t" computation is needed
                // here, unlike the intensity path below.
                if let Some(idx) = find_single_removal_match(
                    stack.iter().map(|&d| d as i64),
                    removed_duration_ms as i64,
                ) {
                    stack.remove(idx);
                    push_state(&mut states, e.time, stack.len() as u32);
                }
            }
            BuffEventKind::RemoveAll => {
                if !stack.is_empty() {
                    stack.clear();
                    push_state(&mut states, e.time, 0);
                }
            }
            BuffEventKind::Extend { extended_ms, new_duration_ms } => {
                // M3 Task 2: verified `BuffSimulatorDuration.Extend`:
                // `if ((BuffStack.Count != 0 && oldValue > 0) || IsFull) {
                // BuffStack[0].Extend(extension, src); } else { Add(oldValue
                // + extension, ..., addedActive: true, ...); }`. `oldValue`
                // is GW2EI's post-`OffsetNewDuration`-corrected value; this
                // project doesn't implement that correction (deferred by
                // MBUFFSIM Task 2 as below the noise floor -- see
                // `BuffEventKind::Extend`'s doc comment), so `old_value`
                // here is the RAW
                // `new_duration_ms - extended_ms` -- an approximation.
                // `BuffStackItem.Extend` appends to a separate `Extensions`
                // list rather than mutating `Duration` directly, but for a
                // pure stack-COUNT timeline (this simulator's only output)
                // adding `extended_ms` straight onto the target slot's
                // remaining value is observably equivalent: both simply
                // defer that slot's eventual expiry by `extended_ms`
                // without changing the count at any point in between.
                let extended = extended_ms as i64;
                let old_value = new_duration_ms as i64 - extended;
                let is_full = stack.len() as u32 >= capacity;
                if (!stack.is_empty() && old_value > 0) || is_full {
                    if let Some(active) = stack.first_mut() {
                        *active = (*active as i64 + extended).max(0) as u64;
                    }
                } else {
                    let duration_ms = (old_value + extended).max(0) as u64;
                    stack.push(duration_ms);
                    push_state(&mut states, e.time, stack.len() as u32);
                    // `addedActive: true` -- verified `Add(..., addedActive:
                    // true, ...)` in the `else` branch above -- same
                    // promote-to-front-of-queue mechanics as an `is_shields`
                    // apply (see `BuffEventKind::Apply` handling above).
                    let val = stack.pop().unwrap();
                    stack.insert(0, val);
                }
            }
        }
    }
    advance_duration(&mut stack, &mut clock, log_end_ms, &mut states);
    states
}

// ---------------------------------------------------------------------
// Intensity boons: Might, Stability. Unchanged from the original Task 1
// implementation (reviewer-verified correct against
// `BuffSimulatorIntensity.Update`, which genuinely ticks every held stack
// down concurrently) other than reusing the shared, now-fixed
// `find_single_removal_match`.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Stack {
    start: u64,
    duration: u64,
}

impl Stack {
    fn expiry(&self) -> u64 {
        self.start + self.duration
    }

    /// Remaining duration (ms) at time `t`. Callers only compare this after
    /// flushing expired stacks, so in practice it stays >= 0 for anything
    /// still held.
    fn remaining_at(&self, t: u64) -> i64 {
        self.expiry() as i64 - t as i64
    }
}

fn run_intensity(mut events: Vec<BuffEvent>, capacity: u32, log_end_ms: u64) -> Vec<(u64, u32)> {
    events.sort_by_key(|e| e.time);

    let mut stacks: Vec<Stack> = Vec::new();
    let mut states: Vec<(u64, u32)> = Vec::new();

    // Removes every stack whose expiry is <= `upto`, in expiry order,
    // emitting a count-drop step at each one's own expiry timestamp (not at
    // `upto`) -- this is what lets a stack that naturally times out BETWEEN
    // two explicit apply/remove events still produce a correctly-timed step.
    let flush_expiries = |stacks: &mut Vec<Stack>, states: &mut Vec<(u64, u32)>, upto: u64| {
        loop {
            let next = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| s.expiry() <= upto)
                .min_by_key(|(_, s)| s.expiry())
                .map(|(i, s)| (i, s.expiry()));
            match next {
                Some((i, exp)) => {
                    stacks.remove(i);
                    push_state(states, exp, stacks.len() as u32);
                }
                None => break,
            }
        }
    };

    for e in &events {
        flush_expiries(&mut stacks, &mut states, e.time);
        match e.kind {
            BuffEventKind::Apply { duration_ms, .. } => {
                // `is_shields`/activation has NO effect for intensity-type
                // boons: verified `OverrideLogic` does not override
                // `Activate` (uses `StackingLogic`'s empty default) --
                // every held stack ticks concurrently regardless of list
                // position, so "becoming active" is meaningless here.
                let new_stack = Stack { start: e.time, duration: duration_ms as u64 };
                if (stacks.len() as u32) < capacity {
                    stacks.push(new_stack);
                    push_state(&mut states, e.time, stacks.len() as u32);
                } else if let Some((i, _)) = stacks
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, s)| s.remaining_at(e.time))
                {
                    // At capacity: replace whichever held stack is closest
                    // to expiring (verified: `OverrideLogic.FindLowestValue`
                    // removes `stacks[0]`, which its `Add`'s binary-search
                    // insert keeps as the globally-smallest-remaining item).
                    stacks[i] = new_stack;
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                // Verified: `OverrideLogic` keeps `BuffStack` SORTED
                // ascending by remaining duration (via its binary-insert
                // `Add`). Since every intensity stack ticks down at the
                // SAME rate, their relative order never changes over time,
                // so sorting by `remaining_at(e.time)` here is equivalent
                // to querying that persistently-sorted list at this
                // instant. `find_single_removal_match` then applies GW2EI's
                // real first-match/strict-`<` `BuffSimulator.Remove` scan
                // over that (list-order-faithful) sequence.
                let mut order: Vec<usize> = (0..stacks.len()).collect();
                order.sort_by_key(|&i| stacks[i].remaining_at(e.time));
                if let Some(pos) = find_single_removal_match(
                    order.iter().map(|&i| stacks[i].remaining_at(e.time)),
                    removed_duration_ms as i64,
                ) {
                    stacks.remove(order[pos]);
                    push_state(&mut states, e.time, stacks.len() as u32);
                }
            }
            BuffEventKind::RemoveAll => {
                if !stacks.is_empty() {
                    stacks.clear();
                    push_state(&mut states, e.time, 0);
                }
            }
            BuffEventKind::Extend { extended_ms, new_duration_ms } => {
                // M3 Task 2: verified `BuffSimulatorIntensity.Extend`: `if
                // ((BuffStack.Count != 0 && oldValue > 0) || IsFull) { var
                // minItem = BuffStack.MinBy(x => Math.Abs(x.TotalDuration -
                // oldValue)); minItem?.Extend(extension, src); } else {
                // Add(oldValue + extension, ..., addedActive: true); }` --
                // same raw-value approximation for `old_value` as the
                // duration path (see that arm's doc comment): this project
                // doesn't decode `BuffInstance`, so GW2EI's
                // `OffsetNewDuration` per-instance correction isn't
                // replicated. `remaining_at(e.time)` is used as
                // `TotalDuration` here (expiries were already flushed to
                // `e.time` above), and `.duration` is bumped directly by
                // `extended_ms` for the same "observably equivalent to a
                // separate Extensions list" reasoning as the duration path.
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
                    stacks.push(Stack { start: e.time, duration: duration_ms });
                    push_state(&mut states, e.time, stacks.len() as u32);
                }
            }
        }
    }
    flush_expiries(&mut stacks, &mut states, log_end_ms);
    states
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(time: u64, buff_id: u32, owner: u64, duration_ms: u32) -> BuffEvent {
        apply_shields(time, buff_id, owner, duration_ms, false)
    }
    fn apply_shields(time: u64, buff_id: u32, owner: u64, duration_ms: u32, is_shields: bool) -> BuffEvent {
        BuffEvent {
            time,
            buff_id,
            owner,
            agent: owner,
            buff_instance: 0,
            kind: BuffEventKind::Apply { duration_ms, is_shields },
        }
    }
    fn remove_single(time: u64, buff_id: u32, owner: u64, removed_duration_ms: u32) -> BuffEvent {
        BuffEvent {
            time,
            buff_id,
            owner,
            agent: owner,
            buff_instance: 0,
            kind: BuffEventKind::RemoveSingle { removed_duration_ms },
        }
    }
    fn remove_all(time: u64, buff_id: u32, owner: u64) -> BuffEvent {
        BuffEvent { time, buff_id, owner, agent: owner, buff_instance: 0, kind: BuffEventKind::RemoveAll }
    }

    fn run_duration_boon(events: Vec<BuffEvent>, buff_id: u32, log_end_ms: u64) -> Vec<(u64, u32)> {
        run(events, capacity_for(buff_id), false, log_end_ms)
    }
    fn run_intensity_boon(events: Vec<BuffEvent>, buff_id: u32, log_end_ms: u64) -> Vec<(u64, u32)> {
        run(events, capacity_for(buff_id), true, log_end_ms)
    }

    /// Duration-type boon: applying a second stack while the first is still
    /// active queues it FROZEN -- it must NOT start ticking (and thus must
    /// NOT expire) until the first one finishes and it gets promoted.
    /// **Fix Round 1**: a naive "both tick concurrently" model would have
    /// the second stack (applied at t=100, duration 5000) expire at 5100;
    /// the real (frozen-queue) semantics instead have it start ticking only
    /// once promoted at t=1000, so it actually expires at 1000+5000=6000.
    #[test]
    fn duration_apply_while_active_queues_frozen_until_promoted() {
        let events = vec![
            apply(0, super::super::FURY, 1, 1000),   // active: expires 1000
            apply(100, super::super::FURY, 1, 5000), // queued FROZEN (not active -- is_shields=false)
        ];
        let states = run_duration_boon(events, super::super::FURY, 10_000);
        assert_eq!(
            states,
            vec![(0, 1), (100, 2), (1000, 1), (6000, 0)],
            "queued stack must be frozen until promoted at t=1000, then run its full 5000ms to expire at 6000 -- not 5100"
        );
    }

    /// Promotion: the active stack removed EARLY (before natural expiry)
    /// must promote the next queued stack immediately, and that promoted
    /// stack must start ticking from the promotion time (not have somehow
    /// already been counting down while queued).
    #[test]
    fn early_removal_of_active_promotes_and_starts_queued_stack_ticking() {
        let events = vec![
            apply(0, super::super::PROTECTION, 1, 10_000), // A: active, would expire 10000
            apply(0, super::super::PROTECTION, 1, 3000),   // B: queued frozen at 3000
            // Remove A early, at t=500. A's remaining at t=500 is 9500.
            remove_single(500, super::super::PROTECTION, 1, 9500),
        ];
        let states = run_duration_boon(events, super::super::PROTECTION, 10_000);
        // 0->1 (A active), 0->2 (B queues), 500->1 (A removed, B promoted
        // and starts ticking NOW) -- B's full frozen 3000ms then runs from
        // t=500, expiring at 3500 (NOT at "3000", which would be true only
        // if it had been ticking since t=0).
        assert_eq!(states, vec![(0, 1), (0, 2), (500, 1), (3500, 0)]);
    }

    /// `is_shields` (the raw event's `IsShields`/"active when applied"
    /// flag) forces an apply to immediately become the active stack,
    /// demoting whatever was previously active to the (still-frozen) front
    /// of the queue -- verified via `QueueLogic.Activate`.
    #[test]
    fn is_shields_apply_promotes_over_existing_active_stack() {
        let events = vec![
            apply(0, super::super::VIGOR, 1, 10_000), // A: active, would run to 10000
            // B applied at t=100 WITH is_shields set: forces B to become
            // active immediately, demoting A (which had 9900ms left) to
            // the queue, frozen at 9900.
            apply_shields(100, super::super::VIGOR, 1, 2000, true),
        ];
        let states = run_duration_boon(events, super::super::VIGOR, 20_000);
        // 0->1 (A active), 100->2 (B applied+activated, count unchanged by
        // the activation itself -- only the push increases it), B expires
        // at 100+2000=2100 (count->1), promoting the demoted A (frozen at
        // 9900 since t=100) which then runs from t=2100, expiring at
        // 2100+9900=12000.
        assert_eq!(states, vec![(0, 1), (100, 2), (2100, 1), (12_000, 0)]);
    }

    /// Intensity boon (Might): capping at 25 concurrent stacks -- the 26th
    /// apply must NOT push the count past 25.
    #[test]
    fn intensity_caps_at_25() {
        let mut events = Vec::new();
        for i in 0..26u64 {
            // Spread far enough apart (and with a long enough duration)
            // that none expire mid-sequence; only capacity should bound the
            // count.
            events.push(apply(i * 10, super::super::MIGHT, 1, 60_000));
        }
        // `log_end_ms` cut off well before any of the 60s-duration stacks
        // would naturally expire (~60000-60250), so this isolates the
        // apply-side capping behavior from the natural-expiry cascade that
        // would otherwise follow.
        let states = run_intensity_boon(events, super::super::MIGHT, 300);
        let max_count = states.iter().map(|&(_, c)| c).max().unwrap();
        assert_eq!(max_count, 25, "26th apply must not exceed the 25-stack cap");
        assert_eq!(states.len(), 25);
    }

    /// SINGLE removal must remove the stack whose CURRENT remaining
    /// duration matches the event's `removed_duration_ms`, including a
    /// QUEUED (non-active) stack. For duration-type boons the frozen
    /// queued value IS its current remaining duration directly (no
    /// elapsed-time adjustment, since it hasn't been ticking).
    #[test]
    fn single_removal_targets_queued_stack_by_current_remaining_duration() {
        let events = vec![
            apply(0, super::super::PROTECTION, 1, 10_000), // A: active, expires 10000
            apply(100, super::super::PROTECTION, 1, 2000), // B: queued frozen at 2000
            // Remove B specifically: its frozen remaining duration is
            // exactly 2000 (it hasn't ticked at all since it's queued).
            remove_single(200, super::super::PROTECTION, 1, 2000),
        ];
        let states = run_duration_boon(events, super::super::PROTECTION, 20_000);
        // 0->1 (A), 100->2 (B queues), 200->1 (B removed). Only A remains,
        // so the next transition must be A's natural expiry at 10000.
        assert_eq!(states, vec![(0, 1), (100, 2), (200, 1), (10_000, 0)]);
    }

    /// ALL removal clears every currently-held stack immediately, and no
    /// further natural-expiry transitions should fire for stacks that no
    /// longer exist.
    #[test]
    fn all_removal_clears_every_stack() {
        let events = vec![
            apply(0, super::super::AEGIS, 1, 10_000),
            apply(50, super::super::AEGIS, 1, 20_000),
            remove_all(100, super::super::AEGIS, 1),
        ];
        let states = run_duration_boon(events, super::super::AEGIS, 30_000);
        assert_eq!(states, vec![(0, 1), (50, 2), (100, 0)]);
    }

    /// Natural expiry for an INTENSITY boon (Might): must fire in
    /// EXPIRY-time order, not apply-order -- a later-applied, shorter
    /// stack that expires first must produce its count-drop step before an
    /// earlier-applied, longer one, since all intensity stacks tick
    /// concurrently. Unchanged from the original Task 1 implementation.
    #[test]
    fn intensity_natural_expiry_fires_in_expiry_order_not_apply_order() {
        let events = vec![
            apply(0, super::super::MIGHT, 1, 3000), // A: expires 3000 (applied first)
            apply(0, super::super::MIGHT, 1, 1000), // B: expires 1000 (applied second, but shorter)
        ];
        let states = run_intensity_boon(events, super::super::MIGHT, 5000);
        assert_eq!(states, vec![(0, 1), (0, 2), (1000, 1), (3000, 0)]);
    }

    /// Natural expiry for a duration-type boon: the ACTIVE stack always
    /// runs to completion first regardless of the queued stack's (possibly
    /// shorter) duration -- promotion only happens on expiry/removal of the
    /// active stack, never by "soonest expiry wins" the way intensity's
    /// concurrent-tick model works (that ordering is covered by
    /// `intensity_caps_at_25`/`run_intensity`, unchanged from the original
    /// Task 1 implementation).
    #[test]
    fn duration_active_always_finishes_before_promoting_shorter_queued_stack() {
        let events = vec![
            apply(0, super::super::VIGOR, 1, 3000), // A: active, expires 3000
            apply(0, super::super::VIGOR, 1, 1000), // B: queued frozen at 1000 (shorter than A)
        ];
        let states = run_duration_boon(events, super::super::VIGOR, 5000);
        // 0->1 (A active), 0->2 (B queues frozen at 1000), A expires at
        // 3000 (count->1, B promoted and starts ticking from 3000), B's
        // frozen 1000ms then runs from 3000, expiring at 4000 -- NOT at
        // 1000, even though B's duration (1000) is shorter than A's.
        assert_eq!(states, vec![(0, 1), (0, 2), (3000, 1), (4000, 0)]);
    }

    /// A SINGLE removal event that doesn't match any held stack's current
    /// remaining duration (within tolerance) is a no-op -- mirrors GW2EI's
    /// for-loop falling through without removing anything.
    #[test]
    fn single_removal_with_no_match_is_noop() {
        let events = vec![
            apply(0, super::super::RESOLUTION, 1, 10_000),
            remove_single(100, super::super::RESOLUTION, 1, 999_999), // wildly off, no match
        ];
        let states = run_duration_boon(events, super::super::RESOLUTION, 20_000);
        assert_eq!(states, vec![(0, 1), (10_000, 0)], "no state change from the unmatched removal");
    }

    /// **Fix Round 1 (Low)**: SINGLE removal must take the FIRST matching
    /// held stack in list order with a STRICT `< 15ms` tolerance, not a
    /// globally-closest search with `<=`.
    #[test]
    fn single_removal_takes_first_list_match_not_globally_closest() {
        let events = vec![
            apply(0, super::super::RESISTANCE, 1, 10_000), // active: 10000
            apply(0, super::super::RESISTANCE, 1, 2010),   // A: queued frozen 2010 (diff from 2000 = 10, further)
            apply(0, super::super::RESISTANCE, 1, 2002),   // B: queued frozen 2002 (diff from 2000 = 2, numerically closer)
            remove_single(0, super::super::RESISTANCE, 1, 2000),
        ];
        let states = run_duration_boon(events, super::super::RESISTANCE, 20_000);
        // A globally-closest implementation would remove B (closer, diff 2)
        // leaving stacks [active=10000, A=2010]. The real first-list-match
        // semantics remove A (first list match within tolerance, diff 10 <
        // 15) leaving [active=10000, B=2002] -- distinguishable by the
        // final promotion duration (2002 vs 2010) once the active stack
        // finishes at 10000.
        assert_eq!(&states[..3], &[(0, 1), (0, 2), (0, 3)], "three applies land first");
        assert_eq!(states[3], (0, 2), "removal drops count to 2 immediately (same instant)");
        assert_eq!(
            states.last().copied(),
            Some((12002, 0)),
            "surviving queued stack must be B (2002), proving first-list-match (not globally-closest) semantics: {states:?}"
        );
    }
}
