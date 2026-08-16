use axilog_core::evtc::decode_raw;

mod common;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
fn local_fixture_path() -> String {
    common::local_fixture("wvw-small.zevtc")
}

fn check_decodes(bytes: &[u8]) {
    let raw = decode_raw(bytes).unwrap();
    assert_eq!(raw.header.revision, 1);
    assert!(!raw.agents.is_empty());
    assert!(!raw.skills.is_empty());
    assert!(!raw.events.is_empty());
    // This helper runs against BOTH the committed fixture and a local one,
    // so it can only assert what holds for any log. The layout/count
    // agreement it used to claim to check is pinned on the committed
    // fixture instead -- see `decodes_committed_wvw_fixture`.
}

/// Committed, PII-safe fixture — always present, so this runs in CI too
/// (Task 5, M2).
#[test]
fn decodes_committed_wvw_fixture() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode committed fixture");
    check_decodes(&bytes);

    // The exact decoded counts, pinned. This replaces an assertion that
    // claimed to check "event count computed from layout matches decoded
    // vec length" but actually read `assert_eq!(raw.events.len(),
    // raw.events.len())` -- a value compared to itself, which no defect
    // could ever fail (found 2026-08-16 by clippy's neighbouring
    // `len() > 0` lint, not by the test failing, because it cannot fail).
    //
    // A pinned count is the check that was wanted. The failure it guards
    // is real and has happened here before: `EVENT_SIZE_REV1` was once 96
    // instead of 64, which silently misaligned every event after the first
    // -- decode still "succeeded" and returned a plausible-looking vec,
    // just a shorter one full of garbage. A wrong stride changes this
    // number; a self-comparison would have stayed green through it.
    assert_eq!(raw.events.len(), 120_435, "decoded event count moved");
    assert_eq!(raw.agents.len(), 173, "decoded agent count moved");
    assert_eq!(raw.skills.len(), 969, "decoded skill count moved");
}

/// Belt-and-braces: when the real local raw fixture is also present
/// (gitignored, PII, dev-only), decode it too.
#[test]
fn decodes_local_raw_wvw_fixture_when_present() {
    let bytes = match std::fs::read(local_fixture_path()) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {} absent (local-only extra check)", local_fixture_path());
            return;
        }
    };
    check_decodes(&bytes);
}

/// MATTRIB Task 1: `decode_raw` runs GW2EI's `EvtcParser.CompleteAgents`
/// orphaned-instid repair, so no consumer ever sees an addr-`0` row whose
/// instid names a live agent within `±300ms` of that agent's aware window.
/// Pinned on the committed fixture (43 of its 46 orphans are repairable:
/// 39 `EffectGroundCreate` src rows + 4 `MissileLaunch` dst rows) so the
/// pre-pass cannot silently stop running.
#[test]
fn committed_fixture_has_no_repairable_orphans_left_after_decode() {
    use axilog_core::evtc::{dst_is_agent, repair_orphaned_agents, src_is_agent, RepairStats};

    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let raw = decode_raw(&bytes).expect("decode committed fixture");

    // Re-running the repair on an already-repaired stream must be a no-op
    // on the repairable rows (the 3 that stay are the ones GW2EI's
    // aware-window rule rejects; they keep their zero addr by design).
    let mut events = raw.events.clone();
    let again = repair_orphaned_agents(&raw.agents, &mut events);
    assert_eq!(
        again,
        RepairStats { src_orphans: 3, dst_orphans: 0, src_repaired: 0, dst_repaired: 0 },
        "decode_raw must leave only the unrepairable orphans behind"
    );
    assert!(
        events.iter().zip(&raw.events).all(|(a, b)| a.src_agent == b.src_agent
            && a.dst_agent == b.dst_agent),
        "a second repair pass must be idempotent"
    );

    // And the surviving zero-addr rows are exactly those three.
    let left = raw
        .events
        .iter()
        .filter(|e| {
            (src_is_agent(e.is_statechange) && e.src_agent == 0 && e.src_instid > 0)
                || (dst_is_agent(e.is_statechange) && e.dst_agent == 0 && e.dst_instid > 0)
        })
        .count();
    assert_eq!(left, 3);
}
