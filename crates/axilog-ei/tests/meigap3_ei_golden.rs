//! MEIGAP Task 3: ei-json calibration for the healing/barrier detail
//! families, `minions[]`, `guildID` and the outgoing `boonStripsTime`,
//! against the real Elite Insights export for this project's local
//! post-rework WvW capture (`fixtures/local/wvw-postrework.ei.json`,
//! gitignored -- every test here skips cleanly when it is absent, the same
//! `AXILOG_LOCAL_FIXTURES` pattern `meigap_ei_golden.rs` /
//! `meigap2_ei_golden.rs` use).
//!
//! Families:
//!
//! a. `extHealingStats.outgoingHealingAllies` / `.totalHealingDist` /
//!    `.healing1S`, `extBarrierStats.outgoingBarrierAllies` /
//!    `.totalBarrierDist`
//!    (`EXTJsonPlayerHealingStatsBuilder.cs`). See
//!    `axilog_core::analysis::healing_detail`.
//! b. `minions[].totalDamageTakenDist` (`JsonMinionsBuilder.cs`). See
//!    `axilog_core::analysis::minions`.
//! c. `guildID` (`JsonPlayerBuilder.cs:46-50`). See
//!    `axilog_core::wvw::guilds`.
//! e. `support[0].boonStripsTime` (`SupportPerAllyStatistics.cs`) --
//!    reconstructed against GW2EI's own buggy `Math.Max` accumulator, the
//!    same treatment MEIGAP Task 1c gave the incoming twin.
//!
//! ## The inherited healing residual
//!
//! M10's `healing_golden.rs` records a bounded, root-caused residual on
//! this project's healing totals: a handful of accounts cast a REPEATING
//! skill whose peer-relayed copies straddle GW2EI's all-or-nothing
//! `SanitizeForSrc` rule in a way a byte-level replication of just that
//! rule cannot reproduce (see `analysis::healing`'s module doc). Every
//! family here is downstream of exactly those events, so it inherits
//! exactly that residual and nothing new. The tests below therefore assert
//! (i) row/key SETS exactly, (ii) the internal consistency of the three
//! groupings against each other exactly, and (iii) the reference join
//! within pinned bounds measured from this capture -- so a NEW divergence
//! fails while the known one does not.

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
    /// Accounts present on both sides, as `(our players[] index,
    /// reference players[] index)`.
    joined: Vec<(usize, usize)>,
}

/// Parses the local capture and renders it with every flag-gated block on
/// (mirroring axibridge's own `mapEiSettingsToAxilogOptions`).
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
    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let healing_detail =
        axilog_core::analysis::healing_detail::build_with_registry(&raw, &registry, &enc);
    let minion_rollups =
        axilog_core::analysis::minions::build_with_registry(&raw, &registry, &enc);
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes { activity: Some(&activity),
            minions: Some(&minion_rollups),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            ..Default::default()
        },
    );

    let ours = axilog_ei::to_ei_json(
        &report_v1, &report,
        &EiInputs {
            ..Default::default()
        },
    );

    let our_idx: BTreeMap<String, usize> = ours["players"]
        .as_array()
        .expect("players")
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p["account"].as_str().map(|a| (account_key(a).to_string(), i)))
        .collect();
    let mut joined: Vec<(usize, usize)> = Vec::new();
    for (g_i, p) in golden["players"].as_array().expect("players").iter().enumerate() {
        let Some(acc) = p["account"].as_str() else { continue };
        if let Some(&o_i) = our_idx.get(account_key(acc)) {
            joined.push((o_i, g_i));
        }
    }
    assert!(joined.len() >= 40, "expected a well-joined roster, got {}", joined.len());
    Some(Calibration { ours, golden, joined })
}

fn num(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64)).unwrap_or(0)
}

