//! M14 Task 1: per-player rotation (cast tracking) calibration against EI's
//! `rotation[]` (`analysis::rotation`).
//!
//! Same real-account join method as `skill_damage_golden.rs`/
//! `hit_stats_golden.rs`: raw agent-table index -> `anon_account(i)` ->
//! golden `account`. `fixtures/wvw-small.ei.json`'s per-player `rotation`
//! array (see that fixture's `_note`, M14 Task 1 addendum) holds EI's real
//! `rotation[]` entries extracted from `axibridge/test-fixtures/boon/
//! 20260117-181030.json`, FILTERED to the `AnimatedCastEvent`-pipeline
//! subset this module actually computes -- a PER-CAST discriminator,
//! `id >= 0 && duration > 1` (verified against `CombatEventFactory.
//! CreateCastEvents`'s own `ActualDuration <= 1` drop plus
//! `InstantCastEvent`'s ctor, which always hardcodes `ActualDuration = 0`:
//! any surviving `duration > 1` entry MUST be a real `AnimatedCastEvent`;
//! any `duration <= 1` entry MUST be an `InstantCastEvent`/`WeaponSwapEvent`
//! -- NOT the coarser per-skill-id `skillMap[id].isInstantCast` flag, which
//! a real post-rework capture proved can be `true` for a skill (e.g. a
//! signet) that ALSO has genuine animated casts under the same id -- see
//! the golden fixture's own `_note` for the full empirical writeup) -- see
//! `analysis::rotation`'s module doc for the "why filtered, not the full EI
//! rotation[]" writeup. 37 of 41 fixture players have at least one animated
//! cast.
//!
//! ## Tolerance
//!
//! - Per-player animated cast COUNT (`total_casts`): **EXACT**.
//! - Per-skill-id cast SET (which skill ids appear at all): **EXACT**.
//! - Per-cast `castTime`/`duration`/`timeGained` (all integer ms fields):
//!   within `TIME_FIELD_ABS_TOLERANCE_MS` -- documented headroom for
//!   GW2EI's own cast-boundary/rounding quirks (`SetAcceleration`'s
//!   `Math.Round` ties-to-even vs this module's ties-away-from-zero, see
//!   `analysis::rotation`'s module doc), though in practice every one of
//!   this fixture's 1,222 animated casts matches EXACTLY (0 measured
//!   residual) -- see this file's test output for the measured max.
//! - Per-cast `quickness` (float, [-1,1]): within `QUICKNESS_ABS_TOLERANCE`
//!   (same ties-to-even-vs-away-from-zero rounding headroom, at the 3rd
//!   decimal).

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
const LOCAL_POSTREWORK_EI_JSON: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-postrework.ei.json");

/// Measured max residual on the committed fixture is 0 (every field matches
/// exactly); this small headroom is kept for the documented GW2EI
/// ties-to-even vs this module's ties-away-from-zero rounding divergence
/// (`analysis::rotation`'s module doc) rather than asserting a brittle "0
/// tolerance" that would break the moment a future real capture happens to
/// hit that boundary.
const TIME_FIELD_ABS_TOLERANCE_MS: i64 = 1;
/// Same rounding-mode headroom, at `quickness`'s 3-decimal scale.
const QUICKNESS_ABS_TOLERANCE: f64 = 0.001;

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

/// `golden["rotation"]` (already animated-only filtered, see module doc) ->
/// skill id -> ordered `(castTime, duration, timeGained, quickness)` list.
fn golden_rotation_map(golden_p: &serde_json::Value) -> HashMap<i64, Vec<(i64, i64, i64, f64)>> {
    let mut map = HashMap::new();
    let Some(rotation) = golden_p.get("rotation").and_then(|v| v.as_array()) else { return map };
    for grp in rotation {
        let id = grp["id"].as_i64().expect("id");
        let skills: Vec<(i64, i64, i64, f64)> = grp["skills"]
            .as_array()
            .expect("skills array")
            .iter()
            .map(|s| {
                (
                    s["castTime"].as_i64().expect("castTime"),
                    s["duration"].as_i64().expect("duration"),
                    s["timeGained"].as_i64().expect("timeGained"),
                    s["quickness"].as_f64().expect("quickness"),
                )
            })
            .collect();
        map.insert(id, skills);
    }
    map
}

