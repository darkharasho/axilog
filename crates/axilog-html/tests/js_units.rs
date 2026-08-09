//! Node-based unit tests for `report.js`'s pure functions (Task 2, M7
//! plan): sorting (direction + stability), formatting (thousands
//! separators, percentages, boon presence), and the team-chip builder
//! (including the hide-zero-total rule). The actual assertions live in
//! `tests/js/pure_fn_tests.mjs`, imported via Node's CJS/ESM default-
//! export interop against `assets/report.js` directly (no build step —
//! that file is inlined verbatim into the HTML report by `src/lib.rs`, so
//! testing it in place is testing exactly what ships).
//!
//! Per the plan's brief for this task: "Skip-gracefully if node absent
//! (CI ubuntu has it)" — CI's `cargo test --workspace` step runs before
//! `actions/setup-node@v4`, and even once Node is set up elsewhere in the
//! job it's specific to the `axilog-node` working directory, so this test
//! does not assume `node` is guaranteed to be on PATH; it probes for it
//! first and prints a skip message (without failing the suite) if it
//! isn't found, rather than asserting Node is installed.

use std::process::Command;

#[test]
fn report_js_pure_functions_pass_node_unit_tests() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping report_js_pure_functions_pass_node_unit_tests: `node` not on PATH");
        return;
    }

    let runner = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js/pure_fn_tests.mjs");
    let output = Command::new("node")
        .arg(runner)
        .output()
        .expect("failed to spawn `node`");

    assert!(
        output.status.success(),
        "node pure-function tests failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok -"),
        "expected pure_fn_tests.mjs's summary line in stdout, got:\n{stdout}"
    );
}
