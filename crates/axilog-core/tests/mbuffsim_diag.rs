//! MBUFFSIM Task 1 — **DIAGNOSIS ONLY** instrumentation.
//!
//! These tests are `#[ignore]`d: they assert nothing, they PRINT a
//! per-`(account, buff)` comparison of this project's simulated stack
//! timeline against GW2EI's own `buffUptimes[]` numbers in the local
//! reference export. They exist so Task 2's fix can be re-measured with the
//! same instrument, and so the numbers in
//! `.superpowers/sdd/2026-08-09-mbuffsim/task-1-report.md` are reproducible.
//!
//! ```sh
//! AXILOG_LOCAL_FIXTURES=/path/to/axilog/fixtures/local \
//!     cargo test -p axilog-core --release --test mbuffsim_diag -- \
//!     --ignored --nocapture
//! ```
//!
//! Why `buffUptimes` is a legitimate oracle for NON-boon buffs: GW2EI emits
//! one `buffUptimes[]` row per buff it tracked on the player, for EVERY
//! buff, not just boons (`buffMap` in this export carries 243 of them,
//! including `b69855` Relic of Fireworks and `b76865` Chant of Action). The
//! field semantics are the ones `analysis::buffs::uptime` already documents:
//! duration-type buffs put "% of the phase with >= 1 stack" in `uptime` and
//! leave `presence` at 0; intensity-type buffs put the time-weighted mean
//! stack count in `uptime` and the presence percentage in `presence`.

use axilog_core::analysis::buffs::{events, simulator, uptime, BoonTimeline};
use axilog_core::analysis::damage::InstidRegistry;
use axilog_core::analysis::damage_mods::catalog::buff_stack;
use axilog_core::evtc::{decode_raw, sc};
use axilog_core::model::resolve;
use std::collections::{BTreeMap, BTreeSet};

mod common;
#[path = "common/eiref.rs"]
mod eiref;

/// The diagnosis subjects, by class (see the Task 1 report).
///
/// A: `d422` Might 25. B: Stability `d-425..-428`. C: `d312`/`d369`
/// Force-type. D: `d174`/`d111` distinct-boons-present counters (their
/// trackers are over the twelve boons, so the whole boon set is the
/// subject; Aegis/Protection/Resolution/Fury are the ones the two
/// definitions actually watch).
const TARGETS: &[(u32, &str, &str)] = &[
    (740, "Might", "A"),
    (1122, "Stability", "B"),
    (69855, "Relic of Fireworks", "C"),
    (76865, "Chant of Action", "C"),
    (743, "Aegis", "D"),
    (717, "Protection", "D"),
    (873, "Retaliation/Resolution-slot", "D"),
    (725, "Fury", "D"),
    (1187, "Quickness", "D"),
    (30328, "Alacrity", "D"),
    (719, "Swiftness", "D"),
    (726, "Vigor", "D"),
    (718, "Regeneration", "D"),
    (26980, "Resistance", "D"),
];

struct Row {
    account: String,
    ours_presence: f64,
    ours_avg: f64,
    ei_uptime: f64,
    ei_presence: f64,
}

