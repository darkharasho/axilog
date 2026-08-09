//! `axilog-html`: renders a single, self-contained dark-theme HTML report
//! from an `axilog_schema::Report` — no external requests (CSS/JS/data are
//! all inlined into one document) and deterministic byte-for-byte output
//! for a given report (see `tests::deterministic_output`).
//!
//! The CLI (`axilog-cli`) stays thin: it just calls [`render`] and writes
//! the result to stdout or `-o <file>`. All number/date formatting and
//! interactive behavior (sorting, theme toggle, etc.) lives in
//! `assets/report.js`, which runs client-side against the embedded JSON
//! data block — Rust never duplicates that formatting logic.
//!
//! XSS note: see the contract documented at the top of
//! `assets/report.js` (render log-derived strings via `textContent`
//! only). This module's half of that contract is [`escape_for_script`]
//! below, which makes the embedded JSON safe to place literally inside a
//! `<script>` element.

use axilog_schema::Report;

const SKELETON: &str = include_str!("../assets/skeleton.html");
const CSS: &str = include_str!("../assets/report.css");
const JS: &str = include_str!("../assets/report.js");

/// Render `report` as a complete, self-contained HTML document.
pub fn render(report: &Report) -> String {
    let json = serde_json::to_string(report).expect("Report always serializes to JSON");
    let safe_json = escape_for_script(&json);
    SKELETON
        .replacen("__AXILOG_CSS__", CSS, 1)
        .replacen("__AXILOG_JS__", JS, 1)
        .replacen("__AXILOG_DATA__", &safe_json, 1)
}

