//! M12 Task 1: per-skill damage distribution calibration against EI's
//! `totalDamageDist`/`totalDamageTaken` (`crates/axilog-core/src/analysis/
//! skill_damage.rs`).
//!
//! ## EI's grouping semantics (source: `axibridge/test-fixtures/boon/
//! 20260117-181030.json`, a real dps.report WvW export)
//!
//! EI's `players[].totalDamageDist`/`totalDamageTaken` are `[phase][]`
//! arrays of entries shaped `{ id, totalDamage, hits, min, max, crit,
//! flank, indirectDamage, ... }` -- **one entry per (skill id, direct-vs-
//! indirect) pair**, NOT one entry per skill id: a skill that deals both a
//! direct-hit component and a separate indirect/condition-tick component
//! (rare, but structurally possible) gets TWO entries sharing the same
//! `id`, distinguished only by `indirectDamage: bool`. Verified directly on
//! this fixture's real data: every one of the 41 players' `totalDamageDist`
//! arrays has zero duplicate `id`s in practice (`indirectDamage: true`/
//! `false` never co-occur for the same skill on this fixture), but the
//! extraction script that builds `fixtures/wvw-small.ei.json`'s
//! `players[].skillDamage` block still sums by `id` defensively (folding
//! direct + indirect together) to produce EI's PER-SKILL TOTAL -- the
//! semantics the M12 plan asks to calibrate against, not the raw
//! per-(id, direct/indirect) row shape.
//!
//! This project's `skill_damage::SkillEntry` never splits direct/indirect
//! at all (the established damage predicate, `analysis::damage::
//! accumulate`, already collapses `buff==0`/`buff==1` into one `dmg` value
//! per event before this module ever groups by skill id) -- so "one entry
//! per skill id" here already matches EI's per-skill TOTAL once EI's own
//! direct/indirect split is folded back together, which is exactly what
//! the golden fixture's extraction does.
//!
//! ## The one documented systematic gap: pet/minion damage
//!
//! EI's own `totalDamageDist` tracks the PLAYER ACTOR ONLY -- verified
//! directly against the real source JSON: `players[].dpsAll[0].damage`
//! (the scalar EI itself reports as "this player's total damage", which
//! this project's `damage_total`/native `damage.total` calibrate against
//! elsewhere) frequently EXCEEDS `sum(totalDamageDist[*].totalDamage)` by a
//! nonzero amount for players with an active pet/minion -- e.g. this
//! fixture's `HarborJet.5544` (now `Anon140.6180`): `dpsAll[0].damage =
//! 100089` vs `sum(totalDamageDist) = 85037`, a gap of exactly `15052`.
//! That gap is NOT split-out anywhere else on the player row -- EI tracks
//! it under a completely separate per-MINION `totalDamageDist` (`players[].
//! minions[]`, not extracted into the golden fixture, out of scope for this
//! task), never folded back onto the owning player's own distribution.
//!
//! This project's `skill_damage::build` instead folds pet/minion damage
//! onto the owning player (via the same time-aware `InstidRegistry` owner
//! resolution `damage::accumulate_pet_credit` already uses for
//! `damage_total`), **keyed by the PET'S OWN skill id** -- so
//! `sum(outgoing[*].total)` always equals the player's full `damage_total`
//! (actor + pet/minion), matching EI's `dpsAll[0].damage` scalar exactly,
//! while individual pet-only skill ids simply don't appear anywhere in
//! EI's own `totalDamageDist` to compare against.
//!
//! Measured on this fixture (37 real accounts joined via the established
//! `anon_account(raw agent-table index)` method, same as `healing_golden.
//! rs`/`support_golden.rs`): **4 of 37** accounts have one or more skill ids
//! present in `outgoing` that are absent from golden's folded
//! `totalDamageDist` (the pet/minion-attributed ids) -- summing to exactly
//! `24369` extra damage across those 4 accounts, which is EXACTLY the
//! squad-wide gap between `sum(all players' folded totalDamageDist)`
//! (`2114045`) and `squadTotalDamage`/`sum(dpsAll[0].damage)` (`2138414`),
//! confirming this is the full, sole explanation for the gap (not a partial
//! one). For every SHARED skill id (present in both this project's
//! `outgoing` and EI's folded `totalDamageDist`), the two totals match
//! **EXACTLY** -- zero mismatches across all 37 accounts' shared ids.
//!
//! `taken` has one further, much smaller EI quirk: skill id `23279` (not
//! present in EI's own `skillMap` at all -- i.e. EI itself doesn't name/
//! track this id) contributes 1-2 extra incoming damage on 8/37 accounts,
//! entirely absent from EI's `totalDamageTaken`. Negligible (max 2 damage
//! against per-account `taken` totals in the tens of thousands) and
//! documented via `TAKEN_SUM_ABS_TOLERANCE` below rather than silently
//! ignored.
//!
//! ## Tolerance
//!
//! - Every SHARED skill id (outgoing AND taken): **EXACT**.
//! - `sum(outgoing) == golden["damage"]` (EI's `dpsAll[0].damage`, the
//!   pet-inclusive total): **EXACT** (holds for all 37 joined accounts,
//!   including the 4 pet/minion-affected ones -- see above).
//! - `sum(taken)` vs golden's folded `totalDamageTaken` sum: within a flat
//!   `TAKEN_SUM_ABS_TOLERANCE` (the measured max residual, `23279`'s 1-2
//!   damage, plus margin) -- not a relative tolerance, since the absolute
//!   magnitude is what's actually bounded here.
//! - `sum(per-skill outgoing) == PlayerMetrics::damage_total` and
//!   `sum(per-skill taken) == PlayerMetrics::damage_taken`: **EXACT**, for
//!   EVERY player (not just the 37 joined to golden) -- internal
//!   invariants, true by construction (`skill_damage`'s module doc), reverified
//!   directly here as a regression guard.

