//! MEIGAP Task 2: ei-json calibration for the series splits and the
//! `targets[]` mirrors, against the real Elite Insights export for this
//! project's local post-rework WvW capture
//! (`fixtures/local/wvw-postrework.ei.json`, gitignored -- every test here
//! skips cleanly when it is absent, the same `AXILOG_LOCAL_FIXTURES`
//! pattern `meigap_ei_golden.rs`/`damage_mods_ei_golden.rs` use).
//!
//! Families, with the GW2EI semantics each one is calibrated against:
//!
//! a. `powerDamageTaken1S` + `targetPowerDamage1S` -- the POWER half of the
//!    per-player series (`JsonPlayerBuilder.cs:75-76,99-100`, filter at
//!    `Actor.cs:449-451`). See `axilog_core::analysis::timeseries`'s
//!    POWER-split section.
//! b. `targets[].damage1S` + `.powerDamage1S` -- per-ENEMY OUTGOING series
//!    (the shared `JsonActorBuilder.FillJsonActor:108-109` over an NPC).
//! c. `targets[].totalDamageDist[0]` -- per-enemy OUTGOING per-skill
//!    distribution, actor-only (`JsonActorBuilder.cs:130-137` ->
//!    `GetJustActorDamageEvents`). See
//!    `axilog_core::analysis::skill_damage::build_enemy_dist`.
//! d. `targets[].buffs[].id` + `.statesPerSource` -- incoming-condition
//!    attribution on enemy players (`JsonNPCBuilder.cs:87-118`). See
//!    `axilog_core::analysis::target_conditions`.
//!
//! ## The targets join
//!
//! Same M16 pattern `meigap_ei_golden.rs`'s `statsTargets` test
//! established, and for the same reason: GW2EI's WvW logic curates 57
//! targets while this project's `targets[]` is the full unfiltered enemy
//! roster (624 here), and the two name spaces do not intersect. The join
//! goes through arcdps AGENT IDENTITY -- the instid GW2EI encodes into its
//! `"<Spec> pl-<instid>"` placeholder name -> the addr that instid belonged
//! to -> this project's enemy index. Only joined targets are calibratable;
//! they are asserted, the rest are not compared.

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_ei::EiInputs;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

struct Calibration {
    ours: Value,
    golden: Value,
    /// `(our targets[] index, reference targets[] index)` for every target
    /// both sides agree is the same arcdps agent.
    joinable: Vec<(usize, usize)>,
}

/// Parses the local capture and renders it with every flag-gated block on
/// (mirroring axibridge's own `mapEiSettingsToAxilogOptions`, which forces
/// `skillDamage` and passes `rawTimelineArrays` through as `timeseries`).
fn render_and_reference(label: &str) -> Option<Calibration> {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let Ok(bytes) = std::fs::read(&zevtc) else {
        println!("skip: {zevtc} absent ({label})");
        return None;
    };
    let Ok(golden_s) = std::fs::read_to_string(&ei_json) else {
        println!("skip: {ei_json} absent ({label})");
        return None;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );

    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let enemies: BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let enemy_addr_to_rep: BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    let enemy_series = axilog_core::analysis::timeseries::build_enemy_series(
        &enc,
        &raw,
        &registry,
        &enemies,
        &enemy_addr_to_rep,
    );
    let enemy_dist =
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &enemy_addr_to_rep);
    let target_conditions =
        axilog_core::analysis::target_conditions::build_with_registry(&raw, &registry, &enc);

    let ours = axilog_ei::to_ei_json(
        &report,
        &EiInputs {
            activity: &activity,
            enemy_series: Some(&enemy_series),
            enemy_dist: Some(&enemy_dist),
            target_conditions: Some(&target_conditions),
            ..Default::default()
        },
    );

    // --- the instid-based targets join (see this file's module doc) ---
    let mut instid_to_addrs: BTreeMap<u16, BTreeSet<u64>> = BTreeMap::new();
    for ev in &raw.events {
        for (instid, addr) in [(ev.src_instid, ev.src_agent), (ev.dst_instid, ev.dst_agent)] {
            if instid != 0 && addr != 0 {
                instid_to_addrs.entry(instid).or_default().insert(addr);
            }
        }
    }
    let addr_to_enemy_index: BTreeMap<u64, usize> = enc
        .enemies
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.agent_addrs.iter().map(move |&a| (a, i)))
        .collect();
    let mut joinable: Vec<(usize, usize)> = Vec::new();
    for (g_i, t) in golden["targets"].as_array().expect("reference targets").iter().enumerate() {
        let Some(instid) = t["name"]
            .as_str()
            .and_then(|n| n.rsplit_once("pl-"))
            .and_then(|(_, s)| s.parse::<u16>().ok())
        else {
            continue;
        };
        let indices: BTreeSet<usize> = instid_to_addrs
            .get(&instid)
            .into_iter()
            .flatten()
            .filter_map(|a| addr_to_enemy_index.get(a).copied())
            .collect();
        if indices.len() == 1 {
            joinable.push((*indices.iter().next().expect("len == 1"), g_i));
        }
    }

    Some(Calibration { ours, golden, joinable })
}

