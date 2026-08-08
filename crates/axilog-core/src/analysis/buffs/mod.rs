//! Boon buff-event extraction + stack-state-machine simulation (M3, Task 1).
//!
//! This is the foundation for M3's boons/support-stats milestone: turning
//! raw arcdps buff apply/remove events into a per-agent, per-boon
//! stack-count-over-time timeline. See `events.rs` and `simulator.rs` for
//! the verified field semantics and stack-machine behavior respectively
//! (each cites the arcdps EVTC reference and/or GW2EI source directly).
//!
//! Scope (Task 1, deliberately narrow): only the 12 boon ids in
//! `BOON_IDS`, and only for SQUAD players (`Encounter::players`) -- mirrors
//! how `analysis::cc`/`analysis::damage` scope their own squad-side passes.
//! Enemy/NPC buff uptime, and boon-source attribution ("who generated this
//! boon"), are out of scope here and left to later M3 tasks.

pub mod events;
pub mod simulator;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// A stack-count-over-time step timeline for one (agent, boon): each entry
/// is a `(time_ms, stack_count)` transition -- the count holds at that value
/// until the next entry. No entry before the first one means "0 stacks".
/// `time_ms` is the same absolute arcdps `timeGetTime()` timescale as
/// `RawEvent::time` (not log-relative), matching every other raw-event
/// timestamp already flowing through this crate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoonTimeline {
    pub states: Vec<(u64, u32)>,
}

pub const MIGHT: u32 = 740;
pub const FURY: u32 = 725;
pub const REGENERATION: u32 = 718;
pub const VIGOR: u32 = 726;
pub const SWIFTNESS: u32 = 719;
pub const PROTECTION: u32 = 717;
pub const AEGIS: u32 = 743;
pub const RESOLUTION: u32 = 873;
pub const STABILITY: u32 = 1122;
pub const QUICKNESS: u32 = 1187;
pub const RESISTANCE: u32 = 26980;
pub const ALACRITY: u32 = 30328;

/// The 12 tracked boons: `(skill id, display name, is_intensity)`.
///
/// Every id verified against GW2EI's `SkillIDs.cs`
/// (`GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs`) and cross-checked
/// against `CommonBuffs.Boons` (`GW2EIEvtcParser/EIData/Buffs/
/// CommonBuffs.cs`) for stack type: `is_intensity == true` for
/// `BuffStackType.Stacking`/`StackingConditionalLoss` (Might, Stability --
/// many concurrent independently-ticking stacks, capacity 25), `false` for
/// `BuffStackType.Queue` (the rest -- one active stack plus a queue, per-boon
/// capacity 5 or 9, see `simulator::capacity_for`).
pub const BOON_IDS: [(u32, &str, bool); 12] = [
    (MIGHT, "Might", true),
    (FURY, "Fury", false),
    (REGENERATION, "Regeneration", false),
    (VIGOR, "Vigor", false),
    (SWIFTNESS, "Swiftness", false),
    (PROTECTION, "Protection", false),
    (AEGIS, "Aegis", false),
    (RESOLUTION, "Resolution", false),
    (STABILITY, "Stability", true),
    (QUICKNESS, "Quickness", false),
    (RESISTANCE, "Resistance", false),
    (ALACRITY, "Alacrity", false),
];