use axilog_core::analysis::analyze;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::{HashMap, HashSet};

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-small.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
const LOCAL_POSTREWORK_ZEVTC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/local/wvw-postrework.zevtc");

/// `taken` sum flat-absolute tolerance -- module doc: the measured residual
/// is skill id `23279` (absent from EI's own `skillMap`) contributing 1-2
/// extra damage on 8/37 accounts. `10` is that measured max (2) plus a
/// generous margin, still utterly negligible against real per-account
/// `taken` totals (tens of thousands).
const TAKEN_SUM_ABS_TOLERANCE: i64 = 10;

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

struct SkillIdMismatch {
    account: String,
    direction: &'static str,
    skill_id: u32,
    ours: Option<i64>,
    golden: i64,
}

fn check_skill_damage_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    // Internal invariants (module doc): hold for EVERY player, not just
    // those joined to golden -- `skill_damage::build` is a grouped
    // refinement of the SAME predicate `damage::accumulate`/
    // `accumulate_damage_taken` already use for `damage_total`/
    // `damage_taken`, so these sums must match exactly by construction.
    for p in &metrics.players {
        let outgoing_sum: u64 = p.skill_damage.outgoing.iter().map(|e| e.total).sum();
        assert_eq!(
            outgoing_sum, p.damage_total,
            "agent {:#x}: sum(skill_damage.outgoing) must equal damage_total exactly",
            p.agent_addr
        );
        let taken_sum: u64 = p.skill_damage.taken.iter().map(|e| e.total).sum();
        assert_eq!(
            taken_sum, p.damage_taken,
            "agent {:#x}: sum(skill_damage.taken) must equal damage_taken exactly",
            p.agent_addr
        );
        let per_target_sum: u64 =
            p.skill_damage.per_target.iter().flat_map(|t| t.skills.iter()).map(|e| e.total).sum();
        assert_eq!(
            per_target_sum, outgoing_sum,
            "agent {:#x}: sum(skill_damage.per_target) must equal sum(skill_damage.outgoing) exactly",
            p.agent_addr
        );
    }

    // Per-account join against golden, same method as `healing_golden.rs`/
    // `support_golden.rs`: raw agent-table index -> `anon_account(i)` ->
    // golden `account`.
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }
    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut joined = 0usize;
    let mut exact_mismatches: Vec<SkillIdMismatch> = Vec::new();
    let mut damage_total_mismatches: Vec<(String, i64, i64)> = Vec::new();
    let mut taken_sum_mismatches: Vec<(String, i64, i64)> = Vec::new();
    // Accounts with a skill id in `outgoing` absent from golden's
    // `totalDamageDist` (the pet/minion gap, module doc) -- tracked for the
    // sanity assertion below, not a failure on its own.
    let mut accounts_with_pet_gap: HashSet<String> = HashSet::new();
    let mut pet_gap_total: i64 = 0;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(sd) = golden_p.get("skillDamage") else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else { continue };
        joined += 1;

        let golden_outgoing: HashMap<u32, i64> = sd["outgoing"]
            .as_array()
            .expect("outgoing array")
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["total"].as_i64().unwrap()))
            .collect();
        let our_outgoing: HashMap<u32, i64> =
            pm.skill_damage.outgoing.iter().map(|e| (e.skill_id, e.total as i64)).collect();

        for (&id, &gtotal) in &golden_outgoing {
            let ours = our_outgoing.get(&id).copied();
            if ours != Some(gtotal) {
                exact_mismatches.push(SkillIdMismatch {
                    account: key.clone(), direction: "outgoing", skill_id: id, ours, golden: gtotal,
                });
            }
        }
        for (&id, &ototal) in &our_outgoing {
            if !golden_outgoing.contains_key(&id) {
                accounts_with_pet_gap.insert(key.clone());
                pet_gap_total += ototal;
            }
        }

        // sum(outgoing) must equal golden's `damage` scalar (EI's
        // `dpsAll[0].damage`, pet-inclusive) EXACTLY -- module doc: holds
        // even for the pet/minion-affected accounts, since the pet's own
        // skill id(s) are folded into `outgoing` too.
        let our_outgoing_sum: i64 = our_outgoing.values().sum();
        let golden_damage = golden_p["damage"].as_i64().expect("damage");
        if our_outgoing_sum != golden_damage {
            damage_total_mismatches.push((key.clone(), our_outgoing_sum, golden_damage));
        }

        let golden_taken: HashMap<u32, i64> = sd["taken"]
            .as_array()
            .expect("taken array")
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["total"].as_i64().unwrap()))
            .collect();
        let our_taken: HashMap<u32, i64> =
            pm.skill_damage.taken.iter().map(|e| (e.skill_id, e.total as i64)).collect();

        for (&id, &gtotal) in &golden_taken {
            let ours = our_taken.get(&id).copied();
            if ours != Some(gtotal) {
                exact_mismatches.push(SkillIdMismatch {
                    account: key.clone(), direction: "taken", skill_id: id, ours, golden: gtotal,
                });
            }
        }

        let our_taken_sum: i64 = our_taken.values().sum();
        let golden_taken_sum: i64 = golden_taken.values().sum();
        if (our_taken_sum - golden_taken_sum).abs() > TAKEN_SUM_ABS_TOLERANCE {
            taken_sum_mismatches.push((key.clone(), our_taken_sum, golden_taken_sum));
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the skill-damage-augmented golden fixture, got {joined}"
    );

    if !exact_mismatches.is_empty() {
        let report: Vec<String> = exact_mismatches
            .iter()
            .map(|m| {
                format!(
                    "{} {} skill {}: ours={:?} golden={}",
                    m.account, m.direction, m.skill_id, m.ours, m.golden
                )
            })
            .collect();
        panic!(
            "{} shared skill-id total(s) out of EXACT tolerance (checked {joined} accounts):\n{}",
            exact_mismatches.len(),
            report.join("\n")
        );
    }
    if !damage_total_mismatches.is_empty() {
        let report: Vec<String> = damage_total_mismatches
            .iter()
            .map(|(a, o, g)| format!("{a}: sum(outgoing)={o} golden[\"damage\"]={g}"))
            .collect();
        panic!(
            "{} account(s) where sum(skill_damage.outgoing) != golden[\"damage\"]:\n{}",
            damage_total_mismatches.len(),
            report.join("\n")
        );
    }
    if !taken_sum_mismatches.is_empty() {
        let report: Vec<String> = taken_sum_mismatches
            .iter()
            .map(|(a, o, g)| format!("{a}: our_sum={o} golden_sum={g} diff={}", o - g))
            .collect();
        panic!(
            "{} account(s) where sum(skill_damage.taken) exceeds the {TAKEN_SUM_ABS_TOLERANCE}-unit \
             tolerance vs golden's folded totalDamageTaken sum:\n{}",
            taken_sum_mismatches.len(),
            report.join("\n")
        );
    }

    // Sanity check on the documented pet/minion gap itself (module doc):
    // this fixture is known to have exactly 4 affected accounts summing to
    // exactly 24369 extra outgoing damage (the full squadTotalDamage vs
    // sum(totalDamageDist) gap) -- a hard-coded assertion here would be
    // fragile to fixture/extraction changes, so this only asserts the
    // qualitative shape (a small minority of accounts, never dominating).
    assert!(
        accounts_with_pet_gap.len() <= joined / 4,
        "pet/minion-attributed skill ids should affect only a small minority of accounts, \
         got {}/{joined} (total extra damage {pet_gap_total})",
        accounts_with_pet_gap.len()
    );

    println!(
        "skill_damage_matches_ei_golden: {joined} accounts joined, 0 shared-id mismatches, \
         sum(outgoing)==golden[\"damage\"] exact for all, {}/{joined} accounts show the documented \
         pet/minion gap (total {pet_gap_total} extra damage, not present in EI's own totalDamageDist)",
        accounts_with_pet_gap.len()
    );
}