fn our_rotation_map(rotation: &axilog_core::analysis::rotation::RotationMetrics) -> HashMap<i64, Vec<(i64, i64, i64, f64)>> {
    rotation
        .iter()
        .map(|s| {
            let casts = s
                .casts
                .iter()
                .map(|c| (c.cast_time_ms, c.duration_ms, c.time_gained_ms, c.quickness))
                .collect();
            (s.skill_id as i64, casts)
        })
        .collect()
}

struct FieldMismatch {
    account: String,
    skill_id: i64,
    index: usize,
    field: &'static str,
    ours: f64,
    golden: f64,
}

fn check_rotation_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
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
    let mut players_with_casts = 0usize;
    let mut count_mismatches: Vec<(String, usize, usize)> = Vec::new();
    let mut skill_set_mismatches: Vec<(String, Vec<i64>, Vec<i64>)> = Vec::new();
    let mut field_mismatches: Vec<FieldMismatch> = Vec::new();
    let mut max_time_delta: i64 = 0;
    let mut max_quickness_delta: f64 = 0.0;
    let mut total_casts_checked = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        // Only join accounts the fixture actually extracted a `rotation`
        // block for (every one of the 41 committed rows has the KEY, but
        // may be an empty array for the 4 `Non Squad Player N` rows).
        if golden_p.get("rotation").is_none() {
            continue;
        }
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        let golden_map = golden_rotation_map(golden_p);
        let our_map = our_rotation_map(&pm.rotation);

        let golden_total: usize = golden_map.values().map(|v| v.len()).sum();
        let our_total: usize = our_map.values().map(|v| v.len()).sum();
        if golden_total != our_total {
            count_mismatches.push((key.clone(), our_total, golden_total));
        }
        if golden_total > 0 {
            players_with_casts += 1;
        }

        let mut golden_ids: Vec<i64> = golden_map.keys().copied().collect();
        let mut our_ids: Vec<i64> = our_map.keys().copied().collect();
        golden_ids.sort_unstable();
        our_ids.sort_unstable();
        if golden_ids != our_ids {
            skill_set_mismatches.push((key.clone(), our_ids, golden_ids));
            // Still compare whatever skill ids ARE shared, below, so a
            // partial mismatch doesn't hide field-level info.
        }

        for (id, gcasts) in &golden_map {
            let Some(ocasts) = our_map.get(id) else { continue };
            if ocasts.len() != gcasts.len() {
                continue; // already captured by skill_set/count mismatches above
            }
            for (idx, (o, g)) in ocasts.iter().zip(gcasts.iter()).enumerate() {
                total_casts_checked += 1;
                let (o_time, o_dur, o_gained, o_q) = *o;
                let (g_time, g_dur, g_gained, g_q) = *g;

                let dt = (o_time - g_time).abs();
                max_time_delta = max_time_delta.max(dt);
                if dt > TIME_FIELD_ABS_TOLERANCE_MS {
                    field_mismatches.push(FieldMismatch { account: key.clone(), skill_id: *id, index: idx, field: "castTime", ours: o_time as f64, golden: g_time as f64 });
                }
                let dd = (o_dur - g_dur).abs();
                max_time_delta = max_time_delta.max(dd);
                if dd > TIME_FIELD_ABS_TOLERANCE_MS {
                    field_mismatches.push(FieldMismatch { account: key.clone(), skill_id: *id, index: idx, field: "duration", ours: o_dur as f64, golden: g_dur as f64 });
                }
                let dg = (o_gained - g_gained).abs();
                max_time_delta = max_time_delta.max(dg);
                if dg > TIME_FIELD_ABS_TOLERANCE_MS {
                    field_mismatches.push(FieldMismatch { account: key.clone(), skill_id: *id, index: idx, field: "timeGained", ours: o_gained as f64, golden: g_gained as f64 });
                }
                let dq = (o_q - g_q).abs();
                max_quickness_delta = max_quickness_delta.max(dq);
                if dq > QUICKNESS_ABS_TOLERANCE {
                    field_mismatches.push(FieldMismatch { account: key.clone(), skill_id: *id, index: idx, field: "quickness", ours: o_q, golden: g_q });
                }
            }
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the rotation-augmented golden fixture, got {joined}"
    );
    assert!(
        players_with_casts >= 30,
        "expected at least 30 accounts to have at least one animated cast, got {players_with_casts}"
    );

    if !count_mismatches.is_empty() {
        let report: Vec<String> = count_mismatches
            .iter()
            .map(|(a, o, g)| format!("{a}: ours={o} golden={g}"))
            .collect();
        panic!(
            "{} account(s) with a total animated-cast COUNT mismatch (checked {joined} accounts):\n{}",
            count_mismatches.len(),
            report.join("\n")
        );
    }
    if !skill_set_mismatches.is_empty() {
        let report: Vec<String> = skill_set_mismatches
            .iter()
            .map(|(a, ours, golden)| format!("{a}: ours={ours:?} golden={golden:?}"))
            .collect();
        panic!(
            "{} account(s) with a different set of animated skill ids (checked {joined} accounts):\n{}",
            skill_set_mismatches.len(),
            report.join("\n")
        );
    }
    if !field_mismatches.is_empty() {
        let report: Vec<String> = field_mismatches
            .iter()
            .map(|m| format!("{} skill {} cast[{}] {}: ours={} golden={}", m.account, m.skill_id, m.index, m.field, m.ours, m.golden))
            .collect();
        panic!(
            "{} per-cast field mismatch(es) exceeding tolerance (checked {total_casts_checked} casts):\n{}",
            field_mismatches.len(),
            report.join("\n")
        );
    }

    println!(
        "rotation_matches_ei_golden: {joined} accounts joined ({players_with_casts} with >=1 animated \
         cast), 0 count mismatches, 0 skill-id-set mismatches, {total_casts_checked} casts field-checked, \
         max time-field delta {max_time_delta}ms (tolerance {TIME_FIELD_ABS_TOLERANCE_MS}ms), max \
         quickness delta {max_quickness_delta:.6} (tolerance {QUICKNESS_ABS_TOLERANCE})"
    );
}

