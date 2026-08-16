//! Buff apply/remove/initial event extraction (M3, Task 1).
//!
//! Field semantics verified against two sources (see individual doc
//! comments below, and `crate::evtc::sc::BUFF_INITIAL` /
//! `crate::evtc::buff_remove` for the full citations):
//! - the arcdps EVTC reference, hand-read from
//!   `curl https://www.deltaconnected.com/arcdps/evtc/README.txt` (never
//!   WebFetch'd -- project policy, fabricated content observed twice before).
//! - GW2EI source (`GW2EIEvtcParser/CombatItem.cs`,
//!   `GW2EIEvtcParser/ParsedData/CombatEvents/BuffEvents/**`), which is the
//!   arbiter for anything the arcdps reference leaves ambiguous (e.g. which
//!   of src/dst is the "remover" vs the buff owner on a removal event).
//!
//! This project's golden/calibration fixture is arcdps build 20260114,
//! which PREDATES GW2EI's `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges`
//! / `ResultEnumRework` threshold (`20260501`) -- see `sc::BUFF_INITIAL` for
//! the full version-split explanation. So the PRE-era branch below extracts
//! the OLDER shape: apply/remove are ordinary `is_statechange == 0` combat
//! events (flagged by `buff`/`is_buffremove`).
//!
//! M4 Task 1 adds the POST-era branch (`extract_buff_events_post_era`) for
//! builds `>= 20260501`, which decode the dedicated
//! `sc::BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`
//! statechanges the *current* (live, 2026-08) arcdps reference documents
//! for newer builds -- see those consts' doc comments in `crate::evtc::sc`
//! for the full payload verification, and `extract_buff_events`'s doc
//! comment for the era dispatch. Both branches produce the identical
//! `BuffEvent`/capacity output shape (see the era-equivalence tests at the
//! bottom of this module) so everything downstream (the simulator,
//! uptimes, generation) works unchanged regardless of era.

use crate::analysis::damage::InstidRegistry;
use crate::evtc::{buff_remove, iff, sc, RawLog};
use std::collections::BTreeSet;

/// One extracted apply/remove/initial event for a tracked boon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffEvent {
    pub time: u64,
    pub buff_id: u32,
    /// The agent that HOLDS the stack (buff owner/recipient), as a raw
    /// agent addr (not yet folded to an account representative -- see
    /// `super::simulate_boons`). For apply events this is the raw event's
    /// `dst_agent`; for removal events it is the raw event's `src_agent` --
    /// GW2EI's `AbstractBuffApplyEvent`/`AbstractBuffRemoveEvent`
    /// constructors resolve these OPPOSITELY (apply: `To (owner) =
    /// DstAgent`; remove: `To (owner) = SrcAgent`, `By (remover) =
    /// DstAgent`) -- `ParsedData/CombatEvents/BuffEvents/{BuffApplies,
    /// BuffRemoves}/Abstract*.cs`. This is the exact ambiguity the Task 1
    /// brief flagged ("verify which field is the remover vs owner").
    pub owner: u64,
    /// The other party: applier (apply events) or remover (removal
    /// events), master-resolved to the owning player when it's a pet/minion
    /// (via the shared `damage::InstidRegistry` -- the same time-aware
    /// instid->addr resolution `damage`/`cc` already use for pet-credit).
    /// Not consumed by `simulator`/`simulate_boons` in this task (the
    /// stack-count timeline only needs `owner`), but extracted now per the
    /// Task 1 brief's field-semantics verification scope, and to save a
    /// later "who generated this boon" task (M3) from re-deriving it.
    pub agent: u64,
    /// GW2EI's `BuffInstance` -- arcdps's per-stack "trackable id"
    /// (`RawEvent::pad`, the `pad61` field; `0` when the row carries none).
    /// Added by MBUFFSIM Task 2: `BuffsContainer.cs:206-210` groups a
    /// buff's events by `(To, BuffInstance)` to pair a removal with the
    /// apply it removes, which is what the `StackingConditionalLoss`
    /// `RemovedDuration` band aid needs (see
    /// [`apply_conditional_loss_band_aid`]).
    ///
    /// NOT consumed by `simulator::run` -- this project still uses the NoID
    /// simulator family exclusively, exactly as GW2EI does
    /// (`CombatData.cs:611`, `UseBuffInstanceSimulator = false`). The
    /// instance id is a PRE-PROCESSING input only.
    pub buff_instance: u32,
    pub kind: BuffEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffEventKind {
    /// A stack application (`buff == 1`) or pre-log-start stack
    /// (`is_statechange == BUFF_INITIAL`). `duration_ms` seeds the new
    /// stack's timer -- always the raw event's `value` field (verified:
    /// GW2EI's `BuffApplyEvent.AppliedDuration = evtcItem.Value` for BOTH
    /// regular apply and `Initial` dispatch; on a `BUFFINITIAL` row,
    /// `buff_dmg` instead carries the stack's *original* as-cast duration,
    /// used by GW2EI only for display/extension bookkeeping this task's
    /// simplified simulator doesn't model -- `ParsedData/CombatEvents/
    /// BuffEvents/BuffApplies/BuffApplyEvent.cs`).
    ///
    /// `is_shields` is the raw event's `is_shields` byte (`!= 0`), verified
    /// (Fix Round 1) against the arcdps reference comment on
    /// `CBTS_BUFFAPPLY`: "is_shields: non-zero if buff is active when
    /// applied", cross-checked against GW2EI's `BuffApplyEvent._addedActive
    /// = evtcItem.IsShields > 0`. For duration/Queue-type boons this
    /// decides whether the new stack becomes the immediately ACTIVE
    /// (ticking) one or joins the back of the FROZEN queue -- see
    /// `simulator::run_duration`. Has no effect for intensity boons
    /// (Might/Stability), which don't have an "active slot" concept.
    Apply { duration_ms: u32, is_shields: bool },
    /// `is_buffremove == SINGLE`: removes exactly one currently-held stack.
    /// `removed_duration_ms` is the raw event's `value` field -- GW2EI's
    /// default (non-instance) simulator matches it against each held
    /// stack's REMAINING duration at removal time (not the stack's
    /// originally applied duration), within a `15`ms tolerance
    /// (`ParserHelper.BuffSimulatorDelayConstant`) -- see `simulator.rs`.
    RemoveSingle { removed_duration_ms: u32 },
    /// `is_buffremove == ALL`: clears every currently-held stack.
    RemoveAll,
    /// An apply-shaped event (same `IsBuffApplyEvent` predicate as `Apply`)
    /// with `is_offcycle != 0` (M3 Task 2). Verified against GW2EI's
    /// `CombatEventFactory.AddBuffApplyEvent`, pre-
    /// `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges` branch: `if
    /// (buffEvent.IsOffcycle > 0) { ... new BuffExtensionEvent(...) } else
    /// { ... new BuffApplyEvent(...) }` -- this fixture's build (20260114)
    /// takes this branch. EXTENDS an already-active stack's remaining
    /// duration in place (or, if none is active/at capacity, becomes a
    /// fresh active stack) rather than pushing a new queued stack --
    /// `EIData/Buffs/BuffSimulators/BuffSimulatorNoID/
    /// {BuffSimulatorDuration,BuffSimulatorIntensity}.cs`'s `Extend`
    /// overrides.
    ///
    /// `extended_ms` is the raw event's `value` field
    /// (`BuffExtensionEvent.ExtendedDuration = Math.Max(evtcItem.Value,
    /// 0)`), `new_duration_ms` is the raw event's `overstack` field
    /// (`BuffExtensionEvent.NewDuration = evtcItem.OverstackValue`) --
    /// `ParsedData/CombatEvents/BuffEvents/BuffApplies/
    /// BuffExtensionEvent.cs`. GW2EI additionally runs a per-`BuffInstance`
    /// wall-clock correction (`CombatData.OffsetBuffExtensionEvents` /
    /// `BuffExtensionEvent.OffsetNewDuration`) that adjusts these two
    /// values before simulating; this project does not implement that
    /// correction (MBUFFSIM Task 1 measured its cost as below the noise
    /// floor -- 160 extension events over all 14 calibrated buffs in the
    /// reference capture -- and Task 2 left it DEFERRED), so
    /// `simulator::run` consumes the RAW `extended_ms`/`new_duration_ms`
    /// values directly. The `BuffInstance` (`pad`) field the correction
    /// keys on IS now decoded ([`BuffEvent::buff_instance`], MBUFFSIM Task
    /// 2) -- implementing `OffsetNewDuration` is no longer blocked on the
    /// wire, only unprioritised.
    Extend { extended_ms: u32, new_duration_ms: u32 },
}

