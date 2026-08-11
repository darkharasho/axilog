use super::ByEntity;
use crate::v1::entities::{EntityIndex, Role};
use serde::Serialize;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DefensesBlock {
    pub by_entity: ByEntity<DefensesEntity>,
}

/// Incoming defenses. Mirrors the legacy `DefensesOut` field-for-field --
/// this spec reshapes, it does not recompute. Every field below is copied
/// from the REAL `DefensesOut` in `crates/axilog-schema/src/lib.rs` (the
/// task-6 brief's own draft of this struct listed a different, smaller
/// field set that does not match; per the brief's own "real struct wins"
/// instruction this follows the real one instead). Names are kept
/// identical to the legacy struct -- none of them are EI abbreviations
/// needing a clearer 1.0 name, and matching names keeps the mapping
/// trivially auditable against `lib.rs`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DefensesEntity {
    pub blocked_count: u32,
    pub evaded_count: u32,
    pub dodge_count: u32,
    pub missed_count: u32,
    pub interrupted_count: u32,
    pub invulned_count: u32,
    pub strike_count: u32,
    pub strike_damage: u64,
    pub power_count: u32,
    pub power_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub barrier_count: u32,
    pub barrier_damage: u64,
    pub breakbar_count: u32,
    pub breakbar_damage: u64,
    /// Incoming crowd control -- see `DefenseStats::received_cc_count`.
    /// Deliberately lives here (not in the `cc` block) because it mirrors
    /// `DefensesOut`, which is where the legacy report carries it.
    pub received_cc_count: u32,
    pub received_cc_duration_ms: u64,
    /// Boons stripped OFF this player -- the incoming counterpart of
    /// `SupportOut::strips`.
    pub boon_strips_taken: u32,
    pub boon_strips_taken_duration_ms: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HitStatsBlock {
    pub by_entity: ByEntity<HitStatsEntity>,
}

/// Outgoing hit quality. Mirrors the legacy `HitStatsOut` field-for-field
/// (the real struct, not the brief's smaller sketch -- see `DefensesEntity`'s
/// doc comment for why). `above90_*` counts/damage are against a target at
/// or above 90% health.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HitStatsEntity {
    pub crit_count: u32,
    pub crit_damage: u64,
    pub flank_count: u32,
    pub glance_count: u32,
    pub moving_count: u32,
    pub connected_count: u32,
    pub connected_damage: u64,
    pub direct_count: u32,
    pub direct_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    pub critable_direct_count: u32,
    pub against_downed_count: u32,
    pub against_downed_damage: u64,
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub above90_power_count: u32,
    pub above90_power_damage: u64,
    pub above90_condition_count: u32,
    pub above90_condition_damage: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcBlock {
    pub squad: CcSquad,
    pub by_entity: ByEntity<CcEntity>,
}

/// Aggregates `Role::Squad` entities ONLY. `by_entity` below carries the
/// full friendly roster, including `Role::FriendlyPlayer`. Filtering here
/// rather than summing the roster keeps this correct once the upstream
/// non-squad-friendly split is populated -- same reasoning as
/// `damage::DamageSquad`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcSquad {
    pub applied_total: u32,
    pub applied_duration_ms: u64,
}

/// Mirrors the legacy `CcOut` field-for-field. Unlike the brief's draft,
/// this has no `received_total`/`received_duration_ms` -- the real `CcOut`
/// carries no incoming-CC fields at all; that data lives in `DefensesOut`
/// as `received_cc_count`/`received_cc_duration_ms` and is surfaced on
/// `DefensesEntity` above instead.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcEntity {
    pub applied_total: u32,
    pub applied_duration_ms: u64,
    pub stun_breaks: u32,
    pub removed_stun_duration_ms: u64,
}

pub fn build_defenses(report: &crate::Report, index: &EntityIndex) -> DefensesBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let d = &p.defenses;
        by_entity.insert(
            id,
            DefensesEntity {
                blocked_count: d.blocked_count,
                evaded_count: d.evaded_count,
                dodge_count: d.dodge_count,
                missed_count: d.missed_count,
                interrupted_count: d.interrupted_count,
                invulned_count: d.invulned_count,
                strike_count: d.strike_count,
                strike_damage: d.strike_damage,
                power_count: d.power_count,
                power_damage: d.power_damage,
                condition_count: d.condition_count,
                condition_damage: d.condition_damage,
                life_leech_count: d.life_leech_count,
                life_leech_damage: d.life_leech_damage,
                barrier_count: d.barrier_count,
                barrier_damage: d.barrier_damage,
                breakbar_count: d.breakbar_count,
                breakbar_damage: d.breakbar_damage,
                received_cc_count: d.received_cc_count,
                received_cc_duration_ms: d.received_cc_duration_ms,
                boon_strips_taken: d.boon_strips_taken,
                boon_strips_taken_duration_ms: d.boon_strips_taken_duration_ms,
            },
        );
    }
    DefensesBlock { by_entity }
}

