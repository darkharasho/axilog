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
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false);
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

/// M12 Task 3: `to_ei_json`'s `totalDamageDist`/`damage1S` mapping,
/// calibrated against the SAME golden sidecar fields
/// `skill_damage_golden.rs`/`timeseries_golden.rs` already calibrate the
/// underlying `axilog_core` metrics against (`fixtures/wvw-small.ei.json`'s
/// `players[].skillDamage`/`players[].timeSeries`) -- this test's job is
/// narrower: confirm the ei-json ADAPTER LAYER carries those already-
/// calibrated numbers through into EI's own array shape correctly, not
/// re-derive the calibration itself.
///
/// - `totalDamageDist[0]`: every shared skill id vs golden's
///   `skillDamage.outgoing` -- **EXACT** (same "every shared id matches
///   exactly" bar `skill_damage_golden.rs` established for the underlying
///   `SkillEntry`s this just re-shapes).
/// - `damage1S[0].last()` vs golden's `dpsAll[0].damage` scalar --
///   **EXACT** (the M12 Task 3 brief's specific calibration bar: "damage1S
///   final == scalar").
///
/// Join method: `anon_account(raw agent-table index)`, same as
/// `skill_damage_golden.rs` (reaches the same ~37/41 real accounts; the
/// other 4 `Non Squad Player N` placeholder rows have no real account to
/// join through, same documented limitation).
#[test]
fn ei_json_per_skill_and_per_second_blocks_match_the_golden() {
    use axilog_core::evtc::anon_account;
    use std::collections::HashMap;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    // Both opt-in blocks requested -- this test's whole point is
    // calibrating what `to_ei_json` emits WHEN they're present (the
    // gate-respecting omission-when-absent behavior is covered by
    // `axilog-ei`'s own unit tests, not this golden-fixture test).
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, true);
    let ei = axilog_ei::to_ei_json(&report, &[]);

    let mut joined = 0usize;
    let mut dist_mismatches: Vec<String> = Vec::new();
    let mut damage1s_mismatches: Vec<String> = Vec::new();

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(sd) = golden_p.get("skillDamage") else { continue };
        let Some(player_idx) = enc.players.iter().position(|p| {
            p.agent_addrs.contains(&agent.addr)
        }) else { continue };
        joined += 1;

        let our_player = &ei["players"][player_idx];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key.as_str()),
            "positional join sanity: ei-json players[{player_idx}] must be this same account"
        );

        // -- totalDamageDist[0]: every shared skill id, EXACT. --
        let golden_outgoing: HashMap<u32, i64> = sd["outgoing"]
            .as_array()
            .expect("outgoing array")
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["total"].as_i64().unwrap()))
            .collect();
        let our_dist = our_player["totalDamageDist"][0]
            .as_array()
            .unwrap_or_else(|| panic!("account {key}: totalDamageDist[0] must be an array"));
        let our_outgoing: HashMap<u32, i64> = our_dist
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["totalDamage"].as_i64().unwrap()))
            .collect();
        for (&id, &gtotal) in &golden_outgoing {
            let ours = our_outgoing.get(&id).copied();
            if ours != Some(gtotal) {
                dist_mismatches.push(format!("{key} skill {id}: ours={ours:?} golden={gtotal}"));
            }
        }

        // -- damage1S[0].last() vs golden's dpsAll[0].damage scalar, EXACT. --
        let golden_damage = golden_p["damage"].as_i64().expect("damage");
        let our_damage1s_final = our_player["damage1S"][0]
            .as_array()
            .unwrap_or_else(|| panic!("account {key}: damage1S[0] must be an array"))
            .last()
            .unwrap_or_else(|| panic!("account {key}: damage1S[0] must be non-empty"))
            .as_i64()
            .expect("damage1S[0].last() is numeric");
        if our_damage1s_final != golden_damage {
            damage1s_mismatches.push(format!("{key}: damage1S final={our_damage1s_final} golden dpsAll[0].damage={golden_damage}"));
        }
    }

    assert!(joined >= 30, "expected at least 30 accounts to join, got {joined}");
    assert!(
        dist_mismatches.is_empty(),
        "{} totalDamageDist shared skill-id mismatch(es):\n{}",
        dist_mismatches.len(),
        dist_mismatches.join("\n")
    );
    assert!(
        damage1s_mismatches.is_empty(),
        "{} damage1S-final mismatch(es):\n{}",
        damage1s_mismatches.len(),
        damage1s_mismatches.join("\n")
    );

    println!(
        "ei_json_per_skill_and_per_second_blocks_match_the_golden: {joined} accounts joined, \
         0 totalDamageDist shared-id mismatches, 0 damage1S-final mismatches"
    );
}
