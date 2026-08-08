use crate::analysis::{PlayerMetrics, Timeline};
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

pub fn timeline(
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> Timeline {
    let res = 1000u64;
    let buckets = ((enc.duration_ms / res) + 1) as usize;
    let mut squad_damage = vec![0u64; buckets];
    let mut cc_applied = vec![0u32; buckets];
    let mut downs = vec![0u32; buckets];
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);

    // WvW: fold friendly pet/minion damage into the same per-second buckets
    // as direct squad damage, matching how `analysis::analyze` credits it
    // to owning players — otherwise sum(squad_damage) undercounts
    // sum(player.damage_total) by the pet-credit share (Finding #4).
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (time, _owner, _dst, dmg) in
        crate::analysis::damage::pet_credit_events(raw, squad, friendly_team, &agent_team)
    {
        let rel = time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b < buckets {
            squad_damage[b] += dmg;
        }
    }

    for e in &raw.events {
        let rel = e.time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b >= buckets {
            continue;
        }
        if e.is_statechange == 0
            && e.is_activation == 0
            && e.is_buffremove == 0
            // Crowd-control application events reuse value/buff_dmg to
            // carry CC duration ms, not damage — exclude them so the
            // timeline matches `damage::accumulate`'s predicate exactly
            // (Finding #4).
            && e.result != crate::evtc::result::CROWD_CONTROL
            && squad.contains(&e.src_agent)
            && enemies.contains(&e.dst_agent)
        {
            let d = if e.buff == 1 {
                e.buff_dmg.max(0) as u64
            } else {
                e.value.max(0) as u64
            };
            squad_damage[b] += d;
        }
        if e.is_statechange == 0
            && e.result == crate::evtc::result::DOWNED
            && enemies.contains(&e.dst_agent)
        {
            downs[b] += 1;
        }
        if is_cc(e) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            cc_applied[b] += 1;
        }
    }
    Timeline { resolution_ms: res, squad_damage, cc_applied, downs }
}

/// CC application predicate (Task 3, M2): a non-statechange, non-buff
/// combat event whose `result` is `CROWD_CONTROL` — `value` carries the CC
/// duration in ms (see `result::CROWD_CONTROL` docs). Replaces the earlier
/// overstack-based heuristic (Task 11).
///
/// The `buff == 0` check matters: arcdps/GW2EI only ever construct a
/// `CrowdControlEvent` from the *direct* (non-buff) combat-result byte
/// (GW2EI's `CombatEventFactory.AddDirectDamageEvent` routes
/// `result::CrowdControl` there; the buff/condition-stack path
/// (`AddBuffDamageDamageEvent`) uses a different result-byte interpretation
/// entirely — a distinct `ConditionResult`/rework-era `DamageResult`
/// namespace, not the same enum). Debugged against the golden WvW fixture:
/// without `buff == 0`, byte value 12 also coincidentally turns up on
/// ordinary boon-stack application events (Might/Stability/Fury/
/// Vulnerability/Resolution), producing nonsensical multi-minute "CC
/// durations" — those are a different field entirely, not CC. Calibrated
/// against the golden fixture: this predicate (combined with pet-credit in
/// `apply_cc`) reproduces EI's `appliedCrowdControl`/
/// `appliedCrowdControlDuration` squad totals within tolerance.
fn is_cc(e: &crate::evtc::RawEvent) -> bool {
    e.is_statechange == 0 && e.buff == 0 && e.result == crate::evtc::result::CROWD_CONTROL
}

/// Pet/minion-sourced CC application events credited to the owning squad
/// player: `(owner, dst, duration_ms)`. Mirrors
/// `damage::pet_credit_events`'s owner resolution (via `src_master_instid`
/// looked up against a log-wide, last-write-wins instid -> agent-address
/// table) but selects `is_cc` rows instead of excluding them, and reads
/// the CC duration from `value` instead of damage from `value`/`buff_dmg`.
///
/// Required to match EI: GW2EI's `SingleActor.InitOutgoingCrowdControlEvents`
/// folds each player's minions' `GetOutgoingCrowdControlEvents` into the
/// owning player's own outgoing-CC list (same as it does for damage) —
/// confirmed by reading GW2EI's `SingleActor.cs` on GitHub. Calibrated
/// against the golden WvW fixture (Task 3, M2): squad
/// `cc_applied`/`cc_duration_ms` sums only land within 2% of EI's golden
/// 34 / 50460ms when pet-sourced CC is included; excluding it undercounts.
///
/// Unlike `damage::pet_credit_events`, this DOES restrict `dst_agent` to
/// the `enemies` set (rather than relying solely on `iff`). Debugged
/// against the golden fixture: some pet-sourced CC rows carry `dst_agent ==
/// 0` (periodic/self ground-effect ticks with no real target — e.g. a
/// ranger pet's repeating ~280ms-interval skill), which pass the `iff`
/// check but were never "CC applied to an enemy" and aren't in EI's count.
/// Restricting to `enemies` drops those and reproduces the golden fixture
/// exactly (34 / 50460ms, not just within tolerance).
fn pet_credit_cc_events(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    friendly_team: Option<u16>,
    agent_team: &std::collections::BTreeMap<u64, u16>,
) -> Vec<(u64, u64, u64)> {
    use std::collections::BTreeMap;
    let mut instid_to_addr: BTreeMap<u16, u64> = BTreeMap::new();
    for e in &raw.events {
        if e.src_instid != 0 { instid_to_addr.insert(e.src_instid, e.src_agent); }
        if e.dst_instid != 0 { instid_to_addr.insert(e.dst_instid, e.dst_agent); }
    }
    let mut out = Vec::new();
    for e in &raw.events {
        if !is_cc(e) { continue; }
        if !enemies.contains(&e.dst_agent) { continue; } // must be a real, known enemy
        if e.iff == 0 { continue; } // FRIEND: not CC applied to an enemy
        if squad.contains(&e.src_agent) { continue; } // real players: handled directly below
        if agent_team.get(&e.src_agent).copied() != friendly_team { continue; } // not our pet
        let owner = match instid_to_addr.get(&e.src_master_instid) {
            Some(&addr) if squad.contains(&addr) => addr,
            _ => continue,
        };
        out.push((owner, e.dst_agent, e.value.max(0) as u64));
    }
    out
}