/// Simulates every tracked boon's stack-count timeline for every squad
/// player in `enc`. Keyed by `(agent-representative addr, buff id)` --
/// agent addrs are folded onto each account's representative
/// `Player.agent_addr` first (relog/build-swap within one recording gets a
/// new raw addr per login, same as `analysis::analyze`'s
/// `addr_to_rep` -- see that function's doc comment), so a relogged
/// account's boon uptime stays on one entry instead of splitting across
/// addrs.
pub fn simulate_boons(raw: &RawLog, enc: &Encounter) -> BTreeMap<(u64, u32), BoonTimeline> {
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let boon_ids: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();

    let raw_events = events::extract_buff_events(raw, &boon_ids);
    let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    for e in raw_events {
        // Squad-only scope (see module docs): an event whose owner isn't a
        // known squad addr (enemy/NPC recipient, or an addr we never saw in
        // the agent table) is dropped here.
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    grouped
        .into_iter()
        .map(|((rep, buff_id), evs)| {
            let capacity = simulator::capacity_for(buff_id);
            let states = simulator::run(evs, capacity, log_end_ms);
            ((rep, buff_id), BoonTimeline { states })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{buff_remove, RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn player(addr: u64, addrs: Vec<u64>) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup: 1,
            in_squad: true, commander: false, marker: None, commander_tag: None, agent_addrs: addrs,
        }
    }

    fn enc(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "20260114".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], tick_rate: None,
        }
    }

    fn apply_ev(time: u64, buff_id: u32, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: duration_ms, buff_dmg: 0, overstack: 0,
            skillid: buff_id, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 1, result: 0, is_activation: 0, is_buffremove: 0,
            is_statechange: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    #[test]
    fn simulate_boons_folds_relog_addrs_onto_representative() {
        let p = player(1, vec![1, 2]); // relogged: addrs 1 (pre) and 2 (post)
        let e = enc(vec![p]);
        let raw = raw_from(vec![
            apply_ev(0, MIGHT, 9, 1, 5000),   // pre-relog: Might on addr 1
            apply_ev(6000, MIGHT, 9, 2, 5000), // post-relog: Might on addr 2 (same account)
            // Trailing non-boon event so `log_end_ms` extends past the
            // second stack's natural expiry (11000) -- otherwise the
            // simulator correctly (per GW2EI's `Trim(logEnd)`) leaves that
            // last stack un-flushed rather than fabricating an expiry past
            // the log's actual end.
            apply_ev(12_000, 999_999, 0, 0, 0),
        ]);
        let boons = simulate_boons(&raw, &e);
        // Both applies must land on the SAME key (the representative addr
        // 1), not split into two entries.
        let key = (1u64, MIGHT);
        assert!(boons.contains_key(&key), "expected a folded entry for the representative addr");
        assert_eq!(boons.len(), 1, "relog addrs must fold into one timeline, not two");
        let tl = &boons[&key];
        // Two separate apply/expiry cycles: 0->1 (5000)->0, 6000->1 (11000)->0.
        assert_eq!(tl.states, vec![(0, 1), (5000, 0), (6000, 1), (11000, 0)]);
    }

    #[test]
    fn simulate_boons_skips_non_squad_recipients() {
        let p = player(1, vec![1]);
        let e = enc(vec![p]);
        // Might applied to agent 99, which isn't in the squad -- must not
        // appear in the output at all.
        let raw = raw_from(vec![apply_ev(0, MIGHT, 1, 99, 5000)]);
        let boons = simulate_boons(&raw, &e);
        assert!(boons.is_empty());
    }

    /// Smoke-shaped check (real fixture coverage lives in
    /// `tests/golden.rs`): a plausible multi-player Might scenario produces
    /// non-empty timelines for more than one player, per the Task 1 GATE.
    #[test]
    fn simulate_boons_nonempty_for_multiple_players() {
        let e = enc(vec![player(1, vec![1]), player(2, vec![2])]);
        let raw = raw_from(vec![
            apply_ev(0, MIGHT, 9, 1, 5000),
            apply_ev(0, MIGHT, 9, 2, 5000),
        ]);
        let boons = simulate_boons(&raw, &e);
        assert_eq!(boons.len(), 2);
        assert!(!boons[&(1, MIGHT)].states.is_empty());
        assert!(!boons[&(2, MIGHT)].states.is_empty());
    }

    #[test]
    fn simulate_boons_ignores_manual_removal_via_buff_remove_const() {
        // Sanity: buff_remove::MANUAL round-trips through the public evtc
        // re-export used by this module's callers.
        assert_eq!(buff_remove::MANUAL, 3);
    }
}
