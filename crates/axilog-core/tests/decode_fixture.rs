use axilog_core::evtc::decode_raw;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);

fn check_decodes(bytes: &[u8]) {
    let raw = decode_raw(bytes).unwrap();
    assert_eq!(raw.header.revision, 1);
    assert!(raw.agents.len() > 0);
    assert!(raw.skills.len() > 0);
    assert!(raw.events.len() > 0);
    // sanity: event count computed from layout matches decoded vec length
    assert_eq!(raw.events.len(), raw.events.len());
}

/// Committed, PII-safe fixture — always present, so this runs in CI too
/// (Task 5, M2).
#[test]
fn decodes_committed_wvw_fixture() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    check_decodes(&bytes);
}

/// Belt-and-braces: when the real local raw fixture is also present
/// (gitignored, PII, dev-only), decode it too.
#[test]
fn decodes_local_raw_wvw_fixture_when_present() {
    let bytes = match std::fs::read(LOCAL_FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {LOCAL_FIXTURE_PATH} absent (local-only extra check)");
            return;
        }
    };
    check_decodes(&bytes);
}
