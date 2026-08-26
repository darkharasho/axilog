//! No emitted id may go unnamed. MNAME's standing guard.
//!
//! A WvW player reported eleven skills rendering as the literal string
//! `"Skill <id>"` in AxiBridge's healing tables. The cause was structural
//! rather than a missing entry: `blocks.healing` referenced ids that
//! `analysis::skill_map`'s damage-and-rotation scope never covered, and
//! `CatalogBuilder::finish` had no access to the log's own skill table to
//! name them with. Any block could have had the same hole; healing is
//! simply the one someone noticed.
//!
//! So this test does not check healing. It walks EVERY id-bearing array in
//! the emitted EI JSON and asserts each id resolves -- in `skillMap`,
//! `buffMap`, or, for the damage-modifier rows that live in their own id
//! space, `damageModMap` -- to a name that is neither the `Skill <id>`
//! placeholder nor empty. The empty case matters as much as the
//! placeholder: `BuffEntry`'s name used to default to `""`, which a
//! consumer cannot even detect as a failure. A second pass then checks
//! EVERY catalog entry a referenced id has, not just the first one that
//! answers, so a shadowed entry cannot rot unseen.
//!
//! Verified to actually fail: reverting the `CatalogBuilder::finish` name
//! chain to its pre-Task-1 form (`.or_else(skill_icons::name)`, no log
//! table) reintroduces 22 leaking ids on the committed WvW fixture,
//! including the heal-only ids 31536, 53183, 69336 and 70765 that started
//! the report.
//!
//! Ids no source can name get an explicit allowlist below, each with a
//! reason. That is the point: the allowlist growing is a visible diff in
//! review, which a placeholder appearing in a Discord screenshot is not.

mod common;

use std::collections::{BTreeMap, BTreeSet};

/// Ids that legitimately have no name in any source we carry.
///
/// EVERY entry needs a reason. If you are adding one to make a build pass,
/// the question to answer first is why no catalog knows the id -- an entry
/// here is a permanent admission, not a silencer.
const UNNAMEABLE: &[(i64, &str)] = &[
    // ---- Weaver dual attunements, the POSITIVE half ----
    //
    // `skill_map::skill_name_overrides`'s "The one gap" section records
    // that GW2EI names its Weaver dual-attunement ids from EI's BUFF table
    // (`WeaverHelper.cs:307-322`, `new Buff("Dual Fire Attunement",
    // DualFireAttunement, ...)`) rather than from `OverridenSkillNames`,
    // and that this project does not read that subsystem. The doc frames
    // the gap around the NEGATIVE pseudo ids `-5..-16`; these three are the
    // same gap at real, positive game ids, reached through `rotation`
    // rather than through a synthetic cast. arcdps writes the numeric
    // string as the name, /v2/skills 404s on all three (checked live,
    // 2026-08-26), and GW2EI's `SkillIDs.cs` is the only place the ids
    // appear at all. Closing this properly means importing EI's BUFF
    // table, which is a separate piece of work from MNAME.
    (41166, "SkillIDs.DualWaterAttunement -- Weaver, named only by EI's BUFF table"),
    (42264, "SkillIDs.DualAirAttunement -- Weaver, named only by EI's BUFF table"),
    (44857, "SkillIDs.DualEarthAttunement -- Weaver, named only by EI's BUFF table"),
    // ---- Unlisted internal ids: no source we carry OR could carry ----
    //
    // For each of these four, all three naming sources were checked and
    // all three genuinely have nothing:
    //
    //   1. the log's own skill table: arcdps wrote the id back as its own
    //      decimal string ("30060", "31311", ...), the numeric placeholder
    //      `resolve_name`'s first rung rejects by design;
    //   2. `skill_icons::SKILL_NAMES` (generated from /v2/skills): absent
    //      -- confirmed against the LIVE API on 2026-08-26,
    //      `?ids=30060,31311,54960,69665` -> "all ids provided are
    //      invalid", so this is not catalog staleness;
    //   3. GW2EI: the ids do not occur anywhere in the `/var/tmp/gw2ei`
    //      checkout -- not in `SkillIDs.cs`, not in `OverridenSkillNames`,
    //      not in any `new Buff(...)`. Real EI leaves the numeric log name
    //      in place for exactly this case (`SkillItem.cs:78-90`).
    //
    // They are unlisted/internal effect ids the game never published. An
    // entry here is not a claim they are unimportant, only that no rung
    // this project has can name them; a future source that does name them
    // should delete the entry rather than add one.
    (30060, "unlisted id, 4 events, outgoing on a Ranger -- numeric-only in arcdps, absent from /v2/skills and GW2EI"),
    (31311, "unlisted id, 12 events, incoming on a Guardian -- same three misses"),
    (54960, "unlisted id, 22 events, outgoing on a Mesmer -- same three misses"),
    (69665, "unlisted id, 12 events, outgoing on a Necromancer -- same three misses"),
];

