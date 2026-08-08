pub mod damage;
pub mod downs;
pub mod cc;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct PlayerMetrics { pub agent_addr: u64, pub damage_total: u64, pub dps: f64,
    pub per_enemy: Vec<(u64,u64)>, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32,
    pub damage_taken: u64, pub cc_applied: u32, pub cc_duration_ms: u64 }
#[derive(Debug, Clone)]
pub struct Timeline { pub resolution_ms: u64, pub squad_damage: Vec<u64>,
    pub cc_applied: Vec<u32>, pub downs: Vec<u32> }
#[derive(Debug, Clone)]
pub struct Metrics { pub players: Vec<PlayerMetrics>, pub timeline: Timeline }

pub fn analyze(enc: &Encounter, raw: &RawLog) -> Metrics {
    // A friendly account can own several raw agent addrs (relog / build
    // swap within the same recording — arcdps assigns a new addr per
    // login). `squad` is the union of ALL of an account's addrs so no
    // post-relog damage/downs/kills/CC/pets is dropped, while `addr_to_rep`
    // maps every one of those addrs back to the account's single
    // representative `Player.agent_addr` so per-account sums still land on
    // one `PlayerMetrics` entry (Finding #1).
    let squad: BTreeSet<u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let enemies: BTreeSet<u64> = enc.enemies.iter().map(|e| e.id).collect();
    let mut dmg = damage::accumulate(&raw.events, &squad, &enemies);
    // WvW: credit friendly pet/minion damage (arcdps attributes it to the
    // pet's own agent) to the owning squad player — see Task 16A.
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, (total, per)) in damage::accumulate_pet_credit(raw, &squad, friendly_team, &agent_team) {
        let entry = dmg.entry(owner).or_default();
        entry.0 += total;
        for (dst, d) in per {
            *entry.1.entry(dst).or_default() += d;
        }
    }
    // Fold every source addr's damage onto its account representative so a
    // relogged/build-swapped account's damage is summed, not dropped.
    let mut dmg_by_rep: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for (addr, (total, per)) in dmg.into_iter() {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        let entry = dmg_by_rep.entry(rep).or_default();
        entry.0 += total;
        for (dst, d) in per {
            *entry.1.entry(dst).or_default() += d;
        }
    }
    let damage_taken = damage::accumulate_damage_taken(&raw.events, &squad);
    let mut taken_by_rep: BTreeMap<u64, u64> = BTreeMap::new();
    for (addr, total) in damage_taken {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        *taken_by_rep.entry(rep).or_default() += total;
    }
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut players: Vec<PlayerMetrics> = enc.players.iter().map(|p| {
        let (total, per) = dmg_by_rep.get(&p.agent_addr).cloned().unwrap_or_default();
        let taken = taken_by_rep.get(&p.agent_addr).copied().unwrap_or(0);
        PlayerMetrics { agent_addr: p.agent_addr, damage_total: total,
            dps: total as f64 / secs, damage_taken: taken,
            per_enemy: per.into_iter().collect(), ..Default::default() }
    }).collect();
    downs::apply(&mut players, enc, raw, &squad, &enemies, &addr_to_rep);
    cc::apply_cc(&mut players, raw, &squad, &enemies, &addr_to_rep);
    let timeline = cc::timeline(enc, raw, &squad, &enemies);
    Metrics { players, timeline }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time: 0, src_agent: src, dst_agent: dst, value: dmg, buff_dmg: 0,
            overstack: 0, skillid: 1, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: 0 }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog { header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![] }
    }

    /// Finding #1: a relogged/build-swapped account (two raw agent addrs,
    /// one `Player` after dedupe) must have damage from BOTH addrs summed
    /// into the single surviving `PlayerMetrics` entry, not dropped.
    #[test]
    fn relog_damage_aggregates_across_all_account_addrs() {
        let player = Player {
            agent_addr: 1, // representative (first-seen addr)
            account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false,
            agent_addrs: vec![1, 2], // pre-relog addr 1, post-relog addr 2
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true }],
        };
        let raw = raw_from(vec![
            strike(1, 9, 100), // pre-relog damage from addr 1
            strike(2, 9, 250), // post-relog damage from addr 2 (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players.len(), 1);
        assert_eq!(metrics.players[0].agent_addr, 1);
        assert_eq!(metrics.players[0].damage_total, 350); // 100 + 250, not just one addr
    }

    /// Finding #3: damage_taken sums incoming damage (from any source) for
    /// every one of an account's addrs, folded onto the representative.
    #[test]
    fn damage_taken_sums_incoming_damage_across_relog_addrs() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false,
            agent_addrs: vec![1, 2],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true }],
        };
        let raw = raw_from(vec![
            strike(9, 1, 80),  // enemy hits pre-relog addr
            strike(9, 2, 120), // enemy hits post-relog addr (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players[0].damage_taken, 200);
    }
}
