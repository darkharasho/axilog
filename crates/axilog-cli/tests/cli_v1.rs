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
