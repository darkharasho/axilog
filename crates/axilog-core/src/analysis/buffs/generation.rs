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
//! source instead of a stack-count timeline -- see
//! `run_duration_segments`/`run_intensity_segments` below, which mirror
//! `simulator::run_duration`/
//! `run_intensity` branch-for-branch (reusing the same removal-matching
//! helper, `simulator::find_single_removal_match`) so any future fix to
//! the count-timeline model doesn't silently diverge from this one.

use super::events::{BuffEvent, BuffEventKind};
use super::simulator::{self, find_single_removal_match};
use super::BOON_IDS;
use crate::evtc::RawLog;
use crate::model::{Encounter, Player};
use std::collections::BTreeMap;

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
    /// WASTED counterpart of [`Self::self_pct`] (MSMALL item 2):
    /// `GetBuffsForSelf`'s `uptime.Wasted`.
    pub self_wasted: f64,
    /// WASTED counterpart of [`Self::group_pct`].
    pub group_wasted: f64,
    /// WASTED counterpart of [`Self::squad_pct`].
    pub squad_wasted: f64,
}

// ---------------------------------------------------------------------
// Per-(target, buff) simulation: generated-ms per source.
// ---------------------------------------------------------------------

/// One stretch of time during which one SOURCE's stack was held on the
/// target (MEIGAP Task 1b). `[start, end)`, absolute arcdps time.
///
/// This is the raw material both consumers of this module reduce: summing
/// `end - start` per source gives the generated-ms map
/// (`simulate_boon_generation_ms`, unchanged in behaviour -- the two
/// `run_*_ms` entry points below are now thin sums over exactly the
/// segments they used to accumulate inline), while counting overlaps per
/// source gives GW2EI's `statesPerSource` stack timeline
/// (`super::states::build`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldSegment {
    pub source: u64,
    pub start: u64,
    pub end: u64,
}

/// One stack's worth of WASTED boon duration, credited to the source that
/// applied it (MSMALL item 2).
///
/// GW2EI's `BuffSimulationItemWasted`. "Waste" is boon-time a source
/// generated that the target never got to spend: the stack was destroyed
/// while it still had duration left. There are exactly THREE sites that
/// produce it, all in `BuffSimulatorNoID`:
///
/// 1. **Capacity-overflow eviction** -- `StackingLogic.FindLowestValue`,
///    reached from `BuffSimulator.Add` when `IsFull`
///    (`BuffSimulator.cs:56-69`). Each logic subclass picks its own victim
///    (`QueueLogic.cs:12-30`, `HealingLogic.cs:31-60`,
///    `OverrideLogic.cs:14-31`) and then:
///    `wastes.Add(new BuffSimulationItemWasted(toRemove.Src,
///    toRemove.Duration, toRemove.Start))`.
/// 2. **`BuffRemove.Single`** -- the matched stack
///    (`BuffSimulator.cs:110-127`).
/// 3. **`BuffRemove.All`** -- EVERY held stack (`BuffSimulator.cs:93-108`).
///
/// In all three the credited amount is the victim's CURRENT remaining
/// `Duration` (post-`Shift`), attributed to that stack's own `Src` --
/// followed by one further record per entry in its `Extensions` list,
/// each credited to the EXTENDER rather than the original applier.
///
/// **Extensions are deliberately folded, not modelled separately.**
/// `BuffStackItem.Extensions` is only ever populated by a `BuffExtension`
/// event, and those are vanishingly rare in practice: measured over this
/// project's fixtures, `fixtures/wvw-small.anon.zevtc` has **0** extend
/// events out of 11,359 boon events, and the local post-rework capture has
/// **160** out of 68,593 (0.23%). This module therefore keeps the
/// pre-existing folded representation (an extension adds straight onto the
/// slot's remaining duration) and credits the whole of a wasted stack to
/// its original `Src`. The bounded consequence: for a stack that was BOTH
/// extended and later wasted, the extension's share of the waste lands on
/// the applier instead of the extender. That is an attribution split
/// within one stack, never a change to the TOTAL waste, and it can only
/// affect the <=160 extended stacks in either fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasteRecord {
    /// The stack's `Src` -- the account-representative addr that applied it.
    pub source: u64,
    /// The remaining `Duration` (ms) destroyed with the stack.
    pub ms: u64,
}

