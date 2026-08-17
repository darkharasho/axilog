pub mod damage;
pub mod downs;
pub mod cc;
pub mod buffs;
pub mod support;
/// arcdps healing-extension stats (M10 Task 1) -- see `healing::apply`'s
/// module doc for the wire format / aggregation writeup.
pub mod healing;
pub mod healing_detail;
pub mod minions;
/// Combat replay position tracks (M9 Task 1) -- standalone from
/// [`analyze`]; see `replay::build_replay`.
pub mod replay;
/// GW2EI-shape fixed-rate combat replay (M15 Task 1) -- the EI-compatible
/// counterpart to `replay` above (map-pixel positions/orientations on
/// GW2EI's own polling grid, plus `dc` sentinel bracketing). Standalone
/// from [`analyze`], same as `replay`; see `ei_replay::build_ei_replay`.
pub mod ei_replay;
/// Glider (`CBTS_GLIDER`) and transformation (`CBTS_TRANSFORMATION`) state
/// intervals -- the replay eye-candy backlog's agent-scoped half. Standalone
/// from [`analyze`], like `replay` above; see `agent_states`'s module doc for
/// why this is decoded despite GW2EI having no consumer for either family.
pub mod agent_states;
/// Capture-point areas (`CBTS_GADGETCAPTURE*`) and the environment combat-
/// replay decorations built from them -- the replay eye-candy backlog's
/// world-scoped half. Standalone from [`analyze`]; see `gadget_capture` and
/// `decorations`.
pub mod gadget_capture;
/// The combat-replay decoration container: shapes with a lifespan, a colour
/// and a world anchor. See `decorations`'s module doc for its scope.
pub mod decorations;
/// The one-call bundle over `agent_states`/`gadget_capture`/`decorations`,
/// for callers that want all three (the CLI and both SDKs do).
pub mod replay_extras;
/// Opt-in missile (projectile) analytics (M10 Task 2) -- standalone from
/// [`analyze`]; see `missiles::build_missiles`.
pub mod missiles;
/// Per-agent health-percent tracking + the M11 contribution-family's
/// "over-99 anchor" query (M11 Task 1) -- standalone from [`analyze`], like
/// `replay`/`missiles` above; see `health::HealthTracker`'s module doc for
/// the ordinal/payload citation trail and the "why not on `Metrics`" design
/// note.
pub mod health;
/// The arcdps-methodology down/CC/strip/movement-impair contribution family
/// (M11 Task 2) -- unlike `health` above, this IS wired into [`analyze`]
/// below (`PlayerMetrics::downs_contribution`/`downed_by`), replacing the
/// retired `downs::apply` 10s-window approximation. See `contribution`'s
/// module doc for the full methodology writeup.
pub mod contribution;
/// Per-skill damage distribution (outgoing + taken, per-target) (M12 Task 1)
/// -- unlike `health`/`replay`/`missiles` above, this IS wired into
/// [`analyze`] below (`PlayerMetrics::skill_damage`), computed unconditionally
/// like every other damage-derived pass (cheap: one extra grouped scan over
/// `raw.events`, reusing `damage`'s own predicate/`InstidRegistry`). See
/// `skill_damage`'s module doc for the full EI-calibration writeup
/// (notably: EI's `totalDamageDist` excludes pet/minion damage entirely,
/// while this DOES fold it onto the owner, same as `PlayerMetrics::
/// damage_total` already does).
pub mod skill_damage;
/// Per-player per-second series (`damage`/`damage_taken`/`per_target`,
/// cumulative) plus `dps_targets` (M12 Task 2) -- like `skill_damage`
/// above, wired into [`analyze`] below (`PlayerMetrics::timeseries`),
/// computed unconditionally (cheap: one extra bucketed scan, reusing
/// `damage`'s predicate and `damage::pet_credit_events`). See
/// `timeseries`'s module doc for the GW2EI cumulative-vs-instant citation
/// and the `dps_targets`-vs-EI-`dpsTargets` scope note.
pub mod timeseries;
/// Incoming-condition attribution on ENEMY agents -- GW2EI's
/// `targets[].buffs[].id`/`.statesPerSource` (MEIGAP Task 2d). A STANDALONE
/// pass (not wired into [`analyze`], like `replay`/`missiles`/`damage_mods`/
/// `buffs::states`), gated by the ei-json adapter on `--timeseries` -- see
/// its module doc for the GW2EI direction citation and the reuse of the boon
/// simulation machinery.
pub mod target_conditions;
/// Outgoing hit-quality stats (M13 Task 1) -- like `skill_damage`/
/// `timeseries` above, wired into [`analyze`] below (`PlayerMetrics::
/// hit_stats`), computed unconditionally (cheap: one extra scan reusing
/// `damage`'s squad/enemy membership predicate, WITHOUT the pet-credit fold
/// -- see `hit_stats`'s module doc for why EI's own `statsAll` is
/// actor-only, unlike `damage_total`/`skill_damage`).
pub mod hit_stats;
/// Incoming defenses -- hit-outcome counts + damage-taken breakdown (M13
/// Task 2) -- like `hit_stats` above, wired into [`analyze`] below
/// (`PlayerMetrics::defenses`), computed unconditionally (cheap: two extra
/// scans over `raw.events` plus a third cast-event scan for `dodge_count`).
/// Purely additive alongside the pre-existing `downs_taken`/`deaths`/
/// `damage_taken`/`cc` fields on `PlayerMetrics` -- see `defenses`'s module
/// doc for the full GW2EI `DefenseAllStatistics` citation trail, notably the
/// `dodge_count` vs `evaded_count` distinction and a real GW2EI counting bug
/// in `LifeLeechDamageTakenCount` this module deliberately does NOT
/// reproduce.
pub mod defenses;
/// Per-player rotation (cast tracking) (M14, Task 1) -- like `skill_damage`/
/// `timeseries`/`hit_stats`/`defenses` above, wired into [`analyze`] below
/// (`PlayerMetrics::rotation`), computed unconditionally (one extra scan
/// over `raw.events`, classifying `is_activation`/`ANIMATION_START`/
/// `ANIMATION_STOP` cast-boundary rows and running a per-skill start/end
/// pairing state machine -- no pet-credit fold, casts belong to the caster
/// only). See `rotation`'s module doc for the full GW2EI
/// `CombatEventFactory.CreateCastEvents`/`AnimatedCastEvent` citation trail
/// and for the `InitCastEvents` merge that folds `instant_cast`'s
/// synthesized casts and the log's `CBTS_WEAPSWAP` rows into the same
/// per-player list.
pub mod rotation;
/// Generated reference tables, not analysis passes: skill art from the GW2
/// API and buff art from GW2EI's own buff list. They are grouped here, ahead
/// of `skill_map`, because nothing in the pipeline computes them -- they are
/// the two lookups that answer what a log cannot say about an id.
pub mod buff_icons;
pub mod skill_icons;
/// Best-effort `skillMap` built from the log's own skill table (M14, Task 2)
/// -- like `hit_stats`/`defenses` above, wired into [`analyze`] below
/// (`Metrics::skill_map`), computed ONCE after every per-player pass
/// (`skill_damage`/`rotation`) has finished, since it's scoped to whatever
/// skill ids those passes actually referenced (not a per-player metric
/// itself, so it lives on `Metrics` directly, alongside
/// `combat_participant_enemies` -- same "derived from already-finished
/// per-player data" placement). See `skill_map`'s module doc for the full
/// name-gap-vs-EI honesty writeup and the `is_swap`/`can_crit`/`auto_attack`
/// citation trail.
pub mod skill_map;