#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_buff_uptimes_vs_ei() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let Some(golden_s) =
        std::fs::read_to_string(common::local_fixture("wvw-postrework.ei.json")).ok()
    else {
        println!("skip: local golden absent");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_s).unwrap();

    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();

    let ids: BTreeSet<u32> = TARGETS.iter().map(|&(id, _, _)| id).collect();
    let evs = events::extract_buff_events_with_registry(&raw, &registry, &ids);
    let caps = events::extract_buff_capacities(&raw, &ids);
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);

    println!("\nlog window: [{log_start_ms}, {log_end_ms}]  ({} ms)", log_end_ms - log_start_ms);
    println!("\n-- capacity source per target id --");
    for &(id, name, class) in TARGETS {
        let cat = buff_stack::stack_info(id);
        println!(
            "  b{id:<6} {name:<28} class {class}  BUFF_INFO={:?}  catalog=(intensity {:?}, cap {:?})  USED={}",
            caps.get(&id),
            cat.map(|c| c.stack_type),
            cat.map(|c| c.capacity),
            caps.get(&id)
                .copied()
                .unwrap_or_else(|| cat.map(|c| c.capacity).unwrap_or(5)),
        );
    }

    // Event-kind histogram per target id (all agents).
    let mut hist: BTreeMap<u32, [u64; 4]> = BTreeMap::new();
    for e in &evs {
        let h = hist.entry(e.buff_id).or_default();
        let i = match e.kind {
            events::BuffEventKind::Apply { .. } => 0,
            events::BuffEventKind::RemoveSingle { .. } => 1,
            events::BuffEventKind::RemoveAll => 2,
            events::BuffEventKind::Extend { .. } => 3,
        };
        h[i] += 1;
    }
    println!("\n-- event-kind histogram (apply / remove-single / remove-all / extend) --");
    for &(id, name, _) in TARGETS {
        let h = hist.get(&id).copied().unwrap_or_default();
        println!("  b{id:<6} {name:<28} {:>7} {:>7} {:>7} {:>7}", h[0], h[1], h[2], h[3]);
    }

    // Extend fine structure: how often is `new_duration - extended <= 0`
    // (the branch that ADDS a stack instead of extending the active one)?
    println!("\n-- Extend shape per target id --");
    for &(id, name, _) in TARGETS {
        let (mut n, mut nonpos_old, mut zero_ext, mut ext_gt_new) = (0u64, 0u64, 0u64, 0u64);
        for e in evs.iter().filter(|e| e.buff_id == id) {
            if let events::BuffEventKind::Extend { extended_ms, new_duration_ms } = e.kind {
                n += 1;
                if new_duration_ms as i64 - extended_ms as i64 <= 0 {
                    nonpos_old += 1;
                }
                if extended_ms == 0 {
                    zero_ext += 1;
                }
                if extended_ms > new_duration_ms {
                    ext_gt_new += 1;
                }
            }
        }
        if n > 0 {
            println!(
                "  b{id:<6} {name:<28} extends={n:<7} oldValue<=0: {nonpos_old:<7} \
                 extended==0: {zero_ext:<7} extended>new: {ext_gt_new}"
            );
        }
    }

    // Simulate + compare against EI's own numbers.
    let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    for &e in &evs {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }
    let mut timelines: BTreeMap<(u64, u32), BoonTimeline> = BTreeMap::new();
    for ((rep, id), group) in grouped {
        let capacity = caps.get(&id).copied().unwrap_or_else(|| {
            buff_stack::stack_info(id).map(|b| b.capacity).unwrap_or(5)
        });
        let intensity = buff_stack::is_intensity(id);
        timelines.insert(
            (rep, id),
            BoonTimeline { states: simulator::run(group, capacity, intensity, log_end_ms) },
        );
    }

    // EI side.
    let mut ei: BTreeMap<(String, u32), (f64, f64)> = BTreeMap::new();
    for p in golden["players"].as_array().unwrap() {
        let Some(a) = p["account"].as_str() else { continue };
        let key = common::account_key(a).to_string();
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            let Some(id) = b["id"].as_u64() else { continue };
            if !ids.contains(&(id as u32)) {
                continue;
            }
            let d = &b["buffData"][0];
            ei.insert(
                (key.clone(), id as u32),
                (d["uptime"].as_f64().unwrap_or(0.0), d["presence"].as_f64().unwrap_or(0.0)),
            );
        }
    }

    for &(id, name, class) in TARGETS {
        let intensity = buff_stack::is_intensity(id);
        let mut rows: Vec<Row> = Vec::new();
        for p in &enc.players {
            let key = common::account_key(&p.account).to_string();
            let ours = timelines
                .get(&(p.agent_addr, id))
                .map(|tl| uptime::compute(tl, log_start_ms, log_end_ms));
            let g = ei.get(&(key.clone(), id));
            if ours.is_none() && g.is_none() {
                continue;
            }
            let o = ours.unwrap_or(uptime::BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 });
            let (gu, gp) = g.copied().unwrap_or((0.0, 0.0));
            rows.push(Row {
                account: key,
                ours_presence: o.presence_pct,
                ours_avg: o.avg_stacks,
                ei_uptime: gu,
                ei_presence: gp,
            });
        }
        if rows.is_empty() {
            continue;
        }
        println!(
            "\n=== class {class}  b{id} {name}  ({}) -- {} row(s) ===",
            if intensity { "intensity" } else { "duration" },
            rows.len()
        );
        println!(
            "{:<10} {:>12} {:>12} {:>10}   {:>12} {:>12} {:>10}",
            "account", "ours.pres", "ei.pres", "d", "ours.avg", "ei.avg", "d"
        );
        let (mut sp, mut sa) = (0.0f64, 0.0f64);
        for r in &rows {
            // Duration buffs: EI's `uptime` IS the presence percentage and
            // `presence` is unset. Intensity: `uptime` is avg stacks.
            let (ei_pres, ei_avg) =
                if intensity { (r.ei_presence, r.ei_uptime) } else { (r.ei_uptime, 0.0) };
            let dp = r.ours_presence - ei_pres;
            let da = r.ours_avg - ei_avg;
            sp += dp.abs();
            sa += da.abs();
            println!(
                "{:<10} {:>12.3} {:>12.3} {:>+10.3}   {:>12.3} {:>12.3} {:>+10.3}",
                r.account, r.ours_presence, ei_pres, dp, r.ours_avg, ei_avg, da
            );
        }
        println!(
            "  mean |d presence| = {:.4} pp, mean |d avg-stacks| = {:.4}",
            sp / rows.len() as f64,
            sa / rows.len() as f64
        );
    }
}

