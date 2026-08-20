//! `blocks.self_effects` -- the squad-side condition and control-effect
//! block.
//!
//! A ONE-gate block, unlike `blocks.boons`: uptime and timelines are
//! produced by the same gated pass and arrive together, so
//! `coverage.self_effects` settles the whole question and `states` is not
//! optional. These tests pin exactly that, plus the two conventions every
//! block here shares (ids resolve through `catalogs.buffs`, and an absent
//! `avg_stacks` means "duration-stacking", never zero).

mod common;

use axilog_schema::v1::envelope::{BlockName, CoverageState};

#[test]
fn the_gate_being_off_reports_not_computed_and_omits_the_block() {
    let (_e, _m, _l, v1) = common::fixture_report_no_gates();
    assert_eq!(
        v1.coverage.get(BlockName::SelfEffects.as_str()),
        Some(CoverageState::NotComputed),
        "the pass did not run, which is not the same as running and finding nothing"
    );
    assert!(v1.blocks.self_effects.is_none(), "a not_computed block is omitted, never empty");
}

#[test]
fn the_gate_being_on_reports_present_with_rows() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    assert_eq!(v1.coverage.get(BlockName::SelfEffects.as_str()), Some(CoverageState::Present));
    let block = v1.blocks.self_effects.as_ref().expect("block is carried when computed");
    assert!(!block.by_entity.is_empty(), "the committed fixture has squad conditions");
}

/// The two ids this block exists for. Measured on the committed fixture
/// before this plan was written: Stun (872) reaches 3 squad players and
/// Daze (833) reaches 8. A pass that silently emitted nothing for the
/// control effects would still light up every condition lane, so this is
/// the assertion that actually guards the change.
#[test]
fn stun_and_daze_reach_squad_entities() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    let mut stun = 0usize;
    let mut daze = 0usize;
    for (_id, rows) in block.by_entity.iter() {
        stun += usize::from(rows.contains_key(&872));
        daze += usize::from(rows.contains_key(&833));
    }
    assert!(stun > 0, "no entity carries Stun (872)");
    assert!(daze > 0, "no entity carries Daze (833)");
}

/// No block carries a human-readable name; every id must resolve through
/// `catalogs.buffs`, which is what makes the ids joinable at all.
#[test]
fn every_emitted_buff_id_resolves_in_the_buff_catalog() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for id in rows.keys() {
            let entry = v1
                .catalogs
                .buffs
                .get(id)
                .unwrap_or_else(|| panic!("buff {id} on entity {entity} is not in the catalog"));
            assert!(!entry.name.is_empty(), "buff {id} resolves to an empty name");
        }
    }
}

/// `avg_stacks` follows the `BoonRow` convention: present for
/// intensity-stacking effects, OMITTED for duration ones rather than
/// carrying a meaningless zero.
#[test]
fn avg_stacks_is_present_exactly_for_intensity_effects() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    let mut saw_intensity = false;
    let mut saw_duration = false;
    for (_entity, rows) in block.by_entity.iter() {
        for (&id, row) in rows {
            let (is_intensity, _) =
                axilog_core::analysis::self_effects::effect_kind(id).expect("tracked id");
            assert_eq!(
                row.avg_stacks.is_some(),
                is_intensity,
                "buff {id}: avg_stacks presence must follow the stacking kind"
            );
            saw_intensity |= is_intensity;
            saw_duration |= !is_intensity;
        }
    }
    assert!(saw_intensity && saw_duration, "the fixture must exercise both branches");
}

/// `states` is NOT optional here, and every emitted timeline is a real one
/// -- the one-gate argument, asserted rather than asserted-in-prose.
///
/// "Real" means it reaches a nonzero stack count, not merely that it has
/// two entries: a zero-length buff application fuses into `[[0, 0], [t, 0]]`,
/// which has two entries and no information, and the core pass drops
/// exactly those rows.
#[test]
fn every_row_carries_a_nontrivial_timeline() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for (id, row) in rows {
            assert_eq!(
                row.states.first(),
                Some(&(0u64, 0u32)),
                "entity {entity} buff {id} must open with [0, 0]"
            );
            assert!(
                row.states.iter().any(|&(_, stacks)| stacks > 0),
                "entity {entity} buff {id} never leaves 0 stacks"
            );
        }
    }
}

/// Uptime is a percentage of the fight, so it cannot leave `[0, 100]`.
#[test]
fn uptime_percentages_stay_in_range() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for (id, row) in rows {
            assert!(
                (0.0..=100.0).contains(&row.uptime_pct),
                "entity {entity} buff {id}: uptime_pct {} out of range",
                row.uptime_pct
            );
        }
    }
}
