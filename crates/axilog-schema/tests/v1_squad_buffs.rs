//! `blocks.squad_buffs` -- squad-side uptime for every buff that is
//! neither one of the 12 boons nor a condition/control effect: sigils,
//! relics, food, auras, signets, trait buffs.
//!
//! An ALWAYS-ON block, unlike `blocks.self_effects` and
//! `blocks.conditions`: it emits uptime only, which is the cost
//! `blocks.boons`' always-on half already carries (see
//! `axilog_core::analysis::squad_buffs`' module doc). `Passes::squad_buffs`
//! being `None` therefore means a caller with no log to scan, the same
//! sense `Passes::activity` carries, not a gate that was off.
//!
//! The load-bearing property here is the PARTITION: the EI adapter
//! concatenates this block's rows onto `blocks.boons`' into one
//! `buffUptimes` array, and Elite Insights' array has one entry per id. An
//! id in both blocks would appear twice.

mod common;

use axilog_schema::v1::envelope::{BlockName, CoverageState};
use std::collections::BTreeSet;

#[test]
fn the_block_is_present_with_rows_on_the_committed_fixture() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    assert_eq!(v1.coverage.get(BlockName::SquadBuffs.as_str()), Some(CoverageState::Present));
    let block = v1.blocks.squad_buffs.as_ref().expect("block is carried when computed");
    assert!(!block.by_entity.is_empty(), "the committed fixture's squad carries sigils and food");
}

/// Always-on: the block is there with every OTHER gate off, because
/// nothing gates it. A regression that quietly attached it to
/// `--timeseries` would empty axibridge's Special Buffs and Sigil/Relic
/// sections again for every default parse -- the exact symptom this block
/// was added to fix.
#[test]
fn no_gate_turns_the_block_off() {
    let (_e, _m, _l, v1) = common::fixture_report_no_gates();
    assert_eq!(
        v1.coverage.get(BlockName::SquadBuffs.as_str()),
        Some(CoverageState::Present),
        "squad buffs are always-on; no compute gate may suppress them"
    );
    assert!(v1.blocks.squad_buffs.is_some());
}

/// A caller that supplies no pass at all still gets an honest answer --
/// `not_computed`, not an empty block reported as a real zero.
#[test]
fn a_caller_that_ran_no_pass_reports_not_computed() {
    let (enc, metrics, legacy, _v1) = common::fixture_report_all_gates();
    let bare = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes::default(),
    );
    assert_eq!(
        bare.coverage.get(BlockName::SquadBuffs.as_str()),
        Some(CoverageState::NotComputed)
    );
    assert!(bare.blocks.squad_buffs.is_none(), "a not_computed block is omitted, never empty");
}

/// The partition the EI adapter's concatenation depends on.
#[test]
fn no_id_appears_in_both_this_block_and_blocks_boons() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.squad_buffs.as_ref().expect("block");
    let boons = v1.blocks.boons.as_ref().expect("boons block");
    for (entity, rows) in block.by_entity.iter() {
        let Some(boon_rows) = boons.by_entity.get(entity) else { continue };
        let overlap: Vec<u32> =
            rows.keys().filter(|id| boon_rows.contains_key(id)).copied().collect();
        assert!(
            overlap.is_empty(),
            "entity {entity} carries {overlap:?} in BOTH blocks; \
             the EI adapter would emit each twice"
        );
    }
}

/// The other half of the same partition.
#[test]
fn no_id_appears_in_both_this_block_and_blocks_self_effects() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.squad_buffs.as_ref().expect("block");
    let effects = v1.blocks.self_effects.as_ref().expect("self_effects block");
    for (entity, rows) in block.by_entity.iter() {
        let Some(effect_rows) = effects.by_entity.get(entity) else { continue };
        let overlap: Vec<u32> =
            rows.keys().filter(|id| effect_rows.contains_key(id)).copied().collect();
        assert!(overlap.is_empty(), "entity {entity} carries {overlap:?} in BOTH blocks");
    }
}

/// No block carries a human-readable name; every id must resolve through
/// `catalogs.buffs`, which is what makes the ids joinable at all -- and
/// what axibridge's `buffMap` lookup silently drops a row for when it
/// misses.
#[test]
fn every_emitted_buff_id_resolves_in_the_buff_catalog() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.squad_buffs.as_ref().expect("block");
    let mut seen = BTreeSet::new();
    for (entity, rows) in block.by_entity.iter() {
        for id in rows.keys() {
            assert!(
                v1.catalogs.buffs.contains_key(id),
                "buff {id} on entity {entity} is not in catalogs.buffs"
            );
            seen.insert(*id);
        }
    }
    assert!(seen.len() > 5, "a vacuous catalog check: only {} ids emitted", seen.len());
}

/// `avg_stacks` is present exactly for intensity-stacking buffs and
/// omitted -- never zero -- for duration ones, the convention
/// `blocks.boons` and `blocks.self_effects` both follow.
#[test]
fn avg_stacks_is_present_exactly_for_intensity_buffs() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.squad_buffs.as_ref().expect("block");
    let mut intensity_seen = 0;
    let mut duration_seen = 0;
    for (_entity, rows) in block.by_entity.iter() {
        for (&id, row) in rows {
            let (is_intensity, _) = axilog_core::analysis::buffs::stacking(id);
            assert_eq!(
                row.avg_stacks.is_some(),
                is_intensity,
                "buff {id}: avg_stacks presence must track the stack type"
            );
            if is_intensity {
                intensity_seen += 1;
            } else {
                duration_seen += 1;
            }
        }
    }
    assert!(intensity_seen > 0 && duration_seen > 0, "both branches must be exercised");
}

/// Every row's entity id must name a real `entities[]` row -- the join
/// invariant every block shares.
#[test]
fn every_entity_key_names_a_real_entity() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.squad_buffs.as_ref().expect("block");
    let ids: BTreeSet<u32> = v1.entities.iter().map(|e| e.id).collect();
    for (entity, _rows) in block.by_entity.iter() {
        assert!(ids.contains(&entity), "entity {entity} is not in entities[]");
    }
}