/// Extracts apply/remove/initial events for exactly the `boon_ids` skill
/// ids (M3 Task 1 scope: only the 12 tracked boons -- see
/// `super::BOON_IDS`), in event order. `Manual` removals
/// (`buff_remove::MANUAL`) are intentionally NOT extracted -- GW2EI's
/// `BuffRemoveManualEvent` excludes them from the simulator entirely (see
/// `crate::evtc::buff_remove::MANUAL` docs).
///
/// Era dispatch (M4 Task 1): on arcdps builds >= 20260501
/// (`raw.header.is_post_buff_rework()`), buff apply/extend/remove events
/// arrive as their own dedicated `is_statechange` values
/// (`sc::BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`)
/// rather than the older `is_statechange == 0` combat-event shape below --
/// see those consts' doc comments in `crate::evtc::sc` for full
/// payload/field-role verification. The PRE-ERA BRANCH BELOW IS
/// BYTE-IDENTICAL TO BEFORE M4 (calibrated; do not edit its logic) --
/// `extract_buff_events_post_era` is a fully separate function producing
/// the same `BuffEvent` output shape from the post-era wire format.
pub fn extract_buff_events(raw: &RawLog, boon_ids: &BTreeSet<u32>) -> Vec<BuffEvent> {
    extract_buff_events_with_registry(raw, &InstidRegistry::build(raw), boon_ids)
}

/// GW2EI's `BuffRemoveSingleEvent.OverstackOrNaturalEnd` (MBUFFSIM Task 2,
/// rule 1) -- `GW2EIEvtcParser/ParsedData/CombatEvents/BuffEvents/
/// BuffRemoves/BuffRemoveSingleEvent.cs`:
///
/// ```csharp
/// :11   internal bool OverstackOrNaturalEnd =>
///           (IFF == IFF.Unknown && CreditedBy.IsUnknown && !_byShouldntBeUnknown);
/// :16   _byShouldntBeUnknown = evtcItem.DstAgent != 0;   // ctor
/// :26-38 internal override bool IsBuffSimulatorCompliant(bool useBuffInstanceSimulator) {
///           if (!base.IsBuffSimulatorCompliant(...)) return false;
///           if (useBuffInstanceSimulator) return true;
///           return !OverstackOrNaturalEnd;
///        }
/// ```
///
/// and `GW2EIEvtcParser/EIData/Buffs/BuffDictionary.cs:83-86` drops any
/// non-compliant event before it ever reaches a simulator. The
/// `useBuffInstanceSimulator` escape hatch is unreachable: GW2EI hard-codes
/// `UseBuffInstanceSimulator = false` (`ParsedData/CombatData.cs:611`), so
/// the NoID family -- and this predicate -- is always the arbiter.
///
/// **Three conjuncts reduce to two.** `CreditedBy` derives from `By =
/// agentData.GetAgent(evtcItem.DstAgent, ...)`
/// (`AbstractBuffRemoveEvent.cs:72`), and `GetAgent(0, _)` is exactly what
/// yields the unknown agent -- so `CreditedBy.IsUnknown` is IMPLIED by
/// `!_byShouldntBeUnknown` (`dst_agent == 0`), not merely correlated with
/// it. (The converse doesn't hold -- a nonzero addr that isn't in the agent
/// pool also resolves to unknown -- which is precisely why GW2EI carries
/// `_byShouldntBeUnknown` separately, per its own ctor comment: "Sometimes
/// there is a dstAgent value but the agent itself is not in the pool, such
/// cases should not trigger _overstackOrNaturalEnd".) So the pair below is
/// EQUIVALENT to the C#, not just sufficient.
///
/// Semantically: such a row is arcdps REPORTING that a stack ended on its
/// own (natural expiry) or was overstacked by a re-application -- nothing
/// stripped it. The simulator already models that expiry from the apply's
/// own duration, so replaying the row would strip a stack twice. On the
/// post-era WvW reference capture, 51863 of 52116 (99.5%) of SINGLE
/// removals over the calibrated buffs are of this kind; feeding them to the
/// simulator was the entire cause of MBUFFSIM's classes A, C and D (see
/// `.superpowers/sdd/2026-08-09-mbuffsim/task-1-report.md`).
fn is_overstack_or_natural_end(e: &crate::evtc::RawEvent) -> bool {
    e.dst_agent == 0 && e.iff == iff::UNKNOWN
}

/// GW2EI's old-format `CombatItem.IsBuffApplyEvent` predicate: the shape a
/// PRE-era (`is_statechange == 0`) apply-or-extension row has, before
/// `is_offcycle` routes it to one or the other
/// (`CombatEventFactory.AddBuffApplyEvent`'s pre-
/// `BuffAppliesAndRemovesAsStateChanges` branch). Factored out in MBUFFSIM
/// Task 3 because [`apply_conditional_loss_band_aid`] has to re-derive the
/// same classification from raw rows, and two hand-copied six-clause
/// predicates that must agree is a defect waiting to happen. Behaviourally
/// inert: this is the extractor's own condition verbatim, and the era
/// -equivalence tests below pin both callers.
///
/// Widened to `pub(crate)` for MPROC: `analysis::instant_cast` classifies
/// buff rows itself rather than consuming [`BuffEvent`]s, because the
/// instant-cast finders need two facts this module's output drops --
/// whether an apply was `Initial` (`BuffGainCastFinder` excludes those),
/// and the RAW, un-master-resolved applier. That makes it a third caller
/// of this same classification, which is precisely the duplication the
/// comment above says to avoid.
pub(crate) fn is_pre_era_apply_shaped(e: &crate::evtc::RawEvent) -> bool {
    e.is_statechange == 0
        && e.buff != 0
        && e.buff_dmg == 0
        && e.value > 0
        && e.is_activation == 0
        && e.is_buffremove == buff_remove::NONE
}

