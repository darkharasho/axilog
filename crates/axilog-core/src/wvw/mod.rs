use crate::evtc::{RawLog, sc};
use crate::model::{Encounter, Team, Player};
use std::collections::BTreeMap;

/// Collapse relog/build-swap duplicates: one Player per account (fallback character).
pub fn dedupe_players(players: &mut Vec<Player>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        let key = if p.account.is_empty() { p.character.clone() } else { p.account.clone() };
        match seen.get(&key) {
            Some(&i) => { out[i].in_squad |= p.in_squad; out[i].commander |= p.commander; }
            None => { seen.insert(key, out.len()); out.push(p); }
        }
    }
    *players = out;
}

// team ids verified against golden wvWMapData in Task 16
fn team_color(team_id: u16) -> String {
    // WvW team ids → colors (verify against wvWMapData in the golden fixture).
    match team_id {
        883 | 39 | 2520 => "red".into(),
        882 | 38 | 2519 => "blue".into(),
        // remaining known green ids
        _ => "green".into(),
    }
}

pub fn apply(enc: &mut Encounter, raw: &RawLog) {
    // team assignment: TEAM_CHANGE src_agent -> value (team id) in dst_agent field
    let mut agent_team: BTreeMap<u64, u16> = BTreeMap::new();
    let mut recorded_by: Option<u64> = None;
    for e in &raw.events {
        if e.is_statechange == sc::TEAM_CHANGE {
            agent_team.insert(e.src_agent, e.dst_agent as u16);
        } else if e.is_statechange == sc::POINT_OF_VIEW {
            recorded_by = Some(e.src_agent);
        }
    }
    let mut team_ids: Vec<u16> = agent_team.values().copied().collect();
    team_ids.sort_unstable(); team_ids.dedup();
    enc.teams = team_ids.iter().map(|&id| Team { color: team_color(id), team_id: id }).collect();
    for p in &mut enc.players {
        if let Some(&t) = agent_team.get(&p.agent_addr) { p.team = team_color(t); }
    }
    for en in &mut enc.enemies {
        if let Some(&t) = agent_team.get(&en.id) { en.team = team_color(t); }
    }
    if let Some(addr) = recorded_by {
        if let Some(p) = enc.players.iter().find(|p| p.agent_addr == addr) {
            enc.recorded_by = Some(p.account.clone());
        }
    }
    dedupe_players(&mut enc.players);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Encounter, Player};
    fn player(addr: u64, acc: &str) -> Player {
        Player { agent_addr: addr, account: acc.into(), character: "C".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "".into(),
            subgroup: 1, in_squad: true, commander: false }
    }
    #[test]
    fn dedupes_players_by_account() {
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1"), player(3, ":B.2")],
            enemies: vec![] };
        dedupe_players(&mut enc.players);
        assert_eq!(enc.players.len(), 2);
    }
}
