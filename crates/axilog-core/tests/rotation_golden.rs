//! M14 Task 1: per-player rotation (cast tracking) calibration against EI's
//! `rotation[]` (`analysis::rotation`).
//!
//! Same real-account join method as `skill_damage_golden.rs`/
//! `hit_stats_golden.rs`: raw agent-table index -> `anon_account(i)` ->
//! golden `account`. `fixtures/wvw-small.ei.json`'s per-player `rotation`
//! array (see that fixture's `_note`, M14 Task 1 and its ADDENDUM) holds
//! EI's real `rotation[]` entries from `axibridge/test-fixtures/boon/
//! 20260117-181030.json` VERBATIM -- all 1,732 of them, no longer filtered
//! to the animated subset, since `analysis::rotation` now reproduces
//! GW2EI's whole `InitCastEvents` merge. 37 of 41 fixture players have at
//! least one animated cast.
//!
//! ## Tolerance, per cast FAMILY
//!
//! The merged list holds three families, told apart per cast by
//! `duration > 1` (animated) and then by the `-2` pseudo id (weapon swap)
//! -- the discriminator `analysis::rotation`'s module doc grounds in
//! `CombatEventFactory.CreateCastEvents`'s `ActualDuration <= 1` drop plus
//! `InstantCastEvent`/`WeaponSwapEvent`'s ctors. They are held to
//! different standards, because this project reproduces them to different
//! depths:
//!
//! - **Animated** -- the start/end pairing state machine, fully ported.
//!   EXACT, as it has been since M14: per-player count, per-skill-id set,
//!   and every per-cast field within the tolerances below.
//! - **Weapon swaps** -- a `CBTS_WEAPSWAP` row IS the event, with no
//!   heuristics in between, so the only thing on trial is the merge and
//!   its `ServerDelayConstant` dedup. Per-player count **EXACT** (134 for
//!   134 on the committed fixture).
//! - **Instant casts** -- `analysis::instant_cast`'s finder catalog, which
//!   documents its own coverage gaps (effect-keyed finders, the
//!   `UsingNoAnimatedCastChecker` family, and GW2EI's non-finder cast
//!   sources: `SpecialCastEventProcess`, `ProfHelper.
//!   ComputeEndWithBuffApplyCastEvents`, the Engineer toolbelt helpers).
//!   BOUNDED, not exact: [`INSTANT_PER_PLAYER_ABS_TOLERANCE`] per player
//!   and [`INSTANT_TOTAL_RECOVERY_FLOOR`]..[`INSTANT_TOTAL_RECOVERY_CEILING`]
//!   on the squad total. Asserting exactness here would mean asserting
//!   that catalog is complete, which it is not and does not claim to be.
//!
//! - Per-player animated cast COUNT (`total_casts`): **EXACT**.
//! - Per-skill-id cast SET (which animated skill ids appear at all): **EXACT**.
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

mod common;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
fn local_fixture_path() -> String {
    common::local_fixture("wvw-small.zevtc")
}
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
fn local_postrework_zevtc() -> String {
    common::local_fixture("wvw-postrework.zevtc")
}
fn local_postrework_ei_json() -> String {
    common::local_fixture("wvw-postrework.ei.json")
}

/// Measured max residual on the committed fixture is 0 (every field matches
/// exactly); this small headroom is kept for the documented GW2EI
/// ties-to-even vs this module's ties-away-from-zero rounding divergence
/// (`analysis::rotation`'s module doc) rather than asserting a brittle "0
/// tolerance" that would break the moment a future real capture happens to
/// hit that boundary.
const TIME_FIELD_ABS_TOLERANCE_MS: i64 = 1;
/// Same rounding-mode headroom, at `quickness`'s 3-decimal scale.
const QUICKNESS_ABS_TOLERANCE: f64 = 0.001;