/// Make a JSON string safe to embed *literally* (unquoted) inside an HTML
/// `<script type="application/json">…</script>` element.
///
/// JSON syntax never produces a `<` character outside of string content,
/// so replacing every `<` with the equivalent `\u003c` unicode escape
/// cannot change the value a conforming JSON parser recovers (`JSON.parse`
/// decodes `\u003c` back to `<`), while guaranteeing the literal byte
/// sequence `</script` can never appear in the emitted HTML — which is
/// what would otherwise let a log-derived string (e.g. a crafted player
/// name) terminate the `<script>` element early and inject markup into
/// the surrounding document. `serde_json` does not perform this escaping
/// by default; this is the same technique used by libraries like
/// `serialize-javascript` / Rails' `escape_javascript` for embedding
/// untrusted JSON in `<script>` tags.
fn escape_for_script(json: &str) -> String {
    json.replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_schema::{
        CcOut, DamageOut, EncounterOut, PlayerOut, Report, SupportOut, TeamOut, TimelineOut,
        PerSecondOut,
    };

    fn fixture_report() -> Report {
        Report {
            schema_version: "0.1",
            axilog_version: "0.1.0-test".into(),
            encounter: EncounterOut {
                kind: "wvw".into(),
                map: "Eternal Battlegrounds".into(),
                duration_ms: 125_000,
                build: "20260114".into(),
                revision: 1,
                recorded_by: Some(":Recorder.1234".into()),
                teams: vec![
                    TeamOut { color: "red".into(), team_id: 1, guid: None },
                    TeamOut { color: "blue".into(), team_id: 2, guid: None },
                ],
                markers: vec![],
                tick_rate: None,
            },
            players: vec![PlayerOut {
                account: ":Player.1234".into(),
                character: "PlayerOne".into(),
                profession: "Guardian".into(),
                elite_spec: "Firebrand".into(),
                team: "red".into(),
                subgroup: 1,
                in_squad: true,
                commander: false,
                marker: None,
                commander_tag: None,
                damage: DamageOut { total: 1000, dps: 8.0, per_enemy: vec![] },
                downs_dealt: 0,
                kills_dealt: 0,
                down_contribution: 0,
                downs_taken: 0,
                deaths: 0,
                damage_taken: 0,
                cc: CcOut {
                    applied_total: 0,
                    applied_duration_ms: 0,
                    stun_breaks: 0,
                    removed_stun_duration_ms: 0,
                },
                boons: vec![],
                support: SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, resurrects: 0 },
            }],
            enemies: vec![],
            timeline: TimelineOut {
                resolution_ms: 1000,
                per_second: PerSecondOut { squad_damage: vec![], cc_applied: vec![], downs: vec![] },
            },
            warnings: vec![],
        }
    }

    /// Extract the raw text between the `axilog-data` script tags, the
    /// way a browser's `textContent` would see it.
    fn extract_data_block(html: &str) -> &str {
        let start_tag_end = html
            .find(r#"<script id="axilog-data" type="application/json">"#)
            .expect("data script tag present")
            + r#"<script id="axilog-data" type="application/json">"#.len();
        let end = html[start_tag_end..]
            .find("</script>")
            .expect("closing script tag present");
        &html[start_tag_end..start_tag_end + end]
    }

    #[test]
    fn contains_data_block_that_parses_as_json() {
        let html = render(&fixture_report());
        let data = extract_data_block(&html);
        let v: serde_json::Value = serde_json::from_str(data).expect("valid JSON");
        assert!(v["players"].as_array().unwrap().len() > 0);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
    }

    #[test]
    fn contains_header_container_ids() {
        let html = render(&fixture_report());
        for id in [
            "axilog-header",
            "axilog-map",
            "axilog-duration",
            "axilog-teams",
            "axilog-recorded-by",
            "axilog-commander",
            "axilog-warnings",
            "axilog-tick-rate",
            "axilog-theme-toggle",
            "axilog-data",
            "axilog-main",
        ] {
            assert!(
                html.contains(&format!(r#"id="{id}""#)),
                "missing container id {id}"
            );
        }
    }

    #[test]
    fn deterministic_output() {
        let report = fixture_report();
        let a = render(&report);
        let b = render(&report);
        assert_eq!(a, b, "render() must be a pure/deterministic function of the report");
    }

    /// A crafted player name containing a literal `</script>` must not be
    /// able to break out of the embedded JSON `<script>` element.
    #[test]
    fn escapes_script_breakout_in_player_name() {
        let mut report = fixture_report();
        report.players[0].character = "</script><script>alert(1)</script>".into();
        report.players[0].account = ":Evil</script>.9999".into();
        let html = render(&report);

        // The literal breakout sequence must never appear anywhere in the
        // emitted document.
        assert!(
            !html.contains("</script><script>alert(1)"),
            "raw </script> breakout sequence leaked into output"
        );

        // The data block must still parse and round-trip the original
        // (unescaped) strings.
        let data = extract_data_block(&html);
        let v: serde_json::Value = serde_json::from_str(data).expect("still valid JSON");
        assert_eq!(v["players"][0]["character"], "</script><script>alert(1)</script>");
        assert_eq!(v["players"][0]["account"], ":Evil</script>.9999");
    }

    #[test]
    fn no_external_urls() {
        let html = render(&fixture_report());
        assert!(!html.contains("http://"), "output must not reference external URLs");
        assert!(!html.contains("https://"), "output must not reference external URLs");
    }

    /// Regression test for a render-breaking bug: `report.js`/`report.css`
    /// are inlined *verbatim* (byte-for-byte) into a `<script>`/`<style>`
    /// element by [`render`]. The HTML tokenizer's "script data state"
    /// terminates the element at the very first literal `</script`
    /// sequence (case-insensitive) it finds in the raw bytes, regardless
    /// of whether that sequence sits inside a JS string, a comment, or is
    /// otherwise inert to the JS parser — a doc-comment in `report.js`
    /// once contained exactly that sequence (as an example of what
    /// *log-derived* strings are safe against), which silently truncated
    /// the inlined `<script>` in every real browser, spilling the rest of
    /// the file into the document body as text and leaving the header
    /// permanently unrendered — while every purely-structural Rust test
    /// above kept passing, because the truncation only matters to an
    /// HTML tokenizer, not to `str::contains`/`serde_json`. Pin it here.
    #[test]
    fn inlined_assets_contain_no_literal_script_close_sequence() {
        for (name, asset) in [("report.js", JS), ("report.css", CSS)] {
            let lower = asset.to_ascii_lowercase();
            assert!(
                !lower.contains("</script"),
                "{name} contains a literal `</script` sequence (case-insensitive) -- \
                 inlining it verbatim into the report's <script> element would \
                 truncate that element in every real browser"
            );
        }
    }

    /// Companion to the regression test above, checked from the other
    /// direction: the fully-rendered document should contain exactly the
    /// two `<script>`/`</script>` element pairs the skeleton defines (the
    /// `axilog-data` JSON block and the inlined `report.js` code block) --
    /// no more, no fewer, and no stray/unbalanced tag from either the
    /// inlined assets or (via [`escape_for_script`]) the embedded report
    /// data.
    #[test]
    fn rendered_html_has_exactly_two_balanced_script_tags() {
        let html = render(&fixture_report());
        let lower = html.to_ascii_lowercase();
        let opens = lower.matches("<script").count();
        let closes = lower.matches("</script").count();
        assert_eq!(opens, 2, "expected exactly 2 <script openings (data block + code block)");
        assert_eq!(closes, 2, "expected exactly 2 </script closings (data block + code block)");
    }
}