/// **Cell-by-cell ledger for the ALWAYS-ON committed fixture**, through
/// axilog's REAL pipeline (`analyze` -> `Metrics::boon_uptime`), not the
/// `eiref` port. Prints one stable, PII-free line per
/// `(agent index, boon, field)` golden cell with `ours`, `EI` and
/// `|ours - EI|`, so a before/after run of this test can be diffed
/// mechanically and every moved cell justified individually.
///
/// This is the instrument MBUFFSIM Task 2's "any moved cell must be an
/// improvement" gate is measured with.
#[test]
#[ignore = "MBUFFSIM diagnosis instrumentation; prints the committed-fixture boon cell ledger"]
fn diag_committed_fixture_boon_cell_ledger() {
    const ANON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
    const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
    let bytes = std::fs::read(ANON).expect("committed fixture");
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(GOLDEN).expect("golden")).expect("json");

    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let by_addr: BTreeMap<u64, u64> = enc.players.iter().map(|p| (p.agent_addr, p.agent_addr)).collect();

    let mut boons_by_account: BTreeMap<String, &serde_json::Value> = BTreeMap::new();
    for p in golden["players"].as_array().expect("players") {
        if let (Some(a), Some(b)) = (p["account"].as_str(), p.get("boons")) {
            boons_by_account.insert(a.to_string(), b);
        }
    }

    println!("\n# committed-fixture boon cell ledger (agent-table index, no PII)");
    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let key = axilog_core::evtc::anon_account(i).trim_start_matches(':').to_string();
        let Some(&boons) = boons_by_account.get(&key) else { continue };
        if !by_addr.contains_key(&agent.addr) {
            continue;
        }
        for &(boon_id, name, is_intensity) in axilog_core::analysis::buffs::BOON_IDS.iter() {
            let Some(g) = boons.get(boon_id.to_string().as_str()) else { continue };
            let g_uptime = g["uptime"].as_f64().unwrap_or(0.0);
            let g_presence = g["presence"].as_f64().unwrap_or(0.0);
            let ours = metrics
                .boon_uptime
                .get(&(agent.addr, boon_id))
                .copied()
                .unwrap_or(uptime::BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 });
            let (ei_pres, ei_avg) =
                if is_intensity { (g_presence, g_uptime) } else { (g_uptime, f64::NAN) };
            println!(
                "cell a{i:03} {name:<12} presence ours={:.6} ei={:.6} d={:.6}",
                ours.presence_pct,
                ei_pres,
                (ours.presence_pct - ei_pres).abs()
            );
            if is_intensity {
                let rel = if ei_avg.abs() > 1e-9 {
                    (ours.avg_stacks - ei_avg).abs() / ei_avg.abs()
                } else {
                    (ours.avg_stacks - ei_avg).abs()
                };
                println!(
                    "cell a{i:03} {name:<12} avgstack ours={:.6} ei={:.6} rel={:.6}",
                    ours.avg_stacks, ei_avg, rel
                );
            }
        }
    }
}

