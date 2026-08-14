//! Reconstructs Elite Insights' positional joins from the native 1.0
//! document.
//!
//! EI's shape is positional everywhere native's is id-keyed: `players[]`
//! and `targets[]` are arrays whose INDEX is the identity, and
//! `dpsTargets`/`statsTargets`/`targetDamageDist` are indexed by position
//! within `targets[]`. Native has no positions at all -- it has
//! `entities[]` sorted for legibility and blocks keyed by entity id.
//!
//! Every order reconstruction in this crate goes through here. That is a
//! deliberate constraint, not a convenience: ordering is the single
//! highest-risk part of the adapter re-point (a wrong order diffs every
//! golden at once), so it gets exactly one implementation to audit rather
//! than one per block.

use axilog_schema::v1::{EntityOut, ReportV1};

pub struct EiJoin<'a> {
    report: &'a ReportV1,
}

impl<'a> EiJoin<'a> {
    pub fn new(report: &'a ReportV1) -> Self {
        Self { report }
    }

    /// `(ei_index, entity_id, entity)` in EI `players[]` order.
    pub fn players(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)> + '_ {
        let report = self.report;
        report
            .source_order
            .players()
            .iter()
            .enumerate()
            .filter_map(move |(i, &id)| report.entities.get(id as usize).map(|e| (i, id, e)))
    }

    /// `(ei_index, entity_id, entity)` in EI `targets[]` order.
    pub fn targets(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)> + '_ {
        let report = self.report;
        report
            .source_order
            .targets()
            .iter()
            .enumerate()
            .filter_map(move |(i, &id)| report.entities.get(id as usize).map(|e| (i, id, e)))
    }

    /// This entity's index in EI `targets[]`, for the arrays keyed by it.
    pub fn target_slot(&self, entity_id: u32) -> Option<usize> {
        self.report.source_order.target_position(entity_id)
    }

    pub fn entity(&self, entity_id: u32) -> Option<&'a EntityOut> {
        self.report.entities.get(entity_id as usize)
    }

    /// The label EI uses for this entity: a player's character name, an
    /// NPC's name, or `""` when neither is recorded.
    ///
    /// This is the inverse of the native shape's source-entity-id keying
    /// (see the boon-state and condition blocks). Native keys timelines by
    /// id precisely so two players sharing a character name cannot
    /// collide; EI's own shape cannot express that, so the collision
    /// reappears on THIS side of the boundary, exactly where EI already
    /// had it. That is faithful reprojection, not a regression.
    pub fn display_name(&self, entity_id: u32) -> &'a str {
        self.entity(entity_id)
            .and_then(|e| e.character.as_deref().or(e.name.as_deref()))
            .unwrap_or("")
    }
}
