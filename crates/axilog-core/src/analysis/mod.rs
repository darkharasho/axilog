pub mod damage;
pub mod downs;
pub mod cc;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

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
    let squad: BTreeSet<u64> = enc.players.iter().map(|p| p.agent_addr).collect();
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
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut players: Vec<PlayerMetrics> = enc.players.iter().map(|p| {
        let (total, per) = dmg.get(&p.agent_addr).cloned().unwrap_or_default();
        PlayerMetrics { agent_addr: p.agent_addr, damage_total: total,
            dps: total as f64 / secs,
            per_enemy: per.into_iter().collect(), ..Default::default() }
    }).collect();
    downs::apply(&mut players, enc, raw, &squad, &enemies);
    cc::apply_cc(&mut players, raw, &squad, &enemies);
    let timeline = cc::timeline(enc, raw, &squad, &enemies);
    Metrics { players, timeline }
}
