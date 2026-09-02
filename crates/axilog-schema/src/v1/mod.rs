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

use crate::v1::blocks::{
    activity, conditions, damage, defense, focus, minions, self_effects, squad_buffs, support,
};
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
    /// `"wvw"`, or GW2EI's PvE `LogCategory` slug (`"raid_wing"`,
    /// `"fractal"`, `"raid_encounter"`, `"golem"`, `"story"`,
    /// `"open_world"`, `"convergence"`, `"unknown_encounter"`,
    /// `"unknown"`). Was `"wvw"` for every log until PvE encounter
    /// identification landed -- a consumer that treats any non-`"wvw"`
    /// value as unreachable predates that.
    pub kind: String,
    /// WvW map display name. EMPTY STRING for a PvE log: `map_id` below is
    /// still the real instance map, but there is no id->name table for PvE
    /// maps here, and the WvW fallback ("World vs World") would be a lie on
    /// a raid log. Read [`Self::encounter_name`] for what to show instead.
    pub map: String,
    /// The fight's name, for a PvE log -- `"Gorseval the Multifarious"`,
    /// `"Twin Largos"`, `"Harvest Temple"`. Omitted (not `null`) for WvW
    /// logs, which have no encounter identity separate from their map.
    ///
    /// This carries NO challenge-mote suffix: GW2EI would render a
    /// challenge-mode Skorvald as `"Skorvald CM"`, this says `"Skorvald"`.
    /// See [`axilog_core::pve`]'s module doc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encounter_name: Option<String>,
    /// arcdps's header trigger species id (evtc bytes 13-14), for a PvE
    /// log. Omitted for WvW. This is the one fact arcdps records about
    /// which encounter was fought, and the join key into GW2EI's tables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_id: Option<u32>,
    /// GW2EI's `SubLogCategory`: the wing or fractal grouping
    /// (`"SpiritVale"`, `"ShatteredObservatory"`). Omitted for WvW logs and
    /// for trigger ids GW2EI declares no grouping for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_category: Option<String>,
    /// Whether the squad won. Omitted for WvW, which has no failure state.
    ///
    /// **`true` is reliable, `false` is not.** Only GW2EI's generic
    /// success rule is implemented -- "every agent of the trigger species
    /// died" ([`axilog_core::pve::succeeded`]). Encounters GW2EI succeeds
    /// by reward chest or scripted event (Siege the Stronghold, Twisted
    /// Castle, River of Souls, the Hall of Chains statues) report `false`
    /// on a clean kill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    /// The raw `CBTS_MAPID` (`sc::MAP_ID`) value that `map` is the display
    /// name for. Carried alongside the name, not instead of it: the name is
    /// for people, the id is the join key. A consumer that wants to place
    /// this encounter on its own map assets -- tile sets, objective
    /// catalogs, landmark tables -- needs a stable id, and matching on the
    /// display string would break the moment a name is reworded.
    ///
    /// The geometry needed to project `blocks.replay` positions onto those
    /// assets does NOT have to be looked up from this id: it travels with
    /// the positions as [`super::blocks::activity::ArenaOut`]. This field is
    /// for the consumer's own tables.
    ///
    /// Omitted (not `null`) when the log carries no MAP_ID event, which must
    /// stay distinguishable from map id 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_id: Option<u32>,
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
    /// Ground-placed squad markers, from `CBTS_SQUADMARKER`.
    ///
    /// A different system from `markers` above, not a variant of it: these
    /// are attached to a world POSITION rather than an agent, and identified
    /// by a fixed index rather than a content GUID.
    pub ground_markers: Vec<GroundMarkerOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_rate: Option<crate::TickRateOut>,
    /// Wall-clock log start, seconds since the epoch, from arcdps's
    /// `CBTS_LOGSTART`. `None` when the log carries no such event --
    /// absence must stay distinguishable from epoch zero, which is why
    /// this is not a bare `u64` defaulting to 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_unix: Option<u64>,
    /// The log's `t0` in arcdps **session time** ms -- the origin every
    /// other time in this document is already measured from.
    ///
    /// This document has exactly two fields that are NOT log-relative,
    /// both raw event times passed through deliberately:
    /// [`MarkerAssignmentOut::time_ms`] and
    /// [`super::v1::entities::CommanderOut::segments`]. Both say to rebase
    /// by the log's `t0`; this is it. It is carried because a consumer
    /// reading this container as JSON has no other access to it, which made
    /// those two fields uninterpretable -- session time has no fixed
    /// origin, so their values could not be compared against `duration_ms`,
    /// against each other across logs, or against anything else. Subtract
    /// this to get encounter-relative ms; the result may be negative for a
    /// tag held before the first event, which is why the rebase is left to
    /// the consumer rather than done here in `u64`.
    ///
    /// Not optional: `t0` is defined for every log (0 for one with no
    /// events), unlike the wall-clock [`Self::started_at_unix`].
    pub log_start_ms: u64,
    /// WvW objective ownership timelines (MOBJ). Carried verbatim from the
    /// legacy shape -- unlike `markers` there is no join key to rekey, an
    /// objective belongs to the map rather than to an entity.
    pub objectives: Vec<crate::ObjectiveOut>,
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
    /// The raw 32-hex content GUID, exactly as arcdps reported it.
    pub marker: String,
    /// `"squad_marker"`, `"commander_tag"` or `"catmander_tag"`, resolved
    /// from [`axilog_core::analysis::marker_icons`]. Absent when GW2EI does
    /// not know this GUID -- real logs do carry such ids, so a consumer must
    /// handle the absence rather than assume coverage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_kind: Option<&'static str>,
    /// `"Arrow"`, `"Purple"`, `"X"`. Absent on the same terms as `marker_kind`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_label: Option<&'static str>,
    /// Wiki art for the marker. Absent on the same terms as `marker_kind`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_icon: Option<&'static str>,
    /// arcdps **session time** ms, passed through from
    /// `model::MarkerAssignment::time_ms` with no rebase -- one of this
    /// document's only two non-log-relative times. Subtract
    /// [`EncounterOut::log_start_ms`] for an encounter-relative value.
    pub time_ms: u64,
}

