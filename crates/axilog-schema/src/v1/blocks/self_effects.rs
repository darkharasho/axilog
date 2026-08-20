//! `blocks.self_effects` -- what was on a SQUAD player: the 14 conditions
//! plus Stun and Daze, with uptime and a fused stack timeline each.
//!
//! The squad-side counterpart to `blocks.conditions` (which is enemy-side)
//! and the missing half of `blocks.boons` (which is squad-side but covers
//! only the 12 boons). `blocks.cc` is not a substitute: it counts
//! crowd-control EVENTS, a different measurement that carries no timeline.
//!
//! ## One gate, unlike `blocks.boons`
//!
//! `blocks.boons` is a two-gate block because its uptime half is computed
//! on every parse by `build_boons` while `attach_boon_states` only enriches
//! existing rows -- so `coverage.boons` answers the uptime question and
//! says nothing about the timelines. Here, uptime and states come out of
//! one gated pass and arrive together, so `coverage.self_effects` answers
//! the whole question and [`SelfEffectRow::states`] is not an `Option`.
//!
//! ## No `per_source`
//!
//! The machinery could produce it and "which enemy chained that stun" is a
//! real question, but nothing asks it today and it roughly doubles the
//! block. Additive to add later; a breaking removal if it ships unused.

use super::{ByEntity, StateTimeline};
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use axilog_core::analysis::self_effects::{effect_kind, SelfEffects};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SelfEffectsBlock {
    /// squad entity id -> buff id -> row. Two levels of real ids, like
    /// every other block -- the buff id resolves through `catalogs.buffs`.
    pub by_entity: ByEntity<BTreeMap<u32, SelfEffectRow>>,
}

impl SelfEffectsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SelfEffectRow {
    /// Percent of the fight this entity held at least one stack.
    pub uptime_pct: f64,
    /// Time-weighted mean stack count -- present for intensity-stacking
    /// effects (the 6 `BuffStackType.Stacking` conditions,
    /// `CommonBuffs.cs:36-40` + `:49`), omitted for duration ones
    /// rather than reported as a meaningless zero. The same convention
    /// [`super::support::BoonRow::avg_stacks`] follows, for the same
    /// reason: Elite Insights never populates it for a duration buff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    /// The fused stack timeline. Unconditional, per the one-gate argument
    /// in this module's doc: if the block is here at all, the pass ran.
    /// Duration effects are clamped to 0/1 upstream so the graph means what
    /// Elite Insights' means; the intensity ones carry their real count.
    pub states: StateTimeline,
}

/// Reproject the pass onto entity ids.
///
/// The pass keys `(player representative addr, buff id)`; the address joins
/// through [`EntityIndex::by_agent_addr`]. A player whose representative
/// address resolves to no entity is skipped rather than given a fabricated
/// id -- the same rule [`super::support::build_boons`] applies to the same
/// join.
///
/// `uptime` and `states` are written by the pass under identical keys, so a
/// missing uptime is a contract violation rather than a normal absence;
/// this walks `states` and skips a key with no uptime rather than emitting
/// a row with a fabricated zero.
pub fn build_self_effects(
    effects: &SelfEffects,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> SelfEffectsBlock {
    let mut by_entity: BTreeMap<u32, BTreeMap<u32, SelfEffectRow>> = BTreeMap::new();
    for (&(addr, buff_id), timeline) in &effects.states {
        let Some(entity_id) = index.by_agent_addr(addr) else { continue };
        let Some(uptime) = effects.uptime.get(&(addr, buff_id)) else { continue };
        // The same lookup the pass itself used -- asked of `axilog-core`
        // rather than re-derived here, so the omission rule for
        // `avg_stacks` cannot drift from the clamping rule for `states`.
        let Some((is_intensity, _)) = effect_kind(buff_id) else { continue };
        cats.reference_buff(buff_id);
        by_entity.entry(entity_id).or_default().insert(
            buff_id,
            SelfEffectRow {
                uptime_pct: uptime.presence_pct,
                avg_stacks: is_intensity.then_some(uptime.avg_stacks),
                states: timeline.clone(),
            },
        );
    }
    SelfEffectsBlock { by_entity: ByEntity(by_entity) }
}