fn placeholder(name: &str, id: i64) -> bool {
    name.trim().is_empty() || name == format!("Skill {id}")
}

/// Every id-bearing array in the emitted document, by the path a reader
/// would use to find it. Extend this list when a new array ships -- an
/// array not listed here is not guarded.
///
/// Ids stay `i64`, SIGNED. GW2EI's synthetic skills live at negative ids
/// (`SkillIDs.WeaponSwap = -2`, plus the ~36 `instant_cast` finder ids) and
/// the adapter writes them that way -- `"id": -2` in `rotation[]`, `"s-2"`
/// in `skillMap` (see `ei_skill_id`). `damageModMap`'s ids are signed for a
/// different reason: the sign is what marks a modifier incoming. Narrowing
/// to `u32` here, as this walker's first draft did, turns `-28` into
/// `4294967268` and then fails to find `"s-28"` -- a leak report for an id
/// that is in fact named.
fn collect_ids(v: &serde_json::Value) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let mut push = |path: &str, val: &serde_json::Value| {
        if let Some(id) = val.get("id").and_then(|i| i.as_i64()) {
            out.push((path.to_string(), id));
        }
    };

    let players = v["players"].as_array().cloned().unwrap_or_default();
    for (i, p) in players.iter().enumerate() {
        // Phase-then-row nesting: `[phase][row]`. The two damage families
        // hang off the player object directly; the healing and barrier
        // families hang off the extension sub-objects the adapter only
        // emits for a player the heal addon actually covered
        // (`lib.rs`'s `extHealingStats`/`extBarrierStats` insert). The
        // brief's draft of this walker looked for the latter two at the
        // player root, where they do not exist -- the per-array floor
        // below is what caught that.
        for (parent, key) in [
            (None, "totalDamageDist"),
            (None, "totalDamageTaken"),
            (Some("extHealingStats"), "totalHealingDist"),
            (Some("extBarrierStats"), "totalBarrierDist"),
        ] {
            let owner = match parent {
                Some(p_key) => &p[p_key],
                None => p,
            };
            if let Some(phases) = owner[key].as_array() {
                for rows in phases {
                    for row in rows.as_array().into_iter().flatten() {
                        push(&format!("players[{i}].{key}"), row);
                    }
                }
            }
        }
        // `targetDamageDist` adds a target level: `[target][phase][row]`.
        for targets in p["targetDamageDist"].as_array().into_iter().flatten() {
            for phases in targets.as_array().into_iter().flatten() {
                for row in phases.as_array().into_iter().flatten() {
                    push(&format!("players[{i}].targetDamageDist"), row);
                }
            }
        }
        for phases in p["rotation"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].rotation"), phases);
        }
        for row in p["buffUptimes"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].buffUptimes"), row);
        }
        for row in p["damageModifiers"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].damageModifiers"), row);
        }
    }
    for (i, t) in v["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        for row in t["buffs"].as_array().into_iter().flatten() {
            push(&format!("targets[{i}].buffs"), row);
        }
    }
    out
}

/// The nine arrays `collect_ids` walks, as the substrings the emitted paths
/// carry. Reported per-array so a reader can see what the guard actually
/// covered on this fixture, not just that it covered something.
const WALKED_ARRAYS: &[&str] = &[
    "totalDamageDist",
    "totalHealingDist",
    "totalBarrierDist",
    "totalDamageTaken",
    "targetDamageDist",
    "rotation",
    "buffUptimes",
    "damageModifiers",
    "buffs",
];

/// Row and distinct-id counts per array, for the report and for the floor
/// below to assert on.
///
/// Matching is a plain `contains` on the emitted path, the same rule the
/// floor uses, so the two cannot disagree. No key here is a substring of
/// another emitted path: in particular `"buffs"` does not occur in
/// `"players[i].buffUptimes"` (capital `U`), and `"targetDamageDist"` does
/// not occur in `"players[i].totalDamageDist"`.
fn per_array_counts(found: &[(String, i64)]) -> BTreeMap<&'static str, (usize, usize)> {
    WALKED_ARRAYS
        .iter()
        .map(|&want| {
            let rows: Vec<i64> = found
                .iter()
                .filter(|(path, _)| path.contains(want))
                .map(|&(_, id)| id)
                .collect();
            let distinct: BTreeSet<i64> = rows.iter().copied().collect();
            (want, (rows.len(), distinct.len()))
        })
        .collect()
}

