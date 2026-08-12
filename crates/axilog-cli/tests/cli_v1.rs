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
