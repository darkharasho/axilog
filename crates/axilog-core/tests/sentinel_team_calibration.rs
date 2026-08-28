//! Real-capture calibration for the `1875` sentinel team id.
//!
//! Squad membership has an authority independent of `TEAM_CHANGE`: arcdps
//! writes the player's squad subgroup into the EVTC agent name block
//! (`character \0 account \0 subgroup`), and only squad members get a
//! non-zero one. This test uses that as ground truth to assert that the
//! friend/foe partition never files a subgroup-tagged player as an enemy.
//!
//! Gitignored local fixture, skipped when absent -- same pattern as
//! `postrework_golden.rs`. `wvw-sentinel-team.zevtc` is the capture the
//! regression was found in (`20260117-180458`): squad member
//! `ZephyrLagoon.2752` emits `1875` mid-fight and its real team `2767`
//! afterwards, and first-write-wins latched the sentinel.

use axilog_core::evtc::decode_raw;

mod common;

#[test]
fn sentinel_team_id_does_not_exile_a_squad_member() {
    let path = common::local_fixture("wvw-sentinel-team.zevtc");
    let Some(bytes) = common::read_bytes_or_skip(&path, "sentinel-team calibration capture") else {
        return;
    };
    let raw = decode_raw(&bytes).expect("decode capture");

    let (agent_team, recorded_by) = axilog_core::wvw::resolve_teams(&raw);
    let friendly = recorded_by
        .and_then(|addr| agent_team.get(&addr).copied())
        .expect("capture has a POINT_OF_VIEW agent with a team");

    // Every subgroup-tagged player must resolve onto the recorder's team.
    let mut exiled = Vec::new();
    let mut squad = 0usize;
    for a in raw.agents.iter().filter(|a| a.is_player()) {
        let (character, account, subgroup) = a.name_parts();
        if subgroup.unwrap_or(0) == 0 {
            continue; // an ally outside the squad, or an enemy
        }
        squad += 1;
        if agent_team.get(&a.addr).copied() != Some(friendly) {
            exiled.push(format!(
                "{character} ({account}) resolved to team {:?}, friendly is {friendly}",
                agent_team.get(&a.addr),
            ));
        }
    }

    assert!(squad > 0, "capture must contain squad-tagged players");
    assert!(
        exiled.is_empty(),
        "{} of {squad} squad members were filed as enemies:\n  {}",
        exiled.len(),
        exiled.join("\n  "),
    );
}
