use axilog_api::{parse_report_v1, ParseOpts};

fn fixture() -> Vec<u8> {
    // Path taken from crates/axilog-ei/tests/common/mod.rs
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("fixture readable")
}

// `Coverage::get` returns the `CoverageState` enum, not a `String`; go
// through `serde_json` (already a dev-dependency) to compare against the
// same snake_case strings the wire format uses.
fn coverage_str(coverage: &axilog_api::v1::envelope::Coverage, block: &str) -> Option<String> {
    let value = serde_json::to_value(coverage).expect("coverage serializes");
    value.get(block).and_then(|v| v.as_str()).map(str::to_owned)
}

#[test]
fn default_opts_compute_nothing_optional() {
    let r = parse_report_v1(&fixture(), &ParseOpts::default(), None).unwrap();
    // `missiles`/`damage_mods` have no ungated half, so they read
    // `not_computed` outright when their flag is off.
    assert_eq!(coverage_str(&r.coverage, "missiles").as_deref(), Some("not_computed"));
    assert_eq!(coverage_str(&r.coverage, "damage_mods").as_deref(), Some("not_computed"));
    // `rotation` is always built (its `aftercast` half is ungated), so
    // with the gate off it reads `empty` -- never `not_computed` -- rather
    // than `present`.
    assert_eq!(coverage_str(&r.coverage, "rotation").as_deref(), Some("empty"));
    // The always-on half of replay is present regardless of the gate.
    assert_eq!(coverage_str(&r.coverage, "replay").as_deref(), Some("present"));
}

#[test]
fn everything_is_a_union_not_an_override() {
    let all = parse_report_v1(&fixture(), &ParseOpts::everything(), None).unwrap();
    for block in ["damage", "defenses", "boons", "support", "contribution",
                  "healing", "cc", "rotation", "series", "replay"] {
        assert_eq!(
            coverage_str(&all.coverage, block).as_deref(),
            Some("present"),
            "block {block} should be present under everything()"
        );
    }
    // A single explicit flag ORed with `everything` must not narrow anything.
    let mixed = parse_report_v1(
        &fixture(),
        &ParseOpts { rotation: true, everything: true, ..Default::default() },
        None,
    ).unwrap();
    assert_eq!(all.coverage, mixed.coverage);
}

#[test]
fn generated_from_is_threaded() {
    let r = parse_report_v1(&fixture(), &ParseOpts::default(), Some("wvw.zevtc")).unwrap();
    assert_eq!(r.axilog.generated_from.as_deref(), Some("wvw.zevtc"));
}
