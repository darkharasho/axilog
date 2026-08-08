//! M3 Task 3: support-stat (condition cleanses / boon strips / resurrects)
//! calibration against the EI golden fixture.
//!
//! Joins by raw agent-table index exactly like `golden.rs`'s
//! `professions_match_ei_golden` / `boons_golden.rs`'s
//! `boon_uptimes_match_ei_golden` (`anon_account(i)` -> golden `account`),
//! then compares `analysis::support::apply`'s per-player `PlayerMetrics::
//! support` counts against `fixtures/wvw-small.ei.json` `players[].support`
//! (EI's raw `support[0]` `{condiCleanse, condiCleanseSelf, boonStrips,
//! resurrects}` fields, verbatim -- see that fixture's `_note`). Also checks
//! the squad-wide sums against the fixture's `squadCondiCleanse`/
//! `squadCondiCleanseSelf`/`squadBoonStrips`/`squadResurrects` (801/97/437/6
//! -- the M3 Task 3 brief's exact calibration targets).
//!
//! Tolerance: EXACT, both squad-wide and per-player (no allowlist needed --
//! see `analysis::support::apply`'s doc comment for the two ground-truth
//! findings, verified against the real deployed GW2EIEvtcParser.dll via a
//! decompiled/live C# harness, that were required to reach exact parity).

use axilog_core::analysis::analyze;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

fn read_local_fixture_or_skip(test_name: &str) -> Option<Vec<u8>> {
    match std::fs::read(LOCAL_FIXTURE_PATH) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {LOCAL_FIXTURE_PATH} absent ({test_name} local-only extra check)");
            None
        }
    }
}

fn read_anon_fixture() -> Vec<u8> {
    std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"))
}

fn read_golden_json() -> serde_json::Value {
    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&golden_str).expect("parse golden EI JSON")
}

struct Mismatch {
    account: String,
    field: &'static str,
    ours: u32,
    golden: i64,
}

fn check_support_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    // Squad-wide sums (the M3 Task 3 brief's exact calibration targets).
    let squad_cleanses: u64 = metrics.players.iter().map(|p| p.support.cleanses as u64).sum();
    let squad_cleanses_self: u64 =
        metrics.players.iter().map(|p| p.support.cleanses_self as u64).sum();
    let squad_strips: u64 = metrics.players.iter().map(|p| p.support.strips as u64).sum();
    let squad_resurrects: u64 = metrics.players.iter().map(|p| p.support.resurrects as u64).sum();

    let golden_cleanses = golden["squadCondiCleanse"].as_i64().expect("squadCondiCleanse");
    let golden_cleanses_self =
        golden["squadCondiCleanseSelf"].as_i64().expect("squadCondiCleanseSelf");
    let golden_strips = golden["squadBoonStrips"].as_i64().expect("squadBoonStrips");
    let golden_resurrects = golden["squadResurrects"].as_i64().expect("squadResurrects");

    assert_eq!(squad_cleanses, golden_cleanses as u64, "squad condiCleanse");
    assert_eq!(squad_cleanses_self, golden_cleanses_self as u64, "squad condiCleanseSelf");
    assert_eq!(squad_strips, golden_strips as u64, "squad boonStrips");
    assert_eq!(squad_resurrects, golden_resurrects as u64, "squad resurrects");
    assert_eq!(
        (squad_cleanses, squad_cleanses_self, squad_strips, squad_resurrects),
        (801, 97, 437, 6),
        "squad support totals must match the M3 Task 3 brief's exact calibration targets"
    );

    // Per-player: join by raw agent-table index (same method as
    // `boons_golden.rs`), assert exact per-account match.
    let golden_players = golden["players"].as_array().expect("players array");
    let mut support_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        if let Some(support) = p.get("support") {
            support_by_account.insert(account, support);
        }
    }

    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut joined = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(&support) = support_by_account.get(&key) else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else {
            continue;
        };
        joined += 1;

        let checks: [(&str, u32, i64); 4] = [
            ("condiCleanse", pm.support.cleanses, support["condiCleanse"].as_i64().unwrap()),
            (
                "condiCleanseSelf",
                pm.support.cleanses_self,
                support["condiCleanseSelf"].as_i64().unwrap(),
            ),
            ("boonStrips", pm.support.strips, support["boonStrips"].as_i64().unwrap()),
            ("resurrects", pm.support.resurrects, support["resurrects"].as_i64().unwrap()),
        ];
        for (field, ours, golden_v) in checks {
            if ours as i64 != golden_v {
                mismatches.push(Mismatch { account: key.clone(), field, ours, golden: golden_v });
            }
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the support-augmented golden fixture, got {joined}"
    );

    if !mismatches.is_empty() {
        let report: Vec<String> = mismatches
            .iter()
            .map(|m| format!("{} {}: ours={} golden={}", m.account, m.field, m.ours, m.golden))
            .collect();
        panic!(
            "{} per-player support cell(s) out of tolerance (checked {joined} accounts, EXACT \
             tolerance, no allowlist):\n{}",
            mismatches.len(),
            report.join("\n")
        );
    }

    println!(
        "support_matches_ei_golden: squad totals exact (801/97/437/6), {joined} accounts joined, \
         0 per-player mismatches"
    );
}

#[test]
fn support_matches_ei_golden() {
    let golden = read_golden_json();
    check_support_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn support_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("support_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_support_matches_ei_golden(&bytes, &golden);
}
