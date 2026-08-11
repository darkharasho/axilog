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
    /// Keyed by the TARGET's entity id -- so it joins directly to that
    /// entity's own row. Sparse; omitted when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, PerTarget>,
    /// Keyed by skill id. Present only when the per-skill compute gate
    /// (`--skill-damage` / SDK `skill_damage: true`, `PlayerOut::
    /// skill_damage`) was on; omitted otherwise, since the legacy field
    /// itself is `Option` and absent when the gate is off.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill: BTreeMap<u32, SkillRow>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTarget {
    pub total: u64,
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

        let per_target = p
            .damage
            .per_enemy
            .iter()
            .filter_map(|pe| {
                index
                    .by_enemy_id(pe.enemy_id)
                    .map(|tid| (tid, PerTarget { total: pe.total }))
            })
            .collect();

        let mut by_skill = BTreeMap::new();
        if let Some(skill_damage) = p.skill_damage.as_ref() {
            for e in &skill_damage.outgoing {
                cats.reference_skill(e.skill_id);
                by_skill.insert(
                    e.skill_id,
                    SkillRow { total: e.total, hits: e.hits, min: e.min, max: e.max },
                );
            }
        }

        by_entity.insert(
            entity_id,
            DamageEntity {
                total: p.damage.total,
                dps: p.damage.dps,
                taken: p.damage_taken,
                per_target,
                by_skill,
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