/// Both reductions of one (target, buff) simulation: the held segments that
/// become GENERATION, and the destroyed remainders that become WASTE. They
/// are produced by one pass so they can never describe different
/// simulations -- the same single-sourcing argument
/// [`simulate_boon_held_segments`] already makes for segments alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimOutput {
    pub segments: Vec<HeldSegment>,
    pub wastes: Vec<WasteRecord>,
}

/// Sums [`WasteRecord`]s into a per-source wasted-ms map, exactly as
/// [`segments_to_gen_ms`] does for generation.
fn wastes_to_ms(wastes: &[WasteRecord]) -> BTreeMap<u64, u64> {
    let mut out: BTreeMap<u64, u64> = BTreeMap::new();
    for w in wastes {
        *out.entry(w.source).or_default() += w.ms;
    }
    out
}

/// A duration-type (Queue) boon's held stack, carrying its source alongside
/// the remaining-ms value `simulator::run_duration`'s `DurationStack`
/// already tracks (see that function's doc comment for the frozen-queue
/// tick model this mirrors).
#[derive(Debug, Clone, Copy)]
struct DSlot {
    remaining: u64,
    source: u64,
    /// Insertion sequence number, used ONLY to re-find this exact slot
    /// after `HealingLogic.Sort` may have moved it (GW2EI holds the
    /// `BuffStackItem` reference across `Add` -> `Activate`; we hold an
    /// index, which a sort invalidates).
    ///
    /// Never read by any ordering or matching rule. For the eleven
    /// non-Regeneration boons nothing sorts, so the lookup provably returns
    /// the same index the pre-MSMALL code used literally -- which is what
    /// keeps their generation output byte-identical.
    seq: u64,
}

/// Sums [`HeldSegment`]s into the per-source generated-ms map every
/// pre-MEIGAP caller of this module expects. Zero-length segments
/// contribute nothing, exactly as the old inline `+= 0` did.
fn segments_to_gen_ms(segments: &[HeldSegment]) -> BTreeMap<u64, u64> {
    let mut out: BTreeMap<u64, u64> = BTreeMap::new();
    for s in segments {
        *out.entry(s.source).or_default() += s.end.saturating_sub(s.start);
    }
    out
}

