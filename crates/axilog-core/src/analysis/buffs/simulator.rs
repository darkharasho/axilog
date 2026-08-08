//! Per-(agent, buff) stack-count state machine (M3, Task 1).
//!
//! Verified against GW2EI's default ("NoID") buff simulator --
//! `GW2EIEvtcParser/EIData/Buffs/BuffSimulators/BuffSimulatorNoID/
//! BuffSimulator.cs` and its `StackingLogic` strategies
//! (`EffectStackingLogic/{QueueLogic,OverrideLogic}.cs`) -- which is what
//! GW2EI uses for ordinary boon uptime (the instance-id-based simulator is a
//! separate, more precise mode used selectively; our 12 tracked boons don't
//! need it -- `BuffStackActiveEvent.IsBuffSimulatorCompliant` only requires
//! it for `useBuffInstanceSimulator` or `BuffID == Regeneration`).
//!
//! For a pure stack-COUNT-over-time timeline (this task's output shape --
//! we don't track *which* application is "active" vs "queued", just how
//! many stacks exist), GW2EI's real capacity-overflow behavior (evict the
//! queued/lowest-remaining-duration stack and replace it with the new one,
//! rather than reject the new apply) turns out to be observationally
//! equivalent in COUNT to a simple "clamp at capacity" -- both leave the
//! total stack count unchanged at the cap. So this machine implements the
//! full verified eviction behavior (not just a naive clamp), since it's the
//! same effort and stays correct for expiry TIMING (which stack survives
//! determines when the next count-drop happens).

use super::events::{BuffEvent, BuffEventKind};

/// GW2EI's `ParserHelper.BuffSimulatorDelayConstant` (`GW2EIEvtcParser/
/// ParserHelpers/ParserHelper.cs`): the tolerance (ms) used to match a
/// `BuffRemove.Single` event's `removedDuration` against a held stack's
/// current REMAINING duration (`BuffStackItem.TotalDuration`) at removal
/// time -- not the stack's originally-applied duration. Verified straight
/// from source, not guessed.
const REMOVE_MATCH_TOLERANCE_MS: i64 = 15;

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

#[derive(Debug, Clone, Copy)]
struct Stack {
    start: u64,
    duration: u64,
}

impl Stack {
    fn expiry(&self) -> u64 {
        self.start + self.duration
    }

    /// Remaining duration (ms) at time `t`. May be negative if `t` is past
    /// expiry (callers only compare this after flushing expired stacks, so
    /// in practice it stays >= 0 for anything still held).
    fn remaining_at(&self, t: u64) -> i64 {
        self.expiry() as i64 - t as i64
    }
}

