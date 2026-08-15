//! Enemy outgoing series on `blocks.series` -- the surface side-channel
//! absorption Task 8 moved onto the native wire.
//!
//! Same headline as Task 7's enemy damage rows: an enemy is an entity like
//! any other, so its per-second series land in the SAME `by_entity` map the
//! players use. Two things differ, and both are pinned below: the outgoing
//! power split lives in a field only the enemy pass populates, and -- unlike
//! Task 7 -- the GATE absorbed here too, because every enemy is given a row
//! so an absent one can only mean the flag was off.

mod common;
use common::{fixture_report_all_gates, fixture_report_no_gates};
use axilog_schema::v1::entities::Role;

fn is_enemy(role: Role) -> bool {
    matches!(role, Role::Npc | Role::EnemyPlayer)
}

/// The headline invariant: enemy series are on the shared map.
#[test]
fn enemy_series_rows_land_on_the_same_block_as_players() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let series = v1.blocks.series.as_ref().expect("series is always built");

    let enemies = v1
        .entities
        .iter()
        .filter(|e| is_enemy(e.role))
        .filter(|e| series.by_entity.get(e.id).is_some())
        .count();
    assert!(enemies > 0, "enemy series did not land");

    let players = v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Squad | Role::FriendlyPlayer))
        .filter(|e| series.by_entity.get(e.id).is_some())
        .count();
    assert!(
        players > 0,
        "premise: player rows use this same map, so the assertion above is \
         not passing on an enemy-only structure"
    );
}

/// Every `Enemy` gets a row, not just the ones that dealt damage, and every
/// row is the same grid length. This is the zero-fill that used to live in
/// the ei-json adapter; moving it here is what lets an absent row mean
/// "gate off" rather than "this enemy dealt nothing".
///
/// The roster is `report.enemies`, NOT every enemy-ROLE entity: the entity
/// list is the broader one (80 enemy-role entities against 49 `Enemy`
/// records on the committed fixture -- the extra rows are minions and
/// gadgets promoted to entities), and no pass produces a series for an
/// entity with no `Enemy` record. Zero-filling those would invent a
/// measurement rather than carry one. `blocks.damage` draws the same line
/// for the same reason.
#[test]
fn every_enemy_gets_a_full_length_row_even_if_it_never_dealt_damage() {
    let (_enc, _metrics, legacy, v1) = fixture_report_all_gates();
    let series = v1.blocks.series.as_ref().expect("series is always built");
    let enemy_ids: std::collections::BTreeSet<u64> = legacy.enemies.iter().map(|e| e.id).collect();

    let mut lengths = std::collections::BTreeSet::new();
    let mut all_zero = 0;
    let mut enemies = 0;
    for e in v1.entities.iter().filter(|e| is_enemy(e.role)) {
        if !enemy_ids.contains(&e.agent_addr) {
            assert!(
                series.by_entity.get(e.id).is_none(),
                "entity {} has no `Enemy` record, so nothing can have measured a \
                 series for it -- a row here would be invented",
                e.id
            );
            continue;
        }
        let row = series
            .by_entity
            .get(e.id)
            .unwrap_or_else(|| panic!("enemy {} has no series row", e.id));
        let damage = row.damage.decode_u64();
        lengths.insert(damage.len());
        if damage.iter().all(|&v| v == 0) {
            all_zero += 1;
        }
        enemies += 1;
    }
    assert!(enemies > 0, "premise: the fixture has enemies");
    assert_eq!(
        lengths.len(),
        1,
        "every enemy series must be the one grid length ei_grid computes, got {lengths:?}"
    );
    assert!(
        all_zero > 0,
        "premise: at least one enemy dealt nothing, so the zero-fill path is \
         actually exercised rather than assumed"
    );
}

/// The adapter reads `targets[].damage1S` straight off this block and takes
/// an absent row to mean the gate was off. That inference is only sound if
/// every RENDERED target has a row under the gate -- which is a stronger
/// claim than the roster test above, because `source_order.targets()` is
/// its own curated list. Pin it directly rather than reasoning from
/// `report.enemies` being a superset.
#[test]
fn every_rendered_target_has_a_row_so_absent_can_only_mean_gate_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let series = v1.blocks.series.as_ref().expect("series is always built");
    let targets = v1.source_order.targets();
    assert!(!targets.is_empty(), "premise: the fixture renders targets");
    for &id in targets {
        assert!(
            series.by_entity.get(id).is_some(),
            "target entity {id} has no series row, so the adapter would omit \
             damage1S for it and call that a closed gate"
        );
    }
}

