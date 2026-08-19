//! The native `--format json` output must not move when the CLI stops
//! hand-rolling the orchestration and calls `axilog_api::parse_report_v1`
//! instead (M17 Task 3).
//!
//! This compares a hash of the current build's stdout against a committed
//! digest of the pre-refactor baseline (`fixtures/native-json-baseline.sha256.json`)
//! -- not a `/tmp` file, so it reproduces identically on any machine and in
//! CI (that exact `/tmp`-dependency defect was already found and fixed in
//! Task 2's ei-byte-identity guard; see that fix's commit message).
//!
//! Regeneration procedure (ONLY legitimate when the native `json` format is
//! being intentionally changed -- e.g. a new opt-in field, a schema bump --
//! never to make a red run here go green without understanding why):
//!
//!   1. Identify the last commit BEFORE the change you're validating
//!      against (for the original baseline this was 831ad3d, the last
//!      commit before the axilog-api facade refactor touched axilog-cli).
//!      Check that commit out into a throwaway worktree so your working
//!      tree's in-progress change can't leak in:
//!
//!     git worktree add /tmp/axilog-base <commit-ish>
//!     cd /tmp/axilog-base && cargo build --release -p axilog-cli
//!
//!   2. Capture the output TWICE and confirm determinism before trusting
//!      it -- if the two runs disagree (e.g. non-deterministic map
//!      iteration order, float formatting), STOP: that is a finding about
//!      the parser, not something this test can paper over.
//!
//!     ./target/release/axilog parse fixtures/wvw-small.anon.zevtc \
//!       --format json --all > /tmp/base-run1.json
//!     ./target/release/axilog parse fixtures/wvw-small.anon.zevtc \
//!       --format json --all > /tmp/base-run2.json
//!     cmp /tmp/base-run1.json /tmp/base-run2.json && echo deterministic
//!     wc -c /tmp/base-run1.json
//!     sha256sum /tmp/base-run1.json
//!
//!   3. Update `fixtures/native-json-baseline.sha256.json`'s `byteLength`
//!      and `sha256` to the (matching, both-runs-agreed) values, and its
//!      `sourceCommit` to the commit you captured from.
//!
//!   4. Clean up: `git worktree remove /tmp/axilog-base`.

use sha2::{Digest, Sha256};

#[derive(serde::Deserialize)]
struct Baseline {
    #[serde(rename = "byteLength")]
    byte_length: u64,
    sha256: String,
}

#[test]
fn native_json_matches_baseline_digest() {
    let repo_root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let baseline_path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/native-json-baseline.sha256.json");
    let baseline: Baseline = serde_json::from_str(
        &std::fs::read_to_string(baseline_path).expect("read committed baseline fixture"),
    )
    .expect("parse committed baseline fixture");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_axilog"))
        .args(["parse", "fixtures/wvw-small.anon.zevtc", "--format", "json", "--all"])
        .current_dir(repo_root)
        .output()
        .expect("cli runs");
    assert!(out.status.success(), "cli failed: {}", String::from_utf8_lossy(&out.stderr));

    let digest = format!("{:x}", Sha256::digest(&out.stdout));

    assert_eq!(
        out.stdout.len() as u64,
        baseline.byte_length,
        "native json length changed vs the committed baseline \
         (tests/fixtures/native-json-baseline.sha256.json) -- see this file's header comment \
         for the regeneration procedure before touching the baseline"
    );
    assert_eq!(
        digest, baseline.sha256,
        "native json content changed vs the committed baseline \
         (tests/fixtures/native-json-baseline.sha256.json) -- length matched but the digest did \
         not, so bytes moved within the document; see this file's header comment for the \
         regeneration procedure before touching the baseline"
    );
    println!("native json byte-identical to committed baseline: {} bytes, sha256 {digest}", out.stdout.len());
}
