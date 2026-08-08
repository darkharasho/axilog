use std::process::Command;

#[test]
#[ignore = "fixture committed in Task 16"]
fn parses_fixture_to_json() {
    let out = Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", "../../fixtures/wvw-small.zevtc"])
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
#[ignore = "fixture committed in Task 16"]
fn table_and_csv_have_headers() {
    // Build a report via the library path is out of scope for a bin test;
    // instead run the binary against the fixture.
    for (fmt, needle) in [("table", "DPS"), ("csv", "account,")] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_axilog"))
            .args(["parse", "../../fixtures/wvw-small.zevtc", "--format", fmt])
            .output().unwrap();
        assert!(out.status.success());
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains(needle), "format {fmt} missing {needle}");
    }
}
