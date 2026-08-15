use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::{EntityIndex, Role};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageBlock {
    pub squad: DamageSquad,
    pub by_entity: ByEntity<DamageEntity>,
}

impl DamageBlock {
    /// Whether this block carries nothing at all, which is what
    /// [`crate::v1::envelope::CoverageState::Empty`] reports. `squad` is a
    /// sum over the `by_entity` rows, so no rows implies a zero aggregate
    /// and there is nothing else to check.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Aggregates `Role::Squad` entities ONLY -- not every friendly player.
/// `DamageBlock::by_entity` is the full roster (squad AND non-squad
/// friendlies); this aggregate deliberately excludes non-squad friendlies
/// (`Role::FriendlyPlayer`) so its name stays true even once the upstream
/// `Player::in_squad` gap (currently hardcoded `true` for every friendly)
/// is filled and non-squad friendlies start actually appearing.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageSquad {
    pub total: u64,
    pub dps: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageEntity {
    pub total: u64,
    pub dps: f64,
    pub taken: u64,
    /// Enemy players this entity landed the DOWNING blow on -- the legacy
    /// `PlayerOut::downs_dealt`. An outgoing OUTCOME, so it lives here
    /// beside the damage that produced it, not on `defenses` (which carries
    /// the incoming mirror, `downs_taken`/`deaths`) -- the same split GW2EI
    /// makes, whose `defenses[0]` carries `downCount`/`deadCount` while the
    /// offensive per-target rows carry `downed`/`killed`. Always present:
    /// the legacy field is ungated.
    pub downs_dealt: u32,
    /// Enemy players this entity landed the KILLING blow on -- the legacy
    /// `PlayerOut::kills_dealt`. See `downs_dealt`.
    pub kills_dealt: u32,
    /// Breakbar damage this entity DEALT -- the legacy
    /// `PlayerOut::breakbar_damage_dealt`, which had no 1.0 destination
    /// and is the one `players[]` field the ei-json adapter could not read
    /// from this document (it feeds `dpsAll[0].breakbarDamage`).
    ///
    /// Here rather than on `defenses` for the reason `downs_dealt` gives
    /// above: this is outgoing. `DefensesEntity::breakbar_count`/
    /// `breakbar_damage` are its INCOMING mirror, and the legacy field's
    /// origin (`Metrics::defenses`) is where it was computed, not what it
    /// measures.
    pub breakbar_damage_dealt: u64,
    /// Keyed by the TARGET's entity id -- so it joins directly to that
    /// entity's own row. Sparse; omitted when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, PerTarget>,
    /// OUTGOING per-skill damage, keyed by skill id. Present only when the
    /// per-skill compute gate (`--skill-damage` / SDK `skill_damage: true`,
    /// `PlayerOut::skill_damage`) was on; omitted otherwise, since the
    /// legacy field itself is `Option` and absent when the gate is off.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill: BTreeMap<u32, SkillRow>,
    /// INCOMING per-skill damage, keyed by skill id -- the legacy
    /// `SkillDamageOut::taken`, which had no 1.0 destination at all before
    /// the final review. Named to mirror this row's own `total`/`taken`
    /// pair, so the outgoing/incoming split reads the same way at both
    /// levels. Same gate as `by_skill`; `sum(by_skill_taken[*].total) ==
    /// taken` holds by construction (see `SkillDamageOut`'s doc comment).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill_taken: BTreeMap<u32, SkillRow>,
}

/// One `(entity, target)` pair.
///
/// `total` is ungated (the legacy `DamageOut::per_enemy`). Everything else
/// here comes from the `--skill-damage`-gated families (`PlayerOut::
/// per_target` and `SkillDamageOut::per_target`), so a row can legitimately
/// carry `total` alone.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTarget {
    pub total: u64,
    /// The gated per-target offensive detail -- the legacy
    /// `PerTargetStatsOut`, which had no 1.0 destination before the final
    /// review.
    ///
    /// Grouped under ONE optional key rather than flattened onto
    /// `PerTarget`, deliberately: the seven fields below are computed only
    /// when `--skill-damage` is on, so flattening them would force this row
    /// to publish seven fabricated zeros whenever the gate is off --
    /// exactly the "absent reported as zero" ambiguity `coverage` exists to
    /// remove, one level down. One `Option` gives that gate a single,
    /// unambiguous presence signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<PerTargetDetail>,
    /// Per-`(entity, target, skill)` outgoing damage, keyed by skill id --
    /// the legacy `SkillDamageOut::per_target`, which had no 1.0
    /// destination before the final review. Same gate as
    /// `DamageEntity::by_skill`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill: BTreeMap<u32, SkillRow>,
}