/// The floor that stops this test passing vacuously.
///
/// `collect_ids` hardcodes each array's nesting depth, and EI's shapes are
/// not uniform -- `totalDamageDist` is `[phase][row]`, `targetDamageDist`
/// is `[target][phase][row]`, `rotation` is its own thing again. Get one
/// wrong and the walker silently yields nothing for that array, so the
/// invariant holds over an empty set and the guard guards nothing. This
/// floor is per-array, not a total, because a single fat array could
/// otherwise mask three empty ones.
///
/// `arrays` is per-configuration because the gates decide which arrays are
/// emitted at all: a flagless parse writes no dist, rotation or
/// damage-modifier family, so demanding ids from them there would fail for
/// a reason that has nothing to do with naming. Each caller states its own
/// floor explicitly, so lowering one is a visible diff.
fn assert_walker_reaches_every_array(found: &[(String, i64)], label: &str, arrays: &[&str]) {
    for (want, (rows, distinct)) in per_array_counts(found) {
        println!("{label}: {want} -> {rows} id-bearing row(s), {distinct} distinct id(s)");
    }
    for &want in arrays {
        assert!(
            found.iter().any(|(path, _)| path.contains(want)),
            "{label}: the walker found ZERO ids in {want}. Either the fixture \
             genuinely has none, or `collect_ids` has the wrong nesting depth \
             for it -- check against the emitted JSON before relaxing this."
        );
    }
}

fn check_no_leaks(v: &serde_json::Value, label: &str, must_reach: &[&str]) {
    let allowed: BTreeSet<i64> = UNNAMEABLE.iter().map(|&(id, _)| id).collect();
    let skills = v["skillMap"].as_object().cloned().unwrap_or_default();
    let buffs = v["buffMap"].as_object().cloned().unwrap_or_default();
    let mods = v["damageModMap"].as_object().cloned().unwrap_or_default();

    let found = collect_ids(v);
    assert_walker_reaches_every_array(&found, label, must_reach);

    let mut leaks: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for (path, id) in found {
        if allowed.contains(&id) || !seen.insert(id) {
            continue;
        }
        // `damageModifiers[].id` is a DAMAGE-MODIFIER id, in its own
        // namespace: EI names it from `damageModMap` under `"d<id>"`, never
        // from `skillMap`/`buffMap`. Checking it against the skill maps (as
        // this walker's first draft did) reports all 36 of the fixture's
        // modifiers as leaks purely because they were looked up in the
        // wrong map. The array is still guarded -- against the map that
        // actually names it.
        let name = if path.contains("damageModifiers") {
            mods.get(&format!("d{id}")).and_then(|e| e["name"].as_str())
        } else {
            None
        };
        let skill = skills
            .get(&format!("s{id}"))
            .and_then(|e| e["name"].as_str());
        let buff = buffs
            .get(&format!("b{id}"))
            .and_then(|e| e["name"].as_str());
        let resolved = match (name, skill, buff) {
            (Some(n), _, _) if !placeholder(n, id) => continue,
            (_, Some(n), _) if !placeholder(n, id) => continue,
            (_, _, Some(n)) if !placeholder(n, id) => continue,
            (Some(n), _, _) | (_, Some(n), _) | (_, _, Some(n)) => format!("{n:?}"),
            (None, None, None) => "absent from EVERY catalog".to_string(),
        };
        leaks.push(format!("  {path}: id {id} -> {resolved}"));
    }

    assert!(
        leaks.is_empty(),
        "{label}: {} id(s) emitted with no usable name:\n{}\n\
         Either a name source is missing the id, or the id belongs in \
         UNNAMEABLE with a reason. Do not add it without one.",
        leaks.len(),
        leaks.join("\n")
    );

    // EVERY catalog entry, not just the first one that answers.
    //
    // The loop above accepts an id as soon as ANY map names it, which is
    // the right rule for a consumer -- but it means a second entry for the
    // same id can rot unnoticed. That is not hypothetical: the indirect
    // heal ids are deliberately registered in BOTH catalogs (`blocks
    // .healing`'s buff-and-skill superset), so `buffMap`'s copy of them is
    // shadowed by `skillMap`'s on this fixture, and `BuffEntry::name`
    // regressing to its old `unwrap_or_default()` empty string would slip
    // past the check above with every test still green. This pass looks at
    // both entries for every referenced id.
    let mut empty: Vec<String> = Vec::new();
    let mut checked = BTreeSet::new();
    for (_, id) in collect_ids(v) {
        if allowed.contains(&id) || !checked.insert(id) {
            continue;
        }
        for (map, prefix, which) in [(&skills, 's', "skillMap"), (&buffs, 'b', "buffMap")] {
            if let Some(n) = map
                .get(&format!("{prefix}{id}"))
                .and_then(|e| e["name"].as_str())
            {
                if n.trim().is_empty() {
                    empty.push(format!("  {which}[{prefix}{id}].name is empty"));
                }
            }
        }
    }
    assert!(
        empty.is_empty(),
        "{label}: {} catalog entr(ies) carry an EMPTY name:\n{}",
        empty.len(),
        empty.join("\n")
    );
}

