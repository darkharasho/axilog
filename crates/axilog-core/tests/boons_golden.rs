//! M3 Task 2: boon-uptime calibration against the EI golden fixture.
//!
//! Joins by raw agent-table index exactly like `golden.rs`'s
//! `professions_match_ei_golden` (`anon_account(i)` -> golden `account`),
//! then compares `analysis::buffs::uptime::compute`'s per-player,
//! per-boon `BoonUptime` (via `Metrics::boon_uptime`) against
//! `fixtures/wvw-small.ei.json` players[].boons[<id>] = {uptime, presence}
//! (EI's raw `buffUptimes[].buffData[0]` fields, verbatim -- see that
//! fixture's `_note` and `analysis::buffs::uptime`'s module docs for the
//! full field-meaning mapping).
//!
//! Tolerances (M3 Task 2 brief): duration-boon presence within 2
//! percentage points; intensity-boon average stacks within 5% relative
//! AND presence within 2 percentage points.
//!
//! M3 Task 4 additionally calibrates `Metrics::boon_generation`'s
//! `squad_pct` for 4 named boons (Might, Quickness, Alacrity, Stability)
//! against `fixtures/wvw-small.ei.json` players[].generation[<id>].squad
//! (EI's own `squadBuffs[boon].buffData[0].generation`, verbatim) -- see
//! `GENERATION_TOLERANCE_PP`/`check_boon_generation_matches_ei_golden`
//! below and `analysis::buffs::generation`'s module docs for the field
//! mapping.

use axilog_core::analysis::analyze;
use axilog_core::analysis::buffs::BOON_IDS;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

/// Duration-boon presence, and intensity-boon presence: percentage points.
///
/// M3's brief set this at 2pp and it has never been approached. Measured
/// worst cell after MBUFFSIM: **0.000455pp** (Quickness), i.e. 4400x inside
/// the bound. Deliberately NOT tightened to match: unlike the average-stack
/// integral, presence is a step-function boundary quantity, so a log with a
/// different event cadence (a raid boss rather than this WvW capture, or the
/// pre-rework era) can legitimately land further out without anything being
/// wrong. The average-stack tolerance below is the one MBUFFSIM earned the
/// right to tighten, because its residual has a named, now-eliminated cause.
const PRESENCE_TOLERANCE_PP: f64 = 2.0;

/// Intensity-boon average stacks: relative error.
///
/// **MBUFFSIM tightened this from `0.05` (5%) to `0.005` (0.5%).** M3 chose
/// 5% because Stability could not do better -- seven cells sat at 0.064-0.098
/// and had to be allowlisted (see [`INTENSITY_STACK_ALLOWLIST`]). Porting
/// GW2EI's `BuffsContainer.cs:196-252` band aid removed that cause entirely.
/// Measured on this fixture after MBUFFSIM:
///
/// Measured over ALL 37 cells per boon (not just the formerly-allowlisted
/// ones -- an early draft of this note quoted 0.000075 for Stability, which
/// was the max over those seven cells only and understated the real worst by
/// 6x):
///
/// | | worst cell | mean |
/// |---|---|---|
/// | Might | 0.000558 (`a133`) | 0.000035 |
/// | Stability | 0.000476 (`a133`) | 0.000066 |
///
/// `0.005` keeps a **~9x margin** over the worst cell in the fixture while
/// being **10x** tighter than the old bound (0.05 -> 0.005). Separately: the
/// OLD bound sat ~90x above that worst cell, which is exactly how the defect
/// MBUFFSIM fixed (Stability at 0.06+, Might at 0.0073) passed silently for
/// two milestones. It is set from the measurement WITH margin, not clamped to
/// it: a bound equal to the worst observed value is a bound that fails on the
/// next log.
const INTENSITY_STACK_RELATIVE_TOLERANCE: f64 = 0.005;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

