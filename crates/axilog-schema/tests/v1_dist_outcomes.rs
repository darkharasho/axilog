//! Side-channel absorption Task 9: `dist_outcomes` -> the outcome columns
//! on `blocks.damage.by_entity[].by_skill`/`by_skill_taken`.
//!
//! Task 9 is the first absorption that does not add a ROW SET of its own.
//! It merges a second pass into skill rows another pass already built, and
//! the two passes disagree about which skills exist -- so the interesting
//! failures are joins, not shapes: a row silently dropped because the
//! merge intersected instead of unioning, a counter landing in the wrong
//! field because three different hit counts share one concept, an enemy
//! row picking up columns from a pass that never looked at enemies.
//!
//! These tests pin each of those. `every_outcome_row_joins_an_existing_
//! skill_row`, the assertion the plan sketched, is deliberately NOT among
//! them -- see `the_row_set_is_a_union_not_an_intersection` for why it is
//! false on real data.

mod common;

use axilog_schema::v1::entities::Role;

fn is_enemy(role: Role) -> bool {
    matches!(role, Role::Npc | Role::EnemyPlayer)
}

/// The columns land on the SAME rows the distributions already occupy --
/// not a parallel `outcomes` map keyed by skill id beside them.
///
/// This is the whole point of the task: a consumer reading a skill row
/// gets its damage and its outcome breakdown from one lookup, and cannot
/// hold a `total` from one pass beside a `blocked` from a row that no
/// longer exists.
#[test]
fn outcome_columns_land_on_the_rows_the_distributions_already_use() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");

    let mut annotated = 0usize;
    for e in v1.entities.iter().filter(|e| !is_enemy(e.role)) {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        if row.by_skill.as_ref().map_or(true, |m| m.is_empty()) {
            continue;
        }
        for (skill_id, skill) in row.by_skill.iter().flatten() {
            assert!(
                skill.outcomes.is_some(),
                "entity {} skill {skill_id}: a player row in a gated-on document has no outcome \
                 columns, so the merge missed it",
                e.id
            );
            assert!(
                skill.connected_hits.is_some(),
                "entity {} skill {skill_id}: outcomes present but connected_hits absent -- the \
                 two are filled by one pass and must not be able to disagree",
                e.id
            );
            annotated += 1;
        }
    }
    assert!(annotated >= 380, "expected the full by-skill matrix, annotated only {annotated}");
}

