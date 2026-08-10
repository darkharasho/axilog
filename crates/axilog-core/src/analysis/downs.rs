use crate::analysis::PlayerMetrics;
use crate::evtc::{result, sc, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

pub fn apply(
    players: &mut [PlayerMetrics],
    _enc: &Encounter,
    raw: &RawLog,
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
        let src_is_squad = squad.contains(&e.src_agent);
        let dst_is_enemy = enemies.contains(&e.dst_agent);
        if src_is_squad && dst_is_enemy && e.result == result::DOWNED {
            if let Some(&i) = idx.get(&rep(e.src_agent)) { players[i].downs_dealt += 1; }
        }
        if src_is_squad && dst_is_enemy && e.result == result::KILLING_BLOW {
            if let Some(&i) = idx.get(&rep(e.src_agent)) { players[i].kills_dealt += 1; }
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
