//! M11 Task 3: ei-json calibration for the three axibridge tier-1 wins --
//! `targets[].isFake`, `players[].combatReplayData.{down,dead}`, and
//! `players[].activeTimes` -- against the committed EI golden
//! (`fixtures/wvw-small.ei.json`, itself extracted from axibridge's
//! `test-fixtures/boon/20260117-181030.json`, the real dps.report EI export
//! for this same log -- see that golden file's `_note` field, "Task 3
//! (M11)" entry, for the exact extraction/join method).
//!
//! `down`/`dead` are asserted byte-exact (the replay module's own doc
//! comment claims exact reproduction of GW2EI's own down/dead arrays;
//! this test is that claim's adapter-level assertion). `activeTimes` is
//! calibrated to within 0.5% per player (see
//! `axilog_core::analysis::replay::ActivityIntervals::active_ms`'s doc
//! comment for why this project's formula isn't expected to be
//! byte-exact -- it doesn't track GW2EI's rarer mid-log despawn/respawn
//! `dc` segments).

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

/// Fraction-of-value tolerance for `activeTimes` (M11 Task 3 brief: within
/// 0.5% of the EI golden per player).
const ACTIVE_TIMES_TOLERANCE: f64 = 0.005;

fn read_json(path: &str) -> serde_json::Value {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse JSON {path}: {e}"))
}

/// A serialized EI fragment with our icon mirror undone.
///
/// These goldens compare our output as TEXT against real GW2EI exports,
/// which still name the upstream icon hosts. Undoing the mirror on our side
/// keeps the comparison exact everywhere else -- freezing the mirrored
/// strings into the assertions instead would stop them noticing GW2EI
/// changing an icon, which is what they are for.
fn unmirrored_text(v: &serde_json::Value) -> String {
    let text = serde_json::to_string(v).unwrap();
    let base = axilog_core::icons::ICON_MIRROR_BASE;
    text.replace(&format!("{base}imgur-"), &format!("https://i.{}/", "imgur.com"))
        .replace(&format!("{base}gw2dat-"), &format!("https://assets.{}/", "gw2dat.com"))
}

#[test]
fn ei_json_matches_the_golden_isfake_down_dead_and_active_times() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    let ei = axilog_ei::to_ei_json(&report_v1, None);

    // -- isFake: every target, no exceptions --
    let targets = ei["targets"].as_array().expect("targets must be an array");
    assert!(!targets.is_empty(), "expected at least one target");
    for t in targets {
        assert_eq!(t["isFake"], false, "target {t:?} must be isFake: false");
    }

    // -- profession: a class NAME, never an elite-spec id --
    // `targets[].profession` is this crate's deliberate superset over EI
    // (GW2EI's `JsonNPC` has no profession member), and it is what
    // downstream class-breakdown UIs group enemies by. An elite-spec id
    // this project does not name must surface as the base profession
    // ("Revenant"), never as the id ("79") -- the fixture has 5 such enemy
    // rows (1x Thief id 77, 4x Revenant id 79). Asserts the shape rather
    // than the ids so it survives those specs eventually being named.
    for t in targets {
        let prof = t["profession"].as_str().unwrap_or_default();
        assert!(
            !prof.is_empty(),
            "target {:?} must carry a profession",
            t["name"]
        );
        assert!(
            !prof.chars().all(|c| c.is_ascii_digit()),
            "target {:?} reports elite-spec id {prof:?} as its profession NAME; \
             an unnamed spec must fall back to the base profession",
            t["name"]
        );
    }

    // -- down/dead/activeTimes: join by raw agent-table index -> Anon<N>
    // account -> golden row (same join `professions_match_ei_golden`/
    // `replay_calibrated_against_ei_combat_replay_data` already use). --
    let addr_to_index: HashMap<u64, usize> =
        raw.agents.iter().enumerate().map(|(i, a)| (a.addr, i)).collect();

    let mut players_checked = 0usize;
    let mut down_dead_players_checked = 0usize;
    let mut max_active_times_err_pct = 0.0f64;

    for (i, p) in enc.players.iter().enumerate() {
        let Some(&idx) = addr_to_index.get(&p.agent_addr) else { continue };
        let expected_account = anon_account(idx);
        let key = expected_account.trim_start_matches(':');
        let Some(gp) = golden_players.iter().find(|gp| gp["account"].as_str() == Some(key)) else {
            continue;
        };
        let Some(golden_crd) = gp.get("combatReplayData") else { continue };
        let Some(golden_active) = gp.get("activeTimes").and_then(|v| v.as_array()) else { continue };

        let our_player = &ei["players"][i];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key),
            "positional join sanity: ei-json players[{i}] must be this same account"
        );

        players_checked += 1;

        // down/dead: byte-exact.
        assert_eq!(
            our_player["combatReplayData"]["down"], golden_crd["down"],
            "down intervals must be byte-exact for account {key}"
        );
        assert_eq!(
            our_player["combatReplayData"]["dead"], golden_crd["dead"],
            "dead intervals must be byte-exact for account {key}"
        );
        if golden_crd["down"].as_array().is_some_and(|a| !a.is_empty())
            || golden_crd["dead"].as_array().is_some_and(|a| !a.is_empty())
        {
            down_dead_players_checked += 1;
        }

        // activeTimes: within 0.5%.
        let our_active = our_player["activeTimes"][0].as_f64().expect("our activeTimes[0] is numeric");
        let golden_active_val = golden_active[0].as_f64().expect("golden activeTimes[0] is numeric");
        let err_pct = if golden_active_val > 0.0 {
            (our_active - golden_active_val).abs() / golden_active_val
        } else {
            (our_active - golden_active_val).abs()
        };
        max_active_times_err_pct = max_active_times_err_pct.max(err_pct);
        assert!(
            err_pct <= ACTIVE_TIMES_TOLERANCE,
            "activeTimes for account {key}: ours={our_active} golden={golden_active_val} \
             ({:.3}% off, need <= {:.1}%)",
            err_pct * 100.0,
            ACTIVE_TIMES_TOLERANCE * 100.0
        );
    }

    println!(
        "ei_golden: players_checked={players_checked} down_dead_players_checked={down_dead_players_checked} \
         max_active_times_err={:.4}%",
        max_active_times_err_pct * 100.0
    );

    assert!(players_checked >= 30, "expected at least 30 matched players, got {players_checked}");
    // The golden fixture's own `_note` documents exactly 2 players (of 41)
    // with a non-empty down/dead array in this log -- `players[35]`/`Anon130.5810`
    // (a real account, reachable through this test's account-based join)
    // and `Non Squad Player 10` (one of the 4 rows with no real account to
    // join through at all, per the same `_note` -- unreachable here, same
    // limitation the M9/M3 golden tests already document for that row
    // type). This asserts the ONE reachable non-empty row was actually
    // exercised above, not silently skipped by the join.
    assert_eq!(
        down_dead_players_checked, 1,
        "expected the 1 (of 2) non-empty down/dead row reachable via the real-account join"
    );
}

/// M12 Task 3: `to_ei_json`'s `totalDamageDist`/`damage1S` mapping,
/// calibrated against the SAME golden sidecar fields
/// `skill_damage_golden.rs`/`timeseries_golden.rs` already calibrate the
/// underlying `axilog_core` metrics against (`fixtures/wvw-small.ei.json`'s
/// `players[].skillDamage`/`players[].timeSeries`) -- this test's job is
/// narrower: confirm the ei-json ADAPTER LAYER carries those already-
/// calibrated numbers through into EI's own array shape correctly, not
/// re-derive the calibration itself.
///
/// - `totalDamageDist[0]`: every shared skill id vs golden's
///   `skillDamage.outgoing` -- **EXACT** (same "every shared id matches
///   exactly" bar `skill_damage_golden.rs` established for the underlying
///   `SkillEntry`s this just re-shapes).
/// - `damage1S[0].last()` vs golden's `dpsAll[0].damage` scalar --
///   **EXACT** (the M12 Task 3 brief's specific calibration bar: "damage1S
///   final == scalar").
///
/// Join method: `anon_account(raw agent-table index)`, same as
/// `skill_damage_golden.rs` (reaches the same ~37/41 real accounts; the
/// other 4 `Non Squad Player N` placeholder rows have no real account to
/// join through, same documented limitation).
#[test]
fn ei_json_per_skill_and_per_second_blocks_match_the_golden() {
    use axilog_core::evtc::anon_account;
    use std::collections::HashMap;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    // Both opt-in blocks requested -- this test's whole point is
    // calibrating what `to_ei_json` emits WHEN they're present (the
    // gate-respecting omission-when-absent behavior is covered by
    // `axilog-ei`'s own unit tests, not this golden-fixture test).
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, true, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &Default::default());
    let ei = axilog_ei::to_ei_json(&report_v1, None);

    let mut joined = 0usize;
    let mut dist_mismatches: Vec<String> = Vec::new();
    let mut damage1s_mismatches: Vec<String> = Vec::new();

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(sd) = golden_p.get("skillDamage") else { continue };
        let Some(player_idx) = enc.players.iter().position(|p| {
            p.agent_addrs.contains(&agent.addr)
        }) else { continue };
        joined += 1;

        let our_player = &ei["players"][player_idx];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key.as_str()),
            "positional join sanity: ei-json players[{player_idx}] must be this same account"
        );

        // -- totalDamageDist[0]: every shared skill id, EXACT. --
        let golden_outgoing: HashMap<u32, i64> = sd["outgoing"]
            .as_array()
            .expect("outgoing array")
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["total"].as_i64().unwrap()))
            .collect();
        let our_dist = our_player["totalDamageDist"][0]
            .as_array()
            .unwrap_or_else(|| panic!("account {key}: totalDamageDist[0] must be an array"));
        let our_outgoing: HashMap<u32, i64> = our_dist
            .iter()
            .map(|e| (e["id"].as_u64().unwrap() as u32, e["totalDamage"].as_i64().unwrap()))
            .collect();
        for (&id, &gtotal) in &golden_outgoing {
            let ours = our_outgoing.get(&id).copied();
            if ours != Some(gtotal) {
                dist_mismatches.push(format!("{key} skill {id}: ours={ours:?} golden={gtotal}"));
            }
        }

        // -- damage1S[0].last() vs golden's dpsAll[0].damage scalar, EXACT. --
        let golden_damage = golden_p["damage"].as_i64().expect("damage");
        let our_damage1s_final = our_player["damage1S"][0]
            .as_array()
            .unwrap_or_else(|| panic!("account {key}: damage1S[0] must be an array"))
            .last()
            .unwrap_or_else(|| panic!("account {key}: damage1S[0] must be non-empty"))
            .as_i64()
            .expect("damage1S[0].last() is numeric");
        if our_damage1s_final != golden_damage {
            damage1s_mismatches.push(format!("{key}: damage1S final={our_damage1s_final} golden dpsAll[0].damage={golden_damage}"));
        }
    }

    assert!(joined >= 30, "expected at least 30 accounts to join, got {joined}");
    assert!(
        dist_mismatches.is_empty(),
        "{} totalDamageDist shared skill-id mismatch(es):\n{}",
        dist_mismatches.len(),
        dist_mismatches.join("\n")
    );
    assert!(
        damage1s_mismatches.is_empty(),
        "{} damage1S-final mismatch(es):\n{}",
        damage1s_mismatches.len(),
        damage1s_mismatches.join("\n")
    );

    println!(
        "ei_json_per_skill_and_per_second_blocks_match_the_golden: {joined} accounts joined, \
         0 totalDamageDist shared-id mismatches, 0 damage1S-final mismatches"
    );
}

