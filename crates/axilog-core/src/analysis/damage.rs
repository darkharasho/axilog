use crate::evtc::RawEvent;
use std::collections::{BTreeMap, BTreeSet};

/// Returns per-source: (total_to_enemies, per_enemy_map).
pub fn accumulate(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    let mut out: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) { continue; }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        let entry = out.entry(e.src_agent).or_default();
        entry.0 += dmg;
        *entry.1.entry(e.dst_agent).or_default() += dmg;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;
    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time:0, src_agent:src, dst_agent:dst, value:dmg, buff_dmg:0,
            overstack:0, skillid:1, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:1, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_statechange:0 }
    }
    #[test]
    fn sums_physical_damage_to_enemy() {
        let squad = [1u64].into_iter().collect();
        let enemies = [9u64].into_iter().collect();
        let evs = vec![strike(1, 9, 100), strike(1, 9, 50), strike(1, 2, 999)];
        let dmg = accumulate(&evs, &squad, &enemies);
        assert_eq!(dmg[&1].0, 150); // total to enemies only
        assert_eq!(dmg[&1].1.get(&9).copied(), Some(150));
    }
}
