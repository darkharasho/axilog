//! The axilog native output format 1.0 container.
//!
//! Built alongside the legacy [`crate::Report`] from the same inputs. See
//! `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`.
pub mod blocks;
pub mod catalogs;
pub mod entities;
pub mod envelope;
pub mod order;
pub mod series;

use crate::v1::blocks::{activity, damage, defense, support};
use crate::v1::catalogs::{CatalogBuilder, Catalogs};
use crate::v1::entities::build_entities;
use crate::v1::envelope::{AxilogMeta, BlockName, Coverage, CoverageState, Severity, WarningOut};
pub use entities::EntityOut;
pub use order::SourceOrder;
use axilog_core::analysis::damage_mods::DamageModifierResults;
use axilog_core::analysis::Metrics;
use axilog_core::model::Encounter;
use serde::Serialize;

/// The 1.0 encounter envelope. A reprojection of [`crate::EncounterOut`]
/// with `markers[]` rekeyed from `agent_addr` (the legacy join key) to
/// `entity_id` (the 1.0 join key) -- the legacy type is left untouched
/// because the EI adapter still reads it by `agent_addr`.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct EncounterOut {
    pub kind: String,
    pub map: String,
    pub duration_ms: u64,
    pub build: String,
    pub revision: u8,
    /// The recording player, as an entity id. The legacy shape carried the
    /// recorder's raw account string here, which put personal identity in a
    /// second place outside `entities[]` and meant every scrub had to know
    /// about both. Absent when the recorder does not resolve to an entity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_by: Option<u32>,
    pub teams: Vec<crate::TeamOut>,
    pub markers: Vec<MarkerAssignmentOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_rate: Option<crate::TickRateOut>,
}

/// One `CBTS_MARKER` assignment. `arcdps` does not restrict `CBTS_MARKER`
/// to squad members -- `Encounter::markers` records it "across all agents
/// (squad, enemy, and NPC/gadget alike)". Many of those agents never become
/// tracked entities: `wvw::apply`'s `enc.enemies.retain(...)` drops
/// friendly-side NPCs/gadgets (siege, pets, own-team guards) that never
/// took a hostile hit, so a squad marker placed on friendly siege is an
/// ordinary WvW pattern with no resolvable entity id. Dropping those
/// markers would silently discard real data, so `agent_addr` is always
/// carried (the same documented public attribute `EntityOut::agent_addr`
/// exposes) and `entity_id` is populated only when the agent is in the
/// roster.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MarkerAssignmentOut {
    /// The entity this marker is on, when the agent is a tracked entity.
    /// Absent for agents that carry markers but are not tracked entities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
    /// Always present, so a marker is never lost just because its agent is
    /// not a tracked entity.
    pub agent_addr: u64,
    pub marker: String,
    pub time_ms: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ReportV1 {
    pub axilog: AxilogMeta,
    pub encounter: EncounterOut,
    pub entities: Vec<EntityOut>,
    /// The encounter's original agent order, for reprojections that need
    /// positional arrays. Never serialized -- see [`SourceOrder`].
    #[serde(skip)]
    pub source_order: SourceOrder,
    pub catalogs: Catalogs,
    pub blocks: Blocks,
    pub coverage: Coverage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<WarningOut>,
}

/// A block is omitted entirely when `coverage` says `not_computed` or
/// `unsupported`; `empty` blocks are still carried, so a consumer can tell
/// "computed and there was nothing" from "never ran".
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct Blocks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<damage::DamageBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defenses: Option<defense::DefensesBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_stats: Option<defense::HitStatsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<defense::CcBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boons: Option<support::BoonsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<support::SupportBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution: Option<support::ContributionBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing: Option<support::HealingBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<activity::RotationBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage_mods: Option<activity::DamageModsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missiles: Option<activity::MissilesBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<activity::ReplayBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<activity::SeriesBlock>,
}

/// The coverage state for a block that DID run: [`CoverageState::Empty`]
/// when it produced nothing, [`CoverageState::Present`] otherwise.
///
/// Before the final whole-branch review, every computed block was reported
/// `Present` unconditionally and `Empty` was unreachable -- which meant a
/// log with no healing extension reported `healing: "present"` with zero
/// rows, exactly the "absent reported as zero" ambiguity `coverage` exists
/// to remove. Each block decides what "produced nothing" means for its own
/// shape (see each `is_empty`), because a block with an independently
/// computed aggregate -- `missiles`, `series` -- is not empty just because
/// it has no per-entity rows.
///
/// [`CoverageState::Unsupported`] stays unreachable from this container:
/// nothing here is era- or encounter-kind-gated, so no code path can
/// honestly produce it. It is RESERVED vocabulary for spec #2's era-gated
/// surfaces, and `docs/NATIVE-FORMAT.md` tells consumers so explicitly
/// rather than implying today's binary can emit it.
fn computed(is_empty: bool) -> CoverageState {
    if is_empty {
        CoverageState::Empty
    } else {
        CoverageState::Present
    }
}