/// [`extract_buff_events`] against a caller-supplied, already-built
/// [`InstidRegistry`] (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. Both era branches take the same registry: `InstidRegistry::
/// build` is era-agnostic (it scans `src_instid`/`dst_instid`/`*_agent` on
/// every non-extension row regardless of `is_statechange`), so the pre- and
/// post-era extractors were already building bit-identical maps. The
/// `raw`-only wrapper ([`extract_buff_events`]) stays for standalone/test
/// callers.
pub fn extract_buff_events_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    boon_ids: &BTreeSet<u32>,
) -> Vec<BuffEvent> {
    if raw.header.is_post_buff_rework() {
        let mut out = extract_buff_events_post_era(raw, registry, boon_ids);
        apply_conditional_loss_band_aid(raw, boon_ids, &mut out);
        return out;
    }
    // Master-resolve a (possibly-pet) source addr via its instid's master
    // instid, mirroring `damage::pet_credit_events`'s owner resolution
    // exactly: `*_master_instid != 0` means the acting agent is a
    // pet/minion, and the registry maps that master instid back to the
    // owning player's addr at this event's own time.
    let resolve_agent = |addr: u64, master_instid: u16, time: u64| -> u64 {
        if master_instid != 0 {
            registry.resolve_at(master_instid, time).unwrap_or(addr)
        } else {
            addr
        }
    };

    let mut out = Vec::new();
    for e in &raw.events {
        if !boon_ids.contains(&e.skillid) {
            continue;
        }
        // Pre-log-start stack (verified `sc::BUFF_INITIAL` docs): same
        // src=applier/dst=owner roles as a regular apply, `value` seeds the
        // stack duration the same way.
        if e.is_statechange == sc::BUFF_INITIAL {
            out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::Apply {
                    duration_ms: e.value.max(0) as u32,
                    is_shields: e.is_shields != 0,
                },
            });
            continue;
        }
        if e.is_statechange != 0 {
            continue;
        }
        // Regular apply (verified `CombatItem.IsBuffApplyEvent` old-format
        // predicate, GW2EI `CombatItem.cs`): a buff-flagged combat event
        // carrying a positive duration in `value` and zero `buff_dmg`
        // (which distinguishes it from a buff-damage-tick event, where
        // `buff_dmg` carries the tick damage and `value == 0` --
        // `IsBuffDamageEvent`), with no activation and no buffremove flag
        // set. Among these, `is_offcycle != 0` further routes to `Extend`
        // instead of `Apply` -- see `BuffEventKind::Extend`'s doc comment
        // (`CombatEventFactory.AddBuffApplyEvent`'s pre-
        // `BuffAppliesAndRemovesAsStateChanges` branch). BUFFINITIAL rows
        // (handled above) never take this branch -- GW2EI's
        // `AddBuffApplyEvent`/its `is_offcycle` routing is only reached
        // from `combatItem.IsBuffApplyEvent()`, gated on `IsStateChange ==
        // Combat` (i.e. `is_statechange == 0`), not from the separate
        // `BuffInitial` statechange dispatch.
        if is_pre_era_apply_shaped(e) {
            if e.is_offcycle != 0 {
                out.push(BuffEvent {
                    time: e.time,
                    buff_id: e.skillid,
                    owner: e.dst_agent,
                    agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                    buff_instance: e.pad,
                    kind: BuffEventKind::Extend {
                        extended_ms: e.value.max(0) as u32,
                        new_duration_ms: e.overstack,
                    },
                });
                continue;
            }
            out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::Apply {
                    duration_ms: e.value as u32,
                    is_shields: e.is_shields != 0,
                },
            });
            continue;
        }
        // Removal (verified `CombatItem.IsBuffRemoveEvent` old-format
        // predicate): any `is_buffremove != NONE` combat event with no
        // activation flag. Field roles verified against GW2EI's
        // `AbstractBuffRemoveEvent` ctor: `By (remover) = DstAgent`, `To
        // (owner) = SrcAgent` -- the OPPOSITE of apply events.
        if e.is_activation == 0 && e.is_buffremove != buff_remove::NONE {
            let agent = resolve_agent(e.dst_agent, e.dst_master_instid, e.time);
            match e.is_buffremove {
                buff_remove::ALL => out.push(BuffEvent {
                    time: e.time,
                    buff_id: e.skillid,
                    owner: e.src_agent,
                    agent,
                    buff_instance: e.pad,
                    kind: BuffEventKind::RemoveAll,
                }),
                // MBUFFSIM Task 2, rule 1: an OverstackOrNaturalEnd row is
                // arcdps reporting an expiry the simulator already models,
                // and GW2EI never feeds it to a simulator -- see
                // `is_overstack_or_natural_end`.
                buff_remove::SINGLE if !is_overstack_or_natural_end(e) => out.push(BuffEvent {
                    time: e.time,
                    buff_id: e.skillid,
                    owner: e.src_agent,
                    agent,
                    buff_instance: e.pad,
                    kind: BuffEventKind::RemoveSingle { removed_duration_ms: e.value.max(0) as u32 },
                }),
                // MANUAL or unknown: not simulator-compliant, skip (see
                // `buff_remove::MANUAL` docs). Same for the
                // OverstackOrNaturalEnd SINGLE rows the arm above rejects.
                _ => {}
            }
        }
    }
    apply_conditional_loss_band_aid(raw, boon_ids, &mut out);
    out
}