fn players_by_account(v: &Value) -> BTreeMap<String, &Value> {
    v["players"]
        .as_array()
        .expect("players array")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a).to_string(), p)))
        .collect()
}

/// `[[a, b, ...]]` (EI's `[phase][second]`) -> phase 0's numbers.
fn phase0(v: &Value) -> Vec<i64> {
    v[0].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------------
// (a) powerDamageTaken1S / targetPowerDamage1S
// ---------------------------------------------------------------------

/// `powerDamageTaken1S` is calibrated as a SPLIT, not as a standalone
/// series: for every bucket of every joined account,
///
/// ```text
/// ours.powerDamageTaken1S[i] - reference.powerDamageTaken1S[i]
///   ==  ours.damageTaken1S[i] - reference.damageTaken1S[i]
/// ```
///
/// That is a strictly stronger statement than "the two series are close",
/// and it is the RIGHT statement here. `damageTaken1S` -- the `All` sibling
/// this task did not touch -- carries a small pre-existing residual against
/// the reference on this capture (measured: 2,442 of 15,400 buckets across
/// 23 of 44 accounts, worst absolute delta **10** on a 180,053 total, i.e.
/// 0.006%; the same skill-`23279` incoming-damage residual
/// `skill_damage_golden.rs`'s `TAKEN_SUM_ABS_TOLERANCE` and
/// `timeseries_golden.rs`'s `TAKEN_ABS_TOLERANCE` already document and
/// bound, re-confirmed here on the whole series rather than just its final
/// element). Asserting the POWER series to a flat tolerance would silently
/// absorb a real classification error of the same magnitude. Asserting
/// that its residual is IDENTICAL to its `All` sibling's, bucket for
/// bucket, pins the thing this task actually added -- the
/// `!ConditionDamageBased` split -- to EXACT, and leaves the inherited
/// residual visible and separately bounded.
///
/// Measured: **0** buckets where the two residuals differ, and the
/// `damageTaken1S` residual is itself bounded below by
/// [`TAKEN_RESIDUAL_BOUND`].
#[test]
fn ei_json_power_damage_taken_1s_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json powerDamageTaken1S") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);

    let mut joined = 0usize;
    let mut buckets = 0usize;
    let mut nonzero = 0usize;
    let mut split_mismatches = 0usize;
    let mut inherited_buckets = 0usize;
    let mut worst_inherited = 0i64;
    let mut exact_buckets = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        let op = phase0(&o["powerDamageTaken1S"]);
        let gp = phase0(&g["powerDamageTaken1S"]);
        let oa = phase0(&o["damageTaken1S"]);
        let ga = phase0(&g["damageTaken1S"]);
        assert_eq!(
            op.len(),
            gp.len(),
            "{account}.powerDamageTaken1S length must match the reference grid exactly \
             (GW2EI's `InterpolatedGraph` allocation, see `analysis::timeseries::ei_grid`)"
        );
        assert_eq!(oa.len(), ga.len(), "{account}.damageTaken1S length");
        // Contract, not tolerance: cumulative, and power <= all.
        assert!(
            op.windows(2).all(|w| w[1] >= w[0]),
            "{account}.powerDamageTaken1S must be cumulative (monotone non-decreasing)"
        );
        assert!(
            op.iter().zip(oa.iter()).all(|(p, a)| p <= a),
            "{account}.powerDamageTaken1S must be element-wise <= damageTaken1S"
        );

        for i in 0..op.len() {
            buckets += 1;
            if op[i] != 0 || gp[i] != 0 {
                nonzero += 1;
            }
            let (r_power, r_all) = (op[i] - gp[i], oa[i] - ga[i]);
            if r_power == 0 {
                exact_buckets += 1;
            } else {
                inherited_buckets += 1;
                worst_inherited = worst_inherited.max(r_power.abs());
            }
            if r_power != r_all {
                split_mismatches += 1;
                if failures.len() < 25 {
                    failures.push(format!(
                        "{account}.powerDamageTaken1S[{i}]: power residual {r_power} != \
                         damageTaken1S residual {r_all} (ours={} reference={})",
                        op[i], gp[i]
                    ));
                }
            }
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(nonzero >= 1000, "degenerate comparison: only {nonzero} nonzero buckets");
    assert!(
        failures.is_empty(),
        "{split_mismatches} bucket(s) where the POWER split introduces a residual its `All` \
         sibling does not have:\n{}",
        failures.join("\n")
    );
    assert!(
        worst_inherited <= TAKEN_RESIDUAL_BOUND,
        "the inherited damageTaken1S residual grew to {worst_inherited}, past the pinned \
         {TAKEN_RESIDUAL_BOUND}"
    );
    println!(
        "ei_json_power_damage_taken_1s: {buckets} buckets across {joined} accounts; \
         {exact_buckets} exact, {inherited_buckets} carrying the pre-existing damageTaken1S \
         residual (worst {worst_inherited}), 0 where the POWER split itself diverges"
    );
}

/// Bound on the pre-existing `damageTaken1S` residual this capture carries
/// against the reference, in raw damage units, per bucket. Measured worst:
/// **10** (on a 180,053 running total). Pinned at 40 so the split test
/// above cannot quietly become a rubber stamp if that residual grows.
const TAKEN_RESIDUAL_BOUND: i64 = 40;

/// `targetPowerDamage1S`: the per-(player, joined enemy) power series.
///
/// Only the JOINED targets are compared, and each is compared for its whole
/// length. Structural invariant asserted on EVERY target (joined or not):
/// `targetPowerDamage1S[t] <= targetDamage1S[t]` element-wise, since power
/// is a filter of all.
#[test]
fn ei_json_target_power_damage_1s_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json targetPowerDamage1S") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);
    assert!(c.joinable.len() >= 40, "expected >= 40 joinable targets, got {}", c.joinable.len());

    let mut joined = 0usize;
    let mut buckets = 0usize;
    let mut nonzero = 0usize;
    let mut mismatched = 0usize;
    let mut worst_abs = 0i64;
    let mut failures: Vec<String> = Vec::new();

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        let (o_all, o_pow) = (&o["targetDamage1S"], &o["targetPowerDamage1S"]);
        // Structural: power <= all, on every emitted target.
        for i in 0..o_all.as_array().map(|a| a.len()).unwrap_or(0) {
            let (a, p) = (phase0(&o_all[i]), phase0(&o_pow[i]));
            assert_eq!(a.len(), p.len(), "{account}.targetPowerDamage1S[{i}] length");
            assert!(
                a.iter().zip(p.iter()).all(|(x, y)| y <= x),
                "{account}.targetPowerDamage1S[{i}] must be element-wise <= targetDamage1S"
            );
        }
        for &(o_i, g_i) in &c.joinable {
            let (ov, gv) = (phase0(&o_pow[o_i]), phase0(&g["targetPowerDamage1S"][g_i]));
            if ov.len() != gv.len() {
                failures.push(format!(
                    "{account}.targetPowerDamage1S[our {o_i}] length: ours={} reference={}",
                    ov.len(),
                    gv.len()
                ));
                continue;
            }
            for (i, (&a, &b)) in ov.iter().zip(gv.iter()).enumerate() {
                buckets += 1;
                if a != 0 || b != 0 {
                    nonzero += 1;
                }
                if a != b {
                    mismatched += 1;
                    worst_abs = worst_abs.max((a - b).abs());
                    if failures.len() < 25 {
                        failures.push(format!(
                            "{account}.targetPowerDamage1S[our {o_i} / ref {g_i}][{i}]: \
                             ours={a} reference={b}"
                        ));
                    }
                }
            }
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(nonzero >= 1000, "degenerate comparison: only {nonzero} nonzero buckets");
    assert!(
        failures.is_empty(),
        "{mismatched} targetPowerDamage1S bucket mismatch(es) (worst |delta| {worst_abs}):\n{}",
        failures.join("\n")
    );
    println!(
        "ei_json_target_power_damage_1s: {buckets} buckets EXACT ({nonzero} nonzero) across \
         {joined} accounts x {} joined targets",
        c.joinable.len()
    );
}

// ---------------------------------------------------------------------
// (b) targets[].damage1S / .powerDamage1S
// ---------------------------------------------------------------------

/// Per-enemy OUTGOING series, both the `All` and the `Power` variant, whole
/// series, on every joined target.
#[test]
fn ei_json_target_series_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json targets[].damage1S") else { return };
    assert!(c.joinable.len() >= 40, "expected >= 40 joinable targets, got {}", c.joinable.len());
    let (o_t, g_t) = (&c.ours["targets"], &c.golden["targets"]);

    let mut buckets = 0usize;
    let mut nonzero = 0usize;
    let mut mismatched = 0usize;
    let mut worst_abs = 0i64;
    let mut failures: Vec<String> = Vec::new();

    for &(o_i, g_i) in &c.joinable {
        for field in ["damage1S", "powerDamage1S"] {
            let (ov, gv) = (phase0(&o_t[o_i][field]), phase0(&g_t[g_i][field]));
            if ov.len() != gv.len() {
                failures.push(format!(
                    "targets[our {o_i}].{field} length: ours={} reference={}",
                    ov.len(),
                    gv.len()
                ));
                continue;
            }
            assert!(
                ov.windows(2).all(|w| w[1] >= w[0]),
                "targets[our {o_i}].{field} must be cumulative"
            );
            for (i, (&a, &b)) in ov.iter().zip(gv.iter()).enumerate() {
                buckets += 1;
                if a != 0 || b != 0 {
                    nonzero += 1;
                }
                if a != b {
                    mismatched += 1;
                    worst_abs = worst_abs.max((a - b).abs());
                    if failures.len() < 25 {
                        failures.push(format!(
                            "targets[our {o_i} / ref {g_i}].{field}[{i}]: ours={a} reference={b}"
                        ));
                    }
                }
            }
        }
    }

    assert!(nonzero >= 1000, "degenerate comparison: only {nonzero} nonzero buckets");
    assert!(
        failures.is_empty(),
        "{mismatched} targets[] series mismatch(es) (worst |delta| {worst_abs}):\n{}",
        failures.join("\n")
    );
    println!(
        "ei_json_target_series: {buckets} buckets EXACT ({nonzero} nonzero) across {} joined \
         targets x 2 fields",
        c.joinable.len()
    );
}

