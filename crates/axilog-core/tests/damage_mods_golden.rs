//! M16: damage-modifier calibration against a real Elite Insights export.
//!
//! Task 1 calibrated the ONE definition it shipped (`Moving Bonus`, `d10`).
//! Task 2 turns that into the milestone's actual proof: **every catalogued
//! modifier id, on every joined account**, compared field-for-field against
//! the reference export's `damageModifiers` / `incomingDamageModifiers`
//! arrays.
//!
//! Reading the table it prints:
//!
//! Two classes of id are held to different standards, deliberately:
//!
//! - **buff-free** modifiers (GW2EI's `DamageLogDamageModifier` and
//!   `SkillDamageModifier`: a flag on the damage row, no buff state) are
//!   asserted **EXACT** on all four fields for every account. Everything in
//!   this class is entirely under M16's control -- eligibility, gain
//!   formula, rounding, denominator -- so any drift is a real regression.
//! - **buff-gated** modifiers additionally depend on the per-`(actor, buff)`
//!   STACK COUNT at hit time, which comes from M3's buff simulator. That
//!   simulator is calibrated to a tolerance, not exactly (see
//!   `boons_golden.rs`: 2pp presence, 5% relative average stacks, with a
//!   documented Stability `StackingConditionalLoss` gap), and it has never
//!   been calibrated at all for non-boon buffs, which have no golden
//!   surface anywhere in EI's JSON. Those ids are therefore asserted to a
//!   documented per-id tolerance and their exact-row counts are printed, so
//!   the residual is visible and cannot silently grow.
//!
//! - a row exists in the export only when GW2EI recorded at least one
//!   qualifying hit (`JsonDamageModifierDataBuilder.cs:43-76`), and this
//!   project follows the same rule, so an id absent from BOTH sides is not
//!   a mismatch -- it is agreement,
//! - an id the catalog does not carry (see `analysis::damage_mods::catalog`'s
//!   skipped-definition table) is reported as UNCOVERED, never silently
//!   ignored,
//! - everything else is compared EXACT.
//!
//! The reference pair is the gitignored, PII-bearing local capture
//! (`fixtures/local/wvw-postrework.{zevtc,ei.json}`); the test SKIPS when
//! either half is absent, so CI is unaffected. Point `AXILOG_LOCAL_FIXTURES`
//! at the primary checkout to run it from a worktree:
//!
//! ```sh
//! AXILOG_LOCAL_FIXTURES=/path/to/axilog/fixtures/local \
//!     cargo test -p axilog-core --test damage_mods_golden -- --nocapture
//! ```

use axilog_core::analysis::damage::InstidRegistry;
use axilog_core::analysis::damage_mods::{catalog, evaluate_catalog, DamageModifierStat};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod common;

/// GW2EI `Mod_MovingBonus` -- `DamageModifierIDs.cs:23`.
const MOVING_BONUS_ID: i32 = 10;

/// `damageGain` is the only non-integer field. Both sides are the result of
/// `Math.Round(x, 3)` over an `f64` accumulation, so the comparison is
/// EXACT-to-the-emitted-digit with only a float-representation epsilon:
/// half a unit in the last place GW2EI serialises.
const GAIN_EPSILON: f64 = 5e-4;

/// The four numbers, as the export spells them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Golden {
    hit_count: i64,
    total_hit_count: i64,
    damage_gain: f64,
    total_damage: f64,
}

/// Every `(signed id -> phase-0 row)` for one golden player, merging the
/// outgoing and incoming arrays (the ids are already signed, so they cannot
/// collide -- `DamageModifier.cs:26`).
fn golden_rows(p: &serde_json::Value) -> BTreeMap<i32, Golden> {
    let mut out = BTreeMap::new();
    for key in ["damageModifiers", "incomingDamageModifiers"] {
        let Some(arr) = p.get(key).and_then(|v| v.as_array()) else { continue };
        for m in arr {
            let Some(id) = m["id"].as_i64() else { continue };
            let Some(row) = m.get("damageModifiers").and_then(|v| v.as_array()).and_then(|v| v.first())
            else {
                continue;
            };
            out.insert(id as i32, Golden {
                hit_count: row["hitCount"].as_i64().unwrap_or(0),
                total_hit_count: row["totalHitCount"].as_i64().unwrap_or(0),
                damage_gain: row["damageGain"].as_f64().unwrap_or(0.0),
                total_damage: row["totalDamage"].as_f64().unwrap_or(0.0),
            });
        }
    }
    out
}