/// The decisive experiment: run the SAME extracted event stream through
/// (a) `analysis::buffs::simulator` and (b) [`eiref`], a literal port of
/// GW2EI's NoID simulator family, and compare BOTH against GW2EI's own
/// `buffUptimes[]`. If (b) lands on EI's number and (a) does not, the gap is
/// entirely inside `simulator.rs` and the diff between the two ports names
/// the rule.
#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_three_way_axilog_vs_eiref_vs_ei() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let Some(golden_s) =
        std::fs::read_to_string(common::local_fixture("wvw-postrework.ei.json")).ok()
    else {
        println!("skip: local golden absent");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_s).unwrap();

    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let ids: BTreeSet<u32> = TARGETS.iter().map(|&(id, _, _)| id).collect();
    let evs = events::extract_buff_events_with_registry(&raw, &registry, &ids);
    let caps = events::extract_buff_capacities(&raw, &ids);
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);

    let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    for &e in &evs {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }
    for g in grouped.values_mut() {
        g.sort_by_key(|e| e.time);
    }

    let mut ei: BTreeMap<(String, u32), (f64, f64)> = BTreeMap::new();
    for p in golden["players"].as_array().unwrap() {
        let Some(a) = p["account"].as_str() else { continue };
        let key = common::account_key(a).to_string();
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            let Some(id) = b["id"].as_u64() else { continue };
            if !ids.contains(&(id as u32)) {
                continue;
            }
            let d = &b["buffData"][0];
            ei.insert(
                (key.clone(), id as u32),
                (d["uptime"].as_f64().unwrap_or(0.0), d["presence"].as_f64().unwrap_or(0.0)),
            );
        }
    }

    for &(id, name, class) in TARGETS {
        let intensity = buff_stack::is_intensity(id);
        let stack_type = if intensity {
            eiref::StackType::Override
        } else if buff_stack::stack_info(id).map(|b| b.capacity) == Some(1) {
            eiref::StackType::Force
        } else {
            eiref::StackType::Queue
        };
        let capacity =
            caps.get(&id).copied().unwrap_or_else(|| {
                buff_stack::stack_info(id).map(|b| b.capacity).unwrap_or(5)
            });
        let mut printed = false;
        let (mut n, mut sum_a, mut sum_r) = (0usize, 0.0f64, 0.0f64);
        for p in &enc.players {
            let key = common::account_key(&p.account).to_string();
            let Some(group) = grouped.get(&(p.agent_addr, id)) else { continue };
            let Some(&(ei_uptime, ei_presence)) = ei.get(&(key.clone(), id)) else { continue };
            let (ei_pres, ei_avg) =
                if intensity { (ei_presence, ei_uptime) } else { (ei_uptime, 1.0) };

            let ours = uptime::compute(
                &BoonTimeline {
                    states: simulator::run(group.clone(), capacity, intensity, log_end_ms),
                },
                log_start_ms,
                log_end_ms,
            );
            let mut sim = eiref::Sim::new(capacity, stack_type);
            sim.simulate(group, log_start_ms, log_end_ms);
            let (rp, ra) = sim.graph_uptime(log_start_ms, log_end_ms);

            if !printed {
                println!(
                    "\n=== class {class}  b{id} {name}  ({stack_type:?}, cap {capacity}) ===\n\
                     {:<10} {:>9} {:>9} {:>9} | {:>9} {:>9} {:>9}",
                    "account", "ax.pres", "ref.pres", "ei.pres", "ax.avg", "ref.avg", "ei.avg"
                );
                printed = true;
            }
            // For duration buffs EI's graph value is a constant 1, so
            // `ref.avg` is by construction the presence fraction; only the
            // presence columns are meaningful there.
            println!(
                "{:<10} {:>9.3} {:>9.3} {:>9.3} | {:>9.3} {:>9.3} {:>9.3}",
                key, ours.presence_pct, rp, ei_pres, ours.avg_stacks, ra, ei_avg
            );
            n += 1;
            sum_a += (if intensity { ours.avg_stacks - ei_avg } else { ours.presence_pct - ei_pres })
                .abs();
            sum_r += (if intensity { ra - ei_avg } else { rp - ei_pres }).abs();
        }
        if n > 0 {
            println!(
                "  mean |axilog - EI| = {:.5}   mean |eiref - EI| = {:.5}   ({n} rows)",
                sum_a / n as f64,
                sum_r / n as f64
            );
        }
    }
}