// ---------------------------------------------------------------------
// (c) targets[].totalDamageDist
// ---------------------------------------------------------------------

/// Per-enemy outgoing skill distribution. Compared over the UNION of both
/// sides' skill-id sets (so a row we wrongly dropped fails, not just a row
/// we wrongly added), on the four fields both sides define identically.
#[test]
fn ei_json_target_damage_dist_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json targets[].totalDamageDist") else { return };
    assert!(c.joinable.len() >= 40, "expected >= 40 joinable targets, got {}", c.joinable.len());
    let (o_t, g_t) = (&c.ours["targets"], &c.golden["targets"]);

    let by_id = |v: &Value| -> BTreeMap<i64, Value> {
        v[0].as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["id"].as_i64().map(|id| (id, e.clone())))
            .collect()
    };

    let mut cells = 0usize;
    let mut nonzero = 0usize;
    let mut mismatched = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut ids_compared = 0usize;

    for &(o_i, g_i) in &c.joinable {
        let ours = by_id(&o_t[o_i]["totalDamageDist"]);
        let golden = by_id(&g_t[g_i]["totalDamageDist"]);
        let ids: BTreeSet<i64> = ours.keys().chain(golden.keys()).copied().collect();
        for id in ids {
            ids_compared += 1;
            for (ours_key, golden_key) in [
                ("totalDamage", "totalDamage"),
                ("connectedHits", "connectedHits"),
                ("min", "min"),
                ("max", "max"),
                ("crit", "crit"),
                ("flank", "flank"),
            ] {
                let ov = ours.get(&id).and_then(|e| e[ours_key].as_i64()).unwrap_or(0);
                let gv = golden.get(&id).and_then(|e| e[golden_key].as_i64()).unwrap_or(0);
                cells += 1;
                if ov != 0 || gv != 0 {
                    nonzero += 1;
                }
                if ov != gv {
                    mismatched += 1;
                    if failures.len() < 40 {
                        failures.push(format!(
                            "targets[our {o_i} / ref {g_i}].totalDamageDist[id {id}].{ours_key}: \
                             ours={ov} reference={gv}"
                        ));
                    }
                }
            }
        }
    }

    assert!(nonzero >= 200, "degenerate comparison: only {nonzero} nonzero cells");
    assert!(
        failures.is_empty(),
        "{mismatched} targets[].totalDamageDist mismatch(es) over {ids_compared} (target, skill) \
         rows:\n{}",
        failures.join("\n")
    );
    println!(
        "ei_json_target_damage_dist: {cells} cells EXACT ({nonzero} nonzero) over {ids_compared} \
         (target, skill) rows across {} joined targets",
        c.joinable.len()
    );
}

