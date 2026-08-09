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
use axilog_core::analysis::replay::{build_replay, DEFAULT_POLL_MS};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use axilog_schema::{build_report, Report};

const FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const CSS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/report.css");
const JS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/report.js");

/// Full pipeline, fixture -> native `Report` — mirrors what `axilog-cli`'s
/// `parse --format html` (without `--replay`) does.
fn fixture_report() -> Report {
    let bytes = std::fs::read(FIXTURE_PATH).expect("committed fixture present");
    let raw = decode_raw(&bytes).expect("fixture decodes");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    build_report(&enc, &metrics, "0.1.0-test", None, None, false, false, false)
}

/// Same pipeline, but with `--replay` (M9, Task 2): computes
/// `axilog_core::analysis::replay::build_replay` at the CLI's default
/// polling rate and embeds it, mirroring `axilog-cli`'s
/// `parse --format html --replay`.
fn fixture_report_with_replay() -> Report {
    let bytes = std::fs::read(FIXTURE_PATH).expect("committed fixture present");
    let raw = decode_raw(&bytes).expect("fixture decodes");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    let replay = build_replay(&raw, &enc, DEFAULT_POLL_MS);
    build_report(&enc, &metrics, "0.1.0-test", Some(&replay), None, false, false, false)
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
        // Replay tab (M9 Task 3) -- present but hidden unless requested,
        // see contains_replay_tab_and_panel_containers_regardless_of_replay_data.
        "axilog-tab-replay",
        "axilog-view-replay",
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

/// M14 Task 2: the budget here was raised from the original M7 250,000-byte
/// figure to 275,000 to make room for the now-always-on `skill_map` block
/// (`Report::skill_map` -- NOT gated behind an opt-in flag, unlike
/// `skill_damage`/`per_second`/`dps_targets`/`rotation`, per that field's
/// own doc comment: it's scoped to only REFERENCED skill ids, not a dump of
/// the whole log skill table, so it stays modest even though it's always
/// on). Measured directly on this same committed fixture: adding
/// `skill_map` alone (every other opt-in block still off, matching this
/// test's `fixture_report()`) grew the rendered HTML from 236,198 to
/// 260,520 bytes (**+10.3%**, 368 referenced skill ids, a 24,309-byte JSON
/// block) -- real growth, but well under the ~30% guideline every OTHER
/// block in this schema was measured against before being gated, so this
/// stays a budget adjustment, not a new opt-in flag (see `axilog_core::
/// analysis::skill_map`'s module doc for the full scoping/size writeup).
/// 275,000 keeps ~14.5KB of headroom above the current measured 260,520.
#[test]
fn total_report_size_stays_under_budget() {
    let report = fixture_report();
    let html = axilog_html::render(&report);
    assert!(
        html.len() < 275_000,
        "fixture report is {} bytes, must stay under the 275KB total-file budget",
        html.len()
    );
}

/// M9 Task 2 size gate: a replay-enabled fixture report must stay under
/// 600KB (the plan's Global Constraints budget -- deliberately looser than
/// the non-replay report's 275KB budget above, since `ReplayOut.tracks[]`
/// adds a full downsampled position track per squad/enemy-player). Also
/// logs the embedded replay block's own serialized byte size (informational
/// only, per the Task 2 brief -- "no hard gate" on that number alone).
#[test]
fn replay_enabled_report_stays_under_600kb_budget() {
    let report = fixture_report_with_replay();
    let replay = report.replay.as_ref().expect("replay block present when requested");
    let replay_json_len = serde_json::to_string(replay).expect("replay serializes").len();
    println!(
        "replay-enabled fixture: embedded replay JSON block is {replay_json_len} bytes \
         ({} tracks)",
        replay.tracks.len()
    );

    let html = axilog_html::render(&report);
    assert!(
        html.len() < 600_000,
        "replay-enabled fixture report is {} bytes, must stay under the 600KB budget \
         (replay JSON block alone is {replay_json_len} bytes)",
        html.len()
    );
}

/// Determinism must hold with replay data embedded too (M9 Task 2 brief),
/// not just for the non-replay path already covered by
/// `render_is_deterministic_for_the_real_fixture` above.
#[test]
fn render_is_deterministic_with_replay_enabled() {
    let report = fixture_report_with_replay();
    let a = axilog_html::render(&report);
    let b = axilog_html::render(&report);
    assert_eq!(a, b, "render() must stay byte-for-byte deterministic with replay data embedded");
}

/// The replay block must actually reach the embedded JSON the client-side
/// Replay tab (M9 Task 3) reads from -- i.e. `--format html --replay`'s
/// data script tag carries `replay`, not just the in-memory `Report`.
#[test]
fn embedded_json_carries_the_replay_block_when_requested() {
    let report = fixture_report_with_replay();
    let html = axilog_html::render(&report);
    let data = extract_embedded_json(&html);
    assert!(data.get("replay").is_some(), "expected a replay key in the embedded JSON");
    assert!(!data["replay"]["tracks"].as_array().expect("tracks array").is_empty());

    // The non-replay path must still omit it entirely (not null).
    let plain_report = fixture_report();
    let plain_html = axilog_html::render(&plain_report);
    let plain_data = extract_embedded_json(&plain_html);
    assert!(plain_data.get("replay").is_none(), "replay must be omitted when not requested");
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
        combined < 65_536,
        "combined raw report.css + report.js is {combined} bytes, must stay under the \
         64KB (65,536B) budget (M10 Task 3: controller pre-authorized raising this from \
         60KB this milestone -- the polish batch's replay minors (bounds-finiteness doc \
         comment, the \"no position data\" empty-state message + its pure-fn gate, the \
         enemy-dot contrast bump) left only ~170 bytes of headroom under the old 60KB \
         ceiling per the M10 plan's Global Constraints budget note; see the M10 plan \
         and progress.md)"
    );
}

/// M9 Task 3: the Replay tab's containers are always in the skeleton
/// (`hidden` by default -- `report.js`'s `hasReplay()` gate unhides the tab
/// button at runtime only when the embedded data actually carries a
/// non-empty `replay.tracks`), and the client-side Replay code itself must
/// ship in `report.js` regardless of whether a given report requests
/// `--replay` (assets are static/inlined; see the plan's Task 3 note that
/// "non-replay reports WILL contain the new JS"). Mirrors
/// `contains_all_view_and_timeline_containers` above, extended with the
/// replay-specific ids.
#[test]
fn contains_replay_tab_and_panel_containers_regardless_of_replay_data() {
    for report in [fixture_report(), fixture_report_with_replay()] {
        let html = axilog_html::render(&report);
        for id in ["axilog-tab-replay", "axilog-view-replay"] {
            assert!(html.contains(&format!(r#"id="{id}""#)), "missing replay container id {id}");
        }
        assert!(
            html.contains(r#"aria-controls="axilog-view-replay" hidden>Replay<"#),
            "replay tab button must be present but hidden by default in the static skeleton"
        );
    }
}
