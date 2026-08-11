use axilog_core::analysis::buffs;
use axilog_core::analysis::condition_catalog;
use axilog_core::analysis::damage_mods::catalog::buff_stack;
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
    /// `"boon"` or `"condition"`. arcdps does not distinguish them
    /// structurally; this field carries the distinction so one catalog can
    /// serve both.
    pub kind: &'static str,
    /// `"intensity"` or `"duration"`.
    pub stacking: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stacks: Option<u32>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DamageModEntry {
    pub name: String,
    pub kind: String,
    pub approximate: bool,
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
                // `stack_type_for` is the real accessor (buffs::mod.rs);
                // `is_intensity()` is `BuffStackType`'s own method. Unknown
                // ids default to duration, matching GW2EI's own
                // `Buff.cs:120` default.
                let is_intensity =
                    buffs::stack_type_for(id).is_some_and(|t| t.is_intensity());
                (
                    id,
                    BuffEntry {
                        name: buffs::name(id).unwrap_or_default().to_string(),
                        kind: if condition_catalog::is_condition_damage_based(id) {
                            "condition"
                        } else {
                            "boon"
                        },
                        stacking: if is_intensity { "intensity" } else { "duration" },
                        max_stacks: buff_stack::stack_info(id).map(|b| b.capacity),
                    },
                )
            })
            .collect();

        let damage_mods = match mods {
            None => BTreeMap::new(),
            Some(m) => self
                .damage_mods
                .into_iter()
                .filter_map(|id| {
                    m.meta.get(&id).map(|meta| {
                        // No single existing field is a "kind" string; this
                        // orders the same three flags `tooltip()`
                        // (damage_mods/model.rs) already reports in, picking
                        // the first that applies.
                        let kind = if meta.is_counter {
                            "counter"
                        } else if meta.skill_based {
                            "skill"
                        } else if meta.non_multiplier {
                            "flat"
                        } else {
                            "multiplier"
                        }
                        .to_string();
                        (
                            id,
                            DamageModEntry {
                                name: meta.name.to_string(),
                                kind,
                                approximate: meta.approximate,
                            },
                        )
                    })
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
}
