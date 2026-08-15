//! Shared fixture builders for `crates/axilog-schema/tests/`.
//!
//! Each function decodes the committed fixture
//! (`fixtures/wvw-small.anon.zevtc`), resolves it, analyzes it, and returns
//! the resolved `Encounter`, the `Metrics` computed over it, the legacy
//! `Report`, and the reprojected `ReportV1` -- from the same inputs, so
//! callers can cross-check the reshape rather than compute a second,
//! divergent document.
//!
//! Lifted out of `v1_equivalence.rs`'s `build()`, which had this exact
//! setup (decode -> resolve -> analyze -> build_report -> build_report_v1)
//! duplicated across test files that needed a real document. Other test
//! files (`v1_shape.rs`, `v1_size.rs`) keep their own local copies rather
//! than being refactored here, since their builders also return the raw
//! `serde_json::Value` / stringified forms this module has no reason to
//! produce.

use axilog_core::analysis::Metrics;
use axilog_core::model::Encounter;
use axilog_schema::v1::ReportV1;
use axilog_schema::Report;

/// `all_gates` selects between every optional compute gate ON (skill-damage,
/// timeseries, rotation, replay, missiles, damage-mods) or OFF. There is no
/// partial-gate mode -- callers that need a specific mix should build their
/// own `Encounter`/`Metrics` by hand, the way `v1.rs`'s and
/// `v1_equivalence.rs`'s own inline tests already do for the cases this
/// fixture can't express.
fn build(all_gates: bool) -> (Encounter, Metrics, Report, ReportV1) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);

    let replay_data = all_gates.then(|| {
        axilog_core::analysis::replay::build_replay(&raw, &enc, axilog_core::analysis::replay::DEFAULT_POLL_MS)
    });
    let missiles_data = all_gates.then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
    let damage_mods = all_gates.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            &enc,
            false,
        )
    });

    // Side-channel absorption Task 6: the two passes behind `blocks.
    // minions` and `series[].health_percents`. Gated with everything else
    // so `build(false)` still exercises the not-computed path.
    let minion_rollups = all_gates.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        all_gates.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    // Tasks 7 and 8: the two passes behind the enemy rows' `by_skill` and
    // the enemy rows on `blocks.series`.
    let enemy_sets = all_gates.then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
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
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });

    // Task 9: the outcome columns on both player-side distributions.
    let dist_outcomes =
        all_gates.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    // Task 10: ONE pass feeding two families under two different flags.
    // `build` self-gates to `None` on a log with no healing extension, so
    // the `.flatten()` here is the "unsupported", not the "gate off", case
    // -- and both `Passes` fields get it, since `build(true)` means every
    // flag is on.
    let healing_detail =
        all_gates.then(|| axilog_core::analysis::healing_detail::build(&raw, &enc)).flatten();

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
        },
    );
    (enc, metrics, legacy, v1)
}

/// The default fixture: every compute gate ON. An alias for
/// [`fixture_report_all_gates`] kept as its own name because it is the one
/// most tests reach for, and a second reprojection wanting "the fixture,
/// fully populated" should not have to know the gates are involved at all.
#[allow(dead_code)]
pub fn fixture_report() -> (Encounter, Metrics, Report, ReportV1) {
    build(true)
}

/// Every optional gate (skill_damage, timeseries, rotation, replay,
/// missiles, damage-mods) ON -- populated blocks, not empty ones.
#[allow(dead_code)]
pub fn fixture_report_all_gates() -> (Encounter, Metrics, Report, ReportV1) {
    build(true)
}

/// Every optional gate OFF -- the minimal document a bare `axilog parse`
/// with no optional flags would produce.
#[allow(dead_code)]
pub fn fixture_report_no_gates() -> (Encounter, Metrics, Report, ReportV1) {
    build(false)
}
