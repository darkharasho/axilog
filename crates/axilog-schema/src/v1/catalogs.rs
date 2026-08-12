use axilog_core::analysis::buffs;
use axilog_core::analysis::condition_catalog;
use axilog_core::analysis::damage_mods::DamageModifierResults;
use axilog_core::analysis::Metrics;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Definition metadata for every id any block references.
///
/// The rule that makes this pay: no human-readable name appears outside
/// `catalogs` or `entities`. Every block references integers, so a skill
/// name appears once per document instead of once per player per target per
/// distribution row.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct Catalogs {
    pub skills: BTreeMap<u32, SkillEntry>,
    pub buffs: BTreeMap<u32, BuffEntry>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub damage_mods: BTreeMap<i32, DamageModEntry>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SkillEntry {
    pub name: String,
    pub is_swap: bool,
    pub can_crit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_attack: Option<bool>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct BuffEntry {
    pub name: String,
    /// `"condition"` (any of the 14 tracked conditions, damaging or not --
    /// e.g. Chilled/Taunt are conditions with no damage component),
    /// `"boon"` (any of the 12 tracked boons), or `"effect"` (everything
    /// else this catalog tracks stacking for -- auras, forms, and other
    /// non-boon non-condition buffs, e.g. Frost Aura/Death Shroud). arcdps
    /// does not distinguish these structurally; this field carries GW2's
    /// real three-way taxonomy so one catalog can serve all of them.
    pub kind: &'static str,
    /// `"intensity"` or `"duration"`.
    pub stacking: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stacks: Option<u32>,
}

/// Mirrors GW2EI's `damageModMap` entry fields
/// (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs`): `non_multiplier`,
/// `skill_based` and `approximate` are independent booleans there, not a
/// single classification, because a modifier can be BOTH skill-based AND a
/// multiplier at once -- folding them into one label (as an earlier draft
/// of this schema did) silently erases whichever axis lost the tie.
///
/// The four booleans are `Option<bool>` and omitted together, rather than
/// `false`, for ids with no known definition: absence is the honest signal
/// for "we have no metadata for this id", not "we checked and it's false".
///
/// The `damage_mods` map key's SIGN encodes direction: negative ids are
/// incoming modifiers, positive ids outgoing. This is existing behaviour
/// carried over unchanged; `DamageModifierMeta::incoming` is redundant with
/// it and is not exposed as a separate field here.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DamageModEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_multiplier: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_counter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_based: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate: Option<bool>,
}

/// Accumulates referenced ids as blocks emit them, then materializes
/// exactly that subset. This is what makes "every catalog entry is
/// referenced" true by construction.
#[derive(Debug, Default, Clone)]
pub struct CatalogBuilder {
    skills: BTreeSet<u32>,
    buffs: BTreeSet<u32>,
    damage_mods: BTreeSet<i32>,
}

impl CatalogBuilder {
    pub fn reference_skill(&mut self, id: u32) {
        self.skills.insert(id);
    }
    pub fn reference_buff(&mut self, id: u32) {
        self.buffs.insert(id);
    }
    pub fn reference_damage_mod(&mut self, id: i32) {
        self.damage_mods.insert(id);
    }