/// Runs the stack machine over one (agent, buff) event stream (already
/// filtered/grouped by caller -- see `super::simulate_boons`) and returns a
/// step timeline of `(time_ms, stack_count)` transitions. Only entries where
/// the count actually CHANGES are emitted (a compact step function, per
/// `BoonTimeline` semantics) -- no entry means "still whatever the previous
/// entry said", and the implicit count before the first entry is 0.
///
/// `log_end_ms` bounds natural expiry: any stack still held when the log
/// ends emits its expiry step only if that expiry falls at or before
/// `log_end_ms` (mirrors GW2EI's `AbstractBuffSimulator.Simulate`, which
/// advances the simulation up to `logEnd` and trims anything still running
/// past it).
pub fn run(mut events: Vec<BuffEvent>, capacity: u32, log_end_ms: u64) -> Vec<(u64, u32)> {
    events.sort_by_key(|e| e.time);

    let mut stacks: Vec<Stack> = Vec::new();
    let mut states: Vec<(u64, u32)> = Vec::new();

    let push_state = |states: &mut Vec<(u64, u32)>, t: u64, count: u32| {
        if states.last().map(|&(_, c)| c) != Some(count) {
            states.push((t, count));
        }
    };

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
            BuffEventKind::Apply { duration_ms } => {
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
                    // to expiring (verified: `BuffSimulator.Add` ->
                    // `_logic.FindLowestValue`, both `QueueLogic` (duration
                    // boons) and `OverrideLogic` (Might/Stability) evict the
                    // lowest-`TotalDuration` item and splice the new stack
                    // into its slot). No net count change, so no new state.
                    stacks[i] = new_stack;
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                // Match against each held stack's REMAINING duration at
                // this removal time (verified: `BuffSimulator.Remove`,
                // `BuffRemove.Single` case, compares
                // `stackItem.TotalDuration` -- the live remaining value,
                // not the originally-applied one). No match within
                // tolerance => no-op, exactly like GW2EI's for-loop falling
                // through without removing anything (this covers
                // overstack/natural-end removal events that don't
                // correspond to a currently-held stack).
                if let Some((i, _)) = stacks
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i, (s.remaining_at(e.time) - removed_duration_ms as i64).abs()))
                    .filter(|&(_, diff)| diff <= REMOVE_MATCH_TOLERANCE_MS)
                    .min_by_key(|&(_, diff)| diff)
                {
                    stacks.remove(i);
                    push_state(&mut states, e.time, stacks.len() as u32);
                }
            }
            BuffEventKind::RemoveAll => {
                if !stacks.is_empty() {
                    stacks.clear();
                    push_state(&mut states, e.time, 0);
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
        BuffEvent { time, buff_id, owner, agent: owner, kind: BuffEventKind::Apply { duration_ms } }
    }
    fn remove_single(time: u64, buff_id: u32, owner: u64, removed_duration_ms: u32) -> BuffEvent {
        BuffEvent {
            time,
            buff_id,
            owner,
            agent: owner,
            kind: BuffEventKind::RemoveSingle { removed_duration_ms },
        }
    }
    fn remove_all(time: u64, buff_id: u32, owner: u64) -> BuffEvent {
        BuffEvent { time, buff_id, owner, agent: owner, kind: BuffEventKind::RemoveAll }
    }

    /// Duration-type boon: applying a second stack while the first is still
    /// active must QUEUE it (count goes to 2), not replace/ignore it.
    #[test]
    fn duration_apply_while_active_queues() {
        let events = vec![
            apply(0, super::super::FURY, 1, 1000), // expires 1000
            apply(100, super::super::FURY, 1, 5000), // expires 5100, queued behind the first
        ];
        let states = run(events, capacity_for(super::super::FURY), 10_000);
        assert_eq!(states, vec![(0, 1), (100, 2), (1000, 1), (5100, 0)]);
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
        // would otherwise follow (each of the 25 held stacks expiring in
        // turn, which is exercised separately by
        // `natural_expiry_fires_in_expiry_order_not_apply_order`).
        let states = run(events, capacity_for(super::super::MIGHT), 300);
        let max_count = states.iter().map(|&(_, c)| c).max().unwrap();
        assert_eq!(max_count, 25, "26th apply must not exceed the 25-stack cap");
        // Exactly 25 transitions up to the cap (0->1, 1->2, ..., 24->25),
        // and no further count-changing transition from the 26th apply
        // (it replaces the soonest-to-expire stack in place, no net count
        // change).
        assert_eq!(states.len(), 25);
    }

    /// SINGLE removal must remove the stack whose REMAINING duration
    /// matches the event's `removed_duration_ms` -- including a QUEUED
    /// (non-first-applied) stack, not just the oldest one. This is
    /// offset/semantics-meaningful: matching against ORIGINAL applied
    /// duration instead of remaining-at-removal-time, or always popping the
    /// first-applied stack (FIFO) instead of matching by duration, would
    /// both remove the wrong stack here and fail this assertion.
    #[test]
    fn single_removal_targets_queued_stack_by_remaining_duration() {
        let events = vec![
            apply(0, super::super::PROTECTION, 1, 10_000), // A: expires 10000
            apply(100, super::super::PROTECTION, 1, 2000), // B: expires 2100, queued
            // Remove B specifically: at t=200, B's remaining duration is
            // 2100 - 200 = 1900ms. A's remaining at t=200 is 10000-200=9800,
            // clearly not a match.
            remove_single(200, super::super::PROTECTION, 1, 1900),
        ];
        let states = run(events, capacity_for(super::super::PROTECTION), 20_000);
        // 0->1 (A), 100->2 (B queues), 200->1 (B removed). Only A remains,
        // so the next transition must be A's natural expiry at 10000 -- if
        // the wrong stack (A) had been removed instead, we'd see a
        // transition at 2100 (B's expiry) instead, and none at 10000.
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
        let states = run(events, capacity_for(super::super::AEGIS), 30_000);
        assert_eq!(states, vec![(0, 1), (50, 2), (100, 0)]);
    }

    /// Natural expiry must fire in EXPIRY-time order, not apply-order: a
    /// later-applied, shorter-duration stack that expires first must
    /// produce its count-drop step before an earlier-applied, longer stack.
    #[test]
    fn natural_expiry_fires_in_expiry_order_not_apply_order() {
        let events = vec![
            apply(0, super::super::VIGOR, 1, 3000), // A: expires 3000 (applied first)
            apply(0, super::super::VIGOR, 1, 1000), // B: expires 1000 (applied second, but shorter)
        ];
        let states = run(events, capacity_for(super::super::VIGOR), 5000);
        // Both applies land at the same instant (count 0->1->2), then B's
        // shorter duration expires at 1000 (count->1) BEFORE A's at 3000
        // (count->0) -- reverse of apply order.
        assert_eq!(states, vec![(0, 1), (0, 2), (1000, 1), (3000, 0)]);
    }

    /// A SINGLE removal event that doesn't match any held stack's remaining
    /// duration (within tolerance) is a no-op -- mirrors GW2EI's for-loop
    /// falling through without removing anything.
    #[test]
    fn single_removal_with_no_match_is_noop() {
        let events = vec![
            apply(0, super::super::RESOLUTION, 1, 10_000),
            remove_single(100, super::super::RESOLUTION, 1, 999_999), // wildly off, no match
        ];
        let states = run(events, capacity_for(super::super::RESOLUTION), 20_000);
        assert_eq!(states, vec![(0, 1), (10_000, 0)], "no state change from the unmatched removal");
    }
}
