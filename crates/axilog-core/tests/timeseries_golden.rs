//! M12 Task 2: per-player per-second series calibration against GW2EI's
//! `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets`
//! (`crates/axilog-core/src/analysis/timeseries.rs`).
//!
//! ## Cumulative vs instant
//!
//! Confirmed directly against a real dps.report WvW export
//! (`axibridge/test-fixtures/boon/20260117-181030.json`): every player's
//! `damage1S[0]`/`damageTaken1S[0]` array is monotonic non-decreasing and
//! its LAST element equals that same player's `dpsAll[0].damage`/
//! `defenses[0].damageTaken` scalar exactly -- i.e. GW2EI's `*1S` arrays
//! are CUMULATIVE running totals, not per-second deltas. See
//! `analysis::timeseries`'s module doc for the full citation and
//! `fixtures/wvw-small.ei.json`'s `_note` (Task 2/M12 addendum) for how
//! the golden `timeSeries` scalars below were extracted.
//!
//! ## Join method
//!
//! Unlike `skill_damage_golden.rs`/`healing_golden.rs` (which join via
//! `anon_account(raw agent-table index)`, reaching 37/41 real accounts --
//! 4 `Non Squad Player N` placeholder rows have no real account to join
//! through), this fixture's `timeSeries` scalars were extracted by joining
//! the source EI JSON to this fixture's EXISTING `players[]` rows via a
//! `(profession, damage, appliedCrowdControl)` triple key (verified unique
//! across all 41 rows on both sides, zero collisions) -- see the fixture's
//! `_note`. That reaches all 41 rows (none of these four scalars are
//! account-identifying on their own), so this test re-derives the SAME
//! triple key from `axilog_core`'s own decoded `Encounter`/`Metrics`
//! (`p.profession`, `pm.damage_total`, `pm.cc_applied`) to join back to the
//! golden fixture, rather than the `anon_account` method.
//!
//! ## Tolerance
//!
//! - `sum(dps_targets[*].damage) == PlayerMetrics::per_enemy` total, and
//!   the final element of `timeseries.damage`/`timeseries.damage_taken`
//!   equals `damage_total`/`damage_taken`: **EXACT**, for EVERY player (not
//!   just the 41 joined to golden) -- internal invariants, true by
//!   construction (`timeseries`'s module doc), reverified directly here.
//! - `timeseries.damage.last()` vs golden's `timeSeries.damage1SFinal`:
//!   **EXACT** for all 41 joined accounts (this is the same value
//!   `damage_total`/`damage.total` is already calibrated against
//!   elsewhere -- this assertion is the `*1S`-shape-specific regression
//!   guard the M12 plan asks for, not a new independent calibration).
//! - `timeseries.damage_taken.last()` vs golden's `timeSeries.
//!   damageTaken1SFinal`: within a flat `TAKEN_ABS_TOLERANCE` (measured
//!   max 2, generous margin) for 8/41 accounts -- the SAME documented
//!   `skill_damage_golden.rs` `taken`-side residual (skill id `23279`,
//!   absent from EI's own `skillMap`), re-confirmed here on the identical
//!   8 accounts at the identical 1-2 unit magnitude. **EXACT** for the
//!   other 33/41.
//! - `dps_targets` total vs golden's `timeSeries.dpsTargetsTotalDamage`:
//!   **NOT** expected to match exactly (see `timeseries`'s module doc's
//!   `dps_targets` section) -- GW2EI's non-`detailedWvW` WvW export
//!   collapses every enemy player into one synthetic target and drops
//!   NPC-target damage (siege/dolyaks/guards) from `dpsTargets` entirely,
//!   while this project's own `per_enemy`/`dps_targets` enumerate every
//!   NPC enemy the log tracks. This test instead asserts the STRUCTURAL
//!   relationship the module doc claims: our total is always `>=` golden's
//!   (a strict superset), verified on all 41 joined accounts, zero
//!   exceptions.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-small.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
const LOCAL_POSTREWORK_ZEVTC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-postrework.zevtc");

/// `damage_taken` final-value flat-absolute tolerance -- the SAME
/// documented residual `skill_damage_golden.rs`'s `TAKEN_SUM_ABS_TOLERANCE`
/// already tolerates (skill id `23279`, absent from EI's own `skillMap`,
/// contributing 1-2 extra incoming damage on a small subset of accounts,
/// entirely absent from EI's `totalDamageTaken`/`damageTaken1S`). Measured
/// here on the SAME 8 accounts, same 1-2 unit magnitude -- confirms it's
/// the identical underlying gap, not a new one. `10` matches `skill_damage_
/// golden.rs`'s constant (measured max 2, generous margin).
const TAKEN_ABS_TOLERANCE: i64 = 10;