/// Damage modifiers -- GW2EI's "+X% while ..." framework (M16): definition
/// model, evaluation engine and a 205-entry definition catalog.
/// Deliberately NOT wired into [`analyze`], like `replay`/`ei_replay`/
/// `missiles`: it is a separate full pass over every damage event crossed
/// with the whole catalogue, so the caller runs
/// `damage_mods::evaluate_catalog_full` itself only on CLI `--modifiers` /
/// SDK `modifiers: true`, and feeds the result to the native schema
/// (`players[].damage_mods` + `damage_mod_map`) and the ei-json adapter
/// (`damageModifiers`/`incomingDamageModifiers`/`*Target` +
/// `damageModMap`). See its module doc for the exact GW2EI semantics of
/// the four output fields (`hitCount`/`totalHitCount`/`damageGain`/
/// `totalDamage`) and the documented gap list.
pub mod damage_mods;

/// GW2EI's `InstantCastFinder` subsystem (MPROC) -- the machinery behind
/// `skillMap`'s `isTraitProc` / `isGearProc` / `isUnconditionalProc` /
/// `isNotAccurate` / `isInstantCast`. Like `damage_mods`, a definition
/// model plus one engine plus a machine-extracted catalog -- but UNLIKE
/// `damage_mods` it IS wired into [`analyze`], through `skill_map::build`.
/// The five flags are `skillMap` fields, which `analyze` already emits,
/// and the pass is one scan over the event stream plus a per-finder walk
/// of the few streams the catalog names -- not `damage_mods`' whole
/// catalogue crossed with every damage event.
/// See its module doc for what the five flags actually are (not a skill
/// database) and for the effect-finder gap.
pub mod instant_cast;

/// GW2EI's `SkillEvent.ConditionDamageBased` skill-id catalog (MCONDCAT
/// Task 1) -- the `BuffClassification.Condition` id set that decides the
/// condition-vs-power bucketing in `hit_stats` (outgoing) and `defenses`
/// (incoming), and the condition-cleanse id set in `support`. See that
/// module's doc comment for the exhaustive-scan provenance and the proof
/// that GW2EI's runtime `BuffsByIDs` membership can never differ from the
/// static list.
pub mod condition_catalog;

/// Per-(player, target) offensive splits (MEIGAP Task 1d) -- like
/// `hit_stats`/`defenses` above, wired into [`analyze`] below
/// (`PlayerMetrics::per_target`), computed unconditionally: one extra scan
/// reusing `hit_stats::classify` and the same squad/enemy membership
/// predicate, into a SPARSE map (only pairs that actually interacted). See
/// `per_target`'s module doc for the GW2EI `OffensiveStatistics` citation
/// trail and why `downContribution` deliberately lives on the
/// contribution family instead.
pub mod per_target;
/// Per-skill hit-OUTCOME columns for the two player-side damage
/// distributions (MEIGAP2 row 1) -- opt-in, not run by `analyze()`; see
/// `dist_outcomes`'s module doc.
pub mod dist_outcomes;
/// GW2EI's `distToCom`/`stackDist` over the combat replay -- computed by
/// `replay::build_replay`, not by `analyze()`; see `distance`'s module doc.
pub mod distance;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct PlayerMetrics { pub agent_addr: u64, pub damage_total: u64, pub dps: f64,
    pub per_enemy: Vec<(u64,u64)>, pub downs_dealt: u32, pub kills_dealt: u32,
    pub downs_taken: u32, pub deaths: u32,
    pub damage_taken: u64, pub cc_applied: u32, pub cc_duration_ms: u64,
    pub stun_breaks: u32, pub removed_stun_duration_ms: u64,
    /// Condition cleanses / boon strips / resurrects (M3, Task 3) -- see
    /// `support::SupportMetrics`.
    pub support: support::SupportMetrics,
    /// arcdps healing-extension totals (M10 Task 1) -- see
    /// `healing::HealingMetrics`. All-zero (the `Default`) on a log that
    /// doesn't carry the extension at all, OR on a real extension log for a
    /// player who never healed/granted barrier -- `Metrics::
    /// has_healing_extension` is the flag that distinguishes "genuinely
    /// zero" from "extension absent", used to gate schema/warning output.
    pub healing: healing::HealingMetrics,
    /// Outgoing arcdps-methodology contribution toward downing enemy
    /// players (M11 Task 2) -- see `contribution`'s module doc. Replaces
    /// the retired M1-era `down_contribution` 10s-window approximation
    /// (schema 0.1 -> 0.2 bump).
    pub downs_contribution: contribution::ContributionMetrics,
    /// The mirror: what non-squad contributors did to THIS player before
    /// each of their own downs, aggregated onto this row (M11 Task 2).
    pub downed_by: contribution::ContributionMetrics,
    /// Per-skill damage distribution: outgoing (total + per-target) and
    /// incoming, each grouped by skill id (M12 Task 1) -- see
    /// `skill_damage`'s module doc. `sum(skill_damage.outgoing[*].total) ==
    /// damage_total` and `sum(skill_damage.taken[*].total) == damage_taken`
    /// hold exactly by construction.
    pub skill_damage: skill_damage::SkillDamageMetrics,
    /// Per-second cumulative `damage`/`damage_taken`/`per_target` series
    /// plus `dps_targets` (M12 Task 2) -- see `timeseries`'s module doc.
    /// `sum(dps_targets[*].damage)` equals `per_enemy`'s own total, and the
    /// final element of `damage`/`damage_taken` equals `damage_total`/
    /// `damage_taken` exactly, both by construction.
    pub timeseries: timeseries::TimeseriesMetrics,
    /// Outgoing hit-quality stats (M13 Task 1) -- see `hit_stats`'s module
    /// doc. Deliberately does NOT fold pet/minion damage (unlike
    /// `damage_total`/`skill_damage`) -- matches EI's own actor-only
    /// `statsAll[0]` scope.
    pub hit_stats: hit_stats::HitStats,
    /// Incoming defenses: hit-outcome counts + damage-taken breakdown (M13,
    /// Task 2). See `defenses`'s module doc. Purely additive alongside
    /// `downs_taken`/`deaths`/`damage_taken`/`cc` above.
    pub defenses: defenses::DefenseStats,
    /// Per-skill cast list (M14, Task 1) -- see `rotation`'s module doc.
    /// Sorted by skill id ascending; `rotation::total_casts` sums the
    /// per-skill cast counts.
    pub rotation: rotation::RotationMetrics,
    /// Per-(enemy representative id) offensive split (MEIGAP Task 1d) --
    /// see `per_target`'s module doc. Sparse: only enemies this player
    /// actually interacted with. Sums back to `hit_stats.connected_count`/
    /// `against_downed_count` and `downs_dealt`/`kills_dealt` by
    /// construction (identical predicates), minus whatever landed on
    /// agents outside the `enemies` set.
    pub per_target: BTreeMap<u64, per_target::PerTargetOffense>,
    /// Per-(enemy representative id) arcdps-methodology down-contribution
    /// DAMAGE (MEIGAP Task 1d) -- the per-target split of
    /// `downs_contribution.damage`, keyed by the enemy whose down this
    /// player contributed to. Sums back to `downs_contribution.damage`
    /// exactly (same credits, just not collapsed).
    ///
    /// This is the arcdps methodology, NOT GW2EI's own per-target
    /// `downContribution` (damage inside the target's 90%-to-downstate
    /// window, `OffensiveStatistics.cs:85-108`) -- the same deliberate,
    /// documented divergence `downs_contribution` itself already carries;
    /// see `contribution`'s module doc.
    pub downs_contribution_per_target: BTreeMap<u64, u64>,
    /// Applied crowd control split by the enemy it landed on, keyed by
    /// enemy representative id -- EI's `appliedCrowdControl` and
    /// `appliedCrowdControlDuration` in `statsTargets`. `(count,
    /// duration_ms)`, the same pair `CcEntity` carries whole-fight.
    pub cc_per_target: BTreeMap<u64, (u32, u64)>,
    /// The CC half of the down-contribution split, keyed by the DOWNED
    /// enemy's representative id -- EI's
    /// `appliedCrowdControlDownContribution` and its duration pair.
    /// `(count, duration_ms)`. The damage half is
    /// `downs_contribution_per_target` above.
    pub cc_downs_contribution_per_target: BTreeMap<u64, (u32, u64)>,
    /// The same credits again, split by SKILL id instead of by target
    /// (MEIGAP2 row 1) -- GW2EI's `totalDamageDist[][].downContribution`
    /// (`JsonDamageDistBuilder.cs:44-47`, fed by
    /// `OffensiveStatistics.downContributionPerSkillID`). Sums back to
    /// `downs_contribution.damage` exactly, same as
    /// `downs_contribution_per_target` above.
    ///
    /// Carries the SAME two documented divergences the scalar does, and no
    /// new ones: it is the arcdps methodology's window (over-99% anchor
    /// minus a 2s lead-in), not GW2EI's 90%-to-downstate window, and it
    /// inherits `credit_window`'s deliberate breakbar carve-out. Splitting
    /// by skill makes the second one per-skill visible, which is exactly
    /// why it is stated here as well as there.
    pub downs_contribution_per_skill: BTreeMap<u32, u64> }