/// M14 Task 3: `to_ei_json`'s `rotation[]` mapping, calibrated against the
/// golden fixture's own `players[].rotation` -- since the instant-cast
/// merge, the source export's VERBATIM `rotation[]`, all three cast
/// families (see the golden's `_note`, "M14 Task 1 ADDENDUM"). Like
/// `ei_json_per_skill_and_per_second_blocks_match_the_golden` above, this
/// test's job is narrower than `rotation_golden.rs`'s own `Metrics`-level
/// calibration: confirm the ei-json ADAPTER LAYER carries the
/// already-calibrated per-skill cast data through into EI's own flat,
/// non-phase-wrapped `rotation[]` shape unchanged, not re-derive the
/// underlying cast-classification calibration itself.
///
/// Counts are asserted per FAMILY, mirroring `rotation_golden.rs`'s own
/// per-family tolerance table (which is where the reasoning lives): the
/// animated and weapon-swap counts EXACT, the finder-derived instant count
/// bounded. Splitting them here is not just borrowed rigour -- the two
/// exact families are the ones that would catch this layer's real failure
/// mode, a pseudo id escaping as its unsigned bit pattern, since a weapon
/// swap emitted as `4294967294` instead of `-2` lands in neither the
/// golden's swap group nor its animated one.
///
/// Two adapter-level claims this test also pins:
/// - `rotation[].id` is SIGNED: the golden's own swap group is keyed `-2`,
///   so `ei_skill_id`'s cast is what makes the join land at all.
/// - `skillMap` gains a `"s-2"` key with EI's own `"Weapon Swap"` name,
///   from `analysis::skill_map::PSEUDO_SKILL_NAMES`.
#[test]
fn ei_json_rotation_cast_counts_match_the_golden() {
    use axilog_core::evtc::anon_account;
    use std::collections::HashMap;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    // `--rotation` requested -- this test's whole point is calibrating what
    // `to_ei_json` emits WHEN it's present (the gate-respecting
    // omission-when-absent behavior is covered by `axilog-ei`'s own unit
    // tests, not this golden-fixture test).
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, true, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &Default::default());
    let ei = axilog_ei::to_ei_json(&report_v1, None);

    let mut joined = 0usize;
    let mut count_mismatches: Vec<String> = Vec::new();
    let mut instant_ours = 0usize;
    let mut instant_golden = 0usize;

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(golden_rotation) = golden_p.get("rotation").and_then(|v| v.as_array()) else { continue };
        let Some(player_idx) = enc.players.iter().position(|p| p.agent_addrs.contains(&agent.addr)) else { continue };
        joined += 1;

        let our_player = &ei["players"][player_idx];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key.as_str()),
            "positional join sanity: ei-json players[{player_idx}] must be this same account"
        );

        let our_rotation = our_player["rotation"]
            .as_array()
            .unwrap_or_else(|| panic!("account {key}: rotation must be an array"));

        // `(animated, weapon swaps, instants)`, split per cast by the same
        // discriminator `analysis::rotation`'s module doc grounds in GW2EI
        // source: `duration > 1` is animated, and of the rest the `-2`
        // pseudo id is a weapon swap.
        let split = |rotation: &[serde_json::Value]| {
            let (mut a, mut s, mut i) = (0usize, 0usize, 0usize);
            for grp in rotation {
                let id = grp["id"].as_i64().expect("rotation group id");
                for c in grp["skills"].as_array().expect("skills array") {
                    match (c["duration"].as_i64().expect("duration"), id) {
                        (d, _) if d > 1 => a += 1,
                        (_, -2) => s += 1,
                        _ => i += 1,
                    }
                }
            }
            (a, s, i)
        };
        let (g_anim, g_swap, g_inst) = split(golden_rotation);
        let (o_anim, o_swap, o_inst) = split(our_rotation);

        if (o_anim, o_swap) != (g_anim, g_swap) {
            count_mismatches.push(format!(
                "{key}: animated ours={o_anim} golden={g_anim}, swaps ours={o_swap} golden={g_swap}"
            ));
        }
        instant_ours += o_inst;
        instant_golden += g_inst;
    }

    assert!(joined >= 30, "expected at least 30 accounts to join, got {joined}");
    assert!(
        count_mismatches.is_empty(),
        "{} account(s) with an animated-cast or weapon-swap COUNT mismatch (ei-json adapter \
         layer -- these two families are held EXACT):\n{}",
        count_mismatches.len(),
        count_mismatches.join("\n")
    );
    let recovery = instant_ours as f64 / instant_golden.max(1) as f64;
    assert!(
        (0.90..=1.02).contains(&recovery),
        "squad-total instant-cast recovery {recovery:.4} (ours {instant_ours}, golden \
         {instant_golden}) is outside the bound `rotation_golden.rs` documents"
    );

    // The pseudo id survives the adapter as EI writes it, key and name both.
    assert_eq!(
        ei["skillMap"]["s-2"]["name"], "Weapon Swap",
        "skillMap must key the weapon-swap pseudo id as `s-2` (signed), naming it as EI does"
    );
    assert_eq!(ei["skillMap"]["s-2"]["isSwap"], true);

    println!(
        "ei_json_rotation_cast_counts_match_the_golden: {joined} accounts joined, animated + \
         weapon-swap counts EXACT for all of them, instant casts {instant_ours}/{instant_golden} \
         ({:.1}% recovered)",
        recovery * 100.0
    );
}