pub fn build_hit_stats(report: &crate::Report, index: &EntityIndex) -> HitStatsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let h = &p.hit_stats;
        by_entity.insert(
            id,
            HitStatsEntity {
                crit_count: h.crit_count,
                crit_damage: h.crit_damage,
                flank_count: h.flank_count,
                glance_count: h.glance_count,
                moving_count: h.moving_count,
                connected_count: h.connected_count,
                connected_damage: h.connected_damage,
                direct_count: h.direct_count,
                direct_damage: h.direct_damage,
                condition_count: h.condition_count,
                condition_damage: h.condition_damage,
                critable_direct_count: h.critable_direct_count,
                against_downed_count: h.against_downed_count,
                against_downed_damage: h.against_downed_damage,
                life_leech_count: h.life_leech_count,
                life_leech_damage: h.life_leech_damage,
                above90_power_count: h.above90_power_count,
                above90_power_damage: h.above90_power_damage,
                above90_condition_count: h.above90_condition_count,
                above90_condition_damage: h.above90_condition_damage,
            },
        );
    }
    HitStatsBlock { by_entity }
}

pub fn build_cc(report: &crate::Report, index: &EntityIndex) -> CcBlock {
    let mut by_entity = ByEntity::default();
    let mut squad = CcSquad::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let c = &p.cc;
        // Squad aggregate excludes non-squad friendlies; `by_entity` does not.
        if index.role_of(id) == Some(Role::Squad) {
            squad.applied_total += c.applied_total;
            squad.applied_duration_ms += c.applied_duration_ms;
        }
        by_entity.insert(
            id,
            CcEntity {
                applied_total: c.applied_total,
                applied_duration_ms: c.applied_duration_ms,
                stun_breaks: c.stun_breaks,
                removed_stun_duration_ms: c.removed_stun_duration_ms,
            },
        );
    }
    CcBlock { squad, by_entity }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;

    #[test]
    fn every_squad_player_has_a_defenses_row_keyed_by_entity_id() {
        let (report, index) = fixture_report();
        let block = build_defenses(&report, &index);
        assert_eq!(
            block.by_entity.len(),
            report.players.len(),
            "one defenses row per player, no positional joins"
        );
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            assert!(block.by_entity.get(id).is_some(), "row for entity {id}");
        }
    }

    #[test]
    fn defenses_values_match_the_legacy_report_exactly() {
        // NOTE: the brief's own draft of this test used `row.blocked`/
        // `row.evaded`/`row.damage_taken`, but the real `DefensesOut` (see
        // `crates/axilog-schema/src/lib.rs`) has no such fields -- it's
        // `blocked_count`/`evaded_count`, and there is no unified
        // `damage_taken` at all (only the per-category breakdown:
        // `strike_damage`/`power_damage`/`condition_damage`/
        // `life_leech_damage`/`barrier_damage`; the unified incoming total
        // already exists as `PlayerOut::damage_taken`, surfaced by the
        // `damage` block's `DamageEntity::taken`). Adjusted to the real
        // field names per the brief's own "real struct wins" instruction.
        let (report, index) = fixture_report();
        let block = build_defenses(&report, &index);
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            let row = block.by_entity.get(id).expect("row");
            assert_eq!(row.blocked_count, p.defenses.blocked_count, "no number may change");
            assert_eq!(row.evaded_count, p.defenses.evaded_count);
            assert_eq!(row.strike_damage, p.defenses.strike_damage);
        }
    }

    #[test]
    fn cc_squad_totals_sum_only_the_squad_rows() {
        // On the committed fixture every player is in-squad, so the aggregate
        // equals the sum of all rows. This pins the arithmetic; the filter
        // itself is pinned by the non-fixture test below, because a
        // fixture-only test cannot tell a working filter from a missing one.
        let (report, index) = fixture_report();
        let block = build_cc(&report, &index);
        let summed: u32 = block.by_entity.0.values().map(|r| r.applied_total).sum();
        assert_eq!(block.squad.applied_total, summed, "the aggregate must be the sum of its parts");
    }

    #[test]
    fn cc_squad_aggregate_excludes_non_squad_friendlies() {
        // Hand-built, NOT the fixture: every fixture player is in-squad.
        // Give the two players DIFFERENT cc totals so an unfiltered sum is
        // distinguishable from a filtered one.
        let (mut report, index) = crate::v1::blocks::tests_support::two_player_report();
        report.players[0].cc.applied_total = 7; // in-squad
        report.players[1].cc.applied_total = 5; // non-squad friendly
        let block = build_cc(&report, &index);
        assert_eq!(block.by_entity.len(), 2, "the roster keeps both players");
        assert_eq!(block.squad.applied_total, 7, "only the in-squad player counts");
    }

    #[test]
    fn hit_stats_rows_carry_no_names() {
        let (report, index) = fixture_report();
        let block = build_hit_stats(&report, &index);
        let v = serde_json::to_value(&block).expect("serializable");
        let text = serde_json::to_string(&v).expect("stringify");
        assert!(!text.contains("\"name\""), "no block inlines a name");
    }
}
