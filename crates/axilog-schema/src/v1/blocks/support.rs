use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use serde::Serialize;
use std::collections::BTreeMap;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonsBlock {
    /// entity id -> buff id -> row. Two levels of real ids, no positional
    /// joins: the legacy shape was a `Vec` in `buffs::BOON_IDS` order, which
    /// a consumer could only read by knowing that table.
    pub by_entity: ByEntity<BTreeMap<u32, BoonRow>>,
}

impl BoonsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::BoonOut` field-for-field, minus `id` (it's the map key)
/// and `name` (no human-readable name may appear in a block -- a consumer
/// resolves the id through the buff catalog instead).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonRow {
    pub uptime_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    /// `crate::BoonOut::generation` is NOT optional in the legacy shape, so
    /// this stays unconditional too rather than inventing an `Option` the
    /// source data never has.
    pub generation: GenerationRow,
}

/// Mirrors `crate::GenerationOut` field-for-field, including the three
/// WASTED fields (rounded to 3 decimals upstream, omitted when exactly
/// zero -- same convention the legacy struct documents for the same
/// reason: the common case wastes nothing, and omission is real
/// information, not loss).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct GenerationRow {
    pub self_pct: f64,
    pub group_pct: f64,
    pub squad_pct: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub self_wasted: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub group_wasted: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub squad_wasted: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportBlock {
    pub by_entity: ByEntity<SupportEntity>,
}

impl SupportBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::SupportOut` field-for-field, including
/// `strips_duration_ms` (the brief's sketch dropped it).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportEntity {
    pub cleanses: u32,
    pub cleanses_self: u32,
    pub strips: u32,
    pub strips_duration_ms: u64,
    pub resurrects: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionBlock {
    pub by_entity: ByEntity<ContributionEntity>,
}

impl ContributionBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Both directions of the arcdps-methodology down contribution (M11).
/// GW2EI has no equivalent surface -- this follows arcdps itself.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionEntity {
    pub downs_contribution: ContributionRow,
    pub downed_by: ContributionRow,
}

/// Mirrors `crate::ContributionOut` field-for-field. `movement_impairing`
/// is `u64` there (not `u32` as an earlier draft of this block assumed),
/// so this row keeps it `u64` too.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionRow {
    pub damage: u64,
    pub cc: u32,
    pub strips: u32,
    pub movement_impairing: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingBlock {
    pub by_entity: ByEntity<HealingEntity>,
}

impl HealingBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::HealingOut` field-for-field. Field names/shape differ
/// substantially from the brief's sketch (`outgoing`/`outgoing_barrier`/
/// `downed_healing`) -- the real struct is `healing_out_total`/
/// `healing_out_allies`/`healing_out_self`/`barrier_out`/
/// `downed_healing_out`; all five are carried across.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingEntity {
    pub outgoing_total: u64,
    pub outgoing_allies: u64,
    pub outgoing_self: u64,
    pub barrier_out: u64,
    pub downed_healing_out: u64,
}

pub fn build_boons(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> BoonsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let mut rows = BTreeMap::new();
        for b in &p.boons {
            cats.reference_buff(b.id);
            rows.insert(
                b.id,
                BoonRow {
                    uptime_pct: b.presence_pct,
                    avg_stacks: b.avg_stacks,
                    generation: GenerationRow {
                        self_pct: b.generation.self_pct,
                        group_pct: b.generation.group_pct,
                        squad_pct: b.generation.squad_pct,
                        self_wasted: b.generation.self_wasted,
                        group_wasted: b.generation.group_wasted,
                        squad_wasted: b.generation.squad_wasted,
                    },
                },
            );
        }
        by_entity.insert(id, rows);
    }
    BoonsBlock { by_entity }
}

pub fn build_support(report: &crate::Report, index: &EntityIndex) -> SupportBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            SupportEntity {
                cleanses: p.support.cleanses,
                cleanses_self: p.support.cleanses_self,
                strips: p.support.strips,
                strips_duration_ms: p.support.strips_duration_ms,
                resurrects: p.support.resurrects,
            },
        );
    }
    SupportBlock { by_entity }
}

pub fn build_contribution(report: &crate::Report, index: &EntityIndex) -> ContributionBlock {
    let row = |c: &crate::ContributionOut| ContributionRow {
        damage: c.damage,
        cc: c.cc,
        strips: c.strips,
        movement_impairing: c.movement_impairing,
    };
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            ContributionEntity {
                downs_contribution: row(&p.downs_contribution),
                downed_by: row(&p.downed_by),
            },
        );
    }
    ContributionBlock { by_entity }
}

/// `crate::PlayerOut::healing` is `Option<HealingOut>` -- `None` when the
/// log carries no healing-extension data at all
/// (`Metrics::has_healing_extension == false`), a real "no data" signal,
/// not "genuinely all zero". A player without healing data gets no row
/// here, same "absent, not null/zero" convention the legacy field itself
/// uses.
pub fn build_healing(report: &crate::Report, index: &EntityIndex) -> HealingBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(h) = p.healing.as_ref() else { continue };
        by_entity.insert(
            id,
            HealingEntity {
                outgoing_total: h.healing_out_total,
                outgoing_allies: h.healing_out_allies,
                outgoing_self: h.healing_out_self,
                barrier_out: h.barrier_out,
                downed_healing_out: h.downed_healing_out,
            },
        );
    }
    HealingBlock { by_entity }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;
    use crate::v1::catalogs::CatalogBuilder;

    #[test]
    fn boons_are_keyed_by_buff_id_not_by_position_in_a_fixed_array() {
        // The legacy shape is `Vec<BoonOut>` in `buffs::BOON_IDS` order --
        // a positional join a consumer must know the table to read.
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);

        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("boon row");
        assert!(!row.is_empty(), "player carries per-boon rows");
        for buff_id in row.keys() {
            assert!(*buff_id > 0, "keys are real buff ids");
        }
    }

    #[test]
    fn every_referenced_buff_id_resolves_in_the_catalog() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        let built = cats.finish(&Default::default(), None);
        for row in block.by_entity.0.values() {
            for buff_id in row.keys() {
                assert!(built.buffs.contains_key(buff_id), "buff {buff_id} must resolve");
            }
        }
    }

    #[test]
    fn boon_uptime_matches_the_legacy_report_exactly() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            let row = block.by_entity.get(id).expect("row");
            for legacy in &p.boons {
                let got = row.get(&legacy.id).expect("boon present");
                assert_eq!(got.uptime_pct, legacy.presence_pct, "no number may change");
            }
        }
    }

    #[test]
    fn contribution_carries_both_directions() {
        let (report, index) = fixture_report();
        let block = build_contribution(&report, &index);
        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("row");
        assert_eq!(row.downs_contribution.damage, p.downs_contribution.damage);
        assert_eq!(row.downed_by.damage, p.downed_by.damage);
    }
}