/// Mirrors `simulator::advance_duration` exactly, but credits elapsed
/// active-slot ms as a [`HeldSegment`] owned by `stack[0].source` --
/// GW2EI only ever attributes DURATION-type generation to whichever
/// source's stack currently occupies the active slot (`Stacks[0]`), never
/// to a frozen queued stack.
fn advance_duration_ms(
    stack: &mut Vec<DSlot>,
    clock: &mut u64,
    to_t: u64,
    segments: &mut Vec<HeldSegment>,
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
            segments.push(HeldSegment { source: active.source, start: *clock, end: to_t });
            stack[0].remaining -= budget;
            *clock = to_t;
            break;
        }
        segments.push(HeldSegment {
            source: active.source,
            start: *clock,
            end: *clock + active.remaining,
        });
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
fn run_duration_segments(
    mut events: Vec<BuffEvent>,
    capacity: u32,
    log_end_ms: u64,
    is_regeneration: bool,
    healing_of: &dyn Fn(u64) -> i16,
) -> SimOutput {
    events.sort_by_key(|e| e.time);
    let mut stack: Vec<DSlot> = Vec::new();
    let mut clock = events.first().map(|e| e.time).unwrap_or(0);
    let mut segments: Vec<HeldSegment> = Vec::new();
    let mut wastes: Vec<WasteRecord> = Vec::new();
    let mut seq: u64 = 0;

    /// Index of the slot with this insertion sequence number. Always
    /// present at every call site (the slot was just written).
    fn locate(stack: &[DSlot], seq: u64) -> usize {
        stack.iter().position(|s| s.seq == seq).unwrap_or(0)
    }

    // `HealingLogic.Sort` (`HealingLogic.cs:23-30`): Regeneration's stack
    // list is kept sorted by SeedSrc.Healing DESCENDING, re-applied on every
    // `StackingLogic.Add`. See `run_sim`'s doc comment for why a STABLE sort
    // is the faithful choice here.
    //
    // **Measured inert on real arcdps data, and kept anyway.** Both this
    // project's fixtures report `healing == 0` for EVERY player agent (74
    // of 74 committed, 119 of 119 local) -- the agent table's healing field
    // carries the recording player's own relative stats, not a populated
    // per-agent healing-power figure. With all keys equal, a stable sort is
    // by definition a no-op, which is confirmed end-to-end: sorting and not
    // sorting produce byte-identical waste numbers on both fixtures. It is
    // implemented because it is what GW2EI does and because a log that DOES
    // carry healing values would need it -- not because it is currently
    // doing any work. The eviction rule below is the half of HealingLogic
    // that actually moves numbers.
    let resort = |stack: &mut Vec<DSlot>| {
        if is_regeneration {
            stack.sort_by_key(|s| std::cmp::Reverse(healing_of(s.source)));
        }
    };

    for e in &events {
        advance_duration_ms(&mut stack, &mut clock, e.time, &mut segments);
        match e.kind {
            BuffEventKind::Apply { duration_ms, is_shields } => {
                let duration_ms = duration_ms as u64;
                let was_full = stack.len() as u32 >= capacity;
                seq += 1;
                let new_slot = DSlot { remaining: duration_ms, source: e.agent, seq };
                let inserted_idx = if !was_full {
                    stack.push(new_slot);
                    resort(&mut stack);
                    locate(&stack, seq)
                } else if stack.len() > 1 {
                    // Capacity overflow -> `FindLowestValue`: waste site 1.
                    // `QueueLogic` evicts the min-TotalDuration NON-active
                    // stack; `HealingLogic` (Regeneration) instead takes
                    // `stacks.Last()` -- the LOWEST-healing source, by the
                    // descending sort invariant above
                    // (`HealingLogic.cs:44-45`: `toRemove ??= stacks.Last()`,
                    // reached whenever the NoID path leaves both
                    // `overridenStackID` and `overridenDuration` at 0).
                    let idx = if is_regeneration {
                        stack.len() - 1
                    } else {
                        stack.iter().enumerate().skip(1).min_by_key(|&(_, s)| s.remaining).unwrap().0
                    };
                    wastes.push(WasteRecord { source: stack[idx].source, ms: stack[idx].remaining });
                    stack[idx] = new_slot;
                    resort(&mut stack);
                    locate(&stack, seq)
                } else {
                    wastes.push(WasteRecord { source: stack[0].source, ms: stack[0].remaining });
                    stack[0] = new_slot;
                    0
                };
                if is_shields {
                    let val = stack.remove(inserted_idx);
                    stack.insert(0, val);
                }
            }
            BuffEventKind::RemoveSingle { removed_duration_ms } => {
                // Waste site 2: the matched stack's remaining duration.
                if let Some(idx) = find_single_removal_match(
                    stack.iter().map(|s| s.remaining as i64),
                    removed_duration_ms as i64,
                ) {
                    wastes.push(WasteRecord { source: stack[idx].source, ms: stack[idx].remaining });
                    stack.remove(idx);
                }
            }
            BuffEventKind::RemoveAll => {
                // Waste site 3: EVERY held stack.
                for slot in stack.drain(..) {
                    wastes.push(WasteRecord { source: slot.source, ms: slot.remaining });
                }
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
                    seq += 1;
                    stack.insert(0, DSlot { remaining: duration_ms, source: e.agent, seq });
                }
            }
        }
    }
    advance_duration_ms(&mut stack, &mut clock, log_end_ms, &mut segments);
    // Anything still held at log end is NOT waste: GW2EI only records waste
    // where a stack is actively destroyed, and `AfterSimulate` just releases
    // the remaining pool entries without touching `WasteSimulationResult`
    // (`BuffSimulator.cs:50-54`).
    SimOutput { segments, wastes }
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
/// time by construction), as a [`HeldSegment`] owned by `s.source`.
fn credit(segments: &mut Vec<HeldSegment>, s: &IStack, end_t: u64) {
    segments.push(HeldSegment { source: s.source, start: s.start, end: end_t.max(s.start) });
}

