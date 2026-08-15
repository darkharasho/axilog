//! MEIGAP2: ei-json calibration for the six audit rows this milestone
//! closes, against the real Elite Insights export for this project's local
//! post-rework WvW capture (`fixtures/local/wvw-postrework.ei.json`,
//! gitignored -- every test here skips cleanly when it is absent, the same
//! `AXILOG_LOCAL_FIXTURES` pattern the other `*_ei_golden.rs` files use).
//!
//! Rows, with the GW2EI semantics each is calibrated against:
//!
//! 1. `players[].totalDamageDist[][]` / `totalDamageTaken[][]` outcome
//!    columns (`JsonDamageDistBuilder.cs:48-77`) plus the per-skill
//!    `downContribution` (`OffensiveStatistics.cs:81-108` -- a DIFFERENT
//!    algorithm on this side by design, see the test).
//! 2. `players[].healthPercents` (`JsonActorBuilder.cs:90-100` ->
//!    `SingleActorGraphsHelper.ListFromStates`).
//! 3. `instanceID` on players and targets (`JsonActorBuilder.cs:31`).
//! 4. `players[].boonsStates` (`JsonActorBuilder.cs:93` ->
//!    `BuffGraph.MergePresenceInto`).
//! 5. `targets[].dpsAll[0].damage` (`JsonActorBuilder.cs:46` ->
//!    `DamageStatistics`, minion-inclusive, `!ToFriendly`).
//! 6. `players[].dpsAll[0].breakbarDamage` (`DamageStatistics.cs:60`).
//!
//! The targets join is the same instid-based one `meigap2_ei_golden.rs`
//! established (GW2EI curates its WvW target list; the two name spaces do
//! not intersect, so identity goes through the arcdps instid GW2EI encodes
//! into its `"<Spec> pl-<instid>"` placeholder names).

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_ei::EiInputs;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

struct Calibration {
    ours: Value,
    golden: Value,
    /// `(our targets[] index, reference targets[] index)` for every target
    /// both sides agree is the same arcdps agent.
    joinable: Vec<(usize, usize)>,
}

/// Parses the local capture and renders it with every flag-gated block on,
/// mirroring axibridge's own settings mapping (it forces `skillDamage` and
/// passes `rawTimelineArrays` through as `timeseries`).
fn render_and_reference(label: &str) -> Option<Calibration> {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let Ok(bytes) = std::fs::read(&zevtc) else {
        println!("skip: {zevtc} absent ({label})");
        return None;
    };
    let Ok(golden_s) = std::fs::read_to_string(&ei_json) else {
        println!("skip: {ei_json} absent ({label})");
        return None;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    let health_percents = axilog_core::analysis::health::ei_health_percents(&raw, &enc);
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes { boon_states: Some(&boon_states), activity: Some(&activity),
            health_percents: Some(&health_percents),
            // Task 9: the outcome columns enter through the native damage
            // block now, not through `EiInputs`.
            dist_outcomes: Some(&dist_outcomes),
            ..Default::default()
        },
    );


    let ours = axilog_ei::to_ei_json(
        &report_v1, &report,
        &EiInputs {
            ..Default::default()
        },
    );

    // --- the instid-based targets join (see this file's module doc) ---
    let mut instid_to_addrs: BTreeMap<u16, BTreeSet<u64>> = BTreeMap::new();
    for ev in &raw.events {
        for (instid, addr) in [(ev.src_instid, ev.src_agent), (ev.dst_instid, ev.dst_agent)] {
            if instid != 0 && addr != 0 {
                instid_to_addrs.entry(instid).or_default().insert(addr);
            }
        }
    }
    // MROSTER: `targets[]` is the CURATED roster (`Report::ei_targets` --
    // enemy PLAYERS only, per `WvWLogic.cs`), so an index into
    // `enc.enemies` is no longer an index into `targets[]`. Route the join
    // through the enemy's representative addr, which the adapter emits
    // verbatim as `targets[].id`: that keeps this join correct for whatever
    // the curation rule is, instead of re-encoding the rule here. An enemy
    // absent from the curated roster simply yields no index, so the golden
    // comparison skips it rather than joining to the wrong row.
    let target_index_by_id: BTreeMap<u64, usize> = ours["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t["id"].as_u64().map(|id| (id, i)))
        .collect();
    let addr_to_enemy_index: BTreeMap<u64, usize> = enc
        .enemies
        .iter()
        .filter_map(|e| target_index_by_id.get(&e.id).map(|&i| (e, i)))
        .flat_map(|(e, i)| e.agent_addrs.iter().map(move |&a| (a, i)))
        .collect();
    let mut joinable: Vec<(usize, usize)> = Vec::new();
    for (g_i, t) in golden["targets"].as_array().expect("reference targets").iter().enumerate() {
        let Some(instid) = t["name"]
            .as_str()
            .and_then(|n| n.rsplit_once("pl-"))
            .and_then(|(_, s)| s.parse::<u16>().ok())
        else {
            continue;
        };
        let indices: BTreeSet<usize> = instid_to_addrs
            .get(&instid)
            .into_iter()
            .flatten()
            .filter_map(|a| addr_to_enemy_index.get(a).copied())
            .collect();
        if indices.len() == 1 {
            joinable.push((*indices.iter().next().expect("len == 1"), g_i));
        }
    }

    Some(Calibration { ours, golden, joinable })
}

fn players_by_account(v: &Value) -> BTreeMap<String, &Value> {
    v["players"]
        .as_array()
        .expect("players array")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a).to_string(), p)))
        .collect()
}