/// M13 Task 3: `to_ei_json`'s `statsAll[0]` hit-quality mapping and
/// `defenses[0]` mapping, calibrated against the SAME golden sidecar fields
/// `hit_stats_golden.rs`/`defenses_golden.rs` already calibrate the
/// underlying `axilog_core` metrics against (`fixtures/wvw-small.ei.json`'s
/// `players[].hitStats`/`players[].defenses`) -- like
/// `ei_json_per_skill_and_per_second_blocks_match_the_golden` above, this
/// test's job is narrower: confirm the ei-json ADAPTER LAYER carries those
/// already-calibrated `HitStatsOut`/`DefensesOut` numbers through into EI's
/// own `statsAll[0]`/`defenses[0]` field names correctly, not re-derive the
/// underlying calibration itself.
///
/// Every field is checked EXACT except `defenses[0].lifeLeechDamageTaken(Count)`,
/// which is INTENTIONALLY expected to diverge from the fixture's raw value
/// on the count (real EI's own `lifeLeechDamageTakenCount` is a verified-buggy
/// always-0 -- see `axilog_core::analysis::defenses`'s module doc and this
/// crate's `to_ei_json` doc comment on the `defenses` block) -- this test
/// asserts OUR emitted values against a derived TRUE reference instead:
/// primarily the fourth-bucket-immune bug identity
/// (`ours lifeLeechDamageTaken + ours lifeLeechDamageTakenCount ==
/// golden's double-incremented lifeLeechDamageTaken`), with the older
/// `powerDamageTakenCount - strikeDamageTakenCount` derivation retained as
/// a cross-check that this committed fixture is still fourth-bucket-free
/// (MCONDCAT Task 1 -- see `axilog_core::analysis::condition_catalog`).
#[test]
fn ei_json_stats_all_hit_quality_and_defenses_match_the_golden() {
    use axilog_core::evtc::anon_account;
    use std::collections::HashMap;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");
    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account").to_string();
        golden_by_account.insert(account, p);
    }

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &Default::default());
    let ei = axilog_ei::to_ei_json(&report_v1, None);

    // (ei-json statsAll[0] key, golden hitStats key) EXACT count/sum pairs.
    const HIT_STATS_FIELDS: &[&str] = &[
        "criticalRate",
        "criticalDmg",
        "flankingRate",
        "glanceRate",
        "againstMovingRate",
        "connectedDamageCount",
        "connectedDmg",
        "connectedDirectDamageCount",
        "connectedDirectDmg",
        "connectedConditionCount",
        "connectedConditionDamage",
        "critableDirectDamageCount",
        "againstDownedCount",
        "againstDownedDamage",
        "connectedLifeLeechCount",
        "connectedLifeLeechDamage",
        "connectedPowerAbove90HPCount",
        "connectedPowerAbove90HPDamage",
        "connectedConditionAbove90HPCount",
        "connectedConditionAbove90HPDamage",
    ];
    // (ei-json defenses[0] key) EXACT pairs -- `lifeLeechDamageTaken(Count)`
    // handled separately below (see module doc).
    const DEFENSES_FIELDS: &[&str] = &[
        "blockedCount",
        "evadedCount",
        "dodgeCount",
        "missedCount",
        "interruptedCount",
        "invulnedCount",
        "strikeDamageTaken",
        "strikeDamageTakenCount",
        "powerDamageTaken",
        "powerDamageTakenCount",
        "conditionDamageTaken",
        "conditionDamageTakenCount",
        "damageBarrier",
        "damageBarrierCount",
        "breakbarDamageTaken",
        "breakbarDamageTakenCount",
        // MEIGAP Task 1c -- incoming CC + incoming strip COUNTS are exact.
        // `boonStripsTime` is NOT in this list: EI's own exported value is
        // produced by a verified arithmetic bug (see the reconstruction
        // check below and `axilog_core::analysis::defenses::DefenseStats::
        // boon_strips_taken_duration_ms`).
        "receivedCrowdControl",
        "receivedCrowdControlDuration",
        "boonStrips",
    ];

    // MEIGAP Task 1c: the per-boon incoming-strip detail behind the
    // `boonStripsTime` reconstruction below (see that block's comment).
    let squad: std::collections::BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let addr_to_rep: std::collections::BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let strip_detail =
        axilog_core::analysis::defenses::incoming_boon_strips(&raw, &squad, &addr_to_rep);
    let log_duration_ms = golden["durationMS"].as_f64().expect("golden durationMS");

    let mut joined_hit_stats = 0usize;
    let mut joined_defenses = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = anon_account(i);
        let key = expected_account.trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(player_idx) = enc.players.iter().position(|p| p.agent_addrs.contains(&agent.addr))
        else {
            continue;
        };
        let our_player = &ei["players"][player_idx];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key.as_str()),
            "positional join sanity: ei-json players[{player_idx}] must be this same account"
        );

        if let Some(hs) = golden_p.get("hitStats") {
            joined_hit_stats += 1;
            let our_stats_all = &our_player["statsAll"][0];
            for &field in HIT_STATS_FIELDS {
                let golden_val = hs[field].as_i64().unwrap_or(0);
                let our_val = our_stats_all[field].as_i64().unwrap_or_else(|| {
                    panic!("account {key}: statsAll[0].{field} must be an integer")
                });
                if our_val != golden_val {
                    mismatches.push(format!(
                        "{key} statsAll[0].{field}: ours={our_val} golden[hitStats.{field}]={golden_val}"
                    ));
                }
            }
        }

        if let Some(de) = golden_p.get("defenses") {
            joined_defenses += 1;
            let our_defenses = &our_player["defenses"][0];
            for &field in DEFENSES_FIELDS {
                let golden_val = de[field].as_i64().unwrap_or(0);
                let our_val = our_defenses[field].as_i64().unwrap_or_else(|| {
                    panic!("account {key}: defenses[0].{field} must be an integer")
                });
                if our_val != golden_val {
                    mismatches.push(format!(
                        "{key} defenses[0].{field}: ours={our_val} golden[defenses.{field}]={golden_val}"
                    ));
                }
            }

            // `boonStripsTime`: the SECOND intentional divergence on this
            // block (after life-leech below). GW2EI's exported value comes
            // out of `DefensePerTargetStatistics.cs:63`'s
            // `Math.Max(currentBoonStripTime + brae.RemovedDuration,
            // log.LogData.LogDuration)` -- a `Max` where `Min` was plainly
            // intended -- so it is roughly
            // `distinct_boons_stripped * logDuration`, not a duration sum.
            // axilog emits the TRUE sum instead, so the golden's raw value
            // is joined by RECONSTRUCTING EI's formula from this project's
            // own per-boon strip detail. Note what that does and does not
            // pin: `max(current + r, L)` from `current = 0` swallows the
            // FIRST removal's duration per boon whenever it is below the
            // log length, so the join pins the distinct-boon SET, the
            // per-boon removal COUNT (via `boonStrips`, asserted exactly
            // above) and every removal AFTER the first -- not the first
            // one's duration. Still materially stronger than comparing two
            // sums, and it does not enshrine the bug in axilog's output.
            {
                let mut per_boon: std::collections::BTreeMap<u32, Vec<u64>> =
                    std::collections::BTreeMap::new();
                for &(boon, ms) in
                    strip_detail.get(&enc.players[player_idx].agent_addr).into_iter().flatten()
                {
                    per_boon.entry(boon).or_default().push(ms);
                }
                let recon_ms: f64 = per_boon
                    .values()
                    .map(|removals| {
                        let mut current = 0.0f64;
                        for &ms in removals {
                            current = (current + ms as f64).max(log_duration_ms);
                        }
                        current
                    })
                    .sum();
                let recon = round3_ties_even(recon_ms / 1000.0);
                let golden_time = de["boonStripsTime"].as_f64().unwrap_or(0.0);
                if (recon - golden_time).abs() > 0.0005 {
                    mismatches.push(format!(
                        "{key} defenses[0].boonStripsTime [EI-bug reconstruction]: ours={recon:.3} \
                         golden={golden_time:.3}"
                    ));
                }
                // And axilog's own emitted value is the true sum: strictly
                // below EI's inflated one whenever anything was stripped.
                let ours_time = our_defenses["boonStripsTime"].as_f64().unwrap_or(-1.0);
                // `<=`, not `<`: EI's per-boon accumulator only inflates
                // when the FIRST removal of that boon reported less
                // remaining duration than the whole log
                // (`max(r1, L) + r2 + ...`), so on a short fight where every
                // stripped boon's first removal already exceeded the log
                // length the buggy total collapses onto the true sum. Two
                // of this fixture's 37 accounts are exactly that case --
                // which is a stronger agreement, not a weaker one.
                if de["boonStrips"].as_i64().unwrap_or(0) > 0
                    && !(ours_time > 0.0 && ours_time <= golden_time)
                {
                    mismatches.push(format!(
                        "{key} defenses[0].boonStripsTime [emitted]: expected 0 < {ours_time} <= \
                         {golden_time}"
                    ));
                }
            }

            // `lifeLeechDamageTaken(Count)`: intentional divergence -- see
            // this test's module doc. Golden's raw
            // `lifeLeechDamageTakenCount` is a known-buggy always-0, so the
            // reference has to be derived.
            //
            // **MCONDCAT Task 1 changed the derivation.** It used to be
            // `powerDamageTakenCount - strikeDamageTakenCount`, which is
            // `life_leech + FOURTH BUCKET` (buff==1 hits outside GW2EI's
            // condition catalog and not life-leech -- see
            // `axilog_core::analysis::condition_catalog`), not life-leech
            // alone. That difference happens to be 0 on THIS committed
            // pre-rework fixture, but it is emphatically not 0 in general
            // (33 of 48 players on the local post-rework capture), so the
            // old derivation was correct here only by accident. Use the
            // fourth-bucket-immune identity instead: GW2EI's ctor
            // increments the SUM field twice per life-leech hit (the second
            // increment being the copy-paste bug that should have been
            // `LifeLeechDamageTakenCount++`), so the reported sum is
            // exactly `[true sum] + [true count]`.
            let buggy_life_leech_sum = de["lifeLeechDamageTaken"].as_i64().unwrap_or(0);
            let our_llc = our_defenses["lifeLeechDamageTakenCount"].as_i64().unwrap_or(-1);
            let our_lld = our_defenses["lifeLeechDamageTaken"].as_i64().unwrap_or(-1);
            if our_lld + our_llc != buggy_life_leech_sum {
                mismatches.push(format!(
                    "{key} defenses[0] life-leech: ours lifeLeechDamageTaken({our_lld}) + \
                     lifeLeechDamageTakenCount({our_llc}) = {}, golden's raw (double-incremented) \
                     lifeLeechDamageTaken={buggy_life_leech_sum}",
                    our_lld + our_llc
                ));
            }
            // Retain the OLD derivation as a cross-check, valid precisely
            // because this fixture is fourth-bucket-free (asserted below).
            let power_count = de["powerDamageTakenCount"].as_i64().unwrap_or(0);
            let strike_count = de["strikeDamageTakenCount"].as_i64().unwrap_or(0);
            let true_life_leech_count = (power_count - strike_count).max(0);
            if our_llc != true_life_leech_count {
                mismatches.push(format!(
                    "{key} defenses[0].lifeLeechDamageTakenCount: ours={our_llc} \
                     derived_true={true_life_leech_count} -- on THIS fixture the fourth bucket is \
                     empty, so `powerDamageTakenCount - strikeDamageTakenCount` must still agree \
                     with the bug-identity derivation above; a mismatch means the committed \
                     fixture has grown fourth-bucket rows and its golden numbers need review \
                     (NOT golden's raw buggy lifeLeechDamageTakenCount={})",
                    de["lifeLeechDamageTakenCount"].as_i64().unwrap_or(0)
                ));
            }
            // Confirm the intentional divergence is actually real on this
            // fixture (golden's raw count really is the documented bug's
            // 0), not an accidentally-vacuous check -- same "prove the
            // divergence is real, not just permitted" bar the M13 Task 3
            // brief expects.
            if true_life_leech_count > 0 {
                assert_eq!(
                    de["lifeLeechDamageTakenCount"].as_i64().unwrap_or(-1),
                    0,
                    "{key}: expected the golden fixture's raw lifeLeechDamageTakenCount to \
                     exhibit the documented GW2EI bug (always 0) whenever a true nonzero \
                     life-leech count exists -- if this fires, the fixture/GW2EI may have \
                     changed and this test's divergence framing needs revisiting"
                );
            }
        }
    }

    assert!(joined_hit_stats >= 30, "expected at least 30 accounts to join hitStats, got {joined_hit_stats}");
    assert!(joined_defenses >= 30, "expected at least 30 accounts to join defenses, got {joined_defenses}");
    assert!(
        mismatches.is_empty(),
        "{} statsAll[0]/defenses[0] mismatch(es) (checked {joined_hit_stats} hitStats + \
         {joined_defenses} defenses accounts):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    println!(
        "ei_json_stats_all_hit_quality_and_defenses_match_the_golden: {joined_hit_stats} hitStats \
         accounts + {joined_defenses} defenses accounts joined, all fields EXACT except the \
         intentional lifeLeechDamageTakenCount divergence (ours=derived-true, asserted correct)"
    );
}