/// Mirrors `simulator::run_intensity`'s `flush_expiries`, additionally
/// crediting each naturally-expired stack's full lifetime (`start` to its
/// own `expiry()`, NOT `upto`) to its source.
fn flush_expiries_ms(stacks: &mut Vec<IStack>, segments: &mut Vec<HeldSegment>, upto: u64) {
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
                credit(segments, &s, exp);
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
fn run_intensity_segments(
    mut events: Vec<BuffEvent>,
    capacity: u32,
    log_end_ms: u64,
) -> SimOutput {
    events.sort_by_key(|e| e.time);
    let mut stacks: Vec<IStack> = Vec::new();
    let mut segments: Vec<HeldSegment> = Vec::new();
    let mut wastes: Vec<WasteRecord> = Vec::new();

    for e in &events {
        flush_expiries_ms(&mut stacks, &mut segments, e.time);
        match e.kind {
            BuffEventKind::Apply { duration_ms, .. } => {
                let new_stack = IStack { start: e.time, duration: duration_ms as u64, source: e.agent };
                if (stacks.len() as u32) < capacity {
                    stacks.push(new_stack);
                } else if let Some((i, _)) =
                    stacks.iter().enumerate().min_by_key(|(_, s)| s.remaining_at(e.time))
                {
                    // Waste site 1 (`OverrideLogic.FindLowestValue`): the
                    // evicted stack's remaining duration is destroyed.
                    let evicted = stacks[i];
                    credit(&mut segments, &evicted, e.time);
                    wastes.push(WasteRecord {
                        source: evicted.source,
                        ms: evicted.remaining_at(e.time).max(0) as u64,
                    });
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
                    // Waste site 2.
                    let s = stacks.remove(order[pos]);
                    credit(&mut segments, &s, e.time);
                    wastes.push(WasteRecord {
                        source: s.source,
                        ms: s.remaining_at(e.time).max(0) as u64,
                    });
                }
            }
            BuffEventKind::RemoveAll => {
                // Waste site 3: EVERY held stack.
                for s in stacks.drain(..) {
                    credit(&mut segments, &s, e.time);
                    wastes.push(WasteRecord {
                        source: s.source,
                        ms: s.remaining_at(e.time).max(0) as u64,
                    });
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
    flush_expiries_ms(&mut stacks, &mut segments, log_end_ms);
    // Anything still held past `log_end_ms` is credited only up to
    // `log_end_ms` -- mirrors GW2EI's `GetClampedDuration(start, end)`
    // (phase-clamped generation), same convention `uptime::compute` already
    // uses for its own window.
    for s in &stacks {
        credit(&mut segments, s, log_end_ms);
    }
    // Still-held stacks at log end are not waste -- see the duration path.
    SimOutput { segments, wastes }
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
    simulate_boon_generation_ms_with_inputs(
        raw,
        &super::extract_boon_inputs_with_registry(raw, registry),
        enc,
    )
}

/// [`simulate_boon_generation_ms`] against caller-supplied, already-extracted
/// [`super::BoonInputs`] (MPERF Task 3) -- see that struct's doc comment for
/// why sharing the extraction with `super::simulate_boons_with_inputs` is
/// output-identical (the two *simulations* stay fully independent, as this
/// module's own doc requires; only their identical raw input is shared). The
/// `registry`-taking wrapper above stays for callers that have a registry but
/// no extracted inputs.
///
/// `raw` is still needed here for the log's final event time (the absolute
/// window generation is accumulated over), which is not part of the
/// extracted inputs.
pub(crate) fn simulate_boon_generation_ms_with_inputs(
    raw: &RawLog,
    inputs: &super::BoonInputs,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), BTreeMap<u64, u64>> {
    simulate_boon_generation_and_waste_ms(raw, inputs, enc).0
}

/// `(target rep addr, buff id) -> (source rep addr -> ms)` -- the shape both
/// halves of [`simulate_boon_generation_and_waste_ms`] return.
pub type PerTargetSourceMs = BTreeMap<(u64, u32), BTreeMap<u64, u64>>;

/// Per-source GENERATED-ms and WASTED-ms for every (target, buff), from ONE
/// simulation pass (MSMALL item 2).
///
/// Returns `(generated, wasted)`, both keyed `(target rep addr, buff id) ->
/// (source rep addr -> ms)`. The generated half is bit-for-bit what
/// [`simulate_boon_generation_ms_with_inputs`] always returned; the wasted
/// half is the new one. See [`WasteRecord`] for what waste is and the three
/// GW2EI sites that produce it.
pub(crate) fn simulate_boon_generation_and_waste_ms(
    raw: &RawLog,
    inputs: &super::BoonInputs,
    enc: &Encounter,
) -> (PerTargetSourceMs, PerTargetSourceMs) {
    let log_end_ms = log_end_of(raw);
    // `HealingLogic` sorts on the SOURCE's healing power, and the event
    // stream has already been folded onto account representatives, so the
    // lookup has to accept a representative addr. A representative IS one of
    // the account's raw addrs, so the raw table resolves it directly; an
    // unknown source (enemy/NPC) falls back to 0, which sorts last -- the
    // same "evict the least-healing stack" outcome an unrecognized applier
    // should get.
    let healing_of = |addr: u64| inputs.healing_power.get(&addr).copied().unwrap_or(0);
    let mut gen_out = BTreeMap::new();
    let mut waste_out = BTreeMap::new();
    for ((rep, buff_id), evs) in grouped_boon_events(inputs, enc) {
        let (capacity, is_intensity) = capacity_and_kind(inputs, buff_id);
        let out = run_sim(
            evs,
            capacity,
            is_intensity,
            log_end_ms,
            buff_id == super::REGENERATION,
            &healing_of,
        );
        gen_out.insert((rep, buff_id), segments_to_gen_ms(&out.segments));
        waste_out.insert((rep, buff_id), wastes_to_ms(&out.wastes));
    }
    (gen_out, waste_out)
}

/// The log's final absolute event time -- the window both simulations tick
/// stacks against (see the two `*_with_inputs` doc comments).
fn log_end_of(raw: &RawLog) -> u64 {
    raw.events.last().map(|e| e.time).unwrap_or(0)
}

/// `(arcdps-reported-or-hardcoded capacity, is intensity)` for one buff.
fn capacity_and_kind(inputs: &super::BoonInputs, buff_id: u32) -> (u32, bool) {
    let capacity = inputs
        .capacities
        .get(&buff_id)
        .copied()
        .unwrap_or_else(|| simulator::capacity_for(buff_id));
    let is_intensity = BOON_IDS
        .iter()
        .any(|&(id, _, is_intensity)| id == buff_id && is_intensity);
    (capacity, is_intensity)
}

/// The `(target representative addr, buff id) -> events` grouping BOTH
/// reductions in this module consume, with the source addr already folded
/// onto its account representative.
///
/// A relogged source folds onto ONE key instead of splitting its credit
/// across pre/post-relog addrs (the same relog-fold reasoning the target
/// side uses). A source that isn't a known squad addr at all (enemy, NPC,
/// unrecognized) keeps its raw addr -- see
/// [`simulate_boon_generation_ms`]'s doc comment.
fn grouped_boon_events(
    inputs: &super::BoonInputs,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), Vec<BuffEvent>> {
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for &(mut e) in &inputs.events {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        e.agent = addr_to_rep.get(&e.agent).copied().unwrap_or(e.agent);
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }
    grouped
}

/// Stack-type dispatch for the two segment simulators -- the single place
/// the intensity/duration choice is made, shared by
/// [`simulate_boon_generation_ms_with_inputs`] and
/// [`simulate_boon_held_segments_with_inputs`] so the two views can never
/// diverge on it.
///
/// Made `pub` by MEIGAP Task 2d so `analysis::target_conditions` can run the
/// SAME two simulators over CONDITION events on ENEMY agents. That pass
/// deliberately reuses this entry point rather than copying the dispatch:
/// the intensity/duration choice, and both simulators behind it, then stay
/// single-sourced across boons-on-players and conditions-on-enemies.
pub fn run_segments(
    events: Vec<BuffEvent>,
    capacity: u32,
    is_intensity: bool,
    log_end_ms: u64,
) -> Vec<HeldSegment> {
    run_sim(events, capacity, is_intensity, log_end_ms, false, &|_| 0).segments
}

/// The full [`SimOutput`] (segments AND wastes) behind [`run_segments`].
///
/// `is_regeneration` selects GW2EI's `HealingLogic` instead of the default
/// `QueueLogic` -- `BuffSimulator.cs:24-43` dispatches on
/// `buff.StackType`, and `BuffStackType.Regeneration` is the ONLY value
/// mapped to `_healingLogic`. `healing_of` resolves a source's
/// `SeedSrc.Healing` (the arcdps agent-table healing-power attribute),
/// which is the only thing that logic sorts on.
///
/// `HealingLogic` differs from its `QueueLogic` base in exactly two ways
/// that matter to a no-stack-ID (NoID) simulation:
///
/// * `Sort` (`HealingLogic.cs:23-30`) keeps the stack list ordered by
///   `SeedSrc.Healing` DESCENDING, re-applied on every `Add`. Implemented
///   with a STABLE `sort_by`: .NET's `List.Sort` is an introsort, but for
///   the <=9 elements a boon capacity ever reaches it takes the insertion-
///   sort path, which preserves the relative order of equal keys -- and
///   equal keys are the common case here (any two sources with the same
///   healing power). An unstable sort would make the result depend on
///   pivot choice, which is not something to reproduce.
/// * `FindLowestValue` (`HealingLogic.cs:31-60`) evicts by stack ID, else
///   by nearest `TotalDuration` to `overridenDuration`, else
///   `stacks.Last()`. In NoID mode both overrides are 0 (they are only ever
///   set by `BuffDictionary.AddRegen` threading a `BuffStackActiveEvent`'s
///   instance, which requires stack IDs), so the live branch is
///   `stacks.Last()` -- the LOWEST-healing source, by the sort invariant.
///   That is a genuinely different victim from `QueueLogic`'s
///   min-`TotalDuration`-among-non-active, and it is why Regeneration waste
///   needs this logic rather than the Queue default.
///
/// # Enumerated effect of switching Regeneration onto this logic
///
/// The commit message for this change (58ea2e1) said "75 native cells
/// change". That figure is right but ambiguous, and a review measured 50
/// under a different (also right) definition, so the full breakdown is
/// recorded here as the durable record. Measured on the local post-rework
/// capture, native `parse` output, before vs after:
///
/// | quantity | changed |
/// |---|---|
/// | `generation.self_pct` | 25 |
/// | `generation.group_pct` | 23 |
/// | `generation.squad_pct` | 27 |
/// | **all three scopes (the "75")** | **75** |
/// | **group + squad only (the "50")** | **50** |
/// | distinct `(player, boon)` cells | 31 |
/// | `presence_pct` / `avg_stacks` | **0** |
/// | boon ids affected | **{718} (Regeneration) only** |
///
/// The two rows that matter for the "improvements only" gate: NOTHING
/// outside Regeneration moved, and no uptime/presence cell moved at all --
/// this logic changes which SOURCE is credited, never how long the target
/// held the boon. Every one of the moved cells moves toward EI (mean error
/// 2.729 -> 1.796 self, 3.161 -> 2.053 group, 0.614 -> 0.495 squad; worst
/// cell 20.97 -> 8.99, 11.20 -> 7.24, 4.81 -> 3.67).
///
/// `BuffStackActiveEvent` compliance in NoID mode
/// (`BuffStackActiveEvent.cs:12-15`: `BuffInstance != 0 && base && (useBuff
/// InstanceSimulator || BuffID == Regeneration)`) is deliberately NOT
/// modelled -- it drives `Activate(stackID)`/`_noSort`, which this
/// project's stack-ID-less pipeline has no input for.
fn run_sim(
    events: Vec<BuffEvent>,
    capacity: u32,
    is_intensity: bool,
    log_end_ms: u64,
    is_regeneration: bool,
    healing_of: &dyn Fn(u64) -> i16,
) -> SimOutput {
    if is_intensity {
        run_intensity_segments(events, capacity, log_end_ms)
    } else {
        run_duration_segments(events, capacity, log_end_ms, is_regeneration, healing_of)
    }
}

/// The per-(target representative addr, buff id) HELD SEGMENT lists behind
/// [`simulate_boon_generation_ms_with_inputs`]'s own generated-ms rollup
/// (MEIGAP Task 1b) -- the input `super::states::build` reduces into
/// GW2EI's `statesPerSource` stack timelines.
///
/// Same grouping, same relog folds, same simulators as the generated-ms
/// pass, which is now literally a sum over these segments -- so a source's
/// `statesPerSource` timeline and its `generated`/`squadBuffs` numbers can
/// never describe different simulations.
///
/// **Opt-in, not wired into `analyze()`**: it is a second full run of the
/// boon simulation (the first, inside `analyze`, keeps only the summed
/// form), so the caller runs it exactly when the EI-shape state timelines
/// were requested -- the standalone-pass convention `replay`/`missiles`/
/// `damage_mods` already use.
pub fn simulate_boon_held_segments(
    raw: &RawLog,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), Vec<HeldSegment>> {
    let registry = crate::analysis::damage::InstidRegistry::build(raw);
    simulate_boon_held_segments_with_inputs(
        raw,
        &super::extract_boon_inputs_with_registry(raw, &registry),
        enc,
    )
}

/// [`simulate_boon_held_segments`] against caller-supplied, already-
/// extracted [`super::BoonInputs`].
pub fn simulate_boon_held_segments_with_inputs(
    raw: &RawLog,
    inputs: &super::BoonInputs,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), Vec<HeldSegment>> {
    grouped_boon_events(inputs, enc)
        .into_iter()
        .map(|((rep, buff_id), evs)| {
            let (capacity, is_intensity) = capacity_and_kind(inputs, buff_id);
            ((rep, buff_id), run_segments(evs, capacity, is_intensity, log_end_of(raw)))
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
    rollup_with_waste(target_gen, &BTreeMap::new(), enc, log_start_ms, log_end_ms)
}

/// [`rollup`] plus the WASTED half (MSMALL item 2).
///
/// `target_waste` has the identical shape to `target_gen` and is reduced
/// through the identical scaling -- verified: `BuffStatistics.cs` applies
/// exactly the same `/phaseDuration`, `/playerCount` and `*100`-only-for-
/// duration treatment to `wasted` as to `generation`, in BOTH
/// `GetBuffsForPlayers` (lines 116-141) and `GetBuffsForSelf` (lines
/// 190-216). So this reuses `avg_pct`/`ms_to_pct` rather than restating it.
pub fn rollup_with_waste(
    target_gen: &BTreeMap<(u64, u32), BTreeMap<u64, u64>>,
    target_waste: &BTreeMap<(u64, u32), BTreeMap<u64, u64>>,
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

            let self_wasted_ms = target_waste
                .get(&(source_addr, boon_id))
                .and_then(|m| m.get(&source_addr))
                .copied()
                .unwrap_or(0);
            let self_wasted = ms_to_pct(self_wasted_ms, phase_ms, is_intensity);
            let group_wasted =
                avg_pct(&group_targets, boon_id, source_addr, target_waste, phase_ms, is_intensity);
            let squad_wasted =
                avg_pct(&squad_targets, boon_id, source_addr, target_waste, phase_ms, is_intensity);

            out.insert(
                (source_addr, boon_id),
                GenerationStats {
                    self_pct, group_pct, squad_pct,
                    self_wasted, group_wasted, squad_wasted,
                },
            );
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
    let log_start_ms = raw.log_start_ms();
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
            in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    fn enc(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "20260114".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], tick_rate: None, started_at_unix: None,
        }
    }

    fn apply_ev(time: u64, buff_id: u32, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: duration_ms, buff_dmg: 0, overstack: 0,
            skillid: buff_id, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 1, result: 0, is_activation: 0, is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
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