/// `dist[0]` as `skill id -> row`.
fn dist_rows(v: &Value) -> BTreeMap<i64, &Value> {
    v[0].as_array()
        .map(|a| a.iter().filter_map(|r| r["id"].as_i64().map(|id| (id, r))).collect())
        .unwrap_or_default()
}

fn num(v: &Value, key: &str) -> i64 {
    v[key].as_i64().unwrap_or(0)
}


/// The one incoming skill id this project emits a `totalDamageTaken` row
/// for that the reference has not -- the pre-existing, already-pinned
/// residual (see [`ei_json_taken_dist_outcome_columns_match_the_reference_export_when_available`]).
const KNOWN_TAKEN_PHANTOM_SKILL: i64 = 23279;
/// Measured row/cell bounds; see each test's doc comment.
const TAKEN_EXTRA_ROW_BOUND: usize = 25;
const OUTGOING_COUNT_DIFF_BOUND: usize = 8;
const OUTGOING_COUNT_DIFF_MAGNITUDE: i64 = 20;
const OUTGOING_MISSING_ROW_BOUND: usize = 5;
const OUTGOING_EXTRA_ROW_BOUND: usize = 40;
/// Measured bound on the `boonsAppliedCount` scalar; see its test.
const BOONS_APPLIED_TOLERANCE: f64 = 0.10;


// ---------------------------------------------------------------------
// Row 1: the two player-side distributions' outcome columns
// ---------------------------------------------------------------------