/// Cells that cannot meet the 5% relative intensity-avg-stacks tolerance.
///
/// **EMPTY as of MBUFFSIM Task 2.** M3 Task 2 shipped seven entries here,
/// all Stability (`STABILITY == 1122`), all on the average-stack integral
/// (never presence, never Might, never a duration boon). M3 correctly
/// diagnosed the FAMILY -- GW2EI types Stability as
/// `BuffStackType.StackingConditionalLoss` -- and correctly established
/// that the divergence is NOT in the stack simulator (it grep'd
/// `BuffSimulator`/`OverrideLogic`/`ForceOverrideLogic` for a
/// conditional-loss branch and found none, which is right: GW2EI's
/// `BuffSimulator.cs:35-39` maps `StackingConditionalLoss` and `Stacking`
/// to the same `_overrideLogic`). What it could not find was the actual
/// rule, and it guessed the cause was arcdps-side data this project cannot
/// see. It was not: the rule is
/// `GW2EIEvtcParser/EIData/Buffs/BuffsContainer.cs:196-252`, GW2EI's "band
/// aid for the stack type situation with fake inactive/infinite durations",
/// which rewrites a conditional-loss strip's `RemovedDuration` from the
/// stack's ORIGINAL applied duration to its REMAINING duration before the
/// simulator's 15ms match ever runs. Ported in
/// `analysis::buffs::events::apply_conditional_loss_band_aid`.
///
/// Measured on this fixture, mean relative Stability average-stack error
/// against the EI golden: **0.031969 -> 0.000066**, and each of the seven
/// formerly-allowlisted cells individually:
///
/// | cell | before | after |
/// |---|---|---|
/// | a105 | 0.074670 | 0.000041 |
/// | a117 | 0.067951 | 0.000074 |
/// | a119 | 0.096168 | 0.000075 |
/// | a123 | 0.098174 | 0.000034 |
/// | a150 | 0.068835 | 0.000034 |
/// | a166 | 0.064276 | 0.000020 |
/// | a171 | 0.065096 | 0.000045 |
///
/// The worst average-stack cell in the whole fixture is now 0.000558
/// (Might), ~9x inside the (since-tightened, 0.005) tolerance -- it was 90x
/// inside the 0.05 bound this shipped with, which is why that bound was
/// tightened too. Keep this list EMPTY:
/// `allowlist_is_empty_and_unused` fails if an entry is added back without
/// a cell that actually needs it.
const INTENSITY_STACK_ALLOWLIST: &[(&str, u32)] = &[];
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
    boon: &'static str,
    kind: &'static str, // "presence" or "avg_stacks"
    ours: f64,
    golden: f64,
    delta: f64, // pp for presence, relative fraction for avg_stacks
    allowlisted: bool,
}

