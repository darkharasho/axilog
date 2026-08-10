//! M16 Task 3: **emission** calibration for the damage-modifier surface.
//!
//! `axilog-core`'s `tests/damage_mods_golden.rs` proves the ENGINE -- the
//! four numbers, per `(account, modifier)`, against the reference export.
//! This file proves the ei-json ADAPTER on top of it: that the numbers
//! reach `damageModifiers`/`incomingDamageModifiers`/`damageModifiersTarget`
//! /`incomingDamageModifiersTarget` in EI's exact shape, that
//! `damageModMap`'s eight descriptor fields are reproduced character for
//! character, and -- the part a numeric comparison would miss -- that the
//! serialized JSON **text** matches, `damageGain`'s `double` formatting
//! included.
//!
//! Why text and not values: `damageGain` is a C# `double` on both sides
//! (`GW2EIEvtcParser/EIData/Statistics/DamageModifierStat.cs:7` and
//! `GW2EIJSON/.../JsonDamageModifierData.cs:28`), `Math.Round(_, 3)`-ed at
//! construction. M15's replay work established that this project's floats
//! must be emitted through whatever numeric WIDTH GW2EI actually used, and
//! the trap here is the opposite of M15's: routing `damageGain` through
//! `axilog_ei`'s `f32`-narrowing `ei_float` would silently corrupt every
//! value past `f32`'s 24-bit integer range (the export has `damageGain`
//! values above 279,000) and every 3-decimal value with no exact `f32`
//! decimal. Comparing `serde_json`'s emitted text against the export's own
//! text is what catches that; comparing `f64`s parsed from both sides would
//! not.
//!
//! Two fixtures, two standards:
//!
//! - the gitignored local capture pair
//!   (`fixtures/local/wvw-postrework.{zevtc,ei.json}`) carries a real
//!   `damageModMap` and real modifier arrays, so it is compared for real.
//!   SKIPS when absent, like every other `*_golden.rs`. Point
//!   `AXILOG_LOCAL_FIXTURES` at the primary checkout to run it from a
//!   worktree.
//! - the committed fixture (`fixtures/wvw-small.{anon.zevtc,ei.json}`) has
//!   **no** `damageModMap` and no modifier arrays at all -- it is a trimmed
//!   golden, not a full EI export (Task 2 established this), so there is
//!   nothing to compare against and the committed-fixture test asserts
//!   SHAPE and gating only. That is a deliberate, documented limit, not an
//!   oversight: extending it would mean committing a fuller export, which
//!   would carry PII.

use axilog_core::analysis::damage::InstidRegistry;
use axilog_core::analysis::damage_mods::evaluate_catalog_full;
use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_ei::EiInputs;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const COMMITTED_GOLDEN_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

/// The ids `axilog-core`'s `damage_mods_golden.rs` declares
/// `IdBound::exact` -- every row of every account matched GW2EI on all four
/// fields. Those are the only ids whose EMISSION can be asserted
/// text-exact here; the rest carry a documented buff-simulator residual
/// (see that file's `ID_BOUNDS` writeup) and are asserted shape-and-bounds
/// through the engine contract instead, not re-litigated here.
///
/// Duplicated rather than shared because `damage_mods_golden.rs` is an
/// integration test in a DIFFERENT crate (`axilog-core`), which cannot be
/// imported from here -- and `axilog-ei` depends on `axilog-core`, so the
/// dependency cannot go the other way either. The duplication is guarded on
/// both sides: the count is asserted below, and any id in this list whose
/// emitted text does NOT match the export fails hard, so the two lists
/// cannot silently drift apart in the direction that matters.
/// MBUFFSIM Task 3 re-seed: `d369` (Chant of Action) promoted from bounded to
/// exact once rule 1 stopped cancelling the buff on its own overstack report.
/// MATTRIB Task 2 re-seed: seven more (`d-389`, `d-368`, `d-336`, `d-126`,
/// `d-62`, `d-59`, `d-54`) promoted once the incoming denominator stopped
/// dropping self-inflicted condition ticks -- see that file's
/// tracked-cause-2 note.
const TEXT_EXACT_IDS: [i32; 38] = [
    -411, -390, -389, -376, -370, -368, -336, -176, -129, -128, -126, -99, -94, -93, -78, //
    -62, -61, -59, -54, //
    10, 11, 18, 21, 25, 36, 93, 98, 119, 170, 175, 313, 319, 361, 362, 364, 369, 374, 403,
];

