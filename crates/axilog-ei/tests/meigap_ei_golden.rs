//! MEIGAP Task 1: ei-json calibration for the four families this task adds
//! to `axilog_ei::to_ei_json`, against the real Elite Insights export for
//! this project's local post-rework WvW capture
//! (`fixtures/local/wvw-postrework.ei.json`, gitignored -- every test here
//! skips cleanly when it is absent, the same
//! `local_fixture`/skip-in-CI pattern `damage_mods_ei_golden.rs` and
//! `axilog-core`'s `tests/common::local_fixture` already use).
//!
//! Families, and where each one's GW2EI semantics are cited:
//!
//! a. `selfBuffs`/`groupBuffs`/`squadBuffs` -- boon-generation attribution
//!    (`axilog_ei`'s own `buff_generation_json` call-site comment, and
//!    `axilog_core::analysis::buffs::generation`'s module doc).
//! b. `buffUptimes[].states`/`.statesPerSource` -- boon stack timelines.
//! c. `defenses[0].receivedCrowdControl`/`.receivedCrowdControlDuration`/
//!    `.boonStrips`/`.boonStripsTime` -- incoming CC + incoming strips
//!    (`axilog_core::analysis::defenses`'s module doc).
//! d. `statsTargets[i][0]` per-target offensive split
//!    (`axilog_core::analysis::per_target`'s module doc).

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_ei::EiInputs;
use serde_json::Value;
use std::collections::BTreeMap;

/// `fixtures/local/` path, honouring `AXILOG_LOCAL_FIXTURES` -- same
/// resolution `damage_mods_ei_golden.rs` uses (see its own copy's comment
/// for why `axilog-core`'s `tests/common` module cannot be shared here).
fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

/// Everything a calibration test in this file needs: the emitted ei-json,
/// the reference export, and the decoded log/encounter behind the emission
/// (which family (c) needs to reconstruct GW2EI's own buggy
/// `boonStripsTime` accumulator from this project's strip detail).
struct Calibration {
    ours: Value,
    golden: Value,
    raw: axilog_core::evtc::RawLog,
    enc: axilog_core::model::Encounter,
}

/// Parses the local capture and renders it through the ei-json adapter with
/// every flag-gated block on (mirroring what axibridge's own
/// `mapEiSettingsToAxilogOptions` forces). `None` when either local fixture
/// is absent.
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
    let report =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, true, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &Default::default());
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let ours = axilog_ei::to_ei_json(
        &report_v1, &report,
        &EiInputs { activity: &activity, boon_states: Some(&boon_states), ..Default::default() },
    );
    Some(Calibration { ours, golden, raw, enc })
}

/// `account -> player object`, for both sides of a join.
fn players_by_account(v: &Value) -> BTreeMap<String, &Value> {
    v["players"]
        .as_array()
        .expect("players array")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (account_key(a).to_string(), p)))
        .collect()
}

/// One `[{id, buffData:[{...}]}]` array as `id -> buffData[0]`.
fn buff_rows(v: &Value) -> BTreeMap<i64, &Value> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = e["id"].as_i64()?;
            let d = e["buffData"].as_array()?.first()?;
            Some((id, d))
        })
        .collect()
}

// ---------------------------------------------------------------------
// (a) selfBuffs / groupBuffs / squadBuffs
// ---------------------------------------------------------------------

/// Per-cell tolerance for boon GENERATION, in percentage points (duration
/// boons) / average stacks (intensity boons).
///
/// This is the M3-boon-precision class, not an exactness claim, and the
/// project's global calibration constraint names it explicitly as the one
/// allowed tolerance class. The same 2pp bar
/// (`boons_golden.rs`'s `GENERATION_TOLERANCE_PP`) already gates
/// `Metrics::boon_generation` itself against a real EI export; these three
/// arrays are a pure re-serialization of exactly those numbers, so the
/// adapter cannot be held to a tighter bar than its own input. See
/// `axilog_core::analysis::buffs::generation`'s module doc for the
/// mechanics behind the residual (GW2EI's `playerCount` denominator counts
/// only targets `InAwareTimes(start, end)`, `BuffStatistics.cs:25-33`,
/// where this project averages over the whole recorded roster).
const GENERATION_TOLERANCE: f64 = 2.0;