/// One ground-placed squad marker.
///
/// Positions are WORLD INCHES, the same space `blocks.replay` tracks use, so
/// a consumer can project them with the same arena transform and needs no
/// separate calibration.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct GroundMarkerOut {
    /// 0..=7, GW2EI's `SquadMarkerIndex` order.
    pub index: u8,
    /// `"arrow"`, `"circle"`, ... -- the same names the overhead variants
    /// use, so one icon lookup serves both.
    pub name: String,
    /// Wiki art for the shape, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<&'static str>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// arcdps **session time** ms, like [`MarkerAssignmentOut::time_ms`].
    pub start_ms: u64,
    /// Absent for a marker still placed when the log ends -- the log
    /// boundary is not a removal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minions: Option<minions::MinionsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<conditions::ConditionsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_effects: Option<self_effects::SelfEffectsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squad_buffs: Option<squad_buffs::SquadBuffsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<focus::FocusBlock>,
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
/// [`CoverageState::Unsupported`] is NOT one of this function's two
/// answers, because it is not a property of the output -- it says the
/// question is unanswerable for this log, which only the caller knows.
/// Exactly one block reaches it today: `healing`, on a log written without
/// the arcdps healing extension (Task 10). Until then it was reserved
/// vocabulary, and `docs/NATIVE-FORMAT.md` said so.
fn computed(is_empty: bool) -> CoverageState {
    if is_empty {
        CoverageState::Empty
    } else {
        CoverageState::Present
    }
}