/// Largest per-player instant-cast count difference tolerated. Measured
/// worst case on the committed fixture is 5 (in both directions); this is
/// headroom, and its job is to catch a REGIME change -- a finder family
/// silently going dark, or one over-firing on every event -- not to
/// certify the count. See this file's module doc for why this family is
/// bounded rather than exact.
const INSTANT_PER_PLAYER_ABS_TOLERANCE: usize = 8;
/// Squad-total instant casts recovered, as a fraction of the golden's.
/// Measured 339/364 = 0.931 on the committed fixture.
const INSTANT_TOTAL_RECOVERY_FLOOR: f64 = 0.90;
/// The same ratio's upper bound -- an over-firing finder is as much a
/// regression as a missing one, and the ext-healing double-count this
/// bound was written after (`instant_cast`'s `SanitizeForSrc` port) would
/// have shown up here as ~1.04.
const INSTANT_TOTAL_RECOVERY_CEILING: f64 = 1.02;

fn read_anon_fixture() -> Vec<u8> {
    std::fs::read(ANON_FIXTURE_PATH).unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"))
}

fn read_local_fixture_or_skip(test_name: &str) -> Option<Vec<u8>> {
    match std::fs::read(local_fixture_path()) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {} absent ({test_name} local-only extra check)", local_fixture_path());
            None
        }
    }
}

fn read_golden_json() -> serde_json::Value {
    let s = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&s).expect("parse golden EI JSON")
}

/// GW2EI's pseudo skill id for a weapon swap (`SkillIDs.WeaponSwap`), in
/// the SIGNED space an EI export writes it in.
const WEAPON_SWAP_ID: i64 = -2;

/// The three cast families `InitCastEvents` merges, told apart by the
/// per-cast discriminator `analysis::rotation`'s module doc grounds in
/// GW2EI source: `duration > 1` can only be an `AnimatedCastEvent`,
/// `duration <= 1` can only be an `InstantCastEvent`/`WeaponSwapEvent`,
/// and the two of THOSE are told apart by the pseudo id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Family {
    Animated,
    Swap,
    Instant,
}

fn family(skill_id: i64, duration_ms: i64) -> Family {
    if duration_ms > 1 {
        Family::Animated
    } else if skill_id == WEAPON_SWAP_ID {
        Family::Swap
    } else {
        Family::Instant
    }
}

/// One side's rotation, split by [`Family`]: family -> skill id -> ordered
/// `(castTime, duration, timeGained, quickness)` list.
type RotationByFamily = HashMap<Family, HashMap<i64, Vec<(i64, i64, i64, f64)>>>;

fn insert_cast(map: &mut RotationByFamily, id: i64, cast: (i64, i64, i64, f64)) {
    map.entry(family(id, cast.1)).or_default().entry(id).or_default().push(cast);
}

/// `golden["rotation"]`, verbatim from the source EI export (NOT filtered
/// any more -- see this file's module doc and the golden's own `_note`).
fn golden_rotation_map(golden_p: &serde_json::Value) -> RotationByFamily {
    let mut map = RotationByFamily::new();
    let Some(rotation) = golden_p.get("rotation").and_then(|v| v.as_array()) else { return map };
    for grp in rotation {
        let id = grp["id"].as_i64().expect("id");
        for s in grp["skills"].as_array().expect("skills array") {
            insert_cast(&mut map, id, (
                s["castTime"].as_i64().expect("castTime"),
                s["duration"].as_i64().expect("duration"),
                s["timeGained"].as_i64().expect("timeGained"),
                s["quickness"].as_f64().expect("quickness"),
            ));
        }
    }
    map
}

fn our_rotation_map(rotation: &axilog_core::analysis::rotation::RotationMetrics) -> RotationByFamily {
    let mut map = RotationByFamily::new();
    for s in rotation {
        // `as i32`: this project carries EI's negative pseudo ids as their
        // `u32` bit pattern -- same cast the ei-json adapter's
        // `ei_skill_id` makes on the way out.
        let id = i64::from(s.skill_id as i32);
        for c in &s.casts {
            insert_cast(&mut map, id, (c.cast_time_ms, c.duration_ms, c.time_gained_ms, c.quickness));
        }
    }
    map
}

