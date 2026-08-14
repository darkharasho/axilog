//! `SourceOrder` must reproduce the legacy report's iteration orders
//! exactly, because ei-json's positional arrays are indexed by them.

mod common;
use common::fixture_report;

#[test]
fn player_order_matches_the_legacy_report() {
    let (enc, metrics, legacy, v1) = fixture_report();

    let by_source: Vec<u32> = v1.source_order.players().to_vec();
    assert_eq!(
        by_source.len(),
        legacy.players.len(),
        "every legacy player must appear exactly once in source order"
    );

    // Position i in source order must be the entity for legacy.players[i].
    for (i, p) in legacy.players.iter().enumerate() {
        let entity_id = by_source[i];
        let e = &v1.entities[entity_id as usize];
        assert_eq!(
            e.account.as_deref(),
            Some(p.account.as_str()),
            "source-order slot {i} resolves to the wrong entity"
        );
    }
    let _ = (&enc, &metrics);
}

// RULING PF-2 FINDING, fixed under RULING T1-1 (see task-1-report.md): this
// test originally failed under the corrected, character-first label
// precedence because `build_entities` never populated `character` OR `name`
// for `Role::EnemyPlayer` entities. Fixed at the source
// (`crates/axilog-schema/src/v1/entities.rs`'s enemy loop now sets
// `name: Some(e.name.clone())` unconditionally), not by changing this test's
// precedence or by ignoring the failure.
#[test]
fn target_order_matches_the_legacy_ei_targets() {
    let (_enc, _metrics, legacy, v1) = fixture_report();

    let by_source: Vec<u32> = v1.source_order.targets().to_vec();
    assert_eq!(by_source.len(), legacy.ei_targets.len());

    for (i, t) in legacy.ei_targets.iter().enumerate() {
        let entity_id = by_source[i];
        let e = &v1.entities[entity_id as usize];
        // Ruling PF-2: character first, then name. Enemy players are
        // players and can carry a character name; every later task
        // resolves labels character-first.
        let label = e.character.as_deref().or(e.name.as_deref());
        assert_eq!(label, Some(t.name.as_str()), "target slot {i} mismatched");
    }
}

#[test]
fn positions_round_trip() {
    let (_enc, _metrics, _legacy, v1) = fixture_report();
    for (i, &id) in v1.source_order.players().iter().enumerate() {
        assert_eq!(v1.source_order.player_position(id), Some(i));
    }
    for (i, &id) in v1.source_order.targets().iter().enumerate() {
        assert_eq!(v1.source_order.target_position(id), Some(i));
    }
}

#[test]
fn source_order_is_not_serialized() {
    let (_enc, _metrics, _legacy, v1) = fixture_report();
    let doc = serde_json::to_value(&v1).unwrap();
    assert!(
        doc.get("source_order").is_none(),
        "source_order is a reprojection aid, never wire data"
    );
    // And it must not have leaked into any other key either.
    let text = serde_json::to_string(&doc).unwrap();
    assert!(!text.contains("source_order"));
}