/// GW2EI's "Band aid for the stack type situation with fake
/// inactive/infinite durations" (MBUFFSIM Task 2, rule 2) --
/// `GW2EIEvtcParser/EIData/Buffs/BuffsContainer.cs:196-252`, ported whole.
///
/// On a `StackingConditionalLoss` strip, arcdps sometimes reports the
/// stack's ORIGINAL applied duration in `value` instead of its REMAINING
/// duration. `simulator::find_single_removal_match` compares that value
/// against each held stack's remaining duration within a strict 15ms
/// tolerance, so the raw value matches nothing and the stack is never
/// removed -- Stability then sits systematically HIGH. GW2EI detects the
/// case (the reported value equals the stack's reconstructed total) and
/// rewrites the value to the remaining duration BEFORE any simulator runs.
///
/// The C#, and every gate this transcribes:
///
/// ```csharp
/// // :197  the HasStackIDs precondition (CombatData.cs:610):
/// //         evtcVersion.Build > ArcDPSBuilds.ProperConfusionDamageSimulation
/// //         && buffEvents.Any(x => x is BuffStackActiveEvent || x is BuffStackDeactiveEvent)
/// if (combatData.HasStackIDs) {
///   var stackTypeBuffs = currentBuffs.Where(x =>
///       x.StackType == BuffStackType.StackingConditionalLoss ||
///       x.StackType == BuffStackType.Stacking);                       // :199
///   foreach (Buff buff in stackTypeBuffs) {
///     // :202  the PER-BUFF precondition: at least one qualifying removal
///     if (buffData.OfType<BuffRemoveSingleEvent>().Any(x => !x.OverstackOrNaturalEnd
///           && (buff.StackType == BuffStackType.StackingConditionalLoss
///               || x.RemovedDuration == int.MaxValue))) {
///       foreach (var group in buffData.GroupBy(x => x.To)) {           // :204
///         ... GroupBy(BuffInstance) ...                                // :206-210
///         BuffApplyEvent? apply = applyList.LastOrDefault(x => x.Time <= remove.Time);
///         var totalDuration = apply.OriginalAppliedDuration;           // :219
///         var previousTime  = apply.Time;
///         foreach (var other in others) {                              // :222-239
///           if (other.Time >= apply.Time && other.Time <= remove.Time) {
///             if (other is BuffExtensionEvent bee)   totalDuration += bee.ExtendedDuration;
///             else if (other is BuffStackActiveEvent) totalDuration -= (other.Time - previousTime);
///           }
///           previousTime = other.Time;   // NOTE: updated for EVERY other,
///         }                              // including ones outside the window
///         if (totalDuration == remove.RemovedDuration) {               // :241
///           int activeTime  = apply.OriginalAppliedDuration - apply.AppliedDuration;
///           int elapsedTime = (int)(remove.Time - apply.Time);
///           remove.OverrideRemovedDuration(remove.RemovedDuration - activeTime - elapsedTime);
///         }                              // OverrideRemovedDuration clamps
///       }                                // with Math.Max(x, 0)
///     }                                  // (BuffRemoveSingleEvent.cs:40-43)
///   }
/// }
/// ```
///
/// Every removal reaching here is already non-`OverstackOrNaturalEnd` --
/// [`is_overstack_or_natural_end`] dropped the rest during extraction --
/// so the `!x.OverstackOrNaturalEnd` conjunct is satisfied by construction
/// and is not re-tested.
///
/// **One documented deviation from "ported whole".** GW2EI runs this over
/// `BuffExtensionEvent`s whose `ExtendedDuration` has already been rewritten
/// by `CombatData.OffsetBuffExtensionEvents`
/// (`ParsedData/CombatData.cs:464-525`); this project consumes the RAW
/// `value` because `OffsetNewDuration` is a deliberate, separately-ledgered
/// deferral (see [`BuffEventKind::Extend`]). It can therefore only matter
/// for a removal whose `totalDuration` reconstruction crosses an extension
/// event, of which the reference capture has 5 for Stability across the
/// whole log -- and the calibration below is measured WITH that deviation in
/// place, so it is bounded, not merely argued. Every other clause of
/// `BuffsContainer.cs:196-252` is transcribed exactly.
///
/// Measured on the post-era WvW reference capture: 181 of Stability's 253
/// real SINGLE removals hit the rewrite; the mean per-account average-stack
/// error against GW2EI drops from 0.04124 to 0.00027.
fn apply_conditional_loss_band_aid(raw: &RawLog, boon_ids: &BTreeSet<u32>, out: &mut [BuffEvent]) {
    // `CombatData.HasStackIDs` (`ParsedData/CombatData.cs:610`). GW2EI's
    // `buffEvents` here is the WHOLE log's buff-event list, not a per-buff
    // slice, so the scan is over every row rather than `boon_ids`.
    if !raw.header.has_proper_confusion_damage_simulation() {
        return;
    }
    let has_stack_ids = raw
        .events
        .iter()
        .any(|e| e.is_statechange == sc::STACK_ACTIVE || e.is_statechange == sc::STACK_DEACTIVE);
    if !has_stack_ids {
        return;
    }

    // `BuffApplyEvent.OriginalAppliedDuration` (`BuffApplyEvent.cs:21-28`):
    // `buff_dmg` on a BUFF_INITIAL row from this build onward, else `value`.
    let initial_uses_buff_dmg = raw.header.has_buff_extension_overstack_value_changed();

    // Per `(buff, owner, instance)`: the applies, and the "others"
    // (extensions + stack-active/deactive) the `totalDuration`
    // reconstruction walks. Built from RAW rows because two of the inputs --
    // a BUFF_INITIAL row's `buff_dmg`, and the stack-active timestamps --
    // are not carried on `BuffEvent`.
    type Key = (u32, u64, u32);
    #[derive(Clone, Copy)]
    struct Apply {
        time: u64,
        /// `AppliedDuration` (`evtcItem.Value`).
        applied: i64,
        /// `OriginalAppliedDuration`.
        original: i64,
    }
    /// `others`: `true` == `BuffExtensionEvent` (carrying
    /// `ExtendedDuration`), `false` == `BuffStackActiveEvent`.
    #[derive(Clone, Copy)]
    struct Other {
        time: u64,
        extension_ms: Option<i64>,
    }
    let mut applies: std::collections::BTreeMap<Key, Vec<Apply>> = std::collections::BTreeMap::new();
    let mut others: std::collections::BTreeMap<Key, Vec<Other>> = std::collections::BTreeMap::new();

    let post = raw.header.is_post_buff_rework();
    for e in &raw.events {
        if !boon_ids.contains(&e.skillid) {
            continue;
        }
        // Only the intensity stack types the band aid's `stackTypeBuffs`
        // filter keeps (`BuffsContainer.cs:198-199`) are worth indexing.
        let Some(st) = super::stack_type_for(e.skillid) else { continue };
        if st.band_aid_scope().is_none() {
            continue;
        }
        let initial = e.is_statechange == sc::BUFF_INITIAL;
        let is_apply = initial
            || if post { e.is_statechange == sc::BUFF_APPLY } else { is_pre_era_apply_shaped(e) && e.is_offcycle == 0 };
        if is_apply {
            let applied = i64::from(e.value);
            let original =
                if initial && initial_uses_buff_dmg { i64::from(e.buff_dmg) } else { applied };
            applies
                .entry((e.skillid, e.dst_agent, e.pad))
                .or_default()
                .push(Apply { time: e.time, applied, original });
            continue;
        }
        let is_extension = if post {
            e.is_statechange == sc::BUFF_CHANGE
        } else {
            is_pre_era_apply_shaped(e) && e.is_offcycle != 0
        };
        if is_extension {
            others
                .entry((e.skillid, e.dst_agent, e.pad))
                .or_default()
                .push(Other { time: e.time, extension_ms: Some(i64::from(e.value.max(0))) });
        } else if e.is_statechange == sc::STACK_ACTIVE {
            // `BuffStackActiveEvent.BuffInstance = (uint)evtcItem.DstAgent`
            // (`BuffStackActiveEvent.cs:10`) -- NOT `pad`, unlike every
            // other buff event. Its owner is `src_agent` (`BuffStackEvent`
            // inherits `AbstractBuffEvent`'s `To = SrcAgent` for
            // non-apply rows).
            others
                .entry((e.skillid, e.src_agent, e.dst_agent as u32))
                .or_default()
                .push(Other { time: e.time, extension_ms: None });
        } else if e.is_statechange == sc::STACK_DEACTIVE {
            // Indexed so it advances `previousTime` exactly as GW2EI's
            // `others` list does, but contributes no `totalDuration` term
            // (`BuffsContainer.cs:227-238` only branches on
            // `BuffExtensionEvent` and `BuffStackActiveEvent`).
            others
                .entry((e.skillid, e.src_agent, e.pad))
                .or_default()
                .push(Other { time: e.time, extension_ms: None });
        }
    }
    if applies.is_empty() {
        return;
    }
    for v in applies.values_mut() {
        v.sort_by_key(|a| a.time);
    }
    for v in others.values_mut() {
        v.sort_by_key(|o| o.time);
    }

    // `BuffsContainer.cs:202` -- the per-buff precondition. A buff with no
    // qualifying removal is skipped entirely, so a `Stacking` buff whose
    // removals all report finite durations never enters the rewrite.
    let mut qualifies: std::collections::BTreeMap<u32, bool> = std::collections::BTreeMap::new();
    for ev in out.iter() {
        let BuffEventKind::RemoveSingle { removed_duration_ms } = ev.kind else { continue };
        let Some(scope) = super::stack_type_for(ev.buff_id).and_then(|st| st.band_aid_scope())
        else {
            continue;
        };
        let q = scope == super::BandAidScope::EveryRealRemoval
            || removed_duration_ms == super::INFINITE_REMOVED_DURATION_MS;
        *qualifies.entry(ev.buff_id).or_default() |= q;
    }

    for ev in out.iter_mut() {
        let BuffEventKind::RemoveSingle { removed_duration_ms } = ev.kind else { continue };
        let Some(scope) = super::stack_type_for(ev.buff_id).and_then(|st| st.band_aid_scope())
        else {
            continue;
        };
        if !qualifies.get(&ev.buff_id).copied().unwrap_or(false) {
            continue;
        }
        // The same disjunct again, now per-removal
        // (`BuffsContainer.cs:210`'s `removeSinglesPerInstanceID` filter).
        if scope == super::BandAidScope::OnlyInfiniteRemovedDuration
            && removed_duration_ms != super::INFINITE_REMOVED_DURATION_MS
        {
            continue;
        }
        let key = (ev.buff_id, ev.owner, ev.buff_instance);
        let Some(list) = applies.get(&key) else { continue };
        let Some(apply) = list.iter().rev().find(|a| a.time <= ev.time) else { continue };

        let mut total_duration = apply.original;
        let mut previous_time = apply.time;
        for o in others.get(&key).map(|v| v.as_slice()).unwrap_or(&[]) {
            if o.time >= apply.time && o.time <= ev.time {
                match o.extension_ms {
                    Some(ext) => total_duration += ext,
                    None => total_duration -= o.time as i64 - previous_time as i64,
                }
            }
            // Deliberately OUTSIDE the window check -- see the C# above.
            previous_time = o.time;
        }
        if total_duration != i64::from(removed_duration_ms) {
            continue;
        }
        let active_time = apply.original - apply.applied;
        let elapsed = ev.time as i64 - apply.time as i64;
        // `OverrideRemovedDuration`'s `Math.Max(removedDuration, 0)`
        // (`BuffRemoveSingleEvent.cs:40-43`).
        let rewritten = (i64::from(removed_duration_ms) - active_time - elapsed).max(0);
        ev.kind = BuffEventKind::RemoveSingle { removed_duration_ms: rewritten as u32 };
    }
}

