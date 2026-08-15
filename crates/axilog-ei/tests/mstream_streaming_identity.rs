//! MSTREAM's byte-identity gate for the streaming ei-json writer.
//!
//! `axilog_ei` has two entry points onto one document definition
//! (`ei_doc`):
//!
//! * [`axilog_ei::write_ei_json`] — streams straight to an `io::Write`,
//!   emitting the root keys in a HAND-WRITTEN order and building each
//!   `players[]`/`targets[]` row on demand. This is what the CLI uses, and
//!   it is why peak RSS no longer scales with document size.
//! * [`axilog_ei::to_ei_json`] — materializes a `serde_json::Value` for the
//!   SDKs, whose root map is a `BTreeMap` and therefore RE-SORTS the keys
//!   regardless of what order they were emitted in.
//!
//! That asymmetry is exactly what makes this file a real gate rather than a
//! tautology: if the hand-written order in `EiDoc::serialize` ever drifts
//! from byte-wise ascending, the streamed text and the tree's pretty-print
//! diverge and these tests fail. Same for any difference in number
//! formatting, in the gating decisions, or in row order.
//!
//! The committed anonymized fixture is used, so this always runs in CI, and
//! it is exercised across the flag combinations the CLI actually offers
//! (flagless, each single flag, all flags) — the same enumeration MSTREAM's
//! CLI-level `cmp` sweep against the pre-MSTREAM base used.

use axilog_core::analysis::replay::build_activity_intervals;
use axilog_core::evtc::decode_raw;
use axilog_ei::EiInputs;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");

/// The three `ei-json`-relevant CLI opt-in flags, as
/// `(replay, skill_damage, timeseries, modifiers, rotation)` is overkill —
/// `rotation` does not reach `EiInputs` — so the matrix is over the four
/// that do.
#[derive(Clone, Copy)]
struct Flags {
    replay: bool,
    skill_damage: bool,
    timeseries: bool,
    modifiers: bool,
}

const FLAG_MATRIX: &[(&str, Flags)] = &[
    ("flagless", Flags { replay: false, skill_damage: false, timeseries: false, modifiers: false }),
    ("replay", Flags { replay: true, skill_damage: false, timeseries: false, modifiers: false }),
    ("skill-damage", Flags { replay: false, skill_damage: true, timeseries: false, modifiers: false }),
    ("timeseries", Flags { replay: false, skill_damage: false, timeseries: true, modifiers: false }),
    ("modifiers", Flags { replay: false, skill_damage: false, timeseries: false, modifiers: true }),
    ("all", Flags { replay: true, skill_damage: true, timeseries: true, modifiers: true }),
];

/// Renders the committed fixture under `flags` BOTH ways and returns
/// `(streamed_text, tree_text)`.
///
/// The side-input construction below is a deliberate mirror of
/// `axilog-cli`'s own `Cmd::Parse` arm — same gates, same order — so that a
/// pass here is evidence about the binary's real output and not about some
/// test-only input shape.
fn render_both(flags: Flags) -> (String, String) {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = build_activity_intervals(&raw, &enc);

    let ei_replay_data = flags
        .replay
        .then(|| axilog_core::analysis::ei_replay::build_ei_replay_auto(&raw, &enc));
    let report = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        None,
        None,
        flags.skill_damage,
        flags.timeseries,
        false,
        None,
    );
    let minion_rollups =
        flags.skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        flags.timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    let enemy_sets = (flags.timeseries || flags.skill_damage).then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (enemies, rep)
    });
    let enemy_dist = enemy_sets.as_ref().filter(|_| flags.skill_damage).map(|(en, rep)| {
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep)
    });
    let enemy_series = enemy_sets.as_ref().filter(|_| flags.timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    let dist_outcomes =
        flags.skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    let healing_detail = (flags.skill_damage || flags.timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    let boon_states = flags
        .timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let target_conditions =
        flags.timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-test", None,
        &axilog_schema::v1::Passes {
            activity: Some(&activity),
            boon_states: boon_states.as_ref(),
            target_conditions: target_conditions.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            enemy_series: enemy_series.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            healing_detail: healing_detail.as_ref().filter(|_| flags.skill_damage),
            healing_series: healing_detail.as_ref().filter(|_| flags.timeseries),
            ..Default::default()
        },
    );
    let damage_mods = flags.modifiers.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            &enc,
            true,
        )
    });

    // Rebuilt per render: `EiInputs` is `Copy`, but each `write_ei_json`/
    // `to_ei_json` call builds its own single-use document.
    let inputs = || EiInputs {
        replay: ei_replay_data.as_ref(),
        modifiers: damage_mods.as_ref(),
    };

    let mut streamed: Vec<u8> = Vec::new();
    axilog_ei::write_ei_json(&report_v1, &report, &inputs(), &mut streamed)
        .expect("stream ei-json");
    let streamed = String::from_utf8(streamed).expect("ei-json is UTF-8");
    let tree = serde_json::to_string_pretty(&axilog_ei::to_ei_json(&report_v1, &report, &inputs()))
        .expect("pretty-print ei-json tree");
    (streamed, tree)
}

/// The headline gate: the streamed bytes and the tree's pretty-print are the
/// same bytes, for every flag combination.
#[test]
fn streaming_matches_value_tree_byte_for_byte() {
    for (label, flags) in FLAG_MATRIX {
        let (streamed, tree) = render_both(*flags);
        assert_eq!(
            streamed.len(),
            tree.len(),
            "[{label}] streamed ei-json is {} bytes, tree pretty-print is {} bytes",
            streamed.len(),
            tree.len()
        );
        if streamed != tree {
            // Report the first divergence rather than dumping two ~10 MB
            // strings into the test log.
            let at = streamed
                .bytes()
                .zip(tree.bytes())
                .position(|(a, b)| a != b)
                .expect("equal-length differing strings differ somewhere");
            let lo = at.saturating_sub(120);
            panic!(
                "[{label}] streamed ei-json diverges from the tree at byte {at}\n\
                 stream: ...{}\n  tree: ...{}",
                &streamed[lo..(at + 120).min(streamed.len())],
                &tree[lo..(at + 120).min(tree.len())],
            );
        }
    }
}

/// The root keys are byte-wise ascending — the alphabetized-key convention
/// consumers rely on. Checked on the streamed TEXT (the tree cannot fail
/// this, since `BTreeMap` sorts for it), which is what makes it a real
/// assertion about the hand-written emit order in `EiDoc::serialize`.
#[test]
fn streamed_root_keys_are_alphabetized() {
    for (label, flags) in FLAG_MATRIX {
        let (streamed, _) = render_both(*flags);
        // Root keys are the only ones at exactly two spaces of indent.
        let keys: Vec<&str> = streamed
            .lines()
            .filter_map(|l| l.strip_prefix("  \""))
            .filter_map(|l| l.split('"').next())
            .collect();
        assert!(keys.len() >= 10, "[{label}] expected the full root key set, got {keys:?}");
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "[{label}] root keys are not byte-wise ascending: {keys:?}");
    }
}