    pub fn finish(self, metrics: &Metrics, mods: Option<&DamageModifierResults>) -> Catalogs {
        let skills = self
            .skills
            .into_iter()
            .map(|id| {
                let entry = metrics.skill_map.get(&id);
                (
                    id,
                    SkillEntry {
                        // A referenced id ALWAYS resolves, even when the log
                        // table never named it -- a dangling reference would
                        // break the invariant the integrity test asserts.
                        name: entry
                            .map(|e| e.name.clone())
                            .unwrap_or_else(|| format!("Skill {id}")),
                        is_swap: entry.map(|e| e.is_swap).unwrap_or(false),
                        can_crit: entry.map(|e| e.can_crit).unwrap_or(true),
                        auto_attack: entry.and_then(|e| e.auto_attack),
                    },
                )
            })
            .collect();

        let buffs = self
            .buffs
            .into_iter()
            .map(|id| {
                // `stacking` resolves `(is_intensity, max_stacks)` in the
                // order Finding 1 (review round 1) specifies: the condition
                // table first (the only source with correct capacities for
                // all 14 conditions), then the boon/damage-mod stack-type
                // table, else duration/no-capacity.
                let (is_intensity, max_stacks) = buffs::stacking(id);
                // GW2's real three-way taxonomy (Finding 2, review round
                // 1): condition membership is checked directly against
                // `CONDITION_BUFFS`, NOT `is_condition_damage_based` --
                // that predicate answers "does this deal condition damage",
                // which would silently exclude the 8 non-damaging
                // conditions (Blind, Crippled, Chilled, Immobile, Weakness,
                // Fear, Slow, Taunt) from `kind: "condition"`.
                let is_condition = condition_catalog::CONDITION_BUFFS
                    .iter()
                    .any(|&(cid, _, _, _)| cid == id);
                let is_boon = buffs::BOON_IDS.iter().any(|&(bid, _, _)| bid == id);
                let kind =
                    if is_condition { "condition" } else if is_boon { "boon" } else { "effect" };
                (
                    id,
                    BuffEntry {
                        name: buffs::name(id).unwrap_or_default().to_string(),
                        kind,
                        stacking: if is_intensity { "intensity" } else { "duration" },
                        max_stacks,
                    },
                )
            })
            .collect();

        let damage_mods = match mods {
            None => BTreeMap::new(),
            Some(m) => self
                .damage_mods
                .into_iter()
                .map(|id| match m.meta.get(&id) {
                    Some(meta) => (
                        id,
                        DamageModEntry {
                            name: meta.name.to_string(),
                            description: Some(meta.description.clone()),
                            non_multiplier: Some(meta.non_multiplier),
                            is_counter: Some(meta.is_counter),
                            skill_based: Some(meta.skill_based),
                            approximate: Some(meta.approximate),
                        },
                    ),
                    // A referenced id ALWAYS resolves (Finding 3, review
                    // round 1) -- GW2EI's own `damageModMap` is built from
                    // inside the same emission loop that writes the rows,
                    // so a dangling reference is unrepresentable there.
                    // Mirrors the skills path's `"Skill {id}"` placeholder.
                    // The booleans/description are omitted rather than
                    // defaulted -- there is no metadata to report, and
                    // `false`/`""` would assert something false.
                    None => (
                        id,
                        DamageModEntry {
                            name: format!("Damage modifier {id}"),
                            description: None,
                            non_multiplier: None,
                            is_counter: None,
                            skill_based: None,
                            approximate: None,
                        },
                    ),
                })
                .collect(),
        };

        Catalogs { skills, buffs, damage_mods }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::analysis::Metrics;

    fn metrics_with_skills() -> Metrics {
        let mut m = Metrics::default();
        m.skill_map.insert(
            5491,
            axilog_core::analysis::skill_map::SkillMapEntry {
                name: "Symbol of Protection".into(),
                auto_attack: None,
                is_swap: false,
                can_crit: true,
            },
        );
        m.skill_map.insert(
            9999,
            axilog_core::analysis::skill_map::SkillMapEntry {
                name: "Never Referenced".into(),
                auto_attack: None,
                is_swap: false,
                can_crit: true,
            },
        );
        m
    }

    #[test]
    fn a_catalog_holds_only_referenced_ids() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        assert!(c.skills.contains_key(&5491), "referenced id must resolve");
        assert!(!c.skills.contains_key(&9999), "an unreferenced definition must not appear");
    }

