use crate::evtc::{result, RawEvent, RawLog};
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
        // Crowd-control application events reuse `value`/`buff_dmg` to carry
        // CC duration (ms), not damage — see `result::CROWD_CONTROL` docs.
        if e.result == result::CROWD_CONTROL { continue; }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) { continue; }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        let entry = out.entry(e.src_agent).or_default();
        entry.0 += dmg;
        *entry.1.entry(e.dst_agent).or_default() += dmg;
    }
    out
}

/// Credits friendly-side NPC/gadget (pet, minion, spirit, mount-summon,
/// etc.) damage to the owning squad player.
///
/// arcdps attributes pet/minion damage to the pet's own agent, not its
/// owner, so `accumulate` (which only looks at `squad` player addresses as
/// `src_agent`) misses it entirely. In the WvW golden fixture (Task 16A)
/// that undercounted squadTotalDamage by ~1.1% — enough to fail the 0.5%
/// calibration tolerance. This resolves the owner via each event's
/// `src_master_instid`, looked up against a log-wide (last-write-wins)
/// instid -> agent-address table built from the same events. That's an
/// approximation (arcdps recycles instids after an agent despawns) but is
/// accurate enough for a single, short encounter.
///
/// `friendly_team` / `agent_team` come from `wvw::resolve_teams`. Damage
/// destination is intentionally NOT restricted to the `enemies` set here:
/// arcdps' own `iff` field (relative to the pet, i.e. non-FRIEND) is used
/// instead, matching how `accumulate`'s squad/enemies split is derived in
/// the first place — see wvw::apply.
pub fn accumulate_pet_credit(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u16>,
    agent_team: &BTreeMap<u64, u16>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    let mut instid_to_addr: BTreeMap<u16, u64> = BTreeMap::new();
    for e in &raw.events {
        if e.src_instid != 0 { instid_to_addr.insert(e.src_instid, e.src_agent); }
        if e.dst_instid != 0 { instid_to_addr.insert(e.dst_instid, e.dst_agent); }
    }

    let mut out: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if e.result == result::CROWD_CONTROL { continue; }
        if e.iff == 0 { continue; } // FRIEND: never damage
        if squad.contains(&e.src_agent) { continue; } // real players: handled by `accumulate`
        if agent_team.get(&e.src_agent).copied() != friendly_team { continue; } // not our pet
        let owner = match instid_to_addr.get(&e.src_master_instid) {
            Some(&addr) if squad.contains(&addr) => addr,
            _ => continue,
        };
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        let entry = out.entry(owner).or_default();
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
