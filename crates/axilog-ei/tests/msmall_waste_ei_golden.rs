//! MSMALL item 2, review round 1: the boon-generation `wasted` column, on
//! the EMITTED ei-json surface.
//!
//! Why this file exists rather than another assertion in
//! `axilog-core`'s `boons_golden.rs`: that test reads
//! `Metrics::boon_generation` directly, so it validates the SIMULATION but
//! is structurally blind to the ADAPTER. The first version of item 2 was
//! correct in the simulator and still dropped 9 real EI rows on the way
//! out, because `buff_generation_json` filtered `groupBuffs`/`squadBuffs`
//! on `generation > 0.0` and a waste-only source has `generation == 0`.
//! Nothing that reads the metrics map can catch that class of bug; only a
//! test that joins through the serialized document can.
//!
//! The membership assertion below is the durable guard: every reference-EI
//! row carrying a non-zero `wasted` must have a counterpart in our output.

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::{anon_account, decode_raw};
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

/// The twelve boons this project simulates. The reference export also
/// carries rows for buffs outside that set (food, runes, class buffs);
/// those are out of scope here and filtered out on both sides, exactly as
/// `boons_golden.rs` does.
fn tracked_boons() -> BTreeSet<u32> {
    axilog_core::analysis::buffs::BOON_IDS.iter().map(|&(id, _, _)| id).collect()
}

fn render_ei(bytes: &[u8]) -> Value {
    let raw = decode_raw(bytes).expect("decode fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    axilog_ei::to_ei_json(
        &report_v1, &report,
        &EiInputs { ..Default::default() },
    )
}

/// `(scope, buff id) -> buffData[0]` for one player object.
fn rows_of(player: &Value, tracked: &BTreeSet<u32>) -> BTreeMap<(&'static str, u32), Value> {
    let mut out = BTreeMap::new();
    for scope in ["selfBuffs", "groupBuffs", "squadBuffs"] {
        for e in player[scope].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
            let Some(id) = e["id"].as_u64() else { continue };
            let id = id as u32;
            if tracked.contains(&id) {
                out.insert((scope, id), e["buffData"][0].clone());
            }
        }
    }
    out
}

/// THE REGRESSION GUARD. Every reference-EI `groupBuffs`/`squadBuffs` row
/// with a non-zero `wasted` must be present in our emitted document.
///
/// GW2EI's id-set rule is `hasGeneration = buffDistribution.HasSrc(boon.ID,
/// srcAgentItem)` (`BuffStatistics.cs:67`), and `HasSrc` is bare key
/// presence (`BuffDistribution.cs:78-81`). `SimulationItem.AddWaste`
/// registers a source as `BuffDistributionItem(0, 0, value, 0, 0, 0)`
/// (`SimulationItem.cs:99-116`) -- `Value == 0`, `Waste == value` -- so a
/// source that only ever wasted IS a recorded source and EI emits its row.
#[test]
fn no_ei_row_with_nonzero_wasted_is_missing_from_our_output() {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let (Ok(bytes), Ok(golden_s)) =
        (std::fs::read(&zevtc), std::fs::read_to_string(&ei_json))
    else {
        println!("skip: {zevtc} / {ei_json} absent (MSMALL waste ei-surface guard)");
        return;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");
    let ours = render_ei(&bytes);
    let tracked = tracked_boons();

    let our_by_account: BTreeMap<String, &Value> = ours["players"]
        .as_array()
        .expect("players")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a).to_string(), p)))
        .collect();

    let mut joined = 0usize;
    let mut waste_rows = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut extension_only: Vec<String> = Vec::new();
    for g in golden["players"].as_array().expect("players") {
        let Some(acct) = g["account"].as_str().map(account_key) else { continue };
        let Some(o) = our_by_account.get(acct) else { continue };
        joined += 1;
        let our_rows = rows_of(o, &tracked);
        for scope in ["groupBuffs", "squadBuffs"] {
            for e in g[scope].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
                let Some(id) = e["id"].as_u64() else { continue };
                let id = id as u32;
                if !tracked.contains(&id) {
                    continue;
                }
                if e["buffData"][0]["wasted"].as_f64().unwrap_or(0.0) == 0.0 {
                    continue;
                }
                waste_rows += 1;
                if our_rows.contains_key(&(scope, id)) {
                    continue;
                }
                // A row we miss because the source reached EI's
                // distribution ONLY through the EXTENSION channel
                // (`byExtension != 0`) is a known, separate limitation:
                // `BuffExtensionEvent` folds onto the active stack's
                // existing source here rather than registering the extender
                // as a source of its own, so such a source has neither
                // generation nor waste in our model and no filter could
                // recover it. Counted, not excused -- see the pinned
                // `extension_only` assertion below.
                if e["buffData"][0]["byExtension"].as_f64().unwrap_or(0.0) != 0.0 {
                    extension_only.push(format!(
                        "  {acct} {scope} buff {id}: wasted={} byExtension={}",
                        e["buffData"][0]["wasted"], e["buffData"][0]["byExtension"]
                    ));
                    continue;
                }
                missing.push(format!(
                    "  {acct} {scope} buff {id}: EI has wasted={} but we emit no row",
                    e["buffData"][0]["wasted"]
                ));
            }
        }
    }

    assert!(joined >= 40, "expected a well-joined roster, got {joined}");
    assert!(
        missing.is_empty(),
        "{} reference-EI row(s) with non-zero `wasted` are absent from our ei-json \
         (of {waste_rows} such rows across {joined} players) -- the group/squad id-set \
         filter has regressed to generation-only:\n{}",
        missing.len(),
        missing.join("\n")
    );
    // Pinned so the extension-channel shortfall cannot silently grow while
    // being waved through as "known".
    assert_eq!(
        extension_only.len(),
        3,
        "expected exactly 3 known extension-channel-only misses on this capture, got {}:\n{}",
        extension_only.len(),
        extension_only.join("\n")
    );
    println!(
        "no_ei_row_with_nonzero_wasted_is_missing: {waste_rows} non-zero-wasted reference rows \
         across {joined} players; {} present, {} missed via the unmodelled extension channel only",
        waste_rows - extension_only.len(),
        extension_only.len()
    );
}

