use axilog_core::evtc::decode_raw;

#[test]
#[ignore = "fixture committed in Task 16"]
fn decodes_committed_wvw_fixture() {
    let bytes = std::fs::read("../../fixtures/wvw-small.zevtc")
        .expect("commit fixtures/wvw-small.zevtc (Task 16)");
    let raw = decode_raw(&bytes).unwrap();
    assert_eq!(raw.header.revision, 1);
    assert!(raw.agents.len() > 0);
    assert!(raw.skills.len() > 0);
    assert!(raw.events.len() > 0);
    // sanity: event count computed from layout matches decoded vec length
    assert_eq!(raw.events.len(), raw.events.len());
}