/// `fixtures/local/` path, honouring `AXILOG_LOCAL_FIXTURES` -- same
/// resolution `axilog-core`'s `tests/common::local_fixture` does (that
/// module lives under `axilog-core/tests/`, which this crate cannot pull
/// in).
fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

/// `:Account.1234` -> `Account.1234` (the export drops EI's leading colon
/// on some captures and not others).
fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

/// One player's `{ signed id -> the phase-0 item object }`, merging the
/// outgoing and incoming arrays. Ids are already signed, so they cannot
/// collide (`DamageModifier.cs:26`).
fn rows_by_id(p: &Value, outgoing_key: &str, incoming_key: &str) -> BTreeMap<i32, Value> {
    let mut out = BTreeMap::new();
    for key in [outgoing_key, incoming_key] {
        for entry in p[key].as_array().into_iter().flatten() {
            let (Some(id), Some(items)) =
                (entry["id"].as_i64(), entry["damageModifiers"].as_array())
            else {
                continue;
            };
            assert_eq!(
                items.len(),
                1,
                "this project models one phase; a {key} entry with {} phases means the \
                 reference log is phased and this comparison is not apples-to-apples",
                items.len()
            );
            out.insert(id as i32, items[0].clone());
        }
    }
    out
}

/// Canonical text for one emitted item, field by field in EI's declaration
/// order -- `serde_json::to_string` on a `Value` sorts keys, which would
/// hide nothing but also compares nothing about ordering, so the four
/// fields are pulled out and stringified individually. `damageGain` is the
/// one that matters: `Value::to_string` reproduces exactly the decimal
/// `serde_json` will write into the real output.
fn item_text(v: &Value) -> String {
    format!(
        "hitCount={} totalHitCount={} damageGain={} totalDamage={}",
        v["hitCount"], v["totalHitCount"], v["damageGain"], v["totalDamage"]
    )
}