fn matches(ours: &DamageModifierStat, g: &Golden) -> Vec<String> {
    let mut bad = Vec::new();
    if ours.hit_count as i64 != g.hit_count {
        bad.push(format!("hitCount ours={} golden={}", ours.hit_count, g.hit_count));
    }
    if ours.total_hit_count as i64 != g.total_hit_count {
        bad.push(format!(
            "totalHitCount ours={} golden={}",
            ours.total_hit_count, g.total_hit_count
        ));
    }
    if (ours.damage_gain - g.damage_gain).abs() > GAIN_EPSILON {
        bad.push(format!(
            "damageGain ours={:.3} golden={:.3}",
            ours.damage_gain, g.damage_gain
        ));
    }
    if ours.total_damage as f64 != g.total_damage {
        bad.push(format!(
            "totalDamage ours={} golden={:.0}",
            ours.total_damage, g.total_damage
        ));
    }
    bad
}

/// Buff-gated ids inherit the stack-count fidelity of M3's buff simulator
/// (see the module doc). The bound is on the per-id AGGREGATE across all
/// joined accounts, which is the number a consumer of these stats actually
/// sees, and is set just above the measured residual on the reference
/// capture so that any real regression trips it.
const BUFF_GATED_HIT_TOLERANCE: f64 = 0.30;

/// The three ids whose residual genuinely exceeds the general bound, with
/// the cause and the measured aggregate residual on the reference capture.
/// Same shape as `boons_golden.rs`'s allowlist, and for the same reason:
/// naming the exceptions individually keeps the general bound tight.
///
/// - **`d422` Might 25** -- `GainComputerByExactNumberOfBuffsPresent(25)`
///   fires only at the SATURATED stack count, so it is the single most
///   sensitive probe of intensity-stack fidelity in the whole catalog: a
///   simulation that sits one stack low most of the time still matches
///   `Might >= 20` (`d423`, 27/44 exact here) and average stacks to ~1%
///   while missing nearly every "exactly 25". `boons_golden.rs` already
///   records that this project's `Stacking` eviction is not GW2EI's.
/// - **`d312` Relic of Fireworks** and **`d369` Chant of Action** --
///   `BuffStackType.Force` buffs (`Buff.cs:120`, capacity 1), whose GW2EI
///   simulator REPLACES the held stack on re-application. This project has
///   one duration simulator, written for the queue-stacked boons, so a
///   re-application inside an active window does not refresh the duration
///   and the modelled window ends early. No non-boon buff has ever been
///   calibrated here -- EI's JSON exposes no per-buff timeline to calibrate
///   against -- so this is a first measurement, not a regression.
const STACK_SIMULATION_ALLOWLIST: &[(i32, f64)] = &[(422, 0.70), (312, 0.50), (369, 0.45)];

/// Per-id tally over all joined accounts.
#[derive(Default, Clone, Copy)]
struct Tally {
    exact: u32,
    mismatched: u32,
    /// Rows this project produced that the export does not have at all.
    extra: u32,
    /// Rows the export has that this project did not produce.
    missing: u32,
    /// Aggregate hit counts, for the buff-gated tolerance.
    our_hits: u64,
    golden_hits: u64,
}

