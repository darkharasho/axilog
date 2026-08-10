//! M14 Task 2: best-effort `skillMap` (`analysis::skill_map`) structural
//! checks against the committed fixture, plus an HONEST spot-check against a
//! real dps.report export's OWN `skillMap` when the gitignored local
//! post-rework fixture pair is present.
//!
//! Per `analysis::skill_map`'s module doc, this is NOT a calibration
//! target for `name`/`is_swap` the way every other `*_golden.rs` file in
//! this suite calibrates a numeric metric exactly (or within a documented
//! tolerance) against EI: `name` comes from a genuinely different data
//! source (this log's own skill table vs EI's bundled/API-backed skill DB),
//! and `is_swap` is a deliberately NARROWER check than EI's own -- as of
//! M14 Task 3 it covers the `WeaponSwap` sentinel PLUS 3 curated
//! non-sentinel categories (elementalist attunement swaps, revenant legend
//! swaps, necromancer shroud transforms), but still excludes Weaver's own
//! separate combo-attunement table (see that module's "Extended
//! non-sentinel `is_swap` ids" doc section). `can_crit` alone (reused
//! verbatim from M13's already-calibrated `NonCritableSkills` table) is
//! asserted EXACT on every overlapping id.

mod common;

use axilog_core::analysis::analyze;
use axilog_core::analysis::buffs::BOON_IDS;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use common::{read_bytes_or_skip, read_json_or_skip};
use std::collections::BTreeSet;

const FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
fn local_postrework_zevtc() -> String {
    common::local_fixture("wvw-postrework.zevtc")
}
fn local_postrework_ei_json() -> String {
    common::local_fixture("wvw-postrework.ei.json")
}