/// Mirrors the legacy `crate::PerTargetStatsOut` field-for-field, minus
/// `enemy_id` -- that is the enclosing map's KEY here (as the target's
/// ENTITY id, joined through `EntityIndex::by_enemy_id`), not carried
/// redundantly inside the value. Same convention `series::TargetSeries`
/// already uses.
///
/// `downed`/`killed` are this pair's split of `DamageEntity::downs_dealt`/
/// `kills_dealt`; `interrupts` and `downs_contribution_damage` are NOT
/// recoverable from any other block, which is what made dropping this
/// struct a real data loss rather than a redundancy.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTargetDetail {
    pub connected_hits: u32,
    pub connected_damage: u64,
    pub against_downed_count: u32,
    pub downed: u32,
    pub killed: u32,
    pub interrupts: u32,
    /// arcdps-methodology down-contribution DAMAGE credited to this entity
    /// for downs of this specific target -- NOT GW2EI's own
    /// 90%-to-downstate-window algorithm. See `crate::PerTargetStatsOut`.
    pub downs_contribution_damage: u64,
}

/// Mirrors `crate::SkillEntryOut` field-for-field. `min`/`max` are `u64`
/// there (not `Option<u32>` as an earlier draft of this block assumed --
/// the legacy struct always populates them, no optionality to preserve),
/// so this row keeps them non-optional too rather than inventing an
/// `Option` the source data never has.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SkillRow {
    pub total: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    /// Hit COUNT (not a damage sum) of hits that crit, mirroring
    /// `crate::SkillEntryOut::crit_hits` -- see that field's doc comment
    /// for the derivation. Fix round 2, Finding: this was dropped entirely
    /// in the first pass of this block, with no stated reason -- a real
    /// equivalence gap, not a design choice.
    pub crit_hits: u32,
    /// Hit COUNT of hits landed while flanking, mirroring
    /// `crate::SkillEntryOut::flank_hits`. Same fix-round-2 correction as
    /// `crit_hits` above.
    pub flank_hits: u32,
}

/// One legacy `SkillEntryOut` as a [`SkillRow`]. Shared by the three
/// per-skill families this block carries (`by_skill`, `by_skill_taken`, and
/// each `PerTarget::by_skill`) so a field added to one cannot silently miss
/// the other two -- which is how `crit_hits`/`flank_hits` were dropped once
/// already.
fn skill_row(e: &crate::SkillEntryOut) -> SkillRow {
    SkillRow {
        total: e.total,
        hits: e.hits,
        min: e.min,
        max: e.max,
        crit_hits: e.crit_hits,
        flank_hits: e.flank_hits,
    }
}

