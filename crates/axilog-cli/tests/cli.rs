use std::process::Command;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);

fn check_parses_fixture_to_json(fixture_path: &str) {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", fixture_path])
        .output()
        .expect("run axilog");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "0.2");
    assert!(v["players"].as_array().unwrap().len() > 0);
}

/// Committed, PII-safe fixture — always present, so this runs in CI too
/// (Task 5, M2).
#[test]
fn parses_fixture_to_json() {
    check_parses_fixture_to_json(ANON_FIXTURE_PATH);
}

/// Belt-and-braces: when the real local raw fixture is also present
/// (gitignored, PII, dev-only), parse it too.
#[test]
fn parses_local_raw_fixture_to_json_when_present() {
    if std::fs::metadata(LOCAL_FIXTURE_PATH).is_err() {
        println!("skip: {LOCAL_FIXTURE_PATH} absent (local-only extra check)");
        return;
    }
    check_parses_fixture_to_json(LOCAL_FIXTURE_PATH);
}

fn check_table_and_csv_have_headers(fixture_path: &str) {
    for (fmt, needle) in [("table", "DPS"), ("csv", "account,")] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_axilog"))
            .args(["parse", fixture_path, "--format", fmt])
            .output().unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains(needle), "format {fmt} missing {needle}");
    }
}

#[test]
fn table_and_csv_have_headers() {
    check_table_and_csv_have_headers(ANON_FIXTURE_PATH);
}

#[test]
fn table_and_csv_have_headers_local_raw_when_present() {
    if std::fs::metadata(LOCAL_FIXTURE_PATH).is_err() {
        println!("skip: {LOCAL_FIXTURE_PATH} absent (local-only extra check)");
        return;
    }
    check_table_and_csv_have_headers(LOCAL_FIXTURE_PATH);
}

/// M3, Task 5: `--view support` — header token plus a known row value from
/// the committed anon fixture (`:Anon133.5921`, Elementalist, 93
/// cleanses — the top cleanser in the fixture).
#[test]
fn table_view_support_has_header_and_known_row() {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", ANON_FIXTURE_PATH, "--format", "table", "--view", "support"])
        .output()
        .expect("run axilog");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("cleanses"), "support view header missing 'cleanses'");
    assert!(s.contains("strips"), "support view header missing 'strips'");
    assert!(s.contains("resurrects"), "support view header missing 'resurrects'");
    assert!(s.contains("stunbreaks"), "support view header missing 'stunbreaks'");
    assert!(
        s.contains(":Anon133.5921") && s.contains("Elementalist") && s.contains("93"),
        "support view missing known row (:Anon133.5921, Elementalist, 93 cleanses):\n{s}"
    );
}

/// M3, Task 5: `--view boons` — header token plus a known row value from
/// the committed anon fixture (`:Anon163.7031`, Mesmer, ~37.4% Alacrity
/// presence — the only player with any Alacrity uptime in this fixture).
#[test]
fn table_view_boons_has_header_and_known_row() {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", ANON_FIXTURE_PATH, "--format", "table", "--view", "boons"])
        .output()
        .expect("run axilog");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("Might(avg)"), "boons view header missing 'Might(avg)'");
    assert!(s.contains("Quick%"), "boons view header missing 'Quick%'");
    assert!(s.contains("Alac%"), "boons view header missing 'Alac%'");
    assert!(s.contains("Stab%"), "boons view header missing 'Stab%'");
    assert!(s.contains("Prot%"), "boons view header missing 'Prot%'");
    let row = s.lines().find(|l| l.contains(":Anon163.7031")).expect("known player row present");
    assert!(row.contains("Mesmer"), "known row missing profession: {row}");
    assert!(row.contains("37.4"), "known row missing Alacrity presence 37.4%: {row}");
}

/// M10, Task 1: `--view healing` — header token plus a known row value from
/// the committed anon fixture (`:Anon125.5625`, Ranger, the fixture's top
/// healer at 208414 total healing out / 42827 downed healing -- see
/// `axilog-core/tests/healing_golden.rs` for the calibration this number
/// traces back to).
#[test]
fn table_view_healing_has_header_and_known_row() {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", ANON_FIXTURE_PATH, "--format", "table", "--view", "healing"])
        .output()
        .expect("run axilog");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("healing out"), "healing view header missing 'healing out'");
    assert!(s.contains("allies"), "healing view header missing 'allies'");
    assert!(s.contains("barrier"), "healing view header missing 'barrier'");
    assert!(s.contains("downed"), "healing view header missing 'downed'");
    let row = s.lines().find(|l| l.contains(":Anon125.5625")).expect("known player row present");
    assert!(row.contains("Ranger"), "known row missing profession: {row}");
    assert!(row.contains("208414"), "known row missing healing out total 208414: {row}");
    assert!(row.contains("42827"), "known row missing downed healing 42827: {row}");
}

/// Task 5 (M2): the `anonymize` subcommand itself, exercised against the
/// already-anonymized committed fixture (always present, so this runs in
/// CI — running it on an already-`Anon<N>`-named file just re-anonymizes to
/// new `Anon<N>` values, a fine, deterministic roundtrip to exercise the
/// subcommand end-to-end). Confirms: exit success, output is a decodable
/// .zevtc, and every metric-bearing field is unchanged (names never feed
/// metrics).
#[test]
fn anonymize_subcommand_roundtrips_metrics() {
    let dir = std::env::temp_dir();
    let out_path = dir.join(format!("axilog-anonymize-test-{}.zevtc", std::process::id()));

    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["anonymize", ANON_FIXTURE_PATH, out_path.to_str().unwrap()])
        .output()
        .expect("run axilog anonymize");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out_path.exists(), "anonymize subcommand did not write an output file");

    let before = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", ANON_FIXTURE_PATH])
        .output()
        .expect("run axilog parse (before)");
    let after = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", out_path.to_str().unwrap()])
        .output()
        .expect("run axilog parse (after)");
    let _ = std::fs::remove_file(&out_path);

    assert!(before.status.success());
    assert!(after.status.success());
    let mut v_before: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    let mut v_after: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();

    // Strip name fields (expected to differ — re-anonymized to fresh
    // Anon<N> values) before comparing the rest of the report.
    for v in [&mut v_before, &mut v_after] {
        if let Some(players) = v["players"].as_array_mut() {
            for p in players {
                p.as_object_mut().unwrap().remove("account");
                p.as_object_mut().unwrap().remove("character");
            }
        }
        if let Some(enemies) = v["enemies"].as_array_mut() {
            for e in enemies {
                e.as_object_mut().unwrap().remove("name");
            }
        }
        v["encounter"]["recorded_by"] = serde_json::Value::Null;
    }
    assert_eq!(v_before, v_after, "anonymize subcommand must not change any metric-bearing field");
}
