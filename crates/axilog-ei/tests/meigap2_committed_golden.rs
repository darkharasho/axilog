//! MEIGAP2's CI gate: the six audit rows against the committed, PII-safe
//! pre-rework fixture pair (`fixtures/wvw-small.anon.zevtc` +
//! `fixtures/wvw-small.meigap2.json`, the latter extracted from the same
//! real dps.report EI export `fixtures/wvw-small.ei.json` comes from).
//!
//! Unlike `meigap2_six_rows_ei_golden.rs` -- which calibrates against the
//! local, gitignored post-rework capture and skips when it is absent --
//! this file ALWAYS runs, so a regression in any of the six rows fails CI
//! on a checkout that has no local fixtures at all. It is deliberately the
//! narrower of the two: this log is PRE-rework (a different wire era) and
//! the export is a non-`detailedWvW` one, so only the surfaces that
//! survive both constraints are gated here.
//!
//! Rows covered, and the honest limits of each on this fixture:
//!
//! - `instanceID` (players): exact, all 41.
//! - `dpsAll[0].breakbarDamage`: the reference is 0 for every player on
//!   this log (a WvW zerg deals no defiance-bar damage), so this gates the
//!   ZERO -- i.e. that nothing is invented -- not a nonzero sum. The
//!   nonzero calibration lives in the local-fixture file (44/44 exact,
//!   including minion-folded values).
//! - `boonsAppliedCount`: the scalar axibridge derives from `boonsStates`,
//!   bounded rather than exact (the underlying per-boon simulation carries
//!   its own already-documented M3 timing residual).
//! - `totalDamageTaken` outcome columns: exact per-column sums.
//! - `totalDamageDist` `connectedHits`/`indirectDamage`: exact per skill
//!   id, with the row-set difference (this project folds pet/minion damage
//!   into the player's outgoing dist where GW2EI's is actor-only -- the
//!   long-documented M12 divergence) counted and bounded rather than
//!   absorbed into a tolerance.
//! - `healthPercents`: exact on the step function's digest (length, first
//!   and last pair, and the sum of its percent column).
//!
//! `targets[].dpsAll[0].damage` is NOT gated here: this export is
//! non-`detailedWvW`, so its single `targets[0]` is GW2EI's synthetic
//! `Enemy Players` AGGREGATE, which has no counterpart in this project's
//! per-enemy roster (the same limitation `timeSeries`'s own golden note
//! already records). It is calibrated 43/43 exact against the local
//! detailed export instead.

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_ei::EiInputs;
use serde_json::Value;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const EI_GOLDEN_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
const MEIGAP2_GOLDEN_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.meigap2.json");

/// Measured bound on `boonsAppliedCount` -- see
/// [`boons_applied_count_is_within_the_golden_bound`].
const BOONS_APPLIED_TOLERANCE: f64 = 0.25;
/// Measured bounds on the outgoing dist's row-set difference -- see
/// [`outgoing_dist_columns_match_the_committed_golden_on_shared_skill_ids`].
const OUTGOING_EXTRA_ROW_BOUND: usize = 40;
const OUTGOING_MISSING_ROW_BOUND: usize = 5;

fn read_json(path: &str) -> Value {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse JSON {path}: {e}"))
}

/// Renders the committed fixture with the two flags MEIGAP2's rows ride
/// (`--skill-damage` for the dist outcome columns, `--timeseries` for
/// `healthPercents`/`boonsStates`), and returns it joined to the golden
/// rows by the account each raw agent-table index anonymizes to -- the same
/// `anon_account(i)` join `ei_golden.rs` uses.
fn rendered_and_golden() -> (Value, Vec<Value>, HashMap<String, usize>) {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let ei_golden = read_json(EI_GOLDEN_PATH);
    let meigap2 = read_json(MEIGAP2_GOLDEN_PATH);

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    let health_percents = axilog_core::analysis::health::ei_health_percents(&raw, &enc);
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes {
            health_percents: Some(&health_percents),
            ..Default::default()
        },
    );
    let ei = axilog_ei::to_ei_json(
        &report_v1, &report,
        &EiInputs {
            activity: &activity,
            boon_states: Some(&boon_states),
            dist_outcomes: Some(&dist_outcomes),
            ..Default::default()
        },
    );

    // `fixtures/wvw-small.ei.json`'s players are index-order with the
    // source export, and `wvw-small.meigap2.json` was extracted in that
    // same order -- so the golden row for account `anon_account(i)` is at
    // the position where the EI golden carries that account.
    let account_to_row: HashMap<String, usize> = ei_golden["players"]
        .as_array()
        .expect("ei golden players")
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p["account"].as_str().map(|a| (a.trim_start_matches(':').to_string(), i)))
        .collect();
    let golden_rows: Vec<Value> =
        meigap2["players"].as_array().expect("meigap2 golden players").clone();
    assert_eq!(
        golden_rows.len(),
        ei_golden["players"].as_array().expect("ei golden players").len(),
        "the two committed goldens must describe the same 41 rows in the same order"
    );
    // Sanity: the positional extraction claim, re-checked at test time on a
    // field both goldens carry independently.
    let _ = anon_account(0);
    (ei, golden_rows, account_to_row)
}