/// The optional analysis passes this container is built from.
///
/// Every field is a pass that `analyze()` does NOT run -- an opt-in gated
/// on a CLI flag or SDK option -- borrowed from whichever caller ran it.
/// `None` means "that pass did not run", which is exactly what the
/// corresponding block's `coverage` reports as `not_computed`.
///
/// This exists because of what the absorption program does to this
/// function's arity. `damage_mods` arrived as a seventh positional
/// parameter, and Tasks 6-12 each move one or two more passes in here:
/// following that pattern ends at a fourteen-argument function whose call
/// sites are a row of bare `None`s, and rewrites all ~40 of them seven
/// times over. A `Default`-able borrow struct costs those call sites one
/// edit total (this task's), after which a new pass is a new field and no
/// call site moves at all.
///
/// It was emphatically NOT a second `EiInputs` (the ei-json adapter's own
/// side-channel struct, deleted in Task 13). The distinction that made
/// the program work is *who reads it*: this is consumed HERE, by the
/// native builder, and every value in it lands in a block on the native
/// wire. `EiInputs` was read by the ei-json adapter, which is precisely the
/// data path spec #2 exists to delete -- data that reached ei-json without
/// ever existing natively. A pass entering through this struct has already
/// been absorbed.
#[derive(Default, Clone, Copy)]
pub struct Passes<'a> {
    /// M16 `--modifiers`. Was the seventh positional parameter.
    pub damage_mods: Option<&'a DamageModifierResults>,
    /// MEIGAP Task 3b `--skill-damage`: per-player minion damage-taken
    /// rollups, positionally joined to `enc.players`.
    pub minions: Option<&'a axilog_core::analysis::minions::MinionRollups>,
    /// MEIGAP2 row 2 `--timeseries`: per-player health-percent step series,
    /// keyed by the account's representative agent address.
    pub health_percents: Option<&'a std::collections::BTreeMap<u64, Vec<(u64, f64)>>>,
    /// MEIGAP Task 2c `--skill-damage`: per-enemy outgoing per-skill damage,
    /// keyed by the enemy's representative agent id (`Enemy::id`). Lands on
    /// `blocks.damage.by_entity[enemy].by_skill` -- the same field the
    /// player rows use, not a parallel enemy structure.
    pub enemy_dist: Option<
        &'a std::collections::BTreeMap<u64, Vec<axilog_core::analysis::skill_damage::SkillEntry>>,
    >,
    /// MEIGAP Task 2b `--timeseries`: per-enemy cumulative outgoing damage
    /// and its power half, keyed by the enemy's representative agent id
    /// (`Enemy::id`). Lands on `blocks.series.by_entity[enemy]` -- the same
    /// rows the players use, with the outgoing power split in
    /// `EntitySeries::power_damage`.
    pub enemy_series: Option<
        &'a std::collections::BTreeMap<u64, axilog_core::analysis::timeseries::EnemySeries>,
    >,
    /// MEIGAP2 row 1 `--skill-damage`: per-player hit-OUTCOME columns for
    /// both player-side distributions, keyed by the account's
    /// representative agent address. Lands on
    /// `blocks.damage.by_entity[player].by_skill[_taken][].outcomes` --
    /// enriching the rows `skill_damage` already built, and ADDING the
    /// mitigation-only rows it has no entry for (see `merge_outcomes`).
    pub dist_outcomes: Option<
        &'a std::collections::BTreeMap<u64, axilog_core::analysis::dist_outcomes::DistOutcomes>,
    >,
    /// MEIGAP Task 3a `--skill-damage`: the per-ally and per-skill halves of
    /// `healing_detail::build`'s output, positionally joined to
    /// `enc.players`. Lands on `blocks.healing.by_entity[].detail`.
    pub healing_detail: Option<&'a axilog_core::analysis::healing_detail::HealingDetail>,
    /// MEIGAP Task 3a `--timeseries`: the 1S half of that SAME pass's
    /// output. Lands on `blocks.series.by_entity[].healing_1s`.
    ///
    /// Two fields for one pass, deliberately. `healing_detail::build` is the
    /// only pass in this struct that feeds two families under two DIFFERENT
    /// flags -- the ally matrix and dists ride `--skill-damage` for payload
    /// reasons (they grow quadratically in squad size), the graph rides
    /// `--timeseries` with every other per-second array. A single field
    /// could not express "ran, but you were told to emit only the graph", so
    /// the caller sets whichever of the two its own flags justify, and both
    /// point at the same `Vec` when both are on.
    ///
    /// They are borrows rather than the `bool` pair this replaces in
    /// `EiInputs` precisely because a borrow cannot be set without the data:
    /// a flag can claim a pass ran when it did not, a `&HealingDetail`
    /// cannot.
    pub healing_series: Option<&'a axilog_core::analysis::healing_detail::HealingDetail>,
    /// CC-strip-timelines Task 2/4 `--timeseries`: per-player 1-second
    /// lanes for outgoing CC and boon strips in both directions,
    /// positionally joined to `enc.players` exactly like
    /// [`Self::healing_series`]. Lands on
    /// `blocks.series.by_entity[].cc_applied` / `.strips` / `.strips_taken`.
    ///
    /// One pass, one flag, unlike the healing pair above -- every family it
    /// produces is a per-second array, so there is nothing to re-split.
    pub entity_series: Option<&'a axilog_core::analysis::entity_series::EntitySeriesDetail>,
    /// M11 Task 3's activity pass, positionally joined to `enc.players`.
    /// Lands on `blocks.replay.by_entity` -- the ALWAYS-ON half of that
    /// block, which is why this field is unlike every other one here: the
    /// rest name an optional pass, so `None` means "the flag was off", while
    /// this one names a pass every caller already runs unconditionally
    /// (`build_activity_intervals` is cheap by construction -- see its doc
    /// comment). `None` here therefore means a caller that has no log to
    /// scan at all, not a gate.
    pub activity: Option<&'a [axilog_core::analysis::replay::ActivityIntervals]>,
    /// The replay eye-candy families (glider/transformation windows,
    /// capture-point areas, environment decorations). Like
    /// [`Self::activity`] above and unlike every other field here, this
    /// names a pass that is cheap enough to run unconditionally rather than
    /// an opt-in gate -- see
    /// [`axilog_core::analysis::replay_extras`]. `None` therefore means a
    /// caller with no log to scan, not a flag that was off.
    pub replay_extras: Option<&'a axilog_core::analysis::replay_extras::ReplayExtras>,
    /// MEIGAP Task 1b `--timeseries`: per-(player, boon) stack timelines,
    /// total and split by applier. Lands on `blocks.boons`'
    /// `states`/`per_source` row fields -- enriching the rows the always-on
    /// uptime pass already built, which is why `blocks.boons` is a two-gate
    /// block and `coverage.boons` says nothing about these.
    pub boon_states: Option<&'a axilog_core::analysis::buffs::BoonStates>,
    /// MEIGAP Task 2d `--timeseries`: per-(enemy, condition) stack
    /// timelines split by squad applier. Lands on `blocks.conditions`, the
    /// block spec #1 reserved the name for.
    pub target_conditions:
        Option<&'a axilog_core::analysis::target_conditions::TargetConditionStates>,
    /// `--timeseries`: per-(squad player, effect) uptime and fused stack
    /// timelines for the 14 conditions plus Stun and Daze. Lands on
    /// `blocks.self_effects`, the squad-side counterpart to
    /// `blocks.conditions`. ONE gate, unlike `boon_states`: this pass
    /// produces the whole block, so `coverage.self_effects` settles the
    /// question by itself.
    pub self_effects: Option<&'a axilog_core::analysis::self_effects::SelfEffects>,
    /// Squad-side uptime for every buff that is neither a boon nor a
    /// condition/control effect. Lands on `blocks.squad_buffs`.
    ///
    /// ALWAYS-ON, like [`Self::activity`] and unlike the three fields
    /// above it: the pass emits uptime only, which is the cost
    /// `blocks.boons`' always-on half already carries, so no flag gates
    /// it. `None` here therefore means a caller with no log to scan --
    /// [`Passes::default`], or a reprojection driven from a `Report` alone
    /// -- not a gate that was off, and `coverage.squad_buffs` reports
    /// `not_computed` for exactly that case rather than an empty block a
    /// consumer would read as real zeros.
    pub squad_buffs: Option<&'a axilog_core::analysis::squad_buffs::SquadBuffs>,
    /// The attributed incoming-CC pass -- `blocks.cc.taken_events`.
    ///
    /// Gated on `--timeseries`, like the `cc_taken` lane on
    /// `blocks.series` it decomposes: both answer timeline questions and
    /// neither is worth a scan for a caller that only wants totals. The
    /// rest of `blocks.cc` is always-on, so `coverage.cc` cannot speak for
    /// this half; see [`defense::CcBlock::taken_events`] for why the
    /// `Option` on the wire is the gate signal instead.
    pub cc_taken_events: Option<&'a [axilog_core::analysis::cc::CcTakenEvent]>,
    /// Enemy attention drawn per squad player. Lands on `blocks.focus`.
    ///
    /// ALWAYS-ON, like [`Self::activity`] and [`Self::squad_buffs`]: the
    /// pass is one linear scan of the raw event list, so no flag gates it.
    /// `None` here means a caller with no log to scan -- [`Passes::default`],
    /// or a reprojection driven from a `Report` alone -- and
    /// `coverage.focus` reports `not_computed` for exactly that case.
    pub focus: Option<&'a axilog_core::analysis::focus::FocusDetail>,
}

