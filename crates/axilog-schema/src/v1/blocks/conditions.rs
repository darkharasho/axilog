//! `blocks.conditions` -- per-enemy condition stack timelines, split by the
//! squad member who applied them.
//!
//! The block name was reserved (and reported `not_computed`) from spec #1
//! onward; Task 12 fills it. It is the enemy-side counterpart to the
//! `states`/`per_source` fields on `blocks.boons`' rows, and carries the
//! same [`PerSourceStates`] shape -- minus a fused total, which the source
//! pass does not compute for enemies.
//!
//! ## Why the appliers are narrowed to the squad
//!
//! `axilog_core::analysis::target_conditions` emits only squad-sourced
//! conditions -- an enemy-applied condition on another enemy is real, but
//! has no consumer and is pure payload. That narrowing is inherited here
//! rather than re-decided, so [`PerSourceStates::unresolved`] is expected to
//! be absent on every row in this block: a squad applier that failed to
//! resolve to an entity would be a roster bug, not a normal log.

use super::{ByEntity, PerSourceStates};
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use axilog_core::analysis::target_conditions::TargetConditionStates;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ConditionsBlock {
    /// enemy entity id -> condition buff id -> row. Two levels of real ids,
    /// like every other block -- the condition id resolves through
    /// `catalogs.buffs`.
    pub by_entity: ByEntity<BTreeMap<u32, ConditionRow>>,
}

impl ConditionsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ConditionRow {
    /// Who applied this condition to this enemy, and when it was up.
    ///
    /// There is no sibling `states` total here, unlike `blocks.boons`: the
    /// enemy-side pass computes only the source split, and summing the
    /// sources would NOT reconstruct a fused total (two appliers holding
    /// the same duration condition overlap rather than stack).
    pub per_source: PerSourceStates,
}

/// Reproject the source pass onto entity ids.
///
/// The pass keys `(enemy representative id, condition id) -> applier addr`;
/// the enemy id joins through [`EntityIndex::by_enemy_id`] and the applier
/// address through [`EntityIndex::by_agent_addr`]. Two applier addresses can
/// resolve to the SAME entity (a relogged player's addresses all fold onto
/// their one roster entry), so appliers merge rather than overwrite --
/// dropping the second would silently lose a relogged player's second
/// session.
pub fn build_conditions(
    states: &TargetConditionStates,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> ConditionsBlock {
    let mut by_entity: BTreeMap<u32, BTreeMap<u32, ConditionRow>> = BTreeMap::new();
    for (&(enemy_id, condition_id), per_source) in states {
        let Some(entity_id) = index.by_enemy_id(enemy_id) else { continue };
        let mut row = PerSourceStates::default();
        for (&source_addr, timeline) in per_source {
            super::support::merge_source_timeline(
                &mut row,
                index.by_agent_addr(source_addr),
                timeline,
            );
        }
        if row.is_empty() {
            continue;
        }
        cats.reference_buff(condition_id);
        by_entity.entry(entity_id).or_default().insert(condition_id, ConditionRow {
            per_source: row,
        });
    }
    ConditionsBlock { by_entity: ByEntity(by_entity) }
}
