//! Calibration test for the WvW friend/foe partition (Task 16A).
//!
//! Runs only when the local (PII, not committed) raw log is present at
//! `fixtures/local/wvw-small.zevtc`. Golden aggregates to reproduce come
//! from Elite Insights, committed at `fixtures/wvw-small.ei.json`:
//!   durationMS = 49285, friendlyPlayerCount = 41, squadTotalDamage = 2138414
//!
//! When the fixture is absent (e.g. in CI), this prints a skip message and
//! returns successfully rather than failing the build.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

const FIXTURE_PATH: &str = "../../fixtures/local/wvw-small.zevtc";

const GOLDEN_DURATION_MS: f64 = 49285.0;
const GOLDEN_FRIENDLY_PLAYERS: i64 = 41;
const GOLDEN_SQUAD_DAMAGE: f64 = 2_138_414.0;

const RELATIVE_TOLERANCE: f64 = 0.005; // 0.5%
const FRIENDLY_COUNT_TOLERANCE: i64 = 2; // ±2

#[test]
fn wvw_partition_matches_golden_fixture() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {FIXTURE_PATH} absent");
            return;
        }
    };

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
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