/// **The hypothesis test.** GW2EI drops a `BuffRemove.Single` from the
/// simulator's event list entirely when
/// `BuffRemoveSingleEvent.OverstackOrNaturalEnd` holds
/// (`GW2EIEvtcParser/ParsedData/CombatEvents/BuffEvents/BuffRemoves/
/// BuffRemoveSingleEvent.cs:11,26-38` + `EIData/Buffs/BuffDictionary.cs:83-86`):
///
/// ```csharp
/// internal bool OverstackOrNaturalEnd =>
///     (IFF == IFF.Unknown && CreditedBy.IsUnknown && !_byShouldntBeUnknown);
/// // _byShouldntBeUnknown = evtcItem.DstAgent != 0
/// internal override bool IsBuffSimulatorCompliant(bool useBuffInstanceSimulator)
///     => ... : !OverstackOrNaturalEnd;   // useBuffInstanceSimulator == false here
/// ```
///
/// i.e. a SINGLE removal with `dst_agent == 0` and `iff == Unknown` is
/// arcdps telling you a stack ended on its own (or was overstacked), NOT
/// that something stripped it — the simulator already models that expiry, so
/// applying the event again double-counts it. This test re-runs the EI
/// reference simulator with and without that filter.
#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_overstack_or_natural_end_filter() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let Some(golden_s) =
        std::fs::read_to_string(common::local_fixture("wvw-postrework.ei.json")).ok()
    else {
        println!("skip: local golden absent");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_s).unwrap();
    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let ids: BTreeSet<u32> = TARGETS.iter().map(|&(id, _, _)| id).collect();
    let caps = events::extract_buff_capacities(&raw, &ids);
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);

    // `RemoveSingle` rows that GW2EI would drop, keyed by (time, owner,
    // buff, value) — enough to identify them in the extracted stream.
    const IFF_UNKNOWN: u8 = 2;
    let mut dropped: BTreeMap<(u64, u64, u32, i32), u32> = BTreeMap::new();
    let (mut n_single, mut n_dropped) = (0u64, 0u64);
    let mut dropped_per_buff: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    for e in &raw.events {
        if !ids.contains(&e.skillid) {
            continue;
        }
        let is_single = e.is_statechange == axilog_core::evtc::sc::BUFF_REMOVE_SINGLE
            && e.is_buffremove == axilog_core::evtc::buff_remove::SINGLE;
        if !is_single {
            continue;
        }
        n_single += 1;
        let ent = dropped_per_buff.entry(e.skillid).or_default();
        ent.0 += 1;
        if e.dst_agent == 0 && e.iff == IFF_UNKNOWN {
            n_dropped += 1;
            ent.1 += 1;
            *dropped.entry((e.time, e.src_agent, e.skillid, e.value)).or_default() += 1;
        }
    }
    println!(
        "\nSINGLE removals over the target ids: {n_single}, of which \
         OverstackOrNaturalEnd (dst_agent==0 && iff==Unknown): {n_dropped} \
         ({:.1}%)",
        100.0 * n_dropped as f64 / n_single.max(1) as f64
    );
    for &(id, name, _) in TARGETS {
        if let Some(&(tot, dr)) = dropped_per_buff.get(&id) {
            println!(
                "  b{id:<6} {name:<28} {dr}/{tot} dropped ({:.1}%)",
                100.0 * dr as f64 / tot.max(1) as f64
            );
        }
    }

    let evs = events::extract_buff_events_with_registry(&raw, &registry, &ids);
    let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    let mut grouped_filtered: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    let mut budget = dropped.clone();
    for &e in &evs {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
        let drop_it = match e.kind {
            events::BuffEventKind::RemoveSingle { removed_duration_ms } => {
                let k = (e.time, e.owner, e.buff_id, removed_duration_ms as i32);
                match budget.get_mut(&k) {
                    Some(n) if *n > 0 => {
                        *n -= 1;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if !drop_it {
            grouped_filtered.entry((rep, e.buff_id)).or_default().push(e);
        }
    }

    let mut ei: BTreeMap<(String, u32), (f64, f64)> = BTreeMap::new();
    for p in golden["players"].as_array().unwrap() {
        let Some(a) = p["account"].as_str() else { continue };
        let key = common::account_key(a).to_string();
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            let Some(id) = b["id"].as_u64() else { continue };
            if !ids.contains(&(id as u32)) {
                continue;
            }
            let d = &b["buffData"][0];
            ei.insert(
                (key.clone(), id as u32),
                (d["uptime"].as_f64().unwrap_or(0.0), d["presence"].as_f64().unwrap_or(0.0)),
            );
        }
    }

    println!(
        "\n{:<30} {:>16} {:>16} {:>16}",
        "buff", "|axilog-EI|", "|eiref-EI|", "|eiref+filter-EI|"
    );
    for &(id, name, class) in TARGETS {
        let intensity = buff_stack::is_intensity(id);
        let stack_type = if intensity {
            eiref::StackType::Override
        } else if buff_stack::stack_info(id).map(|b| b.capacity) == Some(1) {
            eiref::StackType::Force
        } else {
            eiref::StackType::Queue
        };
        let capacity = caps.get(&id).copied().unwrap_or_else(|| {
            buff_stack::stack_info(id).map(|b| b.capacity).unwrap_or(5)
        });
        let (mut n, mut s_ax, mut s_ref, mut s_flt) = (0usize, 0.0f64, 0.0f64, 0.0f64);
        for p in &enc.players {
            let key = common::account_key(&p.account).to_string();
            let Some(group) = grouped.get(&(p.agent_addr, id)) else { continue };
            let Some(&(ei_uptime, ei_presence)) = ei.get(&(key.clone(), id)) else { continue };
            let (ei_pres, ei_avg) = (if intensity { ei_presence } else { ei_uptime }, ei_uptime);
            let ours = uptime::compute(
                &BoonTimeline {
                    states: simulator::run(group.clone(), capacity, intensity, log_end_ms),
                },
                log_start_ms,
                log_end_ms,
            );
            let mut a = eiref::Sim::new(capacity, stack_type);
            a.simulate(group, log_start_ms, log_end_ms);
            let (ap, aa) = a.graph_uptime(log_start_ms, log_end_ms);
            let empty = Vec::new();
            let fg = grouped_filtered.get(&(p.agent_addr, id)).unwrap_or(&empty);
            let mut b = eiref::Sim::new(capacity, stack_type);
            b.simulate(fg, log_start_ms, log_end_ms);
            let (bp, ba) = b.graph_uptime(log_start_ms, log_end_ms);
            n += 1;
            if intensity {
                s_ax += (ours.avg_stacks - ei_avg).abs();
                s_ref += (aa - ei_avg).abs();
                s_flt += (ba - ei_avg).abs();
            } else {
                s_ax += (ours.presence_pct - ei_pres).abs();
                s_ref += (ap - ei_pres).abs();
                s_flt += (bp - ei_pres).abs();
            }
        }
        if n > 0 {
            println!(
                "{:<30} {:>16.5} {:>16.5} {:>16.5}   (class {class}, {n} rows)",
                format!("b{id} {name}"),
                s_ax / n as f64,
                s_ref / n as f64,
                s_flt / n as f64
            );
        }
    }
}

/// Class B: on top of the `OverstackOrNaturalEnd` filter, apply GW2EI's
/// Stability-specific "band aid for the stack type situation with fake
/// inactive/infinite durations" (`GW2EIEvtcParser/EIData/Buffs/
/// BuffsContainer.cs:196-252`): for a `StackingConditionalLoss` buff, a REAL
/// (non-overstack) SINGLE removal whose `RemovedDuration` equals the
/// matching stack's TOTAL APPLIED duration is arcdps reporting the ORIGINAL
/// duration rather than the remaining one, and GW2EI rewrites it to
/// `RemovedDuration - activeTime - elapsedTime` before simulating.
#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_stability_removed_duration_band_aid() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let Some(golden_s) =
        std::fs::read_to_string(common::local_fixture("wvw-postrework.ei.json")).ok()
    else {
        println!("skip: local golden absent");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_s).unwrap();
    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    const STABILITY: u32 = 1122;
    const IFF_UNKNOWN: u8 = 2;
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    let capacity = events::extract_buff_capacities(&raw, &[STABILITY].into_iter().collect())
        .get(&STABILITY)
        .copied()
        .unwrap_or(25);

    // Rebuild the Stability stream straight off the wire so `pad`
    // (`BuffInstance`) is available.
    #[derive(Clone, Copy)]
    struct W {
        time: u64,
        owner: u64,
        inst: u32,
        value: i32,
        kind: u8, // 0 apply, 1 remove-single, 2 remove-all, 3 extend
        overstack_end: bool,
        is_shields: bool,
        new_duration: u32,
    }
    let mut wire: Vec<W> = Vec::new();
    for e in &raw.events {
        if e.skillid != STABILITY {
            continue;
        }
        let (kind, owner) = match e.is_statechange {
            x if x == sc::BUFF_APPLY || x == sc::BUFF_INITIAL => (0u8, e.dst_agent),
            x if x == sc::BUFF_CHANGE => (3, e.dst_agent),
            x if x == sc::BUFF_REMOVE_SINGLE => {
                if e.is_buffremove != axilog_core::evtc::buff_remove::SINGLE {
                    continue;
                }
                (1, e.src_agent)
            }
            x if x == sc::BUFF_REMOVE_ALL => (2, e.src_agent),
            _ => continue,
        };
        wire.push(W {
            time: e.time,
            owner,
            inst: e.pad,
            value: e.value,
            kind,
            overstack_end: e.dst_agent == 0 && e.iff == IFF_UNKNOWN,
            is_shields: e.is_shields != 0,
            new_duration: e.overstack,
        });
    }

    // The band aid, per (owner, BuffInstance).
    let mut applies: BTreeMap<(u64, u32), Vec<(u64, i32)>> = BTreeMap::new();
    for w in &wire {
        if w.kind == 0 {
            applies.entry((w.owner, w.inst)).or_default().push((w.time, w.value));
        }
    }
    let (mut real, mut rewritten) = (0u64, 0u64);
    let mut fixed_wire = wire.clone();
    for w in fixed_wire.iter_mut() {
        if w.kind != 1 || w.overstack_end {
            continue;
        }
        real += 1;
        let Some(list) = applies.get(&(w.owner, w.inst)) else { continue };
        let Some(&(at, av)) = list.iter().rev().find(|&&(t, _)| t <= w.time) else { continue };
        // `totalDuration == remove.RemovedDuration` (extensions and
        // `BuffStackActiveEvent` corrections omitted: this buff has 5
        // extensions in the whole log).
        if av == w.value {
            let elapsed = (w.time - at) as i32;
            w.value = (w.value - elapsed).max(0);
            rewritten += 1;
        }
    }
    println!(
        "\nStability: {real} non-overstack SINGLE removals, {rewritten} matched the \
         band aid's `totalDuration == RemovedDuration` test and were rewritten"
    );

    let to_events = |src: &[W], filter_natural: bool| -> BTreeMap<u64, Vec<events::BuffEvent>> {
        let mut out: BTreeMap<u64, Vec<events::BuffEvent>> = BTreeMap::new();
        for w in src {
            if w.kind == 1 && filter_natural && w.overstack_end {
                continue;
            }
            let Some(&rep) = addr_to_rep.get(&w.owner) else { continue };
            let kind = match w.kind {
                0 => events::BuffEventKind::Apply {
                    duration_ms: w.value.max(0) as u32,
                    is_shields: w.is_shields,
                },
                1 => events::BuffEventKind::RemoveSingle {
                    removed_duration_ms: w.value.max(0) as u32,
                },
                2 => events::BuffEventKind::RemoveAll,
                _ => events::BuffEventKind::Extend {
                    extended_ms: w.value.max(0) as u32,
                    new_duration_ms: w.new_duration,
                },
            };
            out.entry(rep).or_default().push(events::BuffEvent {
                time: w.time,
                buff_id: STABILITY,
                owner: w.owner,
                agent: 0,
                buff_instance: w.inst,
                kind,
            });
        }
        out
    };
    let raw_ev = to_events(&wire, false);
    let filt_ev = to_events(&wire, true);
    let band_ev = to_events(&fixed_wire, true);

    let mut ei: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for p in golden["players"].as_array().unwrap() {
        let Some(a) = p["account"].as_str() else { continue };
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            if b["id"].as_u64() != Some(STABILITY as u64) {
                continue;
            }
            let d = &b["buffData"][0];
            ei.insert(
                common::account_key(a).to_string(),
                (d["uptime"].as_f64().unwrap_or(0.0), d["presence"].as_f64().unwrap_or(0.0)),
            );
        }
    }

    let run = |evs: &Vec<events::BuffEvent>| -> f64 {
        let mut s = eiref::Sim::new(capacity, eiref::StackType::Override);
        s.simulate(evs, log_start_ms, log_end_ms);
        s.graph_uptime(log_start_ms, log_end_ms).1
    };
    let (mut n, mut s0, mut s1, mut s2) = (0usize, 0.0f64, 0.0f64, 0.0f64);
    println!(
        "{:<12} {:>10} {:>10} {:>10} {:>10}",
        "account", "raw", "+filter", "+bandaid", "EI"
    );
    let empty = Vec::new();
    for p in &enc.players {
        let key = common::account_key(&p.account).to_string();
        let Some(&(ei_avg, _)) = ei.get(&key) else { continue };
        let Some(g0) = raw_ev.get(&p.agent_addr) else { continue };
        let a0 = run(g0);
        let a1 = run(filt_ev.get(&p.agent_addr).unwrap_or(&empty));
        let a2 = run(band_ev.get(&p.agent_addr).unwrap_or(&empty));
        println!("{key:<12} {a0:>10.3} {a1:>10.3} {a2:>10.3} {ei_avg:>10.3}");
        n += 1;
        s0 += (a0 - ei_avg).abs();
        s1 += (a1 - ei_avg).abs();
        s2 += (a2 - ei_avg).abs();
    }
    println!(
        "\nmean |d avg-stacks| vs EI: raw {:.5}  +natural-end filter {:.5}  \
         +band aid {:.5}   ({n} rows)",
        s0 / n as f64,
        s1 / n as f64,
        s2 / n as f64
    );
}

