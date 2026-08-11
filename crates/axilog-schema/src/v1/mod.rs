//! The axilog native output format 1.0 container.
//!
//! Built alongside the legacy [`crate::Report`] from the same inputs. See
//! `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`.
pub mod blocks;
pub mod catalogs;
pub mod entities;
pub mod envelope;
pub mod series;

use crate::v1::blocks::{activity, damage, defense, support};
use crate::v1::catalogs::{CatalogBuilder, Catalogs};
use crate::v1::entities::{build_entities, EntityOut};
use crate::v1::envelope::{AxilogMeta, BlockName, Coverage, CoverageState, Severity, WarningOut};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_by: Option<String>,
    pub teams: Vec<crate::TeamOut>,
    pub markers: Vec<MarkerAssignmentOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_rate: Option<crate::TickRateOut>,
}

/// One `CBTS_MARKER` assignment, rekeyed onto the 1.0 entity id space.
/// Markers on agents that never resolved to an entity (an observed-but-
/// unrostered agent) are dropped rather than emitted with a dangling id --
/// every id in the 1.0 document must resolve.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MarkerAssignmentOut {
    pub entity_id: u32,
    pub marker: String,
    pub time_ms: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ReportV1 {
    pub axilog: AxilogMeta,
    pub encounter: EncounterOut,
    pub entities: Vec<EntityOut>,
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
    let (entities, index) = build_entities(enc, metrics);
    let mut cats = CatalogBuilder::default();
    let mut coverage = Coverage::new();

    // Always-on blocks.
    let damage_block = damage::build_damage(legacy, &index, &mut cats);
    coverage.set(BlockName::Damage, CoverageState::Present);
    let defenses = defense::build_defenses(legacy, &index);
    coverage.set(BlockName::Defenses, CoverageState::Present);
    let hit_stats = defense::build_hit_stats(legacy, &index);
    coverage.set(BlockName::HitStats, CoverageState::Present);
    let cc = defense::build_cc(legacy, &index);
    coverage.set(BlockName::Cc, CoverageState::Present);
    let boons = support::build_boons(legacy, &index, &mut cats);
    coverage.set(BlockName::Boons, CoverageState::Present);
    let support_block = support::build_support(legacy, &index);
    coverage.set(BlockName::Support, CoverageState::Present);
    let contribution = support::build_contribution(legacy, &index);
    coverage.set(BlockName::Contribution, CoverageState::Present);
    let healing = support::build_healing(legacy, &index);
    coverage.set(BlockName::Healing, CoverageState::Present);
    let series = activity::build_series(legacy, &index);
    coverage.set(BlockName::Series, CoverageState::Present);

    // Gated blocks: presence of the legacy `Option` IS the gate signal, the
    // same rule the legacy shape already uses.
    let rotation = legacy.players.iter().any(|p| p.rotation.is_some()).then(|| {
        coverage.set(BlockName::Rotation, CoverageState::Present);
        activity::build_rotation(legacy, &index, &mut cats)
    });
    let damage_mods_block = legacy.damage_mod_map.is_some().then(|| {
        coverage.set(BlockName::DamageMods, CoverageState::Present);
        activity::build_damage_mods(legacy, &index, &mut cats)
    });
    let missiles = legacy.missiles.is_some().then(|| {
        coverage.set(BlockName::Missiles, CoverageState::Present);
        activity::build_missiles(legacy, &index)
    });
    let replay = legacy.replay.is_some().then(|| {
        coverage.set(BlockName::Replay, CoverageState::Present);
        activity::build_replay(legacy, &index)
    });

    // Reserved for spec #2. Named here so the vocabulary is fixed.
    coverage.set(BlockName::Conditions, CoverageState::NotComputed);
    coverage.set(BlockName::Minions, CoverageState::NotComputed);

    // `Metrics::warnings` carries a code at the source as of this task --
    // see `axilog_core::analysis::Warning`. A catch-all code would defeat
    // the whole point of making warnings structured, so there is no `_ =>`
    // arm here -- every `WarningSeverity` variant maps explicitly.
    let warnings = metrics
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
        .filter_map(|m| {
            index.by_agent_addr(m.agent_addr).map(|entity_id| MarkerAssignmentOut {
                entity_id,
                marker: m.marker.clone(),
                time_ms: m.time_ms,
            })
        })
        .collect();

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
            recorded_by: legacy.encounter.recorded_by.clone(),
            teams: legacy.encounter.teams.clone(),
            markers,
            tick_rate: legacy.encounter.tick_rate.clone(),
        },
        entities,
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