/// Assemble the 1.0 [`ReportV1`] alongside the already-built legacy
/// [`crate::Report`], which keeps this a pure reprojection instead of a
/// second, divergent computation from `Metrics`.
///
/// `enc` is used only for [`build_entities`] -- every block routes through
/// `legacy`/`metrics`/[`Passes`] instead.
pub fn build_report_v1(
    enc: &Encounter,
    metrics: &Metrics,
    legacy: &crate::Report,
    axilog_version: &str,
    generated_from: Option<&str>,
    passes: &Passes<'_>,
) -> ReportV1 {
    let damage_mods = passes.damage_mods;
    let (entities, index, source_order) = build_entities(enc, metrics);
    let mut cats = CatalogBuilder::default();
    let mut coverage = Coverage::new();

    // Always-on blocks. `computed` distinguishes `Present` from `Empty` --
    // see its doc comment for why that distinction has to be made HERE, at
    // the only layer that knows a block both ran and produced nothing.
    let damage_block =
        damage::build_damage(legacy, &index, &mut cats, passes.enemy_dist, passes.dist_outcomes);
    coverage.set(BlockName::Damage, computed(damage_block.is_empty()));
    let defenses = defense::build_defenses(legacy, &index);
    coverage.set(BlockName::Defenses, computed(defenses.is_empty()));
    let hit_stats = defense::build_hit_stats(legacy, &index);
    coverage.set(BlockName::HitStats, computed(hit_stats.is_empty()));
    let mut cc = defense::build_cc(legacy, &index);
    // Set from the always-on half only: `coverage.cc` answers "were there
    // CC rows", which stays true or false regardless of whether the gated
    // `taken_events` pass ran. Its own `Option` carries that question.
    coverage.set(BlockName::Cc, computed(cc.is_empty()));
    cc.taken_events = passes.cc_taken_events.map(|events| {
        defense::build_cc_taken_events(events, &index, enc.log_start_ms, &mut cats)
    });
    let boons = {
        let mut block = support::build_boons(legacy, &index, &mut cats);
        // `coverage.boons` is set from the UPTIME rows, before the gated
        // timelines are attached -- deliberately. The uptime half is
        // always-on, so `Empty` here means "this log had no boon rows",
        // which stays true whether or not `--timeseries` ran. Setting it
        // after the attach would make the same log report differently
        // depending on a flag that cannot change the answer.
        coverage.set(BlockName::Boons, computed(block.is_empty()));
        if let Some(states) = passes.boon_states {
            support::attach_boon_states(&mut block, &index, states);
        }
        block
    };
    let support_block = support::build_support(legacy, &index);
    coverage.set(BlockName::Support, computed(support_block.is_empty()));
    let contribution = support::build_contribution(legacy, &index, &mut cats);
    coverage.set(BlockName::Contribution, computed(contribution.is_empty()));
    let healing =
        support::build_healing(legacy, &index, passes.healing_detail, &mut cats, metrics.healing_extension.as_ref());
    // The one block whose absence has a THIRD cause, and the reason
    // `Unsupported` stops being reserved vocabulary (Task 10).
    //
    // `healing_extension` is a property of the LOG, not of any gate:
    // arcdps only writes the healing extension when the user runs the plugin
    // that produces it, and on a log without it no flag and no future pass
    // can ever produce these numbers. That is exactly `Unsupported` -- and
    // reporting it as `Empty`, which this line did until now, told a
    // consumer the squad healed for zero. `computed`'s own doc comment cites
    // this very case as the reason `Empty` exists, which was right about the
    // ambiguity and wrong about the answer.
    coverage.set(
        BlockName::Healing,
        if metrics.healing_extension.is_some() {
            computed(healing.is_empty())
        } else {
            CoverageState::Unsupported
        },
    );
    // Always-on, but only when the caller had a raw log to scan -- see
    // `Passes::focus`. `None` stays `not_computed` (the `Coverage::new`
    // default), which is why there is no `else` arm here.
    let focus_block = passes.focus.map(|detail| {
        let block = focus::build_focus(detail, legacy, &index, &mut cats);
        coverage.set(BlockName::Focus, computed(block.is_empty()));
        block
    });
    let series = activity::build_series(
        legacy,
        &index,
        passes.health_percents,
        passes.enemy_series,
        passes.healing_series,
        passes.entity_series,
    );
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
    // Which case a `not present` is -- gate off, or gate on and nobody cast
    // -- is answered by `RotationEntity::casts` itself (Task 13), not here.
    let rotation = Some({
        let block = activity::build_rotation(legacy, &index, &mut cats);
        let no_casts =
            block.by_entity.0.values().all(|r| r.casts.as_ref().map_or(true, |c| c.is_empty()));
        coverage.set(BlockName::Rotation, computed(no_casts));
        block
    });
    let damage_mods_block = legacy.damage_mod_map.is_some().then(|| {
        let block = activity::build_damage_mods(legacy, &index, &mut cats, damage_mods);
        coverage.set(BlockName::DamageMods, computed(block.is_empty()));
        block
    });
    let missiles = legacy.missiles.is_some().then(|| {
        let block = activity::build_missiles(legacy, &index);
        coverage.set(BlockName::Missiles, computed(block.is_empty()));
        block
    });
    // Two gates, one block: the intervals half is always computed, the
    // position half rides `--replay` (see `ReplayBlock`'s doc comment). So
    // the block exists when EITHER input does, and `coverage.replay`
    // answers the intervals question -- `Present` on a log parsed with no
    // `--replay` at all, because the down/dead history really is there.
    let replay = (passes.activity.is_some()
        || legacy.replay.is_some()
        || passes.replay_extras.is_some())
    .then(|| {
        let block = activity::build_replay(legacy, &index, passes.activity, passes.replay_extras);
        coverage.set(BlockName::Replay, computed(block.is_empty()));
        block
    });

    // The pass runs only under `--skill-damage`, so its presence IS the
    // gate signal -- the same "presence, not a flag" rule the legacy
    // `Option` blocks above follow.
    let minions_block = passes.minions.map(|rollups| {
        let block = minions::build_minions(rollups, &source_order, &mut cats);
        coverage.set(BlockName::Minions, computed(block.is_empty()));
        block
    });
    if minions_block.is_none() {
        coverage.set(BlockName::Minions, CoverageState::NotComputed);
    }

    // The name spec #1 reserved, filled by Task 12. The pass runs only
    // under `--timeseries`, so its presence IS the gate signal.
    let conditions_block = passes.target_conditions.map(|states| {
        let block = conditions::build_conditions(states, &index, &mut cats);
        coverage.set(BlockName::Conditions, computed(block.is_empty()));
        block
    });
    if conditions_block.is_none() {
        coverage.set(BlockName::Conditions, CoverageState::NotComputed);
    }

    // The squad-side counterpart to `conditions` above, and gated the same
    // way: the pass runs only under `--timeseries`, so its presence IS the
    // gate signal. Unlike `boons`, there is no always-on half here, which
    // is why `coverage.self_effects` answers the whole question.
    let self_effects_block = passes.self_effects.map(|effects| {
        let block = self_effects::build_self_effects(effects, &index, &mut cats);
        coverage.set(BlockName::SelfEffects, computed(block.is_empty()));
        block
    });
    if self_effects_block.is_none() {
        coverage.set(BlockName::SelfEffects, CoverageState::NotComputed);
    }

    // The long tail of `buffUptimes` -- sigils, relics, food, auras. NOT
    // gated (see `Passes::squad_buffs`), so unlike the two blocks above,
    // `None` here is a caller with no log rather than a flag that was off;
    // the coverage spelling is the same either way, because a consumer's
    // question is "may I read this as zero", and the answer is no in both
    // cases.
    let squad_buffs_block = passes.squad_buffs.map(|buffs| {
        let block = squad_buffs::build_squad_buffs(buffs, &index, &mut cats);
        coverage.set(BlockName::SquadBuffs, computed(block.is_empty()));
        block
    });
    if squad_buffs_block.is_none() {
        coverage.set(BlockName::SquadBuffs, CoverageState::NotComputed);
    }

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
        .map(|m| {
            // arcdps reports only the GUID. Lowercased before lookup because
            // GW2EI writes these uppercase in C# while the raw log spells
            // them lowercase -- a case mismatch would miss silently.
            let known = axilog_core::analysis::marker_icons::lookup(
                &m.marker.to_ascii_lowercase(),
            );
            MarkerAssignmentOut {
                entity_id: index.by_agent_addr(m.agent_addr),
                agent_addr: m.agent_addr,
                marker: m.marker.clone(),
                marker_kind: known.map(|k| k.kind),
                marker_label: known.map(|k| k.label),
                marker_icon: known.and_then(|k| k.icon),
                time_ms: m.time_ms,
            }
        })
        .collect();

    // Ground markers reuse the overhead shapes' art: the eight symbols are
    // the same, so `marker_icons` is looked up by LABEL rather than by GUID
    // (a ground marker carries no GUID at all).
    let ground_markers = legacy
        .encounter
        .ground_markers
        .iter()
        .map(|m| GroundMarkerOut {
            index: m.index,
            name: m.name.clone(),
            icon: axilog_core::analysis::marker_icons::MARKERS
                .iter()
                .find(|k| k.kind == "squad_marker" && k.label.eq_ignore_ascii_case(&m.name))
                .and_then(|k| k.icon),
            x: m.x,
            y: m.y,
            z: m.z,
            start_ms: m.start_ms,
            end_ms: m.end_ms,
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
            encounter_name: legacy.encounter.encounter_name.clone(),
            trigger_id: legacy.encounter.trigger_id,
            sub_category: legacy.encounter.sub_category.clone(),
            success: legacy.encounter.success,
            map_id: legacy.encounter.map_id,
            duration_ms: legacy.encounter.duration_ms,
            build: legacy.encounter.build.clone(),
            revision: legacy.encounter.revision,
            recorded_by,
            teams: legacy.encounter.teams.clone(),
            markers,
            ground_markers,
            tick_rate: legacy.encounter.tick_rate.clone(),
            started_at_unix: legacy.encounter.started_at_unix,
            // Sourced from `enc`, not `legacy`: the legacy shape has no
            // `t0` field and gains none here. It is the format this
            // container replaces, and its consumers never had the two
            // absolute-time fields joined to anything.
            log_start_ms: enc.log_start_ms,
            objectives: legacy.encounter.objectives.clone(),
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
            focus: focus_block,
            replay,
            series: Some(series),
            minions: minions_block,
            conditions: conditions_block,
            self_effects: self_effects_block,
            squad_buffs: squad_buffs_block,
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
        let enc = Encounter { log_start_ms: 0, ground_markers: vec![],
            kind: "wvw".into(), pve: None,
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
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
                strips: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
            log_skill_names: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, &Default::default());

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
        let enc = Encounter { log_start_ms: 0,
            kind: "wvw".into(), pve: None,
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
            markers: vec![], ground_markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
                strips: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
            log_skill_names: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, &Default::default());

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
        let enc = Encounter { log_start_ms: 0,
            kind: "wvw".into(), pve: None,
            map: "".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![player(1, ":Squaddie.1")],
            enemies: vec![],
            markers: vec![], ground_markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
        };
        let metrics = Metrics {
            players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
                strips: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
            log_skill_names: Default::default(),
        };
        let legacy =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let v1 = build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, &Default::default());

        assert_eq!(v1.encounter.recorded_by, None);
        assert!(
            !v1.warnings.iter().any(|w| w.code == "recorded_by_unresolved"),
            "absence of a recorder is not a join failure and must not warn"
        );
    }
}