/// Raw-wire dump for one `(account, buff)` pair, so a divergence can be
/// hand-checked against the C#. Set `MBUFFSIM_DUMP_BUFF` to a buff id and
/// `MBUFFSIM_DUMP_N` to a row budget.
#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_dump_raw_buff_events() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let id: u32 = std::env::var("MBUFFSIM_DUMP_BUFF").ok().and_then(|s| s.parse().ok()).unwrap_or(69855);
    let budget: usize = std::env::var("MBUFFSIM_DUMP_N").ok().and_then(|s| s.parse().ok()).unwrap_or(80);
    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);

    // Pick the agent with the most rows for this buff.
    let mut per_owner: BTreeMap<u64, usize> = BTreeMap::new();
    for e in &raw.events {
        if e.skillid != id {
            continue;
        }
        let owner = if e.is_buffremove != 0 { e.src_agent } else { e.dst_agent };
        *per_owner.entry(owner).or_default() += 1;
    }
    let Some((&owner, _)) = per_owner.iter().max_by_key(|&(_, n)| *n) else {
        println!("no rows for b{id}");
        return;
    };
    let squad: BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    println!(
        "b{id}: {} owners, busiest owner has {} rows (squad member: {})",
        per_owner.len(),
        per_owner[&owner],
        squad.contains(&owner)
    );
    println!(
        "{:>10} {:>4} {:>4} {:>4} {:>9} {:>9} {:>4} {:>4} {:>4}",
        "time", "sc", "brm", "act", "value", "overstk", "shld", "ofcy", "rslt"
    );
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);
    let mut n = 0;
    for e in &raw.events {
        if e.skillid != id {
            continue;
        }
        let o = if e.is_buffremove != 0 { e.src_agent } else { e.dst_agent };
        if o != owner {
            continue;
        }
        println!(
            "{:>10} {:>4} {:>4} {:>4} {:>9} {:>9} {:>4} {:>4} {:>4}",
            e.time - t0,
            e.is_statechange,
            e.is_buffremove,
            e.is_activation,
            e.value,
            e.overstack,
            e.is_shields,
            e.is_offcycle,
            e.result
        );
        n += 1;
        if n >= budget {
            println!("... (truncated)");
            break;
        }
    }
}

