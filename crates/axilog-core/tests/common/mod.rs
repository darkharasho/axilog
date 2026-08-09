//! Shared test-support helpers (M4 Task 3).
//!
//! Factored out for `postrework_golden.rs` (the real-capture calibration
//! hook), which needs the same "skip gracefully when the gitignored local
//! fixture is absent" pattern and the same tolerance conventions every
//! `tests/*_golden.rs` file already hand-rolls (`golden.rs`'s
//! `RELATIVE_TOLERANCE`/`rel_close`, `boons_golden.rs`'s
//! `PRESENCE_TOLERANCE_PP`/`INTENSITY_STACK_RELATIVE_TOLERANCE`,
//! `support_golden.rs`'s exact-tolerance convention). Deliberately NOT
//! migrating `golden.rs`/`boons_golden.rs`/`support_golden.rs` themselves
//! onto this module -- those are already-passing, CI-gating suites, and the
//! M4 Task 3 brief is explicit that this task must not destabilize them;
//! only the new fixture-gated hook consumes this module. Because `tests/`
//! subdirectories aren't auto-discovered as their own integration-test
//! crates (only files directly under `tests/` are), this file is inert on
//! its own and only compiled in via `postrework_golden.rs`'s `mod common;`.

use std::collections::BTreeSet;

/// Duration/damage relative tolerance (M2/M3 `golden.rs` convention): 0.5%.
/// `#[allow(dead_code)]`: not every `tests/*.rs` file that pulls in this
/// shared module (each gets its own compiled copy, per Rust's integration-
/// test model) uses every item in it -- same reasoning as `boon_id_set`
/// below (M10 Task 1's `healing_golden.rs` only needs `rel_close`).
#[allow(dead_code)]
pub const RELATIVE_TOLERANCE: f64 = 0.005;

/// True if `a` and `b` are within `tol` relative of each other, relative to
/// `b` (the golden/expected value) -- `golden.rs`'s `rel_close`/`rel_close_cc`
/// generalized to a caller-supplied tolerance so this one function covers
/// both conventions. `#[allow(dead_code)]`: same reasoning as
/// `RELATIVE_TOLERANCE` above -- not every `tests/*.rs` file that pulls in
/// this shared module needs this particular helper (M14 Task 2's
/// `skill_map_golden.rs` only needs `read_bytes_or_skip`/`read_json_or_skip`).
#[allow(dead_code)]
pub fn rel_close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

/// Reads a gitignored local fixture (`fixtures/local/*`), printing a
/// `skip: ... absent` message and returning `None` (rather than failing the
/// test) when it doesn't exist -- the same skip-gracefully-in-CI pattern
/// every `tests/*_golden.rs` file's `read_local_fixture_or_skip` already
/// uses.
#[allow(dead_code)]
pub fn read_bytes_or_skip(path: &str, label: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {path} absent ({label})");
            None
        }
    }
}

/// Same skip-gracefully pattern as `read_bytes_or_skip`, for an optional
/// (also gitignored, local-only) JSON sidecar fixture -- used for the
/// dps.report EI JSON parity half of the post-rework calibration hook,
/// which is checked only when present (unlike the required `.zevtc`).
#[allow(dead_code)]
pub fn read_json_or_skip(path: &str, label: &str) -> Option<serde_json::Value> {
    let s = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            println!("skip: {path} absent ({label})");
            return None;
        }
    };
    Some(serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse JSON fixture {path}: {e}")))
}

/// Strips arcdps's leading `:` from a `Player.account`/dps.report EI
/// `account` value before joining the two by account text -- every existing
/// golden test's join site does this inline; centralized here for the one
/// new caller.
#[allow(dead_code)]
pub fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

/// The 12 tracked boon skill ids, as a set -- mirrors `analysis::mod`'s own
/// `boon_id_set` construction (`buffs::BOON_IDS.iter().map(|&(id, _, _)|
/// id).collect()`), duplicated here rather than exposed as a new pub item
/// on `buffs::events` purely for one test's convenience.
#[allow(dead_code)]
pub fn boon_id_set() -> BTreeSet<u32> {
    axilog_core::analysis::buffs::BOON_IDS.iter().map(|&(id, _, _)| id).collect()
}
