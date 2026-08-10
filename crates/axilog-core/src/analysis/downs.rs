use crate::analysis::damage::InstidRegistry;
use crate::analysis::PlayerMetrics;
use crate::evtc::{result, sc, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

pub fn apply(
    players: &mut [PlayerMetrics],
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) {
    apply_with_registry(
        players,
        enc,
        raw,
        &InstidRegistry::build(raw),
        squad,
        enemies,
        addr_to_rep,
    );
}

/// The squad player a downing/killing blow is credited to, or `None`.
///
/// GW2EI's `OffensiveStatistics.cs:190-197` increments `DownedCount`/
/// `KilledCount` OUTSIDE the `dl.From.Is(actor.AgentItem)` guard that
/// wraps almost every other field in that loop, over an event list
/// (`SingleActor.cs:734-739`) that already folds in the actor's MINIONS.
/// So a pet's or minion's downing/killing blow counts for its owner --
/// verified against this project's reference export, where exactly this
/// case accounts for the only two `statsAll[0].killed`/`downed` cells that
/// an actor-only predicate misses (one kill, one down, on two different
/// accounts).
///
/// Resolution is the single-hop `*_master_instid` -> [`InstidRegistry`]
/// fold every other pet-crediting pass in this crate already uses
/// (`damage::pet_credit_events`, `cc::pet_credit_cc_events`,
/// `buffs::events::resolve_agent`): a direct squad source credits itself,
/// otherwise the source's master at that event's time must itself be a
/// squad addr.
pub(crate) fn credited_squad_source(
    e: &crate::evtc::RawEvent,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
) -> Option<u64> {
    if squad.contains(&e.src_agent) {
        return Some(e.src_agent);
    }
    match registry.resolve_at(e.src_master_instid, e.time) {
        Some(addr) if squad.contains(&addr) => Some(addr),
        _ => None,
    }
}

/// [`apply`] against a caller-supplied, already-built [`InstidRegistry`]
/// (MPERF Task 2 convention) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for standalone/test
/// callers.
pub fn apply_with_registry(
    players: &mut [PlayerMetrics],
    _enc: &Encounter,
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) {
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    // Any raw agent addr for an account (relog/build-swap) maps to the
    // account's representative agent_addr before indexing into `players`,
    // so per-account sums aggregate across all of that account's addrs.
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    for e in &raw.events {
        if e.is_statechange != 0 { continue; }
        if !enemies.contains(&e.dst_agent) { continue; }
        if e.result != result::DOWNED && e.result != result::KILLING_BLOW { continue; }
        let Some(src) = credited_squad_source(e, registry, squad) else { continue };
        let Some(&i) = idx.get(&rep(src)) else { continue };
        if e.result == result::DOWNED {
            players[i].downs_dealt += 1;
        } else {
            players[i].kills_dealt += 1;
        }
    }
    // downs taken / deaths (squad members as destination / statechange)
    for e in &raw.events {
        if e.is_statechange == sc::CHANGE_DEAD {
            if let Some(&i) = idx.get(&rep(e.src_agent)) { players[i].deaths += 1; }
        }
        if e.is_statechange == 0 && e.result == result::DOWNED
            && squad.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&rep(e.dst_agent)) { players[i].downs_taken += 1; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{result, RawEvent};
    use std::collections::BTreeSet;
    fn ev(time: u64, src: u64, dst: u64, value: i32, result_: u8, sc: u8) -> RawEvent {
        RawEvent { time, src_agent: src, dst_agent: dst, value, buff_dmg: 0, overstack: 0,
            skillid: 1, src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 1, buff: 0, result: result_, is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    #[test]
    fn counts_downs_dealt_and_kills_dealt() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let evs = vec![
            ev(500, 1, 9, 300, 0, 0),                  // damage before down (no longer tracked here -- see analysis::contribution)
            ev(1000, 1, 9, 0, result::DOWNED, 0),       // enemy downed by src 1
            ev(2000, 1, 9, 0, result::KILLING_BLOW, 0), // kill
        ];
        let mut pm = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        // build enc with duration only
        let enc = crate::model::Encounter { kind: "wvw".into(), map: "".into(),
            duration_ms: 2000, build: "".into(), revision: 1, recorded_by: None,
            teams: vec![], players: vec![], enemies: vec![], markers: vec![], tick_rate: None };
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply(&mut pm, &enc, &raw_from(evs), &squad, &enemies, &addr_to_rep);
        assert_eq!(pm[0].downs_dealt, 1);
        assert_eq!(pm[0].kills_dealt, 1);
    }
    fn raw_from(events: Vec<RawEvent>) -> crate::evtc::RawLog {
        crate::evtc::RawLog { header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![] }
    }
}