/// The five outcome columns the damage-mitigation table reads, plus
/// `connectedHits`/`hits`/`indirectDamage`, on the INCOMING distribution --
/// asserted EXACT, cell for cell, over every skill id the reference has.
///
/// Measured: **18,000 of 18,000 cells EXACT across 44 accounts, 0 reference
/// rows missing**. The only row-set difference is in the other direction:
/// this project emits 23 rows the reference has not, every one of them skill
/// `23279` -- the SAME pre-existing incoming-damage residual
/// `skill_damage_golden.rs`'s `TAKEN_SUM_ABS_TOLERANCE`,
/// `timeseries_golden.rs`'s `TAKEN_ABS_TOLERANCE` and MEIGAP Task 2a's
/// bucket identity already pin, now visible as a row rather than as a
/// handful of damage points. It is bounded here, not silently allowed.
#[test]
fn ei_json_taken_dist_outcome_columns_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json totalDamageTaken outcomes") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let cols = [
        "hits", "connectedHits", "glance", "missed", "evaded", "blocked", "invulned",
        "interrupted",
    ];
    let (mut cells, mut joined, mut missing, mut extra) = (0usize, 0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        joined += 1;
        let orows = dist_rows(&o["totalDamageTaken"]);
        let grows = dist_rows(&g["totalDamageTaken"]);
        for (id, gr) in &grows {
            let Some(or) = orows.get(id) else {
                missing += 1;
                failures.push(format!("{account}: reference skill {id} has no row here"));
                continue;
            };
            for col in cols {
                cells += 1;
                if num(or, col) != num(gr, col) {
                    failures.push(format!(
                        "{account} skill {id} {col}: {} vs reference {}",
                        num(or, col),
                        num(gr, col)
                    ));
                }
            }
            if or["indirectDamage"].as_bool() != gr["indirectDamage"].as_bool() {
                failures.push(format!("{account} skill {id} indirectDamage"));
            }
        }
        for id in orows.keys() {
            if !grows.contains_key(id) {
                extra += 1;
                assert_eq!(
                    *id, KNOWN_TAKEN_PHANTOM_SKILL,
                    "{account}: emitted skill {id} the reference has no row for -- only the \
                     already-pinned skill {KNOWN_TAKEN_PHANTOM_SKILL} residual is allowed"
                );
            }
        }
    }
    println!(
        "totalDamageTaken: {joined} accounts, {cells} cells, {missing} missing rows, {extra} extra rows"
    );
    assert!(joined >= 40, "expected the reference's squad to join, got {joined}");
    assert_eq!(missing, 0, "every reference row must exist here");
    assert!(
        extra <= TAKEN_EXTRA_ROW_BOUND,
        "{extra} extra rows exceeds the measured bound {TAKEN_EXTRA_ROW_BOUND}"
    );
    assert!(failures.is_empty(), "{} cell mismatches: {:?}", failures.len(), &failures[..failures.len().min(10)]);
}

/// The OUTGOING distribution's same columns. Exact on every column except
/// the two counts the pet/minion fold can move (`hits`/`connectedHits`),
/// which are bounded by row count and magnitude.
///
/// This project's outgoing dist folds friendly pet/minion damage onto the
/// owner (the documented M12 divergence) where GW2EI's player dist is
/// actor-only, so the two row sets genuinely differ; the outcome columns
/// inherit exactly that and nothing else. Measured (the figures this test
/// prints): **5,340 joined cells over the six exact columns, all EXACT, and
/// 0 `indirectDamage` disagreements; 8 `hits`/`connectedHits` cells differ
/// (worst 17); 3 reference rows absent here; 30 rows emitted the reference
/// has not.**
///
/// The 3 absent reference rows are bounded, not root-caused. The best
/// hypothesis is GW2EI's SECOND dist builder: `JsonDamageDistBuilder.cs:
/// 84-100` emits a row for a skill that produced BREAKBAR events but no
/// health-damage ones at all, incrementing `Hits`/`ConnectedHits` once per
/// breakbar event. This project's row existence is gated on
/// `skill_damage::creates_health_damage_event`, which by design excludes
/// the breakbar result byte -- so a breakbar-only skill has no row on this
/// side. That fits the shape of what is missing (`total: 0` with a small
/// nonzero `connectedHits`), and it is a row set GW2EI itself builds from a
/// different event list, not a miscount of the one this pass reads.
#[test]
fn ei_json_outgoing_dist_outcome_columns_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json totalDamageDist outcomes") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let exact_cols = ["glance", "missed", "evaded", "blocked", "invulned", "interrupted"];
    let (mut cells, mut missing, mut extra, mut count_diffs) = (0usize, 0usize, 0usize, 0usize);
    let mut worst_count_diff = 0i64;
    let mut failures: Vec<String> = Vec::new();
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        let orows = dist_rows(&o["totalDamageDist"]);
        let grows = dist_rows(&g["totalDamageDist"]);
        for (id, gr) in &grows {
            let Some(or) = orows.get(id) else {
                missing += 1;
                continue;
            };
            for col in exact_cols {
                cells += 1;
                if num(or, col) != num(gr, col) {
                    failures.push(format!(
                        "{account} skill {id} {col}: {} vs reference {}",
                        num(or, col),
                        num(gr, col)
                    ));
                }
            }
            if or["indirectDamage"].as_bool() != gr["indirectDamage"].as_bool() {
                failures.push(format!("{account} skill {id} indirectDamage"));
            }
            for col in ["hits", "connectedHits"] {
                let d = num(or, col) - num(gr, col);
                if d != 0 {
                    count_diffs += 1;
                    worst_count_diff = worst_count_diff.max(d.abs());
                }
            }
        }
        extra += orows.keys().filter(|id| !grows.contains_key(id)).count();
    }
    println!(
        "totalDamageDist: {cells} exact-column cells, {count_diffs} hit-count diffs (worst \
         {worst_count_diff}), {missing} missing rows, {extra} extra rows"
    );
    assert!(failures.is_empty(), "{} mismatches: {:?}", failures.len(), &failures[..failures.len().min(10)]);
    assert!(
        count_diffs <= OUTGOING_COUNT_DIFF_BOUND && worst_count_diff <= OUTGOING_COUNT_DIFF_MAGNITUDE,
        "hit-count divergence grew: {count_diffs} cells, worst {worst_count_diff}"
    );
    assert!(missing <= OUTGOING_MISSING_ROW_BOUND, "{missing} reference rows absent");
    assert!(extra <= OUTGOING_EXTRA_ROW_BOUND, "{extra} rows emitted the reference has not");
}

