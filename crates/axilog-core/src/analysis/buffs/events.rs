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
    Apply { duration_ms: u32 },
    /// `is_buffremove == SINGLE`: removes exactly one currently-held stack.
    /// `removed_duration_ms` is the raw event's `value` field -- GW2EI's
    /// default (non-instance) simulator matches it against each held
    /// stack's REMAINING duration at removal time (not the stack's
    /// originally applied duration), within a `15`ms tolerance
    /// (`ParserHelper.BuffSimulatorDelayConstant`) -- see `simulator.rs`.
    RemoveSingle { removed_duration_ms: u32 },
    /// `is_buffremove == ALL`: clears every currently-held stack.
    RemoveAll,
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
                kind: BuffEventKind::Apply { duration_ms: e.value.max(0) as u32 },
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
        // set.
        if e.buff != 0
            && e.buff_dmg == 0
            && e.value > 0
            && e.is_activation == 0
            && e.is_buffremove == buff_remove::NONE
        {
            out.push(BuffEvent {
                time: e.time,
                buff_id: e.skillid,
                owner: e.dst_agent,
                agent: resolve_agent(e.src_agent, e.src_master_instid, e.time),
                kind: BuffEventKind::Apply { duration_ms: e.value as u32 },
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
            is_buffremove: 0, is_statechange: 0,
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
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 5000 });
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
        assert_eq!(events[0].kind, BuffEventKind::Apply { duration_ms: 3000 });
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
}
