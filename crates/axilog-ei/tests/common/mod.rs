//! Shared fixture builder for `crates/axilog-ei/tests/`.
//!
//! Decodes the committed fixture (`fixtures/wvw-small.anon.zevtc`),
//! resolves it, analyzes it, and returns the legacy `Report` and the
//! reprojected `ReportV1` built from the same inputs -- the pattern several
//! golden tests in this crate already build inline (see
//! `crates/axilog-ei/tests/ei_golden.rs`), and the same shape as
//! `crates/axilog-schema/tests/common/mod.rs::fixture_report_all_gates`, but
//! trimmed to the pair this crate's tests need rather than the full
//! `(Encounter, Metrics, Report, ReportV1)` tuple.

use axilog_schema::v1::ReportV1;
use axilog_schema::Report;

/// The committed fixture, every optional gate ON, as `(legacy, v1)`.
#[allow(dead_code)]
pub fn fixture_legacy_and_v1() -> (Report, ReportV1) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);

    let replay_data =
        axilog_core::analysis::replay::build_replay(&raw, &enc, axilog_core::analysis::replay::DEFAULT_POLL_MS);
    let missiles_data = axilog_core::analysis::missiles::build_missiles(&raw, &enc);
    let damage_mods = axilog_core::analysis::damage_mods::evaluate_catalog_full(
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enc,
        false,
    );

    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        Some(&replay_data),
        Some(&missiles_data),
        true,
        true,
        true,
        Some(&damage_mods),
    );
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, Some(&damage_mods));
    (legacy, v1)
}