#[test]
fn ei_json_damage_modifiers_match_the_reference_export_text_when_available() {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let ei_json = local_fixture("wvw-postrework.ei.json");
    let Ok(bytes) = std::fs::read(&zevtc) else {
        println!("skip: {zevtc} absent (ei-json damage-modifier emission calibration)");
        return;
    };
    let Ok(golden_s) = std::fs::read_to_string(&ei_json) else {
        println!("skip: {ei_json} absent (ei-json damage-modifier emission calibration)");
        return;
    };
    let golden: Value = serde_json::from_str(&golden_s).expect("parse reference export");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let registry = InstidRegistry::build(&raw);
    let mods = evaluate_catalog_full(&raw, &registry, &enc, true);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, Some(&mods),
    );
    let ours = axilog_ei::to_ei_json(
        &report,
        &EiInputs { activity: &activity, replay: None, modifiers: Some(&mods), boon_states: None },
    );

    // ---- damageModMap: the descriptor table, character for character ----
    let g_map = golden["damageModMap"].as_object().expect("reference damageModMap");
    let o_map = ours["damageModMap"].as_object().expect("emitted damageModMap");
    assert!(!o_map.is_empty(), "emitted damageModMap must not be empty");

    // The exact field set, verified against the export rather than assumed
    // -- this is the assertion that would catch EI adding or renaming a
    // descriptor field.
    let expected_fields: BTreeSet<&str> = [
        "name", "icon", "description", "nonMultiplier", "isCounter", "skillBased", "approximate",
        "incoming",
    ]
    .into_iter()
    .collect();
    for (k, v) in g_map {
        let got: BTreeSet<&str> = v.as_object().expect("descriptor object").keys().map(|s| s.as_str()).collect();
        assert_eq!(got, expected_fields, "reference damageModMap.{k} field set changed");
    }

    let mut map_failures: Vec<String> = Vec::new();
    for (k, o) in o_map {
        let Some(g) = g_map.get(k) else {
            map_failures.push(format!("{k}: emitted but absent from the reference export"));
            continue;
        };
        let got: BTreeSet<&str> = o.as_object().expect("descriptor object").keys().map(|s| s.as_str()).collect();
        if got != expected_fields {
            map_failures.push(format!("{k}: emitted field set {got:?} != {expected_fields:?}"));
            continue;
        }
        for f in &expected_fields {
            if o[*f] != g[*f] {
                map_failures.push(format!("{k}.{f}: emitted {} != reference {}", o[*f], g[*f]));
            }
        }
    }
    let map_matched = o_map.len() - map_failures.len().min(o_map.len());
    println!(
        "damageModMap: {} emitted, {} of the reference's {} ids, {map_matched} fully identical",
        o_map.len(),
        o_map.keys().filter(|k| g_map.contains_key(*k)).count(),
        g_map.len()
    );
    assert!(
        map_failures.is_empty(),
        "damageModMap descriptor mismatches ({} of {}):\n{}",
        map_failures.len(),
        o_map.len(),
        map_failures.join("\n")
    );

    // ---- the four per-player arrays ----
    let mut golden_players: BTreeMap<&str, &Value> = BTreeMap::new();
    for p in golden["players"].as_array().expect("reference players") {
        if let Some(a) = p["account"].as_str() {
            golden_players.insert(account_key(a), p);
        }
    }
    // -- the target join, and why it cannot be positional --
    //
    // `damageModifiersTarget` is `[targetIndex]`-keyed on both sides, but
    // the two `targets[]` rosters are NOT the same list: GW2EI's WvW logic
    // exposes 57 targets on this capture (the enemy PLAYERS, named
    // `"<Spec> pl-<instid>"`, plus one `Dummy PvP Agent`), while this
    // project's `targets[]` is deliberately the FULL unfiltered enemy
    // roster -- 624 entries here, every gadget, siege, dolyak and guard the
    // log enumerated (`axilog_schema::Report::all_enemies`'s doc comment
    // explains why it stays unfiltered, and `axilog_ei`'s own
    // `stats_targets` comment records the positional-lockstep contract that
    // depends on it). Their name spaces do not even intersect.
    //
    // So the index means something different on each side, and a positional
    // comparison would be nonsense. Nor do the NAMES join: this capture's
    // enemy players have no character name on the wire (WvW hides them), so
    // GW2EI falls back to `"<Spec> pl-<instid>"` while this project shows
    // the WvW rank title ("Diamond Champion"). The one thing both sides
    // agree on is the arcdps AGENT IDENTITY, so the join goes through the
    // instid GW2EI put in that name suffix -> the addr that instid belonged
    // to -> this project's enemy representative -> its `targets[]` index.
    //
    // (`Encounter::Enemy::instid` cannot shortcut this: it is 0 for every
    // enemy on this capture -- `wvw::apply` keys enemies by addr and never
    // fills it in. Noted rather than changed; touching enemy resolution is
    // not this milestone's business.)
    //
    // This is a pre-existing `targets[]` divergence, not one M16 introduced
    // -- but it IS the reason the per-target arrays, unlike the whole-fight
    // ones, are not index-compatible with a real EI client. Recorded in the
    // README's parity row.
    let mut instid_to_addrs: BTreeMap<u16, BTreeSet<u64>> = BTreeMap::new();
    for ev in &raw.events {
        for (instid, addr) in [(ev.src_instid, ev.src_agent), (ev.dst_instid, ev.dst_agent)] {
            if instid != 0 && addr != 0 {
                instid_to_addrs.entry(instid).or_default().insert(addr);
            }
        }
    }
    let addr_to_enemy_index: BTreeMap<u64, usize> = enc
        .enemies
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.agent_addrs.iter().map(move |&a| (a, i)))
        .collect();
    let n_o_targets = ours["targets"].as_array().expect("emitted targets").len();
    let mut joinable: Vec<(usize, usize, u16)> = Vec::new();
    for (g_i, t) in golden["targets"].as_array().expect("reference targets").iter().enumerate() {
        let Some(instid) = t["name"].as_str().and_then(|n| n.rsplit_once("pl-")).and_then(|(_, s)| s.parse::<u16>().ok())
        else {
            continue;
        };
        // Only an UNAMBIGUOUS instid can be joined: arcdps reuses instids
        // across agents, so one resolving to more than one enemy is skipped
        // rather than guessed at.
        let indices: BTreeSet<usize> = instid_to_addrs
            .get(&instid)
            .into_iter()
            .flatten()
            .filter_map(|a| addr_to_enemy_index.get(a).copied())
            .collect();
        if indices.len() == 1 {
            joinable.push((*indices.iter().next().expect("len == 1"), g_i, instid));
        }
    }
    println!(
        "targets: {n_o_targets} emitted vs {} in the reference; \
         {} enemy-player instids joined",
        golden["targets"].as_array().unwrap().len(),
        joinable.len()
    );

    let exact_ids: BTreeSet<i32> = TEXT_EXACT_IDS.into_iter().collect();
    assert_eq!(
        exact_ids.len(),
        38,
        "TEXT_EXACT_IDS must hold 38 DISTINCT ids -- it mirrors damage_mods_golden.rs's \
         38 `IdBound::exact` rows"
    );

    let mut failures: Vec<String> = Vec::new();
    let (mut joined, mut compared, mut identical) = (0usize, 0usize, 0usize);
    let (mut t_compared, mut t_identical, mut t_slots) = (0usize, 0usize, 0usize);

    for (idx, p) in enc.players.iter().enumerate() {
        let Some(g_p) = golden_players.get(account_key(&p.account)) else { continue };
        joined += 1;
        let o_p = &ours["players"][idx];
        assert_eq!(
            o_p["account"].as_str().map(account_key),
            Some(account_key(&p.account)),
            "emitted players[] is positionally keyed to enc.players"
        );

        // -- whole-fight pair --
        let g_rows = rows_by_id(g_p, "damageModifiers", "incomingDamageModifiers");
        let o_rows = rows_by_id(o_p, "damageModifiers", "incomingDamageModifiers");
        for &id in &exact_ids {
            match (g_rows.get(&id), o_rows.get(&id)) {
                (Some(g), Some(o)) => {
                    compared += 1;
                    if item_text(g) == item_text(o) {
                        identical += 1;
                    } else {
                        failures.push(format!(
                            "d{id} player#{joined}: emitted [{}] != reference [{}]",
                            item_text(o),
                            item_text(g)
                        ));
                    }
                }
                (Some(g), None) => failures.push(format!(
                    "d{id} player#{joined}: reference has [{}], we emitted no row",
                    item_text(g)
                )),
                (None, Some(o)) => failures.push(format!(
                    "d{id} player#{joined}: we emitted [{}], reference has no row",
                    item_text(o)
                )),
                (None, None) => {}
            }
        }

        // -- Target pair -- joined by enemy instid, not index (see above).
        for out_key in ["damageModifiersTarget", "incomingDamageModifiersTarget"] {
            let g_arr = g_p[out_key].as_array().expect("reference target array");
            let o_arr = o_p[out_key].as_array().expect("emitted target array");
            assert_eq!(
                o_arr.len(),
                n_o_targets,
                "{out_key} must have exactly one slot per targets[] entry"
            );
            t_slots += o_arr.len();
            for &(o_i, g_i, instid) in &joinable {
                let (Some(g_slot_v), Some(o_slot_v)) = (g_arr.get(g_i), o_arr.get(o_i)) else {
                    continue;
                };
                let g_slot = target_slot_rows(g_slot_v);
                let o_slot = target_slot_rows(o_slot_v);
                for &id in &exact_ids {
                    match (g_slot.get(&id), o_slot.get(&id)) {
                        (Some(g), Some(o)) => {
                            t_compared += 1;
                            if item_text(g) == item_text(o) {
                                t_identical += 1;
                            } else {
                                failures.push(format!(
                                    "{out_key} d{id} player#{joined} target pl-{instid}: \
                                     emitted [{}] != reference [{}]",
                                    item_text(o),
                                    item_text(g)
                                ));
                            }
                        }
                        (Some(g), None) => failures.push(format!(
                            "{out_key} d{id} player#{joined} target pl-{instid}: reference has \
                             [{}], we emitted no row",
                            item_text(g)
                        )),
                        (None, Some(o)) => failures.push(format!(
                            "{out_key} d{id} player#{joined} target pl-{instid}: we emitted \
                             [{}], reference has no row",
                            item_text(o)
                        )),
                        (None, None) => {}
                    }
                }
            }
        }
    }

    println!(
        "whole-fight: {identical}/{compared} rows text-identical across {joined} account(s), \
         {} hard-exact ids\nper-target:  {t_identical}/{t_compared} rows text-identical over \
         {t_slots} emitted target slots",
        exact_ids.len()
    );

    // Guards against the harness silently degrading into a no-op.
    assert!(joined >= 44, "only {joined} account(s) joined -- expected at least 44");
    assert!(
        compared >= 200,
        "only {compared} whole-fight rows compared -- the hard-exact join degraded"
    );
    assert!(
        t_compared >= 20,
        "only {t_compared} per-target rows compared -- the per-target emission or the \
         name join degraded"
    );
    assert!(
        failures.is_empty(),
        "{} ei-json damage-modifier emission mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// One `damageModifiersTarget[i]`/`incomingDamageModifiersTarget[i]` slot -> `{ signed id -> phase-0 item }`.
fn target_slot_rows(slot: &Value) -> BTreeMap<i32, Value> {
    let mut out = BTreeMap::new();
    for entry in slot.as_array().into_iter().flatten() {
        let (Some(id), Some(items)) = (entry["id"].as_i64(), entry["damageModifiers"].as_array())
        else {
            continue;
        };
        if let Some(first) = items.first() {
            out.insert(id as i32, first.clone());
        }
    }
    out
}

/// The committed fixture: shape and gating only.
///
/// The fixture-era export (`fixtures/wvw-small.ei.json`) carries **no**
/// `damageModMap` and no modifier arrays -- Task 2 established that it is a
/// trimmed golden rather than a full EI export -- so this asserts what CAN
/// be asserted without PII: that the flagless output is unchanged, that the
/// gated output has EI's exact shape, and that `damageGain` is emitted as a
/// `double` (never a narrowed `f32`, never a quoted string).
#[test]
fn committed_fixture_damage_modifier_emission_is_correctly_shaped_and_gated() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH).expect("read committed fixture");
    let committed: Value = serde_json::from_str(
        &std::fs::read_to_string(COMMITTED_GOLDEN_PATH).expect("read committed golden"),
    )
    .expect("parse committed golden");
    // The documented reason this test is shape-only. If a future fuller
    // export ever lands here, this assertion fires and the test above's
    // real comparison should be extended to cover it.
    assert!(
        committed.get("damageModMap").is_none(),
        "fixtures/wvw-small.ei.json now HAS a damageModMap -- extend this test into a real \
         text comparison instead of the shape-only assertions below"
    );

    let raw = decode_raw(&bytes).expect("decode committed fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);
    let registry = InstidRegistry::build(&raw);

    // -- gating: absent without the option, on BOTH surfaces --
    let plain =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let plain_ei =
        axilog_ei::to_ei_json(&plain, &EiInputs { activity: &activity, ..Default::default() });
    assert!(plain_ei.get("damageModMap").is_none(), "damageModMap must be omitted when not requested");
    for p in plain_ei["players"].as_array().expect("players") {
        for k in [
            "damageModifiers",
            "incomingDamageModifiers",
            "damageModifiersTarget",
            "incomingDamageModifiersTarget",
        ] {
            assert!(p.get(k).is_none(), "{k} must be omitted when not requested");
        }
    }
    let plain_native = serde_json::to_value(&plain).expect("serialize native report");
    assert!(plain_native.get("damage_mod_map").is_none(), "native damage_mod_map must be omitted");
    assert!(
        plain_native["players"][0].get("damage_mods").is_none(),
        "native players[].damage_mods must be omitted"
    );

    // -- shape: present, and exactly EI's, with the option --
    let mods = evaluate_catalog_full(&raw, &registry, &enc, true);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, Some(&mods),
    );
    let ei = axilog_ei::to_ei_json(
        &report,
        &EiInputs { activity: &activity, replay: None, modifiers: Some(&mods), boon_states: None },
    );

    let map = ei["damageModMap"].as_object().expect("damageModMap present when requested");
    assert!(!map.is_empty(), "the committed fixture triggers at least one modifier");
    for (k, v) in map {
        assert!(k.starts_with('d'), "damageModMap keys carry EI's 'd' prefix, got {k}");
        let id: i32 = k[1..].parse().unwrap_or_else(|_| panic!("damageModMap key {k} is not d<i32>"));
        let fields: BTreeSet<&str> = v.as_object().expect("descriptor").keys().map(|s| s.as_str()).collect();
        assert_eq!(
            fields,
            ["name", "icon", "description", "nonMultiplier", "isCounter", "skillBased", "approximate", "incoming"]
                .into_iter()
                .collect::<BTreeSet<_>>(),
            "damageModMap.{k} field set"
        );
        // The sign of the id IS the direction (`DamageModifier.cs:26`).
        assert_eq!(v["incoming"], Value::Bool(id < 0), "damageModMap.{k}.incoming must match the id's sign");
        // Every suffix `DamageModifier`'s ctor appends is derived, so the
        // description must always carry the two unconditional ones.
        let desc = v["description"].as_str().expect("description is a string");
        assert!(desc.contains("<br>Applied on "), "damageModMap.{k}.description missing 'Applied on'");
        assert!(
            desc.contains("<br>Compared against "),
            "damageModMap.{k}.description missing 'Compared against'"
        );
        if v["isCounter"] == Value::Bool(true) {
            assert!(desc.ends_with("<br>Counter") || desc.contains("<br>Counter"),
                "damageModMap.{k} is a counter but its description says so nowhere");
        }
    }

    let n_targets = ei["targets"].as_array().expect("targets").len();
    let referenced: BTreeSet<i32> = map
        .keys()
        .map(|k| k[1..].parse::<i32>().expect("d<i32>"))
        .collect();
    let (mut players_with_rows, mut rows, mut target_rows) = (0usize, 0usize, 0usize);
    for p in ei["players"].as_array().expect("players") {
        let mut any = false;
        for (k, incoming) in [("damageModifiers", false), ("incomingDamageModifiers", true)] {
            for entry in p[k].as_array().unwrap_or_else(|| panic!("{k} must be an array")) {
                let id = entry["id"].as_i64().expect("id") as i32;
                assert_eq!(id < 0, incoming, "{k} entry d{id} is on the wrong side");
                assert!(referenced.contains(&id), "{k} entry d{id} has no damageModMap entry");
                let items = entry["damageModifiers"].as_array().expect("per-phase array");
                assert_eq!(items.len(), 1, "one phase per entry");
                let it = &items[0];
                assert!(it["hitCount"].is_u64(), "hitCount must be an integer");
                assert!(it["totalHitCount"].is_u64(), "totalHitCount must be an integer");
                assert!(it["totalDamage"].is_u64(), "totalDamage must be an integer");
                // `damageGain` is a NUMBER (never a string), and an
                // integral value is emitted as an integer -- what .NET
                // writes for a whole `double`.
                assert!(it["damageGain"].is_number(), "damageGain must be a JSON number");
                assert!(
                    it["hitCount"].as_u64().unwrap() >= 1,
                    "EI only emits a row when the modifier applied at least once"
                );
                assert!(
                    it["hitCount"].as_u64().unwrap() <= it["totalHitCount"].as_u64().unwrap(),
                    "hitCount cannot exceed totalHitCount"
                );
                rows += 1;
                any = true;
            }
        }
        for k in ["damageModifiersTarget", "incomingDamageModifiersTarget"] {
            let arr = p[k].as_array().unwrap_or_else(|| panic!("{k} must be an array"));
            assert_eq!(arr.len(), n_targets, "{k} must have one slot per targets[] entry");
            for slot in arr {
                target_rows += slot.as_array().expect("each slot is an array").len();
            }
        }
        if any {
            players_with_rows += 1;
        }
    }
    println!(
        "committed fixture: {players_with_rows} player(s) with rows, {rows} whole-fight rows, \
         {target_rows} per-target rows, {} damageModMap ids, {n_targets} targets",
        map.len()
    );
    assert!(players_with_rows >= 30, "only {players_with_rows} players carry modifier rows");
    assert!(target_rows >= rows, "per-target rows ({target_rows}) should outnumber whole-fight ({rows})");

    // -- determinism: the same inputs must produce byte-identical JSON --
    let again = axilog_ei::to_ei_json(
        &report,
        &EiInputs { activity: &activity, replay: None, modifiers: Some(&mods), boon_states: None },
    );
    assert_eq!(
        serde_json::to_string(&ei).unwrap(),
        serde_json::to_string(&again).unwrap(),
        "ei-json damage-modifier emission must be deterministic"
    );
}
