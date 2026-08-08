use crate::evtc::{RawLog, sc};
use crate::model::{Encounter, Team, Player, Enemy};
use std::collections::BTreeMap;

/// Collapse relog/build-swap duplicates: one Player per account (fallback character).
pub fn dedupe_players(players: &mut Vec<Player>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        let key = if p.account.is_empty() { p.character.clone() } else { p.account.clone() };
        match seen.get(&key) {
            Some(&i) => {
                out[i].in_squad |= p.in_squad;
                out[i].commander |= p.commander;
                out[i].agent_addrs.extend(p.agent_addrs);
            }
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

/// Parse per-agent WvW team ids and the POINT_OF_VIEW (recording) agent from
/// raw combat events. Shared by `apply` (friend/foe partition, below) and by
/// the analysis layer (pet/minion damage attribution in `analysis::damage`).
pub fn resolve_teams(raw: &RawLog) -> (BTreeMap<u64, u16>, Option<u64>) {
    let mut agent_team: BTreeMap<u64, u16> = BTreeMap::new();
    let mut recorded_by: Option<u64> = None;
    for e in &raw.events {
        if e.is_statechange == sc::TEAM_CHANGE {
            // The WvW team id is carried in the `value` field (i32 @ offset
            // 24), not `dst_agent` — verified against the golden fixture's
            // teamID (Task 16A). Every agent (players, NPCs, gadgets) gets
            // exactly one TEAM_CHANGE event.
            agent_team.insert(e.src_agent, e.value as u16);
        } else if e.is_statechange == sc::POINT_OF_VIEW {
            recorded_by = Some(e.src_agent);
        }
    }
    (agent_team, recorded_by)
}

pub fn apply(enc: &mut Encounter, raw: &RawLog) {
    let (agent_team, recorded_by) = resolve_teams(raw);

    let mut team_ids: Vec<u16> = agent_team.values().copied().collect();
    team_ids.sort_unstable(); team_ids.dedup();
    enc.teams = team_ids.iter().map(|&id| Team { color: team_color(id), team_id: id }).collect();

    // Friend/foe partition (Task 16A calibration fix).
    //
    // `model::resolve` classifies agents purely from the EVTC agent block
    // (`is_elite != 0xffffffff` => Player), which cannot distinguish squad
    // members from enemy players in WvW — both are real players. It stuffs
    // every player agent into `enc.players` and every NPC/gadget into
    // `enc.enemies`. Here we use each agent's WvW team id relative to the
    // recorder's own team (POINT_OF_VIEW) to split real squad members from
    // enemy players, and to drop friendly-side NPCs/gadgets (pets, siege,
    // guards on our own team) out of `enc.enemies`.
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());

    let mut friendly_players = Vec::new();
    for p in enc.players.drain(..) {
        let is_friendly = match agent_team.get(&p.agent_addr) {
            Some(&t) => Some(t) == friendly_team,
            // Unconstrained player agent (no observed team): not confirmed
            // to be on the recorder's team, so default to enemy — safer
            // than silently inflating the squad.
            None => false,
        };
        if is_friendly {
            friendly_players.push(p);
        } else {
            let team = agent_team.get(&p.agent_addr).map(|&t| team_color(t)).unwrap_or_default();
            enc.enemies.push(Enemy {
                id: p.agent_addr,
                instid: 0,
                name: p.character,
                team,
                is_player: true,
            });
        }
    }
    enc.players = friendly_players;

    // `enc.enemies` now holds the enemy players just moved in above, plus
    // every NPC/gadget agent (model::resolve puts them all there
    // unconditionally). Keep only the hostile ones: agents on a known,
    // non-friendly team. NPCs/gadgets with no team record at all are
    // dropped as neutral — they are not foes and would otherwise inflate
    // squad damage totals.
    enc.enemies.retain(|en| {
        if en.is_player {
            return true; // already resolved above
        }
        match agent_team.get(&en.id) {
            Some(&t) => friendly_team.map(|ft| t != ft).unwrap_or(true),
            None => false,
        }
    });

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
            subgroup: 1, in_squad: true, commander: false, agent_addrs: vec![addr] }
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
    #[test]
    fn dedupe_collects_all_agent_addrs_for_relog() {
        // Same account, two raw agent addrs (relog / build swap). The
        // survivor must retain BOTH addrs so downstream analysis can sum
        // damage across the full account, not just the representative.
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1")],
            enemies: vec![] };
        dedupe_players(&mut enc.players);
        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].agent_addr, 1);
        let mut addrs = enc.players[0].agent_addrs.clone();
        addrs.sort_unstable();
        assert_eq!(addrs, vec![1, 2]);
    }
}