/// The mirror of the roster test: nothing the pass measured may be dropped
/// on the way in. A pass key with no entity would vanish silently, which is
/// exactly the failure Task 7 found on the damage block.
#[test]
fn no_pass_row_is_dropped_for_want_of_an_entity() {
    let (enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enemies: std::collections::BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let rep: std::collections::BTreeMap<u64, u64> = enc
        .enemies
        .iter()
        .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
        .collect();
    let pass = axilog_core::analysis::timeseries::build_enemy_series(
        &enc,
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enemies,
        &rep,
    );
    assert!(!pass.is_empty(), "premise: the pass produced rows");

    let series = v1.blocks.series.as_ref().expect("series is always built");
    let by_addr: std::collections::BTreeMap<u64, u32> =
        v1.entities.iter().map(|e| (e.agent_addr, e.id)).collect();
    for enemy_id in pass.keys() {
        let entity = by_addr
            .get(enemy_id)
            .unwrap_or_else(|| panic!("pass key {enemy_id} resolves to no entity at all"));
        assert!(
            series.by_entity.get(*entity).is_some(),
            "pass key {enemy_id} resolved to entity {entity} but landed nowhere"
        );
    }
}

/// The outgoing power split is measured only for enemies, so only enemy
/// rows carry it. A player row must OMIT it rather than publish a zero
/// series, which would claim "measured, and it was all condition damage".
#[test]
fn only_enemy_rows_carry_the_outgoing_power_split() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let series = v1.blocks.series.as_ref().expect("series is always built");

    let mut enemy_rows = 0;
    let mut player_rows = 0;
    for e in &v1.entities {
        let Some(row) = series.by_entity.get(e.id) else { continue };
        if is_enemy(e.role) {
            let power = row
                .power_damage
                .as_ref()
                .unwrap_or_else(|| panic!("enemy {} lost its outgoing power series", e.id));
            assert_eq!(
                power.decode_u64().len(),
                row.damage.decode_u64().len(),
                "enemy {}: the power split must be on the same grid as the total",
                e.id
            );
            enemy_rows += 1;
        } else {
            assert!(
                row.power_damage.is_none(),
                "player {}: no pass measures outgoing power damage, so the \
                 field must be absent rather than zero",
                e.id
            );
            player_rows += 1;
        }
    }
    assert!(enemy_rows > 0 && player_rows > 0, "premise: both kinds of row exist");
}

/// The gate, and the reason it could absorb here when Task 7's could not:
/// with every enemy filled, "no row" is unambiguous.
#[test]
fn enemy_rows_are_absent_entirely_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_gates();
    let series = v1.blocks.series.as_ref().expect("series is always built");
    let mut enemies = 0;
    for e in v1.entities.iter().filter(|e| is_enemy(e.role)) {
        assert!(
            series.by_entity.get(e.id).is_none(),
            "enemy {} has a series row without --timeseries",
            e.id
        );
        enemies += 1;
    }
    assert!(enemies > 0, "premise: the flagless build still has enemy entities");
}

/// The pass is keyed by representative agent id and the block by entity id,
/// so the join is where a silent loss would happen. Check every value
/// against the pass directly, THROUGH the envelope -- an encode/decode bug
/// would otherwise hide behind a row that merely exists.
#[test]
fn enemy_rows_are_rekeyed_and_survive_the_series_envelope_intact() {
    let (enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enemies: std::collections::BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let rep: std::collections::BTreeMap<u64, u64> = enc
        .enemies
        .iter()
        .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
        .collect();
    let pass = axilog_core::analysis::timeseries::build_enemy_series(
        &enc,
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enemies,
        &rep,
    );

    let series = v1.blocks.series.as_ref().expect("series is always built");
    let mut joined = 0;
    for entity in &v1.entities {
        // `agent_addr` IS the enemy's representative id for an enemy
        // entity, which is exactly what the pass keys on.
        let Some(want) = pass.get(&entity.agent_addr) else { continue };
        let row = series
            .by_entity
            .get(entity.id)
            .unwrap_or_else(|| panic!("enemy {} lost its series row entirely", entity.id));
        assert_eq!(
            row.damage.decode_u64(),
            want.damage,
            "enemy {}: no number may change in this spec",
            entity.id
        );
        assert_eq!(
            row.power_damage.as_ref().map(|s| s.decode_u64()),
            Some(want.power_damage.clone()),
            "enemy {}: the power split changed crossing the envelope",
            entity.id
        );
        joined += 1;
    }
    assert!(joined > 0, "premise: the committed fixture has enemies that dealt damage");
}