/// Post-era (arcdps >= 20260501) twin of the pre-era loop above --
/// dispatches on the dedicated buff statechanges instead of the
/// `is_statechange == 0` combat-event shape. Produces the identical
/// `BuffEvent` stream a pre-era log carrying the same logical
/// apply/extend/remove sequence would produce (see the era-equivalence
/// tests below) -- see `crate::evtc::sc::BUFF_APPLY` /
/// `BUFF_CHANGE` / `BUFF_REMOVE_SINGLE` / `BUFF_REMOVE_ALL` for the full
/// verified payload/field-role citations this function's branches are
/// built from.
fn extract_buff_events_post_era(
    raw: &RawLog,
    registry: &InstidRegistry,
    boon_ids: &BTreeSet<u32>,
) -> Vec<BuffEvent> {
    // Same master-resolution helper as the pre-era loop (see its doc
    // comment above) -- the instid/master-instid byte offsets are
    // unconditional on the wire (decoded for every event regardless of
    // `is_statechange`), so no era-specific change is needed here.
    let resolve_agent = |addr: u64, master_instid: u16, time: u64| -> u64 {
        if master_instid != 0 {
            registry.resolve_at(master_instid, time).unwrap_or(addr)
        } else {
            addr
        }
    };

    let mut out = Vec::new();
    for e in &raw.events {
        if !boon_ids.contains(&e.skillid) {
            continue;
        }
        match e.is_statechange {
            // Pre-existing stacks at log start -- same statechange ordinal
            // (18) and same src=applier/dst=owner roles in both eras (see
            // `sc::BUFF_INITIAL` docs). Included in this era's dispatch
            // because it is unaffected by the rework threshold.
            sc::BUFF_INITIAL => out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::Apply {
                    duration_ms: e.value.max(0) as u32,
                    is_shields: e.is_shields != 0,
                },
            }),
            // `sc::BUFF_APPLY` doc comment: owner = dst_agent, applier =
            // src_agent (same roles as the pre-era `buff == 1` shape, via
            // GW2EI's shared `AbstractBuffApplyEvent` ctor). `is_offcycle`
            // is NOT consulted here -- post-era routes extensions through
            // the dedicated `BUFF_CHANGE` statechange instead.
            sc::BUFF_APPLY => out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::Apply {
                    duration_ms: e.value.max(0) as u32,
                    is_shields: e.is_shields != 0,
                },
            }),
            // `sc::BUFF_CHANGE` doc comment: same owner/applier roles as
            // apply (shared ctor), `extended_ms` from `value` (clamped),
            // `new_duration_ms` from `overstack` -- identical field
            // mapping to the pre-era `is_offcycle`-flagged `Extend`.
            sc::BUFF_CHANGE => out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::Extend {
                    extended_ms: e.value.max(0) as u32,
                    new_duration_ms: e.overstack,
                },
            }),
            // `sc::BUFF_REMOVE_SINGLE` doc comment: this ordinal carries
            // BOTH Single and Manual removals, disambiguated by
            // `is_buffremove` (same enum as the pre-era shape); unlike
            // pre-era, no `is_activation == 0` gate applies post-era.
            // Owner = src_agent, remover = dst_agent (opposite of apply,
            // same as pre-era, via the shared `AbstractBuffRemoveEvent`
            // ctor).
            // Kept as an inner `if` rather than the match guard clippy
            // suggests. The collapse IS semantically safe -- the only
            // sibling arm keys off a different statechange and the
            // fallthrough is `_ => {}` -- but a guard would move this
            // condition up into the pattern and orphan the "Manual (or
            // unknown/None)" comment below from the branch it explains.
            #[allow(clippy::collapsible_match)]
            sc::BUFF_REMOVE_SINGLE => {
                // MBUFFSIM Task 2, rule 1: same `OverstackOrNaturalEnd`
                // drop as the pre-era branch -- GW2EI's
                // `BuffRemoveSingleEvent` is era-agnostic (the era split in
                // `CombatEventFactory.AddBuffRemoveEvent` only changes WHICH
                // rows become one), so the filter belongs on both branches.
                if e.is_buffremove == buff_remove::SINGLE
                    && !is_overstack_or_natural_end(e)
                {
                    out.push(BuffEvent {
                        time: e.time,
                        buff_id: e.skillid,
                        owner: e.src_agent,
                        agent: resolve_agent(e.dst_agent, e.dst_master_instid, e.time),
                        buff_instance: e.pad,
                        kind: BuffEventKind::RemoveSingle { removed_duration_ms: e.value.max(0) as u32 },
                    });
                }
                // Manual (or unknown/None): not simulator-compliant, skip
                // -- see `buff_remove::MANUAL` docs.
            }
            // `sc::BUFF_REMOVE_ALL` doc comment: unconditional (no
            // `is_buffremove` check) -- owner = src_agent, remover =
            // dst_agent, same roles as SINGLE removal.
            sc::BUFF_REMOVE_ALL => out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.src_agent,
                agent: resolve_agent(e.dst_agent, e.dst_master_instid, e.time),
                buff_instance: e.pad,
                kind: BuffEventKind::RemoveAll,
            }),
            _ => {}
        }
    }
    out
}

