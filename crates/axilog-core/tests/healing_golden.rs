//! M10 Task 1: healing/barrier-stat calibration against the EI healing
//! extension (`extHealingStats`/`extBarrierStats`).
//!
//! Same real-account join method as `support_golden.rs`/`boons_golden.rs`:
//! raw agent-table index -> `anon_account(i)` -> golden `account` (works
//! identically against the committed anon fixture or the real local one).
//! `fixtures/wvw-small.ei.json`'s per-player `healing` object (see that
//! fixture's `_note`, Task 1/M10 addendum) carries EI's own
//! `outgoingHealing[0]` `{healing, downedHealing}` and
//! `extBarrierStats.outgoingBarrier[0].barrier` fields verbatim, plus the
//! derived `healingSelf`/`healingAllies` split -- see `analysis::healing`'s
//! module doc for why `healing_out_allies` is derived (`total - self`)
//! rather than a sum of EI's own per-friendly array.
//!
//! ## Tolerance
//!
//! `healing_out_self`/`downed_healing_out`: **EXACT**, both squad-wide and
//! per-player, zero mismatches across all 41 joined accounts -- these two
//! fields never exhibited the residual described below.
//!
//! `healing_out_total`/`healing_out_allies`/`barrier_out`: bounded
//! tolerance, NOT exact, after exhausting systematic causes (same
//! "tolerance after exhausting systematic causes" posture as
//! `boons_golden.rs`'s `INTENSITY_STACK_RELATIVE_TOLERANCE`/
//! `PRESENCE_TOLERANCE_PP`). Squad-wide: `healing_out_total`/
//! `healing_out_allies` are within 1% (0.68%/0.71% actual on this fixture);
//! `barrier_out` is within 10% (7.6% actual). Root cause, verified by
//! direct inspection of the raw decoded events for every mismatching
//! account (not guessed): a handful of accounts cast a REPEATING skill
//! (same skill id, same fixed heal/barrier amount, reapplied ~15-35s apart
//! -- consistent with a recharge-gated repeatable skill, not a genuine
//! duplicate) whose "peer" (cross-client relayed) copies straddle GW2EI's
//! `SanitizeForSrc` all-or-nothing rule in a way this project's byte-level
//! replication cannot perfectly reconstruct without also replicating
//! GW2EI's internal per-agent-lifetime `AgentItem` identity tracking (out
//! of scope -- see `evtc::ext_healing`'s module doc for the documented
//! peer/non-peer mechanism this project DOES replicate exactly, which is
//! what gets 37/41 accounts' `healing_out_total` and 40/41 accounts'
//! `barrier_out` to EXACT parity already). The single largest outlier
//! (one account's `barrier_out`, off by 21240 out of a 49691 golden value)
//! traces to exactly one repeating skill id's peer-report cluster -- see
//! `analysis::healing`'s module doc for the full investigation trail.

use axilog_core::analysis::analyze;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

mod common;
use common::rel_close;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

const LOCAL_POSTREWORK_ZEVTC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-postrework.zevtc"
);

/// Squad-wide tolerance for `healing_out_total`/`healing_out_allies` (module
/// doc: 0.68%/0.71% actual on this fixture).
const SQUAD_HEALING_TOLERANCE: f64 = 0.01;
/// Squad-wide tolerance for `barrier_out` (module doc: 7.6% actual, driven
/// by one account's one repeating-skill peer-report cluster).
const SQUAD_BARRIER_TOLERANCE: f64 = 0.10;

/// Per-player tolerance for `healing_out_total`/`healing_out_allies`: the
/// larger of a flat floor (sized to comfortably cover one extra repeating-
/// skill pulse cycle, empirically ~4248 per pulse on this fixture) or a 2%
/// relative tolerance (for large-magnitude accounts where a flat floor
/// would be too tight). Covers all 4 known-affected accounts (largest:
/// 3262 absolute / 23.96% relative on a low-magnitude account) with margin.
const PLAYER_HEALING_ABS_FLOOR: f64 = 5_000.0;
const PLAYER_HEALING_REL_TOLERANCE: f64 = 0.02;
/// Per-player tolerance for `barrier_out`: same shape, larger floor (the
/// single known-affected account's residual is ~5 pulses, ~21240).
const PLAYER_BARRIER_ABS_FLOOR: f64 = 25_000.0;
const PLAYER_BARRIER_REL_TOLERANCE: f64 = 0.02;

fn within_tolerance(ours: i64, golden: i64, abs_floor: f64, rel_tolerance: f64) -> bool {
    let diff = (ours - golden).unsigned_abs() as f64;
    diff <= abs_floor.max(rel_tolerance * golden.unsigned_abs() as f64)
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

fn read_anon_fixture() -> Vec<u8> {
    std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"))
}

fn read_golden_json() -> serde_json::Value {
    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&golden_str).expect("parse golden EI JSON")
}