#[derive(Debug, Clone, Default)]
pub struct Timeline { pub resolution_ms: u64, pub squad_damage: Vec<u64>,
    pub cc_applied: Vec<u32>, pub downs: Vec<u32> }
/// Severity of a [`Warning`]. Mirrors `v1::envelope::Severity`'s three-way
/// split -- kept as an independent enum here so `axilog-core` does not
/// depend on `axilog-schema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity { Info, Warn, Error }
/// A structured, user-facing analysis warning (native format 1.0, Task 9).
/// Replaces the old `Vec<String>` so a consumer can act on `code`
/// programmatically instead of pattern-matching prose. `code` is a closed,
/// documented set: every `warnings.push` site in this crate names a
/// distinct code, and there is deliberately no catch-all -- see this
/// module's `analyze` for the two current producers
/// (`post_rework_zero_buff_events`, `healing_extension_absent`).
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    /// Closed, documented set. Adding a code is additive; changing one's
    /// meaning is a breaking change to the 1.0 format.
    pub code: &'static str,
    pub severity: WarningSeverity,
    pub message: String,
    /// The agent this warning is about, when it is about one. Mapped to an
    /// entity id at 1.0 serialization time.
    pub agent_addr: Option<u64>,
}
#[derive(Debug, Clone, Default)]
pub struct Metrics { pub players: Vec<PlayerMetrics>, pub timeline: Timeline,
    /// Per-(agent representative, boon id) stack-count timelines (M3, Task
    /// 1) -- see `buffs::simulate_boons`. Computed after all other passes,
    ///    not altering any pre-existing metric.
    pub boons: BTreeMap<(u64, u32), buffs::BoonTimeline>,
    /// Per-(agent representative, boon id) uptime summaries (M3, Task 2) --
    /// see `buffs::simulate_boon_uptimes`/`buffs::uptime`. Reduces `boons`
    /// above over the same absolute fight window; does not alter any
    /// pre-existing metric.
    pub boon_uptime: BTreeMap<(u64, u32), buffs::BoonUptime>,
    /// Per-(source representative addr, boon id) self/group/squad
    /// generation-attribution rollups (M3, Task 4) -- see
    /// `buffs::generation`. Reduces the same per-target event streams
    /// `boons`/`boon_uptime` are built from (re-simulated with source
    /// tracking, not derived from `boons` directly -- see
    /// `buffs::generation`'s module doc for why the two simulations must
    /// stay independently verified against each other); does not alter any
    /// pre-existing metric.
    pub boon_generation: BTreeMap<(u64, u32), buffs::GenerationStats>,
    /// Structured, user-facing warnings about this analysis run (final-review
    /// fix wave; downgraded M4 Task 3 once post-rework extraction landed --
    /// see below). Currently populated with exactly one case: the log's
    /// arcdps build is on/after the post-2026-05-01 buff-statechange rework
    /// (`crate::evtc::RawHeader::is_post_buff_rework`) AND zero buff apply/
    /// remove/initial events were extracted for the tracked boons. As of M4
    /// Tasks 1-2, `events::extract_buff_events`/`support::apply` era-dispatch
    /// internally and DO understand the post-rework wire shape (dedicated
    /// `BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`
    /// statechanges, plus the `ANIMATION_START` resurrect-cast gate) -- so
    /// this warning no longer means "unsupported era". It fires only when a
    /// post-era log genuinely yields zero extracted buff events (e.g. a
    /// truncated/filtered log, or one legitimately carrying no boon
    /// activity), which is indistinguishable from a real extraction gap
    /// from data alone -- the build-era check plus zero-events is the
    /// signal; a false positive here is a harmless heads-up, not a wrong
    /// metric. Empty on every other log, including a post-era log that
    /// extracted at least one buff event. Native schema surfaces this as a
    /// top-level `warnings: [...]` array (omitted when empty); the CLI
    /// table view prints it to stderr; `ei-json` has no comparable field so
    /// it's simply not carried over.
    pub warnings: Vec<Warning>,
    /// Whether the arcdps healing extension (M10 Task 1) was present in
    /// this log at all (a valid signature+revision registration row was
    /// found -- see `evtc::ext_healing::healing_extension_present`).
    /// `false` means every player's `PlayerMetrics::healing` is
    /// meaningfully "no data", not "genuinely zero healing" -- native
    /// schema uses this to omit the `healing` block entirely (rather than
    /// emit a block of misleading zeros) and `warnings` carries a matching
    /// "healing extension not present" note in that case.
    pub has_healing_extension: bool,
    /// Combat-participant enemy ids (M10 Task 3), keyed by `Enemy::id`
    /// (representative addr) -- the subset of `enc.enemies` that actually
    /// interacted with the squad in some way. Real WvW logs enumerate every
    /// nearby lootable/tactivator/chest as an "enemy" NPC even though
    /// nothing in the fight ever targets them (a user report against a real
    /// log: "unknown · 391 enemies" was mostly Bags of Loot) -- this is the
    /// data the native `enemies[]` output and HTML team chips filter down
    /// to, so that count reflects real combat participants instead.
    ///
    /// An id is included when the corresponding enemy: (a) is a player
    /// (always kept -- a real opponent this recording just never landed a
    /// hit on is still real, unlike an untouched loot bag), (b) received
    /// nonzero damage from the squad (read directly off `dmg_by_rep` below,
    /// which already folds in squad-pet/minion credit -- so an enemy that
    /// only ever took pet damage still counts, with zero risk of a dangling
    /// `PerEnemyOut.enemy_id` reference, since this is literally the same
    /// data `per_enemy` serializes), (c) dealt nonzero damage to the squad
    /// (e.g. an enemy catapult/siege that hit squad members), or (d)
    /// received CC from the squad. (c) and (d) are cheap direct scans over
    /// `raw.events` purely for list-membership purposes -- they don't feed
    /// any calibrated sum, so they can't move a golden metric.
    ///
    /// Deliberately does NOT touch `enc.enemies` itself, and the EI adapter
    /// (`axilog_ei::to_ei_json`) deliberately does NOT filter by this set:
    /// GW2EI's `targets[]` genuinely has no interaction filter. It has an
    /// ACTOR-KIND filter instead, which is a different question, and
    /// `axilog_schema::build_report`'s `Report::ei_targets` (EI-adapter-only)
    /// answers that one. So the two derived rosters are INDEPENDENT filters
    /// over the same `enc.enemies`: this set drives `Report::enemies` (native
    /// output + HTML chips), and neither is a subset of the other. See both
    /// fields' doc comments for the full writeup.
    pub combat_participant_enemies: BTreeSet<u64>,
    /// `agent representative addr -> arcdps instid` for every squad player
    /// and every enemy (MEIGAP2 row 3) -- GW2EI's `JsonActor.instanceID`
    /// (`JsonActorBuilder.cs:31`, `jsonActor.InstanceID = actor.InstID`).
    ///
    /// Read straight off the ONE `InstidRegistry` `analyze()` already
    /// builds (`damage::InstidRegistry::instid_of`), so this costs no extra
    /// scan over `raw.events`. An addr with no registration at all is
    /// simply absent (rather than reported as instid `0`), so a consumer
    /// can tell "unknown" from "really zero".
    ///
    /// Deliberately keyed by REPRESENTATIVE addr: a relogged account is one
    /// `PlayerMetrics` here where GW2EI would emit two `players[]` entries
    /// with two different instids, so the representative's own instid is
    /// the one carried.
    pub instance_ids: BTreeMap<u64, u16>,
    /// `enemy representative addr -> total OUTGOING health damage that
    /// enemy dealt` (MEIGAP2 row 5) -- GW2EI's `targets[].dpsAll[0].damage`
    /// (`JsonActorBuilder.cs:46` over `SingleActor.GetDamageStats(log,
    /// phase)`, i.e. `GetDamageEvents(null, ...)`).
    ///
    /// GW2EI's set is `log.CombatData.GetDamageData(agent).Where(x =>
    /// !x.ToFriendly)` PLUS every minion's own damage
    /// (`SingleActor.InitDamageEvents`, `SingleActor.cs:727-748`) -- so it
    /// is minion-INCLUSIVE and filtered by the arcdps `iff` byte
    /// (`SkillEvent.ToFriendly => _iff == IFF.Friend`), not by a
    /// destination-side roster test. Reproduced exactly here, including
    /// the minion fold (via the same `InstidRegistry` owner resolution
    /// `damage::accumulate_pet_credit_with_registry` uses for the squad
    /// side).
    ///
    /// Folded into the always-on `combat_participant_enemies` scan rather
    /// than paid for as its own pass (MPERF: that loop already walks every
    /// event with the same statechange/activation/buffremove skip).
    pub enemy_damage_out: BTreeMap<u64, u64>,
    /// Best-effort skillMap (M14, Task 2) -- see `skill_map`'s module doc.
    /// Scoped to only the skill ids squad players' `skill_damage`/
    /// `rotation`/tracked-boons actually reference, not a dump of the whole
    /// log skill table. Always computed (not opt-in) -- see that module's
    /// doc for the measured-modest-size reasoning.
    pub skill_map: skill_map::SkillMap }