/// Extracts arcdps's own per-buff stack-capacity report (M3 Task 2) for
/// exactly the `boon_ids` skill ids -- `CBTS_BUFFINFO` (`sc::BUFF_INFO`)
/// rows' `src_master_instid` field (GW2EI's `BuffInfoEvent.MaxStacks`).
/// **Load-bearing** (see `sc::BUFF_INFO`'s doc comment): GW2EI's
/// `Buff.CreateSimulator` uses this arcdps-reported value as the REAL
/// simulator capacity whenever it's present and `> 0`, in preference to
/// its own hardcoded `CommonBuffs` table default -- so `simulate_boons`
/// must do the same rather than trusting `simulator::capacity_for`
/// unconditionally. Returns 0 for a buff id with no `BUFFINFO` row (or
/// whose reported `src_master_instid` is 0) -- callers should treat 0 as
/// "no override, use the hardcoded default" per GW2EI's own `> 0` guard.
/// If a buff id has multiple `BUFFINFO` rows (shouldn't normally happen --
/// arcdps documents one per tracked skill id per log), the LAST one wins
/// (plain overwrite on repeated inserts), mirroring `Dictionary`-style
/// last-write-wins semantics GW2EI's own single-event-per-id model doesn't
/// need to disambiguate.
/// Sane upper bound on a `BUFFINFO` row's reported stack capacity
/// (final-review fix wave). `src_master_instid` is a raw `u16` (up to
/// 65535), but arcdps only ever legitimately reports small per-buff
/// capacities in practice -- the highest observed across this project's own
/// calibration fixtures and GW2EI's hardcoded `CommonBuffs` defaults is `99`
/// (several Queue-type boons genuinely report exactly 99, see
/// `extract_buff_capacities`'s doc comment above), so anything higher is
/// treated as a garbled/corrupt row rather than trusted verbatim -- clamped
/// down to this ceiling instead of silently feeding an implausible capacity
/// (e.g. 65535) into `simulator::run`.
const MAX_BUFF_CAPACITY: u32 = 99;

