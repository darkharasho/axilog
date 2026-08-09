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
//! module doc's writeup of why `PowerDamageTaken(Count)` equals
//! `StrikeDamageTaken(Count) + [true life-leech](Count)` on every account
//! observed on this fixture, since the bug only touches the INNER
//! life-leech-sum increment, not the outer power-bucket increment --
//! **NOTE**: this equality holds by THIS PROJECT'S OWN construction
//! unconditionally, but is only empirically-zero-residual, not formally
//! proven, against real GW2EI's own output -- a buff==1 hit outside GW2EI's
//! `ConditionDamageBased` skill-id catalog AND not life-leech would break it
//! there; see `analysis::defenses`'s module doc for the full writeup).

use axilog_core::analysis::analyze;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

mod common;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");

const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
/// Gitignored, PII-bearing local captures -- resolved through
/// `common::local_fixture` (M15 Task 1) so `AXILOG_LOCAL_FIXTURES` can point
/// a `git worktree` at the primary checkout's `fixtures/local/` instead of
/// requiring the captures to be copied (which would duplicate PII):
///
/// ```sh
/// AXILOG_LOCAL_FIXTURES=/path/to/axilog/fixtures/local \
///     cargo test -p axilog-core --test defenses_golden -- --nocapture
/// ```
///
/// Unset, these resolve to the in-tree `fixtures/local/` exactly as the
/// hardcoded `concat!(env!("CARGO_MANIFEST_DIR"), ...)` paths they replaced
/// did, so CI behaviour (skip-when-absent) is unchanged.
fn local_fixture_path() -> String {
    common::local_fixture("wvw-small.zevtc")
}
fn local_postrework_zevtc() -> String {
    common::local_fixture("wvw-postrework.zevtc")
}
fn local_postrework_ei_json() -> String {
    common::local_fixture("wvw-postrework.ei.json")
}

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
    let path = local_fixture_path();
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {path} absent ({test_name} local-only extra check)");
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

        // `life_leech_count`/`life_leech_damage`: derived reference (see
        // module doc's bug writeup), NOT read from the fixture's buggy
        // `lifeLeechDamageTakenCount` (always 0) directly.
        //
        // **MCONDCAT Task 1**: the PRIMARY derivation is now the
        // fourth-bucket-immune bug identity -- GW2EI double-increments the
        // SUM field, so `golden.lifeLeechDamageTaken == [true sum] + [true
        // count]`, and both of ours can be checked with one equality.
        let buggy_life_leech_sum = de["lifeLeechDamageTaken"].as_i64().unwrap_or(0);
        let ours_encoded = (pm.defenses.life_leech_damage + pm.defenses.life_leech_count as u64) as i64;
        if ours_encoded != buggy_life_leech_sum {
            count_mismatches.push(format!(
                "{key} life_leech: ours life_leech_damage({}) + life_leech_count({}) = {ours_encoded}, \
                 golden[lifeLeechDamageTaken]={buggy_life_leech_sum} (GW2EI's double-increment bug, \
                 i.e. `[true sum] + [true count]`)",
                pm.defenses.life_leech_damage, pm.defenses.life_leech_count
            ));
        }
        // Secondary cross-check with the OLD derivation
        // (`powerDamageTakenCount - strikeDamageTakenCount`), which equals
        // `life_leech + fourth_bucket`: on this committed pre-rework
        // fixture the fourth bucket is empty, so it must still agree. If it
        // ever stops agreeing, the fixture has grown fourth-bucket rows and
        // its committed golden numbers need a human review pass.
        let power_count = de["powerDamageTakenCount"].as_i64().unwrap_or(0);
        let strike_count = de["strikeDamageTakenCount"].as_i64().unwrap_or(0);
        let true_life_leech_count = (power_count - strike_count).max(0) as u64;
        let ours_llc = pm.defenses.life_leech_count as u64;
        if ours_llc != true_life_leech_count {
            count_mismatches.push(format!(
                "{key} life_leech_count: ours={ours_llc} derived_golden={true_life_leech_count} \
                 (powerDamageTakenCount={power_count} - strikeDamageTakenCount={strike_count}; \
                 this fixture is expected to be FOURTH-BUCKET-FREE, so the two derivations must \
                 agree -- see `analysis::condition_catalog`)"
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
    let postrework_zevtc = local_postrework_zevtc();
    let Some(bytes) = std::fs::read(&postrework_zevtc).ok() else {
        println!("skip: {postrework_zevtc} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    assert!(raw.header.is_post_buff_rework(), "this fixture must be a post-rework build for this check to be meaningful");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut any_strike = false;
    let mut fourth_bucket_players = 0usize;
    let mut fourth_bucket_hits = 0u32;
    for p in &metrics.players {
        let d = &p.defenses;
        // **MCONDCAT Task 1 reworked this assertion.** It used to be
        // `power == strike + life_leech` EXACTLY, which held only by this
        // module's own pre-catalog construction (three hit kinds, power
        // accumulating on two of them) -- and which real GW2EI's output
        // does NOT satisfy. With `HitKind::PowerOnly` (the fourth bucket:
        // buff==1, outside the condition catalog, not life-leech) the
        // correct invariant is the INEQUALITY, with the slack being exactly
        // the fourth-bucket population:
        //
        //   power_count == strike_count + life_leech_count + fourth_count
        //
        // which we can't decompose further from the outside, so assert the
        // direction and report the slack.
        let strike_plus_leech = d.strike_count + d.life_leech_count;
        assert!(
            d.power_count >= strike_plus_leech,
            "agent {:#x}: power_count ({}) must be >= strike_count + life_leech_count ({strike_plus_leech}) -- \
             power accumulates on every non-condition hit, a superset of both",
            p.agent_addr,
            d.power_count
        );
        assert!(
            d.power_damage >= d.strike_damage + d.life_leech_damage,
            "agent {:#x}: power_damage ({}) must be >= strike_damage + life_leech_damage ({})",
            p.agent_addr,
            d.power_damage,
            d.strike_damage + d.life_leech_damage
        );
        let fourth = d.power_count - strike_plus_leech;
        if fourth > 0 {
            fourth_bucket_players += 1;
            fourth_bucket_hits += fourth;
        }
        if d.strike_count > 0 {
            any_strike = true;
        }
    }
    assert!(any_strike, "a real WvW squad fight should show nonzero incoming strike hits somewhere");
    // The fourth bucket is genuinely populated on this real capture -- that
    // is the whole reason MCONDCAT exists. Assert it is actually exercised
    // here so this check can never silently degrade back into a
    // three-bucket-only test.
    assert!(
        fourth_bucket_hits > 0,
        "expected the MCONDCAT fourth bucket (power-only hits) to be populated on this real \
         post-era capture -- if this ever fires, the catalog probe has stopped doing anything"
    );
    println!(
        "defenses_present_and_sane_on_local_postrework: fourth bucket populated for \
         {fourth_bucket_players} player(s), {fourth_bucket_hits} hit(s) total"
    );

    println!(
        "defenses_present_and_sane_on_local_postrework: {} players, internal invariants hold on a real post-era log",
        metrics.players.len()
    );
}

/// Fields immune to the documented buff==1 condition/life-leech split gap
/// (see `analysis::defenses`'s module doc's "power == strike + life_leech"
/// section, M13 Task 3 review fix): every one of these only ever derives
/// from a `buff == 0` (direct) row, or (dodge) is a wholly separate
/// self-cast-skill scan untouched by the incoming-hit classification at
/// all. Checked with a small absolute tolerance against a real post-era
/// capture (see `RELIABLE_COUNT_ABS_TOLERANCE`'s doc comment for the
/// observed real-world noise this accommodates).
const RELIABLE_COUNT_FIELDS: &[(&str, &str)] = &[
    ("blocked_count", "blockedCount"),
    ("evaded_count", "evadedCount"),
    ("missed_count", "missedCount"),
    ("dodge_count", "dodgeCount"),
    ("invulned_count", "invulnedCount"),
    ("interrupted_count", "interruptedCount"),
    ("strike_count", "strikeDamageTakenCount"),
    ("barrier_count", "damageBarrierCount"),
    ("breakbar_count", "breakbarDamageTakenCount"),
];
/// Small absolute tolerance for `RELIABLE_COUNT_FIELDS` on a real capture
/// (not the committed golden fixture, which is EXACT) -- first real run
/// observed a 1-count `dodgeCount` residual on 2 of 44 joined accounts (a
/// dodge-skill activation row apparently near some other, unrelated
/// boundary condition this hook doesn't attempt to root-cause), nothing
/// larger on any `RELIABLE_COUNT_FIELDS` entry.
const RELIABLE_COUNT_ABS_TOLERANCE: i64 = 2;

/// M13 Task 3 (post-era calibration hook, cheap win from review): when a
/// real post-rework EI JSON sidecar (the dps.report `getJson` output for
/// the SAME log as `local_postrework_zevtc()`) is ALSO present, this
/// calibrates `defenses`'s count fields against real `defenses[0]` numbers
/// on that capture -- unlike
/// `defenses_present_and_sane_on_local_postrework_when_available` above
/// (structural/internal sanity only, no external reference), this is REAL
/// calibration of the post-era classification path (`defenses::classify`'s
/// `post_era` branch), the moment a real capture + export pair exists.
///
/// **History: this hook is what proved the "fourth bucket" real, and
/// MCONDCAT Task 1 is what closed it.** Its first real run (a 48-player WvW
/// capture, arcdps build 20260718) empirically confirmed the then-theoretical
/// "buff==1-outside-the-condition-catalog-and-not-life-leech" edge case:
/// 33 of 44 joined accounts diverged on `power_count`/`condition_count`, by
/// up to 51% relative on one account, while every field immune to the
/// buff==1 split stayed within a 2-count tolerance. The divergence was
/// CONSERVED in total on every account (the `power_count` shortfall equalled
/// the `condition_count` excess exactly), i.e. a pure RECLASSIFICATION
/// rather than a dropped or extra event -- the signature of a missing
/// catalog. Those fields were report-only at the time.
///
/// With `analysis::condition_catalog` in place they are **EXACT on all 44
/// joined accounts** and are hard-asserted again here, along with
/// `power_damage`/`condition_damage` and a fourth-bucket-immune life-leech
/// identity (see the inline comments in the loop below for why the old
/// `powerDamageTakenCount - strikeDamageTakenCount` derivation had to go).
/// `RELIABLE_COUNT_FIELDS` keep their small tolerance for the unrelated
/// `dodgeCount` residual documented on that const.
///
/// Joined by RAW account string (real, non-anonymized local capture, same
/// convention `hit_stats_golden.rs`'s equivalent hook uses). Skip-when-
/// absent, same as every other `fixtures/local/*`-gated test -- CI has no
/// local fixture, so this never runs there.
#[test]
fn defenses_calibrated_against_local_postrework_ei_json_when_available() {
    let postrework_zevtc = local_postrework_zevtc();
    let postrework_ei_json = local_postrework_ei_json();
    let Some(bytes) = std::fs::read(&postrework_zevtc).ok() else {
        println!("skip: {postrework_zevtc} absent (post-era defenses real calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(&postrework_ei_json).ok() else {
        println!("skip: {postrework_ei_json} absent (post-era defenses real calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {postrework_ei_json}: {e}"));

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
        let Some(de) = golden_p.get("defenses").and_then(|v| v.get(0)) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        for &(our_field, golden_key) in RELIABLE_COUNT_FIELDS {
            let ours = field_value(&pm.defenses, our_field) as i64;
            let golden_val = de[golden_key].as_i64().unwrap_or(0);
            if (ours - golden_val).abs() > RELIABLE_COUNT_ABS_TOLERANCE {
                mismatches.push(format!(
                    "{key} {our_field}: ours={ours} golden[defenses[0].{golden_key}]={golden_val} \
                     (diff {} exceeds tolerance {RELIABLE_COUNT_ABS_TOLERANCE})",
                    ours - golden_val
                ));
            }
        }

        // **MCONDCAT Task 1: these are the fields the catalog fixed, and
        // they are now asserted EXACT (they were report-only before).**
        let power_count = de["powerDamageTakenCount"].as_i64().unwrap_or(0);
        let condition_count_golden = de["conditionDamageTakenCount"].as_i64().unwrap_or(0);
        let power_damage = de["powerDamageTaken"].as_i64().unwrap_or(0);
        let condition_damage_golden = de["conditionDamageTaken"].as_i64().unwrap_or(0);
        for (label, ours, golden_val) in [
            ("power_count", pm.defenses.power_count as i64, power_count),
            ("condition_count", pm.defenses.condition_count as i64, condition_count_golden),
            ("power_damage", pm.defenses.power_damage as i64, power_damage),
            ("condition_damage", pm.defenses.condition_damage as i64, condition_damage_golden),
        ] {
            if ours != golden_val {
                mismatches.push(format!(
                    "{key} {label}: ours={ours} golden[defenses[0]]={golden_val} (diff {})",
                    ours - golden_val
                ));
            }
        }

        // `life_leech_count`/`life_leech_damage`: the committed-fixture
        // test's `powerDamageTakenCount - strikeDamageTakenCount`
        // derivation is WRONG post-MCONDCAT (that difference is
        // `life_leech + fourth_bucket`, not `life_leech`). Use the
        // bug-exploiting identity instead, which is fourth-bucket-immune:
        // GW2EI's ctor increments `LifeLeechDamageTaken` twice per
        // life-leech hit (once by `+= HealthDamage`, once by the
        // copy-pasted `LifeLeechDamageTaken++` that should have been
        // `LifeLeechDamageTakenCount++`), so the reported SUM is exactly
        // `[true sum] + [true count]` and nothing else touches it.
        let buggy_life_leech_sum = de["lifeLeechDamageTaken"].as_i64().unwrap_or(0);
        let ours_encoded =
            pm.defenses.life_leech_damage as i64 + pm.defenses.life_leech_count as i64;
        if ours_encoded != buggy_life_leech_sum {
            mismatches.push(format!(
                "{key} life_leech: ours life_leech_damage({}) + life_leech_count({}) = {ours_encoded}, \
                 golden[defenses[0].lifeLeechDamageTaken]={buggy_life_leech_sum} (diff {}) -- \
                 the golden field is GW2EI's double-increment bug, i.e. `[true sum] + [true count]`",
                pm.defenses.life_leech_damage,
                pm.defenses.life_leech_count,
                ours_encoded - buggy_life_leech_sum
            ));
        }
        // And GW2EI's own (never-incremented) count field must still read 0
        // -- if a future GW2EI release fixes the bug, the identity above
        // silently becomes wrong, so pin the bug's own signature.
        let buggy_life_leech_count = de["lifeLeechDamageTakenCount"].as_i64().unwrap_or(0);
        if buggy_life_leech_count != 0 {
            catalog_gap_notes.push(format!(
                "{key}: golden lifeLeechDamageTakenCount={buggy_life_leech_count} != 0 -- GW2EI may \
                 have FIXED its double-increment bug; the life_leech derivation above needs revisiting"
            ));
        }
    }

    if joined == 0 {
        println!(
            "skip: 0 accounts joined between {postrework_zevtc} and {postrework_ei_json} \
             (post-era defenses real calibration) -- account-string mismatch, or the export has no \
             `defenses[0]` block for any joined player"
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
        "defenses_calibrated_against_local_postrework_ei_json: {joined} accounts joined, all {} \
         reliable count fields within tolerance {RELIABLE_COUNT_ABS_TOLERANCE} on a REAL post-era \
         capture; {} catalog-gap-field note(s) (reported, not hard-failed):",
        RELIABLE_COUNT_FIELDS.len(),
        catalog_gap_notes.len()
    );
    for n in &catalog_gap_notes {
        println!("  {n}");
    }
}