#[test]
fn skill_damage_matches_ei_golden() {
    let golden = read_golden_json();
    check_skill_damage_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn skill_damage_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("skill_damage_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_skill_damage_matches_ei_golden(&bytes, &golden);
}

/// Real-log sanity check, post-rework era (M4/M10-style skip-when-absent):
/// no committed EI JSON sidecar exists for this local-only fixture (same
/// limitation `healing_golden.rs`'s equivalent postrework check documents),
/// so this only checks structural/internal sanity -- the invariants below
/// hold regardless of which arcdps build era produced the log, since
/// `skill_damage::build` reuses `damage`'s own era-independent predicate.
#[test]
fn skill_damage_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut any_outgoing = false;
    let mut any_taken = false;
    for p in &metrics.players {
        let outgoing_sum: u64 = p.skill_damage.outgoing.iter().map(|e| e.total).sum();
        assert_eq!(outgoing_sum, p.damage_total, "agent {:#x}: sum(outgoing) must equal damage_total", p.agent_addr);
        let taken_sum: u64 = p.skill_damage.taken.iter().map(|e| e.total).sum();
        assert_eq!(taken_sum, p.damage_taken, "agent {:#x}: sum(taken) must equal damage_taken", p.agent_addr);
        let per_target_sum: u64 =
            p.skill_damage.per_target.iter().flat_map(|t| t.skills.iter()).map(|e| e.total).sum();
        assert_eq!(per_target_sum, outgoing_sum, "agent {:#x}: sum(per_target) must equal sum(outgoing)", p.agent_addr);
        // Every skill id in every list must be nonzero-total (established
        // predicate skips dmg == 0 events -- see skill_damage's module doc).
        for e in p.skill_damage.outgoing.iter().chain(p.skill_damage.taken.iter()) {
            assert!(e.total > 0, "agent {:#x}: skill entry {} has zero total, should have been skipped", p.agent_addr, e.skill_id);
            assert!(e.hits > 0);
        }
        if outgoing_sum > 0 {
            any_outgoing = true;
        }
        if taken_sum > 0 {
            any_taken = true;
        }
    }
    assert!(any_outgoing, "a real WvW squad fight should show nonzero outgoing skill damage somewhere");
    assert!(any_taken, "a real WvW squad fight should show nonzero incoming skill damage somewhere");

    println!("skill_damage_present_and_sane_on_local_postrework: {} players, internal invariants hold", metrics.players.len());
}