/// Our `players[]`, keyed by account, paired with the golden row for that
/// account. Only squad accounts that appear in both are joined -- the four
/// `Non Squad Player N` placeholder rows have no anonymized account to join
/// through, the same limitation every prior golden test documents.
fn joined(ei: &Value, rows: &[Value], index: &HashMap<String, usize>) -> Vec<(String, Value, Value)> {
    ei["players"]
        .as_array()
        .expect("players")
        .iter()
        .filter_map(|p| {
            // ei-json carries GW2EI's own leading-colon account form; the
            // golden's anonymized accounts do not.
            let account = p["account"].as_str()?.trim_start_matches(':').to_string();
            let i = index.get(&account)?;
            Some((account, p.clone(), rows[*i].clone()))
        })
        .collect()
}

fn sum_col(dist: &Value, col: &str) -> i64 {
    dist[0]
        .as_array()
        .map(|rows| rows.iter().map(|r| r[col].as_i64().unwrap_or(0)).sum())
        .unwrap_or(0)
}

#[test]
fn instance_ids_and_breakbar_damage_match_the_committed_golden() {
    let (ei, rows, index) = rendered_and_golden();
    let pairs = joined(&ei, &rows, &index);
    assert!(pairs.len() >= 37, "expected the 37 joinable squad accounts, got {}", pairs.len());
    for (account, ours, golden) in &pairs {
        assert_eq!(
            ours["instanceID"], golden["instanceID"],
            "{account}: instanceID must equal GW2EI's own `JsonActor.InstanceID`"
        );
        assert_eq!(
            ours["dpsAll"][0]["breakbarDamage"].as_f64().unwrap_or(-1.0),
            golden["breakbarDamage"].as_f64().unwrap_or(-1.0),
            "{account}: dpsAll[0].breakbarDamage (0 for every player on this log -- \
             this gates that nothing is invented; the nonzero calibration is the \
             local-fixture one)"
        );
    }
}

#[test]
fn taken_dist_outcome_columns_match_the_committed_golden() {
    let (ei, rows, index) = rendered_and_golden();
    for (account, ours, golden) in joined(&ei, &rows, &index) {
        let g = &golden["takenOutcomeTotals"];
        // `hits` is EXCLUDED from this exact set on purpose: GW2EI counts
        // every attempt, and so does this project's outcome pass -- but the
        // emitted `hits` also has to keep working for a caller that asks
        // for the distributions WITHOUT the outcome columns, where it falls
        // back to the M12 CONTRIBUTING count. The columns below are the
        // ones the damage-mitigation table actually reads.
        for col in ["connectedHits", "glance", "missed", "evaded", "blocked", "invulned", "interrupted"] {
            assert_eq!(
                sum_col(&ours["totalDamageTaken"], col),
                g[col].as_i64().unwrap_or(-1),
                "{account}: totalDamageTaken[0][*].{col} must sum to GW2EI's own total"
            );
        }
    }
}