/// Per-skill `downContribution`: a DIVERGENCE, calibrated as one.
///
/// GW2EI's number is "damage dealt inside the victim's 90%-to-downstate
/// window" (`OffensiveStatistics.cs:81-108`); this project's is the arcdps
/// methodology's over-99%-anchor window minus a 2s lead-in
/// (`axilog_core::analysis::contribution`) -- the founding differentiator,
/// and the same divergence `statsAll[0].downContribution` and
/// `statsTargets[i][0].downContribution` already carry. There is therefore
/// nothing to match here, and pretending otherwise with a tolerance would
/// be dishonest.
///
/// What IS asserted is the property that makes the per-skill split
/// trustworthy: it sums back to the scalar EXACTLY, per player. The overlap
/// against the reference is measured and printed, not asserted as parity:
/// 344 skills both sides credit (114 identically), 53 only this side, 36
/// only GW2EI's.
#[test]
fn ei_json_per_skill_down_contribution_sums_to_the_scalar() {
    let Some(c) = render_and_reference("ei-json per-skill downContribution") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let (mut both, mut same, mut ours_only, mut ref_only) = (0usize, 0usize, 0usize, 0usize);
    for (account, o) in &ours {
        let per_skill: i64 = o["totalDamageDist"][0]
            .as_array()
            .map(|rows| rows.iter().filter_map(|r| r["downContribution"].as_i64()).sum())
            .unwrap_or(0);
        let scalar = o["statsAll"][0]["downContribution"].as_i64().unwrap_or(-1);
        assert_eq!(
            per_skill, scalar,
            "{account}: the per-skill downContribution split must sum to \
             statsAll[0].downContribution exactly -- they are the same credits"
        );
        let Some(g) = golden.get(account) else { continue };
        let orows = dist_rows(&o["totalDamageDist"]);
        for (id, gr) in dist_rows(&g["totalDamageDist"]) {
            let g_dc = gr["downContribution"].as_i64().filter(|&v| v > 0);
            let o_dc = orows.get(&id).and_then(|r| r["downContribution"].as_i64());
            match (o_dc, g_dc) {
                (Some(a), Some(b)) => {
                    both += 1;
                    if a == b {
                        same += 1;
                    }
                }
                (Some(_), None) => ours_only += 1,
                (None, Some(_)) => ref_only += 1,
                (None, None) => {}
            }
        }
    }
    println!(
        "per-skill downContribution vs GW2EI's own algorithm: both={both} identical={same} \
         ours_only={ours_only} reference_only={ref_only} (a measurement, not a parity claim)"
    );
    assert!(both > 0, "the two algorithms should at least agree on which skills matter");
}

// ---------------------------------------------------------------------
// Row 2: healthPercents
// ---------------------------------------------------------------------

/// `healthPercents` is asserted BYTE-EXACT: the whole array, pair for pair,
/// for every joined account. Measured: **44 of 44 accounts identical**,
/// which is the strongest form this file can state and is what
/// `ListFromStates`' clamp/empty-removal/fuse sequence being transcribed
/// literally buys.
#[test]
fn ei_json_health_percents_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json healthPercents") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let mut joined = 0usize;
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        joined += 1;
        assert_eq!(
            o["healthPercents"], g["healthPercents"],
            "{account}: healthPercents must reproduce GW2EI's own step function exactly"
        );
    }
    assert!(joined >= 40, "expected the reference's squad to join, got {joined}");
}