/// M15 Task 3: the whole opt-in combat-replay surface -- per-player
/// `combatReplayData.{positions, orientations, dc, iconURL}` plus the
/// top-level `combatReplayMetaData` -- against the committed golden's own
/// copy of the reference export's values.
///
/// ## Golden provenance / regeneration (the M13 pattern)
///
/// The four per-player arrays and the metadata object in
/// `fixtures/wvw-small.ei.json` are VERBATIM from the same source every
/// prior task extracted from: axibridge's
/// `test-fixtures/boon/20260117-181030.json`, the real dps.report EI export
/// for this same log (see the golden's `_note`, "Task 3 (M15)" entry, for
/// the join method, the single `dc`-sentinel normalization, and the
/// no-PII note). Regenerated with:
///
/// ```python
/// # python3, from the repo root
/// src = json.load(open('.../axibridge/test-fixtures/boon/20260117-181030.json'))
/// dst = json.load(open('fixtures/wvw-small.ei.json'))            # index-order join
/// for sp, dp in zip(src['players'], dst['players']):
///     sc, dc = sp['combatReplayData'], dp['combatReplayData']
///     assert dc['start'] == sc['start'] and dc['end'] == sc['end']  # join sanity
///     dc['iconURL'], dc['positions'] = sc['iconURL'], sc['positions']
///     dc['orientations'] = sc['orientations']
///     dc['dc'] = [[fix_sentinel(a), fix_sentinel(b)] for a, b in sc['dc']]
/// dst['combatReplayMetaData'] = src['combatReplayMetaData']
/// # ... json.dumps(indent=2), then re-collapse the four numeric arrays
/// # onto single lines (size/readability only, no value change).
/// ```
///
/// `fix_sentinel` maps the source's `-/+9223372036854776000` back to
/// `i64::MIN`/`i64::MAX`: that file was round-tripped through JavaScript,
/// whose `Number` cannot hold C# `long.MinValue`/`long.MaxValue`. GW2EI's
/// own writer emits the exact bounds, and so does axilog.
///
/// ## Why the comparison is TEXTUAL
///
/// EI serializes these as C# `float`s, so its JSON text is the shortest
/// decimal that round-trips through SINGLE precision (`0.009`, `246.672`).
/// `serde_json::Value` holds only `f64`, and a widened `f32` prints its own
/// much longer shortest-round-trip (`0.008999999612569809`,
/// `246.6719970703125`) -- a diff that is INVISIBLE to a parsed-`f64`
/// comparison with a tolerance, and even to an exact one after both sides
/// are narrowed to `f32`. So every assertion below compares
/// `serde_json::to_string(...)` of our value against
/// `serde_json::to_string(...)` of the golden's, which for numbers is
/// exactly a comparison of the emitted decimal text (serde_json's `ryu`
/// output is a pure function of the `f64`, and it preserves the
/// integer-vs-float distinction the golden file's own text carries). See
/// `axilog_ei`'s `ei_float`.
#[test]
fn ei_json_combat_replay_matches_the_golden() {
    use axilog_core::analysis::ei_replay::build_ei_replay_auto;
    use axilog_core::evtc::anon_account;
    use std::collections::HashMap;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden_text = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read {GOLDEN_JSON_PATH}: {e}"));
    let golden: serde_json::Value = serde_json::from_str(&golden_text).expect("parse golden");

    // Raw-file evidence that the golden itself carries EI's `float` text
    // (not a widened `f64`) -- if a future regeneration widened it, this
    // fires before any of the value comparisons below.
    assert!(
        golden_text.contains("\"inchToPixel\": 0.009"),
        "the golden's combatReplayMetaData.inchToPixel must be the literal text 0.009"
    );

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden["players"].as_array().expect("players array") {
        golden_by_account.insert(p["account"].as_str().expect("account").to_string(), p);
    }

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    let ei_replay = build_ei_replay_auto(&raw, &enc);
    let ei = axilog_ei::to_ei_json(&report_v1, Some(&ei_replay));

    // -- combatReplayMetaData: EXACT, field by field, as text --
    let meta = &ei["combatReplayMetaData"];
    let golden_meta = &golden["combatReplayMetaData"];
    for field in ["inchToPixel", "pollingRate", "sizes", "maps"] {
        assert_eq!(
            unmirrored_text(&meta[field]),
            serde_json::to_string(&golden_meta[field]).unwrap(),
            "combatReplayMetaData.{field}"
        );
    }
    assert_eq!(
        serde_json::to_string(meta).unwrap(),
        "{\"inchToPixel\":0.009,\"maps\":[{\"interval\":[0,49285],\"position\":[0,0],\
         \"url\":\"https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-nVu2ivF.png\"}],\"pollingRate\":300,\"sizes\":[523,750]}",
        "combatReplayMetaData must serialize to EI's own float TEXT (0.009, not \
         0.008999999612569809; [0,0], not [0.0,0.0])"
    );

    let mut joined = 0usize;
    let mut samples = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let key = anon_account(i).trim_start_matches(':').to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let golden_crd = &golden_p["combatReplayData"];
        if golden_crd.get("positions").is_none() {
            continue;
        }
        let Some(player_idx) = enc.players.iter().position(|p| p.agent_addrs.contains(&agent.addr))
        else {
            continue;
        };
        joined += 1;

        let our_player = &ei["players"][player_idx];
        assert_eq!(
            our_player["account"].as_str().map(|s| s.trim_start_matches(':')),
            Some(key.as_str()),
            "positional join sanity: ei-json players[{player_idx}] must be this same account"
        );
        let ours = &our_player["combatReplayData"];

        // EXACT scalars/intervals (integers -- no float text involved).
        for field in ["start", "end", "dc", "iconURL"] {
            // `iconURL` goes through the mirror inverse; the other three are
            // integers, for which comparing serialized text is equivalent.
            if unmirrored_text(&ours[field]) != serde_json::to_string(&golden_crd[field]).unwrap() {
                mismatches.push(format!(
                    "{key} combatReplayData.{field}: ours={} golden={}",
                    ours[field], golden_crd[field]
                ));
            }
        }
        samples += ours["positions"].as_array().map(|a| a.len()).unwrap_or(0);

        // TEXT-EXACT float arrays.
        for field in ["positions", "orientations"] {
            let a = serde_json::to_string(&ours[field]).unwrap();
            let b = serde_json::to_string(&golden_crd[field]).unwrap();
            if a != b {
                let first = a
                    .chars()
                    .zip(b.chars())
                    .position(|(x, y)| x != y)
                    .unwrap_or(0)
                    .saturating_sub(30);
                mismatches.push(format!(
                    "{key} combatReplayData.{field} TEXT differs near offset {first}:\n  ours  ...{}\n  golden...{}",
                    &a[first..(first + 120).min(a.len())],
                    &b[first..(first + 120).min(b.len())],
                ));
            }
        }
    }

    assert!(joined >= 30, "expected at least 30 accounts to join, got {joined}");
    assert!(
        mismatches.is_empty(),
        "{} combat-replay mismatch(es) across {joined} accounts:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    println!(
        "ei_json_combat_replay_matches_the_golden: {joined} accounts joined, {samples} position \
         samples, positions/orientations/dc/iconURL/start/end all EXACT (text-level), \
         combatReplayMetaData EXACT"
    );
}

