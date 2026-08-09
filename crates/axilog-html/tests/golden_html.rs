//! Golden structural test (M7, Task 3): renders the committed, PII-safe
//! fixture (`fixtures/wvw-small.anon.zevtc`) through the real
//! decode -> resolve -> analyze -> build_report -> render pipeline (the
//! same one the CLI drives), so this checks the actual shipping output —
//! not a hand-built synthetic `Report` like `src/lib.rs`'s unit tests.
//!
//! Calibrated values below (squad damage `2138414`, support sums
//! `801`/`97`/`437`/`6`) mirror the parity table in the repo README —
//! they're the same numbers `crates/axilog-core/tests/golden.rs`/
//! `support_golden.rs` already pin against the real dps.report EI export
//! for this fixture; re-asserting them here catches an HTML-report-layer
//! regression (e.g. a field silently dropped from the embedded JSON)
//! independently of those core-level tests.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_schema::{build_report, Report};

const FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const CSS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/report.css");
const JS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/report.js");

/// Full pipeline, fixture -> native `Report` — mirrors what `axilog-cli`'s
/// `parse --format html` does.
fn fixture_report() -> Report {
    let bytes = std::fs::read(FIXTURE_PATH).expect("committed fixture present");
    let raw = decode_raw(&bytes).expect("fixture decodes");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    build_report(&enc, &metrics, "0.1.0-test")
}

/// Extract the raw text between the `axilog-data` script tags and parse it
/// as JSON — the way `report.js`'s `readEmbeddedReport` (via
/// `textContent`) sees it. Mirrors `src/lib.rs`'s private test helper of
/// the same name/shape; duplicated here because this is a separate test
/// binary with no access to that module-private copy.
fn extract_embedded_json(html: &str) -> serde_json::Value {
    let start_tag = r#"<script id="axilog-data" type="application/json">"#;
    let start = html.find(start_tag).expect("data script tag present") + start_tag.len();
    let end = html[start..].find("</script>").expect("closing script tag present");
    serde_json::from_str(&html[start..start + end]).expect("embedded data block is valid JSON")
}

#[test]
fn embedded_json_contains_calibrated_squad_damage_sum() {
    let report = fixture_report();
    let html = axilog_html::render(&report);
    let data = extract_embedded_json(&html);

    // players[].damage.total summed, recomputed from the ACTUAL embedded
    // JSON (not the in-memory `Report`) — the golden squad-damage total
    // for this fixture (see the README parity table's "Squad total
    // damage" row / crates/axilog-core/tests/golden.rs). The sum itself
    // never appears as a literal substring anywhere in the document (only
    // per-player totals do), so this is the honest way to assert it.
    let players = data["players"].as_array().expect("players array");
    let damage_sum: u64 = players.iter().map(|p| p["damage"]["total"].as_u64().unwrap()).sum();
    assert_eq!(
        damage_sum, 2_138_414,
        "squad damage sum (from the embedded JSON) drifted from the calibrated golden value"
    );
}

#[test]
fn embedded_json_contains_calibrated_support_sums() {
    let report = fixture_report();
    let html = axilog_html::render(&report);
    let data = extract_embedded_json(&html);

    // players[].support.{cleanses,cleanses_self,strips,resurrects} summed
    // from the embedded JSON — matches EI's
    // condiCleanse(Self)/boonStrips/resurrects sums for this fixture
    // (README parity table's "Support: ..." rows).
    let players = data["players"].as_array().expect("players array");
    let sum_field = |field: &str| -> u64 {
        players.iter().map(|p| p["support"][field].as_u64().unwrap()).sum()
    };

    assert_eq!(sum_field("cleanses"), 801, "cleanses sum drifted from the calibrated golden value");
    assert_eq!(
        sum_field("cleanses_self"),
        97,
        "cleanses_self sum drifted from the calibrated golden value"
    );
    assert_eq!(sum_field("strips"), 437, "strips sum drifted from the calibrated golden value");
    assert_eq!(sum_field("resurrects"), 6, "resurrects sum drifted from the calibrated golden value");
}

#[test]
fn contains_all_view_and_timeline_containers() {
    let report = fixture_report();
    let html = axilog_html::render(&report);

    for id in [
        // header
        "axilog-header",
        "axilog-main",
        // tab bar + the three sortable views (Task 2)
        "axilog-tabs",
        "axilog-tab-damage",
        "axilog-tab-support",
        "axilog-tab-boons",
        "axilog-view-damage",
        "axilog-view-support",
        "axilog-view-boons",
        // SVG damage timeline (Task 3)
        "axilog-timeline-section",
        "axilog-timeline-heading",
        "axilog-timeline-chart",
    ] {
        assert!(html.contains(&format!(r#"id="{id}""#)), "missing container id {id}");
    }
}

#[test]
fn render_is_deterministic_for_the_real_fixture() {
    let report = fixture_report();
    let a = axilog_html::render(&report);
    let b = axilog_html::render(&report);
    assert_eq!(a, b, "render() must be byte-for-byte deterministic for identical input");
}

#[test]
fn total_report_size_stays_under_budget() {
    let report = fixture_report();
    let html = axilog_html::render(&report);
    assert!(
        html.len() < 250_000,
        "fixture report is {} bytes, must stay under the 250KB total-file budget",
        html.len()
    );
}

#[test]
fn combined_raw_assets_stay_under_budget() {
    // Read the asset files' lengths directly (not the embedded copies
    // inside a rendered report) — this is the same source `include_str!`
    // inlines in src/lib.rs, and it isolates the CSS+JS budget from the
    // (much larger, data-dependent) embedded-JSON payload asserted above.
    let css_len = std::fs::metadata(CSS_PATH).expect("report.css present").len();
    let js_len = std::fs::metadata(JS_PATH).expect("report.js present").len();
    let combined = css_len + js_len;
    assert!(
        combined < 50 * 1024,
        "combined raw report.css + report.js is {combined} bytes, must stay under the \
         50KB budget (controller-relaxed from 25KB in Task 2, see progress.md)"
    );
}