#[test]
fn rotation_matches_ei_golden() {
    let golden = read_golden_json();
    check_rotation_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn rotation_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("rotation_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_rotation_matches_ei_golden(&bytes, &golden);
}

/// Real-log sanity check, post-rework era, structural/internal invariants
/// only (no external reference needed) -- runs regardless of whether the
/// `.ei.json` sidecar is present.
#[test]
fn rotation_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut any_casts = false;
    for p in &metrics.players {
        for s in &p.rotation {
            assert!(!s.casts.is_empty(), "agent {:#x}: skill {} rotation group has no casts, should not exist", p.agent_addr, s.skill_id);
            any_casts = true;
            for c in &s.casts {
                assert!(c.duration_ms >= 0, "agent {:#x}: skill {} cast has negative duration {}", p.agent_addr, s.skill_id, c.duration_ms);
                assert!(c.quickness >= -1.0 && c.quickness <= 1.0, "agent {:#x}: skill {} cast quickness {} out of [-1,1]", p.agent_addr, s.skill_id, c.quickness);
            }
        }
    }
    assert!(any_casts, "a real ~6-minute WvW squad fight should show at least one animated cast somewhere");

    println!("rotation_present_and_sane_on_local_postrework: {} players, internal invariants hold on a real post-era log", metrics.players.len());
}