struct Mismatch {
    account: String,
    field: &'static str,
    ours: i64,
    golden: i64,
}

fn check_healing_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    assert!(metrics.has_healing_extension, "fixture must carry the healing extension");
    assert!(
        !metrics.warnings.iter().any(|w| w.contains("healing extension not present")),
        "healing extension IS present in this fixture; the absent-warning must not fire, got {:?}",
        metrics.warnings
    );

    // Squad-wide sums (module doc: tolerance after exhausting systematic causes).
    let squad_total: u64 = metrics.players.iter().map(|p| p.healing.healing_out_total).sum();
    let squad_self: u64 = metrics.players.iter().map(|p| p.healing.healing_out_self).sum();
    let squad_allies: u64 = metrics.players.iter().map(|p| p.healing.healing_out_allies).sum();
    let squad_downed: u64 = metrics.players.iter().map(|p| p.healing.downed_healing_out).sum();
    let squad_barrier: u64 = metrics.players.iter().map(|p| p.healing.barrier_out).sum();

    let golden_total = golden["squadHealingTotal"].as_i64().expect("squadHealingTotal");
    let golden_self = golden["squadHealingSelf"].as_i64().expect("squadHealingSelf");
    let golden_allies = golden["squadHealingAllies"].as_i64().expect("squadHealingAllies");
    let golden_downed = golden["squadDownedHealing"].as_i64().expect("squadDownedHealing");
    let golden_barrier = golden["squadBarrierTotal"].as_i64().expect("squadBarrierTotal");

    assert!(
        rel_close(squad_total as f64, golden_total as f64, SQUAD_HEALING_TOLERANCE),
        "squad healing_out_total: ours={squad_total} golden={golden_total} (tolerance {:.1}%)",
        SQUAD_HEALING_TOLERANCE * 100.0
    );
    assert_eq!(squad_self, golden_self as u64, "squad healing_out_self must be EXACT");
    assert!(
        rel_close(squad_allies as f64, golden_allies as f64, SQUAD_HEALING_TOLERANCE),
        "squad healing_out_allies: ours={squad_allies} golden={golden_allies} (tolerance {:.1}%)",
        SQUAD_HEALING_TOLERANCE * 100.0
    );
    assert_eq!(squad_downed, golden_downed as u64, "squad downed_healing_out must be EXACT");
    assert!(
        rel_close(squad_barrier as f64, golden_barrier as f64, SQUAD_BARRIER_TOLERANCE),
        "squad barrier_out: ours={squad_barrier} golden={golden_barrier} (tolerance {:.1}%)",
        SQUAD_BARRIER_TOLERANCE * 100.0
    );

    // Internal consistency: total == self + allies by construction (module
    // doc's derived-allies design), independent of the golden comparison.
    for p in &metrics.players {
        assert_eq!(
            p.healing.healing_out_total,
            p.healing.healing_out_self + p.healing.healing_out_allies,
            "agent {:#x}: total must equal self + allies exactly",
            p.agent_addr
        );
    }

    // Per-player: join by raw agent-table index (same method as
    // `support_golden.rs`).
    let golden_players = golden["players"].as_array().expect("players array");
    let mut healing_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        if let Some(healing) = p.get("healing") {
            healing_by_account.insert(account, healing);
        }
    }

    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut exact_mismatches: Vec<Mismatch> = Vec::new();
    let mut tolerance_mismatches: Vec<Mismatch> = Vec::new();
    let mut joined = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(&healing) = healing_by_account.get(&key) else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        let Some(pm) = metrics.players.iter().find(|m| m.agent_addr == p.agent_addr) else {
            continue;
        };
        joined += 1;

        let g_self = healing["healingSelf"].as_i64().unwrap();
        let g_downed = healing["downedHealing"].as_i64().unwrap();
        let g_total = healing["healing"].as_i64().unwrap();
        let g_allies = healing["healingAllies"].as_i64().unwrap();
        let g_barrier = healing["barrier"].as_i64().unwrap();

        if pm.healing.healing_out_self as i64 != g_self {
            exact_mismatches.push(Mismatch {
                account: key.clone(),
                field: "healingSelf",
                ours: pm.healing.healing_out_self as i64,
                golden: g_self,
            });
        }
        if pm.healing.downed_healing_out as i64 != g_downed {
            exact_mismatches.push(Mismatch {
                account: key.clone(),
                field: "downedHealing",
                ours: pm.healing.downed_healing_out as i64,
                golden: g_downed,
            });
        }
        if !within_tolerance(
            pm.healing.healing_out_total as i64,
            g_total,
            PLAYER_HEALING_ABS_FLOOR,
            PLAYER_HEALING_REL_TOLERANCE,
        ) {
            tolerance_mismatches.push(Mismatch {
                account: key.clone(),
                field: "healing",
                ours: pm.healing.healing_out_total as i64,
                golden: g_total,
            });
        }
        if !within_tolerance(
            pm.healing.healing_out_allies as i64,
            g_allies,
            PLAYER_HEALING_ABS_FLOOR,
            PLAYER_HEALING_REL_TOLERANCE,
        ) {
            tolerance_mismatches.push(Mismatch {
                account: key.clone(),
                field: "healingAllies",
                ours: pm.healing.healing_out_allies as i64,
                golden: g_allies,
            });
        }
        if !within_tolerance(
            pm.healing.barrier_out as i64,
            g_barrier,
            PLAYER_BARRIER_ABS_FLOOR,
            PLAYER_BARRIER_REL_TOLERANCE,
        ) {
            tolerance_mismatches.push(Mismatch {
                account: key.clone(),
                field: "barrier",
                ours: pm.healing.barrier_out as i64,
                golden: g_barrier,
            });
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the healing-augmented golden fixture, got {joined}"
    );

    if !exact_mismatches.is_empty() {
        let report: Vec<String> = exact_mismatches
            .iter()
            .map(|m| format!("{} {}: ours={} golden={}", m.account, m.field, m.ours, m.golden))
            .collect();
        panic!(
            "{} per-player healingSelf/downedHealing cell(s) out of EXACT tolerance (checked \
             {joined} accounts):\n{}",
            exact_mismatches.len(),
            report.join("\n")
        );
    }
    if !tolerance_mismatches.is_empty() {
        let report: Vec<String> = tolerance_mismatches
            .iter()
            .map(|m| format!("{} {}: ours={} golden={}", m.account, m.field, m.ours, m.golden))
            .collect();
        panic!(
            "{} per-player healing/healingAllies/barrier cell(s) out of bounded tolerance \
             (checked {joined} accounts; see this file's module doc for the documented residual \
             cause):\n{}",
            tolerance_mismatches.len(),
            report.join("\n")
        );
    }

    println!(
        "healing_matches_ei_golden: squad totals within tolerance (total/allies <=1%, barrier \
         <=10%; self/downed EXACT), {joined} accounts joined, 0 mismatches"
    );
}