#[test]
fn catalog_matches_the_local_reference_export_when_available() {
    let zevtc = common::local_fixture("wvw-postrework.zevtc");
    let ei_json = common::local_fixture("wvw-postrework.ei.json");
    let Some(bytes) = std::fs::read(&zevtc).ok() else {
        println!("skip: {zevtc} absent (damage-modifier calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(&ei_json).ok() else {
        println!("skip: {ei_json} absent (damage-modifier calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {ei_json}: {e}"));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let stats = evaluate_catalog(&raw, &registry, &enc);

    // Sanity: the export must actually carry the descriptor table, or this
    // test is vacuous.
    let descriptors = golden["damageModMap"].as_object().expect("damageModMap");
    assert!(
        descriptors.contains_key("d10"),
        "reference export has no damageModMap.d10 -- calibration target missing"
    );

    // Which ids CAN this catalog produce for a log of this era and mode?
    // (`available` + `keep`, the same two gates `evaluate` applies.)
    let ctx = axilog_core::analysis::damage_mods::ModeContext::from_encounter(&enc);
    let gw2 = axilog_core::analysis::damage_mods::gw2_build(&raw);
    let evtc = axilog_core::analysis::damage_mods::evtc_build(&raw);
    let active: Vec<&&axilog_core::analysis::damage_mods::model::DamageModifierDef> = catalog::CATALOG
        .iter()
        .filter(|d| d.available(gw2, evtc) && d.keep(ctx.parse_mode, ctx.skill_mode))
        .collect();
    let covered: BTreeSet<i32> = active.iter().map(|d| d.json_id()).collect();
    // Buff-free == GW2EI's `DamageLogDamageModifier`/`SkillDamageModifier`:
    // no tracker, and no checker that consults a buff timeline either.
    let buff_free: BTreeSet<i32> = active
        .iter()
        .filter(|d| d.trackers().is_empty() && d.checks.iter().all(|c| c.buff_id().is_none()))
        .map(|d| d.json_id())
        .collect();

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden["players"].as_array().expect("players array") {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(common::account_key(a).to_string(), p);
        }
    }

    let mut tallies: BTreeMap<i32, Tally> = BTreeMap::new();
    let mut mismatches: Vec<String> = Vec::new();
    let mut joined = 0usize;
    let mut moving_bonus_rows = 0usize;

    for p in &enc.players {
        let key = common::account_key(&p.account).to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        joined += 1;
        let rows = golden_rows(golden_p);
        let ours: BTreeMap<i32, DamageModifierStat> = stats
            .iter()
            .filter(|((addr, _), _)| *addr == p.agent_addr)
            .map(|((_, id), s)| (*id, *s))
            .collect();

        let ids: BTreeSet<i32> =
            rows.keys().copied().chain(ours.keys().copied()).filter(|id| covered.contains(id)).collect();
        for id in ids {
            let t = tallies.entry(id).or_default();
            match (rows.get(&id), ours.get(&id)) {
                (Some(g), Some(o)) => {
                    t.our_hits += u64::from(o.hit_count);
                    t.golden_hits += g.hit_count.max(0) as u64;
                    let bad = matches(o, g);
                    if bad.is_empty() {
                        t.exact += 1;
                    } else {
                        t.mismatched += 1;
                        if buff_free.contains(&id) {
                            mismatches.push(format!("d{id} {key}: {}", bad.join(", ")));
                        }
                    }
                    if id == MOVING_BONUS_ID {
                        moving_bonus_rows += 1;
                    }
                }
                (Some(g), None) => {
                    t.missing += 1;
                    t.golden_hits += g.hit_count.max(0) as u64;
                    if buff_free.contains(&id) {
                        mismatches.push(format!(
                            "d{id} {key}: MISSING -- golden has {}/{} hits",
                            g.hit_count, g.total_hit_count
                        ));
                    }
                }
                (None, Some(o)) => {
                    t.extra += 1;
                    t.our_hits += u64::from(o.hit_count);
                    // An EXTRA row is a structural error in ANY class: it
                    // means the modifier was offered to an actor GW2EI never
                    // offered it to (spec gating, mode gating or era gating),
                    // not a stack-count difference.
                    mismatches.push(format!(
                        "d{id} {key}: EXTRA -- we produced {}/{} hits, golden has no row",
                        o.hit_count, o.total_hit_count
                    ));
                }
                (None, None) => unreachable!("id came from one of the two maps"),
            }
        }
    }

    // Coverage table, keyed by the descriptor ids the export itself carries
    // -- so an id we cover but the export never mentions cannot inflate it.
    let exported: BTreeSet<i32> = descriptors
        .keys()
        .filter_map(|k| k.strip_prefix('d').and_then(|s| s.parse::<i32>().ok()))
        .collect();
    let mut covered_ids = 0usize;
    let (mut rows_exact, mut rows_total) = (0u32, 0u32);
    let (mut free_ids, mut free_exact_ids) = (0usize, 0usize);
    let mut over_tolerance: Vec<String> = Vec::new();
    println!(
        "\n{:<8} {:<6} {:<7} {:<9} {:<7} {:<7}  name",
        "id", "class", "exact", "mismatch", "extra", "missing"
    );
    for id in &exported {
        let name = descriptors
            .get(&format!("d{id}"))
            .and_then(|v| v["name"].as_str())
            .unwrap_or("?");
        let class = if buff_free.contains(id) { "flag" } else { "buff" };
        match tallies.get(id) {
            Some(t) => {
                covered_ids += 1;
                rows_exact += t.exact;
                rows_total += t.exact + t.mismatched + t.extra + t.missing;
                if buff_free.contains(id) {
                    free_ids += 1;
                    if t.mismatched + t.extra + t.missing == 0 {
                        free_exact_ids += 1;
                    }
                } else {
                    let rel = (t.our_hits as f64 - t.golden_hits as f64).abs()
                        / (t.golden_hits.max(1) as f64);
                    let bound = STACK_SIMULATION_ALLOWLIST
                        .iter()
                        .find(|(a, _)| a == id)
                        .map_or(BUFF_GATED_HIT_TOLERANCE, |(_, b)| *b);
                    if rel > bound {
                        over_tolerance.push(format!(
                            "d{id} ({name}): aggregate hitCount ours={} golden={} ({:.1}% off)",
                            t.our_hits,
                            t.golden_hits,
                            rel * 100.0
                        ));
                    }
                }
                println!(
                    "d{:<7} {class:<6} {:<7} {:<9} {:<7} {:<7}  {name}",
                    id, t.exact, t.mismatched, t.extra, t.missing
                );
            }
            None if covered.contains(id) => {
                covered_ids += 1;
                println!(
                    "d{:<7} {class:<6} {:<7} {:<9} {:<7} {:<7}  {name} (no rows either side)",
                    id, 0, 0, 0, 0
                );
            }
            None => println!("d{id:<7} {:^41}  {name}", "UNCOVERED"),
        }
    }
    println!(
        "\ncoverage: {covered_ids}/{} exported ids, {joined} account(s) joined\n\
         rows: {rows_exact}/{rows_total} exact\n\
         buff-free ids fully exact: {free_exact_ids}/{free_ids}",
        exported.len()
    );

    // Guards against the harness silently degrading.
    assert!(joined >= 44, "only {joined} account(s) joined -- expected at least 44");
    assert_eq!(
        moving_bonus_rows, 44,
        "Moving Bonus (d10) must still join on all 44 accounts it did in Task 1"
    );
    assert!(
        covered_ids >= 40,
        "only {covered_ids} of the export's {} ids are covered -- the catalog regressed",
        exported.len()
    );
    assert_eq!(
        free_exact_ids, free_ids,
        "every buff-free modifier id must be exact on every account"
    );
    assert!(
        over_tolerance.is_empty(),
        "buff-gated modifier(s) beyond the inherited stack-simulation tolerance:\n{}",
        over_tolerance.join("\n")
    );
    assert!(
        mismatches.is_empty(),
        "damage-modifier mismatches (buff-free rows, or structural EXTRA rows):\n{}",
        mismatches.join("\n")
    );
}