// ---------------------------------------------------------------------
// Row 3: instanceID
// ---------------------------------------------------------------------

/// `instanceID` on both actor kinds. Measured: **44 of 44 players and 43 of
/// 43 instid-joined targets EXACT**. (The target side is a partial
/// tautology by construction -- the join itself goes through the instid
/// GW2EI encodes into the placeholder name -- but it is not a full one: it
/// asserts that the instid this project independently reads off the event
/// stream for that enemy agent is the same one GW2EI assigned it.)
#[test]
fn ei_json_instance_ids_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json instanceID") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let mut joined = 0usize;
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        joined += 1;
        assert_eq!(o["instanceID"], g["instanceID"], "{account}: players[].instanceID");
    }
    assert!(joined >= 40, "expected the reference's squad to join, got {joined}");

    let ot = c.ours["targets"].as_array().expect("targets");
    let gt = c.golden["targets"].as_array().expect("reference targets");
    for &(o_i, g_i) in &c.joinable {
        assert_eq!(
            ot[o_i]["instanceID"], gt[g_i]["instanceID"],
            "targets[{o_i}] ({}) instanceID",
            gt[g_i]["name"]
        );
    }
    assert!(c.joinable.len() >= 40, "expected the instid-joinable targets, got {}", c.joinable.len());
}

// ---------------------------------------------------------------------
// Row 4: boonsStates -> boonsAppliedCount
// ---------------------------------------------------------------------

/// `boonsStates`, calibrated on the scalar the consumer actually derives
/// from it (`boonsAppliedCount` -- the sum of the series' positive deltas,
/// `axibridge src/main/detailsProcessing.ts:128-142`), not on the array.
///
/// The array itself is a reduction of the same per-boon timelines
/// `buffUptimes[].states` publishes, so it carries that family's already
/// calibrated M3 simulation-timing residual: 28 of 44 accounts reproduce
/// the reference array pair-for-pair, and the rest differ by transitions
/// landing tens of milliseconds apart (e.g. 28659 here vs 28553 there).
/// That does not move the count, which is what is asserted: **43 of 44
/// accounts EXACT, one off by 4 of 101 (4.0%)**.
#[test]
fn ei_json_boons_applied_count_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json boonsStates") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let applied = |states: &Value| -> i64 {
        let mut prev: Option<i64> = None;
        let mut sum = 0;
        for row in states.as_array().into_iter().flatten() {
            let v = row[1].as_i64().unwrap_or(0);
            if let Some(p) = prev {
                if v > p {
                    sum += v - p;
                }
            }
            prev = Some(v);
        }
        sum
    };
    let (mut joined, mut exact, mut arrays_exact) = (0usize, 0usize, 0usize);
    let mut worst = 0.0f64;
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        joined += 1;
        if o["boonsStates"] == g["boonsStates"] {
            arrays_exact += 1;
        }
        let a = applied(&o["boonsStates"]) as f64;
        let b = applied(&g["boonsStates"]) as f64;
        assert!(b > 0.0, "{account}: the reference should report boon applications");
        if (a - b).abs() < f64::EPSILON {
            exact += 1;
        }
        let rel = (a - b).abs() / b;
        worst = worst.max(rel);
        assert!(rel <= BOONS_APPLIED_TOLERANCE, "{account}: boonsAppliedCount {a} vs {b}");
    }
    println!(
        "boonsAppliedCount: {exact}/{joined} exact (arrays pair-for-pair: {arrays_exact}), worst \
         relative delta {worst:.3}"
    );
    assert!(exact * 100 >= joined * 90, "expected >=90% of accounts exact, got {exact}/{joined}");
}

// ---------------------------------------------------------------------
// Rows 5 and 6: the two dpsAll scalars
// ---------------------------------------------------------------------