fn check_boon_uptimes_match_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let golden_players = golden["players"].as_array().expect("players array");
    let mut boons_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        if let Some(boons) = p.get("boons") {
            boons_by_account.insert(account, boons);
        }
    }

    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut checked = 0usize;
    let mut joined = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(&boons) = boons_by_account.get(&key) else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        joined += 1;

        for &(boon_id, name, is_intensity) in BOON_IDS.iter() {
            let Some(g) = boons.get(boon_id.to_string().as_str()) else { continue };
            let g_uptime = g["uptime"].as_f64().expect("boon uptime");
            let g_presence = g["presence"].as_f64().expect("boon presence");

            let ours = metrics.boon_uptime.get(&(p.agent_addr, boon_id)).copied().unwrap_or(
                axilog_core::analysis::buffs::BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 },
            );
            checked += 1;

            if is_intensity {
                // uptime field = avg stacks; presence field = presence %.
                let presence_delta = (ours.presence_pct - g_presence).abs();
                if presence_delta > PRESENCE_TOLERANCE_PP {
                    mismatches.push(Mismatch {
                        account: key.clone(), boon: name, kind: "presence",
                        ours: ours.presence_pct, golden: g_presence, delta: presence_delta,
                        allowlisted: false,
                    });
                }
                let rel = if g_uptime.abs() > 1e-9 {
                    (ours.avg_stacks - g_uptime).abs() / g_uptime.abs()
                } else {
                    (ours.avg_stacks - g_uptime).abs()
                };
                if rel > INTENSITY_STACK_RELATIVE_TOLERANCE {
                    let allowlisted = INTENSITY_STACK_ALLOWLIST.contains(&(key.as_str(), boon_id));
                    mismatches.push(Mismatch {
                        account: key.clone(), boon: name, kind: "avg_stacks",
                        ours: ours.avg_stacks, golden: g_uptime, delta: rel,
                        allowlisted,
                    });
                }
            } else {
                // uptime field IS the presence % for duration boons (see
                // `analysis::buffs::uptime` module docs).
                let delta = (ours.presence_pct - g_uptime).abs();
                if delta > PRESENCE_TOLERANCE_PP {
                    mismatches.push(Mismatch {
                        account: key.clone(), boon: name, kind: "presence",
                        ours: ours.presence_pct, golden: g_uptime, delta,
                        allowlisted: false,
                    });
                }
            }
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the boons-augmented golden fixture, got {joined}"
    );

    let hard_failures: Vec<&Mismatch> = mismatches.iter().filter(|m| !m.allowlisted).collect();
    let allowlisted_count = mismatches.len() - hard_failures.len();

    if !hard_failures.is_empty() {
        let mut sorted = hard_failures.clone();
        sorted.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap());
        let report: Vec<String> = sorted
            .iter()
            .take(20)
            .map(|m| {
                format!(
                    "{} {} {}: ours={:.3} golden={:.3} delta={:.3}{}",
                    m.account, m.boon, m.kind, m.ours, m.golden, m.delta,
                    if m.kind == "avg_stacks" { " (relative)" } else { "pp" }
                )
            })
            .collect();
        panic!(
            "{} boon-uptime cell(s) out of tolerance (checked {checked}, {allowlisted_count} allowlisted) -- worst offenders:\n{}",
            hard_failures.len(),
            report.join("\n")
        );
    }

    // MBUFFSIM Task 2: the allowlist is empty, and this asserts it STAYS
    // earned rather than merely declared -- an entry that no longer
    // suppresses a real mismatch is dead weight that would silently absorb
    // a future regression.
    let used: Vec<&(&str, u32)> = INTENSITY_STACK_ALLOWLIST
        .iter()
        .filter(|(acct, boon)| {
            // Match on the FULL allowlist key. `Mismatch` carries the boon's
            // display name rather than its id, so map the id back through
            // `BOON_IDS` -- comparing on the account alone (as this check
            // first did) would let an entry for the wrong boon look "used".
            let boon_name = BOON_IDS.iter().find(|(id, ..)| id == boon).map(|(_, n, _)| *n);
            mismatches.iter().any(|m| {
                m.allowlisted && m.account == *acct && Some(m.boon) == boon_name
            })
        })
        .collect();
    assert_eq!(
        used.len(),
        INTENSITY_STACK_ALLOWLIST.len(),
        "INTENSITY_STACK_ALLOWLIST has {} entry(ies) that suppress nothing -- remove them",
        INTENSITY_STACK_ALLOWLIST.len() - used.len()
    );

    println!(
        "boon_uptimes_match_ei_golden: {checked} cells checked across {joined} joined players, \
         0 hard failures ({allowlisted_count} allowlisted per INTENSITY_STACK_ALLOWLIST)"
    );
}

/// MBUFFSIM Task 2 pinned the `StackingConditionalLoss` allowlist shut. If
/// a future change needs it re-opened, that is a deliberate act with a
/// documented cause, not an accident.
#[test]
fn allowlist_is_empty_and_unused() {
    assert!(
        INTENSITY_STACK_ALLOWLIST.is_empty(),
        "the Stability avg-stack allowlist was emptied by MBUFFSIM Task 2 (the \
         `BuffsContainer.cs:196-252` band aid); re-adding an entry means a real \
         regression -- document the cause before changing this test"
    );
}

#[test]
fn boon_uptimes_match_ei_golden() {
    let golden = read_golden_json();
    check_boon_uptimes_match_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn boon_uptimes_match_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("boon_uptimes_match_ei_golden") else { return };
    let golden = read_golden_json();
    check_boon_uptimes_match_ei_golden(&bytes, &golden);
}