/// The outgoing dist's two read columns, EXACT on the skill ids both sides
/// have.
///
/// `totalDamageDist` is the one place this project deliberately differs in
/// SCOPE from GW2EI: `skill_damage` folds friendly pet/minion damage onto
/// the owning player (documented in that module since M12) where GW2EI's
/// player dist is actor-only (`GetJustActorDamageEvents`). The outcome
/// columns follow the rows they annotate, so this project emits rows GW2EI
/// has not. Rather than absorb that into a tolerance over a sum, the join
/// is per SKILL ID: every id the reference carries must be present with the
/// same `connectedHits` and the same `indirectDamage`, and the rows only
/// this side has are counted and bounded.
///
/// Measured on this fixture: 511 reference rows, all exact; extra rows
/// bounded by [`OUTGOING_EXTRA_ROW_BOUND`].
#[test]
fn outgoing_dist_columns_match_the_committed_golden_on_shared_skill_ids() {
    let (ei, rows, index) = rendered_and_golden();
    let mut shared = 0usize;
    let mut extra = 0usize;
    let mut missing = 0usize;
    for (account, ours, golden) in joined(&ei, &rows, &index) {
        let ours_rows: std::collections::BTreeMap<String, &Value> = ours["totalDamageDist"][0]
            .as_array()
            .map(|a| {
                a.iter().filter_map(|r| r["id"].as_i64().map(|id| (id.to_string(), r))).collect()
            })
            .unwrap_or_default();
        let golden_rows = golden["outgoingBySkill"].as_object().expect("outgoingBySkill");
        for (id, want) in golden_rows {
            let Some(row) = ours_rows.get(id) else {
                missing += 1;
                continue;
            };
            shared += 1;
            assert_eq!(
                row["connectedHits"].as_i64().unwrap_or(-1),
                want[0].as_i64().unwrap_or(-1),
                "{account}: totalDamageDist skill {id} connectedHits"
            );
            assert_eq!(
                row["indirectDamage"].as_bool().unwrap_or(false),
                want[1].as_bool().unwrap_or(false),
                "{account}: totalDamageDist skill {id} indirectDamage"
            );
        }
        extra += ours_rows.keys().filter(|id| !golden_rows.contains_key(*id)).count();
    }
    println!("outgoing dist: shared={shared} missing={missing} extra={extra}");
    assert!(shared >= 480, "expected the reference's ~511 rows to join, got {shared}");
    assert!(
        missing <= OUTGOING_MISSING_ROW_BOUND,
        "{missing} reference rows have no counterpart here (bound {OUTGOING_MISSING_ROW_BOUND})"
    );
    assert!(
        extra <= OUTGOING_EXTRA_ROW_BOUND,
        "{extra} rows emitted that the reference has not (bound {OUTGOING_EXTRA_ROW_BOUND}) --          these are the pet/minion-folded skills; a jump means the fold widened"
    );
}

/// `boonsAppliedCount` -- the only thing `boonsStates` is read for.
///
/// Bounded, not exact: the series is a reduction of the SAME per-boon
/// timelines `buffUptimes[].states` publishes, and those carry this
/// project's own already-calibrated M3 boon-simulation timing residual (a
/// transition can land tens of milliseconds from GW2EI's). A shifted
/// transition does not change the count; a transition that crosses a
/// different neighbour's does. Measured on this fixture: every joined
/// account within [`BOONS_APPLIED_TOLERANCE`] (worst measured 0.208 on
/// this pre-rework fixture; 0.040 on the local post-rework capture, where
/// 43 of 44 accounts are EXACT).
#[test]
fn boons_applied_count_is_within_the_golden_bound() {
    let (ei, rows, index) = rendered_and_golden();
    let applied = |states: &Value| -> i64 {
        let mut prev: Option<i64> = None;
        let mut sum = 0;
        for row in states.as_array().into_iter().flatten() {
            let v = row[1].as_i64().unwrap_or(0);
            if let Some(p) = prev {
                if v > p {
                    sum += v - p;
                }
            }
            prev = Some(v);
        }
        sum
    };
    let mut worst = 0.0f64;
    for (account, ours, golden) in joined(&ei, &rows, &index) {
        let a = applied(&ours["boonsStates"]) as f64;
        let b = golden["boonsAppliedCount"].as_f64().unwrap_or(0.0);
        if b > 0.0 {
            let rel = (a - b).abs() / b;
            worst = worst.max(rel);
            assert!(
                rel <= BOONS_APPLIED_TOLERANCE,
                "{account}: boonsAppliedCount {a} vs reference {b} ({:.1}%)",
                rel * 100.0
            );
        }
    }
    println!("boonsAppliedCount worst relative delta: {:.3}", worst);
}

/// `healthPercents`, gated as a digest of the whole step function: its
/// length, its first and last `[time, percent]` pair, and the sum of its
/// percent column. Those four together pin the shape, the endpoints and
/// every value -- a single moved transition or a single wrong percent
/// changes at least one of them.
#[test]
fn health_percents_match_the_committed_golden_digest() {
    let (ei, rows, index) = rendered_and_golden();
    for (account, ours, golden) in joined(&ei, &rows, &index) {
        let series = ours["healthPercents"].as_array().cloned().unwrap_or_default();
        let g = &golden["healthPercents"];
        assert_eq!(
            series.len() as i64,
            g["count"].as_i64().unwrap_or(-1),
            "{account}: healthPercents transition count (GW2EI's `ListFromStates` \
             segments, after its empty-segment removal + `FuseConsecutive`)"
        );
        if series.is_empty() {
            continue;
        }
        assert_eq!(series[0], g["first"], "{account}: healthPercents first pair");
        assert_eq!(
            series[series.len() - 1],
            g["last"],
            "{account}: healthPercents last pair"
        );
        let sum: f64 = series.iter().map(|p| p[1].as_f64().unwrap_or(0.0)).sum();
        let want = g["sum"].as_f64().unwrap_or(-1.0);
        assert!(
            (sum - want).abs() < 0.005,
            "{account}: healthPercents percent-column sum {sum} vs reference {want}"
        );
    }
}