/// No era dispatch needed (M4 Task 1, verified): `CBTS_BUFFINFO`
/// (`sc::BUFF_INFO`, ordinal 30) is untouched by
/// `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges` (20260501) -- GW2EI's
/// `BuffInfoEvent.BuildFromBuffInfo`
/// (`GW2EIEvtcParser/ParsedData/CombatEvents/MetaDataEvents/Info/
/// BuffInfoEvent.cs`) reads `MaxStacks = evtcItem.SrcMasterInstid`
/// unconditionally, gated only on the unrelated, much-earlier
/// `ArcDPSBuilds.BuffAttrFlatIncRemoved` threshold (which affects only the
/// `StackingTypeByte`/`Pad1` field this project doesn't decode). So the
/// same single code path below is already correct for both eras.
pub fn extract_buff_capacities(raw: &RawLog, boon_ids: &BTreeSet<u32>) -> std::collections::BTreeMap<u32, u32> {
    let mut out = std::collections::BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange == sc::BUFF_INFO && boon_ids.contains(&e.skillid) {
            let max_stacks = (e.src_master_instid as u32).min(MAX_BUFF_CAPACITY);
            if max_stacks > 0 {
                out.insert(e.skillid, max_stacks);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};

    const MIGHT: u32 = 740;

    fn boon_ids() -> BTreeSet<u32> {
        [MIGHT].into_iter().collect()
    }

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: MIGHT, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// Apply events: owner = dst_agent (recipient), agent = src_agent
    /// (applier). If this were backwards, every downstream stack machine
    /// would attribute Might to the applier instead of the recipient.
    #[test]
    fn apply_event_owner_is_dst_not_src() {
        let mut e = base_event();
        e.time = 100;
        e.src_agent = 0xA; // applier
        e.dst_agent = 0xB; // recipient
        e.buff = 1;
        e.value = 5000; // duration ms
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].owner, 0xB, "owner must be dst_agent (recipient), not src_agent");
        assert_eq!(events[0].agent, 0xA, "agent must be src_agent (applier)");
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 5000, is_shields: false });
    }

    /// **Fix Round 1**: `is_shields` on the raw event must round-trip into
    /// `BuffEventKind::Apply.is_shields`.
    #[test]
    fn apply_event_carries_is_shields_flag() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.buff = 1;
        e.value = 5000;
        e.is_shields = 1;
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 5000, is_shields: true });
    }

    /// M3 Task 2: an apply-shaped event with `is_offcycle != 0` must route
    /// to `Extend`, not `Apply` -- `extended_ms` from `value`,
    /// `new_duration_ms` from `overstack` (verified: `BuffExtensionEvent`
    /// ctor, `ExtendedDuration = Math.Max(evtcItem.Value, 0)`, `NewDuration
    /// = evtcItem.OverstackValue`).
    #[test]
    fn offcycle_apply_routes_to_extend_using_value_and_overstack() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.buff = 1;
        e.value = 1500; // extended_ms
        e.overstack = 4000; // new_duration_ms
        e.is_offcycle = 1;
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].owner, 0xB);
        assert_eq!(events[0].agent, 0xA);
        assert_eq!(
            events[0].kind,
            BuffEventKind::Extend { extended_ms: 1500, new_duration_ms: 4000 }
        );
    }

    /// BUFFINITIAL rows never take the `is_offcycle` extension branch --
    /// GW2EI's `is_offcycle` routing only applies within
    /// `AddBuffApplyEvent`, reached from ordinary `is_statechange == 0`
    /// apply rows, not the separate `BuffInitial` statechange dispatch.
    #[test]
    fn buff_initial_ignores_is_offcycle_stays_apply() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.is_statechange = sc::BUFF_INITIAL;
        e.value = 3000;
        e.is_offcycle = 1; // must be ignored for BUFFINITIAL rows
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 3000, is_shields: false });
    }

    /// The critical field-role check the Task 1 brief calls out by name:
    /// on a SINGLE removal event, `src_agent` is the buff OWNER and
    /// `dst_agent` is the REMOVER -- the opposite of apply events. A naive
    /// implementation that reused apply's src=applier/dst=owner convention
    /// here would attribute the removed stack to the wrong agent.
    #[test]
    fn remove_single_owner_is_src_not_dst() {
        let mut e = base_event();
        e.time = 200;
        e.src_agent = 0xC; // buff owner (had the stack removed)
        e.dst_agent = 0xD; // remover
        e.is_buffremove = buff_remove::SINGLE;
        e.value = 1234; // removed duration ms
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].owner, 0xC, "owner must be src_agent, not dst_agent");
        assert_eq!(events[0].agent, 0xD, "agent (remover) must be dst_agent");
        assert_eq!(
            events[0].kind,
            BuffEventKind::RemoveSingle { removed_duration_ms: 1234 }
        );
    }

    #[test]
    fn remove_all_extracted_with_swapped_roles() {
        let mut e = base_event();
        e.time = 300;
        e.src_agent = 0xC;
        e.dst_agent = 0xD;
        e.is_buffremove = buff_remove::ALL;
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].owner, 0xC);
        assert_eq!(events[0].kind, BuffEventKind::RemoveAll);
    }

    /// GW2EI's `BuffRemoveManualEvent` is excluded from the simulator
    /// entirely (`IsBuffSimulatorCompliant` false, `UpdateSimulator` a
    /// no-op) -- extraction must mirror that by not producing any event.
    #[test]
    fn manual_removal_not_extracted() {
        let mut e = base_event();
        e.src_agent = 0xC;
        e.dst_agent = 0xD;
        e.is_buffremove = buff_remove::MANUAL;
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert!(events.is_empty(), "manual removals must not be extracted");
    }

    /// `CBTS_BUFFINITIAL` (is_statechange == 18): pre-log-start stacks,
    /// same src/dst roles as apply, `value` (not `buff_dmg`) seeds the
    /// stack duration.
    #[test]
    fn buff_initial_extracted_as_apply_using_value_not_buff_dmg() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.is_statechange = sc::BUFF_INITIAL;
        e.value = 3000; // remaining duration -- seeds the stack
        e.buff_dmg = 9000; // original as-cast duration -- NOT used here
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].owner, 0xB);
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 3000, is_shields: false });
    }

    /// A buff-damage-tick event (condi damage) carries `buff == 1` but
    /// `value == 0` (damage is in `buff_dmg` instead) -- must NOT be
    /// misclassified as a stack application.
    #[test]
    fn buff_damage_tick_not_misclassified_as_apply() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.buff = 1;
        e.value = 0;
        e.buff_dmg = 250; // tick damage, not a duration
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert!(events.is_empty(), "buff damage ticks must not be extracted as applies");
    }

    #[test]
    fn non_tracked_skillid_skipped() {
        let mut e = base_event();
        e.skillid = 999_999; // not in boon_ids
        e.buff = 1;
        e.value = 5000;
        let events = extract_buff_events(&raw_from(vec![e]), &boon_ids());
        assert!(events.is_empty());
    }

    /// Pet-sourced apply: the applier's `src_agent` is a pet, whose
    /// `src_master_instid` points back to the owning player's instid.
    /// `agent` must resolve to the owning player, not the pet's own addr
    /// (mirrors `damage::pet_credit_events`'s owner resolution).
    #[test]
    fn apply_agent_master_resolves_pet_to_owner() {
        let mut seed = base_event();
        seed.time = 0;
        seed.src_agent = 1; // owner
        seed.src_instid = 11; // owner's instid, registered here
        seed.dst_agent = 9;

        let mut apply = base_event();
        apply.time = 100;
        apply.src_agent = 300; // pet's own addr
        apply.src_instid = 77;
        apply.src_master_instid = 11; // points back to owner's instid
        apply.dst_agent = 9;
        apply.buff = 1;
        apply.value = 5000;

        let events = extract_buff_events(&raw_from(vec![seed, apply]), &boon_ids());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].agent, 1, "pet applier must master-resolve to owner");
    }

    /// Final-review fix wave: a `BUFFINFO` row reporting an implausible
    /// capacity (`src_master_instid` up to `65535`, the raw field's full
    /// `u16` range) must be clamped down to `MAX_BUFF_CAPACITY` (99) rather
    /// than trusted verbatim.
    #[test]
    fn buff_info_capacity_clamps_to_max() {
        let mut e = base_event();
        e.is_statechange = sc::BUFF_INFO;
        e.src_master_instid = 65535;
        let caps = extract_buff_capacities(&raw_from(vec![e]), &boon_ids());
        assert_eq!(caps.get(&MIGHT), Some(&MAX_BUFF_CAPACITY));
        assert_eq!(caps.get(&MIGHT), Some(&99));
    }

    /// A plausible, already-in-range capacity must pass through unchanged.
    #[test]
    fn buff_info_capacity_within_range_unchanged() {
        let mut e = base_event();
        e.is_statechange = sc::BUFF_INFO;
        e.src_master_instid = 25;
        let caps = extract_buff_capacities(&raw_from(vec![e]), &boon_ids());
        assert_eq!(caps.get(&MIGHT), Some(&25));
    }
}

