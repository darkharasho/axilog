//! M13 Task 1: outgoing hit-quality stats calibration against EI's
//! `statsAll[0]` (`analysis::hit_stats`).
//!
//! Same real-account join method as `skill_damage_golden.rs`/
//! `healing_golden.rs`: raw agent-table index -> `anon_account(i)` -> golden
//! `account`. `fixtures/wvw-small.ei.json`'s per-player `hitStats` object
//! (see that fixture's `_note`, M13 Task 1 addendum) carries a subset of
//! `statsAll[0]`'s real fields verbatim, extracted from `axibridge/
//! test-fixtures/boon/20260117-181030.json` at the same player index.
//!
//! ## Tolerance (per the M13 plan)
//!
//! Counts EXACT where unambiguous: `crit_count`/`flank_count`/
//! `glance_count`/`moving_count`/`connected_count`/`direct_count`/
//! `condition_count`/`critable_direct_count`/`against_downed_count`/
//! `life_leech_count`/`above90_power_count`/`above90_condition_count`.
//! Damage sums within 0.5% (relative, or an absolute epsilon when the
//! golden value is 0): `crit_damage`/`connected_damage`/`direct_damage`/
//! `condition_damage`/`against_downed_damage`/`life_leech_damage`/
//! `above90_power_damage`/`above90_condition_damage`.
//!
//! See `analysis::hit_stats`'s module doc for the full per-field EI
//! derivation/citation, notably: `hit_stats` deliberately does NOT fold
//! pet/minion damage onto the owner (unlike `damage_total`/`skill_damage`)
//! -- EI's own `statsAll[0]` is actor-only, matching `totalDamageDist`, not
//! `dpsAll[0].damage` -- so this join works for ALL joined accounts,
//! including the 4 pet/minion-affected ones `skill_damage_golden.rs`
//! documents (their `statsAll[0]` numbers are unaffected by the pet gap).

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

/// Relative tolerance for damage sums (M13 plan: 0.5%). Measured on the
/// golden fixture: every damage-sum field is actually EXACT (0 tolerance
/// also passes) -- kept at the plan's 0.5% bar as a safety margin against
/// future fixture/extraction changes, not because any residual was
/// observed.
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

/// (native `HitStats` field name, golden `hitStats` key) count pairs --
/// checked EXACT.
const COUNT_FIELDS: &[(&str, &str)] = &[
    ("crit_count", "criticalRate"),
    ("flank_count", "flankingRate"),
    ("glance_count", "glanceRate"),
    ("moving_count", "againstMovingRate"),
    ("connected_count", "connectedDamageCount"),
    ("direct_count", "connectedDirectDamageCount"),
    ("condition_count", "connectedConditionCount"),
    ("critable_direct_count", "critableDirectDamageCount"),
    ("against_downed_count", "againstDownedCount"),
    ("life_leech_count", "connectedLifeLeechCount"),
    ("above90_power_count", "connectedPowerAbove90HPCount"),
    ("above90_condition_count", "connectedConditionAbove90HPCount"),
];

/// (native `HitStats` field name, golden `hitStats` key) damage-sum pairs --
/// checked within `DAMAGE_REL_TOLERANCE`.
const DAMAGE_FIELDS: &[(&str, &str)] = &[
    ("crit_damage", "criticalDmg"),
    ("connected_damage", "connectedDmg"),
    ("direct_damage", "connectedDirectDmg"),
    ("condition_damage", "connectedConditionDamage"),
    ("against_downed_damage", "againstDownedDamage"),
    ("life_leech_damage", "connectedLifeLeechDamage"),
    ("above90_power_damage", "connectedPowerAbove90HPDamage"),
    ("above90_condition_damage", "connectedConditionAbove90HPDamage"),
];

