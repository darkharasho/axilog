//! Census of what survives arcdps's enemy-event filter for cast-animation
//! rows. Takes ONE argument: a file listing log paths, one per line (so the
//! totals are not split across xargs batches).
//!
//! "Enemy-filtered" agents are separated from "fully tracked" ones: arcdps
//! emits `CBTS_BUFFINITIAL` only for agents it tracks at full fidelity, and
//! WvW team detection occasionally files such an agent under the enemy
//! roster, so that flag is the honest way to exclude them.
use axilog_core::evtc::{decode_raw, event::sc};
use std::collections::BTreeSet;

#[derive(Default)]
struct T {
    logs: u32, pre: u32, post: u32, post_with_casts: u32,
    // enemy-filtered casters only
    f_start_squad: u64, f_start_minion: u64, f_start_other: u64, f_start_null: u64, f_stop: u64,
    // agents arcdps tracked fully but we filed as enemies
    t_start: u64, t_null: u64, t_stop: u64,
    // contrast
    squad_start: u64, squad_stop: u64,
    pre_enemy_start: u64, pre_enemy_stop: u64, pre_squad_start: u64,
    enemy_dmg: u64, pre_enemy_dmg: u64,
}

fn main() {
    let list = std::env::args().nth(1).expect("usage: enemy_cast_census <listfile>");
    let paths: Vec<String> = std::fs::read_to_string(&list).unwrap()
        .lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    let mut t = T::default();
    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(raw) = decode_raw(&bytes) else { continue };
        let enc = axilog_core::model::resolve(&raw);
        if enc.kind != "wvw" { continue }
        let squad: BTreeSet<u64> = enc.players.iter().filter(|p| p.in_squad)
            .flat_map(|p| p.agent_addrs.iter().copied()).collect();
        let enemy: BTreeSet<u64> = enc.enemies.iter().filter(|e| e.is_player)
            .flat_map(|e| e.agent_addrs.iter().copied()).collect();
        if squad.is_empty() || enemy.is_empty() { continue }
        t.logs += 1;
        let post = raw.header.is_post_buff_rework();
        let tracked: BTreeSet<u64> = raw.events.iter()
            .filter(|e| e.is_statechange == sc::BUFF_INITIAL).map(|e| e.src_agent).collect();
        let squad_instids: BTreeSet<u16> = raw.events.iter()
            .filter(|e| squad.contains(&e.src_agent)).map(|e| e.src_instid).collect();
        let before = t.f_start_squad + t.f_start_minion;
        if post { t.post += 1 } else { t.pre += 1 }
        for e in &raw.events {
            let is_enemy = enemy.contains(&e.src_agent);
            let is_squad_src = squad.contains(&e.src_agent);
            let (is_start, is_stop) = if post {
                (e.is_statechange == sc::ANIMATION_START, e.is_statechange == sc::ANIMATION_STOP)
            } else {
                (e.is_statechange == 0 && matches!(e.is_activation, 1 | 2),
                 e.is_statechange == 0 && matches!(e.is_activation, 3..=6))
            };
            if !post {
                if is_enemy && is_start { t.pre_enemy_start += 1 }
                if is_enemy && is_stop { t.pre_enemy_stop += 1 }
                if is_squad_src && is_start { t.pre_squad_start += 1 }
                if e.is_statechange == 0 && e.is_activation == 0 && is_enemy && e.buff == 0
                    && e.is_buffremove == 0 && e.value > 0 && squad.contains(&e.dst_agent) { t.pre_enemy_dmg += 1 }
                continue;
            }
            if is_squad_src && is_start { t.squad_start += 1 }
            if is_squad_src && is_stop { t.squad_stop += 1 }
            if e.is_statechange == 0 && e.is_activation == 0 && is_enemy && e.buff == 0
                && e.is_buffremove == 0 && e.value > 0 && squad.contains(&e.dst_agent) { t.enemy_dmg += 1 }
            if !is_enemy { continue }
            let full = tracked.contains(&e.src_agent);
            if is_stop { if full { t.t_stop += 1 } else { t.f_stop += 1 } ; continue }
            if !is_start { continue }
            if full {
                if e.dst_agent == 0 { t.t_null += 1 } else { t.t_start += 1 }
                continue;
            }
            if e.dst_agent == 0 { t.f_start_null += 1 }
            else if squad.contains(&e.dst_agent) { t.f_start_squad += 1 }
            else if e.dst_master_instid != 0 && squad_instids.contains(&e.dst_master_instid) { t.f_start_minion += 1 }
            else { t.f_start_other += 1 }
        }
        if post && t.f_start_squad + t.f_start_minion > before { t.post_with_casts += 1 }
    }
    println!("WvW logs {}   pre-rework {}   post-rework {}   post with >=1 enemy cast {}",
        t.logs, t.pre, t.post, t.post_with_casts);
    println!("\n-- PRE-REWORK era (cast rows = is_activation on ordinary combat events) --");
    println!("  enemy cast START                 {}", t.pre_enemy_start);
    println!("  enemy cast END                   {}", t.pre_enemy_stop);
    println!("  squad cast START (contrast)      {}", t.pre_squad_start);
    println!("  enemy->squad strike rows         {}", t.pre_enemy_dmg);
    println!("\n-- POST-REWORK era (sc 67/68), casters visible ONLY via the enemy filter --");
    println!("  ANIMATION_START dst = squad player   {}", t.f_start_squad);
    println!("  ANIMATION_START dst = squad minion   {}", t.f_start_minion);
    println!("  ANIMATION_START dst = anything else  {}", t.f_start_other);
    println!("  ANIMATION_START dst = 0              {}", t.f_start_null);
    println!("  ANIMATION_STOP  (any)                {}", t.f_stop);
    println!("\n-- POST-REWORK, agents arcdps tracked FULLY but our roster filed as enemy --");
    println!("  ANIMATION_START dst != 0  {}", t.t_start);
    println!("  ANIMATION_START dst == 0  {}", t.t_null);
    println!("  ANIMATION_STOP            {}", t.t_stop);
    println!("\n-- POST-REWORK contrast --");
    println!("  squad ANIMATION_START {}   squad ANIMATION_STOP {}", t.squad_start, t.squad_stop);
    println!("  enemy->squad strike rows {}", t.enemy_dmg);
}