/// Assemble the 1.0 [`ReportV1`] alongside the already-built legacy
/// [`crate::Report`], which keeps this a pure reprojection instead of a
/// second, divergent computation from `Metrics`.
///
/// `enc` is used only for [`build_entities`] -- every block routes through
/// `legacy`/`metrics` instead, since those are what Tasks 5-8's builders
/// already take.
pub fn build_report_v1(
    enc: &Encounter,
    metrics: &Metrics,
    legacy: &crate::Report,
    axilog_version: &str,
    generated_from: Option<&str>,
    damage_mods: Option<&DamageModifierResults>,
) -> ReportV1 {
    let (entities, index, source_order) = build_entities(enc, metrics);
    let mut cats = CatalogBuilder::default();
    let mut coverage = Coverage::new();

    // Always-on blocks. `computed` distinguishes `Present` from `Empty` --
    // see its doc comment for why that distinction has to be made HERE, at
    // the only layer that knows a block both ran and produced nothing.
    let damage_block = damage::build_damage(legacy, &index, &mut cats);
    coverage.set(BlockName::Damage, computed(damage_block.is_empty()));
    let defenses = defense::build_defenses(legacy, &index);
    coverage.set(BlockName::Defenses, computed(defenses.is_empty()));
    let hit_stats = defense::build_hit_stats(legacy, &index);
    coverage.set(BlockName::HitStats, computed(hit_stats.is_empty()));
    let cc = defense::build_cc(legacy, &index);
    coverage.set(BlockName::Cc, computed(cc.is_empty()));
    let boons = support::build_boons(legacy, &index, &mut cats);
    coverage.set(BlockName::Boons, computed(boons.is_empty()));
    let support_block = support::build_support(legacy, &index);
    coverage.set(BlockName::Support, computed(support_block.is_empty()));
    let contribution = support::build_contribution(legacy, &index);
    coverage.set(BlockName::Contribution, computed(contribution.is_empty()));
    let healing = support::build_healing(legacy, &index);
    coverage.set(BlockName::Healing, computed(healing.is_empty()));
    let series = activity::build_series(legacy, &index);
    coverage.set(BlockName::Series, computed(series.is_empty()));

    // Gated blocks: presence of the legacy `Option` IS the gate signal, the
    // same rule the legacy shape already uses. A gate that was ON but
    // produced no rows is `Empty`, not `Present` -- the gate answers
    // "did it run", the row count answers "was there anything".
    // `rotation` is the one exception to the rule above, because it carries
    // TWO quantities with different gates: `casts` is gated on `--rotation`,
    // but `aftercast` is computed unconditionally. Gating the whole block on
    // the casts meant an ungated legacy field silently vanished whenever
    // `--rotation` was off -- which the ei-json adapter then read as four
    // zeroes rather than the real counters. So the block is always built,
    // and `coverage` keeps its exact meaning by answering the casts question
    // it always answered: `Present` only when some row actually has casts.
    let rotation = Some({
        let block = activity::build_rotation(legacy, &index, &mut cats);
        let no_casts = block.by_entity.0.values().all(|r| r.casts.is_empty());
        coverage.set(BlockName::Rotation, computed(no_casts));
        block
    });
    let damage_mods_block = legacy.damage_mod_map.is_some().then(|| {
        let block = activity::build_damage_mods(legacy, &index, &mut cats);
        coverage.set(BlockName::DamageMods, computed(block.is_empty()));
        block
    });
    let missiles = legacy.missiles.is_some().then(|| {
        let block = activity::build_missiles(legacy, &index);
        coverage.set(BlockName::Missiles, computed(block.is_empty()));
        block
    });
    let replay = legacy.replay.is_some().then(|| {
        let block = activity::build_replay(legacy, &index);
        coverage.set(BlockName::Replay, computed(block.is_empty()));
        block
    });

    // Reserved for spec #2. Named here so the vocabulary is fixed.
    coverage.set(BlockName::Conditions, CoverageState::NotComputed);
    coverage.set(BlockName::Minions, CoverageState::NotComputed);

    // `Metrics::warnings` carries a code at the source as of this task --
    // see `axilog_core::analysis::Warning`. A catch-all code would defeat
    // the whole point of making warnings structured, so there is no `_ =>`
    // arm here -- every `WarningSeverity` variant maps explicitly.
    let mut warnings: Vec<WarningOut> = metrics
        .warnings
        .iter()
        .map(|w| WarningOut {
            code: w.code.to_string(),
            severity: match w.severity {
                axilog_core::analysis::WarningSeverity::Info => Severity::Info,
                axilog_core::analysis::WarningSeverity::Warn => Severity::Warn,
                axilog_core::analysis::WarningSeverity::Error => Severity::Error,
            },
            message: w.message.clone(),
            entity_id: w.agent_addr.and_then(|a| index.by_agent_addr(a)),
        })
        .collect();

    let markers = legacy
        .encounter
        .markers
        .iter()
        .map(|m| MarkerAssignmentOut {
            entity_id: index.by_agent_addr(m.agent_addr),
            agent_addr: m.agent_addr,
            marker: m.marker.clone(),
            time_ms: m.time_ms,
        })
        .collect();

    // The legacy `Encounter::recorded_by` carries the recorder's raw
    // account string (see `wvw::apply` in axilog-core), not an agent addr,
    // so it can't be joined through `index.by_agent_addr` directly. Resolve
    // it the same way -- find the roster player with that account, then
    // join on their addr -- rather than duplicating the account string
    // itself into the 1.0 shape. A recorder whose account doesn't match any
    // roster player (or doesn't resolve to a tracked entity) is dropped
    // rather than falling back to the string: a missing join is
    // recoverable, a duplicated identity is not.
    let recorded_by_account = legacy.encounter.recorded_by.as_deref();
    let recorded_by = recorded_by_account.and_then(|account| {
        enc.players
            .iter()
            .find(|p| p.account == account)
            .and_then(|p| index.by_agent_addr(p.agent_addr))
    });
    // RULING T3-6 (side-channel absorption, Task 3, review round 1): a
    // recorder whose account fails this join used to fail SILENTLY --
    // `encounter.recordedBy` just goes missing (native) / `recordedBy`
    // serializes `null` (ei-json, via the adapter's `entities[]` hop), with
    // no signal that anything was dropped. This is the one new failure
    // mode the entity-id-first `recorded_by` design (this task) introduces
    // that the string-carrying legacy shape never had, so it gets its own
    // warning rather than silence. Structured, not a bare string --
    // `Metrics::warnings`'s own "closed, documented set, no catch-all"
    // convention (see its doc comment) extends here even though this
    // producer lives in `axilog-schema`, not `axilog-core`: `metrics` is
    // `&Metrics` (immutable) by the time this function runs, so there is no
    // `Metrics::warnings` to push into, and the join itself only has
    // `index`/`enc`/`legacy` to work with, none of which `analyze()` sees.
    // No account text in the message: that would recreate exactly the kind
    // of second, less-guarded identity surface commit 6eeb4d8 (spec #1)
    // removed `encounter.recordedBy`-as-a-raw-string to prevent -- see
    // `no_unscrubbed_identity_survives_in_the_v1_document`
    // (`crates/axilog-schema/tests/v1_shape.rs`), which scans this whole
    // document's serialized text for exactly that leak shape.
    if recorded_by_account.is_some() && recorded_by.is_none() {
        warnings.push(WarningOut {
            code: "recorded_by_unresolved".to_string(),
            severity: Severity::Warn,
            message: "the recording player's account did not resolve to a tracked entity; \
                encounter.recordedBy is omitted rather than guessed"
                .to_string(),
            entity_id: None,
        });
    }

    ReportV1 {
        axilog: AxilogMeta {
            schema: "1.0",
            version: axilog_version.to_string(),
            generated_from: generated_from.map(|s| s.to_string()),
        },
        encounter: EncounterOut {
            kind: legacy.encounter.kind.clone(),
            map: legacy.encounter.map.clone(),
            duration_ms: legacy.encounter.duration_ms,
            build: legacy.encounter.build.clone(),
            revision: legacy.encounter.revision,
            recorded_by,
            teams: legacy.encounter.teams.clone(),
            markers,
            tick_rate: legacy.encounter.tick_rate.clone(),
        },
        entities,
        source_order,
        catalogs: cats.finish(metrics, damage_mods),
        blocks: Blocks {
            damage: Some(damage_block),
            defenses: Some(defenses),
            hit_stats: Some(hit_stats),
            cc: Some(cc),
            boons: Some(boons),
            support: Some(support_block),
            contribution: Some(contribution),
            healing: Some(healing),
            rotation,
            damage_mods: damage_mods_block,
            missiles,
            replay,
            series: Some(series),
        },
        coverage,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
    use axilog_core::model::{Encounter, MarkerAssignment, Player};

    fn player(addr: u64, account: &str) -> Player {
        Player {
            agent_addr: addr,
            account: account.into(),
            character: format!("Char{addr}"),
            profession: "Guardian".into(),
            elite_spec: "Firebrand".into(),
            team: "red".into(),
            subgroup: 1,
            in_squad: true,
            commander: false,
            marker: None,
            commander_tag: None,
            guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    /// A marker on an agent NOT in the roster is an ordinary WvW pattern --
    /// `arcdps` does not restrict `CBTS_MARKER` to squad members, and
    /// `wvw::apply`'s retain filter drops friendly siege/pets/guards that
    /// never took a hostile hit before they ever reach `Encounter::enemies`.
    /// This must survive as a marker with `entity_id: None`, not be
    /// silently dropped -- see Finding 1 of Task 9's fix round 1.
    #[test]
    fn a_marker_on_an_unrostered_agent_survives_with_no_entity_id() {
        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![player(1, ":Squaddie.1")],
            enemies: vec![],
            markers: vec![
                MarkerAssignment { agent_addr: 1, marker: "arrow".into(), time_ms: 10 },
                // 99 is never a player or an enemy -- e.g. friendly siege
                // dropped by `wvw::apply`'s retain filter -- so it never
                // resolves to an entity id.
                MarkerAssignment { agent_addr: 99, marker: "star".into(), time_ms: 20 },
            ],
            tick_rate: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, None);

        assert_eq!(v1.encounter.markers.len(), 2, "both markers must survive, not just the resolvable one");

        let resolvable = &v1.encounter.markers[0];
        assert_eq!(resolvable.agent_addr, 1);
        assert_eq!(resolvable.entity_id, Some(0), "agent 1 is the sole roster entity, id 0");
        assert_eq!(resolvable.marker, "arrow");

        let unresolvable = &v1.encounter.markers[1];
        assert_eq!(unresolvable.agent_addr, 99);
        assert_eq!(unresolvable.entity_id, None, "agent 99 is not a tracked entity");
        assert_eq!(unresolvable.marker, "star");

        let v = serde_json::to_value(&v1).expect("serializable");
        let markers = v["encounter"]["markers"].as_array().expect("markers array");
        assert!(markers[0].get("entity_id").is_some(), "resolvable marker keeps entity_id");
        assert!(markers[1].get("entity_id").is_none(), "unresolvable marker omits entity_id, never null");
        assert_eq!(markers[1]["agent_addr"], 99);
    }

    /// RULING T3-6 (side-channel absorption, Task 3, review round 1): a
    /// recorder whose account does not join any roster player must not
    /// fail silently -- `encounter.recordedBy` still goes missing (the
    /// join is genuinely unrecoverable, same as the marker case above),
    /// but a structured `recorded_by_unresolved` warning must appear so
    /// the drop is visible instead of indistinguishable from "this log
    /// never recorded a recorder at all".
    #[test]
    fn an_unresolvable_recorder_drops_recorded_by_but_warns_loudly() {
        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            // Never in `players` below -- e.g. the recorder relogged/left
            // and the account arcdps captured for `recorded_by` doesn't
            // match any surviving roster entry.
            recorded_by: Some(":Ghost.9999".into()),
            teams: vec![],
            players: vec![player(1, ":Squaddie.1")],
            enemies: vec![],
            markers: vec![],
            tick_rate: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, None);

        assert_eq!(v1.encounter.recorded_by, None, "an unresolvable recorder must not fabricate an entity id");

        let w = v1
            .warnings
            .iter()
            .find(|w| w.code == "recorded_by_unresolved")
            .expect("an unresolvable recorder must produce a recorded_by_unresolved warning");
        assert_eq!(w.severity, envelope::Severity::Warn);
        assert!(w.entity_id.is_none(), "there is no resolvable entity to attach this warning to");
        assert!(
            !w.message.contains("Ghost"),
            "the warning message must not leak the unresolved account string: {:?}",
            w.message
        );

        // The native document's top-level `warnings` key carries it (no
        // adapter/CLI-specific plumbing needed for the field itself).
        let v = serde_json::to_value(&v1).expect("serializable");
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(
            warnings.iter().any(|x| x["code"] == "recorded_by_unresolved"),
            "the v1 document's warnings[] must carry the code, not just Rust-side"
        );
    }

    /// A log that never recorded a `recorded_by` at all (common -- see
    /// `Encounter::recorded_by`'s own doc comment) is the ordinary,
    /// unremarkable case: nothing to resolve, so no warning either. This
    /// guards against a version of the T3-6 fix that fires on absence
    /// instead of on a genuine join failure.
    #[test]
    fn no_recorded_by_at_all_produces_no_warning() {
        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![player(1, ":Squaddie.1")],
            enemies: vec![],
            markers: vec![],
            tick_rate: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, None);

        assert_eq!(v1.encounter.recorded_by, None);
        assert!(
            !v1.warnings.iter().any(|w| w.code == "recorded_by_unresolved"),
            "absence of a recorder is not a join failure and must not warn"
        );
    }
}