/// REAL calibration of the post-era classification path
/// (`analysis::rotation::build`'s `post_era` branch) against a real
/// post-rework capture's own dps.report export, the moment both are
/// present locally -- mirrors `hit_stats_golden.rs`'s
/// `hit_stats_calibrated_against_local_postrework_ei_json_when_available`.
/// Joined by RAW account string (this fixture is a real, non-anonymized
/// local capture, no `anon_account`-index join to reuse). Skip-when-absent,
/// same as every other `fixtures/local/*`-gated test in this suite -- CI
/// has no local fixture, so this never runs there.
#[test]
fn rotation_calibrated_against_local_postrework_ei_json_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (post-era rotation real calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(LOCAL_POSTREWORK_EI_JSON).ok() else {
        println!("skip: {LOCAL_POSTREWORK_EI_JSON} absent (post-era rotation real calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {LOCAL_POSTREWORK_EI_JSON}: {e}"));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let golden_players = golden["players"].as_array().expect("players array");
    // Per-CAST discriminator (not the coarser per-skill-id `skillMap[id].
    // isInstantCast` flag -- see this file's module doc for the empirical
    // writeup on why that flag is unreliable): `id >= 0 && duration > 1`.
    let is_animated_cast = |id: i64, duration: i64| id >= 0 && duration > 1;

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(a.trim_start_matches(':').to_string(), p);
        }
    }

    let mut joined = 0usize;
    let mut count_mismatches: Vec<String> = Vec::new();
    let mut skill_set_mismatches: Vec<String> = Vec::new();
    let mut field_mismatch_notes: Vec<String> = Vec::new();
    let mut max_time_delta: i64 = 0;
    let mut max_quickness_delta: f64 = 0.0;
    let mut total_casts_checked = 0usize;

    for p in &enc.players {
        let key = p.account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(rotation) = golden_p.get("rotation").and_then(|v| v.as_array()) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        let mut golden_map: HashMap<i64, Vec<(i64, i64, i64, f64)>> = HashMap::new();
        for grp in rotation {
            let id = grp["id"].as_i64().expect("id");
            let skills: Vec<(i64, i64, i64, f64)> = grp["skills"]
                .as_array()
                .expect("skills array")
                .iter()
                .filter_map(|s| {
                    let duration = s["duration"].as_i64().expect("duration");
                    if !is_animated_cast(id, duration) {
                        return None;
                    }
                    Some((
                        s["castTime"].as_i64().expect("castTime"),
                        duration,
                        s["timeGained"].as_i64().expect("timeGained"),
                        s["quickness"].as_f64().expect("quickness"),
                    ))
                })
                .collect();
            if !skills.is_empty() {
                golden_map.insert(id, skills);
            }
        }
        let our_map = our_rotation_map(&pm.rotation);

        let golden_total: usize = golden_map.values().map(|v| v.len()).sum();
        let our_total: usize = our_map.values().map(|v| v.len()).sum();
        if golden_total != our_total {
            count_mismatches.push(format!("{key}: ours={our_total} golden={golden_total}"));
        }

        let mut golden_ids: Vec<i64> = golden_map.keys().copied().collect();
        let mut our_ids: Vec<i64> = our_map.keys().copied().collect();
        golden_ids.sort_unstable();
        our_ids.sort_unstable();
        if golden_ids != our_ids {
            skill_set_mismatches.push(format!("{key}: ours={our_ids:?} golden={golden_ids:?}"));
        }

        for (id, gcasts) in &golden_map {
            let Some(ocasts) = our_map.get(id) else { continue };
            if ocasts.len() != gcasts.len() {
                continue;
            }
            for (idx, (o, g)) in ocasts.iter().zip(gcasts.iter()).enumerate() {
                total_casts_checked += 1;
                let (o_time, o_dur, o_gained, o_q) = *o;
                let (g_time, g_dur, g_gained, g_q) = *g;
                max_time_delta = max_time_delta.max((o_time - g_time).abs()).max((o_dur - g_dur).abs()).max((o_gained - g_gained).abs());
                max_quickness_delta = max_quickness_delta.max((o_q - g_q).abs());
                if (o_time - g_time).abs() > TIME_FIELD_ABS_TOLERANCE_MS
                    || (o_dur - g_dur).abs() > TIME_FIELD_ABS_TOLERANCE_MS
                    || (o_gained - g_gained).abs() > TIME_FIELD_ABS_TOLERANCE_MS
                    || (o_q - g_q).abs() > QUICKNESS_ABS_TOLERANCE
                {
                    field_mismatch_notes.push(format!(
                        "{key} skill {id} cast[{idx}]: ours=({o_time},{o_dur},{o_gained},{o_q}) golden=({g_time},{g_dur},{g_gained},{g_q})"
                    ));
                }
            }
        }
    }

    if joined == 0 {
        println!(
            "skip: 0 accounts joined between {LOCAL_POSTREWORK_ZEVTC} and {LOCAL_POSTREWORK_EI_JSON} \
             (post-era rotation real calibration) -- account-string mismatch, or the export has no \
             `rotation` block for any joined player"
        );
        return;
    }

    assert!(
        count_mismatches.is_empty(),
        "{} account(s) with a total animated-cast COUNT mismatch on a REAL post-era capture \
         (checked {joined} accounts):\n{}",
        count_mismatches.len(),
        count_mismatches.join("\n")
    );
    assert!(
        skill_set_mismatches.is_empty(),
        "{} account(s) with a different set of animated skill ids on a REAL post-era capture:\n{}",
        skill_set_mismatches.len(),
        skill_set_mismatches.join("\n")
    );
    assert!(
        field_mismatch_notes.is_empty(),
        "{} per-cast field mismatch(es) exceeding tolerance on a REAL post-era capture \
         (checked {total_casts_checked} casts):\n{}",
        field_mismatch_notes.len(),
        field_mismatch_notes.join("\n")
    );

    println!(
        "rotation_calibrated_against_local_postrework_ei_json: {joined} accounts joined, 0 count \
         mismatches, 0 skill-id-set mismatches, {total_casts_checked} casts field-checked on a REAL \
         post-era capture, max time-field delta {max_time_delta}ms, max quickness delta \
         {max_quickness_delta:.6}"
    );
}
