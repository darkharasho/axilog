//! `blocks.squad_buffs` -- squad-side uptime for every buff that is
//! neither a boon nor a condition/control effect: sigils, relics, food,
//! utilities, auras, signets, trait buffs.
//!
//! The third and last piece of the split Elite Insights keeps in one
//! `buffUptimes` array. `blocks.boons` owns the 12 boons,
//! `blocks.self_effects` owns the 16 conditions and control effects, and
//! this owns the long tail -- the population axibridge's Special Buffs and
//! Sigil/Relic Uptime sections read, both of which rendered empty for as
//! long as no block carried it.
//!
//! ## A partition, not an overlap
//!
//! The EI adapter CONCATENATES this block's rows onto `blocks.boons`' to
//! rebuild EI's single array, and that array has one entry per id. The
//! three id sets are therefore disjoint by construction --
//! [`axilog_core::analysis::squad_buffs::is_squad_buff`] subtracts the
//! other two -- and `v1_squad_buffs.rs` asserts the disjointness on a real
//! fixture rather than trusting the predicate.
//!
//! ## Always-on, and no `states`
//!
//! Unlike its two siblings this block is not gated. It emits uptime only,
//! which is the cost `blocks.boons`' always-on uptime half already
//! carries; the timelines are what make the other two expensive, and
//! nothing plots a sigil's stack count over time. See
//! [`axilog_core::analysis::squad_buffs`]' module doc for the full
//! argument. Adding `states` later is additive; shipping it unused and
//! removing it would be a break.

use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use axilog_core::analysis::squad_buffs::SquadBuffs;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SquadBuffsBlock {
    /// squad entity id -> buff id -> row. Two levels of real ids, like
    /// every other block -- the buff id resolves through `catalogs.buffs`.
    pub by_entity: ByEntity<BTreeMap<u32, SquadBuffRow>>,
}

impl SquadBuffsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SquadBuffRow {
    /// Percent of the fight this entity held at least one stack.
    pub uptime_pct: f64,
    /// Time-weighted mean stack count -- present for intensity-stacking
    /// buffs, omitted for duration ones rather than reported as a
    /// meaningless zero. The same convention
    /// [`super::support::BoonRow::avg_stacks`] and
    /// [`super::self_effects::SelfEffectRow::avg_stacks`] follow, for the
    /// same reason: Elite Insights never populates it for a duration buff,
    /// and the EI adapter reads the presence of this field to choose which
    /// of its two `buffData[0]` spellings to emit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
}

/// Reproject the pass onto entity ids.
///
/// The pass keys `(player representative addr, buff id)`; the address joins
/// through [`EntityIndex::by_agent_addr`]. A player whose representative
/// address resolves to no entity is skipped rather than given a fabricated
/// id -- the same rule [`super::self_effects::build_self_effects`] and
/// [`super::support::build_boons`] apply to the same join.
///
/// The intensity question is asked of `axilog-core` rather than re-derived
/// here, so the omission rule for `avg_stacks` cannot drift from the stack
/// type the pass actually simulated with.
pub fn build_squad_buffs(
    buffs: &SquadBuffs,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> SquadBuffsBlock {
    let mut by_entity: BTreeMap<u32, BTreeMap<u32, SquadBuffRow>> = BTreeMap::new();
    for (&(addr, buff_id), uptime) in &buffs.uptime {
        let Some(entity_id) = index.by_agent_addr(addr) else { continue };
        let (is_intensity, _) = axilog_core::analysis::buffs::stacking(buff_id);
        cats.reference_buff(buff_id);
        by_entity.entry(entity_id).or_default().insert(
            buff_id,
            SquadBuffRow {
                uptime_pct: uptime.presence_pct,
                avg_stacks: is_intensity.then_some(uptime.avg_stacks),
            },
        );
    }
    SquadBuffsBlock { by_entity: ByEntity(by_entity) }
}
