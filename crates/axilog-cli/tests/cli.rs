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