/// M15 Task 3, the always-on regression gate: turning replay ON must ADD
/// fields and change NOTHING else. M11's `combatReplayData.{start, end,
/// down, dead}` and `activeTimes` are always-on and byte-exact against this
/// same golden (`ei_json_matches_the_golden_isfake_down_dead_and_active_times`
/// above); this asserts that the M15 surface is purely additive by
/// stripping the five new keys off the replay-on document and demanding it
/// be BYTE-IDENTICAL to the replay-off one.
#[test]
fn ei_json_replay_fields_do_not_disturb_the_always_on_surface() {
    use axilog_core::analysis::ei_replay::build_ei_replay_auto;

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });

    let without = axilog_ei::to_ei_json(&report_v1, None);
    let ei_replay = build_ei_replay_auto(&raw, &enc);
    let mut with = axilog_ei::to_ei_json(&report_v1, Some(&ei_replay));

    // Sanity: the replay-on document really does carry the new surface
    // (otherwise this test would pass vacuously).
    assert!(with.get("combatReplayMetaData").is_some());
    assert!(with["players"][0]["combatReplayData"]["positions"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(without.get("combatReplayMetaData").is_none(), "replay-off must omit combatReplayMetaData");
    assert!(
        without["players"][0]["combatReplayData"].get("positions").is_none(),
        "replay-off must omit combatReplayData.positions"
    );

    let root = with.as_object_mut().expect("root object");
    root.remove("combatReplayMetaData");
    for p in root["players"].as_array_mut().expect("players").iter_mut() {
        let crd = p["combatReplayData"].as_object_mut().expect("combatReplayData");
        for k in ["positions", "orientations", "dc", "iconURL"] {
            crd.remove(k);
        }
    }
    for t in root["targets"].as_array_mut().expect("targets").iter_mut() {
        t.as_object_mut().expect("target object").remove("combatReplayData");
    }

    assert_eq!(
        serde_json::to_string(&with).unwrap(),
        serde_json::to_string(&without).unwrap(),
        "enabling replay must be PURELY additive to the ei-json surface"
    );
    println!(
        "ei_json_replay_fields_do_not_disturb_the_always_on_surface: replay-on minus the 5 new \
         keys is byte-identical to replay-off"
    );
}

/// M15 Task 3, post-era spot check (local-only, gitignored): the same
/// ei-json wiring, run end-to-end on the real post-rework capture and
/// compared against GW2EI's own export for it -- the era the committed
/// fixture cannot cover. `axilog_core`'s `ei_replay_golden.rs` calibrates
/// the ENGINE against this pair; this asserts the ADAPTER carries it
/// through unchanged, including the `f32` text.
///
/// Point `AXILOG_LOCAL_FIXTURES` at the primary checkout's
/// `fixtures/local/` to run it from a worktree (the captures are PII and
/// are never committed or copied); it prints `skip:` and passes otherwise.
#[test]
fn ei_json_combat_replay_matches_the_local_postrework_export() {
    use axilog_core::analysis::ei_replay::build_ei_replay_auto;
    use std::collections::HashMap;

    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    let (zevtc, json_path) =
        (format!("{dir}/wvw-postrework.zevtc"), format!("{dir}/wvw-postrework.ei.json"));
    let (Ok(bytes), Ok(golden_text)) =
        (std::fs::read(&zevtc), std::fs::read_to_string(&json_path))
    else {
        println!("skip: {dir}/wvw-postrework.* absent (M15 ei-json post-era spot check)");
        return;
    };
    let golden: serde_json::Value = serde_json::from_str(&golden_text).expect("parse local export");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let report_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &report, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    let ei_replay = build_ei_replay_auto(&raw, &enc);
    let ei = axilog_ei::to_ei_json(&report_v1, Some(&ei_replay));

    // metaData: text-exact against the export's own object.
    let want_meta = &golden["combatReplayMetaData"];
    for field in ["inchToPixel", "pollingRate", "sizes", "maps"] {
        assert_eq!(
            unmirrored_text(&ei["combatReplayMetaData"][field]),
            serde_json::to_string(&want_meta[field]).unwrap(),
            "postrework combatReplayMetaData.{field}"
        );
    }

    // Players join by account (the export anonymizes non-squad players).
    let account_by_addr: HashMap<u64, String> = raw
        .agents
        .iter()
        .filter(|a| a.is_player())
        .map(|a| {
            let (_, account, _) = a.name_parts();
            (a.addr, account.trim_start_matches(':').to_string())
        })
        .collect();
    let golden_by_account: HashMap<&str, &serde_json::Value> = golden["players"]
        .as_array()
        .expect("players")
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (a.trim_start_matches(':'), p)))
        .collect();

    let (mut joined, mut pos_text_exact, mut ang_text_exact, mut samples) = (0, 0, 0, 0usize);
    let mut scalar_mismatches: Vec<String> = Vec::new();
    for (idx, p) in enc.players.iter().enumerate() {
        let Some(account) = account_by_addr.get(&p.agent_addr) else { continue };
        let Some(gp) = golden_by_account.get(account.as_str()) else { continue };
        let want = &gp["combatReplayData"];
        let ours = &ei["players"][idx]["combatReplayData"];
        joined += 1;
        samples += ours["positions"].as_array().map(|a| a.len()).unwrap_or(0);
        for field in ["start", "end", "dc", "iconURL"] {
            // See above: `iconURL` is mirrored, the rest are integers.
            if unmirrored_text(&ours[field]) != serde_json::to_string(&want[field]).unwrap() {
                scalar_mismatches.push(format!("{account}.{field}"));
            }
        }
        if serde_json::to_string(&ours["positions"]).unwrap()
            == serde_json::to_string(&want["positions"]).unwrap()
        {
            pos_text_exact += 1;
        }
        if serde_json::to_string(&ours["orientations"]).unwrap()
            == serde_json::to_string(&want["orientations"]).unwrap()
        {
            ang_text_exact += 1;
        }
    }

    println!(
        "ei_json_combat_replay_matches_the_local_postrework_export: {joined} players joined, \
         {samples} position samples; positions TEXT-exact for {pos_text_exact}/{joined} players, \
         orientations for {ang_text_exact}/{joined}"
    );
    assert!(joined >= 40, "expected ~44 squad players to join, got {joined}");
    assert!(
        scalar_mismatches.is_empty(),
        "start/end/dc/iconURL must be exact: {scalar_mismatches:?}"
    );
    // Whole-array text equality per player is an all-or-nothing bar over
    // ~1200 samples each; `ei_replay_golden.rs` reports the per-SAMPLE
    // numbers (100.00% f32-exact positions, >=99.9% orientations -- the
    // stragglers are platform `atan2` ULP noise), so a handful of players
    // can legitimately miss the whole-array bar on orientations.
    assert_eq!(pos_text_exact, joined, "every player's positions must be text-exact");
    assert!(
        ang_text_exact * 10 >= joined * 9,
        "orientations text-exact for only {ang_text_exact}/{joined} players"
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

/// MEIGAP Task 1d: the `statsTargets[i][0]` split is gated on the SAME
/// `--skill-damage` presence signal as `totalDamageDist`, and when present
/// its columns sum back to their `statsAll[0]` counterparts.
///
/// The sum invariant is the structural half of the per-target calibration
/// (the exact-vs-EI half lives in `meigap_ei_golden.rs`, which needs the
/// gitignored local export): each column is the SAME already-calibrated
/// whole-fight predicate restricted to one enemy, so the two must agree
/// modulo events landing on an agent outside the `enemies` set -- which
/// `statsAll` counts and the split cannot. Hence `<=`, with at least one
/// player asserted to hit exact equality so the check cannot go vacuous.
#[test]
fn ei_json_stats_targets_split_is_gated_and_sums_to_stats_all() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);

    // -- gated off: today's `totalDmg`-only row, split keys ABSENT (not 0) --
    let plain =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let plain_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &plain, "0.0.0-test", None, &Default::default());
    let plain_ei = axilog_ei::to_ei_json(&plain_v1, None);
    for p in plain_ei["players"].as_array().expect("players") {
        for t in p["statsTargets"].as_array().expect("statsTargets") {
            let row = t[0].as_object().expect("statsTargets row");
            assert_eq!(
                row.keys().collect::<Vec<_>>(),
                vec!["totalDmg"],
                "without --skill-damage the split keys must be OMITTED, not emitted as zeros \
                 (axibridge's own `sawTargetSplit` guard keys off exactly that)"
            );
        }
    }

    // -- gated on: the full split, summing back to statsAll[0] --
    //
    // `dist_outcomes` is passed explicitly (unlike a bare `Default::
    // default()`) because the Phase B Task 4 review extension below needs
    // `totalDamageDist`'s `missed`/`evaded`/`blocked`/`invulned` outcome
    // columns populated -- those ride the SAME `--skill-damage` gate but
    // are a SEPARATE pass (`SkillRow::outcomes`) that only fills in when
    // this `Passes` field is actually supplied, same as every other block
    // built through `Passes`.
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    let full =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
    let full_v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &full,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes { dist_outcomes: Some(&dist_outcomes), ..Default::default() },
    );
    let ei = axilog_ei::to_ei_json(&full_v1, None);
    let mut exact_players = 0usize;
    let mut checked = 0usize;
    for p in ei["players"].as_array().expect("players") {
        let targets = p["statsTargets"].as_array().expect("statsTargets");
        let mut all_equal = true;
        for (split_field, all_field) in [
            ("killed", "killed"),
            ("downed", "downed"),
            ("connectedDamageCount", "connectedDamageCount"),
            ("againstDownedCount", "againstDownedCount"),
        ] {
            let sum: i64 =
                targets.iter().map(|t| t[0][split_field].as_i64().expect("integer")).sum();
            let whole = p["statsAll"][0][all_field].as_i64().expect("integer");
            checked += 1;
            assert!(
                sum <= whole,
                "{}: statsTargets sum of {split_field} ({sum}) exceeds statsAll[0].{all_field} \
                 ({whole}) -- the split can only ever be a subset",
                p["account"]
            );
            if sum != whole {
                all_equal = false;
            }
        }
        if all_equal {
            exact_players += 1;
        }
    }
    assert!(checked >= 100, "expected a non-degenerate comparison, checked {checked}");
    assert!(
        exact_players > 0,
        "expected at least one player whose per-target split sums EXACTLY to statsAll -- all \
         {exact_players} short means the split is systematically dropping events"
    );
    println!(
        "ei_json_stats_targets_split_is_gated_and_sums_to_stats_all: {checked} column sums \
         checked, {exact_players} players exact"
    );

    // Phase B Task 4 review finding: nine of the 15 new `statsTargets`
    // keys have a directly analogous whole-fight counterpart computable
    // WITHOUT the gitignored local reference export, so a wiring bug in
    // any of them (crit_count/crit_damage swapped, a CC map misread, ...)
    // should not pass CI silently the way it did before this extension.
    //
    // Five join `statsAll[0]` exactly like the four fields above --
    // `appliedCrowdControl`/`criticalDmg`/`flankingRate`/`glanceRate`/
    // `connectedDirectDamageCount` are all plain COUNTS (or a damage sum
    // for `criticalDmg`) restricted to one enemy, same "same predicate,
    // narrower domain" relationship as `killed`/`downed`/etc above.
    // `flankingRate`/`glanceRate` are NOT floating-point rates despite the
    // name -- confirmed directly against a real EI export
    // (`fixtures/wvw-small.ei.json`'s `hitStats.criticalRate`/
    // `flankingRate`/`glanceRate` are plain integers, e.g. 139/51/3, not
    // percentages), matching this adapter's own `crit_count`/`flank_count`/
    // `glance_count` mapping, so the same `sum <= whole` count invariant
    // applies unchanged -- no rate reconstruction needed for these three.
    //
    // The other four -- `missed`/`evaded`/`blocked`/`invulned` -- have NO
    // `statsAll[0]` scalar at all (that block only carries the INCOMING
    // defense-side counts, `defenses[0].missedCount` etc, a different
    // quantity). Their genuine whole-fight counterpart is
    // `totalDamageDist`'s per-skill outcome columns
    // (`crates/axilog-ei/src/lib.rs:222-225`): both this per-target split
    // and that per-skill split are filled by the SAME `classify_outcome`
    // scan over the SAME outgoing event stream (`per_target::build` and
    // `dist_outcomes::build`'s outgoing pass both import
    // `defenses::classify_outcome` and dispatch on it identically -- see
    // `per_target.rs:238-252` vs `dist_outcomes.rs:144-153`), just grouped
    // by target here instead of by skill. So summing the per-target split
    // over the curated `statsTargets` roster can only ever be <= the same
    // field summed over EVERY skill in `totalDamageDist` (which covers
    // every target, enumerated or not) -- the identical subset argument
    // the five `statsAll`-based fields above already rely on, with a
    // different "whole" expression because no single scalar exists for
    // it.
    //
    // Tracked with a PER-FIELD "at least one exact" bar instead of folding
    // into the single `exact_players` counter above: entangling 13 fields
    // behind one "some player matches ALL of them" requirement would make
    // that bar's satisfiability depend on a coincidence of this one
    // fixture's roster (a player touching zero non-enumerated targets
    // across nine MORE fields, not just the original four) rather than
    // meaningfully exercising each field's own split/whole relationship.
    let mut exact_counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut checked2 = 0usize;
    for p in ei["players"].as_array().expect("players") {
        let targets = p["statsTargets"].as_array().expect("statsTargets");
        for (split_field, all_field) in [
            ("appliedCrowdControl", "appliedCrowdControl"),
            ("criticalDmg", "criticalDmg"),
            ("flankingRate", "flankingRate"),
            ("glanceRate", "glanceRate"),
            ("connectedDirectDamageCount", "connectedDirectDamageCount"),
            // Round 2: `criticalRate` is `criticalDmg`'s exact sibling --
            // per-target source `crit_count` (`lib.rs:1088`), whole-fight
            // counterpart `n_hit_stats.crit_count` (`lib.rs:924`). This is
            // the count-versus-count comparison that would actually catch a
            // `crit_count`/`crit_damage` transposition at the mapping site;
            // `criticalDmg`'s own sum<=whole check would NOT catch that
            // (see the transposition experiment recorded in the round-2
            // report), because a small count still satisfies `<=` a large
            // damage total.
            ("criticalRate", "criticalRate"),
            // Round 2: per-target source `against_downed_damage`
            // (`lib.rs:1091`), whole-fight counterpart
            // `n_hit_stats.against_downed_damage` (`lib.rs:937`). Same
            // "same predicate, narrower domain" relationship as the fields
            // above.
            ("againstDownedDamage", "againstDownedDamage"),
            // Round 2: per-target source `applied_duration_ms`
            // (`lib.rs:1098`), whole-fight counterpart
            // `n_cc.applied_duration_ms` (`lib.rs:909`).
            ("appliedCrowdControlDuration", "appliedCrowdControlDuration"),
        ] {
            let sum: i64 =
                targets.iter().map(|t| t[0][split_field].as_i64().expect("integer")).sum();
            let whole = p["statsAll"][0][all_field].as_i64().expect("integer");
            checked2 += 1;
            assert!(
                sum <= whole,
                "{}: statsTargets sum of {split_field} ({sum}) exceeds statsAll[0].{all_field} \
                 ({whole}) -- the split can only ever be a subset",
                p["account"]
            );
            if sum == whole {
                *exact_counts.entry(split_field).or_insert(0) += 1;
            }
        }

        let dist = p["totalDamageDist"][0].as_array().expect("totalDamageDist[0]");
        for field in ["missed", "evaded", "blocked", "invulned"] {
            let sum: i64 = targets.iter().map(|t| t[0][field].as_i64().expect("integer")).sum();
            let whole: i64 = dist.iter().map(|r| r[field].as_i64().unwrap_or(0)).sum();
            checked2 += 1;
            assert!(
                sum <= whole,
                "{}: statsTargets sum of {field} ({sum}) exceeds totalDamageDist's summed \
                 {field} ({whole}) -- the split can only ever be a subset",
                p["account"]
            );
            if sum == whole {
                *exact_counts.entry(field).or_insert(0) += 1;
            }
        }
    }
    assert!(checked2 >= 100, "expected a non-degenerate comparison, checked {checked2}");
    for field in [
        "appliedCrowdControl",
        "criticalDmg",
        "flankingRate",
        "glanceRate",
        "connectedDirectDamageCount",
        "missed",
        "evaded",
        "blocked",
        "invulned",
        // Round 2 additions -- see the field list above for the source/
        // counterpart citation for each.
        "criticalRate",
        "againstDownedDamage",
        "appliedCrowdControlDuration",
    ] {
        assert!(
            exact_counts.get(field).copied().unwrap_or(0) > 0,
            "expected at least one player whose {field} statsTargets split sums EXACTLY to its \
             whole-fight counterpart -- zero exact means the split is systematically dropping \
             events for this field"
        );
    }
    println!(
        "ei_json_stats_targets_split_extended_invariants: {checked2} column sums checked across \
         12 new fields, exact-hit counts: {exact_counts:?}"
    );
    // Round 3 (A7): `directDmg` is the THIRD `statsTargets` key excluded
    // from the `sum <= whole` pass above, and it needs its own invariant
    // rather than a bare acknowledgement -- it is the one key the adapter
    // itself flags as the trap (`lib.rs:1049`: `directDmg` is
    // `direct_damage`, NOT the similarly-named whole-fight
    // `connected_direct_dmg`), so it is precisely the key where a
    // wrong-source wiring bug is plausible.
    //
    // Why `sum <= whole` genuinely cannot apply: the adapter emits
    // `directDmg` on `statsTargets` ONLY, never on `statsAll[0]`, so there
    // is no whole-fight scalar (and no `totalDamageDist` column either) to
    // sum against. What DOES apply is a reference-free PER-ROW ordering,
    // straight from the definitions in `per_target::build`:
    // `crit_damage` accumulates a subset of the rows `direct_damage` does
    // (crits are direct hits that additionally passed `can_crit` and
    // `is_crit`), and `direct_damage` in turn accumulates a subset of the
    // connected rows `totalDmg` covers. Hence
    // `criticalDmg <= directDmg <= totalDmg` on every row, with no external
    // reference needed.
    //
    // That chain is what makes this load-bearing: feeding `directDmg` from
    // `connected_direct_dmg` (the whole-fight near-miss field) breaks the
    // upper bound on every target the player also hit elsewhere, and
    // feeding it from a crit-scoped or count-scoped field breaks the lower
    // bound.
    let mut direct_rows = 0usize;
    let mut direct_nonzero = 0usize;
    for p in ei["players"].as_array().expect("players") {
        for t in p["statsTargets"].as_array().expect("statsTargets") {
            let row = t[0].as_object().expect("statsTargets row");
            // Ungated runs carry `totalDmg` alone; this pass only means
            // anything on the gated shape.
            let Some(direct) = row.get("directDmg").and_then(|v| v.as_i64()) else { continue };
            let crit = row["criticalDmg"].as_i64().expect("criticalDmg is an integer");
            let total = row["totalDmg"].as_i64().expect("totalDmg is an integer");
            direct_rows += 1;
            if direct != 0 {
                direct_nonzero += 1;
            }
            assert!(
                crit <= direct,
                "{}: statsTargets criticalDmg ({crit}) exceeds directDmg ({direct}) -- crits are \
                 a SUBSET of direct hits, so this row's directDmg is fed from the wrong source",
                p["account"]
            );
            assert!(
                direct <= total,
                "{}: statsTargets directDmg ({direct}) exceeds totalDmg ({total}) -- direct hits \
                 are a SUBSET of this target's damage, so this row's directDmg is fed from the \
                 wrong source (`connected_direct_dmg` is the documented near-miss trap)",
                p["account"]
            );
        }
    }
    assert!(
        direct_rows >= 1_000,
        "expected the gated statsTargets shape on this fixture, saw only {direct_rows} rows"
    );
    assert!(
        direct_nonzero >= 100,
        "only {direct_nonzero} of {direct_rows} rows have a nonzero directDmg -- an all-zero \
         column would satisfy both bounds vacuously"
    );
    println!(
        "ei_json_stats_targets_split_extended_invariants: directDmg ordering checked on \
         {direct_rows} rows ({direct_nonzero} nonzero)"
    );

    // Round 2: two more `statsTargets` keys were reviewed and are
    // legitimately excluded from the invariant pass above --
    // `appliedCrowdControlDownContribution` and
    // `appliedCrowdControlDurationDownContribution` have NO `statsAll[0]`
    // counterpart at all (verified: no such keys exist in the `stats_all`
    // block, `lib.rs:895-970` -- that block carries `appliedCrowdControl`/
    // `appliedCrowdControlDuration` but nothing down-contribution-shaped).
    // Unlike `missed`/`evaded`/`blocked`/`invulned` above, there is also no
    // `totalDamageDist`-style alternate whole to fall back to: down
    // contribution isn't tracked per-skill anywhere in the EI adapter's
    // output, only per-target. There is currently no independently
    // computable whole-fight total to check either field's split against
    // without the gitignored local reference export.
}