// ---------------------------------------------------------------------
// (d) targets[].buffs[].statesPerSource
// ---------------------------------------------------------------------

/// Reads a `[[t, v], ...]` step timeline's value at `t`.
fn state_value_at(states: &[Value], t: i64) -> i64 {
    let mut v = 0i64;
    for s in states {
        let time = s[0].as_i64().unwrap_or(0);
        if time > t {
            break;
        }
        v = s[1].as_i64().unwrap_or(0);
    }
    v
}

/// Per-instant bound on a sampled `statesPerSource` stack count. Set from
/// the measurement in the module doc / task report, with margin.
const CONDITION_STATE_STACK_BOUND: i64 = 16;

/// Per-timeline bound on `|mean(ours) - mean(reference)|` over the 1s
/// sample grid, in stacks -- the tight bar (this is the quantity a
/// condition-uptime view integrates).
const CONDITION_MEAN_TOLERANCE_STACKS: f64 = 0.3;

/// Fraction of sampled instants allowed to differ at all.
const CONDITION_SAMPLE_MISMATCH_FRACTION: f64 = 0.01;

/// `statesPerSource` on joined enemy targets, compared by SAMPLING both
/// step functions on a 1s grid -- the same method (and the same reasoning)
/// `meigap_ei_golden.rs`'s player-side `states` test uses: the two
/// simulators fuse differently, so the transition LISTS legitimately differ
/// in length for the same stack history, while the FUNCTION is what both
/// sides mean and what the consumer integrates
/// (`conditionsMetrics.ts`'s `computeUptimeFromStates`).
///
/// Structural properties asserted EXACTLY (contract, not simulation): the
/// mandatory leading `[0, 0]`, non-decreasing times, and -- the join's own
/// gate -- every `(target, condition, source)` key the reference has for a
/// SQUAD source must be present on our side too.
#[test]
fn ei_json_target_condition_states_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json targets[].buffs") else { return };
    assert!(c.joinable.len() >= 40, "expected >= 40 joinable targets, got {}", c.joinable.len());
    let (o_t, g_t) = (&c.ours["targets"], &c.golden["targets"]);
    let duration_ms = c.golden["durationMS"].as_i64().expect("reference durationMS");
    // Squad character names: the only `statesPerSource` keys this project
    // emits (see `target_conditions`'s module doc), so the reference's
    // enemy-sourced and `UNKNOWN` keys are out of scope by construction.
    let squad_names: BTreeSet<&str> = c.ours["players"]
        .as_array()
        .expect("players")
        .iter()
        .filter_map(|p| p["character_name"].as_str())
        .collect();

    fn buffs_by_id(v: &Value) -> BTreeMap<i64, &Value> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| b["id"].as_i64().map(|id| (id, b)))
            .collect()
    }

    let mut timelines = 0usize;
    let mut samples = 0usize;
    let mut mismatched = 0usize;
    let mut worst_instant = 0i64;
    let mut worst_mean = 0.0f64;
    let mut missing_keys = 0usize;
    let mut extra_keys = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for &(o_i, g_i) in &c.joinable {
        let ours = buffs_by_id(&o_t[o_i]["buffs"]);
        let golden = buffs_by_id(&g_t[g_i]["buffs"]);
        // Only the conditions this project catalogs are in scope.
        for &(id, _, _, _) in axilog_core::analysis::condition_catalog::CONDITION_BUFFS.iter() {
            let id = id as i64;
            let o_src = ours.get(&id).map(|b| &b["statesPerSource"]);
            let g_src = golden.get(&id).map(|b| &b["statesPerSource"]);
            let o_map: BTreeMap<&str, &Value> = o_src
                .and_then(|v| v.as_object())
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.as_str(), v))
                .collect();
            let g_map: BTreeMap<&str, &Value> = g_src
                .and_then(|v| v.as_object())
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.as_str(), v))
                .filter(|(k, _)| squad_names.contains(k))
                .collect();

            for (&name, gv) in &g_map {
                let Some(ov) = o_map.get(name) else {
                    missing_keys += 1;
                    if failures.len() < 25 {
                        failures.push(format!(
                            "targets[our {o_i} / ref {g_i}].buffs[{id}].statesPerSource[{name}]: \
                             MISSING on our side"
                        ));
                    }
                    continue;
                };
                let (o_states, g_states) = (
                    ov.as_array().map(|a| a.as_slice()).unwrap_or(&[]),
                    gv.as_array().map(|a| a.as_slice()).unwrap_or(&[]),
                );
                // Structural contract.
                assert_eq!(
                    o_states.first().map(|s| (s[0].as_i64(), s[1].as_i64())),
                    Some((Some(0), Some(0))),
                    "targets[{o_i}].buffs[{id}].statesPerSource[{name}] must start with [0, 0]"
                );
                assert!(
                    o_states.windows(2).all(|w| w[0][0].as_i64() <= w[1][0].as_i64()),
                    "targets[{o_i}].buffs[{id}].statesPerSource[{name}] times must be \
                     non-decreasing"
                );

                timelines += 1;
                let (mut sum_o, mut sum_g, mut n) = (0i64, 0i64, 0i64);
                let mut t = 0i64;
                while t <= duration_ms {
                    let (a, b) = (state_value_at(o_states, t), state_value_at(g_states, t));
                    samples += 1;
                    n += 1;
                    sum_o += a;
                    sum_g += b;
                    if a != b {
                        mismatched += 1;
                        worst_instant = worst_instant.max((a - b).abs());
                    }
                    t += 1000;
                }
                let mean_delta =
                    ((sum_o as f64 / n as f64) - (sum_g as f64 / n as f64)).abs();
                worst_mean = worst_mean.max(mean_delta);
                if mean_delta > CONDITION_MEAN_TOLERANCE_STACKS && failures.len() < 25 {
                    failures.push(format!(
                        "targets[our {o_i} / ref {g_i}].buffs[{id}].statesPerSource[{name}]: \
                         mean delta {mean_delta:.3} stacks"
                    ));
                }
            }
            for &name in o_map.keys() {
                if !g_map.contains_key(name) {
                    extra_keys += 1;
                }
            }
        }
    }

    println!(
        "ei_json_target_condition_states: {samples} instants across {timelines} timelines / {} \
         joined targets; differing instants {mismatched} ({:.2}%), worst instant {worst_instant}, \
         worst mean {worst_mean:.3}, missing keys {missing_keys}, extra keys {extra_keys}",
        c.joinable.len(),
        100.0 * mismatched as f64 / samples.max(1) as f64
    );

    assert!(timelines >= 100, "degenerate comparison: only {timelines} timelines");
    assert!(
        missing_keys == 0,
        "{missing_keys} reference (target, condition, squad source) key(s) absent from our \
         output:\n{}",
        failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
    assert!(
        worst_instant <= CONDITION_STATE_STACK_BOUND,
        "worst instantaneous stack delta {worst_instant} exceeds the pinned bound \
         {CONDITION_STATE_STACK_BOUND}"
    );
    assert!(
        (mismatched as f64) <= CONDITION_SAMPLE_MISMATCH_FRACTION * samples as f64,
        "{mismatched} of {samples} instants differ, over the pinned \
         {CONDITION_SAMPLE_MISMATCH_FRACTION} fraction"
    );
    assert!(
        failures.is_empty(),
        "{} statesPerSource timeline(s) over the mean tolerance:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