/// M3 Task 4: per-player SQUAD-generation calibration, for exactly the 4
/// boons the brief names (Might, Quickness, Alacrity, Stability -- 2
/// intensity, 2 duration), within 2 percentage points of EI's own
/// `squadBuffs[boon].buffData[0].generation` (extracted verbatim into
/// `fixtures/wvw-small.ei.json`'s `players[].generation[<id>].squad` --
/// see that fixture's `_note` and `analysis::buffs::generation`'s module
/// docs for the full field-meaning mapping/citations).
const GENERATION_TOLERANCE_PP: f64 = 2.0;

const GENERATION_CHECKED_BOONS: &[u32] = &[
    axilog_core::analysis::buffs::MIGHT,
    axilog_core::analysis::buffs::QUICKNESS,
    axilog_core::analysis::buffs::ALACRITY,
    axilog_core::analysis::buffs::STABILITY,
];

/// Cells that genuinely cannot meet the 2pp squad-generation tolerance.
/// Empty for now -- see the M3 Task 4 report if this ever needs entries
/// (the Stability `StackingConditionalLoss` gap already documented above,
/// `INTENSITY_STACK_ALLOWLIST`, is the first suspect for any future
/// Stability-generation failure, since generation is derived from the same
/// underlying stack simulation).
const GENERATION_ALLOWLIST: &[(&str, u32)] = &[];

struct GenMismatch {
    account: String,
    boon: u32,
    ours: f64,
    golden: f64,
    delta: f64,
    allowlisted: bool,
}

fn check_boon_generation_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let golden_players = golden["players"].as_array().expect("players array");
    let mut gen_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        if let Some(gen) = p.get("generation") {
            gen_by_account.insert(account, gen);
        }
    }

    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    let by_addr: HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut mismatches: Vec<GenMismatch> = Vec::new();
    let mut checked = 0usize;
    let mut joined = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(&gen) = gen_by_account.get(&key) else { continue };
        let Some(p) = by_addr.get(&agent.addr) else { continue };
        joined += 1;

        for &boon_id in GENERATION_CHECKED_BOONS {
            let Some(g) = gen.get(boon_id.to_string().as_str()) else { continue };
            let g_squad = g["squad"].as_f64().expect("generation.squad");

            let ours = metrics
                .boon_generation
                .get(&(p.agent_addr, boon_id))
                .copied()
                .unwrap_or_default();
            checked += 1;

            let delta = (ours.squad_pct - g_squad).abs();
            if delta > GENERATION_TOLERANCE_PP {
                let allowlisted = GENERATION_ALLOWLIST.contains(&(key.as_str(), boon_id));
                mismatches.push(GenMismatch {
                    account: key.clone(),
                    boon: boon_id,
                    ours: ours.squad_pct,
                    golden: g_squad,
                    delta,
                    allowlisted,
                });
            }
        }
    }

    assert!(
        joined >= 30,
        "expected at least 30 accounts to join to the generation-augmented golden fixture, got {joined}"
    );

    let hard_failures: Vec<&GenMismatch> = mismatches.iter().filter(|m| !m.allowlisted).collect();
    let allowlisted_count = mismatches.len() - hard_failures.len();

    if !hard_failures.is_empty() {
        let mut sorted = hard_failures.clone();
        sorted.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap());
        let report: Vec<String> = sorted
            .iter()
            .take(20)
            .map(|m| {
                format!(
                    "{} boon={}: ours={:.3} golden={:.3} delta={:.3}pp",
                    m.account, m.boon, m.ours, m.golden, m.delta
                )
            })
            .collect();
        panic!(
            "{} squad-generation cell(s) out of tolerance (checked {checked}, {allowlisted_count} allowlisted) -- worst offenders:\n{}",
            hard_failures.len(),
            report.join("\n")
        );
    }

    println!(
        "boon_generation_matches_ei_golden: {checked} cells checked across {joined} joined players, \
         0 hard failures ({allowlisted_count} allowlisted per GENERATION_ALLOWLIST)"
    );
}

#[test]
fn boon_generation_matches_ei_golden() {
    let golden = read_golden_json();
    check_boon_generation_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn boon_generation_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("boon_generation_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_boon_generation_matches_ei_golden(&bytes, &golden);
}
