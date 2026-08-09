//! M11 Task 3: ei-json calibration for the three axibridge tier-1 wins --
//! `targets[].isFake`, `players[].combatReplayData.{down,dead}`, and
//! `players[].activeTimes` -- against the committed EI golden
//! (`fixtures/wvw-small.ei.json`, itself extracted from axibridge's
//! `test-fixtures/boon/20260117-181030.json`, the real dps.report EI export
//! for this same log -- see that golden file's `_note` field, "Task 3
//! (M11)" entry, for the exact extraction/join method).
//!
//! `down`/`dead` are asserted byte-exact (the replay module's own doc
//! comment claims exact reproduction of GW2EI's own down/dead arrays;
//! this test is that claim's adapter-level assertion). `activeTimes` is
//! calibrated to within 0.5% per player (see
//! `axilog_core::analysis::replay::ActivityIntervals::active_ms`'s doc
//! comment for why this project's formula isn't expected to be
//! byte-exact -- it doesn't track GW2EI's rarer mid-log despawn/respawn
//! `dc` segments).

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

/// Fraction-of-value tolerance for `activeTimes` (M11 Task 3 brief: within
/// 0.5% of the EI golden per player).
const ACTIVE_TIMES_TOLERANCE: f64 = 0.005;

fn read_json(path: &str) -> serde_json::Value {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse JSON {path}: {e}"))
}

#[test]
fn ei_json_matches_the_golden_isfake_down_dead_and_active_times() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None);
    let ei = axilog_ei::to_ei_json(&report, &activity);

    // -- isFake: every target, no exceptions --
    let targets = ei["targets"].as_array().expect("targets must be an array");
    assert!(!targets.is_empty(), "expected at least one target");
    for t in targets {
        assert_eq!(t["isFake"], false, "target {t:?} must be isFake: false");
    }

    // -- down/dead/activeTimes: join by raw agent-table index -> Anon<N>
    // account -> golden row (same join `professions_match_ei_golden`/
    // `replay_calibrated_against_ei_combat_replay_data` already use). --
    let addr_to_index: HashMap<u64, usize> =
        raw.agents.iter().enumerate().map(|(i, a)| (a.addr, i)).collect();

    let mut players_checked = 0usize;
    let mut down_dead_players_checked = 0usize;
    let mut max_active_times_err_pct = 0.0f64;

    for (i, p) in enc.players.iter().enumerate() {
        let Some(&idx) = addr_to_index.get(&p.agent_addr) else { continue };
        let expected_account = anon_account(idx);
        let key = expected_account.trim_start_matches(':');
        let Some(gp) = golden_players.iter().find(|gp| gp["account"].as_str() == Some(key)) else {
            continue;
        };
        let Some(golden_crd) = gp.get("combatReplayData") else { continue };
        let Some(golden_active) = gp.get("activeTimes").and_then(|v| v.as_array()) else { continue };

        let our_player = &ei["players"][i];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key),
            "positional join sanity: ei-json players[{i}] must be this same account"
        );

        players_checked += 1;

        // down/dead: byte-exact.
        assert_eq!(
            our_player["combatReplayData"]["down"], golden_crd["down"],
            "down intervals must be byte-exact for account {key}"
        );
        assert_eq!(
            our_player["combatReplayData"]["dead"], golden_crd["dead"],
            "dead intervals must be byte-exact for account {key}"
        );
        if golden_crd["down"].as_array().is_some_and(|a| !a.is_empty())
            || golden_crd["dead"].as_array().is_some_and(|a| !a.is_empty())
        {
            down_dead_players_checked += 1;
        }

        // activeTimes: within 0.5%.
        let our_active = our_player["activeTimes"][0].as_f64().expect("our activeTimes[0] is numeric");
        let golden_active_val = golden_active[0].as_f64().expect("golden activeTimes[0] is numeric");
        let err_pct = if golden_active_val > 0.0 {
            (our_active - golden_active_val).abs() / golden_active_val
        } else {
            (our_active - golden_active_val).abs()
        };
        max_active_times_err_pct = max_active_times_err_pct.max(err_pct);
        assert!(
            err_pct <= ACTIVE_TIMES_TOLERANCE,
            "activeTimes for account {key}: ours={our_active} golden={golden_active_val} \
             ({:.3}% off, need <= {:.1}%)",
            err_pct * 100.0,
            ACTIVE_TIMES_TOLERANCE * 100.0
        );
    }

    println!(
        "ei_golden: players_checked={players_checked} down_dead_players_checked={down_dead_players_checked} \
         max_active_times_err={:.4}%",
        max_active_times_err_pct * 100.0
    );

    assert!(players_checked >= 30, "expected at least 30 matched players, got {players_checked}");
    // The golden fixture's own `_note` documents exactly 2 players (of 41)
    // with a non-empty down/dead array in this log -- `DaringCanyon.5440`
    // (a real account, reachable through this test's account-based join)
    // and `Non Squad Player 10` (one of the 4 rows with no real account to
    // join through at all, per the same `_note` -- unreachable here, same
    // limitation the M9/M3 golden tests already document for that row
    // type). This asserts the ONE reachable non-empty row was actually
    // exercised above, not silently skipped by the join.
    assert_eq!(
        down_dead_players_checked, 1,
        "expected the 1 (of 2) non-empty down/dead row reachable via the real-account join"
    );
}
