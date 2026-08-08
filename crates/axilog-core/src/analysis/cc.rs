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
    for e in &raw.events {
        let rel = e.time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b >= buckets {
            continue;
        }
        if e.is_statechange == 0
            && e.is_activation == 0
            && e.is_buffremove == 0
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

// CC/breakbar predicate approximate — verify & refine against golden fixture in Task 16
fn is_cc(e: &crate::evtc::RawEvent) -> bool {
    e.is_statechange == 0 && e.is_activation == 0 && e.buff == 0 && e.overstack > 0
}

pub fn apply_cc(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) {
    use std::collections::BTreeMap;
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    for e in &raw.events {
        if is_cc(e) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&e.src_agent) {
                players[i].cc_applied += 1;
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
}