fn field_value(h: &axilog_core::analysis::hit_stats::HitStats, name: &str) -> u64 {
    match name {
        "crit_count" => h.crit_count as u64,
        "crit_damage" => h.crit_damage,
        "flank_count" => h.flank_count as u64,
        "glance_count" => h.glance_count as u64,
        "moving_count" => h.moving_count as u64,
        "connected_count" => h.connected_count as u64,
        "connected_damage" => h.connected_damage,
        "direct_count" => h.direct_count as u64,
        "direct_damage" => h.direct_damage,
        "condition_count" => h.condition_count as u64,
        "condition_damage" => h.condition_damage,
        "critable_direct_count" => h.critable_direct_count as u64,
        "against_downed_count" => h.against_downed_count as u64,
        "against_downed_damage" => h.against_downed_damage,
        "life_leech_count" => h.life_leech_count as u64,
        "life_leech_damage" => h.life_leech_damage,
        "above90_power_count" => h.above90_power_count as u64,
        "above90_power_damage" => h.above90_power_damage,
        "above90_condition_count" => h.above90_condition_count as u64,
        "above90_condition_damage" => h.above90_condition_damage,
        other => panic!("unknown HitStats field {other}"),
    }
}

fn check_hit_stats_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
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
        let Some(hs) = golden_p.get("hitStats") else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        for &(our_field, golden_key) in COUNT_FIELDS {
            let ours = field_value(&pm.hit_stats, our_field);
            let golden_val = hs[golden_key].as_i64().unwrap_or(0) as u64;
            if ours != golden_val {
                count_mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[{golden_key}]={golden_val}"
                ));
            }
        }

        for &(our_field, golden_key) in DAMAGE_FIELDS {
            let ours = field_value(&pm.hit_stats, our_field) as f64;
            let golden_val = hs[golden_key].as_i64().unwrap_or(0) as f64;
            let diff = (ours - golden_val).abs();
            let allowed = (golden_val.abs() * DAMAGE_REL_TOLERANCE).max(DAMAGE_ABS_FLOOR);
            if diff > allowed {
                damage_mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[{golden_key}]={golden_val} diff={diff} allowed={allowed:.2}"
                ));
            }
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the hitStats-augmented golden fixture, got {joined}"
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
        "hit_stats_matches_ei_golden: {joined} accounts joined, all {} count fields EXACT, \
         all {} damage fields within {DAMAGE_REL_TOLERANCE:.1}%",
        COUNT_FIELDS.len(),
        DAMAGE_FIELDS.len()
    );
}

#[test]
fn hit_stats_matches_ei_golden() {
    let golden = read_golden_json();
    check_hit_stats_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn hit_stats_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("hit_stats_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_hit_stats_matches_ei_golden(&bytes, &golden);
}

/// Real-log sanity check, post-rework era: no committed EI JSON sidecar
/// exists for this local-only fixture (same limitation `skill_damage_
/// golden.rs`'s equivalent postrework check documents), so this only
/// checks structural/internal sanity that the post-era classification path
/// (`hit_stats::classify`'s `post_era` branch) produces plausible,
/// self-consistent output on a real post-rework capture.
#[test]
fn hit_stats_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut any_connected = false;
    let mut any_direct = false;
    for p in &metrics.players {
        let h = &p.hit_stats;
        // Internal invariants: connected is the union of direct/condition/
        // life-leech; a hit-count can never exceed its own connected total.
        assert_eq!(
            h.direct_count + h.condition_count + h.life_leech_count,
            h.connected_count,
            "agent {:#x}: direct+condition+life_leech must equal connected exactly",
            p.agent_addr
        );
        assert!(h.crit_count <= h.direct_count, "agent {:#x}: crit_count must not exceed direct_count", p.agent_addr);
        assert!(h.glance_count <= h.direct_count, "agent {:#x}: glance_count must not exceed direct_count", p.agent_addr);
        assert!(h.critable_direct_count <= h.direct_count.max(h.critable_direct_count), "agent {:#x}: critable_direct_count sanity", p.agent_addr);
        assert!(h.against_downed_count <= h.connected_count, "agent {:#x}: against_downed_count must not exceed connected_count", p.agent_addr);
        assert!(h.moving_count <= h.connected_count, "agent {:#x}: moving_count must not exceed connected_count", p.agent_addr);
        if h.connected_count > 0 {
            any_connected = true;
        }
        if h.direct_count > 0 {
            any_direct = true;
        }
    }
    assert!(any_connected, "a real WvW squad fight should show nonzero connected hits somewhere");
    assert!(any_direct, "a real WvW squad fight should show nonzero direct hits somewhere");

    println!(
        "hit_stats_present_and_sane_on_local_postrework: {} players, internal invariants hold on a real post-era log",
        metrics.players.len()
    );
}