/// MEIGAP Task 2, committed-fixture structural gate: the three `targets[]`
/// mirrors and the two POWER series are ABSENT without their flags and
/// present, correctly shaped and internally consistent with them.
///
/// This is the half of Task 2's calibration CI can actually run: the
/// exact-vs-EI half (`meigap2_ei_golden.rs`) needs the gitignored local
/// export. What is pinned here is everything that is a CONTRACT rather
/// than a simulation -- the gates, the array lengths against GW2EI's own
/// `InterpolatedGraph` allocation, `power <= all` on every series, and the
/// `buffMap` rows without which axibridge drops `targets[].buffs` entirely.
#[test]
fn ei_json_meigap2_target_mirrors_are_gated_and_internally_consistent() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);

    // -- gated off --
    let plain = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let plain_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &plain, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    let off = axilog_ei::to_ei_json(&plain_v1, None);
    for p in off["players"].as_array().expect("players") {
        assert!(p.get("powerDamageTaken1S").is_none(), "powerDamageTaken1S must ride --timeseries");
        assert!(p.get("targetPowerDamage1S").is_none(), "targetPowerDamage1S must ride --timeseries");
    }
    for t in off["targets"].as_array().expect("targets") {
        for k in ["damage1S", "powerDamage1S", "totalDamageDist", "buffs"] {
            assert!(t.get(k).is_none(), "targets[].{k} must be gated, not emitted empty");
        }
    }
    // The condition rows must NOT join `buffMap` on a flagless render --
    // that is what keeps the always-on payload byte-identical.
    for &(id, _, _, _) in axilog_core::analysis::condition_catalog::CONDITION_BUFFS.iter() {
        assert!(
            off["buffMap"].get(format!("b{id}")).is_none(),
            "buffMap must not carry condition b{id} without targets[].buffs"
        );
    }

    // -- gated on --
    let enemies: std::collections::BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let enemy_addr_to_rep: std::collections::BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let series = axilog_core::analysis::timeseries::build_enemy_series(
        &enc, &raw, &registry, &enemies, &enemy_addr_to_rep,
    );
    let dist =
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &enemy_addr_to_rep);
    let conditions =
        axilog_core::analysis::target_conditions::build_with_registry(&raw, &registry, &enc);
    let full = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    // Task 7: `targets[].totalDamageDist` now comes off the native damage
    // block, so the pass enters through `Passes`, not `EiInputs`.
    let full_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &full, "0.0.0-test", None,
        &axilog_schema::v1::Passes { activity: Some(&activity),
            target_conditions: Some(&conditions),
            enemy_dist: Some(&dist),
            enemy_series: Some(&series),
            ..Default::default()
        },
    );
    let on = axilog_ei::to_ei_json(
        &full_v1,
        None,
    );

    // GW2EI's `InterpolatedGraph` allocation (`InterpolatedGraph.cs:18-20`).
    let secs = enc.duration_ms / 1000;
    let want_len =
        if secs * 1000 == enc.duration_ms { (secs + 1) as usize } else { (secs + 2) as usize };
    let phase0 = |v: &serde_json::Value| -> Vec<i64> {
        v[0].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(-1)).collect()).unwrap_or_default()
    };

    let target_count = on["targets"].as_array().expect("targets").len();
    let mut nonzero_target_series = 0usize;
    let mut dist_grand_total = 0i64;
    let mut series_grand_total = 0i64;
    for p in on["players"].as_array().expect("players") {
        let all = phase0(&p["damageTaken1S"]);
        let pow = phase0(&p["powerDamageTaken1S"]);
        assert_eq!(all.len(), want_len, "damageTaken1S must use GW2EI's grid length");
        assert_eq!(pow.len(), want_len, "powerDamageTaken1S must use GW2EI's grid length");
        assert!(pow.windows(2).all(|w| w[1] >= w[0]), "powerDamageTaken1S must be cumulative");
        assert!(
            pow.iter().zip(all.iter()).all(|(x, y)| x <= y),
            "powerDamageTaken1S must be element-wise <= damageTaken1S"
        );
        let tp = p["targetPowerDamage1S"].as_array().expect("targetPowerDamage1S");
        let ta = p["targetDamage1S"].as_array().expect("targetDamage1S");
        assert_eq!(tp.len(), target_count, "targetPowerDamage1S is indexed by targets[]");
        for (a, b) in ta.iter().zip(tp.iter()) {
            let (a, b) = (phase0(a), phase0(b));
            assert_eq!(b.len(), want_len);
            assert!(a.iter().zip(b.iter()).all(|(x, y)| y <= x), "target power <= target all");
        }
    }
    for t in on["targets"].as_array().expect("targets") {
        let (all, pow) = (phase0(&t["damage1S"]), phase0(&t["powerDamage1S"]));
        assert_eq!(all.len(), want_len, "targets[].damage1S must use GW2EI's grid length");
        assert_eq!(pow.len(), want_len);
        assert!(all.windows(2).all(|w| w[1] >= w[0]), "targets[].damage1S must be cumulative");
        assert!(pow.iter().zip(all.iter()).all(|(x, y)| x <= y));
        if all.last().copied().unwrap_or(0) > 0 {
            nonzero_target_series += 1;
        }
        // `totalDamageDist` is ACTOR-only where `damage1S` is
        // minion-INCLUSIVE (`GetJustActorDamageEvents` vs
        // `GetDamageEvents`, `SingleActor.cs:752-761`/`:735-740`), so the
        // two are compared in AGGREGATE rather than per row. Per-row `<=`
        // would be wrong in one direction on this project's shape: an
        // enemy's minion used to be a row in the pre-MROSTER unfiltered
        // `targets[]` roster while its own outgoing damage is credited to
        // its MASTER's series -- so a minion row legitimately reported a
        // nonzero `totalDamageDist` beside a zero `damage1S`. MROSTER
        // curated the roster to enemy PLAYERS, so on a WvW log the minion
        // is no longer listed at all (matching GW2EI, where the case never
        // arose); the aggregate comparison is kept because it is the
        // correct one for an actor-only-vs-minion-inclusive pair regardless,
        // and because a hand-built `Report` can still carry NPC rows.
        // Summed over the whole roster the fold cancels out.
        dist_grand_total += t["totalDamageDist"][0]
            .as_array()
            .expect("totalDamageDist[0]")
            .iter()
            .map(|e| e["totalDamage"].as_i64().expect("integer"))
            .sum::<i64>();
        series_grand_total += all.last().copied().unwrap_or(0);
        for b in t["buffs"].as_array().expect("buffs") {
            let id = b["id"].as_u64().expect("buff id") as u32;
            assert!(
                axilog_core::analysis::condition_catalog::is_condition_damage_based(id),
                "targets[].buffs is scoped to the condition catalog; saw b{id}"
            );
            assert!(
                on["buffMap"].get(format!("b{id}")).is_some(),
                "every emitted target buff id needs a buffMap row, or axibridge drops the entry"
            );
            for (_, states) in b["statesPerSource"].as_object().expect("statesPerSource") {
                let s = states.as_array().expect("state list");
                assert_eq!(
                    (s[0][0].as_i64(), s[0][1].as_i64()),
                    (Some(0), Some(0)),
                    "every statesPerSource timeline starts with GW2EI's mandatory [0, 0]"
                );
                assert!(
                    s.windows(2).all(|w| w[0][0].as_i64() <= w[1][0].as_i64()),
                    "statesPerSource times must be non-decreasing"
                );
            }
        }
    }
    assert!(
        dist_grand_total <= series_grand_total,
        "actor-only totalDamageDist across the roster ({dist_grand_total}) cannot exceed the \
         minion-inclusive damage1S total ({series_grand_total})"
    );
    assert!(dist_grand_total > 0, "degenerate: no enemy skill damage at all");
    assert!(
        nonzero_target_series >= 5,
        "expected a non-degenerate fixture: only {nonzero_target_series} targets dealt damage"
    );
    let with_buffs =
        on["targets"].as_array().expect("targets").iter().filter(|t| !t["buffs"].as_array().expect("buffs").is_empty()).count();
    assert!(with_buffs >= 5, "expected several targets to carry conditions, got {with_buffs}");
    println!(
        "ei_json_meigap2_target_mirrors: gated off cleanly; on, {target_count} targets \
         ({nonzero_target_series} damaging, {with_buffs} carrying conditions), all series \
         {want_len} long, power <= all everywhere"
    );
}

