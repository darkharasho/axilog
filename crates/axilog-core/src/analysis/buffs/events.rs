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
//! the full version-split explanation. So this extracts the OLDER shape:
//! apply/remove are ordinary `is_statechange == 0` combat events (flagged by
//! `buff`/`is_buffremove`), not the dedicated statechange types the
//! *current* (live, 2026-08) arcdps reference documents for newer builds.

use crate::analysis::damage::InstidRegistry;
use crate::evtc::{buff_remove, sc, RawLog};
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
    /// values before simulating; this project does not decode the
    /// `BuffInstance` (`pad`) field needed to replicate that correction
    /// (out of scope, same simplification Task 1 already documented for
    /// the `pad61`/instance-id fields), so `simulator::run` consumes the
    /// RAW `extended_ms`/`new_duration_ms` values directly -- see that
    /// module's `Extend` doc comment for the resulting approximation and
    /// its calibrated accuracy.
    Extend { extended_ms: u32, new_duration_ms: u32 },
}

/// Extracts apply/remove/initial events for exactly the `boon_ids` skill
/// ids (M3 Task 1 scope: only the 12 tracked boons -- see
/// `super::BOON_IDS`), in event order. `Manual` removals
/// (`buff_remove::MANUAL`) are intentionally NOT extracted -- GW2EI's
/// `BuffRemoveManualEvent` excludes them from the simulator entirely (see
/// `crate::evtc::buff_remove::MANUAL` docs).
pub fn extract_buff_events(raw: &RawLog, boon_ids: &BTreeSet<u32>) -> Vec<BuffEvent> {
    let registry = InstidRegistry::build(raw);
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
        if e.buff != 0
            && e.buff_dmg == 0
            && e.value > 0
            && e.is_activation == 0
            && e.is_buffremove == buff_remove::NONE
        {
            if e.is_offcycle != 0 {
                out.push(BuffEvent {
                    time: e.time,
                    buff_id: e.skillid,
                    owner: e.dst_agent,
                    agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
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
                    kind: BuffEventKind::RemoveAll,
                }),
                buff_remove::SINGLE => out.push(BuffEvent {
                    time: e.time,
                    buff_id: e.skillid,
                    owner: e.src_agent,
                    agent,
                    kind: BuffEventKind::RemoveSingle { removed_duration_ms: e.value.max(0) as u32 },
                }),
                _ => {} // MANUAL or unknown: not simulator-compliant, skip (see buff_remove::MANUAL docs).
            }
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
            is_buffremove: 0, is_statechange: 0, is_shields: 0, is_offcycle: 0,
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