pub fn analyze(enc: &Encounter, raw: &RawLog) -> Metrics {
    // MPERF Task 2: the ONE `InstidRegistry` for this whole analysis.
    //
    // `damage::InstidRegistry::build` is a pure function of `raw` -- a full
    // linear scan over every event, building a `BTreeMap<u16, Vec<(u64,
    // u64)>>`. Before this task each consumer built its own, so a single
    // `analyze()` paid for it ~10 times (pet-credit damage, CC pet-credit,
    // the CC timeline, contribution, healing, skill_damage, timeseries, and
    // three buff-event extractions). Since the build depends on nothing but
    // `raw`, every one of those maps was bit-for-bit identical -- so
    // building once here and threading `&InstidRegistry` into each pass is
    // provably output-identical, just without the redundant scans.
    //
    // Every pass below keeps a `raw`-only wrapper of its own (`apply` /
    // `build` / `timeline` / `simulate_boons` / ...) that builds a private
    // registry, for SDK, standalone (`replay`/`missiles`) and test callers
    // that have no registry in hand; `analyze()` deliberately calls the
    // `_with_registry` variants instead.
    let registry = damage::InstidRegistry::build(raw);
    // A friendly account can own several raw agent addrs (relog / build
    // swap within the same recording — arcdps assigns a new addr per
    // login). `squad` is the union of ALL of an account's addrs so no
    // post-relog damage/downs/kills/CC/pets is dropped, while `addr_to_rep`
    // maps every one of those addrs back to the account's single
    // representative `Player.agent_addr` so per-account sums still land on
    // one `PlayerMetrics` entry (Finding #1).
    let squad: BTreeSet<u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    // Enemy players can also be deduped across relogs (Task 4, M2): `enemy`
    // is the union of ALL of an enemy's raw addrs (so combat events against
    // any of them still count), while `enemy_addr_to_rep` maps every one of
    // those addrs back to the representative `Enemy.id` so per-enemy damage
    // maps fold onto a single entry instead of splitting across the relog.
    let enemies: BTreeSet<u64> = enc.enemies.iter()
        .flat_map(|e| e.agent_addrs.iter().copied())
        .collect();
    let enemy_addr_to_rep: BTreeMap<u64, u64> = enc.enemies.iter()
        .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
        .collect();
    let mut dmg = damage::accumulate(&raw.events, &squad, &enemies);
    // WvW: credit friendly pet/minion damage (arcdps attributes it to the
    // pet's own agent) to the owning squad player — see Task 16A.
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, (total, per)) in
        damage::accumulate_pet_credit_with_registry(raw, &registry, &squad, friendly_team, &agent_team)
    {
        let entry = dmg.entry(owner).or_default();
        entry.0 += total;
        for (dst, d) in per {
            *entry.1.entry(dst).or_default() += d;
        }
    }
    // Fold every source addr's damage onto its account representative so a
    // relogged/build-swapped account's damage is summed, not dropped. Also
    // fold each destination addr onto its enemy representative, so a
    // relogged enemy's damage lands on one `per_enemy` entry instead of
    // splitting across their addrs (Task 4, M2).
    let mut dmg_by_rep: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for (addr, (total, per)) in dmg.into_iter() {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        let entry = dmg_by_rep.entry(rep).or_default();
        entry.0 += total;
        for (dst, d) in per {
            let dst_rep = enemy_addr_to_rep.get(&dst).copied().unwrap_or(dst);
            *entry.1.entry(dst_rep).or_default() += d;
        }
    }
    // M10 Task 3: combat-participant enemy filter -- see `Metrics::
    // combat_participant_enemies`'s doc comment for the full design
    // writeup. Built from `dmg_by_rep` (already-folded, pet-credit-inclusive
    // "received damage") plus two cheap list-membership-only raw scans
    // ("dealt damage"/"received CC") that don't feed any calibrated sum.
    let mut combat_participant_enemies: BTreeSet<u64> = enc.enemies.iter()
        .filter(|e| e.is_player)
        .map(|e| e.id)
        .collect();
    for (_total, per) in dmg_by_rep.values() {
        for (&dst_rep, &total) in per {
            if total > 0 {
                combat_participant_enemies.insert(dst_rep);
            }
        }
    }
    // DELIBERATE CARVE-OUT from `damage::is_health_damage_result` (MEIGAP
    // Task 2, review round 1): unlike every health-damage total in this
    // crate, this scan still treats a `DamageResult.BreakbarDamage` row
    // (result 10) as participation. That is on purpose -- the question here
    // is "did the squad interact with this agent at all", not "did it deal
    // health damage", and a defiance-bar hit is interaction. The visible
    // consequence is that an enemy struck ONLY for breakbar damage stays in
    // `Metrics::combat_participant_enemies` (and therefore in the native
    // `Report::enemies` list) where a strict health-damage reading would
    // drop it. Recorded so the two definitions of "countable damage" in
    // this crate are not silent; see also `contribution::credit_window`,
    // the other carve-out.
    //
    // **MSMALL item 5 re-examined this and DELIBERATELY KEPT IT.** Measured
    // by applying `is_health_damage_result` here and diffing the full
    // `parse` output: ZERO changed bytes on `fixtures/wvw-small.anon.zevtc`
    // AND zero on the local post-rework capture. So no fixture
    // discriminates between the two readings, and there is no measurement
    // pushing either way -- which leaves the semantic argument, and that
    // argument favours keeping it: this set answers "did the squad interact
    // with this agent", and a defiance-bar hit is interaction. Adding the
    // filter would change behaviour only on some future log where an enemy
    // is struck for breakbar damage and nothing else -- exactly the case
    // the carve-out exists to get right -- in exchange for no present
    // benefit. Unlike `contribution::credit_window` (which MSMALL DID
    // sweep, because health damage is causally required for a down), the
    // two definitions of countable damage differ here for a reason.
    //
    // The two carve-outs therefore resolved differently, and that is the
    // point: "countable damage" is not one question.
    // MEIGAP2 row 5: enemy OUTGOING health damage, folded into this same
    // scan -- see `Metrics::enemy_damage_out`'s doc comment for the GW2EI
    // definition (minion-inclusive, `iff`-filtered).
    let mut enemy_damage_out: BTreeMap<u64, u64> = BTreeMap::new();
    // Damage from agents that are neither squad nor a known enemy -- an
    // enemy's minion, until proven otherwise (resolved after the scan).
    let mut unowned_damage: BTreeMap<u64, u64> = BTreeMap::new();
    let mut unowned_link: BTreeMap<u64, (u16, u64)> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if e.result == crate::evtc::result::CROWD_CONTROL {
            // Received CC: squad -> enemy.
            if squad.contains(&e.src_agent) {
                if let Some(&rep) = enemy_addr_to_rep.get(&e.dst_agent) {
                    combat_participant_enemies.insert(rep);
                }
            }
            continue;
        }
        let d = if e.buff == 1 { e.buff_dmg.max(0) } else { e.value.max(0) };
        if d == 0 { continue; }
        // Dealt damage: enemy -> squad.
        if squad.contains(&e.dst_agent) {
            if let Some(&rep) = enemy_addr_to_rep.get(&e.src_agent) {
                combat_participant_enemies.insert(rep);
            }
        }
        // MEIGAP2 row 5: the same rows, summed per enemy. Unlike the
        // membership test above this one applies GW2EI's own two filters
        // (`is_health_damage_result` -- `damage::accumulate`'s predicate,
        // which drops the breakbar/CC result bytes GW2EI routes away from
        // `HealthDamageEvent` -- and `!ToFriendly`, the raw `iff` byte),
        // and it credits an enemy's MINION to the enemy, exactly as
        // `SingleActor.InitDamageEvents` folds `mins.GetDamageEvents` in.
        if e.iff == 0 || !damage::is_health_damage_result(e.result) { continue; }
        if squad.contains(&e.src_agent) {
            continue;
        }
        // Parked by SOURCE ADDR and attributed after the scan, because
        // ownership has to be an AGENT-level fact rather than a per-row
        // one -- the lesson MEIGAP Task 3b's `minions::build` records at
        // length: arcdps emits many of a minion's rows with
        // `src_master_instid == 0`, so a per-row master test silently
        // drops them (measured there: 49 of 586 rows). It also matters in
        // the other direction here: this project's enemy roster enumerates
        // an enemy's pets as enemies of their own, so crediting by
        // `src_agent`'s own roster entry would leave a ranger's pet damage
        // on the pet, where GW2EI folds it onto the ranger
        // (`SingleActor.InitDamageEvents`). Parking keeps the whole thing
        // in ONE pass, rather than the extra full scan `minions::build`
        // can afford as an opt-in builder.
        *unowned_damage.entry(e.src_agent).or_default() += d as u64;
        if e.src_master_instid != 0 {
            unowned_link.entry(e.src_agent).or_insert((e.src_master_instid, e.time));
        }
    }
    for (addr, dmg) in unowned_damage {
        // A recorded master link wins over the agent's own roster entry;
        // an agent with no link is its own owner.
        let owner = unowned_link
            .get(&addr)
            .and_then(|&(instid, time)| registry.resolve_at(instid, time))
            .unwrap_or(addr);
        if let Some(&rep) = enemy_addr_to_rep.get(&owner) {
            *enemy_damage_out.entry(rep).or_default() += dmg;
        }
    }

    let damage_taken = damage::accumulate_damage_taken(&raw.events, &squad);
    let mut taken_by_rep: BTreeMap<u64, u64> = BTreeMap::new();
    for (addr, total) in damage_taken {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        *taken_by_rep.entry(rep).or_default() += total;
    }
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut players: Vec<PlayerMetrics> = enc.players.iter().map(|p| {
        let (total, per) = dmg_by_rep.get(&p.agent_addr).cloned().unwrap_or_default();
        let taken = taken_by_rep.get(&p.agent_addr).copied().unwrap_or(0);
        PlayerMetrics { agent_addr: p.agent_addr, damage_total: total,
            dps: total as f64 / secs, damage_taken: taken,
            per_enemy: per.into_iter().collect(), ..Default::default() }
    }).collect();
    downs::apply_with_registry(&mut players, enc, raw, &registry, &squad, &enemies, &addr_to_rep);
    cc::apply_cc_with_registry(&mut players, raw, &registry, &squad, &enemies, &addr_to_rep, &enemy_addr_to_rep);
    support::apply(&mut players, raw, enc, &enemies, &addr_to_rep);
    // M11 Task 2: the arcdps-methodology contribution family
    // (downs_contribution/downed_by) -- see `contribution`'s module doc.
    contribution::apply_with_registry(&mut players, raw, &registry, enc, &squad, &enemies, &addr_to_rep, &enemy_addr_to_rep);
    // M10 Task 1: cheap (a handful of linear scans over `raw.events`), so
    // computed unconditionally like every other pass above -- returns
    // whether the extension was present at all (see `Metrics::
    // has_healing_extension`'s doc comment for why that matters beyond just
    // "were all totals zero").
    let has_healing_extension =
        healing::apply_with_registry(&mut players, raw, &registry, &squad, &addr_to_rep);
    // M12 Task 1: per-skill damage distribution -- a grouped refinement of
    // the `dmg_by_rep`/`taken_by_rep` totals already computed above (same
    // predicate + `InstidRegistry` pet-fold, just also keyed by `skillid`),
    // not an independent computation -- see `skill_damage`'s module doc.
    let skill_damage_by_rep =
        skill_damage::build_with_registry(raw, &registry, &squad, &enemies, &addr_to_rep, &enemy_addr_to_rep, friendly_team, &agent_team);
    for p in &mut players {
        if let Some(sd) = skill_damage_by_rep.get(&p.agent_addr) {
            p.skill_damage = sd.clone();
        }
    }
    // M12 Task 2: per-player per-second series + dps_targets -- another
    // grouped/bucketed refinement of the same predicate family (see
    // `timeseries`'s module doc), not an independent computation.
    let timeseries_by_rep = timeseries::build_with_registry(
        enc, raw, &registry, &squad, &enemies, &addr_to_rep, &enemy_addr_to_rep, friendly_team,
        &agent_team,
    );
    for p in &mut players {
        if let Some(ts) = timeseries_by_rep.get(&p.agent_addr) {
            p.timeseries = ts.clone();
        }
    }
    // M13 Task 1: outgoing hit-quality stats -- a classification-only pass
    // over the same `squad -> enemies` event family (no pet-credit fold,
    // see `hit_stats`'s module doc).
    let hit_stats_by_rep = hit_stats::build(raw, &squad, &enemies, &addr_to_rep);
    for p in &mut players {
        if let Some(hs) = hit_stats_by_rep.get(&p.agent_addr) {
            p.hit_stats = *hs;
        }
    }
    // M13 Task 2: incoming defenses -- the mirror-image classification pass
    // (see `defenses`'s module doc). No `enemies` set needed: unlike
    // `hit_stats`, this is scoped to ANY source hitting a squad player, same
    // as `damage::accumulate_damage_taken`.
    let defenses_by_rep = defenses::build_with_registry(raw, &registry, &squad, &addr_to_rep);
    for p in &mut players {
        if let Some(d) = defenses_by_rep.get(&p.agent_addr) {
            p.defenses = *d;
        }
    }
    // MEIGAP Task 1d: per-(player, target) offensive splits -- one more
    // pass over the same `squad -> enemies` event family, reusing
    // `hit_stats::classify` (see `per_target`'s module doc).
    let per_target_stats =
        per_target::build(raw, &registry, &squad, &enemies, &addr_to_rep, &enemy_addr_to_rep);
    let player_idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    for ((src, dst), stats) in per_target_stats {
        if let Some(&i) = player_idx.get(&src) {
            players[i].per_target.insert(dst, stats);
        }
    }
    // The ONE instant-cast finder pass for this whole analysis. Two
    // consumers need it and it is not cheap (an effect-keyed finder forces
    // the effect decode, thousands of rows on a WvW log), so it runs here
    // and both take a slice: `rotation::build` for the instant half of
    // GW2EI's `InitCastEvents` merge, `skill_map::build` for the
    // `is_instant_cast` flag.
    let instant_finders: Vec<instant_cast::FinderDef> =
        instant_cast::catalog::all().into_iter().copied().collect();
    let instants = instant_cast::compute(raw, enc, &instant_finders);
    // M14 Task 1: per-player rotation (cast tracking) -- see `rotation`'s
    // module doc. No `squad`/`enemies` filter needed beyond `addr_to_rep`
    // itself (only registered for squad player addrs).
    let rotation_by_rep = rotation::build(raw, &addr_to_rep, &instants);
    for p in &mut players {
        if let Some(r) = rotation_by_rep.get(&p.agent_addr) {
            p.rotation = r.clone();
        }
    }
    let timeline = cc::timeline_with_registry(enc, raw, &registry, &squad, &enemies);
    // Computed last, after every other pass, per the Task 1 brief -- does
    // not read or alter `players`/`timeline` above.
    // MPERF Task 3: the ONE boon extraction for this whole analysis.
    //
    // Both boon simulations below (`simulate_boons` for stack-count
    // timelines, `generation::simulate_boon_generation_ms` for per-source
    // attribution) plus the post-rework "zero buff events" warning check
    // further down all start from the *same* extracted buff-event vector and
    // the *same* arcdps capacity map -- three identical full scans over
    // `raw.events` before this task. Extracting once here and lending
    // `&BoonInputs` to all three is provably output-identical: the two
    // simulations themselves are untouched and stay independent (see
    // `buffs::generation`'s module doc for why that independence matters),
    // they just stop recomputing bit-identical input. See
    // `buffs::BoonInputs`'s doc comment.
    let boon_inputs = buffs::extract_boon_inputs_with_registry(raw, &registry);
    let boons = buffs::simulate_boons_with_inputs(raw, &boon_inputs, enc);
    // M3 Task 2: reduce each timeline to a `BoonUptime` over the same
    // absolute window `simulate_boons` itself ticks against (see
    // `buffs::uptime`'s module docs) -- computed directly from `boons`
    // rather than via `buffs::simulate_boon_uptimes` (which exists as a
    // standalone convenience for callers that only want uptimes) to avoid
    // re-running the simulator a second time.
    let log_start_ms = raw.log_start_ms();
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    let boon_uptime = boons
        .iter()
        .map(|(&key, tl)| (key, buffs::uptime::compute(tl, log_start_ms, log_end_ms)))
        .collect();
    // M3 Task 4: self/group/squad generation attribution, over the same
    // absolute window as `boon_uptime` above (see `buffs::generation`'s
    // module docs) -- re-simulated with per-stack source tracking rather
    // than derived from `boons` (which only tracks stack COUNT, not WHICH
    // source's stack is held).
    //
    // MSMALL item 2: the SAME pass now also returns per-source WASTED ms
    // (boon-time a source generated that was destroyed before the target
    // could spend it) -- see `buffs::generation::WasteRecord` for the three
    // GW2EI sites that produce it. One pass, so generation and waste can
    // never describe different simulations.
    let (target_gen, target_waste) =
        buffs::generation::simulate_boon_generation_and_waste_ms(raw, &boon_inputs, enc);
    let boon_generation = buffs::generation::rollup_with_waste(
        &target_gen,
        &target_waste,
        enc,
        log_start_ms,
        log_end_ms,
    );
    // M4 Task 3 (downgraded from the final-review fix wave's unconditional
    // warning): post-era extraction now works (M4 Tasks 1-2 era-gated
    // `events::extract_buff_events`/`support::apply` -- see `Metrics::
    // warnings`'s doc comment), so only warn when a post-2026-05-01 build
    // genuinely yields zero extracted buff events -- a truncated/filtered
    // log, or a legitimate no-boon-activity fight, rather than a
    // known-unsupported era. Reads the shared `boon_inputs` extraction above
    // (MPERF Task 3) rather than re-extracting or threading a count out of
    // `buffs::simulate_boons`, which already discards per-owner
    // squad-membership before this point: `boon_inputs.events` is literally
    // the vector the old third `extract_buff_events_with_registry(raw,
    // &registry, BOON_IDS)` call here produced, so `is_empty()` on it is the
    // same predicate.
    let mut warnings = Vec::new();
    if raw.header.is_post_buff_rework() && boon_inputs.events.is_empty() {
        warnings.push(Warning {
            code: "post_rework_zero_buff_events",
            severity: WarningSeverity::Warn,
            message: format!(
                "no buff events found in this post-2026-05-01 log (build {}); boon/support metrics will read zero",
                raw.header.build
            ),
            agent_addr: None,
        });
    }
    if !has_healing_extension {
        warnings.push(Warning {
            code: "healing_extension_absent",
            severity: WarningSeverity::Info,
            message: "healing extension not present in this log".to_string(),
            agent_addr: None,
        });
    }
    // M14 Task 2: best-effort skillMap, scoped to whatever `skill_damage`/
    // `rotation` (both already populated on `players` above) actually
    // referenced, plus the always-tracked boon ids -- see `skill_map`'s
    // module doc.
    let skill_map = skill_map::build(raw, &players, &instants);
    // MEIGAP2 row 3: `instanceID`, read off the registry built at the top
    // of this function -- no extra scan (see `Metrics::instance_ids`).
    let instance_ids: BTreeMap<u64, u16> = enc
        .players
        .iter()
        .map(|p| p.agent_addr)
        .chain(enc.enemies.iter().map(|e| e.id))
        .filter_map(|addr| registry.instid_of(addr).map(|instid| (addr, instid)))
        .collect();
    Metrics { players, timeline, boons, boon_uptime, boon_generation, warnings, has_healing_extension,
        combat_participant_enemies, instance_ids, enemy_damage_out, skill_map }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time: 0, src_agent: src, dst_agent: dst, value: dmg, buff_dmg: 0,
            overstack: 0, skillid: 1, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog { header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![] }
    }

    /// Finding #1: a relogged/build-swapped account (two raw agent addrs,
    /// one `Player` after dedupe) must have damage from BOTH addrs summed
    /// into the single surviving `PlayerMetrics` entry, not dropped.
    #[test]
    fn relog_damage_aggregates_across_all_account_addrs() {
        let player = Player {
            agent_addr: 1, // representative (first-seen addr)
            account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![1, 2], // pre-relog addr 1, post-relog addr 2
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None,
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), agent_addrs: vec![9] }],
            markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        };
        let raw = raw_from(vec![
            strike(1, 9, 100), // pre-relog damage from addr 1
            strike(2, 9, 250), // post-relog damage from addr 2 (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players.len(), 1);
        assert_eq!(metrics.players[0].agent_addr, 1);
        assert_eq!(metrics.players[0].damage_total, 350); // 100 + 250, not just one addr
    }

    /// Finding #3: damage_taken sums incoming damage (from any source) for
    /// every one of an account's addrs, folded onto the representative.
    #[test]
    fn damage_taken_sums_incoming_damage_across_relog_addrs() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![1, 2],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None,
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), agent_addrs: vec![9] }],
            markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        };
        let raw = raw_from(vec![
            strike(9, 1, 80),  // enemy hits pre-relog addr
            strike(9, 2, 120), // enemy hits post-relog addr (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players[0].damage_taken, 200);
    }

    /// Task 4 (M2): a deduped enemy (e.g. a relogging enemy player, with
    /// `agent_addrs` covering both its raw addrs) must fold damage sent to
    /// EITHER addr into a single `per_enemy` entry keyed by the
    /// representative id -- not split into two rows, and not dropped for
    /// the non-representative addr.
    #[test]
    fn per_enemy_damage_folds_across_deduped_enemy_addrs() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![1],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            // Enemy deduped from a relog: representative addr 9, but also
            // covers raw addr 10 (the post-relog addr).
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(),
                is_player: true, marker: None,
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                agent_addrs: vec![9, 10] }],
            markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        };
        let raw = raw_from(vec![
            strike(1, 9, 100),  // damage to the enemy's pre-relog addr
            strike(1, 10, 250), // damage to the enemy's post-relog addr
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players.len(), 1);
        assert_eq!(metrics.players[0].damage_total, 350, "both addrs' damage counted");
        assert_eq!(metrics.players[0].per_enemy.len(), 1, "folded into one per_enemy entry");
        assert_eq!(metrics.players[0].per_enemy[0], (9, 350), "keyed by the representative enemy id");
    }

    /// M10 Task 3: `Metrics::combat_participant_enemies` keeps every enemy
    /// that interacted with the squad in any of the three documented ways
    /// (received damage, dealt damage, received CC), keeps enemy players
    /// unconditionally, and drops a zero-interaction NPC/gadget (the
    /// "unknown · 391 enemies was mostly Bags of Loot" bug this task
    /// fixes). Five enemies, one per case (plus the always-kept player).
    #[test]
    fn combat_participant_enemies_keeps_only_interacting_npcs_and_all_players() {
        use crate::evtc::result;
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![1],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![
                // Received damage from the squad.
                Enemy { id: 9, instid: 0, name: "TookDamage".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![9] },
                // Dealt damage to the squad (e.g. an enemy catapult).
                Enemy { id: 10, instid: 0, name: "DealtDamage".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![10] },
                // Received CC from the squad, no damage either direction.
                Enemy { id: 11, instid: 0, name: "TookCc".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![11] },
                // Zero interaction -- the loot-bag/tactivator/chest case.
                Enemy { id: 12, instid: 0, name: "LootBag".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![12] },
                // Zero interaction, but a real enemy PLAYER -- always kept.
                Enemy { id: 13, instid: 0, name: "UntouchedFoe".into(), team: "blue".into(),
                    is_player: true, marker: None,
                    profession: Some("Guardian".into()), elite_spec: Some("Firebrand".into()),
                    agent_addrs: vec![13] },
            ],
            markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        };
        fn cc_strike(src: u64, dst: u64, duration_ms: i32) -> RawEvent {
            let mut e = strike(src, dst, duration_ms);
            e.result = result::CROWD_CONTROL;
            e
        }
        let raw = raw_from(vec![
            strike(1, 9, 500),   // squad -> enemy 9: received damage
            strike(10, 1, 300),  // enemy 10 -> squad: dealt damage
            cc_strike(1, 11, 1500), // squad -> enemy 11: received CC
        ]);
        let metrics = analyze(&enc, &raw);
        let ids = &metrics.combat_participant_enemies;
        assert!(ids.contains(&9), "received damage must count");
        assert!(ids.contains(&10), "dealt damage must count");
        assert!(ids.contains(&11), "received CC must count");
        assert!(!ids.contains(&12), "zero-interaction NPC must be excluded");
        assert!(ids.contains(&13), "enemy players are always kept, even at zero interaction");
        assert_eq!(ids.len(), 4);
    }

    fn empty_enc() -> Encounter {
        Encounter { kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![], enemies: vec![],
            markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None }
    }

    /// Final-review fix wave (downgraded M4 Task 3): a log whose arcdps
    /// build is on/after the post-2026-05-01 buff-statechange rework, with
    /// ZERO extracted buff events (genuinely absent buff data -- e.g. a
    /// truncated/filtered log; there are no boon skillids in the event list
    /// below), must surface a non-empty `warnings` list naming the build.
    #[test]
    fn post_rework_build_with_no_buff_events_warns() {
        let raw = RawLog {
            header: RawHeader { build: "20260601".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events: vec![], guid_map: vec![],
        };
        let metrics = analyze(&empty_enc(), &raw);
        assert!(!metrics.warnings.is_empty(), "post-rework build with zero buff events must warn");
        assert_eq!(metrics.warnings[0].code, "post_rework_zero_buff_events");
        assert!(metrics.warnings[0].message.contains("20260601"), "warning should name the offending build");
        assert!(
            metrics.warnings[0].message.contains("no buff events found"),
            "warning should use the M4 Task 3 downgraded message, got {:?}",
            metrics.warnings[0]
        );
        // M10 Task 1: this synthetic log has no healing-extension
        // registration row either, so the healing-absent warning is
        // appended AFTER the buff warning (order: existing warnings first,
        // healing note last -- `warnings[0]` above stays the buff message).
        assert_eq!(metrics.warnings.len(), 2, "expected both the buff and healing warnings: {:?}", metrics.warnings);
        assert_eq!(metrics.warnings[1].code, "healing_extension_absent");
        assert!(metrics.warnings[1].message.contains("healing extension not present"));
    }

    /// M4 Task 3: a post-2026-05-01 build that DOES carry extracted buff
    /// events (a real `sc::BUFF_APPLY` statechange for a tracked boon, era-
    /// dispatched by `events::extract_buff_events_post_era` -- M4 Task 1)
    /// must NOT warn -- post-era extraction works, so this is the ordinary
    /// case for a real post-rework capture, not the genuinely-absent-data
    /// case the warning exists for.
    #[test]
    fn post_rework_build_with_buff_events_does_not_warn() {
        use crate::evtc::sc;
        let raw = RawLog {
            header: RawHeader { build: "20260601".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![],
            events: vec![RawEvent {
                time: 0, src_agent: 1, dst_agent: 2, value: 5000, buff_dmg: 0,
                overstack: 0, skillid: buffs::MIGHT, src_instid: 0, dst_instid: 0,
                src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 1, result: 0,
                is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::BUFF_APPLY,
                is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
            }],
            guid_map: vec![],
        };
        let metrics = analyze(&empty_enc(), &raw);
        // M10 Task 1: this synthetic log still has no healing-extension
        // registration row, so it now warns about that (the buff-events
        // warning itself correctly stays absent, which is what this test
        // guards).
        assert_eq!(metrics.warnings.len(), 1);
        assert_eq!(metrics.warnings[0].code, "healing_extension_absent");
        assert_eq!(
            metrics.warnings[0].message, "healing extension not present in this log",
            "post-rework build with a real extracted buff event must not warn about buffs, \
             but must still warn about the absent healing extension, got {:?}",
            metrics.warnings
        );
    }

    /// A pre-rework build produces no BUFF warnings (the ordinary case for
    /// every log this project currently supports), even with zero buff
    /// events -- it still warns about the absent healing extension (M10
    /// Task 1), since this synthetic log has no registration row either.
    #[test]
    fn pre_rework_build_no_warnings() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events: vec![], guid_map: vec![],
        };
        let metrics = analyze(&empty_enc(), &raw);
        assert_eq!(metrics.warnings.len(), 1);
        assert_eq!(metrics.warnings[0].code, "healing_extension_absent");
        assert_eq!(metrics.warnings[0].message, "healing extension not present in this log");
    }
}