    #[test]
    fn referencing_the_same_id_twice_yields_one_entry() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.skills.len(), 1);
    }

    #[test]
    fn skill_keys_serialize_as_bare_decimal_strings_without_an_ei_prefix() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        let v = serde_json::to_value(&c).expect("serializable");
        assert!(v["skills"].get("5491").is_some(), "keys are bare decimal ids");
        assert!(v["skills"].get("s5491").is_none(), "no EI 's' prefix");
        assert_eq!(v["skills"]["5491"]["name"], "Symbol of Protection");
        assert_eq!(v["skills"]["5491"]["can_crit"], true);
    }

    #[test]
    fn a_referenced_id_with_no_definition_still_resolves_to_an_entry() {
        // The referential-integrity invariant is "every referenced id
        // resolves". A skill the log table never named must therefore still
        // produce an entry -- with an honest placeholder name -- rather than
        // a dangling reference.
        let mut b = CatalogBuilder::default();
        b.reference_skill(424242);
        let c = b.finish(&metrics_with_skills(), None);
        let e = c.skills.get(&424242).expect("referenced id must resolve");
        assert_eq!(e.name, "Skill 424242");
    }

    #[test]
    fn buffs_carry_the_stacking_metadata_the_legacy_shape_had_nowhere_to_put() {
        let mut b = CatalogBuilder::default();
        b.reference_buff(740); // Might
        b.reference_buff(717); // Protection
        let c = b.finish(&Metrics::default(), None);

        let might = c.buffs.get(&740).expect("Might resolves");
        assert_eq!(might.name, "Might");
        assert_eq!(might.stacking, "intensity");
        assert_eq!(might.kind, "boon");

        let prot = c.buffs.get(&717).expect("Protection resolves");
        assert_eq!(prot.stacking, "duration");
    }

    #[test]
    fn conditions_are_kind_condition_regardless_of_whether_they_damage() {
        // Regression lock for review round 1's Finding 1 (wrong source
        // table for stacking/max_stacks) and Finding 2 (kind must be GW2's
        // real three-way taxonomy, not a damage-based binary).
        let mut b = CatalogBuilder::default();
        b.reference_buff(722); // Chilled -- non-damaging condition
        b.reference_buff(736); // Bleeding -- damaging condition
        let c = b.finish(&Metrics::default(), None);

        let chilled = c.buffs.get(&722).expect("Chilled resolves");
        assert_eq!(chilled.kind, "condition");
        assert_eq!(chilled.stacking, "duration");

        let bleeding = c.buffs.get(&736).expect("Bleeding resolves");
        assert_eq!(bleeding.kind, "condition");
        assert_eq!(bleeding.stacking, "intensity");
        assert_eq!(bleeding.max_stacks, Some(1500));
    }

    #[test]
    fn a_non_boon_non_condition_buff_is_kind_effect() {
        let mut b = CatalogBuilder::default();
        b.reference_buff(5579); // Frost Aura
        let c = b.finish(&Metrics::default(), None);
        assert_eq!(c.buffs.get(&5579).expect("Frost Aura resolves").kind, "effect");
    }

    #[test]
    fn a_referenced_damage_mod_with_no_definition_still_resolves_to_an_entry() {
        // Mirrors `a_referenced_id_with_no_definition_still_resolves_to_an_entry`
        // above (skills) -- Finding 3, review round 1. `DamageModifierResults`
        // with empty `meta` reproduces "referenced id, no metadata".
        use axilog_core::analysis::damage_mods::DamageModifierResults;

        let mut b = CatalogBuilder::default();
        b.reference_damage_mod(424242);
        let c = b.finish(&Metrics::default(), Some(&DamageModifierResults::default()));
        let e = c.damage_mods.get(&424242).expect("referenced id must resolve");
        assert_eq!(e.name, "Damage modifier 424242");
        // Absence, not `false`/`""`, is the honest signal for "no metadata".
        assert!(e.description.is_none());
        assert!(e.non_multiplier.is_none());
        assert!(e.is_counter.is_none());
        assert!(e.skill_based.is_none());
        assert!(e.approximate.is_none());
    }

    #[test]
    fn a_resolved_damage_mod_carries_all_four_booleans_and_a_description() {
        use axilog_core::analysis::damage_mods::{
            DamageModifierMeta, DamageModifierResults,
        };

        let mut results = DamageModifierResults::default();
        results.meta.insert(
            174,
            DamageModifierMeta {
                name: "Scholar Rune",
                icon: "",
                description: "Deal more damage above 90% health.".into(),
                non_multiplier: false,
                is_counter: false,
                skill_based: false,
                approximate: true,
                incoming: false,
            },
        );

        let mut b = CatalogBuilder::default();
        b.reference_damage_mod(174);
        let c = b.finish(&Metrics::default(), Some(&results));
        let e = c.damage_mods.get(&174).expect("referenced id must resolve");
        assert_eq!(e.name, "Scholar Rune");
        assert_eq!(e.description.as_deref(), Some("Deal more damage above 90% health."));
        assert_eq!(e.non_multiplier, Some(false));
        assert_eq!(e.is_counter, Some(false));
        assert_eq!(e.skill_based, Some(false));
        assert_eq!(e.approximate, Some(true));
    }

    #[test]
    fn a_damage_mod_can_be_both_skill_based_and_a_multiplier_at_once() {
        // The exact case the old folded `kind` string got wrong: a modifier
        // with `non_multiplier == false` (i.e. IS a multiplier) that is also
        // `skill_based`. The priority chain used to emit `"skill"`, silently
        // erasing the multiplier axis. The independent booleans preserve
        // both.
        use axilog_core::analysis::damage_mods::{
            DamageModifierMeta, DamageModifierResults,
        };

        let mut results = DamageModifierResults::default();
        results.meta.insert(
            999,
            DamageModifierMeta {
                name: "Hypothetical Skill Multiplier",
                icon: "",
                description: String::new(),
                non_multiplier: false,
                is_counter: false,
                skill_based: true,
                approximate: false,
                incoming: false,
            },
        );

        let mut b = CatalogBuilder::default();
        b.reference_damage_mod(999);
        let c = b.finish(&Metrics::default(), Some(&results));
        let e = c.damage_mods.get(&999).expect("referenced id must resolve");
        assert_eq!(e.skill_based, Some(true), "skill-based axis must survive");
        assert_eq!(e.non_multiplier, Some(false), "multiplier axis must survive too");
    }
}