/// The waste VALUES on the emitted surface, joined through the serialized
/// document rather than the metrics map.
///
/// Tolerances mirror `axilog-core`'s `boons_golden.rs` and carry the same
/// justification: everything except Regeneration is essentially exact,
/// while Regeneration is a known bounded gap (GW2EI's stack-instance
/// threading for `HealingLogic`, which needs per-stack IDs this project
/// does not carry). See that file's `WASTED_TOLERANCE_PP` /
/// `REGEN_WASTED_TOLERANCE_PP` doc comments for the full writeup.
#[test]
fn emitted_wasted_values_match_reference_ei() {
    const WASTED_TOLERANCE_PP: f64 = 0.5;
    const REGEN_WASTED_TOLERANCE_PP: f64 = 12.0;

    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let (Ok(bytes), Ok(golden_s)) =
        (std::fs::read(&zevtc), std::fs::read_to_string(&ei_json))
    else {
        println!("skip: {zevtc} / {ei_json} absent (MSMALL waste value check)");
        return;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");
    let ours = render_ei(&bytes);
    let tracked = tracked_boons();

    let our_by_account: BTreeMap<String, &Value> = ours["players"]
        .as_array()
        .expect("players")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a).to_string(), p)))
        .collect();

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut worst_non_regen = 0.0f64;
    let mut worst_regen = 0.0f64;
    for g in golden["players"].as_array().expect("players") {
        let Some(acct) = g["account"].as_str().map(account_key) else { continue };
        let Some(o) = our_by_account.get(acct) else { continue };
        let our_rows = rows_of(o, &tracked);
        for ((scope, id), ours_row) in &our_rows {
            let Some(g_row) = g[*scope]
                .as_array()
                .and_then(|a| a.iter().find(|e| e["id"].as_u64() == Some(*id as u64)))
            else {
                continue;
            };
            let o_w = ours_row["wasted"].as_f64().unwrap_or(0.0);
            let g_w = g_row["buffData"][0]["wasted"].as_f64().unwrap_or(0.0);
            checked += 1;
            let delta = (o_w - g_w).abs();
            let is_regen = *id == axilog_core::analysis::buffs::REGENERATION;
            let tol = if is_regen { REGEN_WASTED_TOLERANCE_PP } else { WASTED_TOLERANCE_PP };
            if is_regen {
                worst_regen = worst_regen.max(delta);
            } else {
                worst_non_regen = worst_non_regen.max(delta);
            }
            if delta > tol {
                failures.push(format!(
                    "  {acct} {scope} buff {id}: ours={o_w:.3} ei={g_w:.3} delta={delta:.3}pp (tol {tol})"
                ));
            }
        }
    }

    assert!(checked >= 500, "expected a substantial cell count, got {checked}");
    assert!(
        failures.is_empty(),
        "{} emitted `wasted` cell(s) out of tolerance (checked {checked}):\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!(
        "emitted_wasted_values_match_reference_ei: {checked} emitted cells, worst \
         non-Regeneration {worst_non_regen:.3}pp, worst Regeneration {worst_regen:.3}pp"
    );
}

/// Pins the COMMITTED-fixture waste-only cell count, so the fix is regression
/// -tested in CI too (the two checks above are local-fixture-gated).
///
/// A "waste-only" cell is a `groupBuffs`/`squadBuffs` row with
/// `generation == 0` and `wasted != 0` -- precisely the rows the pre-review
/// generation-only filter dropped. Nine of them exist on
/// `fixtures/wvw-small.anon.zevtc`; if the filter regresses, this hits 0.
#[test]
fn committed_fixture_emits_waste_only_rows() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let ours = render_ei(&bytes);
    let tracked = tracked_boons();

    let mut waste_only = 0usize;
    let mut max_wasted = 0.0f64;
    for p in ours["players"].as_array().expect("players") {
        for scope in ["groupBuffs", "squadBuffs"] {
            for e in p[scope].as_array().map(|v| v.as_slice()).unwrap_or(&[]) {
                let Some(id) = e["id"].as_u64() else { continue };
                if !tracked.contains(&(id as u32)) {
                    continue;
                }
                let bd = &e["buffData"][0];
                let w = bd["wasted"].as_f64().unwrap_or(0.0);
                if bd["generation"].as_f64().unwrap_or(0.0) == 0.0 && w != 0.0 {
                    waste_only += 1;
                    max_wasted = max_wasted.max(w);
                }
            }
        }
    }
    assert_eq!(
        waste_only, 9,
        "expected the 9 known waste-only group/squadBuffs cells on the committed fixture \
         (a source whose every stack was overwritten or stripped before holding time -- \
         EI emits these, see `buff_generation_json`'s doc comment); got {waste_only}. \
         A drop to 0 means the id-set filter regressed to generation-only."
    );
    // The largest is an Aegis `groupBuffs` entry; pin the magnitude so a
    // silent value collapse is caught alongside a membership collapse.
    assert!(
        (max_wasted - 18.247).abs() < 0.001,
        "largest waste-only cell should be the Aegis groupBuffs 18.247, got {max_wasted}"
    );
    println!("committed_fixture_emits_waste_only_rows: {waste_only} cells, max {max_wasted}");
    // `anon_account` is the committed fixture's account convention; touch it
    // so the import stays meaningful if this test grows an account join.
    let _ = anon_account(0);
}
