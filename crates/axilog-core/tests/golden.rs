//! Golden EI parity test (Task 16B).
//!
//! Source log: axibridge testdata/20260117-181030.zevtc (WvW skirmish,
//! Green Alpine Borderlands). The golden JSON at
//! `fixtures/wvw-small.ei.json` is the anonymized dps.report Elite Insights
//! (EI) output for that same log — account names have been replaced with
//! synthetic values, but the aggregate numbers (duration, player counts,
//! damage totals) are untouched, so this test verifies axilog's analysis
//! pipeline reproduces EI's ground truth within tolerance.
//!
//! Runs only when the local (PII, not committed) raw log is present at
//! `fixtures/local/wvw-small.zevtc`. When absent (e.g. in CI), this prints
//! a skip message and returns successfully rather than failing the build.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

const RELATIVE_TOLERANCE: f64 = 0.005; // 0.5%
const FRIENDLY_COUNT_TOLERANCE: i64 = 2; // ±2

/// True if `a` and `b` are within `RELATIVE_TOLERANCE` of each other,
/// relative to `b` (the golden/expected value).
fn rel_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= RELATIVE_TOLERANCE * b.abs().max(1.0)
}

#[test]
fn golden_ei_parity() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run golden parity)"
            );
            return;
        }
    };

    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    let golden: serde_json::Value =
        serde_json::from_str(&golden_str).expect("parse golden EI JSON");

    let golden_duration_ms = golden["durationMS"].as_f64().expect("durationMS");
    let golden_friendly_players = golden["friendlyPlayerCount"].as_i64().expect("friendlyPlayerCount");
    let golden_squad_damage = golden["squadTotalDamage"].as_f64().expect("squadTotalDamage");

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let duration = enc.duration_ms as f64;
    assert!(
        rel_close(duration, golden_duration_ms),
        "duration_ms {duration} not within {RELATIVE_TOLERANCE} relative of golden {golden_duration_ms}"
    );

    let friendly = enc.players.len() as i64;
    assert!(
        (friendly - golden_friendly_players).abs() <= FRIENDLY_COUNT_TOLERANCE,
        "friendly player count {friendly} not within ±{FRIENDLY_COUNT_TOLERANCE} of golden {golden_friendly_players}"
    );

    let squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    let squad_damage = squad_damage as f64;
    assert!(
        rel_close(squad_damage, golden_squad_damage),
        "squad damage {squad_damage} not within {RELATIVE_TOLERANCE} relative of golden {golden_squad_damage}"
    );

    println!(
        "golden parity: duration_ms={duration} (golden {golden_duration_ms}), \
         friendly={friendly} (golden {golden_friendly_players}), \
         squad_damage={squad_damage} (golden {golden_squad_damage})"
    );
}

/// Finding #4: `cc::timeline`'s squad_damage buckets and `downs::apply`'s
/// down_contribution windowed-damage loop must exclude
/// `result::CROWD_CONTROL` rows (which carry CC duration ms, not damage) —
/// exactly like `damage::accumulate` already does. After that fix,
/// `sum(timeline.squad_damage)` should equal `sum(player.damage_total)`
/// on the golden log, since both now use the same damage predicate (and
/// the timeline also folds in the same friendly pet/minion credit that
/// per-player totals get).
#[test]
fn golden_timeline_matches_player_damage_sum() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run golden parity)"
            );
            return;
        }
    };

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let timeline_sum: u64 = metrics.timeline.squad_damage.iter().sum();
    let player_sum: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    assert_eq!(
        timeline_sum, player_sum,
        "sum(timeline.squad_damage)={timeline_sum} != sum(player.damage_total)={player_sum}"
    );
    println!("golden timeline/player damage sum equality: {timeline_sum}");
}