/// Parses the committed WvW fixture and emits the EI-compat document.
///
/// The construction mirrors `crates/axilog-schema/tests/common/mod.rs`'s
/// `build(all_gates)` rather than `meigap3_ei_golden.rs`'s narrower one:
/// meigap3 supplies only four `Passes` fields (it is calibrating healing),
/// and the arrays this test exists to guard -- `targetDamageDist`,
/// `damageModifiers`, `buffUptimes` -- come from passes meigap3 leaves
/// `None`. Emitting the smaller document would silently test fewer arrays
/// than production emits.
fn ei_json_for_fixture(all_gates: bool) -> Option<serde_json::Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    );
    let bytes = common::read_bytes_or_skip(path, "MNAME name-leak guard")?;

    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode committed fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let registry = axilog_core::analysis::damage::InstidRegistry::build(&raw);

    let replay_data = all_gates.then(|| {
        axilog_core::analysis::replay::build_replay(
            &raw,
            &enc,
            axilog_core::analysis::replay::DEFAULT_POLL_MS,
        )
    });
    let missiles_data =
        all_gates.then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
    let damage_mods = all_gates.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(&raw, &registry, &enc, false)
    });
    let minion_rollups = all_gates.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        all_gates.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    let enemy_sets = all_gates.then(|| {
        let enemies: BTreeSet<u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().copied())
            .collect();
        let rep: BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (enemies, rep)
    });
    let enemy_dist = enemy_sets
        .as_ref()
        .map(|(en, rep)| axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep));
    let enemy_series = enemy_sets.as_ref().map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(&enc, &raw, &registry, en, rep)
    });
    let dist_outcomes = all_gates.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    let healing_detail = all_gates
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    // Ungated in every caller -- supplied in BOTH modes, as in the schema
    // crate's shared builder.
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);

    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        replay_data.as_ref(),
        missiles_data.as_ref(),
        all_gates,
        all_gates,
        all_gates,
        damage_mods.as_ref(),
    );
    let boon_states =
        all_gates.then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let target_conditions =
        all_gates.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    let self_effects = all_gates.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));

    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes {
            damage_mods: damage_mods.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            enemy_series: enemy_series.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: boon_states.as_ref(),
            target_conditions: target_conditions.as_ref(),
            self_effects: self_effects.as_ref(),
            squad_buffs: Some(&squad_buffs),
        },
    );

    Some(axilog_ei::to_ei_json(&v1, None))
}

/// Arrays appear and disappear with the gates, so the invariant is checked
/// under both configurations the adapter supports. A flagless parse emits
/// the smaller document; the all-gates parse is where `targetDamageDist`,
/// `totalHealingDist` and `damageModifiers` actually exist.
#[test]
fn no_emitted_id_goes_unnamed_with_all_gates_on() {
    let Some(v) = ei_json_for_fixture(/* all gates */ true) else {
        return;
    };
    check_no_leaks(
        &v,
        "all gates on",
        // Every array `collect_ids` walks EXCEPT `totalBarrierDist`: this
        // fixture's heal-addon players generate healing but no barrier, so
        // `extBarrierStats.totalBarrierDist` is legitimately an empty row
        // set here. The walker still covers it -- a barrier-carrying log
        // would be checked -- but this fixture cannot floor it.
        &[
            "totalDamageDist",
            "totalHealingDist",
            "totalDamageTaken",
            "targetDamageDist",
            "rotation",
            "buffUptimes",
            "damageModifiers",
            "buffs",
        ],
    );
}

#[test]
fn no_emitted_id_goes_unnamed_with_default_gates() {
    let Some(v) = ei_json_for_fixture(/* all gates */ false) else {
        return;
    };
    check_no_leaks(
        &v,
        "default gates",
        // A flagless parse emits no dist, rotation, damage-modifier or
        // target-buff family at all, so `buffUptimes` is the only array
        // that can carry ids in this configuration. That is a property of
        // the gates, not of naming; the all-gates test is where the other
        // eight arrays are floored.
        &["buffUptimes"],
    );
}