/// The one boon whose generation cells genuinely cannot meet
/// [`GENERATION_TOLERANCE`], with a cited GW2EI-mechanics reason.
///
/// **Regeneration (718) is the only `BuffStackType.Regeneration` buff in
/// the game** (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:384-393`), and
/// GW2EI simulates it through a dedicated `HealingLogic` stacking logic
/// (`EIData/Buffs/BuffSimulators/BuffSimulatorNoID/EffectStackingLogic/
/// HealingLogic.cs`) that keeps the HIGHEST-healing-power stack active
/// rather than the queue order every other duration boon uses. This project
/// does not model `HealingLogic` -- `axilog_core::analysis::buffs::
/// BuffStackType::Regeneration`'s own doc comment records that gap
/// explicitly ("Not yet implemented -- this project simulates it as
/// `Queue`") together with the MBUFFSIM Task 1 measurement that motivated
/// deferring it. Because the generation simulator credits whichever source
/// occupies the ACTIVE slot (`generation.rs`'s `advance_duration_ms`),
/// picking a different active stack moves generation credit between
/// sources even when total uptime is unaffected -- which is exactly the
/// shape of the residual measured here: Regeneration's UPTIME calibrates
/// inside `boons_golden.rs`'s own tolerance, only its per-source
/// ATTRIBUTION drifts.
///
/// Measured on the reference export: 28 of 1,584 generation cells exceed
/// 2.0, **all 28 of them Regeneration**; the other 11 boons are 1,452/1,452
/// inside tolerance. Those 28 are asserted against a wider, pinned bound
/// ([`REGENERATION_TOLERANCE`]) instead of being skipped, so the gap cannot
/// silently widen.
const GENERATION_TOLERANCE_ALLOWLISTED_BOONS: &[i64] =
    &[axilog_core::analysis::buffs::REGENERATION as i64];

/// The pinned worst-case bound for the allowlisted Regeneration cells
/// (measured worst: 20.975). Deliberately a real bound, not `f64::MAX`.
const REGENERATION_TOLERANCE: f64 = 21.0;