pub fn build_damage(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> DamageBlock {
    let mut by_entity = ByEntity::default();
    let mut squad_total = 0u64;
    let mut squad_dps = 0f64;

    for p in &report.players {
        let Some(entity_id) = index.by_agent_addr(p.agent_addr) else {
            continue;
        };
        if index.role_of(entity_id) == Some(Role::Squad) {
            squad_total += p.damage.total;
            squad_dps += p.damage.dps;
        }

        // The three per-target families are UNIONED, not zipped: their
        // enemy-id sets genuinely differ. `PlayerOut::per_target` is itself
        // already the union of the per-target offense map and the
        // per-target down-contribution map (see `build_report`), so an
        // enemy can appear there with no `per_enemy` damage row at all --
        // a credit inside that enemy's down window without a landed hit
        // over the whole fight. Keying one map by entity id and filling it
        // from each source in turn keeps every row reachable.
        let mut per_target: BTreeMap<u32, PerTarget> = BTreeMap::new();
        for pe in &p.damage.per_enemy {
            let Some(tid) = index.by_enemy_id(pe.enemy_id) else { continue };
            per_target.entry(tid).or_default().total = pe.total;
        }
        if let Some(stats) = p.per_target.as_ref() {
            for s in stats {
                let Some(tid) = index.by_enemy_id(s.enemy_id) else { continue };
                per_target.entry(tid).or_default().detail = Some(PerTargetDetail {
                    connected_hits: s.connected_hits,
                    connected_damage: s.connected_damage,
                    against_downed_count: s.against_downed_count,
                    downed: s.downed,
                    killed: s.killed,
                    interrupts: s.interrupts,
                    downs_contribution_damage: s.downs_contribution_damage,
                });
            }
        }

        let mut by_skill = BTreeMap::new();
        let mut by_skill_taken = BTreeMap::new();
        if let Some(skill_damage) = p.skill_damage.as_ref() {
            for e in &skill_damage.outgoing {
                cats.reference_skill(e.skill_id);
                by_skill.insert(e.skill_id, skill_row(e));
            }
            for e in &skill_damage.taken {
                cats.reference_skill(e.skill_id);
                by_skill_taken.insert(e.skill_id, skill_row(e));
            }
            for t in &skill_damage.per_target {
                let Some(tid) = index.by_enemy_id(t.enemy_id) else { continue };
                let row = per_target.entry(tid).or_default();
                for e in &t.skills {
                    cats.reference_skill(e.skill_id);
                    row.by_skill.insert(e.skill_id, skill_row(e));
                }
            }
        }

        by_entity.insert(
            entity_id,
            DamageEntity {
                total: p.damage.total,
                dps: p.damage.dps,
                taken: p.damage_taken,
                downs_dealt: p.downs_dealt,
                kills_dealt: p.kills_dealt,
                breakbar_damage_dealt: p.breakbar_damage_dealt,
                per_target,
                by_skill,
                by_skill_taken,
            },
        );
    }

    DamageBlock { squad: DamageSquad { total: squad_total, dps: squad_dps }, by_entity }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_per_target_by_entity_id_not_by_array_position() {
        // The identity/statistics split's payoff: an enemy player's own row
        // and the damage dealt TO them are the same integer. Impossible in
        // the legacy shape, where enemy statistics were `#[serde(skip)]`.
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);

        // agent_addr 4575 / enemy_id 9588 are real ids from the committed
        // `wvw-small.anon.zevtc` fixture (the brief's literal `1`/`9` do not
        // occur in it -- verified by an ad hoc probe over the whole fixture;
        // this player's `damage.per_enemy` genuinely contains this enemy).
        let squad_entity = index.by_agent_addr(4575).expect("squad player resolves");
        let enemy_entity = index.by_enemy_id(9588).expect("enemy resolves");

        let row = block.by_entity.get(squad_entity).expect("squad player has a damage row");
        assert!(
            row.per_target.contains_key(&enemy_entity),
            "per_target is keyed by ENTITY id, so it joins to the enemy's own row"
        );
    }

    #[test]
    fn squad_total_matches_the_legacy_report() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);
        let expected: u64 = report.players.iter().map(|p| p.damage.total).sum();
        assert_eq!(block.squad.total, expected, "no number may change in this spec");
    }

    #[test]
    fn skill_rows_reference_ids_and_register_them_in_the_catalog() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);

        // Same real fixture id as the test above (see its comment).
        let squad_entity = index.by_agent_addr(4575).expect("squad player resolves");
        let row = block.by_entity.get(squad_entity).expect("row");
        for skill_id in row.by_skill.keys() {
            // No name anywhere in the block -- names live in catalogs only.
            let v = serde_json::to_value(&row.by_skill[skill_id]).expect("serializable");
            assert!(v.get("name").is_none(), "a block must never inline a skill name");
        }

        let built = cats.finish(&Default::default(), None);
        for skill_id in row.by_skill.keys() {
            assert!(
                built.skills.contains_key(skill_id),
                "every referenced skill id must resolve in the catalog"
            );
        }
    }

    #[test]
    fn squad_aggregate_excludes_non_squad_friendlies_while_by_entity_keeps_them() {
        // Fix round 1 finding: `squad.total`/`squad.dps` must sum ONLY
        // `Role::Squad` entities, never every friendly player. This is
        // deliberately NOT built on the committed fixture -- every player
        // in it is in-squad, so a fixture-based test cannot distinguish a
        // working filter from a missing one (that's precisely how this
        // defect survived review of the passing suite). Uses the shared
        // two-player helper (one in-squad, one a non-squad friendly) rather
        // than a local hand-built roster, so Tasks 6-8 don't each redefine
        // the same fixture (`tests_support::two_player_report`).
        let (mut report, index) = crate::v1::blocks::tests_support::two_player_report();
        report.players[0].damage.total = 500;
        report.players[0].damage.dps = 50.0;
        report.players[1].damage.total = 300;
        report.players[1].damage.dps = 30.0;
        let mut cats = CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);

        assert_eq!(block.by_entity.len(), 2, "by_entity is the full roster: squad AND non-squad friendlies");
        assert_eq!(block.squad.total, 500, "squad.total must be the in-squad player's damage only, not the sum of both");
        assert_eq!(block.squad.dps, 50.0, "squad.dps must likewise exclude the non-squad friendly");
    }

    #[test]
    fn an_empty_block_serializes_as_an_empty_map_not_null() {
        let block = DamageBlock::default();
        let v = serde_json::to_value(&block).expect("serializable");
        assert_eq!(v["by_entity"], serde_json::json!({}));
    }
}
