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

/// The other half of the sentinel problem, and the one `resolve_teams`
/// cannot reach: an agent whose EVERY `TEAM_CHANGE` is `1875`. There is no
/// real id to prefer, so the friend/foe partition falls back to the EVTC
/// subgroup tag. Asserted at the partition (`wvw::apply`), not at
/// `resolve_teams` -- the team map still reads `1875` for these agents by
/// construction, and the roster is what the fix moves.
///
/// `wvw-sentinel-only-team.zevtc` is `20260824-175919`, where squad member
/// `Aidan Von.6248` emits `1875` and nothing else. Sweeping all 4084 local
/// captures found 73 such players across 70 logs, every one of them on
/// `1875` with a resolvable friendly team.
#[test]
fn sentinel_only_team_id_falls_back_to_the_subgroup_tag() {
    let path = common::local_fixture("wvw-sentinel-only-team.zevtc");
    let Some(bytes) = common::read_bytes_or_skip(&path, "sentinel-only-team capture") else {
        return;
    };
    let raw = decode_raw(&bytes).expect("decode capture");
    let (agent_team, recorded_by) = axilog_core::wvw::resolve_teams(&raw);
    let friendly = recorded_by
        .and_then(|addr| agent_team.get(&addr).copied())
        .expect("capture has a POINT_OF_VIEW agent with a team");

    // Precondition: this capture really does contain the unreachable case,
    // so the assertions below are testing the fallback and not a log that
    // `resolve_teams` already handles on its own.
    let sentinel_only: Vec<u64> = raw
        .agents
        .iter()
        .filter(|a| a.is_player() && a.name_parts().2.unwrap_or(0) != 0)
        .map(|a| a.addr)
        .filter(|addr| agent_team.get(addr).copied().is_some_and(|t| t != friendly))
        .collect();
    assert!(
        !sentinel_only.is_empty(),
        "capture must contain a squad member whose team id never resolves",
    );

    let mut enc = axilog_core::model::resolve(&raw);
    axilog_core::wvw::apply(&mut enc, &raw);

    let squad: std::collections::BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    for addr in &sentinel_only {
        assert!(
            squad.contains(addr),
            "subgroup-tagged agent {addr} (team {:?}) was filed as an enemy",
            agent_team.get(addr),
        );
    }
    let friendly_color = enc
        .players
        .iter()
        .find(|p| Some(p.agent_addr) == recorded_by)
        .map(|p| p.team.clone())
        .expect("recorder is on the squad roster");
    assert_ne!(friendly_color, "unknown");
    for addr in &sentinel_only {
        let p = enc.players.iter().find(|p| p.agent_addrs.contains(addr)).unwrap();
        assert_eq!(p.team, friendly_color, "a rescued member takes the recorder's colour");
    }
}