#[test]
fn ei_json_boon_generation_arrays_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json boon-generation arrays") else { return };
    let (ours, golden) = (&c.ours, &c.golden);
    let ours_by_account = players_by_account(ours);
    let golden_by_account = players_by_account(golden);

    let mut joined = 0usize;
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut worst = 0.0f64;
    let mut allowlisted_over = 0usize;

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        for key in ["selfBuffs", "groupBuffs", "squadBuffs"] {
            let o_rows = buff_rows(&o[key]);
            let g_rows = buff_rows(&g[key]);
            assert!(
                key != "selfBuffs" || o_rows.len() == 12,
                "{account}.selfBuffs must carry all 12 tracked boons (EI's selfBuffs id set is \
                 its buffUptimes id set), got {}",
                o_rows.len()
            );
            // The UNION of both id sets, so the comparison is symmetric:
            // EITHER side may legitimately omit an id (this adapter filters
            // `groupBuffs`/`squadBuffs` on `generation > 0`, EI filters them
            // on `hasGeneration`), and an absent row means 0 on both sides.
            // Iterating our own rows alone would let a reference row we
            // wrongly dropped pass silently -- exactly the regression the
            // MEIGAP fix round's size decision could have introduced.
            let ids: std::collections::BTreeSet<i64> =
                o_rows.keys().chain(g_rows.keys()).copied().collect();
            for id in ids {
                // Restrict to the 12 boons this project tracks at all --
                // EI's arrays span 43 buffs on this capture, and an id
                // outside `BOON_IDS` is a documented SCOPE gap, not a
                // calibration failure.
                if !axilog_core::analysis::buffs::BOON_IDS.iter().any(|&(b, _, _)| b as i64 == id) {
                    continue;
                }
                let g_val =
                    g_rows.get(&id).map(|r| r["generation"].as_f64().unwrap_or(0.0)).unwrap_or(0.0);
                let o_val = o_rows
                    .get(&id)
                    .map(|r| r["generation"].as_f64().expect("emitted generation is a number"))
                    .unwrap_or(0.0);
                checked += 1;
                let delta = (o_val - g_val).abs();
                let allowlisted = GENERATION_TOLERANCE_ALLOWLISTED_BOONS.contains(&id);
                let tol = if allowlisted { REGENERATION_TOLERANCE } else { GENERATION_TOLERANCE };
                if !allowlisted {
                    worst = worst.max(delta);
                } else if delta > GENERATION_TOLERANCE {
                    allowlisted_over += 1;
                }
                if delta > tol {
                    failures.push(format!(
                        "{account}.{key} boon={id}: ours={o_val:.3} reference={g_val:.3} delta={delta:.3}"
                    ));
                }
            }
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(
        failures.is_empty(),
        "{} boon-generation cell(s) over the {GENERATION_TOLERANCE} tolerance (checked {checked} \
         across {joined} accounts) -- worst offenders:\n{}",
        failures.len(),
        failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
    println!(
        "ei_json_boon_generation_arrays: {checked} cells across {joined} accounts, 0 over \
         tolerance, worst non-allowlisted delta {worst:.3} ({allowlisted_over} allowlisted \
         Regeneration cells over {GENERATION_TOLERANCE}, all under {REGENERATION_TOLERANCE})"
    );
}

// ---------------------------------------------------------------------
// (c) defenses[0] incoming CC + incoming boon strips
// ---------------------------------------------------------------------

/// `receivedCrowdControl`, `receivedCrowdControlDuration` and `boonStrips`
/// are asserted EXACT against the reference export (they are integer counts
/// / an unrounded ms sum over an event set this project reproduces
/// event-for-event).
///
/// `boonStripsTime` cannot be asserted directly, because GW2EI's exported
/// value is produced by a verified arithmetic bug
/// (`GW2EIEvtcParser/EIData/Statistics/DefensePerTargetStatistics.cs:63`:
/// `Math.Max(currentBoonStripTime + brae.RemovedDuration,
/// log.LogData.LogDuration)` where `Min` was clearly intended), which
/// axilog deliberately does not reproduce -- see
/// `axilog_core::analysis::defenses::DefenseStats::
/// boon_strips_taken_duration_ms`. Instead this test reconstructs EI's
/// formula from THIS project's own per-boon strip detail
/// (`defenses::incoming_boon_strips`) and asserts the reconstruction is
/// byte-exact against the export. That pins MOST of the removal set, but
/// not all of it, and it is worth being precise about which part:
/// `max(current + r, L)` from `current = 0` means the FIRST removal of each
/// boon contributes `max(r1, L)`, so its own `RemovedDuration` is swallowed
/// entirely whenever `r1 < L`. What the reconstruction pins exactly is
/// (i) the SET of distinct boons stripped off each player, (ii) the COUNT
/// of removals per boon (via `boonStrips`, asserted separately and
/// exactly), and (iii) the `RemovedDuration` of every removal AFTER the
/// first for each boon. A first removal's duration is unconstrained by this
/// check when it falls below the log length. Still a materially stronger
/// join than comparing two duration sums -- and the strongest one available
/// without reproducing the bug.
#[test]
fn ei_json_incoming_cc_and_strips_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json incoming CC + strips") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);

    // GW2EI's `log.LogData.LogDuration`, the constant its buggy accumulator
    // pins each stripped boon to. `durationMS` is exactly that value
    // (`JsonLogBuilder`'s `Duration`/`DurationMS` both read it), and this
    // adapter's own `durationMS` is already calibrated against it.
    let log_duration_ms = c.golden["durationMS"].as_u64().expect("reference durationMS") as f64;
    assert_eq!(
        c.ours["durationMS"].as_u64(),
        Some(log_duration_ms as u64),
        "the reconstruction below is only valid while durationMS itself matches"
    );

    let squad: std::collections::BTreeSet<u64> =
        c.enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let addr_to_rep: BTreeMap<u64, u64> = c
        .enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let strip_detail =
        axilog_core::analysis::defenses::incoming_boon_strips(&c.raw, &squad, &addr_to_rep);
    let account_of_rep: BTreeMap<u64, String> = c
        .enc
        .players
        .iter()
        .map(|p| (p.agent_addr, account_key(&p.account).to_string()))
        .collect();
    // `account -> the EI-bug-shaped boonStripsTime, in seconds`.
    let mut ei_shaped_strip_time: BTreeMap<String, f64> = BTreeMap::new();
    for (rep, strips) in &strip_detail {
        let Some(account) = account_of_rep.get(rep) else { continue };
        let mut per_boon: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
        for &(boon, ms) in strips {
            per_boon.entry(boon).or_default().push(ms);
        }
        // Per boon, in GW2EI's own loop order: `current = 0`, then
        // `current = max(current + removed, logDuration)` per removal.
        let total_ms: f64 = per_boon
            .values()
            .map(|removals| {
                let mut current = 0.0f64;
                for &ms in removals {
                    current = (current + ms as f64).max(log_duration_ms);
                }
                current
            })
            .sum();
        ei_shaped_strip_time
            .insert(account.clone(), round3_ties_even(total_ms / 1000.0));
    }

    let mut joined = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut nonzero_cc = 0usize;
    let mut nonzero_strips = 0usize;

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        let (od, gd) = (&o["defenses"][0], &g["defenses"][0]);
        for field in ["receivedCrowdControl", "receivedCrowdControlDuration", "boonStrips"] {
            let (ov, gv) = (&od[field], &gd[field]);
            assert!(!gv.is_null(), "reference {account}.defenses[0].{field} missing");
            if ov != gv {
                failures.push(format!("{account}.{field}: ours={ov} reference={gv}"));
            }
        }
        if od["receivedCrowdControl"].as_u64().unwrap_or(0) > 0 {
            nonzero_cc += 1;
        }
        if od["boonStrips"].as_u64().unwrap_or(0) > 0 {
            nonzero_strips += 1;
        }
        // boonStripsTime: the reconstruction, not the emitted value.
        let g_time = gd["boonStripsTime"].as_f64().expect("reference boonStripsTime");
        let recon = ei_shaped_strip_time.get(account).copied().unwrap_or(0.0);
        if (recon - g_time).abs() > 0.0005 {
            failures.push(format!(
                "{account}.boonStripsTime[EI-bug reconstruction]: ours={recon:.3} reference={g_time:.3}"
            ));
        }
        // ... and the value axilog actually emits is the TRUE sum, which
        // for any player who was stripped at all must be strictly smaller
        // than EI's inflated one (each stripped boon's remaining duration
        // is necessarily below a whole log's length).
        let o_time = od["boonStripsTime"].as_f64().expect("emitted boonStripsTime");
        // `<=`, not `<`: EI's per-boon accumulator only inflates when the
        // FIRST removal of that boon reported less remaining duration than
        // the whole log (`max(r1, L) + r2 + ...`), so the two can coincide.
        if od["boonStrips"].as_u64().unwrap_or(0) > 0 && !(o_time > 0.0 && o_time <= g_time) {
            failures.push(format!(
                "{account}.boonStripsTime[emitted]: expected 0 < {o_time} <= {g_time}"
            ));
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(
        nonzero_cc >= 10 && nonzero_strips >= 10,
        "expected a materially non-degenerate fixture, got {nonzero_cc} accounts with incoming CC \
         and {nonzero_strips} with incoming strips"
    );
    assert!(
        failures.is_empty(),
        "{} incoming-CC/strip mismatch(es) across {joined} accounts:\n{}",
        failures.len(),
        failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
    println!(
        "ei_json_incoming_cc_and_strips: {joined} accounts joined, all four fields exact \
         ({nonzero_cc} with incoming CC, {nonzero_strips} with incoming strips; boonStripsTime \
         via the EI-bug reconstruction)"
    );
}

// ---------------------------------------------------------------------
// (d) statsTargets[i][0] -- the per-target offensive split
// ---------------------------------------------------------------------

/// The per-target fields asserted EXACT. `totalDmg` is pre-existing (M10)
/// and already calibrated elsewhere; `downContribution` is deliberately
/// absent -- it is this project's arcdps methodology, not EI's
/// 90%-to-downstate-window algorithm (see the adapter's own comment on the
/// `statsTargets` block, and `contribution`'s module doc).
const PER_TARGET_FIELDS: &[&str] =
    &["killed", "downed", "connectedDamageCount", "againstDownedCount", "interrupts"];

/// `statsTargets[i][0]` cannot be compared positionally. Post-MROSTER both
/// sides list the same KIND of actor (enemy players) and post-MINSTID the
/// same GRANULARITY too (both regroup agents sharing an `InstID` into one
/// target, `AgentManipulationHelper.cs:467-474` /
/// `axilog_core::wvw::dedupe_enemy_players` -- 56 rows here against GW2EI's
/// 56). It still cannot be positional: GW2EI carries a 57th synthetic
/// aggregate target this project does not emit, neither side promises an
/// order, and the two name spaces do not intersect either. The join therefore goes through arcdps AGENT IDENTITY --
/// the instid GW2EI encodes into its `"<Spec> pl-<instid>"` placeholder
/// name -> the addr that instid belonged to -> this project's enemy index
/// -- exactly the M16 pattern `damage_mods_ei_golden.rs` established (see
/// that file's long comment for the full reasoning, including why
/// `Enemy::instid` cannot shortcut it).
#[test]
fn ei_json_stats_targets_split_matches_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json statsTargets split") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);

    let mut instid_to_addrs: BTreeMap<u16, std::collections::BTreeSet<u64>> = BTreeMap::new();
    for ev in &c.raw.events {
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
    let target_index_by_id: BTreeMap<u64, usize> = c.ours["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t["id"].as_u64().map(|id| (id, i)))
        .collect();
    let addr_to_enemy_index: BTreeMap<u64, usize> = c.enc
        .enemies
        .iter()
        .filter_map(|e| target_index_by_id.get(&e.id).map(|&i| (e, i)))
        .flat_map(|(e, i)| e.agent_addrs.iter().map(move |&a| (a, i)))
        .collect();
    // `(our targets[] index, reference targets[] index)`.
    let mut joinable: Vec<(usize, usize)> = Vec::new();
    for (g_i, t) in c.golden["targets"].as_array().expect("reference targets").iter().enumerate() {
        let Some(instid) = t["name"]
            .as_str()
            .and_then(|n| n.rsplit_once("pl-"))
            .and_then(|(_, s)| s.parse::<u16>().ok())
        else {
            continue;
        };
        let indices: std::collections::BTreeSet<usize> = instid_to_addrs
            .get(&instid)
            .into_iter()
            .flatten()
            .filter_map(|a| addr_to_enemy_index.get(a).copied())
            .collect();
        if indices.len() == 1 {
            joinable.push((*indices.iter().next().expect("len == 1"), g_i));
        }
    }
    assert!(joinable.len() >= 40, "expected at least 40 joinable targets, got {}", joinable.len());

    let mut joined_players = 0usize;
    let mut checked = 0usize;
    let mut nonzero = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined_players += 1;
        let (o_st, g_st) = (&o["statsTargets"], &g["statsTargets"]);
        for &(o_i, g_i) in &joinable {
            let (op, gp) = (&o_st[o_i][0], &g_st[g_i][0]);
            for &field in PER_TARGET_FIELDS {
                let gv = gp[field].as_i64().unwrap_or(0);
                let ov = op[field].as_i64().unwrap_or_else(|| {
                    panic!("{account}.statsTargets[{o_i}][0].{field} must be an integer")
                });
                checked += 1;
                if gv != 0 || ov != 0 {
                    nonzero += 1;
                }
                if ov != gv {
                    failures.push(format!(
                        "{account}.statsTargets[our {o_i} / ref {g_i}][0].{field}: ours={ov} \
                         reference={gv}"
                    ));
                }
            }
        }
    }

    assert!(joined_players >= 30, "expected at least 30 joined accounts, got {joined_players}");
    assert!(
        nonzero >= 100,
        "expected a materially non-degenerate comparison, got only {nonzero} nonzero cells"
    );
    assert!(
        failures.is_empty(),
        "{} per-target mismatch(es) (checked {checked} cells across {joined_players} accounts x \
         {} joined targets):\n{}",
        failures.len(),
        joinable.len(),
        failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
    println!(
        "ei_json_stats_targets_split: {checked} cells exact ({nonzero} nonzero) across \
         {joined_players} accounts x {} joined targets",
        joinable.len()
    );
}

// ---------------------------------------------------------------------
// (b) buffUptimes[].states / .statesPerSource
// ---------------------------------------------------------------------

/// Per-instant bound for INTENSITY boons (Might, Stability), where both
/// sides are a 25-stack simulation and an instantaneous count legitimately
/// differs by a few stacks while the time-average agrees to four decimal
/// places. DURATION boons get a bound of 1 (i.e. exact, since their graph
/// is 0/1 on both sides) -- measured 397 of 398 such timelines are
/// sample-for-sample identical to the reference.
///
/// The meaningful bar for these timelines is their INTEGRAL -- that is
/// literally `buffData[0].uptime` for an intensity boon, which
/// `boons_golden.rs` already pins to 0.5% relative
/// (`INTENSITY_STACK_RELATIVE_TOLERANCE`). So the mean of the sampled
/// series is checked against [`STATE_MEAN_TOLERANCE_STACKS`] (the tight,
/// meaningful bar) and the instantaneous deviation is bounded here purely
/// so a structural break cannot hide behind a matching average.
///
/// Measured worst instantaneous delta on the reference export: 13 stacks,
/// on ONE account's Might at a 25-stack burst boundary (2 of 169,614
/// sampled instants exceed 9). Pinned at 16 for margin -- set from the
/// measurement with headroom, not clamped to it.
const INTENSITY_STATE_STACK_BOUND: i64 = 16;

/// Per-timeline bound on `|mean(ours) - mean(reference)|` over the 1s
/// sample grid, in stacks. This is the tight bar: it is the same quantity
/// `buffData[0].uptime` reports for an intensity boon, and 0/1 presence for
/// a duration one.
///
/// Measured worst on the reference export: 0.052 stacks. Pinned at 0.2.
const STATE_MEAN_TOLERANCE_STACKS: f64 = 0.2;

/// Fraction of sampled instants allowed to differ AT ALL, across every
/// timeline. Transition times differ by milliseconds between the two
/// simulators (an expiry landing a tick either side of a sample point),
/// which is a timing residual rather than a stack-count one; this bounds
/// how often that can happen. Measured: 83 of 169,614 instants (0.05%).
/// Pinned at 1%.
const STATE_SAMPLE_MISMATCH_FRACTION: f64 = 0.01;

/// `statesPerSource` must sum back to `states`. Not exact, deliberately:
/// this project runs TWO boon simulations kept independent ON PURPOSE (see
/// `axilog_core::analysis::buffs::generation`'s module doc -- one tracks
/// stack COUNT, the other stack OWNERSHIP, and deriving either from the
/// other would remove the cross-check between them). `states` comes from
/// the count simulator (the one `uptime`/`presence` in the same `buffData`
/// are derived from, so those agree by construction); `statesPerSource`
/// from the ownership simulator. This bounds their disagreement. Measured
/// worst on the reference export: 2 stacks. Pinned at 6.
const SOURCE_SUM_STACK_BOUND: i64 = 6;

/// Reads a `[[t, v], ...]` step timeline's value at `t`.
fn state_value_at(states: &[Value], t: i64) -> i64 {
    let mut v = 0i64;
    for s in states {
        let time = s[0].as_i64().unwrap_or(0);
        if time > t {
            break;
        }
        v = s[1].as_i64().unwrap_or(0);
    }
    v
}

fn as_states(v: &Value) -> &[Value] {
    v.as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

/// The two INTENSITY-type boons (`BOON_IDS`' own flag): everything else is
/// a duration boon whose `states` is 0/1 on both sides.
fn is_intensity_boon(id: i64) -> bool {
    axilog_core::analysis::buffs::BOON_IDS
        .iter()
        .any(|&(bid, _, intensity)| bid as i64 == id && intensity)
}

/// `states` is compared by SAMPLING both step functions on a fixed 1s grid
/// rather than by comparing transition lists pairwise: GW2EI fuses its
/// segments and this project's simulator emits its own, so the two lists
/// legitimately have different lengths for the same stack history, while
/// the FUNCTION they describe is the thing both sides actually mean (and
/// the thing axibridge integrates -- `computeStabPerformance.ts` and
/// `computeCommanderStats.ts` both time-average these).
///
/// Two structural properties ARE asserted exactly, because they are
/// contract rather than simulation: every non-empty array starts with the
/// mandatory `[0, 0]` pair, and times are non-decreasing.
#[test]
fn ei_json_buff_states_match_the_reference_export_when_available() {
    let Some(c) = render_and_reference("ei-json buff states") else { return };
    let ours_by_account = players_by_account(&c.ours);
    let golden_by_account = players_by_account(&c.golden);
    let duration_ms = c.golden["durationMS"].as_i64().expect("reference durationMS");

    let mut joined = 0usize;
    let mut compared = 0usize;
    let mut samples = 0usize;
    let mut mismatched = 0usize;
    let mut worst_instant = 0i64;
    let mut worst_mean = 0.0f64;
    let mut worst_source_sum = 0i64;
    let mut with_sources = 0usize;
    let mut duration_exact = 0usize;
    let mut duration_total = 0usize;
    let mut duration_mismatched_instants = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (account, o) in &ours_by_account {
        let Some(g) = golden_by_account.get(account) else { continue };
        joined += 1;
        let g_by_id: BTreeMap<i64, &Value> = g["buffUptimes"]
            .as_array()
            .expect("reference buffUptimes")
            .iter()
            .filter_map(|b| b["id"].as_i64().map(|id| (id, b)))
            .collect();

        for b in o["buffUptimes"].as_array().expect("emitted buffUptimes") {
            let id = b["id"].as_i64().expect("boon id");
            let ours = as_states(&b["states"]);

            // -- structure, exact --
            //
            // An EMPTY array is the one legal exception: this adapter emits
            // a `buffUptimes` entry for all 12 tracked boons including ones
            // the player never held, where GW2EI would carry no entry at
            // all and its own builder returns `[]` for an absent graph
            // (`JsonBuffsUptimeBuilder.cs:68-76`).
            if !ours.is_empty() {
                assert_eq!(
                    ours.first().map(|s| (s[0].as_i64(), s[1].as_i64())),
                    Some((Some(0), Some(0))),
                    "{account} boon {id}: states must lead with the mandatory [0, 0] pair"
                );
            }
            let mut prev = -1i64;
            for s in ours {
                let t = s[0].as_i64().expect("state time is an integer");
                assert!(t >= prev, "{account} boon {id}: states must be non-decreasing in time");
                prev = t;
            }
            // A duration boon's graph is 0/1 on BOTH sides -- GW2EI's
            // `BuffSimulationItemDuration.GetStacks()` is the single active
            // stack, and every one of the reference export's ten duration
            // boons tops out at 1 (measured). This is a hard shape
            // assertion, not a tolerance.
            if !is_intensity_boon(id) {
                for s in ours {
                    assert!(
                        s[1].as_i64().unwrap_or(0) <= 1,
                        "{account} boon {id}: a duration boon's states must be 0/1, got {}",
                        s[1]
                    );
                }
            }

            // -- statesPerSource sums back to states (bounded) --
            let per_source = b["statesPerSource"].as_object().expect("statesPerSource object");
            if !per_source.is_empty() {
                with_sources += 1;
            }
            for s in ours {
                let t = s[0].as_i64().expect("state time");
                let sum: i64 = per_source.values().map(|v| state_value_at(as_states(v), t)).sum();
                let total = s[1].as_i64().expect("state value");
                worst_source_sum = worst_source_sum.max((sum - total).abs());
                if (sum - total).abs() > SOURCE_SUM_STACK_BOUND {
                    failures.push(format!(
                        "{account} boon {id} @{t}ms: statesPerSource sums to {sum}, states says \
                         {total} (over the {SOURCE_SUM_STACK_BOUND}-stack bound)"
                    ));
                }
            }

            // -- value comparison vs the reference, sampled on a 1s grid --
            let Some(gb) = g_by_id.get(&id) else { continue };
            let theirs = as_states(&gb["states"]);
            if theirs.is_empty() {
                continue;
            }
            compared += 1;
            // A duration boon's graph is 0/1 on both sides, so a per-instant
            // bound of 1 would be vacuous. Those are held to a RATIO and an
            // instant COUNT instead (see the two asserts at the end); the
            // per-instant bound below is meaningful only for the two
            // intensity boons.
            let intensity = is_intensity_boon(id);
            let bound = if intensity { INTENSITY_STATE_STACK_BOUND } else { i64::MAX };
            let (mut sum_ours, mut sum_theirs, mut n) = (0i64, 0i64, 0i64);
            let mut all_equal = true;
            let mut t = 0i64;
            while t <= duration_ms {
                let (a, b2) = (state_value_at(ours, t), state_value_at(theirs, t));
                samples += 1;
                n += 1;
                sum_ours += a;
                sum_theirs += b2;
                let delta = (a - b2).abs();
                worst_instant = worst_instant.max(delta);
                if delta != 0 {
                    mismatched += 1;
                    all_equal = false;
                    if !intensity {
                        duration_mismatched_instants += 1;
                    }
                }
                if delta > bound {
                    failures.push(format!(
                        "{account} boon {id} @{t}ms: ours={a} reference={b2} (delta {delta} > \
                         {bound} stacks)"
                    ));
                }
                t += 1000;
            }
            if !intensity {
                duration_total += 1;
                if all_equal {
                    duration_exact += 1;
                }
            }
            let mean_delta =
                ((sum_ours - sum_theirs) as f64 / n.max(1) as f64).abs();
            worst_mean = worst_mean.max(mean_delta);
            if mean_delta > STATE_MEAN_TOLERANCE_STACKS {
                failures.push(format!(
                    "{account} boon {id}: sampled mean ours={:.3} reference={:.3} (delta \
                     {mean_delta:.3} > {STATE_MEAN_TOLERANCE_STACKS})",
                    sum_ours as f64 / n.max(1) as f64,
                    sum_theirs as f64 / n.max(1) as f64
                ));
            }
        }
    }

    assert!(joined >= 30, "expected at least 30 joined accounts, got {joined}");
    assert!(compared >= 200, "expected >=200 comparable boon timelines, got {compared}");
    assert!(
        with_sources >= 100,
        "expected >=100 entries with a populated statesPerSource, got {with_sources}"
    );
    assert!(
        failures.is_empty(),
        "{} boon-state failure(s) (sampled {samples} instants across {compared} timelines):\n{}",
        failures.len(),
        failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
    // DURATION boons: a 0/1 graph on both sides, so "within tolerance" is
    // meaningless and the real bar is how often the two differ AT ALL.
    // Measured on the reference export: 397 of 398 timelines are
    // sample-for-sample identical, and the single dissenter differs at
    // exactly ONE of its sampled instants. Both are asserted, so the claim
    // in the report is a gate and not just a printout.
    assert!(
        duration_total >= 300,
        "expected >=300 duration-boon timelines to compare, got {duration_total}"
    );
    assert!(
        duration_exact * 100 >= duration_total * 99,
        "only {duration_exact}/{duration_total} duration-boon timelines are sample-for-sample \
         exact vs the reference (measured 397/398); a duration graph is 0/1 on both sides, so \
         anything below ~99% is a real divergence, not simulation precision"
    );
    assert!(
        duration_mismatched_instants <= 8,
        "{duration_mismatched_instants} duration-boon instants differ from the reference \
         (measured 1); pinned at 8 for margin"
    );

    let frac = mismatched as f64 / samples.max(1) as f64;
    assert!(
        frac <= STATE_SAMPLE_MISMATCH_FRACTION,
        "{mismatched}/{samples} sampled instants differ ({:.2}%), over the {:.0}% bound",
        frac * 100.0,
        STATE_SAMPLE_MISMATCH_FRACTION * 100.0
    );
    println!(
        "ei_json_buff_states: {samples} instants across {compared} timelines / {joined} \
         accounts; {mismatched} differ ({:.2}%), worst instant {worst_instant} stack(s), worst \
         sampled-mean delta {worst_mean:.3}; {duration_exact}/{duration_total} duration-boon \
         timelines sample-for-sample EXACT ({duration_mismatched_instants} differing \
         instants); statesPerSource sums within {worst_source_sum} stack(s) on \
         {with_sources} populated entries",
        frac * 100.0
    );
}

/// `Math.Round(x, 3)`, .NET's half-to-even default -- see
/// `axilog_ei`'s own `round3_ties_even` (private; duplicated here because
/// an integration test cannot reach into the crate's private items, and
/// pulling it into the public API purely for a test would be worse).
/// Hand-rolled rather than `f64::round_ties_even` for the workspace's
/// 1.74 MSRV.
fn round3_ties_even(x: f64) -> f64 {
    let scaled = x * 1000.0;
    let floor = scaled.floor();
    let frac = scaled - floor;
    let rounded = if frac == 0.5 {
        // The tie: land on the even scaled integer.
        if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded / 1000.0
}