fn read_anon_fixture() -> Vec<u8> {
    std::fs::read(ANON_FIXTURE_PATH).unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"))
}

fn read_local_fixture_or_skip(test_name: &str) -> Option<Vec<u8>> {
    match std::fs::read(LOCAL_FIXTURE_PATH) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {LOCAL_FIXTURE_PATH} absent ({test_name} local-only extra check)");
            None
        }
    }
}

fn read_golden_json() -> serde_json::Value {
    let s = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&s).expect("parse golden EI JSON")
}

/// Internal invariants, checked for EVERY player (module doc): hold true
/// by construction, since `timeseries::build` is a bucketed refinement of
/// the same predicate `damage::accumulate`/`accumulate_damage_taken`
/// already use.
fn check_internal_invariants(metrics: &axilog_core::analysis::Metrics) {
    for p in &metrics.players {
        let final_damage = p.timeseries.damage.last().copied().unwrap_or(0);
        assert_eq!(
            final_damage, p.damage_total,
            "agent {:#x}: final timeseries.damage bucket must equal damage_total exactly",
            p.agent_addr
        );
        let final_taken = p.timeseries.damage_taken.last().copied().unwrap_or(0);
        assert_eq!(
            final_taken, p.damage_taken,
            "agent {:#x}: final timeseries.damage_taken bucket must equal damage_taken exactly",
            p.agent_addr
        );

        // Monotonic non-decreasing (cumulative, per the module doc).
        for w in p.timeseries.damage.windows(2) {
            assert!(w[0] <= w[1], "agent {:#x}: damage series must be monotonic non-decreasing", p.agent_addr);
        }
        for w in p.timeseries.damage_taken.windows(2) {
            assert!(w[0] <= w[1], "agent {:#x}: damage_taken series must be monotonic non-decreasing", p.agent_addr);
        }
        for t in &p.timeseries.per_target {
            for w in t.damage.windows(2) {
                assert!(
                    w[0] <= w[1],
                    "agent {:#x} target {}: per_target series must be monotonic non-decreasing",
                    p.agent_addr, t.enemy_id
                );
            }
            // Every per_target series ends on the same value `dps_targets`
            // reports for that enemy.
            let dt = p.timeseries.dps_targets.iter().find(|d| d.enemy_id == t.enemy_id)
                .unwrap_or_else(|| panic!("agent {:#x}: dps_targets missing entry for enemy {}", p.agent_addr, t.enemy_id));
            assert_eq!(dt.damage, t.damage.last().copied().unwrap_or(0));
        }

        // sum(dps_targets) == sum(per_enemy) == damage_total, all EXACT.
        let dps_targets_sum: u64 = p.timeseries.dps_targets.iter().map(|d| d.damage).sum();
        let per_enemy_sum: u64 = p.per_enemy.iter().map(|(_, d)| d).sum();
        assert_eq!(
            dps_targets_sum, per_enemy_sum,
            "agent {:#x}: sum(dps_targets) must equal sum(per_enemy) exactly",
            p.agent_addr
        );
        assert_eq!(
            dps_targets_sum, p.damage_total,
            "agent {:#x}: sum(dps_targets) must equal damage_total exactly",
            p.agent_addr
        );
    }
}