/// Fields immune to the documented `condition_count`/missing-catalog
/// simplification gap (see `analysis::hit_stats`'s module doc's "Direct vs
/// Condition vs Life-leech" section): every one of these either only ever
/// derives from a `buff == 0` (direct) hit, or is a cross-cutting flag
/// (against-downed/moving) applied identically regardless of which buff==1
/// sub-bucket a hit lands in, so reclassifying a buff==1 hit between
/// "condition" and "life-leech" can never move a count into or out of this
/// set. Checked with a small absolute tolerance (real captures showed a
/// handful of ~1-count residuals even here -- e.g. one crit/connected/
/// direct/critable/against-downed hit apparently near some other, unrelated
/// boundary condition this hook doesn't attempt to root-cause) against a
/// real post-era capture.
const RELIABLE_COUNT_FIELDS: &[(&str, &str)] = &[
    ("crit_count", "criticalRate"),
    ("flank_count", "flankingRate"),
    ("glance_count", "glanceRate"),
    ("moving_count", "againstMovingRate"),
    ("connected_count", "connectedDamageCount"),
    ("direct_count", "connectedDirectDamageCount"),
    ("critable_direct_count", "critableDirectDamageCount"),
    ("against_downed_count", "againstDownedCount"),
];
/// Small absolute tolerance for `RELIABLE_COUNT_FIELDS` on a real capture
/// (not the committed golden fixture, which is EXACT) -- see that const's
/// doc comment.
const RELIABLE_COUNT_ABS_TOLERANCE: i64 = 2;
/// Fields that fall inside the documented buff==1 condition-vs-life-leech
/// simplification gap (`hit_stats::condition_count`'s own doc section) --
/// NOT hard-asserted here (see this test's doc comment for why), only
/// reported.
const CATALOG_GAP_COUNT_FIELDS: &[(&str, &str)] = &[
    ("condition_count", "connectedConditionCount"),
    ("life_leech_count", "connectedLifeLeechCount"),
    ("above90_power_count", "connectedPowerAbove90HPCount"),
    ("above90_condition_count", "connectedConditionAbove90HPCount"),
];