/// Class A fine structure: what fraction of the fight does each account sit
/// at EXACTLY 25 Might in our simulation, and how does the whole stack
/// histogram look? `d422` fires only at saturation, so this is the number
/// that has to move.
#[test]
#[ignore = "MBUFFSIM Task 1 diagnosis instrumentation; needs the local PII fixture"]
fn diag_might_saturation_histogram() {
    let Some(bytes) = std::fs::read(common::local_fixture("wvw-postrework.zevtc")).ok() else {
        println!("skip: local fixture absent");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let ids: BTreeSet<u32> = [740u32].into_iter().collect();
    let evs = events::extract_buff_events_with_registry(&raw, &registry, &ids);
    let caps = events::extract_buff_capacities(&raw, &ids);
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    let capacity = caps.get(&740).copied().unwrap_or(25);
    println!("Might capacity in use: {capacity}");

    let mut grouped: BTreeMap<u64, Vec<events::BuffEvent>> = BTreeMap::new();
    for &e in &evs {
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry(rep).or_default().push(e);
    }
    let window = (log_end_ms - log_start_ms) as f64;
    let mut agg = [0u128; 27];
    for (_rep, group) in grouped {
        let states = simulator::run(group, capacity, true, log_end_ms);
        let mut it = states.iter().peekable();
        while let Some(&(t, c)) = it.next() {
            let s = t.max(log_start_ms);
            let e = it.peek().map(|&&(n, _)| n).unwrap_or(log_end_ms).min(log_end_ms);
            if e > s {
                agg[(c as usize).min(26)] += (e - s) as u128;
            }
        }
    }
    println!("\nMight stack-count time histogram (all squad accounts, ms and % of one fight):");
    for (c, &ms) in agg.iter().enumerate() {
        if ms > 0 {
            println!("  {c:>3} stacks: {ms:>12} ms  ({:.3}% of a fight)", ms as f64 / window * 100.0);
        }
    }
    println!("  == 25 total: {} ms", agg[25]);
}