/// `targets[].dpsAll[0].damage`. Measured: **53 of 56 instid-joined targets
/// EXACT**, which required reproducing both of GW2EI's scoping rules --
/// the `!ToFriendly` `iff` filter and the minion fold (an enemy ranger's pet
/// damage counts for the ranger, even though this project's own enemy
/// roster lists that pet as an enemy of its own).
///
/// MINSTID grew the join from 43 targets to all 56 (the 13 instids that
/// used to carry two of this project's `targets[]` rows each were ambiguous
/// and skipped; they are now one row, GW2EI-style -- see
/// `axilog_core::wvw::dedupe_enemy_players`). 10 of the 13 newly-joined
/// targets are exact. The other 3 are [`RESIDUAL_INSTIDS`], the same three
/// carried by `meigap2_ei_golden`'s series test -- see that test's doc for
/// the diagnosis: a damage-CREDIT divergence that predates MINSTID (each
/// merged total is the exact sum of its pre-merge parts) and was simply
/// invisible to this join until the rows merged.
#[test]
fn ei_json_target_dps_all_damage_matches_the_reference_export_when_available() {
    /// See `meigap2_ei_golden::ei_json_target_series_...`; kept in lockstep
    /// with that list.
    const RESIDUAL_INSTIDS: &[i64] = &[3483, 3954, 4952];

    let Some(c) = render_and_reference("ei-json targets[].dpsAll") else { return };
    let ot = c.ours["targets"].as_array().expect("targets");
    let gt = c.golden["targets"].as_array().expect("reference targets");
    let mut nonzero = 0usize;
    let mut residual_seen: BTreeSet<i64> = BTreeSet::new();
    for &(o_i, g_i) in &c.joinable {
        let a = ot[o_i]["dpsAll"][0]["damage"].as_i64().unwrap_or(-1);
        let b = gt[g_i]["dpsAll"][0]["damage"].as_i64().unwrap_or(-2);
        if b > 0 {
            nonzero += 1;
        }
        let instid = ot[o_i]["instanceID"].as_i64().expect("instanceID");
        if RESIDUAL_INSTIDS.contains(&instid) {
            if a != b {
                residual_seen.insert(instid);
            }
            continue;
        }
        assert_eq!(a, b, "target {} dpsAll[0].damage", gt[g_i]["name"]);
    }
    println!(
        "targets dpsAll[0].damage: {} joined, {nonzero} nonzero, {} allowlisted residual",
        c.joinable.len(),
        RESIDUAL_INSTIDS.len()
    );
    assert!(nonzero >= 30, "expected most joined targets to have dealt damage, got {nonzero}");
    let expected: BTreeSet<i64> = RESIDUAL_INSTIDS.iter().copied().collect();
    assert_eq!(
        residual_seen, expected,
        "RESIDUAL_INSTIDS is stale: these instids no longer diverge and must be removed"
    );
}

/// `players[].dpsAll[0].breakbarDamage`. Measured: **44 of 44 accounts
/// EXACT**, including the 27 with a nonzero value and the ones whose total
/// differs from GW2EI's own `actorBreakbarDamage` (i.e. where the minion
/// fold is load-bearing), and including the `/10` raw-unit conversion.
#[test]
fn ei_json_breakbar_damage_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json dpsAll[0].breakbarDamage") else { return };
    let ours = players_by_account(&c.ours);
    let golden = players_by_account(&c.golden);
    let (mut joined, mut nonzero, mut minion_folded) = (0usize, 0usize, 0usize);
    for (account, o) in &ours {
        let Some(g) = golden.get(account) else { continue };
        joined += 1;
        let a = o["dpsAll"][0]["breakbarDamage"].as_f64().unwrap_or(-1.0);
        let b = g["dpsAll"][0]["breakbarDamage"].as_f64().unwrap_or(-2.0);
        if b > 0.0 {
            nonzero += 1;
        }
        if g["dpsAll"][0]["actorBreakbarDamage"].as_f64().unwrap_or(0.0) != b {
            minion_folded += 1;
        }
        assert!(
            (a - b).abs() < 1e-9,
            "{account}: dpsAll[0].breakbarDamage {a} vs reference {b}"
        );
    }
    println!(
        "breakbarDamage: {joined} accounts exact, {nonzero} nonzero, {minion_folded} where the \
         minion fold moves the number"
    );
    assert!(nonzero >= 20, "expected the reference's nonzero rows, got {nonzero}");
}
