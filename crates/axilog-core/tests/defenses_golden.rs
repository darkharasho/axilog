//! M13 Task 2: incoming defenses calibration against EI's `defenses[0]`
//! (`analysis::defenses`).
//!
//! Same real-account join method as `hit_stats_golden.rs`/
//! `skill_damage_golden.rs`: raw agent-table index -> `anon_account(i)` ->
//! golden `account`. `fixtures/wvw-small.ei.json`'s per-player `defenses`
//! object (see that fixture's `_note`, M13 Task 2 addendum) carries a subset
//! of `defenses[0]`'s real fields verbatim, extracted from `axibridge/
//! test-fixtures/boon/20260117-181030.json` at the same player index.
//!
//! ## Tolerance (per the M13 plan)
//!
//! Counts EXACT where unambiguous: `blocked_count`/`evaded_count`/
//! `dodge_count`/`missed_count`/`interrupted_count`/`invulned_count`/
//! `strike_count`/`power_count`/`condition_count`/`life_leech_count`/
//! `barrier_count`/`breakbar_count`. Damage sums within 0.5% (relative, or
//! an absolute epsilon when the golden value is 0): `strike_damage`/
//! `power_damage`/`condition_damage`/`life_leech_damage`/`barrier_damage`/
//! `breakbar_damage`.
//!
//! ## The `life_leech` derivation -- a real GW2EI counting bug
//!
//! GW2EI's own `defenses[0].lifeLeechDamageTakenCount` is a KNOWN, VERIFIED
//! bug (see `analysis::defenses`'s module doc for the full source-line
//! citation): its ctor increments `LifeLeechDamageTaken` (the SUM field)
//! TWICE per life-leech hit -- once correctly, once by an evident
//! copy-paste mistake that should have been `LifeLeechDamageTakenCount++`.
//! The result: `lifeLeechDamageTakenCount` reports 0 for every one of this
//! fixture's 41 players (even the ones with substantial nonzero
//! `lifeLeechDamageTaken`), and the reported `lifeLeechDamageTaken` sum is
//! inflated by exactly the true hit count. This project deliberately does
//! NOT reproduce that bug -- `analysis::defenses::DefenseStats::
//! life_leech_count`/`life_leech_damage` hold the TRUE values. This test
//! therefore does NOT compare against the fixture's raw (buggy)
//! `lifeLeechDamageTaken(Count)` fields directly; instead it derives the
//! TRUE reference value algebraically from two fields that are NOT affected
//! by the bug: `powerDamageTaken(Count) - strikeDamageTaken(Count)` (see the
//! module doc's proof that `PowerDamageTaken(Count)` unconditionally equals
//! `StrikeDamageTaken(Count) + [true life-leech](Count)`, independent of the
//! bug, since the bug only touches the INNER life-leech-sum increment, not
//! the outer power-bucket increment).

use axilog_core::analysis::analyze;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-small.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
const LOCAL_POSTREWORK_ZEVTC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-postrework.zevtc");

/// Relative tolerance for damage sums (M13 plan: 0.5%).
const DAMAGE_REL_TOLERANCE: f64 = 0.005;
/// Absolute floor applied alongside the relative tolerance, so a golden
/// value of 0 (or very small) doesn't demand byte-exactness from a relative
/// check alone.
const DAMAGE_ABS_FLOOR: f64 = 2.0;

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

/// (native `DefenseStats` field name, golden `defenses` key) count pairs --
/// checked EXACT. `strike_count`/`power_count`/`condition_count` join
/// directly against EI's own `*Count` fields (unaffected by the life-leech
/// bug -- see module doc); `life_leech_count` is handled separately (its
/// golden reference is DERIVED, not read directly).
const COUNT_FIELDS: &[(&str, &str)] = &[
    ("blocked_count", "blockedCount"),
    ("evaded_count", "evadedCount"),
    ("missed_count", "missedCount"),
    ("dodge_count", "dodgeCount"),
    ("invulned_count", "invulnedCount"),
    ("interrupted_count", "interruptedCount"),
    ("strike_count", "strikeDamageTakenCount"),
    ("power_count", "powerDamageTakenCount"),
    ("condition_count", "conditionDamageTakenCount"),
    ("barrier_count", "damageBarrierCount"),
    ("breakbar_count", "breakbarDamageTakenCount"),
];

/// (native `DefenseStats` field name, golden `defenses` key) damage-sum
/// pairs -- checked within `DAMAGE_REL_TOLERANCE`. Same `life_leech_damage`
/// caveat as `COUNT_FIELDS` above.
const DAMAGE_FIELDS: &[(&str, &str)] = &[
    ("strike_damage", "strikeDamageTaken"),
    ("power_damage", "powerDamageTaken"),
    ("condition_damage", "conditionDamageTaken"),
    ("barrier_damage", "damageBarrier"),
    ("breakbar_damage", "breakbarDamageTaken"),
];

fn field_value(d: &axilog_core::analysis::defenses::DefenseStats, name: &str) -> u64 {
    match name {
        "blocked_count" => d.blocked_count as u64,
        "evaded_count" => d.evaded_count as u64,
        "missed_count" => d.missed_count as u64,
        "dodge_count" => d.dodge_count as u64,
        "invulned_count" => d.invulned_count as u64,
        "interrupted_count" => d.interrupted_count as u64,
        "strike_count" => d.strike_count as u64,
        "strike_damage" => d.strike_damage,
        "power_count" => d.power_count as u64,
        "power_damage" => d.power_damage,
        "condition_count" => d.condition_count as u64,
        "condition_damage" => d.condition_damage,
        "life_leech_count" => d.life_leech_count as u64,
        "life_leech_damage" => d.life_leech_damage,
        "barrier_count" => d.barrier_count as u64,
        "barrier_damage" => d.barrier_damage,
        "breakbar_count" => d.breakbar_count as u64,
        "breakbar_damage" => d.breakbar_damage,
        other => panic!("unknown DefenseStats field {other}"),
    }
}