/// MEIGAP Task 2c, review round 1: the committed-fixture (PRE-rework era)
/// calibration for `targets[].totalDamageDist`, against real Elite Insights
/// output -- and the CI gate for the phantom-row class.
///
/// The source export for this fixture is non-`detailedWvW`, so its single
/// `targets[0]` is GW2EI's synthetic `Enemy Players` row: the AGGREGATE of
/// every enemy player's outgoing per-skill damage. That is exactly the shape
/// axibridge's own `precomputeGlobalEnemySkillStats`
/// (`packages/bridge-metrics/src/computePlayerAggregation.ts:490-509`)
/// reduces a detailed payload to, so folding axilog's `enemyPlayer` targets
/// the same way makes the two directly comparable.
///
/// This is the assertion that would have caught review finding 1 in CI:
/// before the fix, `build_enemy_dist` created a row for any non-statechange
/// combat item, including pre-rework buff APPLICATION rows, and 143 of the
/// 488 rows this very fixture emitted were phantoms GW2EI never emits --
/// 19 skill ids' worth once folded (199 ids vs the reference's 180).
/// Comparing the folded ID SET is what makes that visible; comparing only
/// values would let a phantom pass as `0 == 0`. The post-rework local
/// capture had none, so this pre-era fixture is the only place the class is
/// observable at all.
///
/// `min` is deliberately not compared: EI's aggregate row carries one `min`
/// over all enemy players combined, while the consumer's `minTotal/minCount`
/// averages per-target mins -- not the same statistic on this export shape.
/// The per-target and detailed-aggregate calibrations live in
/// `meigap2_ei_golden.rs` (local export, skipped in CI).
#[test]
fn ei_json_enemy_player_skill_dist_matches_the_golden_aggregate() {
    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/wvw-small.ei.json"
        ))
        .expect("read committed golden"),
    )
    .expect("parse committed golden");
    let want: std::collections::BTreeMap<i64, (i64, i64)> = golden["enemyPlayerSkillDist"]
        .as_object()
        .expect("enemyPlayerSkillDist")
        .iter()
        .map(|(k, v)| {
            (
                k.parse::<i64>().expect("skill id key"),
                (
                    v["totalDamage"].as_i64().expect("totalDamage"),
                    v["connectedHits"].as_i64().expect("connectedHits"),
                ),
            )
        })
        .collect();
    assert!(want.len() >= 150, "degenerate golden aggregate: {} ids", want.len());

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let enemies: std::collections::BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let enemy_addr_to_rep: std::collections::BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    let dist =
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &enemy_addr_to_rep);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, false, false, None,
    );
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes { activity: Some(&activity), enemy_dist: Some(&dist), ..Default::default() },
    );
    let ei = axilog_ei::to_ei_json(
        &report_v1, None,
    );

    let mut ours: std::collections::BTreeMap<i64, (i64, i64)> = std::collections::BTreeMap::new();
    for t in ei["targets"].as_array().expect("targets") {
        if !t["enemyPlayer"].as_bool().unwrap_or(false) {
            continue;
        }
        for e in t["totalDamageDist"][0].as_array().into_iter().flatten() {
            let Some(id) = e["id"].as_i64() else { continue };
            if id == 0 {
                continue;
            }
            let slot = ours.entry(id).or_insert((0, 0));
            slot.0 += e["totalDamage"].as_i64().unwrap_or(0);
            slot.1 += e["connectedHits"].as_i64().unwrap_or(0);
        }
    }

    let phantom: Vec<i64> = ours.keys().filter(|k| !want.contains_key(k)).copied().collect();
    let missing: Vec<i64> = want.keys().filter(|k| !ours.contains_key(k)).copied().collect();
    assert!(
        phantom.is_empty(),
        "{} PHANTOM skill id(s) in our enemy-player aggregate that real EI never emits: {:?}",
        phantom.len(),
        &phantom[..phantom.len().min(20)]
    );
    assert!(
        missing.is_empty(),
        "{} skill id(s) real EI emits that our enemy-player aggregate lacks: {:?}",
        missing.len(),
        &missing[..missing.len().min(20)]
    );

    let mut failures: Vec<String> = Vec::new();
    for (id, (wd, wh)) in &want {
        let (od, oh) = ours[id];
        if od != *wd {
            failures.push(format!("skill {id}.totalDamage: ours={od} reference={wd}"));
        }
        if oh != *wh {
            failures.push(format!("skill {id}.connectedHits: ours={oh} reference={wh}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} enemy-player aggregate mismatch(es) over {} skill ids:\n{}",
        failures.len(),
        want.len(),
        failures.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
    );
    println!(
        "ei_json_enemy_player_skill_dist_matches_the_golden_aggregate: {} skill ids, \
         totalDamage + connectedHits EXACT on all of them, 0 phantom / 0 missing rows",
        want.len()
    );
}

/// MEIGAP Task 3, committed-fixture gate: the healing/barrier detail
/// families, `minions[]` and `guildID` are ABSENT without their flags,
/// present and internally consistent with them, and the two figures the
/// committed golden already carries from REAL EI output are matched
/// exactly.
///
/// This is the half of Task 3's calibration CI can run without the
/// gitignored local export (`meigap3_ei_golden.rs` holds the exact-vs-EI
/// half). Two of the assertions below are genuine EI-reference joins, not
/// self-consistency: `squadHealingSelf` and `squadDownedHealing` in
/// `fixtures/wvw-small.ei.json` were extracted from the same real export
/// this fixture came from, and they pin the SELF diagonal of
/// `outgoingHealingAllies` and the `totalDownedHealing` column of
/// `totalHealingDist` respectively.
#[test]
fn ei_json_meigap3_healing_detail_minions_and_guild_are_gated_and_consistent() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);

    // -- gated off --
    let plain = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let plain_v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &plain, "0.0.0-test", None, &axilog_schema::v1::Passes { activity: Some(&activity), ..Default::default() });
    let off = axilog_ei::to_ei_json(&plain_v1, None);
    for p in off["players"].as_array().expect("players") {
        assert!(p.get("minions").is_none(), "minions[] must ride --skill-damage");
        let h = &p["extHealingStats"];
        for k in ["outgoingHealingAllies", "totalHealingDist", "healing1S"] {
            assert!(h.get(k).is_none(), "extHealingStats.{k} must be gated, not emitted empty");
        }
        for k in ["outgoingBarrierAllies", "totalBarrierDist"] {
            assert!(
                p["extBarrierStats"].get(k).is_none(),
                "extBarrierStats.{k} must be gated, not emitted empty"
            );
        }
        // ...while the M10 scalars stay always-on, as they always were.
        assert!(h["outgoingHealing"][0]["healing"].is_number());
    }

    // -- gated on --
    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let detail = axilog_core::analysis::healing_detail::build_with_registry(&raw, &registry, &enc)
        .expect("the committed fixture carries the healing extension");
    let minions = axilog_core::analysis::minions::build_with_registry(&raw, &registry, &enc);
    let full = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, false, None,
    );
    let full_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &full, "0.0.0-test", None,
        // Task 10: the healing detail reaches the adapter through the
        // native container now, and through BOTH `Passes` fields here
        // because this arm turns both flags on.
        &axilog_schema::v1::Passes { activity: Some(&activity),
            minions: Some(&minions),
            healing_detail: Some(&detail),
            healing_series: Some(&detail),
            ..Default::default()
        },
    );
    let on = axilog_ei::to_ei_json(
        &full_v1,
        None,
    );
    let players = on["players"].as_array().expect("players");
    let buckets = golden["series1SBuckets"].as_u64().expect("series1SBuckets") as usize;

    let mut self_healing_sum = 0i64;
    let mut downed_dist_sum = 0i64;
    let mut minion_groups = 0usize;
    let mut minion_rows = 0usize;
    for (i, p) in players.iter().enumerate() {
        let h = &p["extHealingStats"];
        let b = &p["extBarrierStats"];
        let scalar = h["outgoingHealing"][0]["healing"].as_i64().expect("scalar healing");

        // Ally matrices: one row per players[] entry, on both families.
        let allies = h["outgoingHealingAllies"].as_array().expect("outgoingHealingAllies");
        let b_allies = b["outgoingBarrierAllies"].as_array().expect("outgoingBarrierAllies");
        assert_eq!(allies.len(), players.len(), "the ally axis must be one row per player");
        assert_eq!(b_allies.len(), players.len(), "the barrier ally axis must match too");
        self_healing_sum += allies[i][0]["healing"].as_i64().unwrap_or(0);
        let ally_total: i64 = allies.iter().map(|c| c[0]["healing"].as_i64().unwrap_or(0)).sum();
        assert!(
            ally_total <= scalar,
            "the ally matrix can only ever be a SUBSET of the scalar total (heals landing on \
             non-enumerated friendlies are in the scalar and in no ally row)"
        );

        // `totalHealingDist` sums to the scalar EXACTLY -- the shared-producer
        // invariant `healing_detail`'s module doc claims.
        let dist = h["totalHealingDist"][0].as_array().expect("totalHealingDist");
        let dist_total: i64 = dist.iter().map(|r| r["totalHealing"].as_i64().unwrap_or(0)).sum();
        assert_eq!(dist_total, scalar, "totalHealingDist must sum to outgoingHealing[0].healing");
        downed_dist_sum +=
            dist.iter().map(|r| r["totalDownedHealing"].as_i64().unwrap_or(0)).sum::<i64>();
        let ids: Vec<i64> = dist.iter().map(|r| r["id"].as_i64().unwrap_or(-1)).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "totalHealingDist must be sorted by skill id");
        for r in dist {
            assert!(r["hits"].as_i64().unwrap_or(0) > 0, "a dist row exists only for real events");
            assert!(r["min"].as_i64().unwrap_or(0) <= r["max"].as_i64().unwrap_or(0));
            assert!(r["indirectHealing"].is_boolean());
        }
        let b_dist = b["totalBarrierDist"][0].as_array().expect("totalBarrierDist");
        let b_total: i64 = b_dist.iter().map(|r| r["totalBarrier"].as_i64().unwrap_or(0)).sum();
        assert_eq!(
            b_total,
            b["outgoingBarrier"][0]["barrier"].as_i64().expect("scalar barrier"),
            "totalBarrierDist must sum to outgoingBarrier[0].barrier"
        );
        let b_ally_total: i64 = b_allies.iter().map(|c| c[0]["barrier"].as_i64().unwrap_or(0)).sum();
        assert!(b_ally_total <= b_total, "the barrier ally matrix is a subset of the total");

        // `healing1S`: GW2EI's grid length, cumulative, ending at the scalar.
        let s: Vec<i64> =
            h["healing1S"][0].as_array().expect("healing1S").iter().map(|v| v.as_i64().unwrap_or(0)).collect();
        assert_eq!(s.len(), buckets, "healing1S must use GW2EI's InterpolatedGraph length");
        assert!(s.windows(2).all(|w| w[0] <= w[1]), "healing1S must be cumulative");
        assert_eq!(*s.last().expect("non-empty grid"), scalar, "healing1S must end at the total");

        // `minions[]`: present only for players who have them, well-formed.
        if let Some(ms) = p.get("minions") {
            let ms = ms.as_array().expect("minions array");
            assert!(!ms.is_empty(), "an empty minions[] must be omitted, not emitted");
            minion_groups += ms.len();
            for m in ms {
                assert!(m["name"].is_string());
                let rows = m["totalDamageTakenDist"][0].as_array().expect("totalDamageTakenDist");
                minion_rows += rows.len();
                let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap_or(-1)).collect();
                assert!(ids.windows(2).all(|w| w[0] < w[1]), "minion dist must be sorted by skill id");
                for r in rows {
                    let (hits, conn) = (r["hits"].as_i64().unwrap_or(0), r["connectedHits"].as_i64().unwrap_or(0));
                    assert!(conn <= hits, "connectedHits ({conn}) can never exceed hits ({hits})");
                    if r["indirectDamage"].as_bool().unwrap_or(false) {
                        for k in ["blocked", "evaded", "glance", "missed", "interrupted"] {
                            assert_eq!(r[k], 0, "GW2EI zeroes {k} on an indirect skill");
                        }
                    }
                }
            }
        }

        // `guildID`: the committed fixture is anonymized, so every guild
        // row's payload is zeroed (`evtc::anonymize`'s guild pass) and the
        // only value that may appear is GW2EI's own all-zero GUID.
        if let Some(g) = p.get("guildID") {
            assert_eq!(
                g.as_str(), Some("00000000-0000-0000-0000-000000000000"),
                "the committed fixture must never carry a real guild GUID"
            );
        }
    }

    // -- the two real-EI joins --
    assert_eq!(
        self_healing_sum,
        golden["squadHealingSelf"].as_i64().expect("squadHealingSelf"),
        "the SELF diagonal of outgoingHealingAllies must equal real EI's squad self-healing"
    );
    assert_eq!(
        downed_dist_sum,
        golden["squadDownedHealing"].as_i64().expect("squadDownedHealing"),
        "totalHealingDist's downed column must equal real EI's squad downed healing"
    );

    assert!(minion_groups >= 10, "expected a non-degenerate minion set, got {minion_groups}");
    assert!(minion_rows >= 50, "expected a non-degenerate minion dist, got {minion_rows}");

    // -- the real-EI minion join --
    //
    // `fixtures/wvw-small.ei.json`'s `minionDamageTaken` block was extracted
    // verbatim from the same source export the file's `_source` names,
    // bucketed by axibridge's own `normalizeMinionName`. This is REAL EI
    // output for the pre-rework era, i.e. exactly the coverage the local
    // post-rework calibration (`meigap3_ei_golden.rs`) cannot give CI.
    let mut ours_by_name: HashMap<String, (i64, i64)> = HashMap::new(); // rows, damage
    for p in players {
        for m in p.get("minions").and_then(|v| v.as_array()).into_iter().flatten() {
            let raw = m["name"].as_str().unwrap_or("Unknown");
            let stripped = raw.strip_prefix("Juvenile ").unwrap_or(raw);
            let name = if stripped.to_uppercase().contains("UNKNOWN") {
                "Unknown".to_string()
            } else {
                stripped.to_string()
            };
            let rows = m["totalDamageTakenDist"][0].as_array().expect("dist");
            let e = ours_by_name.entry(name).or_insert((0, 0));
            e.0 += rows.len() as i64;
            e.1 += rows.iter().map(|r| r["totalDamage"].as_i64().unwrap_or(0)).sum::<i64>();
        }
    }
    let reference = &golden["minionDamageTaken"];
    let ref_by_name = reference["byName"].as_object().expect("byName");

    // **Every EI minion name that took damage must match EXACTLY**, on both
    // the row count and the damage total. This is the assertion that would
    // catch a real regression in the classifier, the ownership fold or the
    // event-creation gate.
    let mut nonzero_names = 0usize;
    for (name, r) in ref_by_name {
        let ref_damage = r["totalDamage"].as_i64().unwrap_or(0);
        if ref_damage == 0 {
            continue;
        }
        // The ONE documented exclusion (MEIGAP Task 3b): GW2EI's own
        // `UNKNOWN <id>` placeholder group is an englobed agent, which
        // `model::agent_kind` classifies as a PLAYER (`is_elite !=
        // 0xffffffff`) and which is therefore never anyone's minion here.
        // It is not skipped silently -- its damage appears explicitly in
        // the squad-wide identity asserted just below, so removing it from
        // the reference would break that instead.
        if name == "Unknown" {
            continue;
        }
        nonzero_names += 1;
        let (rows, damage) = ours_by_name.get(name).copied().unwrap_or_else(|| {
            panic!("minion group {name} took {ref_damage} damage in real EI output and is absent here")
        });
        assert_eq!(
            damage, ref_damage,
            "minion {name}: damage-taken total must match real EI exactly"
        );
        assert_eq!(
            rows, r["rows"].as_i64().unwrap_or(0),
            "minion {name}: damage-taken dist row count must match real EI exactly"
        );
    }
    assert!(nonzero_names >= 8, "expected a non-degenerate join, got {nonzero_names} names");

    // Row COUNT over the whole squad is exact; the damage TOTAL is not, and
    // the entire difference is the two carve-outs MEIGAP Task 3b documents,
    // reproduced here as an exact identity rather than a tolerance:
    //
    //   ours = EI - (EI's "Unknown" englobed-agent group)
    //             + (groups EI does not treat as minions at all)
    //
    // `Unknown` is the englobed agent `model::agent_kind` classifies as a
    // PLAYER; `Continuum Rift` and the unnamed `ch<species>-<id>` agents are
    // masters-resolved agents GW2EI's own NPC classification excludes.
    let ours_rows: i64 = ours_by_name.values().map(|v| v.0).sum();
    let ours_damage: i64 = ours_by_name.values().map(|v| v.1).sum();
    assert_eq!(
        ours_rows, reference["rows"].as_i64().expect("rows"),
        "squad-wide minion dist row count must match real EI exactly"
    );
    let ei_only_unknown = ref_by_name
        .get("Unknown")
        .and_then(|v| v["totalDamage"].as_i64())
        .expect("the reference carries an englobed-agent Unknown group");
    let ours_only: i64 = ours_by_name
        .iter()
        .filter(|(n, _)| !ref_by_name.contains_key(*n))
        .map(|(_, v)| v.1)
        .sum();
    assert_eq!(
        ours_damage,
        reference["totalDamage"].as_i64().expect("totalDamage") - ei_only_unknown + ours_only,
        "the squad-wide minion damage delta must be EXACTLY the two documented carve-outs"
    );

    // -- the real-EI healing-dist joins --
    //
    // BOUNDED, deliberately: M10's `healing_golden.rs` records a
    // root-caused peer-sanitization residual on this project's healing
    // TOTALS for the PRE-rework era (the post-rework calibration in
    // `meigap3_ei_golden.rs` is exact). The row COUNT and the ally-matrix
    // sum are exact even here, so those two are asserted exactly and only
    // the total is bounded.
    let ref_rows = golden["squadHealingDistRows"].as_i64().expect("squadHealingDistRows");
    let ref_allies = golden["squadOutgoingHealingAlliesTotal"].as_i64().expect("squadOutgoingHealingAlliesTotal");
    let ref_total = golden["squadHealingTotal"].as_i64().expect("squadHealingTotal");
    let mut our_rows = 0i64;
    let mut our_total = 0i64;
    let mut our_allies = 0i64;
    for p in players {
        let h = &p["extHealingStats"];
        let d = h["totalHealingDist"][0].as_array().expect("totalHealingDist");
        our_rows += d.len() as i64;
        our_total += d.iter().map(|r| r["totalHealing"].as_i64().unwrap_or(0)).sum::<i64>();
        for a in h["outgoingHealingAllies"].as_array().expect("allies") {
            our_allies += a[0]["healing"].as_i64().unwrap_or(0);
        }
    }
    assert_eq!(
        our_allies, ref_allies,
        "the whole outgoingHealingAllies matrix must sum to real EI's exactly"
    );
    assert!(
        (our_rows - ref_rows).abs() <= HEALING_DIST_ROW_BOUND,
        "totalHealingDist row count {our_rows} vs real EI's {ref_rows} exceeds the pinned bound"
    );
    let rel = (our_total - ref_total) as f64 / ref_total as f64;
    assert!(
        rel.abs() <= HEALING_DIST_TOTAL_TOLERANCE,
        "totalHealingDist squad total {our_total} vs real EI's {ref_total} is {:.4} relative, past the M10 residual bound",
        rel
    );
    println!(
        "ei_json_meigap3 real-EI join: {nonzero_names} damaged minion names exact, {ours_rows} dist rows exact, \
         healing allies {our_allies} exact, healing dist rows {our_rows} vs {ref_rows}, total rel {rel:+.5}"
    );
    println!(
        "ei_json_meigap3: {} players, self-healing {self_healing_sum}, downed {downed_dist_sum}, \
         {minion_groups} minion groups / {minion_rows} rows",
        players.len()
    );
}

/// `totalHealingDist` squad row count vs real EI's, on the PRE-rework
/// committed fixture. Measured 212 vs 211 -- one extra skill id, from the
/// same M10 peer-sanitization residual as the total below (a skill whose
/// only surviving events differ between the two `SanitizeForSrc` runs).
const HEALING_DIST_ROW_BOUND: i64 = 2;

/// Relative bound on the squad-wide `totalHealingDist` total vs real EI's,
/// on the PRE-rework committed fixture. Measured +0.68% (1,015,275 vs
/// 1,008,414) -- this is M10's documented, root-caused repeating-skill
/// peer-report residual (`analysis::healing`'s module doc), inherited whole
/// and not made worse here. The post-rework calibration
/// (`meigap3_ei_golden.rs`) is EXACT, which is why this is the only place a
/// tolerance appears on the healing detail at all.
const HEALING_DIST_TOTAL_TOLERANCE: f64 = 0.01;