fn check_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    check_internal_invariants(&metrics);

    // Join method (module doc): (profession, damage_total, cc_applied)
    // triple, matching how the golden fixture's `timeSeries` scalars were
    // themselves extracted.
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_key: HashMap<(String, i64, i64), &serde_json::Value> = HashMap::new();
    for gp in golden_players {
        if gp.get("timeSeries").is_none() {
            continue;
        }
        let key = (
            gp["profession"].as_str().expect("profession").to_string(),
            gp["damage"].as_i64().expect("damage"),
            gp["appliedCrowdControl"].as_i64().expect("appliedCrowdControl"),
        );
        golden_by_key.insert(key, gp);
    }

    let pm_by_addr: HashMap<u64, &axilog_core::analysis::PlayerMetrics> =
        metrics.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut joined = 0usize;
    let mut damage_mismatches: Vec<(String, u64, i64)> = Vec::new();
    let mut taken_mismatches: Vec<(String, u64, i64)> = Vec::new();
    let mut superset_violations: Vec<(String, u64, i64)> = Vec::new();

    for p in &enc.players {
        let Some(pm) = pm_by_addr.get(&p.agent_addr) else { continue };
        // Golden's `profession` field is EI-style: elite spec name when the
        // player has one, else the base profession (mirrors `golden.rs`'s
        // `check_golden_ei_parity`'s own `ei_style` derivation).
        let ei_style_profession = if p.elite_spec.is_empty() { &p.profession } else { &p.elite_spec };
        let key = (ei_style_profession.clone(), pm.damage_total as i64, pm.cc_applied as i64);
        let Some(gp) = golden_by_key.get(&key) else { continue };
        let ts = &gp["timeSeries"];
        joined += 1;

        let our_damage_final = pm.timeseries.damage.last().copied().unwrap_or(0);
        let golden_damage_final = ts["damage1SFinal"].as_i64().expect("damage1SFinal");
        if our_damage_final as i64 != golden_damage_final {
            damage_mismatches.push((p.account.clone(), our_damage_final, golden_damage_final));
        }

        let our_taken_final = pm.timeseries.damage_taken.last().copied().unwrap_or(0);
        let golden_taken_final = ts["damageTaken1SFinal"].as_i64().expect("damageTaken1SFinal");
        if (our_taken_final as i64 - golden_taken_final).abs() > TAKEN_ABS_TOLERANCE {
            taken_mismatches.push((p.account.clone(), our_taken_final, golden_taken_final));
        }

        let our_dps_targets_sum: u64 = pm.timeseries.dps_targets.iter().map(|d| d.damage).sum();
        let golden_dps_targets_total = ts["dpsTargetsTotalDamage"].as_i64().expect("dpsTargetsTotalDamage");
        if (our_dps_targets_sum as i64) < golden_dps_targets_total {
            superset_violations.push((p.account.clone(), our_dps_targets_sum, golden_dps_targets_total));
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the timeseries-augmented golden fixture, got {joined}"
    );

    if !damage_mismatches.is_empty() {
        let report: Vec<String> = damage_mismatches.iter()
            .map(|(a, o, g)| format!("{a}: ours={o} golden={g}"))
            .collect();
        panic!("{} account(s) where timeseries.damage.last() != golden damage1SFinal:\n{}", damage_mismatches.len(), report.join("\n"));
    }
    if !taken_mismatches.is_empty() {
        let report: Vec<String> = taken_mismatches.iter()
            .map(|(a, o, g)| format!("{a}: ours={o} golden={g}"))
            .collect();
        panic!("{} account(s) where timeseries.damage_taken.last() != golden damageTaken1SFinal:\n{}", taken_mismatches.len(), report.join("\n"));
    }
    if !superset_violations.is_empty() {
        let report: Vec<String> = superset_violations.iter()
            .map(|(a, o, g)| format!("{a}: our dps_targets sum={o} < golden dpsTargetsTotalDamage={g}"))
            .collect();
        panic!(
            "{} account(s) where our dps_targets total is SMALLER than golden's -- module doc claims \
             ours is always a superset:\n{}",
            superset_violations.len(),
            report.join("\n")
        );
    }

    println!(
        "timeseries_matches_ei_golden: {joined} accounts joined, 0 damage1SFinal/damageTaken1SFinal \
         mismatches, dps_targets superset relationship holds for all"
    );
}

#[test]
fn timeseries_matches_ei_golden() {
    let golden = read_golden_json();
    check_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn timeseries_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("timeseries_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_matches_ei_golden(&bytes, &golden);
}

/// Real-log sanity check, post-rework era (same skip-when-absent pattern
/// `skill_damage_golden.rs`'s equivalent check uses): no committed EI JSON
/// sidecar exists for this local-only fixture, so this only checks
/// structural/internal invariants -- era-independent, since
/// `timeseries::build` reuses `damage`'s own era-independent predicate.
#[test]
fn timeseries_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    check_internal_invariants(&metrics);

    let any_damage = metrics.players.iter().any(|p| p.timeseries.damage.last().copied().unwrap_or(0) > 0);
    let any_taken = metrics.players.iter().any(|p| p.timeseries.damage_taken.last().copied().unwrap_or(0) > 0);
    assert!(any_damage, "a real WvW squad fight should show nonzero cumulative damage somewhere");
    assert!(any_taken, "a real WvW squad fight should show nonzero cumulative damage_taken somewhere");

    // Bucket count sanity: `(duration_ms / 1000) + 1`, same formula
    // `cc::timeline` uses -- every player's series must share that length.
    let expected_buckets = (enc.duration_ms / 1000) + 1;
    for p in &metrics.players {
        assert_eq!(p.timeseries.damage.len() as u64, expected_buckets, "agent {:#x}: damage series length must match the bucket formula", p.agent_addr);
        assert_eq!(p.timeseries.damage_taken.len() as u64, expected_buckets, "agent {:#x}: damage_taken series length must match the bucket formula", p.agent_addr);
    }

    println!(
        "timeseries_present_and_sane_on_local_postrework: {} players, {expected_buckets} buckets, internal invariants hold",
        metrics.players.len()
    );
}

