//! `blocks.minions` and `series[].health_percents` -- the two surfaces
//! side-channel absorption Task 6 moved onto the native wire.
//!
//! Both were previously computed ONLY when the output format was ei-json,
//! so a native consumer could not see them at any flag combination. These
//! tests assert the entity-keyed reprojection is faithful to the pass and
//! that the gates still mean what they meant.

mod common;
use common::{fixture_report_all_gates, fixture_report_no_gates};
use axilog_schema::v1::envelope::{BlockName, CoverageState};

/// The pass is positional over `enc.players`; the block is entity-keyed.
/// Every non-empty positional slot must land under the right entity, with
/// its groups in the pass's own order.
#[test]
fn minions_are_rekeyed_from_player_position_to_entity_id() {
    let (enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let rollups = axilog_core::analysis::minions::build(&raw, &enc);

    let block = v1.blocks.minions.as_ref().expect("--skill-damage was on");
    let mut checked = 0;
    for (i, groups) in rollups.iter().enumerate() {
        let entity_id = v1.source_order.players()[i];
        if groups.is_empty() {
            assert!(
                block.by_entity.get(entity_id).is_none(),
                "a player with no minions gets no row, not an empty one"
            );
            continue;
        }
        let rows = block
            .by_entity
            .get(entity_id)
            .unwrap_or_else(|| panic!("player {i} lost its minions in the rekey"));
        assert_eq!(rows.len(), groups.len(), "player {i} lost a minion group");
        for (row, group) in rows.iter().zip(groups) {
            // Identity resolves through the catalog, not the row -- no
            // block in this format inlines a human-readable name.
            let ident = v1
                .catalogs
                .minions
                .get(&row.minion_id)
                .expect("every minion_id resolves");
            assert_eq!(ident.species_id, group.species_id);
            assert_eq!(ident.name, group.name);
            assert_eq!(row.taken.len(), group.taken.len(), "a damage-taken skill row went missing");
            for taken in &group.taken {
                let got = row
                    .taken
                    .get(&taken.skill_id)
                    .unwrap_or_else(|| panic!("skill {} missing", taken.skill_id));
                assert_eq!(got.total, taken.total, "no number may change in this spec");
                assert_eq!(got.connected_hits, taken.connected_hits);
                assert_eq!(got.indirect, taken.indirect);
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "premise: the committed fixture has minions to check");
}

/// Every skill id a minion row references must resolve in the skill
/// catalog. This is the invariant that makes the block readable on its
/// own, and the reason absorbing minions ADDS entries to ei-json's
/// `skillMap`: those ids were referenced but unresolvable before.
#[test]
fn every_minion_damage_taken_skill_resolves_in_the_catalog() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let block = v1.blocks.minions.as_ref().expect("--skill-damage was on");
    for rows in block.by_entity.0.values() {
        for row in rows {
            for skill_id in row.taken.keys() {
                assert!(
                    v1.catalogs.skills.contains_key(skill_id),
                    "minion damage-taken skill {skill_id} is a dangling reference"
                );
            }
        }
    }
}

#[test]
fn minions_are_absent_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_gates();
    assert!(v1.blocks.minions.is_none());
    assert_eq!(v1.coverage.get(BlockName::Minions.as_str()), Some(CoverageState::NotComputed));
    assert!(
        v1.catalogs.minions.is_empty(),
        "no minion identities either -- the catalog follows the block"
    );
}

/// Absent and empty are different for this field, and the difference is
/// load-bearing: the pass keys its map off `HEALTH_UPDATE` events, so a
/// player that emitted none is absent from it, and ei-json then omits
/// `healthPercents` for that player rather than writing `[]`.
#[test]
fn health_percents_distinguish_never_seen_from_no_transitions() {
    let (enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let pass = axilog_core::analysis::health::ei_health_percents(&raw, &enc);

    let block = v1.blocks.series.as_ref().expect("series is always built");
    let mut with_data = 0;
    for (i, p) in enc.players.iter().enumerate() {
        let entity_id = v1.source_order.players()[i];
        let Some(row) = block.by_entity.get(entity_id) else { continue };
        match pass.get(&p.agent_addr) {
            Some(expected) => {
                assert_eq!(
                    row.health_percents.as_ref(),
                    Some(expected),
                    "health series must survive the rekey unchanged"
                );
                with_data += 1;
            }
            None => assert!(
                row.health_percents.is_none(),
                "a player the pass never saw must be None, not an empty list"
            ),
        }
    }
    assert!(with_data > 0, "premise: the committed fixture has health updates");
}

#[test]
fn health_percents_are_absent_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_gates();
    let block = v1.blocks.series.as_ref().expect("series is built even with no gates");
    assert!(
        block.by_entity.0.values().all(|r| r.health_percents.is_none()),
        "no --timeseries means no health series"
    );
}