/// **The plan's proposed assertion is false, and this is the test that
/// says so.**
///
/// The sketch asked for `every_outcome_row_joins_an_existing_skill_row`.
/// It cannot hold: `skill_damage` accumulates only CONTRIBUTING (`dmg >
/// 0`) rows, so a skill whose every attempt was blocked never reaches it,
/// while `dist_outcomes` counts exactly those rows -- and GW2EI emits them
/// (`totalDamage: 0, hits: n`). Those pure-mitigation rows are the reason
/// the outcome pass exists; asserting them away would have quietly
/// intersected the merge and deleted the payload.
///
/// So the invariant is the opposite one: the union is REAL (at least one
/// such row exists on the committed fixture, or this test is not
/// exercising anything), and every extra row looks like exactly what the
/// outcome pass alone can produce -- no damage, no contributing hits.
#[test]
fn the_row_set_is_a_union_not_an_intersection() {
    let (_enc, _metrics, legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_addr: std::collections::BTreeMap<u64, &axilog_schema::PlayerOut> =
        legacy.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut mitigation_only = 0usize;
    for e in v1.entities.iter().filter(|e| !is_enemy(e.role)) {
        let (Some(row), Some(p)) = (damage.by_entity.get(e.id), by_addr.get(&e.agent_addr)) else {
            continue;
        };
        let Some(sd) = p.skill_damage.as_ref() else { continue };
        let contributing: std::collections::BTreeSet<u32> =
            sd.outgoing.iter().map(|s| s.skill_id).collect();
        for (skill_id, skill) in row.by_skill.iter().flatten() {
            if contributing.contains(skill_id) {
                continue;
            }
            assert_eq!(
                skill.total, 0,
                "entity {} skill {skill_id}: absent from the damage pass yet carries damage",
                e.id
            );
            assert_eq!(
                skill.hits,
                Some(0),
                "entity {} skill {skill_id}: a mitigation-only row must report ZERO contributing \
                 hits, not an absent count -- absence from the damage pass is a measurement",
                e.id
            );
            assert!(
                skill.outcomes.is_some(),
                "entity {} skill {skill_id}: nothing but the outcome pass could have created \
                 this row, so it must carry outcome columns",
                e.id
            );
            mitigation_only += 1;
        }
    }
    assert!(
        mitigation_only > 0,
        "no mitigation-only rows on the committed fixture -- either the union stopped happening \
         or this fixture can no longer prove it does"
    );
}

/// The counters mean three different things and must not be interchanged.
///
/// `hits` counts CONTRIBUTING rows (`dmg > 0`), `connected_hits` counts
/// `HasHit` rows (a superset -- a connecting hit can deal zero), and
/// `attempt_hits` counts every non-marker row (a superset of THAT -- it
/// includes blocked/evaded/missed). The ordering below is the cheapest
/// statement of that nesting, and it is what fails if a future refactor
/// folds two of the three into one field.
#[test]
fn the_three_hit_counts_nest_in_the_documented_order() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");

    let mut checked = 0usize;
    for e in v1.entities.iter().filter(|e| !is_enemy(e.role)) {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        for (label, rows) in
            [("by_skill", &row.by_skill), ("by_skill_taken", &row.by_skill_taken)]
        {
            for (skill_id, skill) in rows.iter().flatten() {
                let (Some(o), Some(connected), Some(hits)) =
                    (skill.outcomes.as_ref(), skill.connected_hits, skill.hits)
                else {
                    continue;
                };
                assert!(
                    hits <= connected,
                    "entity {} {label}[{skill_id}]: contributing hits ({hits}) exceed connecting \
                     hits ({connected})",
                    e.id
                );
                assert!(
                    connected <= o.attempt_hits,
                    "entity {} {label}[{skill_id}]: connecting hits ({connected}) exceed attempts \
                     ({})",
                    e.id,
                    o.attempt_hits
                );
                // `interrupted` is deliberately NOT in this sum. GW2EI
                // excludes its `NoDamageHealthDamageEvent` markers
                // (Interrupt/KillingBlow/Downed) from the attempt count
                // while still counting the interrupt as an OUTCOME, so an
                // interrupt-heavy skill legitimately reports more
                // interrupts than attempts -- measured on the committed
                // fixture: skill 77357 has 2 attempts and 3 interrupts.
                // The first draft of this test asserted the tidier
                // invariant and caught only its own wrong assumption.
                let mitigated = o.missed + o.evaded + o.blocked + o.invulned;
                assert!(
                    connected + mitigated <= o.attempt_hits,
                    "entity {} {label}[{skill_id}]: connecting ({connected}) + mitigated \
                     ({mitigated}) exceed attempts ({})",
                    e.id,
                    o.attempt_hits
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no annotated rows to check");
}

/// GW2EI zeroes `glance`/`missed`/`evaded`/`blocked`/`interrupted` inside
/// its `if (!IndirectDamage)` guard, and `dist_outcomes` reproduces that in
/// a post-pass. `invulned` is deliberately OUTSIDE the guard on both sides
/// -- a condition tick can land on an invulnerable target -- so this pins
/// the five, not six, that the guard covers.
#[test]
fn a_condition_skill_reports_no_direct_hit_outcomes() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");

    let mut indirect_rows = 0usize;
    for e in v1.entities.iter().filter(|e| !is_enemy(e.role)) {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        for (skill_id, skill) in row.by_skill.iter().flatten().chain(row.by_skill_taken.iter().flatten()) {
            let Some(o) = skill.outcomes.as_ref().filter(|o| o.indirect) else { continue };
            assert_eq!(
                (o.glance, o.missed, o.evaded, o.blocked, o.interrupted),
                (0, 0, 0, 0, 0),
                "entity {} skill {skill_id}: a condition skill reported direct-hit outcomes",
                e.id
            );
            indirect_rows += 1;
        }
    }
    assert!(indirect_rows > 0, "no condition skills on the fixture -- the guard is untested");
}

/// The outcome pass runs over the FRIENDLY side only, so an enemy row must
/// carry no outcome columns at all.
///
/// A zero-filled `outcomes` here would be the exact mistake Task 8's
/// zero-fill rule exists to bound: absence means "not measured", and these
/// genuinely are not. Enemy rows keep the `connected_hits` their own pass
/// (Task 7) fills, which is why that field is checked separately -- the
/// two fields have different presence rules on this side.
#[test]
fn enemy_rows_carry_no_outcome_columns() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");

    let mut enemy_rows = 0usize;
    for e in v1.entities.iter().filter(|e| is_enemy(e.role)) {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        for (skill_id, skill) in row.by_skill.iter().flatten() {
            assert!(
                skill.outcomes.is_none(),
                "entity {} skill {skill_id}: an enemy row picked up outcome columns from a pass \
                 that never measured enemies",
                e.id
            );
            assert!(
                skill.hits.is_none(),
                "entity {} skill {skill_id}: enemy rows have no contributing count",
                e.id
            );
            enemy_rows += 1;
        }
    }
    assert!(enemy_rows > 0, "no enemy skill rows on the fixture");
}

/// A skill that only ever appears in the outcome pass still has to be in
/// the skill catalog. `merge_outcomes` calls `reference_skill` for every
/// row it touches, including the ones it creates -- without that, the
/// union's own new rows would be dangling ids in the rendered document.
#[test]
fn every_annotated_skill_id_is_catalogued() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");

    for e in &v1.entities {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        for (skill_id, _) in row.by_skill.iter().flatten().chain(row.by_skill_taken.iter().flatten()) {
            assert!(
                v1.catalogs.skills.contains_key(skill_id),
                "entity {} references skill {skill_id}, which has no catalog entry",
                e.id
            );
        }
    }
}

/// Gate off (`--skill-damage` absent) means the columns are absent, along
/// with the rows that would have carried them.
///
/// Task 9 needs no gate record of its own -- unlike Task 7's, this gate is
/// answered by the same `by_skill` presence the distributions already use,
/// because both ride one flag. This test is what pins that equivalence:
/// if the outcome merge ever ran on a different condition than the
/// distributions, one of these two assertions would fire.
#[test]
fn outcome_columns_are_absent_entirely_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_no_gates();
    let Some(damage) = v1.blocks.damage.as_ref() else { return };

    for e in &v1.entities {
        let Some(row) = damage.by_entity.get(e.id) else { continue };
        assert!(
            row.by_skill.as_ref().map_or(true, |m| m.is_empty()) && row.by_skill_taken.as_ref().map_or(true, |m| m.is_empty()),
            "entity {}: skill rows exist with the gate off",
            e.id
        );
    }
}