pub fn apply_cc(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &std::collections::BTreeMap<u64, u64>,
) {
    use std::collections::BTreeMap;
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    // Direct (player-sourced) CC applied to an enemy.
    for e in &raw.events {
        if is_cc(e) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].cc_applied += 1;
                players[i].cc_duration_ms += e.value.max(0) as u64;
            }
        }
    }

    // Pet/minion-sourced CC, credited to the owning squad player (see
    // `pet_credit_cc_events` docs — required to match EI).
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, _dst, duration_ms) in
        pet_credit_cc_events(raw, squad, enemies, friendly_team, &agent_team)
    {
        if let Some(&i) = idx.get(&rep(owner)) {
            players[i].cc_applied += 1;
            players[i].cc_duration_ms += duration_ms;
        }
    }

    // CBTS_STUNBREAK: keyed by `src_agent`, the agent whose stun broke
    // early (see `sc::STUN_BREAK` docs). `value` is the remaining stun
    // duration (ms) that was cancelled.
    for e in &raw.events {
        if e.is_statechange == crate::evtc::sc::STUN_BREAK && squad.contains(&e.src_agent) {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].stun_breaks += 1;
                players[i].removed_stun_duration_ms += e.value.max(0) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::Encounter;
    use std::collections::BTreeSet;
    fn dmg(time: u64, src: u64, dst: u64, v: i32) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: dst,
            value: v,
            buff_dmg: 0,
            overstack: 0,
            skillid: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 1,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_statechange: 0,
        }
    }
    #[test]
    fn buckets_squad_damage_per_second() {
        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 2500,
            build: "".into(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![],
            enemies: vec![],
        };
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![dmg(100, 1, 9, 50), dmg(1200, 1, 9, 70), dmg(2400, 1, 9, 30)],
            guid_map: vec![],
        };
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let tl = timeline(&enc, &raw, &squad, &enemies);
        assert_eq!(tl.resolution_ms, 1000);
        assert_eq!(tl.squad_damage.len(), 3); // seconds 0,1,2
        assert_eq!(tl.squad_damage[0], 50);
        assert_eq!(tl.squad_damage[1], 70);
        assert_eq!(tl.squad_damage[2], 30);
    }

    fn cc_ev(time: u64, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        let mut e = dmg(time, src, dst, duration_ms);
        e.result = crate::evtc::result::CROWD_CONTROL;
        e
    }
    fn team_change(agent: u64, team_id: i32) -> RawEvent {
        let mut e = dmg(0, agent, 0, team_id);
        e.is_statechange = crate::evtc::sc::TEAM_CHANGE;
        e
    }
    fn pov(agent: u64) -> RawEvent {
        let mut e = dmg(0, agent, 0, 0);
        e.is_statechange = crate::evtc::sc::POINT_OF_VIEW;
        e
    }
    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    #[test]
    fn apply_cc_credits_direct_events_and_sums_duration() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let raw = raw_from(vec![cc_ev(100, 1, 9, 1500), cc_ev(200, 1, 9, 500)]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 2);
        assert_eq!(players[0].cc_duration_ms, 2000);
    }

    #[test]
    fn apply_cc_ignores_non_cc_and_non_enemy_events() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        // Plain damage (not CC) and a CC event against a non-enemy dst.
        let raw = raw_from(vec![dmg(50, 1, 9, 999), cc_ev(100, 1, 2, 1500)]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 0);
        assert_eq!(players[0].cc_duration_ms, 0);
    }

    /// Pet/minion-sourced CC must be credited to the owning squad player
    /// (see `pet_credit_cc_events` docs — required to match EI's
    /// `appliedCrowdControl`, which folds minion CC into the owner).
    #[test]
    fn apply_cc_credits_pet_sourced_cc_to_owner() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut seed = dmg(50, 1, 9, 10); // registers agent 1's instid in the log-wide table
        seed.src_instid = 11;
        let mut pet_cc = cc_ev(100, 2, 9, 800); // pet agent 2, owned by agent 1
        pet_cc.src_instid = 22;
        pet_cc.src_master_instid = 11; // points back to owner's instid
        let raw = raw_from(vec![
            team_change(1, 10), // owner's WvW team
            team_change(2, 10), // pet: same team as owner
            team_change(9, 20), // enemy: different team
            pov(1),             // recorder is the owner, so friendly_team = 10
            seed,
            pet_cc,
        ]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 1);
        assert_eq!(players[0].cc_duration_ms, 800);
    }

    #[test]
    fn apply_cc_tracks_stun_breaks() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut sb1 = dmg(100, 1, 0, 750);
        sb1.is_statechange = crate::evtc::sc::STUN_BREAK;
        let mut sb2 = dmg(200, 1, 0, 250);
        sb2.is_statechange = crate::evtc::sc::STUN_BREAK;
        let raw = raw_from(vec![sb1, sb2]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].stun_breaks, 2);
        assert_eq!(players[0].removed_stun_duration_ms, 1000);
    }
}