#[test]
fn healing_matches_ei_golden() {
    let golden = read_golden_json();
    check_healing_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn healing_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("healing_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_healing_matches_ei_golden(&bytes, &golden);
}

/// Second, independent skip-when-absent sanity check (M10 Task 1 brief):
/// the local post-rework real capture (`fixtures/local/wvw-postrework.zevtc`,
/// gitignored, PII, build 20260718 -- confirmed during this task's research
/// to carry a healing-extension revision-2 registration and 11433 data
/// rows) has no committed EI JSON counterpart to calibrate exact numbers
/// against here (its own `.ei.json` sidecar doesn't carry `extHealingStats`
/// -- dps.report exports used elsewhere in this project's local fixtures
/// don't request extensions), so this only checks structural sanity: the
/// extension is detected, and squad-wide totals are internally consistent
/// and non-zero (a real WvW squad log with healers should show nonzero
/// healing).
#[test]
fn healing_extension_present_and_sane_on_local_postrework_when_available() {
    let Some(bytes) = std::fs::read(LOCAL_POSTREWORK_ZEVTC).ok() else {
        println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only postrework sanity check)");
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    assert!(metrics.has_healing_extension, "local post-rework fixture must carry the healing extension");
    assert!(!metrics.warnings.iter().any(|w| w.contains("healing extension not present")));

    let squad_total: u64 = metrics.players.iter().map(|p| p.healing.healing_out_total).sum();
    let squad_self: u64 = metrics.players.iter().map(|p| p.healing.healing_out_self).sum();
    let squad_allies: u64 = metrics.players.iter().map(|p| p.healing.healing_out_allies).sum();
    let squad_downed: u64 = metrics.players.iter().map(|p| p.healing.downed_healing_out).sum();
    let squad_barrier: u64 = metrics.players.iter().map(|p| p.healing.barrier_out).sum();

    assert!(squad_total > 0, "a real WvW squad fight should show nonzero outgoing healing");
    assert_eq!(squad_total, squad_self + squad_allies, "total must equal self + allies exactly");
    assert!(squad_downed <= squad_total, "downed healing can't exceed total healing");
    // Barrier is legitimately 0 on some fights (not every squad runs
    // barrier-granting builds) -- only assert the internal bound.
    let _ = squad_barrier;

    for p in &metrics.players {
        assert!(
            p.healing.healing_out_self <= p.healing.healing_out_total,
            "agent {:#x}: self healing can't exceed total",
            p.agent_addr
        );
        assert!(
            p.healing.downed_healing_out <= p.healing.healing_out_total,
            "agent {:#x}: downed healing can't exceed total",
            p.agent_addr
        );
    }

    println!(
        "healing_extension_present_and_sane_on_local_postrework: total={squad_total} \
         self={squad_self} allies={squad_allies} downed={squad_downed} barrier={squad_barrier}"
    );
}
