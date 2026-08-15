//! `coverage` means what it says.
//!
//! The whole point of the `coverage` map is that a consumer can tell "the
//! flag was off" from "the log cannot answer this" from "the answer is
//! genuinely zero". That only holds if all three states are REACHABLE --
//! before the final whole-branch review, `Empty` was dead code and every
//! computed block reported `Present` unconditionally, which is precisely
//! the absent-reported-as-zero ambiguity this map exists to remove.
//!
//! So these tests pin reachability, not particular values: they assert
//! that each state occurs somewhere on a real fixture under a known gate
//! setting, and that `--all`-equivalent gating leaves nothing
//! `not_computed`. A block moving between `Present` and `Empty` as the
//! fixture's analysis improves is not a regression; a state becoming
//! unreachable is.

mod common;

use axilog_schema::v1::envelope::{BlockName, CoverageState};

/// Every block's state under a given gate setting, as `(name, state)`
/// pairs -- `Coverage` exposes a `get`, not an iterator, and every test
/// here wants the whole map.
fn states(v1: &axilog_schema::v1::ReportV1) -> Vec<(BlockName, CoverageState)> {
    BlockName::ALL
        .iter()
        .map(|&b| {
            (b, v1.coverage.get(b.as_str()).unwrap_or_else(|| panic!("coverage names {b:?}")))
        })
        .collect()
}

#[test]
fn gate_off_reports_not_computed() {
    let (_e, _m, _l, v1) = common::fixture_report_no_gates();
    assert_eq!(
        v1.coverage.get(BlockName::Minions.as_str()),
        Some(CoverageState::NotComputed),
        "the minions pass did not run, which is not the same as running and finding nothing"
    );
    // Not a lone case: several blocks ride gates, and every one of them
    // must report the gate rather than an empty result.
    let gated: Vec<BlockName> = states(&v1)
        .into_iter()
        .filter(|&(_, s)| s == CoverageState::NotComputed)
        .map(|(b, _)| b)
        .collect();
    assert!(
        gated.len() >= 3,
        "expected several gated blocks to report not_computed with every gate off, got {gated:?}"
    );
}

#[test]
fn no_healing_extension_reports_unsupported() {
    // `Unsupported` is the one state that is a property of the LOG rather
    // than of the flags: arcdps writes the healing extension only when the
    // user runs the plugin that produces it, and on a log without it no
    // flag and no future pass can ever produce those numbers.
    //
    // The committed fixture DOES carry the extension (checked: it reports
    // `present` with every gate on), and the only logs here that lack one
    // are under `fixtures/local/`, which is real capture data with real
    // account names and is never read by a test. Rather than synthesize a
    // whole log, this flips the single `Metrics` property that a log
    // without the extension presents and rebuilds the document -- the same
    // real encounter, answering the same question the arcdps plugin's
    // absence would.
    let (enc, mut metrics, legacy, all_gates) = common::fixture_report_all_gates();
    assert_eq!(
        all_gates.coverage.get(BlockName::Healing.as_str()),
        Some(CoverageState::Present),
        "guard: this test is only meaningful while the fixture DOES carry the extension -- \
         if that changes, assert Unsupported off the fixture directly and drop the flip"
    );

    metrics.has_healing_extension = false;
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes::default(),
    );
    assert_eq!(
        v1.coverage.get(BlockName::Healing.as_str()),
        Some(CoverageState::Unsupported),
        "a log without the healing extension cannot answer the question, \
         which is different from answering it with zero"
    );
}

#[test]
fn empty_is_reachable_and_means_computed_with_nothing_to_report() {
    // `Empty` must be reachable on a real document, or the distinction it
    // draws against `NotComputed` is theoretical. Asserted across both
    // gate settings rather than pinned to one block, so that a fixture
    // whose analysis grows richer does not make this a false failure.
    let (_e, _m, _l, all_on) = common::fixture_report_all_gates();
    let (_e2, _m2, _l2, all_off) = common::fixture_report_no_gates();
    let empties: Vec<BlockName> = states(&all_on)
        .into_iter()
        .chain(states(&all_off))
        .filter(|&(_, s)| s == CoverageState::Empty)
        .map(|(b, _)| b)
        .collect();
    assert!(
        !empties.is_empty(),
        "no block reports `empty` on the committed fixture under either gate setting -- \
         if that is genuinely true, this test needs a fixture where it is not, because an \
         unreachable state cannot be relied on by a consumer"
    );
}

#[test]
fn all_flag_leaves_nothing_not_computed() {
    // `fixture_report_all_gates` turns on every gate the CLI's `--all`
    // turns on, so this is `--all`'s contract stated as a test: after it,
    // no block may still be reporting "you did not ask for this".
    //
    // `Unsupported` is explicitly allowed through -- it is the log's
    // answer, not the flags', and no flag can change it.
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    for (block, state) in states(&v1) {
        assert_ne!(
            state,
            CoverageState::NotComputed,
            "--all must compute {block:?}; a block still reporting not_computed with every \
             gate on means a pass exists that --all does not reach"
        );
    }
}
