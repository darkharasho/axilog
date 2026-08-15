//! Enemy per-skill damage on `blocks.damage` -- the surface side-channel
//! absorption Task 7 moved onto the native wire.
//!
//! The point of the task is that an enemy is an entity like any other, so
//! its skill breakdown lands in the SAME `by_entity[].by_skill` map the
//! player rows use rather than in a parallel enemy structure. These tests
//! pin that, and pin the one place the two kinds of row genuinely differ:
//! which hit-count field they carry.

mod common;
use common::{fixture_report_all_gates, fixture_report_no_gates};
use axilog_schema::v1::entities::Role;

/// The headline invariant: enemy skill rows are on the shared map.
#[test]
fn enemy_damage_rows_land_on_the_same_block_as_players() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let damage = v1.blocks.damage.as_ref().expect("damage is always built");

    let enemy_with_skills = v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Npc | Role::EnemyPlayer))
        .filter_map(|e| damage.by_entity.get(e.id))
        .filter(|row| !row.by_skill.as_ref().map_or(true, |m| m.is_empty()))
        .count();
    assert!(enemy_with_skills > 0, "enemy skill distributions did not land");

    let player_with_skills = v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Squad | Role::FriendlyPlayer))
        .filter_map(|e| damage.by_entity.get(e.id))
        .filter(|row| !row.by_skill.as_ref().map_or(true, |m| m.is_empty()))
        .count();
    assert!(
        player_with_skills > 0,
        "premise: player rows use this same map, so the test above is not \
         passing on an enemy-only structure"
    );
}

/// `hits` and `connected_hits` count different things and come from
/// different passes, so a row carries exactly one of them. Collapsing them
/// into a single field would make `total / hits` mean two different
/// averages depending on the row's role -- with no way for a consumer to
/// tell which it got.
#[test]
fn each_skill_row_carries_the_hit_count_its_pass_actually_measured() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let damage = v1.blocks.damage.as_ref().expect("damage is always built");

    let mut enemy_rows = 0;
    let mut player_rows = 0;
    for entity in &v1.entities {
        let Some(row) = damage.by_entity.get(entity.id) else { continue };
        let is_enemy = matches!(entity.role, Role::Npc | Role::EnemyPlayer);
        for (skill_id, skill) in row.by_skill.iter().flatten() {
            if is_enemy {
                assert!(
                    skill.hits.is_none(),
                    "enemy {} skill {skill_id}: the enemy pass never measures the \
                     contributing-row count, so it must not publish one",
                    entity.id
                );
                assert!(
                    skill.connected_hits.is_some(),
                    "enemy {} skill {skill_id} lost its connected-hit count",
                    entity.id
                );
                enemy_rows += 1;
            } else {
                assert!(
                    skill.hits.is_some(),
                    "player {} skill {skill_id} lost its hit count",
                    entity.id
                );
                player_rows += 1;
            }
        }
    }
    assert!(enemy_rows > 0 && player_rows > 0, "premise: both kinds of row exist");
}

/// Every skill id an enemy row references must resolve in the catalog --
/// the same invariant Task 6's minion rows carry, and the same reason
/// ei-json's `skillMap` grows when a block is absorbed.
#[test]
fn every_enemy_skill_resolves_in_the_catalog() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let damage = v1.blocks.damage.as_ref().expect("damage is always built");
    for entity in v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Npc | Role::EnemyPlayer))
    {
        let Some(row) = damage.by_entity.get(entity.id) else { continue };
        for skill_id in row.by_skill.iter().flatten().map(|(id, _)| id) {
            assert!(
                v1.catalogs.skills.contains_key(skill_id),
                "enemy skill {skill_id} is a dangling reference"
            );
        }
    }
}

/// The gate. An enemy row still exists without `--skill-damage` (it carries
/// `total`/`dps`, which are ungated) -- what disappears is the breakdown.
#[test]
fn enemy_skill_rows_are_absent_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_gates();
    let damage = v1.blocks.damage.as_ref().expect("damage is always built");
    let mut enemy_rows = 0;
    for entity in v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Npc | Role::EnemyPlayer))
    {
        let Some(row) = damage.by_entity.get(entity.id) else { continue };
        assert!(row.by_skill.as_ref().map_or(true, |m| m.is_empty()), "no --skill-damage means no enemy breakdown");
        enemy_rows += 1;
    }
    assert!(enemy_rows > 0, "premise: enemy rows survive the flagless build");
}

/// The pass is keyed by representative agent id and the block by entity id,
/// so the join is where a silent loss would happen. Check it against the
/// pass directly rather than trusting the row count.
#[test]
fn enemy_rows_are_rekeyed_from_agent_id_to_entity_id() {
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
    let pass = axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &rep);

    let damage = v1.blocks.damage.as_ref().expect("damage is always built");
    // Every entity whose `agent_addr` is a pass key must carry that key's
    // rows. `agent_addr` IS the enemy's representative id for an enemy
    // entity, which is exactly what the pass keys on.
    let mut joined = 0;
    for entity in &v1.entities {
        let Some(skills) = pass.get(&entity.agent_addr) else { continue };
        let row = damage
            .by_entity
            .get(entity.id)
            .unwrap_or_else(|| panic!("enemy {} lost its damage row entirely", entity.id));
        assert_eq!(
            row.by_skill.as_ref().map_or(0, |m| m.len()),
            skills.len(),
            "enemy {} lost skill rows in the rekey",
            entity.id
        );
        for s in skills {
            let got = row
                .by_skill
                .as_ref()
                .and_then(|m| m.get(&s.skill_id))
                .unwrap_or_else(|| panic!("enemy {} skill {} missing", entity.id, s.skill_id));
            assert_eq!(got.total, s.total, "no number may change in this spec");
            assert_eq!(got.connected_hits, Some(s.hits));
            assert_eq!(got.min, s.min);
            assert_eq!(got.max, s.max);
            assert_eq!(got.crit_hits, s.crit_hits);
            assert_eq!(got.flank_hits, s.flank_hits);
            joined += 1;
        }
    }
    assert!(joined > 0, "premise: the committed fixture has enemy skill rows");
}
