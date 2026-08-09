use crate::analysis::PlayerMetrics;
use crate::evtc::{result, sc, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

const WINDOW_MS: u64 = 10_000;

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
    // down contribution: damage to each enemy in the window before its down
    let downs: Vec<(u64, u64)> = raw.events.iter()
        .filter(|e| e.is_statechange == 0 && e.result == result::DOWNED
            && enemies.contains(&e.dst_agent))
        .map(|e| (e.dst_agent, e.time)).collect();
    for (enemy, t_down) in downs {
        let lo = t_down.saturating_sub(WINDOW_MS);
        for e in &raw.events {
            if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
            // Crowd-control application events reuse value/buff_dmg to carry
            // CC duration ms, not damage — must match `accumulate`'s
            // predicate exactly (Finding #4).
            if e.result == result::CROWD_CONTROL { continue; }
            if e.dst_agent != enemy || e.time <= lo || e.time > t_down { continue; }
            if !squad.contains(&e.src_agent) { continue; }
            let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
            if let Some(&i) = idx.get(&rep(e.src_agent)) { players[i].down_contribution += dmg; }
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
            iff: 1, buff: 0, result: result_, is_activation: 0, is_buffremove: 0, is_statechange: sc, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    #[test]
    fn counts_down_and_attributes_contribution() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let evs = vec![
            ev(500, 1, 9, 300, 0, 0),                  // damage before down
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
        assert_eq!(pm[0].down_contribution, 300);
    }
    #[test]
    fn down_contribution_excludes_crowd_control_events() {
        // A CC application event carries CC duration ms in `value`, not
        // damage — it must not inflate down_contribution (Finding #4).
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let evs = vec![
            ev(400, 1, 9, 5000, result::CROWD_CONTROL, 0), // bogus "damage" if not excluded
            ev(500, 1, 9, 300, 0, 0),
            ev(1000, 1, 9, 0, result::DOWNED, 0),
        ];
        let mut pm = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let enc = crate::model::Encounter { kind: "wvw".into(), map: "".into(),
            duration_ms: 1000, build: "".into(), revision: 1, recorded_by: None,
            teams: vec![], players: vec![], enemies: vec![], markers: vec![], tick_rate: None };
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply(&mut pm, &enc, &raw_from(evs), &squad, &enemies, &addr_to_rep);
        assert_eq!(pm[0].down_contribution, 300);
    }
    fn raw_from(events: Vec<RawEvent>) -> crate::evtc::RawLog {
        crate::evtc::RawLog { header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![] }
    }
}