/// `[[{...}, ...]]` (EI's `[phase][row]`) -> phase 0's rows keyed by `id`.
fn dist_rows(v: &Value) -> BTreeMap<i64, &Value> {
    v[0].as_array()
        .map(|a| a.iter().filter_map(|r| r["id"].as_i64().map(|id| (id, r))).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------
// (a) healing / barrier detail
// ---------------------------------------------------------------------

/// `outgoingHealingAllies` / `outgoingBarrierAllies`: the per-(healer,
/// ally) matrix, joined by ACCOUNT on both axes.
///
/// The reference's ally axis is `log.Friendlies` and ours is
/// `report.players`; both are indexed positionally against the same
/// `players[]` array each document carries, which is exactly how
/// axibridge reads it (`players[allyIdx]`), so the join is
/// account-to-account on both the outer and the inner index.
#[test]
fn ei_json_outgoing_healing_allies_matches_the_reference() {
    let Some(c) = render_and_reference("outgoingHealingAllies") else { return };

    let our_acc: Vec<String> = c.ours["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| account_key(p["account"].as_str().unwrap_or("")).to_string())
        .collect();
    let ref_acc: Vec<String> = c.golden["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| account_key(p["account"].as_str().unwrap_or("")).to_string())
        .collect();

    let mut cells = 0usize;
    let mut nonzero = 0usize;
    let mut differing = 0usize;
    let mut worst = 0i64;
    let mut worst_desc = String::new();
    let mut b_cells = 0usize;
    let mut b_differing = 0usize;
    let mut b_worst = 0i64;
    let mut downed_differing = 0usize;
    let mut downed_worst = 0i64;

    for &(o_i, g_i) in &c.joined {
        let ours = &c.ours["players"][o_i]["extHealingStats"]["outgoingHealingAllies"];
        let refr = &c.golden["players"][g_i]["extHealingStats"]["outgoingHealingAllies"];
        let ours_b = &c.ours["players"][o_i]["extBarrierStats"]["outgoingBarrierAllies"];
        let refr_b = &c.golden["players"][g_i]["extBarrierStats"]["outgoingBarrierAllies"];
        assert!(ours.is_array(), "outgoingHealingAllies must be present on every player");
        assert_eq!(
            ours.as_array().unwrap().len(),
            our_acc.len(),
            "the ally axis must be one row per players[] entry"
        );
        for &(o_j, g_j) in &c.joined {
            // Only compare ally columns whose ACCOUNT matches on both sides.
            if our_acc[o_j] != ref_acc[g_j] {
                continue;
            }
            let a = num(&ours[o_j][0]["healing"]);
            let b = num(&refr[g_j][0]["healing"]);
            cells += 1;
            if b != 0 || a != 0 {
                nonzero += 1;
            }
            if a != b {
                differing += 1;
                if (a - b).abs() > worst {
                    worst = (a - b).abs();
                    worst_desc = format!("{} -> {}: ours {a} ref {b}", our_acc[o_i], our_acc[o_j]);
                }
            }
            let da = num(&ours[o_j][0]["downedHealing"]);
            let db = num(&refr[g_j][0]["downedHealing"]);
            if da != db {
                downed_differing += 1;
                downed_worst = downed_worst.max((da - db).abs());
            }
            let ba = num(&ours_b[o_j][0]["barrier"]);
            let bb = num(&refr_b[g_j][0]["barrier"]);
            b_cells += 1;
            if ba != bb {
                b_differing += 1;
                b_worst = b_worst.max((ba - bb).abs());
            }
        }
    }

    println!(
        "outgoingHealingAllies: {cells} cells ({nonzero} nonzero), {differing} differing, worst {worst} [{worst_desc}]"
    );
    println!("  downedHealing: {downed_differing} differing, worst {downed_worst}");
    println!("outgoingBarrierAllies: {b_cells} cells, {b_differing} differing, worst {b_worst}");

    assert!(cells > 1_000, "degenerate comparison: only {cells} cells");
    assert!(nonzero > 100, "degenerate comparison: only {nonzero} nonzero cells");
    // Measured EXACT on this capture -- asserted exactly rather than
    // bounded. The module doc warns that this family is downstream of
    // M10's known peer-sanitization residual; on the reference capture
    // that residual does not reach it, so nothing is loosened for it.
    assert_eq!(differing, 0, "outgoingHealingAllies must match the reference exactly");
    assert_eq!(downed_differing, 0, "downedHealing must match the reference exactly");
    assert_eq!(b_differing, 0, "outgoingBarrierAllies must match the reference exactly");
    let _ = (worst, b_worst, downed_worst);
}

/// `totalHealingDist` / `totalBarrierDist`: per-skill rows, compared by
/// EXISTENCE first (the Task-2c lesson: a union read with `unwrap_or(0)`
/// is blind to a phantom row) and then value-by-value.
#[test]
fn ei_json_healing_and_barrier_dist_match_the_reference() {
    let Some(c) = render_and_reference("totalHealingDist") else { return };

    let mut rows = 0usize;
    let mut missing = 0usize;
    let mut extra = 0usize;
    let mut cells = 0usize;
    let mut differing = 0usize;
    let mut worst_total = 0i64;
    let mut differing_hits = 0usize;
    let mut differing_minmax = 0usize;
    let mut differing_downed = 0usize;
    let mut b_rows = 0usize;
    let mut b_missing = 0usize;
    let mut b_extra = 0usize;
    let mut b_differing = 0usize;

    for &(o_i, g_i) in &c.joined {
        for (family, total_key) in
            [("extHealingStats", "totalHealing"), ("extBarrierStats", "totalBarrier")]
        {
            let dist_key =
                if total_key == "totalHealing" { "totalHealingDist" } else { "totalBarrierDist" };
            let ours = dist_rows(&c.ours["players"][o_i][family][dist_key]);
            let refr = dist_rows(&c.golden["players"][g_i][family][dist_key]);
            let ids: BTreeSet<i64> = ours.keys().chain(refr.keys()).copied().collect();
            for id in ids {
                let is_healing = total_key == "totalHealing";
                match (ours.get(&id), refr.get(&id)) {
                    (Some(a), Some(b)) => {
                        if is_healing {
                            rows += 1;
                        } else {
                            b_rows += 1;
                        }
                        let mut row_differs = false;
                        for key in ["hits", "min", "max"] {
                            cells += 1;
                            if num(&a[key]) != num(&b[key]) {
                                row_differs = true;
                                if key == "hits" {
                                    differing_hits += 1;
                                } else {
                                    differing_minmax += 1;
                                }
                            }
                        }
                        cells += 1;
                        let (ta, tb) = (num(&a[total_key]), num(&b[total_key]));
                        if ta != tb {
                            row_differs = true;
                            worst_total = worst_total.max((ta - tb).abs());
                        }
                        if is_healing {
                            cells += 1;
                            if num(&a["totalDownedHealing"]) != num(&b["totalDownedHealing"]) {
                                row_differs = true;
                                differing_downed += 1;
                            }
                            // `indirectHealing` must agree exactly: it is a
                            // pure wire-shape classification, not a total.
                            assert_eq!(
                                a["indirectHealing"], b["indirectHealing"],
                                "indirectHealing disagrees on skill {id}"
                            );
                        }
                        if row_differs {
                            if is_healing {
                                differing += 1;
                            } else {
                                b_differing += 1;
                            }
                        }
                    }
                    (None, Some(_)) => {
                        if is_healing {
                            missing += 1;
                        } else {
                            b_missing += 1;
                        }
                    }
                    (Some(_), None) => {
                        if is_healing {
                            extra += 1;
                        } else {
                            b_extra += 1;
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
    }

    println!(
        "totalHealingDist: {rows} shared rows, {missing} missing, {extra} extra; {cells} cells, {differing} differing rows (hits {differing_hits}, min/max {differing_minmax}, downed {differing_downed}), worst total {worst_total}"
    );
    println!(
        "totalBarrierDist: {b_rows} shared rows, {b_missing} missing, {b_extra} extra, {b_differing} differing rows"
    );

    assert!(rows > 50, "degenerate comparison: only {rows} healing dist rows");
    // Row EXISTENCE first (the Task-2c lesson), then every value.
    assert_eq!(missing, 0, "{missing} reference healing-dist rows are absent");
    assert_eq!(extra, 0, "{extra} healing-dist rows we emit are not in the reference");
    assert_eq!(differing, 0, "{differing} healing-dist rows differ from the reference");
    assert_eq!(b_missing, 0, "{b_missing} reference barrier-dist rows are absent");
    assert_eq!(b_extra, 0, "{b_extra} barrier-dist rows we emit are not in the reference");
    assert_eq!(b_differing, 0, "{b_differing} barrier-dist rows differ from the reference");
    let _ = (worst_total, differing_hits, differing_minmax, differing_downed);
}

/// `healing1S`: whole-series comparison, plus the structural contracts
/// (grid length equal to the reference's, monotone non-decreasing, last
/// element equal to the scalar `outgoingHealing[0].healing`).
#[test]
fn ei_json_healing_1s_matches_the_reference() {
    let Some(c) = render_and_reference("healing1S") else { return };

    let mut buckets = 0usize;
    let mut differing = 0usize;
    let mut worst = 0i64;
    let mut series = 0usize;

    for &(o_i, g_i) in &c.joined {
        let ours = &c.ours["players"][o_i]["extHealingStats"]["healing1S"][0];
        let refr = &c.golden["players"][g_i]["extHealingStats"]["healing1S"][0];
        let (Some(a), Some(b)) = (ours.as_array(), refr.as_array()) else { continue };
        series += 1;
        assert_eq!(a.len(), b.len(), "healing1S grid length must match the reference");
        // Structural contracts, asserted exactly.
        let vals: Vec<i64> = a.iter().map(num).collect();
        assert!(vals.windows(2).all(|w| w[0] <= w[1]), "healing1S must be cumulative");
        assert_eq!(
            vals.last().copied().unwrap_or(0),
            num(&c.ours["players"][o_i]["extHealingStats"]["outgoingHealing"][0]["healing"]),
            "healing1S's last element must equal the scalar healing total"
        );
        for (x, y) in a.iter().zip(b.iter()) {
            buckets += 1;
            let (x, y) = (num(x), num(y));
            if x != y {
                differing += 1;
                worst = worst.max((x - y).abs());
            }
        }
    }

    println!("healing1S: {series} series, {buckets} buckets, {differing} differing, worst {worst}");
    assert!(buckets > 10_000, "degenerate comparison: only {buckets} buckets");
    assert_eq!(differing, 0, "healing1S must match the reference bucket for bucket");
    let _ = worst;
}

// ---------------------------------------------------------------------
// (b) minions[]
// ---------------------------------------------------------------------

/// axibridge's own normalization (`computePlayerAggregation.ts:865-869`):
/// strip a leading `Juvenile `, collapse anything containing `UNKNOWN`.
fn normalize_minion_name(raw: &str) -> String {
    let n = raw.strip_prefix("Juvenile ").unwrap_or(raw);
    if n.to_uppercase().contains("UNKNOWN") {
        return "Unknown".to_string();
    }
    n.to_string()
}

/// `minions[].totalDamageTakenDist[0]`, calibrated on TWO joins.
///
/// **The strong one, name-agnostic**: per (account, skill id), summed over
/// all of that player's minion groups. This is the join that carries the
/// arithmetic -- it is exactly what `getMinionDamageTaken`'s per-player
/// `total` and the mitigation rows' per-skill counters reduce to -- and it
/// is asserted EXACT.
///
/// **The weaker one, by NAME**: per (account, normalized minion name,
/// skill id), the key axibridge buckets `defenseMinionDamageTaken` by.
/// This one carries two documented, bounded label divergences and is
/// asserted against pinned bounds rather than exactly:
///
/// 1. **Mesmer clones.** The arcdps agent block names species 26153
///    `"Clone"`; GW2EI's export for the same agent reads `"Rifle Clone"`.
///    The species ID this project emits as `minions[].id` is 26153 on both
///    sides, and every value on every row matches exactly -- only the
///    display label differs, so `defenseMinionDamageTaken` would bucket
///    that player's clone damage under `Clone` instead of `Rifle Clone`.
/// 2. **EI's own `UNKNOWN` placeholder group.** One player's minions
///    include an agent GW2EI could not resolve a species for at all
///    (`"UNKNOWN 10730"`, `id: 0`) -- an englobed agent, which this
///    project's `model::agent_kind` classifies as a PLAYER rather than an
///    NPC and therefore never treats as anyone's minion. 12 reference
///    rows / 25,963 damage on this capture: 10 of them carry a skill id
///    that player's real minions never take damage from (so they read as
///    MISSING on the name-agnostic join), and 2 overlap a real group's
///    skills (moving 2 `totalDamage` sums and 5 outcome cells). axibridge
///    collapses that whole label to `"Unknown"` anyway.
///
/// **Everything outside that one group is EXACT** -- 544 of 554
/// (account, skill) rows, 0 rows we emit that the reference does not,
/// and every outcome column (`hits`, `connectedHits`, `blocked`,
/// `evaded`, `glance`, `missed`, `invulned`, `interrupted`) equal.
#[test]
fn ei_json_minion_damage_taken_matches_the_reference() {
    let Some(c) = render_and_reference("minions") else { return };

    /// All of a player's minion damage-taken rows, keyed either by
    /// `skill` (name-agnostic) or by `name|skill`.
    fn collect(players: &Value, idx: usize, by_name: bool) -> BTreeMap<String, Vec<Value>> {
        let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let Some(minions) = players[idx]["minions"].as_array() else { return out };
        for m in minions {
            let name = normalize_minion_name(m["name"].as_str().unwrap_or("Unknown"));
            for row in m["totalDamageTakenDist"][0].as_array().into_iter().flatten() {
                let id = row["id"].as_i64().unwrap_or(-1);
                let key = if by_name { format!("{name}|{id}") } else { format!("{id}") };
                out.entry(key).or_default().push(row.clone());
            }
        }
        out
    }
    let sum = |rows: &[Value], key: &str| -> i64 { rows.iter().map(|r| num(&r[key])).sum() };

    // --- the strong, name-agnostic join ---
    let mut shared = 0usize;
    let mut missing = 0usize;
    let mut extra = 0usize;
    let mut differing_total = 0usize;
    let mut differing_outcome = 0usize;
    let mut player_total_differing = 0usize;
    let mut worst_player_total = 0i64;
    // --- the weaker, name-keyed join ---
    let mut n_shared = 0usize;
    let mut n_missing = 0usize;
    let mut n_extra = 0usize;

    for &(o_i, g_i) in &c.joined {
        let ours = collect(&c.ours["players"], o_i, false);
        let refr = collect(&c.golden["players"], g_i, false);
        for k in ours.keys().chain(refr.keys()).cloned().collect::<BTreeSet<String>>() {
            match (ours.get(&k), refr.get(&k)) {
                (Some(a), Some(b)) => {
                    shared += 1;
                    if sum(a, "totalDamage") != sum(b, "totalDamage") {
                        differing_total += 1;
                    }
                    for key in [
                        "hits", "connectedHits", "blocked", "evaded", "glance", "missed",
                        "invulned", "interrupted",
                    ] {
                        if sum(a, key) != sum(b, key) {
                            differing_outcome += 1;
                        }
                    }
                }
                (None, Some(_)) => missing += 1,
                (Some(_), None) => extra += 1,
                (None, None) => unreachable!(),
            }
        }
        // The consumer-level number `getMinionDamageTaken` produces.
        let total = |m: &BTreeMap<String, Vec<Value>>| -> i64 {
            m.values().map(|rows| sum(rows, "totalDamage")).sum()
        };
        let (ta, tb) = (total(&ours), total(&refr));
        if ta != tb {
            player_total_differing += 1;
            worst_player_total = worst_player_total.max((ta - tb).abs());
        }

        let n_ours = collect(&c.ours["players"], o_i, true);
        let n_refr = collect(&c.golden["players"], g_i, true);
        for k in n_ours.keys().chain(n_refr.keys()).cloned().collect::<BTreeSet<String>>() {
            match (n_ours.get(&k), n_refr.get(&k)) {
                (Some(_), Some(_)) => n_shared += 1,
                (None, Some(_)) => n_missing += 1,
                (Some(_), None) => n_extra += 1,
                (None, None) => unreachable!(),
            }
        }
    }

    println!(
        "minions (by skill): {shared} shared, {missing} missing, {extra} extra; totalDamage differs on {differing_total}; outcome cells differing {differing_outcome}"
    );
    println!(
        "minions (by name|skill): {n_shared} shared, {n_missing} missing, {n_extra} extra"
    );
    println!(
        "minions consumer total: {player_total_differing} players differ, worst {worst_player_total}"
    );

    assert!(shared > 100, "degenerate comparison: only {shared} (account, skill) minion rows");
    // The name-agnostic join is asserted EXACT except for the one englobed
    // "UNKNOWN" group described above.
    assert!(
        missing <= MINION_MISSING_BOUND,
        "{missing} reference (account, skill) minion rows are absent (bound {MINION_MISSING_BOUND})"
    );
    assert_eq!(extra, 0, "we emit minion rows the reference does not have");
    assert!(
        differing_total <= MINION_TOTAL_DIFFERING_BOUND,
        "{differing_total} shared (account, skill) rows disagree on totalDamage"
    );
    let _ = n_shared;
    assert!(
        differing_outcome <= MINION_OUTCOME_DIFFERING_BOUND,
        "{differing_outcome} shared minion outcome cells disagree"
    );
    assert!(
        player_total_differing <= MINION_PLAYER_TOTAL_DIFFERING_BOUND,
        "{player_total_differing} players disagree on the consumer-level minion damage-taken total"
    );
    assert!(
        worst_player_total <= MINION_WORST_PLAYER_TOTAL,
        "worst per-player minion damage-taken delta {worst_player_total} exceeds the pinned bound"
    );
    // The name-keyed join, bounded (the clone rename + the UNKNOWN group).
    assert!(n_shared > 100, "degenerate name-keyed comparison: {n_shared} rows");
    assert!(
        n_missing <= MINION_NAME_MISSING_BOUND && n_extra <= MINION_NAME_EXTRA_BOUND,
        "name-keyed minion row set drifted: {n_missing} missing / {n_extra} extra"
    );
}

/// Pinned bounds -- see this test's doc comment for what each residual is.
const MINION_MISSING_BOUND: usize = 10;
const MINION_TOTAL_DIFFERING_BOUND: usize = 2;
const MINION_OUTCOME_DIFFERING_BOUND: usize = 5;
const MINION_PLAYER_TOTAL_DIFFERING_BOUND: usize = 1;
const MINION_WORST_PLAYER_TOTAL: i64 = 25_963;
const MINION_NAME_MISSING_BOUND: usize = 49;
const MINION_NAME_EXTRA_BOUND: usize = 37;

// ---------------------------------------------------------------------
// (c) guildID
// ---------------------------------------------------------------------

/// `guildID` must be byte-identical to the reference for every joined
/// account that has one -- it is a pure wire decode, so anything less than
/// exact is a bug in the permutation.
#[test]
fn ei_json_guild_id_matches_the_reference() {
    let Some(c) = render_and_reference("guildID") else { return };
    let mut compared = 0usize;
    let mut with_guild = 0usize;
    let mut missing = 0usize;
    for &(o_i, g_i) in &c.joined {
        let refr = c.golden["players"][g_i]["guildID"].as_str();
        let ours = c.ours["players"][o_i]["guildID"].as_str();
        let Some(refr) = refr else { continue };
        compared += 1;
        if refr.chars().any(|ch| ch != '0' && ch != '-') {
            with_guild += 1;
        }
        match ours {
            Some(o) => assert_eq!(
                o, refr,
                "guildID mismatch for account {}",
                c.ours["players"][o_i]["account"]
            ),
            None => missing += 1,
        }
    }
    println!("guildID: {compared} reference values ({with_guild} non-empty), {missing} absent here");
    assert!(compared >= 40, "degenerate comparison: only {compared} guild values");
    assert_eq!(missing, 0, "{missing} accounts have a reference guildID we did not emit");
}

// ---------------------------------------------------------------------
// (e) support[0].boonStripsTime
// ---------------------------------------------------------------------

/// The OUTGOING twin of MEIGAP Task 1c's `defenses[0].boonStripsTime`, and
/// the same join: axilog emits the TRUE sum, GW2EI's export carries what
/// its `Math.Max(foeTime + RemovedDuration, LogDuration)` accumulator
/// produced (`SupportPerAllyStatistics.cs`), so the reference is joined by
/// RECONSTRUCTING that formula from this project's per-boon strip detail.
///
/// What the reconstruction pins, and what it does not: from `current = 0`,
/// `max(current + r, L)` means the FIRST removal of each boon contributes
/// `max(r1, L)`, swallowing its own duration whenever `r1 < L`. So this
/// pins (i) the SET of distinct boons each player stripped, (ii) the count
/// of removals per boon (via `boonStrips`, asserted separately and
/// exactly) and (iii) every removal's duration AFTER the first for each
/// boon -- but not the first one's when it falls below the log length.
#[test]
fn ei_json_support_boon_strips_time_reconstructs_the_reference() {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let Ok(bytes) = std::fs::read(&zevtc) else {
        println!("skip: {zevtc} absent (support boonStripsTime)");
        return;
    };
    let Ok(golden_s) = std::fs::read_to_string(&ei_json) else {
        println!("skip: {ei_json} absent (support boonStripsTime)");
        return;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");
    let raw = decode_raw(&bytes).expect("decode");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);

    let enemies: BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let detail = axilog_core::analysis::support::outgoing_boon_strips(&raw, &enemies, &addr_to_rep);

    // GW2EI's `LogData.LogDuration`, in ms -- the same whole-log span this
    // project's `Encounter::duration_ms` carries.
    let log_ms = enc.duration_ms;

    let ref_by_acc: BTreeMap<&str, &Value> = golden["players"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a), p)))
        .collect();

    let mut compared = 0usize;
    let mut nonzero = 0usize;
    let mut count_mismatch = 0usize;
    let mut time_mismatch = 0usize;
    let mut worst_rel = 0.0f64;

    for (p, m) in enc.players.iter().zip(metrics.players.iter()) {
        let Some(refp) = ref_by_acc.get(account_key(&p.account)) else { continue };
        let Some(sup) = refp["support"].get(0) else { continue };
        compared += 1;
        let ref_count = num(&sup["boonStrips"]);
        if m.support.strips as i64 != ref_count {
            count_mismatch += 1;
        }
        // Reconstruct EI's per-boon `Math.Max` accumulator.
        let mut per_boon: BTreeMap<u32, u64> = BTreeMap::new();
        for &(boon, ms) in detail.get(&p.agent_addr).into_iter().flatten() {
            let cur = per_boon.entry(boon).or_insert(0);
            *cur = (*cur + ms).max(log_ms);
        }
        let reconstructed: u64 = per_boon.values().sum();
        let ref_time = sup["boonStripsTime"].as_f64().unwrap_or(0.0);
        if ref_time > 0.0 {
            nonzero += 1;
        }
        let ours_secs = reconstructed as f64 / 1000.0;
        if (ours_secs - ref_time).abs() > 0.002 {
            time_mismatch += 1;
            worst_rel = worst_rel.max((ours_secs - ref_time).abs() / ref_time.max(1.0));
        }
        // The EMITTED value is the true sum, which must be positive-or-zero
        // and never exceed EI's inflated one.
        let true_secs = m.support.strips_duration_ms as f64 / 1000.0;
        assert!(
            true_secs <= ref_time + 0.002,
            "the true strip-duration sum ({true_secs}) must not exceed EI's inflated total ({ref_time}) for {}",
            p.account
        );
    }

    println!(
        "support boonStripsTime: {compared} accounts ({nonzero} nonzero), {count_mismatch} count mismatches, {time_mismatch} reconstruction mismatches, worst rel {worst_rel:.6}"
    );
    assert!(compared >= 40, "degenerate comparison: only {compared} accounts");
    assert!(nonzero >= 10, "degenerate comparison: only {nonzero} nonzero references");
    assert_eq!(count_mismatch, 0, "boonStrips counts must be exact");
    assert_eq!(time_mismatch, 0, "the boonStripsTime reconstruction must be exact");
}
