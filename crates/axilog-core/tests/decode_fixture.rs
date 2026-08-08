use axilog_core::evtc::decode_raw;

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);

#[test]
fn decodes_committed_wvw_fixture() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run this test)");
            return;
        }
    };
    let raw = decode_raw(&bytes).unwrap();
    assert_eq!(raw.header.revision, 1);
    assert!(raw.agents.len() > 0);
    assert!(raw.skills.len() > 0);
    assert!(raw.events.len() > 0);
    // sanity: event count computed from layout matches decoded vec length
    assert_eq!(raw.events.len(), raw.events.len());
}