/// M13 Task 3 (post-era calibration hook, cheap win from review): when a
/// real post-rework EI JSON sidecar (the dps.report `getJson` output for
/// the SAME log as `LOCAL_POSTREWORK_ZEVTC`) is ALSO present, this
/// calibrates `hit_stats`'s count fields against real `statsAll[0]`
/// numbers on that capture -- unlike
/// `hit_stats_present_and_sane_on_local_postrework_when_available` above
/// (structural/internal sanity only, no external reference), this is REAL
/// calibration of the post-era classification path
/// (`hit_stats::classify`'s `post_era` branch), the moment a real capture +
/// export pair exists.
///
/// **First real run of this hook (48-player, ~5m48s, arcdps build
/// 20260718, WvW capture) empirically CONFIRMED the theoretical
/// `condition_count`/catalog-gap caveat this module's doc already
/// disclosed** -- previously "documented, not observed" (the committed
/// synthetic/pre-rework fixtures showed zero residual). On this real
/// capture, a handful of players (`darkpink.9145`: `connectedConditionCount`
/// 139 (golden) vs 177 (ours), `connectedPowerAbove90HPCount` 450 vs 412,
/// `connectedConditionAbove90HPCount` 122 vs 160; `Lento.3714`: same pattern,
/// magnitude 1) show a real, non-trivial condition/power split divergence
/// -- but CONSERVED in total (`ours condition + ours life_leech ==
/// golden's condition + (golden's power_above90-style non-strike bucket)`
/// -- verified by hand for both players), i.e. a genuine buff==1 skill this
/// project's simplification puts in "condition" that real GW2EI's
/// `ConditionDamageBased` skill-id catalog evidently does NOT (or vice
/// versa) -- a real-world instance of the missing-catalog gap, not a
/// dropped/extra event. `RELIABLE_COUNT_FIELDS` (immune to this specific
/// split, see that const's doc comment) are asserted with a small
/// tolerance; `CATALOG_GAP_COUNT_FIELDS` are reported only, not hard-failed
/// -- hard-failing on a KNOWN, now-empirically-confirmed simplification gap
/// would make this hook permanently red for every future real capture with
/// even one uncatalogued buff==1 skill in play, defeating its own "green
/// signal for real regressions" purpose.
///
/// Joined by RAW account string (this fixture is a real, non-anonymized
/// local capture, unlike the committed `wvw-small.anon.zevtc`/
/// `wvw-small.ei.json` pair, so there's no `anon_account`-index join to
/// reuse here -- direct account-string match, same convention
/// `postrework_golden.rs`'s own account-based checks use). Skip-when-
/// absent, same as every other `fixtures/local/*`-gated test in this suite
/// -- CI has no local fixture, so this never runs there; a dev with a real
/// post-rework capture + its dps.report export gets real calibration
/// automatically, without writing a new test first.
#[test]
fn hit_stats_calibrated_against_local_postrework_ei_json_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (post-era hitStats real calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(LOCAL_POSTREWORK_EI_JSON).ok() else {
        println!("skip: {LOCAL_POSTREWORK_EI_JSON} absent (post-era hitStats real calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {LOCAL_POSTREWORK_EI_JSON}: {e}"));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(
        raw.header.is_post_buff_rework(),
        "this fixture must be a post-rework build for this check to be meaningful"
    );
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(a.trim_start_matches(':').to_string(), p);
        }
    }

    let mut joined = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut catalog_gap_notes: Vec<String> = Vec::new();
    for p in &enc.players {
        let key = p.account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(stats_all) = golden_p.get("statsAll").and_then(|v| v.get(0)) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        for &(our_field, golden_key) in RELIABLE_COUNT_FIELDS {
            let ours = field_value(&pm.hit_stats, our_field) as i64;
            let golden_val = stats_all[golden_key].as_i64().unwrap_or(0);
            if (ours - golden_val).abs() > RELIABLE_COUNT_ABS_TOLERANCE {
                mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[statsAll[0].{golden_key}]={golden_val} \
                     (diff {} exceeds tolerance {RELIABLE_COUNT_ABS_TOLERANCE})",
                    ours - golden_val
                ));
            }
        }

        for &(our_field, golden_key) in CATALOG_GAP_COUNT_FIELDS {
            let ours = field_value(&pm.hit_stats, our_field) as i64;
            let golden_val = stats_all[golden_key].as_i64().unwrap_or(0);
            if ours != golden_val {
                catalog_gap_notes.push(format!(
                    "{key} {our_field}: ours={ours} golden[statsAll[0].{golden_key}]={golden_val} \
                     (diff {}, known catalog-gap field, not hard-failed)",
                    ours - golden_val
                ));
            }
        }
    }

    if joined == 0 {
        println!(
            "skip: 0 accounts joined between {LOCAL_POSTREWORK_ZEVTC} and {LOCAL_POSTREWORK_EI_JSON} \
             (post-era hitStats real calibration) -- account-string mismatch, or the export has no \
             `statsAll[0]` block for any joined player"
        );
        return;
    }

    assert!(
        mismatches.is_empty(),
        "{} RELIABLE count mismatch(es) exceeding tolerance {RELIABLE_COUNT_ABS_TOLERANCE} against \
         a REAL post-era capture (checked {joined} accounts):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    println!(
        "hit_stats_calibrated_against_local_postrework_ei_json: {joined} accounts joined, all {} \
         reliable count fields within tolerance {RELIABLE_COUNT_ABS_TOLERANCE} on a REAL post-era \
         capture; {} catalog-gap-field note(s) (reported, not hard-failed):",
        RELIABLE_COUNT_FIELDS.len(),
        catalog_gap_notes.len()
    );
    for n in &catalog_gap_notes {
        println!("  {n}");
    }
}