fn family_total(map: &RotationByFamily, f: Family) -> usize {
    map.get(&f).map_or(0, |m| m.values().map(Vec::len).sum())
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
    let mut swap_mismatches: Vec<(String, usize, usize)> = Vec::new();
    let mut instant_over_budget: Vec<(String, usize, usize)> = Vec::new();
    let mut instant_ours = 0usize;
    let mut instant_golden = 0usize;
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

        let golden_by_family = golden_rotation_map(golden_p);
        let our_by_family = our_rotation_map(&pm.rotation);
        let empty = HashMap::new();
        let golden_map = golden_by_family.get(&Family::Animated).unwrap_or(&empty);
        let our_map = our_by_family.get(&Family::Animated).unwrap_or(&empty);

        let golden_total: usize = golden_map.values().map(|v| v.len()).sum();
        let our_total: usize = our_map.values().map(|v| v.len()).sum();
        if golden_total != our_total {
            count_mismatches.push((key.clone(), our_total, golden_total));
        }
        if golden_total > 0 {
            players_with_casts += 1;
        }

        // -- weapon swaps: EXACT, same as the animated family. This one is
        // fully ours to get right -- a `CBTS_WEAPSWAP` row IS the event,
        // with no finder heuristics in between -- so the only thing being
        // calibrated is the `InitCastEvents` merge and its
        // `ServerDelayConstant` dedup.
        let g_swaps = family_total(&golden_by_family, Family::Swap);
        let o_swaps = family_total(&our_by_family, Family::Swap);
        if g_swaps != o_swaps {
            swap_mismatches.push((key.clone(), o_swaps, g_swaps));
        }

        // -- instant casts: BOUNDED, not exact. See this file's module doc.
        let g_inst = family_total(&golden_by_family, Family::Instant);
        let o_inst = family_total(&our_by_family, Family::Instant);
        instant_ours += o_inst;
        instant_golden += g_inst;
        if o_inst.abs_diff(g_inst) > INSTANT_PER_PLAYER_ABS_TOLERANCE {
            instant_over_budget.push((key.clone(), o_inst, g_inst));
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

        for (id, gcasts) in golden_map {
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
    if !swap_mismatches.is_empty() {
        let report: Vec<String> = swap_mismatches
            .iter()
            .map(|(a, o, g)| format!("{a}: ours={o} golden={g}"))
            .collect();
        panic!(
            "{} account(s) with a weapon-swap COUNT mismatch (checked {joined} accounts):\n{}",
            swap_mismatches.len(),
            report.join("\n")
        );
    }
    if !instant_over_budget.is_empty() {
        let report: Vec<String> = instant_over_budget
            .iter()
            .map(|(a, o, g)| format!("{a}: ours={o} golden={g}"))
            .collect();
        panic!(
            "{} account(s) whose instant-cast count is more than \
             {INSTANT_PER_PLAYER_ABS_TOLERANCE} off the golden (checked {joined} accounts):\n{}",
            instant_over_budget.len(),
            report.join("\n")
        );
    }
    let instant_recovery = instant_ours as f64 / instant_golden.max(1) as f64;
    assert!(
        (INSTANT_TOTAL_RECOVERY_FLOOR..=INSTANT_TOTAL_RECOVERY_CEILING).contains(&instant_recovery),
        "squad-total instant-cast recovery {instant_recovery:.4} (ours {instant_ours}, golden \
         {instant_golden}) is outside [{INSTANT_TOTAL_RECOVERY_FLOOR}, \
         {INSTANT_TOTAL_RECOVERY_CEILING}] -- see this file's module doc"
    );
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
         quickness delta {max_quickness_delta:.6} (tolerance {QUICKNESS_ABS_TOLERANCE}); \
         weapon swaps EXACT for all {joined}; instant casts {instant_ours}/{instant_golden} \
         ({:.1}% recovered)",
        instant_recovery * 100.0
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
    let Some(bytes) = std::fs::read(local_postrework_zevtc()).ok() else {
        println!("skip: {} absent (local-only postrework sanity check)", local_postrework_zevtc());
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
    let Some(bytes) = std::fs::read(local_postrework_zevtc()).ok() else {
        println!("skip: {} absent (post-era rotation real calibration)", local_postrework_zevtc());
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(local_postrework_ei_json()).ok() else {
        println!("skip: {} absent (post-era rotation real calibration)", local_postrework_ei_json());
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {}: {e}", local_postrework_ei_json()));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let golden_players = golden["players"].as_array().expect("players array");
    // This local check stays scoped to the ANIMATED family alone (the one
    // held exact -- see this file's module doc's per-family tolerance
    // table), on both sides. The instant/swap families are calibrated
    // against the committed golden above rather than duplicated here.

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
                    if family(id, duration) != Family::Animated {
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
        let our_by_family = our_rotation_map(&pm.rotation);
        let empty = HashMap::new();
        let our_map = our_by_family.get(&Family::Animated).unwrap_or(&empty);

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
            "skip: 0 accounts joined between {} and {} \
             (post-era rotation real calibration) -- account-string mismatch, or the export has no \
             `rotation` block for any joined player",
            local_postrework_zevtc(),
            local_postrework_ei_json()
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

/// MSMALL item 3: `statsAll[0].{saved,timeSaved,wasted,timeWasted}` --
/// GW2EI's `GameplayStatistics` aftercast/interrupt counters -- against the
/// local post-rework capture's own EI export, EXACTLY.
///
/// These are the four numbers `analysis::rotation::aftercast_stats` derives
/// (see its and `AftercastStats`' doc comments for the
/// `GameplayStatistics.cs:81-99` transcription). They are asserted EXACT,
/// not within a tolerance, because they measured exact for all 44 players
/// once two rules were applied:
///
/// 1. the `GetCastEvents(log, start, end)` window filter (`x.Time >= start`),
///    which excludes this module's backdated pre-log casts -- without it
///    `saved` was `ei + 1` for 11 of 44 players;
/// 2. ties-to-EVEN rounding for `scaledExpectedDuration` (`round_ties_even`,
///    matching .NET `Math.Round`'s documented default) -- without it
///    `timeSaved` was 1ms high for 2 of 44.
///
/// An exact assert is what makes this test able to catch a regression in
/// either rule; a tolerance would have hidden both of the defects it was
/// written to pin down.
#[test]
fn aftercast_stats_match_local_postrework_ei_json() {
    let Some(bytes) = std::fs::read(local_postrework_zevtc()).ok() else {
        println!("skip: {} absent (aftercast calibration)", local_postrework_zevtc());
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(local_postrework_ei_json()).ok() else {
        println!("skip: {} absent (aftercast calibration)", local_postrework_ei_json());
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_s).expect("parse EI json");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden["players"].as_array().expect("players array") {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(a.trim_start_matches(':').to_string(), &p["statsAll"][0]);
        }
    }

    /// EI emits the two durations as seconds with 3 decimals
    /// (`Math.Round(ms / 1000.0, TimeDigit)`); compare on that scale.
    fn secs3(ms: i64) -> f64 {
        (ms as f64 / 1000.0 * 1000.0).round() / 1000.0
    }

    let mut joined = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for p in &enc.players {
        let Some(g) = golden_by_account.get(p.account.trim_start_matches(':')) else { continue };
        let Some(m) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        // `statsAll[0]` only carries these on real recorded players; a
        // golden entry missing them entirely is not a mismatch to report.
        let (Some(g_saved), Some(g_wasted)) = (g["saved"].as_i64(), g["wasted"].as_i64()) else {
            continue;
        };
        joined += 1;
        let a = axilog_core::analysis::rotation::aftercast_stats(&m.rotation);
        let g_time_saved = g["timeSaved"].as_f64().expect("timeSaved");
        let g_time_wasted = g["timeWasted"].as_f64().expect("timeWasted");
        for (field, ours, theirs) in [
            ("saved", a.saved_count as f64, g_saved as f64),
            ("wasted", a.wasted_count as f64, g_wasted as f64),
            ("timeSaved", secs3(a.saved_ms), g_time_saved),
            ("timeWasted", secs3(a.wasted_ms), g_time_wasted),
        ] {
            if (ours - theirs).abs() > 1e-9 {
                mismatches.push(format!("  {} {field}: ours={ours} ei={theirs}", p.account));
            }
        }
    }

    assert!(joined >= 30, "expected >=30 accounts to join for the aftercast check, got {joined}");
    assert!(
        mismatches.is_empty(),
        "{} aftercast field(s) diverge from EI across {joined} players:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    println!(
        "aftercast_stats_match_local_postrework_ei_json: {joined} players, all 4 fields \
         (saved/timeSaved/wasted/timeWasted) EXACT"
    );
}
