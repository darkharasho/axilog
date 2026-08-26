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
    /// Minion identities, keyed by a synthetic id (see
    /// [`CatalogBuilder::reference_minion`]).
    ///
    /// A catalog rather than fields on `blocks.minions`'s rows because
    /// this format keeps human-readable names in `catalogs` and
    /// `entities[]` ONLY -- `v1_shape.rs::no_block_inlines_a_human_
    /// readable_name` enforces it. A minion is not a tracked entity (it
    /// has no `entities[]` row and no statistics of its own beyond the
    /// damage it took), so `entities[]` is not the right home either.
    /// Deduplicating helps as a side effect: on a squad log the same
    /// summon name recurs across every player running that build.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub minions: BTreeMap<u32, MinionEntry>,
}

/// One minion species-and-name pair. Both halves are identity, which is
/// why they live here rather than on the block's rows.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MinionEntry {
    /// The GW2 species id (`RawAgent::prof`) of the first agent folded
    /// into the group. NOT unique across entries -- the pass groups by
    /// NAME, so two entries can share a species id.
    pub species_id: u32,
    pub name: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SkillEntry {
    pub name: String,
    /// The asset URL for this id, from whichever generated catalog knows
    /// it: `skill_icons` (the GW2 API) first, then `buff_icons` (GW2EI's
    /// buff list) for the boons and conditions arcdps reports through the
    /// skill table but the API has no record of. The API wins ties because
    /// it is ArenaNet's own data.
    ///
    /// Still omitted when neither catalog has art -- an absent icon is the
    /// honest answer, and a consumer rendering icons already has to handle
    /// a missing one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub is_swap: bool,
    pub can_crit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_attack: Option<bool>,
    /// MPROC -- see [`crate::SkillMapEntryOut`]'s fields of the same
    /// names. Carried on the catalog entry rather than on a block row
    /// because they are properties of the SKILL, not of any one player's
    /// use of it, which is exactly what this catalog is for.
    ///
    /// **All five are omitted when `false`**, unlike their `is_swap` /
    /// `can_crit` neighbours. A proc flag is rare -- on the committed
    /// fixture nearly every one of the ~370 skills is false on all five
    /// -- so serializing them unconditionally cost 46,048 bytes, +16.3%
    /// of the whole HTML report, for almost no information. `is_swap` and
    /// `can_crit` are informative in BOTH states and stay unconditional.
    /// Absence therefore means `false`, not "unknown"; the schema test
    /// `proc_flags_serialize_only_when_true` pins both states.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_trait_proc: bool,
    /// See [`SkillEntry::is_trait_proc`].
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_gear_proc: bool,
    /// See [`SkillEntry::is_trait_proc`].
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_unconditional_proc: bool,
    /// See [`SkillEntry::is_trait_proc`].
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_not_accurate: bool,
    /// See [`SkillEntry::is_trait_proc`]. Unlike its four neighbours this
    /// one required the finders to actually run.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_instant_cast: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct BuffEntry {
    pub name: String,
    /// The buff's art, resolved the same way `SkillEntry::icon` is: the GW2
    /// API catalog first, then GW2EI's buff table.
    ///
    /// Boons and conditions reach a log through its skill table, so a buff id
    /// often has a `SkillEntry` too — but only when some event referenced it
    /// as a skill. A buff the log only ever reported as an application (a
    /// boon nobody's damage came from) has no skill row at all, so a consumer
    /// rendering a boon or condition had nowhere to read art from and had to
    /// carry its own hardcoded table. Hence this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
    /// The `render.guildwars2.com` asset URL for this modifier, when its
    /// definition is known. Carried because the ei-json adapter's
    /// `damageModMap.icon` must be reconstructible from this document
    /// alone -- absorbing the side channel means native is a superset, and
    /// an icon the adapter emits but native drops would break that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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

/// Accumulates referenced ids as blocks emit them, then materializes that
/// subset -- for buffs and damage modifiers, exactly it.
///
/// `skills` is the deliberate exception. It carries the WHOLE of
/// `Metrics::skill_map` (always computed, never gated) unioned with any
/// referenced id, rather than referenced ids alone. Relaxing "every
/// catalog entry is referenced" here is what makes the native document a
/// superset of ei-json: EI's `skillMap` is unconditionally the full table,
/// so a referenced-only native catalog is EMPTY with every gate off while
/// the adapter emits 368 rows on the committed fixture. The side channel
/// cannot be deleted while the adapter needs a source native lacks.
///
/// The inverse invariant still holds, and is the one worth keeping: every
/// id any row references resolves to an entry.
#[derive(Debug, Default, Clone)]
pub struct CatalogBuilder {
    skills: BTreeSet<u32>,
    buffs: BTreeSet<u32>,
    damage_mods: BTreeSet<i32>,
    /// `(species_id, name)` -> synthetic minion id, in first-seen order.
    minions: std::collections::HashMap<(u32, String), u32>,
}

/// One id, three catalogs, one answer.
///
/// GW2EI's overrides come FIRST -- the order GW2EI itself uses in
/// `SkillItem.cs`, which consults `OverridenSkillIcons` before falling back
/// to `ApiSkill.Icon`. An entry there is a deliberate correction, so
/// deferring to the API would reinstate exactly the value GW2EI overrode.
/// It is also the only source for ids `/v2/skills` does not list at all --
/// sigil procs, pet skills, combo finishers, phantasms. On the committed
/// fixture the API leaves 84 of 508 skill ids art-less and the override
/// table covers 73 of them, while the two overlap on only 29 ids and differ
/// on 19, so putting overrides first is a small, deliberate change.
///
/// `buff_icons` is last because it is the narrowest: boons and conditions,
/// which neither other table carries.
fn resolve_icon(id: u32) -> Option<String> {
    if let Some(url) = axilog_core::analysis::skill_icon_overrides::icon(id) {
        return Some(url.to_owned());
    }
    axilog_core::analysis::skill_icons::icon(id)
        .or_else(|| axilog_core::analysis::buff_icons::icon(id).map(str::to_owned))
}

impl CatalogBuilder {
    pub fn reference_skill(&mut self, id: u32) {
        self.skills.insert(id);
    }
    pub fn reference_buff(&mut self, id: u32) {
        self.buffs.insert(id);
    }
    /// Intern a minion identity, returning its synthetic catalog id.
    ///
    /// Ids are assigned in first-seen order, which makes them stable for a
    /// given log but meaningless across logs -- exactly like the entity
    /// ids this format already uses as join keys, and for the same reason:
    /// nothing in the source data offers a stable unique key here. The
    /// species id alone will not do, because the pass groups minions by
    /// NAME and two differently-named groups can share one species id.
    pub fn reference_minion(&mut self, species_id: u32, name: &str) -> u32 {
        let next = self.minions.len() as u32;
        *self.minions.entry((species_id, name.to_owned())).or_insert(next)
    }
    pub fn reference_damage_mod(&mut self, id: i32) {
        self.damage_mods.insert(id);
    }

    pub fn finish(mut self, metrics: &Metrics, mods: Option<&DamageModifierResults>) -> Catalogs {
        // The always-on half of the skill catalog (see the struct's doc
        // comment). Union, not replacement: a referenced id the log table
        // never named still has to resolve, and keeps its placeholder.
        self.skills.extend(metrics.skill_map.keys().copied());
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
                        //
                        // One chain, shared with `skill_map::resolve_name`
                        // -- see its "One chain, two callers" section. An
                        // id the map covered keeps the name that pass
                        // already resolved; anything else goes through the
                        // full chain from its first rung, the log's own
                        // table, which is the rung this site used to lack.
                        name: entry.map(|e| e.name.clone()).unwrap_or_else(|| {
                            axilog_core::analysis::skill_map::resolve_name(
                                id,
                                metrics.log_skill_names.get(&id).map(String::as_str),
                            )
                        }),
                        icon: resolve_icon(id),
                        // Pure functions of the id -- the SAME two the skill
                        // map itself calls, so a covered and an uncovered id
                        // get the same answer. Defaulting them (`false` /
                        // `true`) let an uncovered heal skill claim it could
                        // crit.
                        is_swap: axilog_core::analysis::skill_map::is_swap(id),
                        can_crit: axilog_core::analysis::hit_stats::can_crit(id),
                        // The log never carries this; the generated GW2 API
                        // catalog is the only source. Kept as a fallback to
                        // whatever the pipeline resolved, which is `None`
                        // everywhere today, so the catalog is in practice
                        // the answer.
                        auto_attack: entry
                            .and_then(|e| e.auto_attack)
                            .or_else(|| axilog_core::analysis::skill_icons::auto_attack(id)),
                        // MPROC. An id the skill map never covered gets
                        // `false` rather than a guess: no finder claimed
                        // it, which is what `false` means here.
                        is_trait_proc: entry.is_some_and(|e| e.is_trait_proc),
                        is_gear_proc: entry.is_some_and(|e| e.is_gear_proc),
                        is_unconditional_proc: entry.is_some_and(|e| e.is_unconditional_proc),
                        is_not_accurate: entry.is_some_and(|e| e.is_not_accurate),
                        is_instant_cast: entry.is_some_and(|e| e.is_instant_cast),
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
                        icon: resolve_icon(id),
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
                            icon: Some(meta.icon.to_string()),
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
                            icon: None,
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

        let minions = self
            .minions
            .into_iter()
            .map(|((species_id, name), id)| (id, MinionEntry { species_id, name }))
            .collect();
        Catalogs { skills, buffs, damage_mods, minions }
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
                is_trait_proc: false,
                is_gear_proc: false,
                is_unconditional_proc: false,
                is_not_accurate: false,
                is_instant_cast: false,
            },
        );
        m.skill_map.insert(
            9999,
            axilog_core::analysis::skill_map::SkillMapEntry {
                name: "Never Referenced".into(),
                auto_attack: None,
                is_swap: false,
                can_crit: true,
                is_trait_proc: false,
                is_gear_proc: false,
                is_unconditional_proc: false,
                is_not_accurate: false,
                is_instant_cast: false,
            },
        );
        m
    }

    /// The buff and damage-mod catalogs are still referenced-only. Skills
    /// are the documented exception (see `CatalogBuilder`'s doc comment):
    /// they carry the whole always-on `Metrics::skill_map` so the ei-json
    /// adapter's unconditionally-full `skillMap` has a native source.
    #[test]
    fn only_the_skill_catalog_admits_unreferenced_definitions() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        b.reference_buff(1187);
        let c = b.finish(&metrics_with_skills(), None);
        assert!(c.skills.contains_key(&5491), "referenced id must resolve");
        assert!(
            c.skills.contains_key(&9999),
            "an unreferenced skill definition rides along -- EI's skillMap is the full table"
        );
        assert_eq!(c.buffs.len(), 1, "buffs stay referenced-only");
        assert!(c.damage_mods.is_empty(), "damage mods stay referenced-only");
    }

    #[test]
    fn referencing_the_same_id_twice_yields_one_entry() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        // 5491 once, plus the always-on 9999 the skill table carries.
        assert_eq!(c.skills.len(), 2);
        assert!(c.skills.contains_key(&5491));
    }

    /// Icons and auto-attack come from the generated GW2 API catalog, NOT
    /// from the log. 5491 is a real API skill (Fireball, `Weapon_1`), so
    /// both resolve -- while its `name` still comes from the log table,
    /// which calls it something else entirely here. That split is the
    /// point: the log owns what it observed, the catalog owns what only
    /// ArenaNet's database knows.
    #[test]
    fn skill_icons_and_auto_attack_come_from_the_generated_api_catalog() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        let e = &c.skills[&5491];
        assert_eq!(e.name, "Symbol of Protection", "the name stays the log's");
        assert_eq!(
            e.icon.as_deref(),
            Some("https://render.guildwars2.com/file/E57B9C0358A6B1CE4631E336D22614E9E544DD4B/102965.png")
        );
        assert_eq!(e.auto_attack, Some(true), "Fireball is Weapon_1");
    }

    /// A referenced id the skill map never covered still gets a real name
    /// when the GW2 API knows one.
    ///
    /// This path is not `skill_map::resolve_name` -- it is the union half
    /// of `finish`, which fabricates an entry for an id referenced by a
    /// block but absent from the map. Its `"Skill {id}"` placeholder needs
    /// the same recovery, or the two halves of the same catalog disagree
    /// about what id 14404 is called.
    #[test]
    fn a_referenced_id_outside_the_skill_map_is_named_from_the_api_catalog() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(14404);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.skills[&14404].name, "Signet of Might");
    }

    /// ...and keeps the placeholder when neither source knows the id, so
    /// the dangling-reference invariant still holds.
    #[test]
    fn a_referenced_id_no_catalog_knows_keeps_the_placeholder_name() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(4_000_000);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.skills[&4_000_000].name, "Skill 4000000");
    }

    /// A buff id the log records as a skill (717 = Protection) has no
    /// record in `/v2/skills`, so it gets no icon rather than a wrong one.
    /// This is the known coverage edge -- 60 of the committed fixture's
    /// 368 skill ids are buff ids like this one.
    #[test]
    fn a_buff_id_logged_as_a_skill_gets_its_icon_from_the_buff_catalog() {
        // 717 is Protection. arcdps reports it through the skill table, so
        // it reaches us looking like a skill, but `/v2/skills` has no record
        // of it -- this is exactly the gap `buff_icons` exists to close, and
        // the fallback is what makes the two catalogs add up to one answer.
        let mut b = CatalogBuilder::default();
        b.reference_skill(717);
        let c = b.finish(&metrics_with_skills(), None);
        assert!(axilog_core::analysis::skill_icons::icon(717).is_none(), "not an API skill");
        assert_eq!(
            c.skills[&717].icon.as_deref(),
            axilog_core::analysis::buff_icons::icon(717),
        );
        // A buff has no weapon slot, so the auto-attack question still does
        // not apply -- the buff catalog carries art and nothing else.
        assert!(c.skills[&717].auto_attack.is_none());
    }

    /// Every boon and condition carries art.
    ///
    /// `BuffEntry` had no `icon` field at all until this test's fix, so a
    /// consumer rendering a boon or condition had nowhere to read one from.
    /// The skill catalog was not a substitute: a buff only gets a `SkillEntry`
    /// when some event referenced its id AS a skill, which a boon nobody's
    /// damage came from never does.
    #[test]
    fn every_boon_and_condition_in_the_catalog_carries_an_icon() {
        let mut b = CatalogBuilder::default();
        for id in [740u32, 717, 1187, 736, 737, 722, 727, 27705] {
            b.reference_buff(id);
        }
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.buffs.len(), 8, "guard against the loop silently building nothing");
        for (id, entry) in &c.buffs {
            assert!(
                matches!(entry.kind, "boon" | "condition"),
                "buff {id} ({}) should be a boon or condition here, got {}",
                entry.name,
                entry.kind
            );
            let icon = entry.icon.as_deref().unwrap_or_else(|| {
                panic!("buff {id} ({}) has no icon", entry.name)
            });
            assert!(icon.starts_with("https://render.guildwars2.com/"), "buff {id}: {icon}");
        }
    }

    /// GW2EI's override table supplies art for ids `/v2/skills` does not
    /// list at all, which is why this catalog exists: 84 of the committed
    /// fixture's 508 skill ids come back `invalid` from the live API.
    #[test]
    fn an_id_the_api_does_not_know_gets_its_icon_from_the_override_table() {
        // 5703 is Arcane Shield (Explosion) -- a real skill in real logs and
        // in real Elite Insights exports, absent from `/v2/skills`.
        assert!(axilog_core::analysis::skill_icons::icon(5703).is_none(), "not an API skill");
        let expected = axilog_core::analysis::skill_icon_overrides::icon(5703)
            .expect("GW2EI overrides Arcane Shield (Explosion)");
        let mut b = CatalogBuilder::default();
        b.reference_skill(5703);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.skills[&5703].icon.as_deref(), Some(expected));
    }

    /// An override BEATS the API, which is the order GW2EI itself uses.
    /// The two tables overlap on only 29 ids and disagree on 19, so this is
    /// a small deliberate change -- but it is a change, and it needs a test
    /// that fails if the precedence is ever flipped back.
    #[test]
    fn the_override_table_wins_over_the_api() {
        let overlapping = axilog_core::analysis::skill_icon_overrides::SKILL_ICON_OVERRIDES
            .iter()
            .find(|(id, url)| {
                axilog_core::analysis::skill_icons::icon(*id).is_some_and(|api| api != *url)
            });
        let Some(&(id, override_url)) = overlapping else {
            return; // no disagreement in the current tables; nothing to assert
        };
        let mut b = CatalogBuilder::default();
        b.reference_skill(id);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(
            c.skills[&id].icon.as_deref(),
            Some(override_url),
            "skill {id}: the override must win over the API"
        );
    }

    #[test]
    fn the_api_catalog_wins_when_both_catalogs_know_an_id() {
        // Ids in both tables are real skills that also apply a buff. The
        // API is ArenaNet's own data, so it is the one that must show up.
        let overlapping = axilog_core::analysis::buff_icons::BUFF_ICONS
            .iter()
            .find(|(id, _)| axilog_core::analysis::skill_icons::icon(*id).is_some());
        let Some(&(id, buff_url)) = overlapping else {
            return; // no overlap in the current tables; nothing to assert
        };
        let mut b = CatalogBuilder::default();
        b.reference_skill(id);
        let c = b.finish(&metrics_with_skills(), None);
        let api_url = axilog_core::analysis::skill_icons::icon(id);
        assert_eq!(c.skills[&id].icon, api_url);
        if api_url.as_deref() != Some(buff_url) {
            assert_ne!(c.skills[&id].icon.as_deref(), Some(buff_url), "buff art did not win");
        }
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

    /// The bug from the MNAME report, reduced: an id that `blocks.healing`
    /// referenced but `skill_map`'s damage/rotation scope never covered.
    /// Before this task `finish` had no way to reach the log's own name for
    /// it and emitted the `Skill <id>` placeholder -- the literal string the
    /// reporter saw rendered in AxiBridge's healing table.
    #[test]
    fn finish_names_a_referenced_id_from_the_logs_own_skill_table() {
        let mut metrics = metrics_with_skills();
        metrics.log_skill_names.insert(13721, "Restorative Mantras".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(13721);
        let catalogs = cats.finish(&metrics, None);

        assert_eq!(
            catalogs.skills[&13721].name, "Restorative Mantras",
            "an id outside skill_map's scope must still resolve through the log table"
        );
    }

    /// The log table is rung ONE, not a fallback: an id `skill_map` did
    /// cover keeps the name that pass already resolved, so this change can
    /// never move a name that resolved before it.
    #[test]
    fn finish_prefers_the_skill_map_entry_over_the_log_table() {
        let mut metrics = metrics_with_skills();
        let covered = *metrics.skill_map.keys().next().expect("helper seeds at least one skill");
        let expected = metrics.skill_map[&covered].name.clone();
        metrics.log_skill_names.insert(covered, "SHOULD NOT WIN".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(covered);
        let catalogs = cats.finish(&metrics, None);

        assert_eq!(catalogs.skills[&covered].name, expected);
    }

    /// Weapon Swap (-2 as u32) is the clearest `is_swap` case: it is true
    /// for it by definition, but an id outside `skill_map`'s scope used to
    /// get `is_swap: false` -- wrong, and computable from the id with no
    /// log at all. `can_crit` is asserted `true` here too, but only
    /// because Weapon Swap happens not to be one of EI's 20
    /// `NonCritableSkills` entries -- see the sibling test below for the
    /// case that actually exercises the `can_crit` fix.
    #[test]
    fn finish_computes_is_swap_for_an_id_the_skill_map_never_covered() {
        let metrics = metrics_with_skills();
        let weapon_swap = (-2i32) as u32;

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(weapon_swap);
        let catalogs = cats.finish(&metrics, None);

        let entry = &catalogs.skills[&weapon_swap];
        assert!(entry.is_swap, "is_swap is a pure function of the id");
        assert!(
            entry.can_crit,
            "Weapon Swap is not in hit_stats::NON_CRITABLE_SKILLS, so the \
             pure function and an uncovered skill_map entry agree by \
             construction -- true, not a default"
        );
    }

    /// 9292 (`LightningStrike_SigilOfAir`) IS one of EI's 20
    /// `NonCritableSkills` entries (`hit_stats::NON_CRITABLE_SKILLS`) and is
    /// deliberately NOT seeded by `metrics_with_skills()` (which only seeds
    /// 5491 and 9999), so this proves `finish` computes `can_crit` for an
    /// id `skill_map` never covered, rather than defaulting it to `true`.
    #[test]
    fn finish_computes_can_crit_for_an_id_the_skill_map_never_covered() {
        let metrics = metrics_with_skills();

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(9292);
        let catalogs = cats.finish(&metrics, None);

        assert!(
            !catalogs.skills[&9292].can_crit,
            "9292 is in EI's NonCritableSkills table; finish used to default it to true"
        );
    }
}
