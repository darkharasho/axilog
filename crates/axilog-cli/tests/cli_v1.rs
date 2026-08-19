//! The CLI's native format is the 1.0 container.
use std::process::Command;

#[test]
fn native_json_output_is_the_one_point_oh_container() {
    let exe = env!("CARGO_BIN_EXE_axilog");
    let out = Command::new(exe)
        .args(["parse", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"), "--format", "json"])
        .output()
        .expect("run axilog parse");
    assert!(out.status.success(), "parse failed: {}", String::from_utf8_lossy(&out.stderr));

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["axilog"]["schema"], "1.0");
    assert!(v.get("entities").is_some(), "1.0 emits entities[]");
    assert!(v.get("coverage").is_some(), "1.0 emits coverage");
    assert!(v.get("players").is_none(), "the legacy players[] is gone from native output");
}

#[test]
fn ei_json_output_is_untouched_by_the_native_reshape() {
    let exe = env!("CARGO_BIN_EXE_axilog");
    let out = Command::new(exe)
        .args(["parse", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"), "--format", "ei-json"])
        .output()
        .expect("run axilog parse");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(v.get("players").is_some(), "ei-json keeps EI's shape");
    assert!(v.get("entities").is_none(), "ei-json is not the native container");
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");

fn parse_json(args: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", FIXTURE, "--format", "json"])
        .args(args)
        .output()
        .expect("run axilog parse");
    assert!(out.status.success(), "parse failed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("valid JSON")
}

/// `--all` computes every pass this binary knows about.
///
/// Asserted through `coverage` rather than against a list of blocks, on
/// purpose: `--all` is DEFINED as "everything that exists in this version",
/// so a test enumerating blocks would drift from it exactly the way a
/// consumer's hand-maintained flag list does -- which is the drift the flag
/// exists to prevent. A pass added later with no `--all` wiring fails this
/// without anyone remembering to update it.
#[test]
fn all_flag_leaves_no_block_not_computed() {
    let v = parse_json(&["--all"]);
    let coverage = v["coverage"].as_object().expect("coverage is an object");
    let missed: Vec<&String> =
        coverage.iter().filter(|(_, s)| *s == "not_computed").map(|(b, _)| b).collect();
    assert!(missed.is_empty(), "--all must compute every block; still not_computed: {missed:?}");

    // `unsupported` stays legal -- it is the LOG's answer, not the flags'.
    for (block, state) in coverage {
        let s = state.as_str().unwrap_or_default();
        assert!(
            matches!(s, "present" | "empty" | "unsupported"),
            "unexpected coverage state for {block}: {s}"
        );
    }

    // It genuinely turns gates on: the default parse leaves several off.
    let bare = parse_json(&[]);
    let off = bare["coverage"]
        .as_object()
        .expect("coverage is an object")
        .values()
        .filter(|s| *s == "not_computed")
        .count();
    assert!(off >= 3, "expected the default parse to leave gates off, got {off}");
}

/// `--all` is a UNION with the individual flags, never an override.
#[test]
fn all_flag_unions_with_the_individual_flags() {
    assert_eq!(
        parse_json(&["--all"]),
        parse_json(&["--all", "--replay", "--modifiers"]),
        "--all and --all --replay --modifiers must produce the same document"
    );
}

/// M17 Task 3 review fix: `--format json` must surface every warning
/// `ReportV1` reports on stderr, unfiltered -- including
/// `recorded_by_unresolved` (`crates/axilog-schema/src/v1/mod.rs`'s
/// deliberately-designed diagnostic for an unresolvable recording-player
/// account), which a prior version of this CLI path filtered out to force
/// byte-identical stderr with the pre-facade CLI. That filter suppressed a
/// diagnostic the schema layer exists to surface; this test guards against
/// it coming back.
///
/// The committed fixture (`fixtures/wvw-small.anon.zevtc`) does not trigger
/// `recorded_by_unresolved` or any other warning today, so this cannot
/// assert on that specific code -- `axilog-schema`'s own unit test
/// (`crates/axilog-schema/src/v1/mod.rs`, search for
/// `recorded_by_unresolved`) already covers that condition with a
/// synthetic `Encounter`/`Metrics`, and manufacturing a `.zevtc` fixture
/// here to reach the same condition through a real log would be a heavier,
/// less direct duplicate of that coverage. Instead this asserts the
/// general contract directly against the real facade output: whatever
/// `axilog_api::parse_report_v1` reports in `warnings`, `--format json`
/// must print to stderr, one `warning: {message}` line per entry, with no
/// filtering in between -- true today (zero warnings, zero lines) and
/// still true if this fixture, or a future one, ever starts triggering
/// one.
#[test]
fn json_format_surfaces_every_report_v1_warning_on_stderr_unfiltered() {
    let bytes = std::fs::read(FIXTURE).expect("read committed fixture");
    let report_v1 = axilog_api::parse_report_v1(
        &bytes,
        &axilog_api::ParseOpts::everything(),
        None,
    )
    .expect("facade parses the committed fixture");

    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", FIXTURE, "--format", "json", "--all"])
        .output()
        .expect("run axilog parse");
    assert!(out.status.success(), "parse failed: {}", String::from_utf8_lossy(&out.stderr));

    let stderr = String::from_utf8_lossy(&out.stderr);
    let warning_lines: Vec<&str> =
        stderr.lines().filter(|l| l.starts_with("warning: ")).collect();

    assert_eq!(
        warning_lines.len(),
        report_v1.warnings.len(),
        "every warning ReportV1 reports must reach stderr unfiltered -- got {} stderr warning \
         line(s) but ReportV1 reports {} -- if this fixture starts triggering a warning (e.g. \
         recorded_by_unresolved) and this count stops matching, a filter has come back",
        warning_lines.len(),
        report_v1.warnings.len(),
    );
    for (line, w) in warning_lines.iter().zip(report_v1.warnings.iter()) {
        assert_eq!(*line, format!("warning: {}", w.message));
    }
}