/// Structural calibration against the committed, PII-safe fixture (CI-
/// gating): every id in `Metrics::skill_map` really is REFERENCED by some
/// squad player's `skill_damage`/`rotation`, or is one of the 12 always-
/// tracked boon ids (the "only-referenced-skills scoping" requirement) --
/// and the map is a small fraction of the log's own full skill table, not a
/// dump of it. Also checks the `name` fallback rule end-to-end against real
/// decoded data: every entry's name is non-empty, and every entry whose id
/// has NO row (or an empty/purely-numeric row) in `raw.skills` gets the
/// `"Skill <id>"` fallback, while every entry whose id DOES have a real
/// (non-numeric, non-empty) row gets that row's trimmed name verbatim.
#[test]
fn skill_map_scoped_to_referenced_ids_on_committed_fixture() {
    let bytes = std::fs::read(FIXTURE_PATH).unwrap_or_else(|e| panic!("read committed fixture {FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    assert!(!metrics.skill_map.is_empty(), "a real WvW fight must reference at least one skill id");
    assert!(
        metrics.skill_map.len() < raw.skills.len(),
        "skill_map ({} entries) must be a strict subset of the log's full skill table ({} entries), \
         not a dump of it",
        metrics.skill_map.len(),
        raw.skills.len()
    );

    // Rebuild the referenced-id set independently (from the SAME already-
    // analyzed `metrics.players`, not `skill_map` itself) to check scoping
    // both ways: every key in `skill_map` must be explainable, and nothing
    // referenced is missing.
    let mut referenced: BTreeSet<u32> = BTreeSet::new();
    for p in &metrics.players {
        for e in &p.skill_damage.outgoing {
            referenced.insert(e.skill_id);
        }
        for e in &p.skill_damage.taken {
            referenced.insert(e.skill_id);
        }
        for t in &p.skill_damage.per_target {
            for e in &t.skills {
                referenced.insert(e.skill_id);
            }
        }
        for r in &p.rotation {
            referenced.insert(r.skill_id);
        }
    }
    for &(id, _, _) in BOON_IDS.iter() {
        referenced.insert(id);
    }
    let map_ids: BTreeSet<u32> = metrics.skill_map.keys().copied().collect();
    assert_eq!(map_ids, referenced, "skill_map's key set must be EXACTLY the referenced-id set, no more, no less");
    assert!(referenced.len() >= 30, "a real ~6-minute WvW squad fight should reference well over a handful of skills, got {}", referenced.len());

    let skill_names: std::collections::BTreeMap<u32, &str> =
        raw.skills.iter().map(|s| (s.id, s.name.as_str())).collect();
    let mut fallback_count = 0usize;
    let mut real_name_count = 0usize;
    for (&id, entry) in &metrics.skill_map {
        assert!(!entry.name.is_empty(), "skill {id}: name must never be empty");
        let raw_name = skill_names.get(&id).map(|s| s.trim()).unwrap_or("");
        let numeric_or_empty = raw_name.is_empty() || raw_name.chars().all(|c| c.is_ascii_digit());
        if numeric_or_empty {
            assert_eq!(entry.name, format!("Skill {id}"), "skill {id}: expected fallback name for empty/numeric/absent log-table row (raw: {raw_name:?})");
            fallback_count += 1;
        } else {
            assert_eq!(entry.name, raw_name, "skill {id}: expected the log table's own trimmed name verbatim");
            real_name_count += 1;
        }
        // `can_crit`/`is_swap` are just booleans on real data here --
        // sanity, not calibration (the real calibration-adjacent claims are
        // covered by the unit tests in `analysis::skill_map` plus the
        // local spot-check below).
        let _ = entry.can_crit;
        let _ = entry.is_swap;
        assert_eq!(entry.auto_attack, None, "skill {id}: auto_attack must always be omitted (None) -- see analysis::skill_map's module doc");
    }

    println!(
        "skill_map_scoped_to_referenced_ids: {} referenced ids ({} real log-table names, {} \"Skill <id>\" \
         fallbacks) out of {} total log-table entries",
        metrics.skill_map.len(), real_name_count, fallback_count, raw.skills.len()
    );
}

/// HONEST spot-check against a REAL dps.report export's own `skillMap`
/// (`fixtures/local/wvw-postrework.ei.json`, gitignored, present locally --
/// skip-gracefully-in-CI, same pattern as every other `fixtures/local/*`-
/// gated test in this suite). Per `analysis::skill_map`'s module doc:
/// - `can_crit` (M13's already-calibrated `NonCritableSkills` table): must
///   match EI's `canCrit` EXACTLY on every overlapping id -- a real
///   assertion, not just a print.
/// - `is_swap`: divergences are EXPECTED (this module's narrower check --
///   `WeaponSwap` sentinel + the 3 curated non-sentinel categories -- still
///   excludes Weaver's separate combo-attunement table, which real EI's
///   own `isSwap` includes) -- counted and printed, not asserted.
/// - `name`: divergences are EXPECTED (different data sources) -- up to 10
///   real examples printed side-by-side, not asserted.
#[test]
fn skill_map_spot_check_against_real_ei_skillmap_when_available() {
    let Some(bytes) = read_bytes_or_skip(&local_postrework_zevtc(), "skill_map spot-check") else { return };
    let Some(golden) = read_json_or_skip(&local_postrework_ei_json(), "skill_map spot-check") else { return };

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let golden_skill_map = golden.get("skillMap").and_then(|v| v.as_object());
    let Some(golden_skill_map) = golden_skill_map else {
        println!("skip: {} has no top-level skillMap object (skill_map spot-check)", local_postrework_ei_json());
        return;
    };

    let mut overlap = 0usize;
    let mut can_crit_mismatches: Vec<String> = Vec::new();
    let mut is_swap_divergences: Vec<String> = Vec::new();
    let mut name_divergences: Vec<(u32, String, String)> = Vec::new();
    let mut name_matches = 0usize;

    for (&id, entry) in &metrics.skill_map {
        let Some(g) = golden_skill_map.get(&format!("s{id}")) else { continue };
        overlap += 1;

        if let Some(g_can_crit) = g.get("canCrit").and_then(|v| v.as_bool()) {
            if g_can_crit != entry.can_crit {
                can_crit_mismatches.push(format!("skill {id} ({}): ours={} golden={g_can_crit}", entry.name, entry.can_crit));
            }
        }

        if let Some(g_is_swap) = g.get("isSwap").and_then(|v| v.as_bool()) {
            if g_is_swap != entry.is_swap {
                is_swap_divergences.push(format!("skill {id} ({}): ours={} golden={g_is_swap}", entry.name, entry.is_swap));
            }
        }

        if let Some(g_name) = g.get("name").and_then(|v| v.as_str()) {
            if g_name == entry.name {
                name_matches += 1;
            } else {
                name_divergences.push((id, entry.name.clone(), g_name.to_string()));
            }
        }
    }

    assert!(overlap >= 30, "expected at least 30 overlapping skill ids between this project's skill_map and the real EI skillMap, got {overlap}");

    // `can_crit` is a REAL calibration claim (reused verbatim from M13's
    // already-calibrated table) -- must be exact on every overlapping id.
    assert!(
        can_crit_mismatches.is_empty(),
        "{} can_crit mismatch(es) against the real EI skillMap (checked {overlap} overlapping ids):\n{}",
        can_crit_mismatches.len(),
        can_crit_mismatches.join("\n")
    );

    println!(
        "skill_map_spot_check: {overlap} overlapping ids, 0 can_crit mismatches (asserted exact), \
         {} name matches / {} name divergences (NOT asserted -- different data sources), \
         {} is_swap divergences (NOT asserted -- this module's narrower check still excludes \
         Weaver's separate combo-attunement table, see analysis::skill_map's module doc)",
        name_matches, name_divergences.len(), is_swap_divergences.len()
    );
    if !is_swap_divergences.is_empty() {
        println!("  is_swap divergence examples (up to 10):");
        for line in is_swap_divergences.iter().take(10) {
            println!("    {line}");
        }
    }
    if !name_divergences.is_empty() {
        println!("  name divergence examples, ours vs EI's (up to 10):");
        for (id, ours, golden) in name_divergences.iter().take(10) {
            println!("    skill {id}: ours={ours:?} golden={golden:?}");
        }
    }
}
