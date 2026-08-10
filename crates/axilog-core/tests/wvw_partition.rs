//! Calibration test for the WvW friend/foe partition (Task 16A).
//!
//! Golden aggregates to reproduce come from Elite Insights, committed at
//! `fixtures/wvw-small.ei.json`:
//!   durationMS = 49285, friendlyPlayerCount = 41, squadTotalDamage = 2138414
//!
//! Runs against the committed, PII-safe `fixtures/wvw-small.anon.zevtc`
//! (always present, so this runs in CI too — Task 5, M2). Names don't feed
//! any metric, so the anonymized fixture reproduces the same numbers as the
//! real log. When the real local raw fixture is also present (gitignored,
//! PII, dev-only), it is checked too as a belt-and-braces extra.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

mod common;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
fn local_fixture_path() -> String {
    common::local_fixture("wvw-small.zevtc")
}

const GOLDEN_DURATION_MS: f64 = 49285.0;
const GOLDEN_FRIENDLY_PLAYERS: i64 = 41;
const GOLDEN_SQUAD_DAMAGE: f64 = 2_138_414.0;

const RELATIVE_TOLERANCE: f64 = 0.005; // 0.5%
const FRIENDLY_COUNT_TOLERANCE: i64 = 2; // ±2

fn check_wvw_partition(bytes: &[u8]) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);

    let friendly = enc.players.len() as i64;
    assert!(
        (friendly - GOLDEN_FRIENDLY_PLAYERS).abs() <= FRIENDLY_COUNT_TOLERANCE,
        "friendly player count {friendly} not within ±{FRIENDLY_COUNT_TOLERANCE} of {GOLDEN_FRIENDLY_PLAYERS}"
    );

    let duration = enc.duration_ms as f64;
    let duration_rel_err = (duration - GOLDEN_DURATION_MS).abs() / GOLDEN_DURATION_MS;
    assert!(
        duration_rel_err <= RELATIVE_TOLERANCE,
        "duration_ms {duration} not within {RELATIVE_TOLERANCE} relative of {GOLDEN_DURATION_MS} (rel err {duration_rel_err})"
    );

    let metrics = analyze(&enc, &raw);
    let squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    let squad_damage = squad_damage as f64;
    let damage_rel_err = (squad_damage - GOLDEN_SQUAD_DAMAGE).abs() / GOLDEN_SQUAD_DAMAGE;
    assert!(
        damage_rel_err <= RELATIVE_TOLERANCE,
        "squad damage {squad_damage} not within {RELATIVE_TOLERANCE} relative of {GOLDEN_SQUAD_DAMAGE} (rel err {damage_rel_err})"
    );

    println!(
        "wvw_partition calibration: friendly={friendly} (golden {GOLDEN_FRIENDLY_PLAYERS}), \
         duration_ms={duration} (golden {GOLDEN_DURATION_MS}), \
         squad_damage={squad_damage} (golden {GOLDEN_SQUAD_DAMAGE})"
    );
}

#[test]
fn wvw_partition_matches_golden_fixture() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    check_wvw_partition(&bytes);
}

#[test]
fn wvw_partition_matches_golden_fixture_local_raw_when_present() {
    let bytes = match std::fs::read(local_fixture_path()) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {} absent (local-only extra check)", local_fixture_path());
            return;
        }
    };
    check_wvw_partition(&bytes);
}
