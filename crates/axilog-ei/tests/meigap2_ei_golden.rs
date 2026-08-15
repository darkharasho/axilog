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
//!    per-player series (`JsonPlayerBuilder.cs:76-77,99-100`, filter at
//!    `Actor.cs:449-451`). See `axilog_core::analysis::timeseries`'s
//!    POWER-split section.
//! b. `targets[].damage1S` + `.powerDamage1S` -- per-ENEMY OUTGOING series
//!    (the shared `JsonActorBuilder.FillJsonActor:108-109` over an NPC).
//! c. `targets[].totalDamageDist[0]` -- per-enemy OUTGOING per-skill
//!    distribution, actor-only (`JsonActorBuilder.cs:109-122` ->
//!    `GetJustActorDamageEvents`). See
//!    `axilog_core::analysis::skill_damage::build_enemy_dist`.
//! d. `targets[].buffs[].id` + `.statesPerSource` -- incoming-condition
//!    attribution on enemy players (`JsonNPCBuilder.cs:89-118`). See
//!    `axilog_core::analysis::target_conditions`.
//!
//! ## The targets join
//!
//! Same M16 pattern `meigap_ei_golden.rs`'s `statsTargets` test
//! established. Post-MROSTER the two rosters agree in KIND (both are the
//! enemy players; see `axilog_schema::Report::ei_targets`) and post-MINSTID
//! they agree in GRANULARITY too (both regroup enemy-player agents sharing
//! an `InstID`, `AgentManipulationHelper.cs:467-474` /
//! `axilog_core::wvw::dedupe_enemy_players`): 56 rows against 56, over the
//! same 56 instids. The join still cannot be positional, because the NAME
//! spaces do not intersect (GW2EI anonymises to `"<Spec> pl-<instid>"`,
//! this project shows the WvW rank title) and neither side promises an
//! order. It therefore goes through arcdps AGENT IDENTITY -- the instid
//! GW2EI encodes into that placeholder name -> the addr that instid
//! belonged to -> this project's `targets[]` index. Only unambiguously
//! joinable targets are calibratable; MINSTID took that from 43 of 56 to
//! all 56, since an instid no longer resolves to two of our rows.

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
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
    // Task 9: the outcome columns on both player-side distributions.
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let enemies: BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let enemy_addr_to_rep: BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    // Task 7: hoisted above the `ReportV1` build, which now carries the
    // enemy per-skill rows on `blocks.damage`.
    let enemy_dist =
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &enemy_addr_to_rep);
    let enemy_series = axilog_core::analysis::timeseries::build_enemy_series(
        &enc,
        &raw,
        &registry,
        &enemies,
        &enemy_addr_to_rep,
    );
    let target_conditions =
        axilog_core::analysis::target_conditions::build_with_registry(&raw, &registry, &enc);

    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes { activity: Some(&activity),
            target_conditions: Some(&target_conditions),
            enemy_dist: Some(&enemy_dist),
            enemy_series: Some(&enemy_series),
            dist_outcomes: Some(&dist_outcomes),
            ..Default::default()
        },
    );

    let ours = axilog_ei::to_ei_json(
        &report_v1, None,
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
    // MROSTER: `targets[]` is the CURATED roster (`Report::ei_targets` --
    // enemy PLAYERS only, per `WvWLogic.cs`), so an index into
    // `enc.enemies` is no longer an index into `targets[]`. Route the join
    // through the enemy's representative addr, which the adapter emits
    // verbatim as `targets[].id`: that keeps this join correct for whatever
    // the curation rule is, instead of re-encoding the rule here. An enemy
    // absent from the curated roster simply yields no index, so the golden
    // comparison skips it rather than joining to the wrong row.
    let target_index_by_id: BTreeMap<u64, usize> = ours["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t["id"].as_u64().map(|id| (id, i)))
        .collect();
    let addr_to_enemy_index: BTreeMap<u64, usize> = enc
        .enemies
        .iter()
        .filter_map(|e| target_index_by_id.get(&e.id).map(|&i| (e, i)))
        .flat_map(|(e, i)| e.agent_addrs.iter().map(move |&a| (a, i)))
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
///
/// ## MINSTID: the join grew from 43 targets to all 56, and what that
/// uncovered
///
/// `render_and_reference`'s join only accepts an instid that resolves to
/// exactly ONE of our `targets[]` rows. Before MINSTID this project emitted
/// 71 rows over 56 instids, so the 13 instids carrying two agent rows each
/// were AMBIGUOUS and silently skipped -- this test only ever compared the
/// 43 single-row targets. MINSTID collapses those rows the way GW2EI does
/// (`wvw::dedupe_enemy_players`), so all 56 targets now join and are
/// compared here for the first time.
///
/// 10 of the 13 newly-joined targets are byte-exact against the reference
/// on both series, which is the direct evidence that the merge reproduces
/// EI's regroup: the merged row equals EI's merged person, not one of its
/// halves.
///
/// [`RESIDUAL_INSTIDS`] are the other 3. Their divergence is NOT introduced
/// by the merge -- each merged series was verified to be the exact sum of
/// its pre-merge parts, and those parts are untouched by MINSTID; the merge
/// only made an already-divergent value visible to this join for the first
/// time. The diagnosed cause is damage CREDIT, not identity: on instid 3954
/// (the worst, |delta| 364694) the reference's whole-log total, 37653, is
/// exactly this project's two parts' DIRECT damage (2927 + 34726), i.e. the
/// reference does not credit that person with the ~364k of non-direct
/// (minion/credited) damage this project folds onto them, while this
/// project's row is otherwise identical. That credit rule is a separate
/// gap, out of MINSTID's scope, and is tracked by this allowlist: the test
/// asserts the residual set is EXACTLY these three, so any regression that
/// widens it -- or any fix that narrows it, which should shrink this list
/// -- fails here.
#[test]
fn ei_json_target_series_match_the_reference_export_when_available() {
    /// Joined targets whose series are knowingly not exact -- see this
    /// test's doc comment. Keyed by `instanceID` (stable across roster
    /// re-indexing) rather than by array position.
    const RESIDUAL_INSTIDS: &[i64] = &[3483, 3954, 4952];

    let Some(c) = render_and_reference("ei-json targets[].damage1S") else { return };
    assert!(c.joinable.len() >= 55, "expected >= 55 joinable targets, got {}", c.joinable.len());
    let (o_t, g_t) = (&c.ours["targets"], &c.golden["targets"]);

    let mut buckets = 0usize;
    let mut nonzero = 0usize;
    let mut mismatched = 0usize;
    let mut worst_abs = 0i64;
    let mut failures: Vec<String> = Vec::new();
    let mut residual_seen: BTreeSet<i64> = BTreeSet::new();

    for &(o_i, g_i) in &c.joinable {
        let instid = o_t[o_i]["instanceID"].as_i64().expect("instanceID");
        let residual = RESIDUAL_INSTIDS.contains(&instid);
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
                    if residual {
                        residual_seen.insert(instid);
                        continue;
                    }
                    mismatched += 1;
                    worst_abs = worst_abs.max((a - b).abs());
                    if failures.len() < 25 {
                        failures.push(format!(
                            "targets[our {o_i} / ref {g_i}, instid {instid}].{field}[{i}]: \
                             ours={a} reference={b}"
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
    // The allowlist is a measurement, not a licence: an entry that stops
    // diverging must be removed from it, or it silently protects nothing.
    let expected: BTreeSet<i64> = RESIDUAL_INSTIDS.iter().copied().collect();
    assert_eq!(
        residual_seen, expected,
        "RESIDUAL_INSTIDS is stale: these instids no longer diverge and must be removed"
    );
    println!(
        "ei_json_target_series: {buckets} buckets EXACT ({nonzero} nonzero) across {} joined \
         targets x 2 fields, minus {} allowlisted residual targets",
        c.joinable.len(),
        RESIDUAL_INSTIDS.len()
    );
}

// ---------------------------------------------------------------------
// (c) targets[].totalDamageDist
// ---------------------------------------------------------------------

/// Per-enemy outgoing skill distribution.
///
/// Two separate assertions, because value equality alone is blind to a
/// class of error the first version of this test shipped with:
///
/// 1. **Row EXISTENCE.** Our emitted skill-id set per joined target must
///    equal the reference's, exactly. Comparing only VALUES over the union
///    with `unwrap_or(0)` on both sides lets a row we invent that the
///    reference does not have pass as `0 == 0` -- which is exactly what
///    happened: before review fix 1, `build_enemy_dist` created an entry
///    for any non-statechange row, including pre-rework buff APPLICATION
///    rows, and 143 of the 488 rows the committed fixture emitted were
///    phantoms GW2EI never emits (19 skill ids' worth in the enemy-player
///    aggregate). Those phantoms are not cosmetic: axibridge's
///    `precomputeGlobalEnemySkillStats` does `minTotal += min;
///    minCount += 1` per entry, so each one drags the `minMitigation`
///    average toward zero. **This capture is post-rework and had none**
///    (546 rows before and after) -- the pre-era coverage lives in
///    `ei_golden.rs`'s `ei_json_enemy_player_skill_dist_matches_the_golden_aggregate`,
///    which is also the CI-runnable half.
/// 2. **Values**, over the union of both id sets, on the six fields both
///    sides define identically.
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
    let mut phantom_rows = 0usize;
    let mut missing_rows = 0usize;
    let mut zero_rows_both = 0usize;
    let mut row_failures: Vec<String> = Vec::new();

    for &(o_i, g_i) in &c.joinable {
        let ours = by_id(&o_t[o_i]["totalDamageDist"]);
        let golden = by_id(&g_t[g_i]["totalDamageDist"]);

        for id in ours.keys() {
            if !golden.contains_key(id) {
                phantom_rows += 1;
                if row_failures.len() < 25 {
                    row_failures.push(format!(
                        "targets[our {o_i} / ref {g_i}].totalDamageDist: PHANTOM row id {id} \
                         (we emit it, GW2EI does not)"
                    ));
                }
            }
        }
        for id in golden.keys() {
            if !ours.contains_key(id) {
                missing_rows += 1;
                if row_failures.len() < 25 {
                    row_failures.push(format!(
                        "targets[our {o_i} / ref {g_i}].totalDamageDist: MISSING row id {id} \
                         (GW2EI emits it, we do not)"
                    ));
                }
            }
        }

        let ids: BTreeSet<i64> = ours.keys().chain(golden.keys()).copied().collect();
        for id in ids {
            ids_compared += 1;
            // A row both sides emit with every field zero is a real GW2EI
            // shape (a fully blocked/evaded strike: `hits > 0`,
            // `connectedHits == 0`), counted so the row-existence check
            // above is visibly not vacuous.
            let all_zero = |m: &BTreeMap<i64, Value>| {
                m.get(&id).is_some_and(|e| {
                    ["totalDamage", "connectedHits", "min", "max", "crit", "flank"]
                        .iter()
                        .all(|k| e[*k].as_i64().unwrap_or(0) == 0)
                })
            };
            if all_zero(&ours) && all_zero(&golden) {
                zero_rows_both += 1;
            }
            for field in ["totalDamage", "connectedHits", "min", "max", "crit", "flank"] {
                let ov = ours.get(&id).and_then(|e| e[field].as_i64()).unwrap_or(0);
                let gv = golden.get(&id).and_then(|e| e[field].as_i64()).unwrap_or(0);
                cells += 1;
                if ov != 0 || gv != 0 {
                    nonzero += 1;
                }
                if ov != gv {
                    mismatched += 1;
                    if failures.len() < 40 {
                        failures.push(format!(
                            "targets[our {o_i} / ref {g_i}].totalDamageDist[id {id}].{field}: \
                             ours={ov} reference={gv}"
                        ));
                    }
                }
            }
        }
    }

    assert!(nonzero >= 200, "degenerate comparison: only {nonzero} nonzero cells");
    assert!(
        row_failures.is_empty(),
        "{phantom_rows} phantom + {missing_rows} missing totalDamageDist row(s) across \
         {} joined targets:\n{}",
        c.joinable.len(),
        row_failures.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{mismatched} targets[].totalDamageDist value mismatch(es) over {ids_compared} \
         (target, skill) rows:\n{}",
        failures.join("\n")
    );
    println!(
        "ei_json_target_damage_dist: {cells} cells EXACT ({nonzero} nonzero) over {ids_compared} \
         (target, skill) rows across {} joined targets; row sets identical (0 phantom, 0 missing), \
         {zero_rows_both} legitimately all-zero rows on both sides",
        c.joinable.len()
    );
}

/// The CONSUMER-level aggregate axibridge actually derives from
/// `targets[].totalDamageDist` -- `precomputeGlobalEnemySkillStats`
/// (`packages/bridge-metrics/src/computePlayerAggregation.ts:490-509`) folds
/// EVERY target's rows into one global `skillId -> {totalDamage,
/// connectedHits, minTotal, minCount}` table, and `resolveGlobalEnemyStats`
/// (`:277-286`) reads `avg = totalDamage / connectedHits` and
/// `min = minTotal / minCount` out of it. Those two numbers are the
/// multipliers in the damage-mitigation columns
/// (`recomputeMitigationTotals`, `:1517-1560`).
///
/// The per-target test above cannot speak to this, for a structural reason:
/// **the fold runs over the whole roster, so the roster is an input to the
/// answer.** `minCount` counts ROWS, and `totalDamage`/`connectedHits` sum
/// over rows, so a roster that is not EI's moves the aggregate even where
/// every individual row is byte-exact (which, per the test above, they all
/// are).
///
/// ## MROSTER: this is now the strong half, unconditionally
///
/// Before MROSTER this project's `targets[]` was the full unfiltered enemy
/// roster (624 agents on this capture) against GW2EI's curated 57, and this
/// test measured a two-fold pair to isolate the cause:
///
/// - **Full roster** (what axibridge actually saw): 250 skill ids vs the
///   reference's 206, and of the 206 shared ids 6 differed on
///   `totalDamage`/`connectedHits`/`avg` and 21 on the min-mean.
/// - **Restricted to `enemyPlayer` targets**: 206 ids, all shared, with
///   `totalDamage`/`connectedHits`/`avg` exact on all 206.
///
/// That pair said the entire `avg` residual -- the multiplier the
/// mitigation totals are built from -- was roster shape and nothing else,
/// and named the fix. MROSTER applied it: `targets[]` is now
/// `Report::ei_targets`, GW2EI's own WvW rule (enemy PLAYERS only,
/// `WvWLogic.cs:325-375`). The two folds are therefore now the SAME fold,
/// and this test asserts that -- the restricted exactness became
/// unrestricted exactness. axibridge's own fold
/// (`precomputeGlobalEnemySkillStats`, which applies no `enemyPlayer`
/// filter of its own and simply trusts the roster) gets the exact numbers
/// without having to know to restrict.
///
/// ## The one residual, and how MINSTID closed it
///
/// Through MROSTER the min-mean still differed on 16 of 206 ids. That was
/// NOT roster kind -- it was roster GRANULARITY, and the cause was pinned
/// rather than guessed: GW2EI regroups non-squad player agents that share
/// an `InstID` into ONE agent before building targets
/// (`AgentManipulationHelper.cs:467-474`), so its 56 enemy-player targets
/// are 56 *people*. This project did not (`wvw::apply`'s
/// `dedupe_enemy_players` keyed on ACCOUNT, which WvW anonymisation leaves
/// empty for enemies), so it emitted 71 rows over the same 56 instids -- 13
/// instids carrying 2 agent rows each. `minCount` counts ROWS, so each
/// split person contributed two minima where EI contributes one.
///
/// MINSTID rekeyed that dedupe on instid (GW2EI's rule verbatim, see
/// `axilog_core::wvw::dedupe_enemy_players`). The roster is now 56 rows
/// over EI's exact 56 instids, and the residual measured here went **16 ->
/// 0**: every one of the 206 shared ids now agrees on the min-mean too, so
/// the whole aggregate -- `totalDamage`, `connectedHits`, `avg` AND the
/// min-mean -- is exact, on both folds. (The prediction from the merged-row
/// simulation was 1; merging for real also fixed the last one, because the
/// simulation could only merge the EXPORTED rows, not the underlying
/// per-agent minima.) The bound below is kept as a ratchet rather than
/// deleted.
#[test]
fn ei_json_enemy_skill_aggregate_residual_is_measured_and_bounded() {
    let Some(c) = render_and_reference("ei-json enemy-skill consumer aggregate") else { return };

    #[derive(Default, Clone, Copy)]
    struct Bucket {
        total_damage: i64,
        connected_hits: i64,
        min_total: i64,
        min_count: i64,
    }
    impl Bucket {
        fn avg(&self) -> f64 {
            if self.connected_hits > 0 {
                self.total_damage as f64 / self.connected_hits as f64
            } else {
                0.0
            }
        }
        fn min_mean(&self) -> f64 {
            if self.min_count > 0 { self.min_total as f64 / self.min_count as f64 } else { 0.0 }
        }
    }
    // Exactly `precomputeGlobalEnemySkillStats`, including its
    // `if (!entry?.id) return;` skip of id 0.
    fn fold(targets: &Value, players_only: bool) -> (BTreeMap<i64, Bucket>, usize) {
        let mut out: BTreeMap<i64, Bucket> = BTreeMap::new();
        let mut n = 0usize;
        for t in targets.as_array().expect("targets") {
            if players_only && !t["enemyPlayer"].as_bool().unwrap_or(false) {
                continue;
            }
            n += 1;
            for e in t["totalDamageDist"][0].as_array().into_iter().flatten() {
                let Some(id) = e["id"].as_i64() else { continue };
                if id == 0 {
                    continue;
                }
                let b = out.entry(id).or_default();
                b.total_damage += e["totalDamage"].as_i64().unwrap_or(0);
                b.connected_hits += e["connectedHits"].as_i64().unwrap_or(0);
                b.min_total += e["min"].as_i64().unwrap_or(0);
                b.min_count += 1;
            }
        }
        (out, n)
    }

    let (golden, g_targets) = fold(&c.golden["targets"], false);
    assert!(golden.len() >= 150, "degenerate reference aggregate: {} ids", golden.len());

    let mut report: Vec<String> = Vec::new();
    let mut measure = |label: &str, ours: &BTreeMap<i64, Bucket>, n: usize| {
        let shared: Vec<i64> = ours.keys().filter(|k| golden.contains_key(k)).copied().collect();
        let mut td = 0usize;
        let mut ch = 0usize;
        let mut avg = 0usize;
        let mut mm = 0usize;
        let mut worst_avg = 0.0f64;
        let mut worst_mm = 0.0f64;
        for id in &shared {
            let (o, g) = (ours[id], golden[id]);
            if o.total_damage != g.total_damage {
                td += 1;
            }
            if o.connected_hits != g.connected_hits {
                ch += 1;
            }
            if (o.avg() - g.avg()).abs() > 1e-9 {
                avg += 1;
                worst_avg = worst_avg.max((o.avg() - g.avg()).abs() / g.avg().max(1.0));
            }
            if (o.min_mean() - g.min_mean()).abs() > 1e-9 {
                mm += 1;
                worst_mm = worst_mm.max((o.min_mean() - g.min_mean()).abs() / g.min_mean().max(1.0));
            }
        }
        report.push(format!(
            "{label}: {n} targets, {} ids ({} shared, {} ours-only, {} reference-only); of the \
             shared ids totalDamage differs {td}, connectedHits {ch}, avg {avg} (worst {:.0}% \
             rel), min-mean {mm} (worst {:.0}% rel)",
            ours.len(),
            shared.len(),
            ours.len() - shared.len(),
            golden.len() - shared.len(),
            100.0 * worst_avg,
            100.0 * worst_mm,
        ));
        (shared.len(), ours.len() - shared.len(), golden.len() - shared.len(), td, ch, avg, mm)
    };

    let (all, all_n) = fold(&c.ours["targets"], false);
    let (a_shared, a_ours_only, a_ref_only, _a_td, _a_ch, a_avg, a_mm) =
        measure("full roster", &all, all_n);
    let (players, players_n) = fold(&c.ours["targets"], true);
    let (p_shared, p_ours_only, p_ref_only, p_td, p_ch, p_avg, p_mm) =
        measure("enemyPlayer only", &players, players_n);

    println!("ei_json_enemy_skill_aggregate (reference: {g_targets} curated targets)");
    for line in &report {
        println!("   {line}");
    }

    assert!(a_shared >= 150, "degenerate: only {a_shared} shared skill ids");
    assert_eq!(
        a_ref_only, 0,
        "the reference aggregate has skill ids ours does not -- every enemy PLAYER EI curates is \
         in our roster too, so this direction must be empty"
    );

    // --- MROSTER: the two folds are now the SAME fold. This is the
    // --- milestone's proof, and it is asserted structurally (the roster IS
    // --- the enemy-player set) rather than by comparing two numbers that
    // --- happen to agree.
    assert_eq!(
        all_n, players_n,
        "every emitted target must be an enemyPlayer -- the curated roster IS the restricted fold"
    );
    assert_eq!(
        a_shared, p_shared,
        "the unrestricted and enemyPlayer-restricted folds must be identical post-curation"
    );

    // --- The strong half, now UNRESTRICTED: the mitigation multiplier
    // --- `avg` and both of its inputs are EXACT over the roster axibridge
    // --- actually folds, with no `enemyPlayer` filter applied by anyone.
    assert_eq!(a_ours_only, 0, "the full fold must carry no skill id the reference lacks");
    assert_eq!(p_ours_only, 0, "enemyPlayer fold must carry no skill id the reference lacks");
    assert_eq!(p_ref_only, 0, "enemyPlayer fold must carry every reference skill id");
    assert_eq!(p_td, 0, "enemyPlayer fold: totalDamage must be exact on every shared id");
    assert_eq!(p_ch, 0, "enemyPlayer fold: connectedHits must be exact on every shared id");
    assert_eq!(
        a_avg, 0,
        "FULL-roster fold: the mitigation multiplier `avg` must be exact on every shared id -- \
         this is MROSTER's proof; a nonzero value means the roster regressed"
    );
    assert_eq!(
        p_avg, 0,
        "enemyPlayer fold: the mitigation multiplier `avg` must be exact on every shared id"
    );
    assert!(p_shared >= 150);

    // --- Formerly the one measured residual (the `InstID`-regroup gap),
    // --- driven to ZERO by MINSTID. Kept as a ratchet at the measured
    // --- value so a regrouping regression cannot reintroduce it quietly.
    assert_eq!(
        a_mm, AGGREGATE_MIN_MEAN_DIFF_BOUND,
        "{a_mm} shared ids differ on the min-mean, past the pinned \
         {AGGREGATE_MIN_MEAN_DIFF_BOUND}"
    );
    assert_eq!(a_mm, p_mm, "the two folds are the same fold, so their min-mean residual must match");
}

/// Shared ids whose `min = minTotal / minCount` differs from the
/// reference's. `minCount` counts ROWS, so this was the one part of the
/// aggregate sensitive to the `InstID`-regroup gap
/// (`AgentManipulationHelper.cs:467-474`): 13 of the old 71 enemy-player
/// rows were second agents for an instid EI folds into one target, so 13
/// people contributed two minima each. 21 on the pre-MROSTER 624-agent
/// roster, 16 after MROSTER, and **0 after MINSTID** rekeyed the enemy
/// dedupe on instid. Pinned at 0: the gap is closed, so any reappearance is
/// a regression, not a known residual.
const AGGREGATE_MIN_MEAN_DIFF_BOUND: usize = 0;

/// MEIGAP Task 2 review fix 3: `players[].damage1S` -- the family the grid
/// fix swept through -- gets its own bounded whole-series calibration, so it
/// is no longer an unmeasured neighbour of the series this task added.
///
/// It was NOT clean before this round. Whole-series comparison found 3,651
/// of 15,400 buckets and 28 of 44 final elements diverging, worst 2,000
/// absolute (3.1% relative). It was never a grid artefact and never an M12
/// series bug: `damage1S[-1] == dpsAll[0].damage` holds exactly on BOTH
/// sides, so the series was faithfully reporting a wrong TOTAL.
///
/// Root cause, found and fixed in this round: `DamageResult.BreakbarDamage`
/// (result byte 10) rows were being summed as health damage. GW2EI routes
/// them to `brkBarDamage` via `AddNonDamageDamageEvent`
/// (`CombatEventFactory.cs:799-809`, reached from both
/// `AddDirectDamageEvent:830` and `AddBuffDamageDamageEvent:862`), so none
/// of their magnitude is health damage -- exactly the same reason
/// `CrowdControl` was already excluded. See
/// `analysis::damage::is_health_damage_result`. The committed fixture
/// carries zero breakbar rows, which is why every committed golden stayed
/// exact and CI never saw it.
///
/// Post-fix this asserts the whole series, bounded rather than exact: one
/// account retains a 272-bucket, worst-2 residual, the same tiny
/// incoming/outgoing rounding class `damageTaken1S` carries.
#[test]
fn ei_json_damage_1s_whole_series_is_bounded_against_the_reference() {
    let Some(c) = render_and_reference("ei-json damage1S whole series") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);

    let mut joined = 0usize;
    let mut buckets = 0usize;
    let mut diverging_accounts = 0usize;
    let mut diverging_buckets = 0usize;
    let mut worst = 0i64;

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        let (ov, gv) = (phase0(&o["damage1S"]), phase0(&g["damage1S"]));
        assert_eq!(ov.len(), gv.len(), "{account}.damage1S grid length");
        assert!(ov.windows(2).all(|w| w[1] >= w[0]), "{account}.damage1S must be cumulative");
        let mut account_diverged = false;
        for (i, (&a, &b)) in ov.iter().zip(gv.iter()).enumerate() {
            buckets += 1;
            if a != b {
                diverging_buckets += 1;
                account_diverged = true;
                if (a - b).abs() > worst {
                    worst = (a - b).abs();
                    assert!(
                        worst <= DAMAGE_1S_ABS_BOUND,
                        "{account}.damage1S[{i}]: ours={a} reference={b} -- delta {worst} past \
                         the pinned {DAMAGE_1S_ABS_BOUND}"
                    );
                }
            }
        }
        if account_diverged {
            diverging_accounts += 1;
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(
        diverging_accounts <= DAMAGE_1S_DIVERGING_ACCOUNTS_BOUND,
        "{diverging_accounts} of {joined} accounts diverge on damage1S, past the pinned \
         {DAMAGE_1S_DIVERGING_ACCOUNTS_BOUND} -- before the breakbar fix this was 28"
    );
    println!(
        "ei_json_damage_1s_whole_series: {buckets} buckets across {joined} accounts; \
         {diverging_buckets} differ on {diverging_accounts} account(s), worst {worst}"
    );
}

/// Per-bucket bound on the `damage1S` residual. Measured worst 2 after the
/// breakbar fix (was 2,000 before it). Pinned at 40, the same class as
/// `TAKEN_RESIDUAL_BOUND`.
const DAMAGE_1S_ABS_BOUND: i64 = 40;
/// Accounts allowed to carry any `damage1S` residual at all. Measured 1
/// after the breakbar fix (was 28 before it). Pinned at 6 -- deliberately
/// tight, because this is the gate that would catch a scope regression of
/// the kind the fix removed.
const DAMAGE_1S_DIVERGING_ACCOUNTS_BOUND: usize = 6;

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
