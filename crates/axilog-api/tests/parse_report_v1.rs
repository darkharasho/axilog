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

#[test]
fn received_series_ride_the_timeseries_gate() {
    let bytes = fixture();

    // `blocks.series` is built unconditionally (see `v1/mod.rs`'s
    // `series = activity::build_series(...)` with no `.then()` gate,
    // unlike `missiles`/`damage_mods`), so it is `Some` even with the
    // gate off. What the `--timeseries` flag controls is the per-entity
    // rows within it: `healing_1s` (and now its two received siblings)
    // are absent until the flag turns the healing pass on.
    let off = parse_report_v1(&bytes, &ParseOpts::default(), None).unwrap();
    let off_series = off.blocks.series.expect("series block is built unconditionally");
    assert!(
        off_series.by_entity.0.values().all(|e| e.healing_1s.is_none()
            && e.healing_received_1s.is_none()
            && e.barrier_received_1s.is_none()),
        "healing series are gated on --timeseries, not present by default"
    );

    let on = parse_report_v1(
        &bytes,
        &ParseOpts { timeseries: true, ..Default::default() },
        None,
    ).unwrap();
    let series = on.blocks.series.expect("series present under the gate");

    // Whoever has an outgoing healing series must also carry the two
    // received series -- one gate, three fields.
    let mut checked = 0;
    for (id, e) in &series.by_entity.0 {
        if e.healing_1s.is_some() {
            assert!(e.healing_received_1s.is_some(), "entity {id} missing healing_received_1s");
            assert!(e.barrier_received_1s.is_some(), "entity {id} missing barrier_received_1s");
            let h = e.healing_received_1s.as_ref().unwrap();
            assert_eq!(h.len, e.healing_1s.as_ref().unwrap().len, "same grid length");
            assert_eq!(h.interval_ms, 1000);
            checked += 1;
        }
    }
    assert!(checked > 0, "fixture must exercise the healing extension");
}
