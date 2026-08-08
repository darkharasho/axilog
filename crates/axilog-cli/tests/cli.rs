use std::process::Command;

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);

#[test]
fn parses_fixture_to_json() {
    if std::fs::metadata(FIXTURE_PATH).is_err() {
        println!("skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run this test)");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", FIXTURE_PATH])
        .output()
        .expect("run axilog");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "0.1");
    assert!(v["players"].as_array().unwrap().len() > 0);
}

#[test]
fn table_and_csv_have_headers() {
    if std::fs::metadata(FIXTURE_PATH).is_err() {
        println!("skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run this test)");
        return;
    }
    // Build a report via the library path is out of scope for a bin test;
    // instead run the binary against the fixture.
    for (fmt, needle) in [("table", "DPS"), ("csv", "account,")] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_axilog"))
            .args(["parse", FIXTURE_PATH, "--format", fmt])
            .output().unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains(needle), "format {fmt} missing {needle}");
    }
}