fn check_defenses_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }
    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut joined = 0usize;
    let mut count_mismatches: Vec<String> = Vec::new();
    let mut damage_mismatches: Vec<String> = Vec::new();

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(de) = golden_p.get("defenses") else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        for &(our_field, golden_key) in COUNT_FIELDS {
            let ours = field_value(&pm.defenses, our_field);
            let golden_val = de[golden_key].as_i64().unwrap_or(0) as u64;
            if ours != golden_val {
                count_mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[{golden_key}]={golden_val}"
                ));
            }
        }

        // `life_leech_count`: derived reference (see module doc's bug
        // writeup), NOT read from the fixture's buggy
        // `lifeLeechDamageTakenCount` (always 0) directly.
        let power_count = de["powerDamageTakenCount"].as_i64().unwrap_or(0);
        let strike_count = de["strikeDamageTakenCount"].as_i64().unwrap_or(0);
        let true_life_leech_count = (power_count - strike_count).max(0) as u64;
        let ours_llc = pm.defenses.life_leech_count as u64;
        if ours_llc != true_life_leech_count {
            count_mismatches.push(format!(
                "{key} life_leech_count: ours={ours_llc} derived_golden={true_life_leech_count} \
                 (powerDamageTakenCount={power_count} - strikeDamageTakenCount={strike_count})"
            ));
        }

        for &(our_field, golden_key) in DAMAGE_FIELDS {
            let ours = field_value(&pm.defenses, our_field) as f64;
            let golden_val = de[golden_key].as_i64().unwrap_or(0) as f64;
            let diff = (ours - golden_val).abs();
            let allowed = (golden_val.abs() * DAMAGE_REL_TOLERANCE).max(DAMAGE_ABS_FLOOR);
            if diff > allowed {
                damage_mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[{golden_key}]={golden_val} diff={diff} allowed={allowed:.2}"
                ));
            }
        }

        // `life_leech_damage`: same derivation as the count above.
        let power_damage = de["powerDamageTaken"].as_i64().unwrap_or(0) as f64;
        let strike_damage = de["strikeDamageTaken"].as_i64().unwrap_or(0) as f64;
        let true_life_leech_damage = (power_damage - strike_damage).max(0.0);
        let ours_lld = pm.defenses.life_leech_damage as f64;
        let diff = (ours_lld - true_life_leech_damage).abs();
        let allowed = (true_life_leech_damage.abs() * DAMAGE_REL_TOLERANCE).max(DAMAGE_ABS_FLOOR);
        if diff > allowed {
            damage_mismatches.push(format!(
                "{key} life_leech_damage: ours={ours_lld} derived_golden={true_life_leech_damage} \
                 diff={diff} allowed={allowed:.2}"
            ));
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the defenses-augmented golden fixture, got {joined}"
    );

    if !count_mismatches.is_empty() {
        panic!(
            "{} count mismatch(es) out of EXACT tolerance (checked {joined} accounts):\n{}",
            count_mismatches.len(),
            count_mismatches.join("\n")
        );
    }
    if !damage_mismatches.is_empty() {
        panic!(
            "{} damage-sum mismatch(es) exceeding {DAMAGE_REL_TOLERANCE:.1}% tolerance \
             (checked {joined} accounts):\n{}",
            damage_mismatches.len(),
            damage_mismatches.join("\n")
        );
    }

    println!(
        "defenses_matches_ei_golden: {joined} accounts joined, all {} count fields EXACT \
         (including derived life_leech_count), all {} damage fields within {DAMAGE_REL_TOLERANCE:.1}% \
         (including derived life_leech_damage)",
        COUNT_FIELDS.len() + 1,
        DAMAGE_FIELDS.len() + 1
    );
}

#[test]
fn defenses_matches_ei_golden() {
    let golden = read_golden_json();
    check_defenses_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn defenses_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("defenses_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_defenses_matches_ei_golden(&bytes, &golden);
}

/// Real-log sanity check, post-rework era: no committed EI JSON sidecar
/// exists for this local-only fixture (same limitation `hit_stats_golden.
/// rs`'s equivalent postrework check documents), so this only checks
/// structural/internal sanity that the post-era classification path
/// produces plausible, self-consistent output on a real post-rework
/// capture. Also exercises `breakbar_count`/`breakbar_damage` and
/// `dodge_count`, both all-zero on the committed golden fixture.
#[test]
fn defenses_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut any_strike = false;
    for p in &metrics.players {
        let d = &p.defenses;
        // Internal invariant: power == strike + life_leech, always (see
        // module doc's algebraic proof).
        assert_eq!(
            d.power_count,
            d.strike_count + d.life_leech_count,
            "agent {:#x}: power_count must equal strike_count + life_leech_count exactly",
            p.agent_addr
        );
        assert_eq!(
            d.power_damage,
            d.strike_damage + d.life_leech_damage,
            "agent {:#x}: power_damage must equal strike_damage + life_leech_damage exactly",
            p.agent_addr
        );
        if d.strike_count > 0 {
            any_strike = true;
        }
    }
    assert!(any_strike, "a real WvW squad fight should show nonzero incoming strike hits somewhere");

    println!(
        "defenses_present_and_sane_on_local_postrework: {} players, internal invariants hold on a real post-era log",
        metrics.players.len()
    );
}