/// M4 Task 1 core deliverable: for every simulator-relevant scenario the
/// pre-era tests above cover, build a POST-era synthetic twin (same
/// logical event, decoded from the dedicated `sc::BUFF_APPLY`/
/// `BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL` statechanges
/// instead of the `is_statechange == 0` combat-event shape) and assert
/// `extract_buff_events`/`extract_buff_capacities` produce an IDENTICAL
/// `BuffEvent`/capacity stream regardless of era. A wrong payload-field
/// mapping in `extract_buff_events_post_era` (wrong owner/agent role, wrong
/// source field for a duration, a missing `is_buffremove` guard, etc.)
/// fails these by construction, since both sides are asserted equal, not
/// just individually plausible.
#[cfg(test)]
mod era_equivalence {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};

    const MIGHT: u32 = 740;

    fn boon_ids() -> BTreeSet<u32> {
        [MIGHT].into_iter().collect()
    }

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: MIGHT, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_pre(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn raw_post(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// Scenario: plain apply. Pre-era: `buff == 1` combat event. Post-era:
    /// dedicated `sc::BUFF_APPLY` statechange. Same owner/applier roles,
    /// same `value` -> `duration_ms` mapping.
    #[test]
    fn apply_is_era_equivalent() {
        let mut pre = base_event();
        pre.time = 100;
        pre.src_agent = 0xA;
        pre.dst_agent = 0xB;
        pre.buff = 1;
        pre.value = 5000;

        let mut post = base_event();
        post.time = 100;
        post.src_agent = 0xA;
        post.dst_agent = 0xB;
        post.is_statechange = sc::BUFF_APPLY;
        post.value = 5000;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent {
                time: 100,
                buff_id: MIGHT,
                owner: 0xB,
                agent: 0xA,
                buff_instance: 0,
                kind: BuffEventKind::Apply { duration_ms: 5000, is_shields: false },
            }]
        );
    }

    /// Scenario: `is_shields`-active apply (new stack becomes the
    /// immediately-active one rather than joining the queue). Pre-era:
    /// `buff == 1` combat event with `is_shields = 1`. Post-era:
    /// `sc::BUFF_APPLY` with the same `is_shields` byte -- unaffected by
    /// era (shared `BuffApplyEvent` ctor, see `sc::BUFF_APPLY` docs).
    #[test]
    fn is_shields_active_apply_is_era_equivalent() {
        let mut pre = base_event();
        pre.src_agent = 0xA;
        pre.dst_agent = 0xB;
        pre.buff = 1;
        pre.value = 5000;
        pre.is_shields = 1;

        let mut post = base_event();
        post.src_agent = 0xA;
        post.dst_agent = 0xB;
        post.is_statechange = sc::BUFF_APPLY;
        post.value = 5000;
        post.is_shields = 1;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent {
                time: 0,
                buff_id: MIGHT,
                owner: 0xB,
                agent: 0xA,
                buff_instance: 0,
                kind: BuffEventKind::Apply { duration_ms: 5000, is_shields: true },
            }]
        );
    }

    /// Scenario: apply-while-active queue extension. Pre-era: apply-shaped
    /// (`buff == 1`) combat event with `is_offcycle != 0`, routed to
    /// `Extend`. Post-era: dedicated `sc::BUFF_CHANGE` statechange (no
    /// `is_offcycle` involved at all -- see `sc::BUFF_CHANGE` docs). Same
    /// `value` -> `extended_ms`, `overstack` -> `new_duration_ms` mapping.
    #[test]
    fn apply_while_active_queue_extend_is_era_equivalent() {
        let mut pre = base_event();
        pre.src_agent = 0xA;
        pre.dst_agent = 0xB;
        pre.buff = 1;
        pre.value = 1500; // extended_ms
        pre.overstack = 4000; // new_duration_ms
        pre.is_offcycle = 1;

        let mut post = base_event();
        post.src_agent = 0xA;
        post.dst_agent = 0xB;
        post.is_statechange = sc::BUFF_CHANGE;
        post.value = 1500;
        post.overstack = 4000;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent {
                time: 0,
                buff_id: MIGHT,
                owner: 0xB,
                agent: 0xA,
                buff_instance: 0,
                kind: BuffEventKind::Extend { extended_ms: 1500, new_duration_ms: 4000 },
            }]
        );
    }

    /// Scenario: SINGLE removal. Pre-era: `is_buffremove == SINGLE` combat
    /// event. Post-era: dedicated `sc::BUFF_REMOVE_SINGLE` statechange
    /// with `is_buffremove == SINGLE` disambiguating it from Manual. Same
    /// owner=src_agent/remover=dst_agent roles (opposite of apply), same
    /// `value` -> `removed_duration_ms` mapping.
    #[test]
    fn remove_single_is_era_equivalent() {
        let mut pre = base_event();
        pre.time = 200;
        pre.src_agent = 0xC;
        pre.dst_agent = 0xD;
        pre.is_buffremove = buff_remove::SINGLE;
        pre.value = 1234;

        let mut post = base_event();
        post.time = 200;
        post.src_agent = 0xC;
        post.dst_agent = 0xD;
        post.is_statechange = sc::BUFF_REMOVE_SINGLE;
        post.is_buffremove = buff_remove::SINGLE;
        post.value = 1234;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent {
                time: 200,
                buff_id: MIGHT,
                owner: 0xC,
                agent: 0xD,
                buff_instance: 0,
                kind: BuffEventKind::RemoveSingle { removed_duration_ms: 1234 },
            }]
        );
    }

    /// Scenario: ALL clear. Pre-era: `is_buffremove == ALL` combat event.
    /// Post-era: dedicated `sc::BUFF_REMOVE_ALL` statechange (unconditional
    /// -- no `is_buffremove` check per `sc::BUFF_REMOVE_ALL` docs). Same
    /// owner/remover roles as SINGLE.
    #[test]
    fn remove_all_is_era_equivalent() {
        let mut pre = base_event();
        pre.time = 300;
        pre.src_agent = 0xC;
        pre.dst_agent = 0xD;
        pre.is_buffremove = buff_remove::ALL;

        let mut post = base_event();
        post.time = 300;
        post.src_agent = 0xC;
        post.dst_agent = 0xD;
        post.is_statechange = sc::BUFF_REMOVE_ALL;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent { time: 300, buff_id: MIGHT, owner: 0xC, agent: 0xD, buff_instance: 0, kind: BuffEventKind::RemoveAll }]
        );
    }

    /// Scenario: MANUAL removal -- must be excluded in both eras (GW2EI's
    /// `BuffRemoveManualEvent` is never simulator-compliant). Pre-era:
    /// `is_buffremove == MANUAL` combat event. Post-era: `is_buffremove ==
    /// MANUAL` on the SAME `sc::BUFF_REMOVE_SINGLE` statechange as a real
    /// Single removal (see `sc::BUFF_REMOVE_SINGLE` docs -- this ordinal
    /// carries both kinds post-era) -- exercises the guard that
    /// distinguishes them.
    #[test]
    fn manual_removal_is_era_equivalent_both_empty() {
        let mut pre = base_event();
        pre.src_agent = 0xC;
        pre.dst_agent = 0xD;
        pre.is_buffremove = buff_remove::MANUAL;

        let mut post = base_event();
        post.src_agent = 0xC;
        post.dst_agent = 0xD;
        post.is_statechange = sc::BUFF_REMOVE_SINGLE;
        post.is_buffremove = buff_remove::MANUAL;

        let pre_events = extract_buff_events(&raw_pre(vec![pre]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![post]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert!(pre_events.is_empty(), "manual removals must not be extracted in either era");
    }

    /// Scenario: pre-existing stacks at log start. `sc::BUFF_INITIAL`
    /// (ordinal 18) is unaffected by the rework threshold -- identical
    /// wire shape in both eras -- so the pre/post twins here are the SAME
    /// event, just under different header builds.
    #[test]
    fn initial_stacks_are_era_equivalent() {
        let mut e = base_event();
        e.src_agent = 0xA;
        e.dst_agent = 0xB;
        e.is_statechange = sc::BUFF_INITIAL;
        e.value = 3000;
        e.buff_dmg = 9000; // original as-cast duration -- must stay unused in both eras

        let pre_events = extract_buff_events(&raw_pre(vec![e.clone()]), &boon_ids());
        let post_events = extract_buff_events(&raw_post(vec![e]), &boon_ids());
        assert_eq!(pre_events, post_events);
        assert_eq!(
            pre_events,
            vec![BuffEvent {
                time: 0,
                buff_id: MIGHT,
                owner: 0xB,
                agent: 0xA,
                buff_instance: 0,
                kind: BuffEventKind::Apply { duration_ms: 3000, is_shields: false },
            }]
        );
    }

    /// Scenario: `BUFFINFO` capacity override/report. `sc::BUFF_INFO`
    /// (ordinal 30) is verified era-invariant (see
    /// `extract_buff_capacities`'s doc comment) -- `extract_buff_capacities`
    /// has no era dispatch at all, so this asserts that claim holds: the
    /// SAME event under a pre-era vs. post-era header produces the same
    /// capacity map.
    #[test]
    fn buff_info_capacity_is_era_equivalent() {
        let mut e = base_event();
        e.is_statechange = sc::BUFF_INFO;
        e.src_master_instid = 25;

        let pre_caps = extract_buff_capacities(&raw_pre(vec![e.clone()]), &boon_ids());
        let post_caps = extract_buff_capacities(&raw_post(vec![e]), &boon_ids());
        assert_eq!(pre_caps, post_caps);
        assert_eq!(pre_caps.get(&MIGHT), Some(&25));
    }
}
