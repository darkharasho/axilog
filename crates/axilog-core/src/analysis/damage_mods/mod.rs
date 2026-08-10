//! Damage modifiers -- definition framework and evaluation engine (M16
//! Task 1), definition catalog (Task 2), emission (Task 3).
//!
//! This is the Rust counterpart of GW2EI's
//! `GW2EIEvtcParser/EIData/DamageModifiers/` subsystem: the "+X% while
//! under condition Y" bookkeeping that produces EI's per-player
//! `damageModifiers` / `incomingDamageModifiers` blocks and the top-level
//! `damageModMap` descriptor table.
//!
//! **Emission is opt-in, and the flag gates the COMPUTATION.** Unlike
//! `rotation`/`skill_damage`/`timeseries` -- which `analyze()` computes
//! unconditionally, their flags deciding only whether the schema copies
//! them -- nothing here runs unless the caller asks: `analyze()` never
//! touches this module. On CLI `--modifiers` / SDK `modifiers: true` the
//! caller runs `evaluate_catalog_full` itself and hands the result to
//! `axilog_schema::build_report` (which fills `players[].damage_mods` and
//! the top-level `damage_mod_map`) and to `axilog_ei::EiInputs::modifiers`
//! (which fills EI's `damageModifiers`/`incomingDamageModifiers`/
//! `damageModifiersTarget`/`incomingDamageModifiersTarget` +
//! `damageModMap`). Without the flag every output is byte-identical to a
//! build without this module at all. The per-target split is a second,
//! internal gate -- see `evaluate_full`. (Plain backticks, not intra-doc
//! links: this `//!` block is merged with the outer `/// ` comment on
//! `analysis::mod`'s `pub mod damage_mods;`, so rustdoc resolves its links
//! in the PARENT module's scope -- which is why every `[`model::...`]`
//! link further down this doc is already an unresolved-link warning.)
//!
//! See the `catalog` module for what the table covers and, just as
//! importantly, the definitions it deliberately does NOT carry.
//!
//! # Spec gating
//!
//! A definition is only offered to actors GW2EI would offer it to:
//! `SingleActorDamageModifierHelper.cs:67-88` unions the four universal
//! sources (`Item`, `Gear`, `Common`, `EncounterSpecific`) with
//! `GetPersonalOutgoingModifiersPerSpec(Spec)`, and
//! `ParserHelper.SpecToSources` (`:401-460`) maps a spec to exactly its
//! base profession plus its elite spec. Without this every Firebrand would
//! be credited with `Empowered` (a Warrior trait) on every hit -- see
//! `model::ModSource::applies_to`.
//!
//! # The four fields, exactly
//!
//! Everything below is `GW2EIEvtcParser/EIData/Actors/ActorsHelper/
//! SingleActorDamageModifierHelper.cs`, `ComputeDamageModifierStats`
//! (`:13-45`, incoming twin `:130-163`):
//!
//! ```text
//! foreach (var damageEvent in eventsToUse) { sum += damageEvent.DamageGain; count++; }
//! int totalDamage = damageMod.GetTotalDamage(this, log, target, start, end);
//! var typeHits    = damageMod.GetDamageEvents(this, log, target, start, end);
//! res[pair.Key]   = new DamageModifierStat(count, typeHits.Count, sum, totalDamage);
//! ```
//!
//! - **`hit_count`** -- how many hits QUALIFIED: the gain came out non-zero
//!   AND every `_dlChecker` predicate passed. (`DamageModifierEvent`s are
//!   only created for those; e.g. `BuffOnActorDamageModifier.cs:97-99`,
//!   `if (ComputeGain(bgms, evt, log, out double gain) && CheckCondition(evt,
//!   log)) res.Add(...)`.)
//! - **`total_hit_count`** -- how many hits were ELIGIBLE, recomputed
//!   independently from `GetDamageEvents`. This is the modifier's candidate
//!   pool, and it is NOT "all damage":
//!   - only CONNECTED hits (`Actor.cs:190-202`, `.Where(x => x.HasHit)`);
//!     misses/blocks/evades/invulns are out (absorbed hits are in only for
//!     the `UsingHitAndAbsorbedDamageEvents()` modifiers, `Actor.cs:233`),
//!   - filtered by the modifier's **`src_type`**
//!     ([`model::DamageType`], `Actor.FilterDamageEvents`, `:446-479`) --
//!     so condition ticks and life-leech ticks ARE eligible whenever the
//!     modifier's `src_type` admits them, and are excluded when it is
//!     `Strike`,
//!   - filtered by **`dmg_src`** ([`model::DamageSource`]): minion damage is
//!     included for `All`, excluded for `NoPets`, and exclusive for
//!     `PetsOnly` (`SingleActor.cs:781-845`).
//! - **`damage_gain`** -- `sum(gain_i * damage_i)` over the qualifying hits,
//!   rounded to 3 decimals (`DamageModifierStat.cs:14`,
//!   `Math.Round(damageGain, ParserHelper.DamageModGainDigit)`,
//!   `DamageModGainDigit = 3`). For a MULTIPLIER modifier `gain_i` is
//!   `g/(100+g)`-shaped, i.e. the share of the observed (already-boosted)
//!   damage the modifier is responsible for -- consumers recover the
//!   effective bonus as `totalDamage/(totalDamage - damageGain) - 1`
//!   (`tmplDamageModifierTable.html:324`). For a counter or a skill-based
//!   modifier `gain_i == 1`, so `damage_gain` is raw "damage during".
//! - **`total_damage`** -- the DENOMINATOR, from `GetTotalDamage`, filtered
//!   by **`compare_type`** (NOT `src_type`; the two are routinely different)
//!   and by `dmg_src`. One GW2EI quirk is reproduced deliberately:
//!   `PetsOnly` reports the actor+minion total, because GW2EI's
//!   minions-only subtraction is commented out (`:35-36, 83-84`). Its
//!   bucket construction mirrors `DamageStatistics.ComputeDamageFrom`
//!   branch for branch -- see [`DamageSums::add`], which documents why
//!   those buckets deliberately do NOT use the same predicates
//!   `DamageType::keeps` does.
//!
//! An entry exists only when the modifier produced at least one qualifying
//! hit: GW2EI's loop iterates the per-modifier event dictionary, so a
//! modifier with zero events is simply absent from the JSON
//! (`JsonDamageModifierDataBuilder.cs:43-76`; the HTML builder separately
//! substitutes a zero row, `DamageModData.cs:33-40`, which the JSON one does
//! not).
//!
//! # Scope choices / documented gaps
//!
//! - **Damage pool.** Outgoing = this project's established squad-source ->
//!   enemy-destination predicate (the one `analysis::damage`/`hit_stats`
//!   already use); incoming = any damage taken by a squad member from a
//!   non-squad source (`analysis::damage::accumulate_damage_taken`'s
//!   scope). GW2EI phrases the same thing as "the actor's damage events
//!   against all foes", over the whole log with `target = null`
//!   (`BuffOnActorDamageModifier.cs:76`) -- there are no phases here, so the
//!   whole log IS the single phase.
//! - **Shield damage.** GW2EI's `HealthDamage` is shield-adjusted
//!   (`HealthDamageEvent.cs:30`, `Max(HealthDamage - ShieldDamage, 0)`);
//!   nothing in this project decodes shield damage yet (see
//!   `analysis::skill_damage`'s note on the omitted `shieldDamage` field),
//!   so both the gain and the denominator here use the same unadjusted
//!   damage every other pass in this crate uses. Consistent, and a known
//!   divergence on barrier-heavy targets.
//! - **`WithBuffOnActorFromFoe` / `WithBuffOnFoeFromActor`.** GW2EI can ask
//!   for "the stacks of B on A that were applied BY the foe"
//!   (`BuffOnActorDamageModifier.cs:42`) and its mirror
//!   (`BuffOnFoeDamageModifier.cs:53`). Both are real -- `DeadeyeHelper.cs`
//!   and `SoulbeastHelper.cs` for the first, `SpellbreakerHelper.cs:57,60`
//!   and `AntiquaryHelper.cs:56` for the second. This project's buff
//!   extraction records the applier but has no per-applier stack
//!   simulation, so BOTH are expressible on the definition
//!   ([`model::Trigger::BuffOnActor::from_foe`],
//!   [`model::Trigger::BuffOnFoe::from_actor`]) purely so [`evaluate`] can
//!   REJECT them, rather than silently evaluating against total stacks.
//! - **`UsingHitAndAbsorbedDamageEvents`**
//!   (`DamageModifierDescriptor.cs:94-99`) widens the eligible pool to
//!   absorbed hits, installs an implicit `dl.IsAbsorbed` checker, and forces
//!   `totalDamage` to `0` (`OutgoingDamageModifier.cs:20-23`). Five real
//!   call sites (Mesmer/Guardian/Elementalist). NOT modelled -- nothing in
//!   this project classifies an absorbed hit (`hit_stats::classify` returns
//!   `None` for every one of them) -- so
//!   [`model::DamageModifierDef::with_absorbed_damage_events`] exists only
//!   to let [`evaluate`] reject such a definition.
//! - **`GetFinalMaster()` depth.** `.UsingActorFetchIsAlwaysMaster()` /
//!   `.UsingFoeFetchIsAlwaysMaster()` ARE modelled (see
//!   [`Hit::actor_key`]/[`Hit::foe_key`]), routing the buff-state key
//!   through the agent's owner. GW2EI walks the master chain to its top;
//!   arcdps only ever reports one level of `*_master_instid`, so this
//!   resolves as far as the wire allows.
//! - **Early-exit checkers** (`_earlyExitCheckers`, ORed, abort the whole
//!   modifier for an actor) and **gain adjusters** (`DamageGainAdjuster`,
//!   e.g. the vulnerability adjuster) are not modelled. Early exit is
//!   always paired with a minion-identity predicate in the definitions that
//!   use it (`Mod_BeastlyWarden_Pet`, `Mod_EmpoweredIllusions`), which this
//!   project cannot express either, so both are skipped together -- see the
//!   `catalog` module's skipped table. Both are additive to
//!   [`model::DamageModifierDef`].
//! - **Buff STACK COUNT fidelity.** Buff-gated modifiers are only as exact
//!   as the stack timelines underneath them. M3's simulator is calibrated
//!   to a tolerance for the twelve boons and has never been calibrated for
//!   non-boon buffs (EI's JSON exposes no per-buff timeline to calibrate
//!   against), and it has one duration simulator where GW2EI has three
//!   (`Queue`, `Regeneration`, `Force`). The measured consequence, per id,
//!   is the calibration table in `tests/damage_mods_golden.rs`; the
//!   flag-based modifiers, which need no buff state, are exact.
//! - **`Skip` fast paths.** GW2EI short-circuits an actor entirely when the
//!   tracked buff graph is empty (or full, for `ByAbsence`)
//!   (`BuffOnActorDamageModifier.cs:64-66`). That is a pure performance
//!   optimisation -- an empty graph yields stack 0, which those gain
//!   computers already turn into a 0 gain -- so it is not reproduced.
//!
//! # Determinism
//!
//! Output is a `BTreeMap` keyed by `(player representative addr, signed
//! modifier id)`; both the event scan and the timeline construction are
//! ordered, and no floating-point accumulation order depends on iteration
//! of an unordered container.

pub mod buff_state;
pub mod catalog;
pub mod model;

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::condition_catalog;
use crate::analysis::damage::InstidRegistry;
use crate::analysis::hit_stats;
use crate::evtc::{result, sc, RawEvent, RawLog};
use crate::model::Encounter;

use buff_state::BuffStates;
use model::{
    DamageModifierDef, DamageSource, DamageType, GainComputer, HitCheck, ParseMode, SkillMode,
    Trigger,
};

/// GW2EI's `DamageModifierStat`
/// (`GW2EIEvtcParser/EIData/Statistics/DamageModifierStat.cs:10-16`) -- the
/// exact four numbers EI serializes per `(player, modifier)`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DamageModifierStat {
    /// Hits where the modifier actually applied.
    pub hit_count: u32,
    /// Hits that were eligible for it at all.
    pub total_hit_count: u32,
    /// `sum(gain * damage)`, rounded to 3 decimals.
    pub damage_gain: f64,
    /// The `compare_type`-filtered damage aggregate this gain is measured
    /// against.
    pub total_damage: u64,
}

/// Which contextual mode the log is in, for
/// [`model::DamageModifierDef::keep`] gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeContext {
    pub parse_mode: ParseMode,
    pub skill_mode: SkillMode,
}

impl ModeContext {
    /// Derive the modes from an already-resolved [`Encounter`].
    ///
    /// This project only produces `kind == "wvw"` encounters today (see
    /// `crate::wvw`), which maps to GW2EI's `ParseModeEnum.WvW` +
    /// `SkillModeEnum.WvW`; anything else is reported as
    /// [`ParseMode::Unknown`] + [`SkillMode::PvE`], which is GW2EI's own
    /// fallback pair for a log whose logic it could not classify
    /// (`LogLogic.cs:25,40`: `ParseModeEnum.Unknown`, `SkillMode` defaults
    /// to `PvE`).
    pub fn from_encounter(enc: &Encounter) -> Self {
        if enc.kind == "wvw" {
            ModeContext { parse_mode: ParseMode::WvW, skill_mode: SkillMode::WvW }
        } else {
            ModeContext { parse_mode: ParseMode::Unknown, skill_mode: SkillMode::PvE }
        }
    }
}

/// The GW2 game build this log was recorded on, read off arcdps's
/// `CBTS_GWBUILD` statechange row (`is_statechange == 15`, GW2EI
/// `StateChange.GWBuild = 15`,
/// `GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:275`), whose payload is
/// `src_agent` (`ParsedData/CombatEvents/MetaDataEvents/Version/
/// GW2BuildEvent.cs:12-15`, `return evtcItem.SrcAgent`).
///
/// `None` when the log carries no such row (or carries a zero one, which is
/// what GW2EI itself treats as absent, `EvtcParser.cs:881`).
pub fn gw2_build(raw: &RawLog) -> Option<u64> {
    raw.events
        .iter()
        .find(|e| e.is_statechange == sc::GW2_BUILD && e.src_agent != 0)
        .map(|e| e.src_agent)
}

/// The arcdps build (`yyyymmdd`) from the EVTC header, as the integer
/// GW2EI's `ArcDPSBuilds` constants are expressed in. `None` for a
/// malformed header build string (same tolerance
/// `evtc::is_post_buff_rework` applies).
pub fn evtc_build(raw: &RawLog) -> Option<i64> {
    raw.header.build.parse::<i64>().ok()
}

/// Everything needed to evaluate one hit against one definition.
struct Hit<'a> {
    ev: &'a RawEvent,
    /// Squad-player representative addr the hit belongs to (dealer for
    /// outgoing, victim for incoming).
    actor: u64,
    /// Buff-timeline key for the DEALING agent -- the minion's own addr for
    /// a minion hit, matching `log.FindActor(GetActor(evt))`.
    actor_buff_key: u64,
    /// The same, but through `GetFinalMaster()` -- what
    /// [`DamageModifierDef::actor_always_master`] selects.
    actor_master_buff_key: u64,
    /// Buff-timeline key for the other party (`GetFoe`).
    foe_buff_key: u64,
    /// The other party's RAW (zero-repaired) agent addr -- `evt.To`
    /// outgoing, `evt.From` incoming. Distinct from `foe_buff_key`, which
    /// is squad-folded; this one is what the per-target split resolves
    /// against the enemy roster.
    foe_addr: u64,
    /// `GetFoe` through `GetFinalMaster()` --
    /// [`DamageModifierDef::foe_always_master`].
    foe_master_buff_key: u64,
    /// Buff-timeline key for `dl.To` -- the DESTINATION of the damage row,
    /// irrespective of direction (the foe outgoing, the squad player
    /// incoming). Only [`HitCheck::DstLacksBuff`] reads it.
    dst_buff_key: u64,
    /// True when the dealer is a minion rather than the player.
    from_minion: bool,
    incoming: bool,
    dmg: u64,
    is_strike: bool,
    is_condition: bool,
    is_life_leech: bool,
    is_crit: bool,
    is_glance: bool,
    is_src_moving: bool,
    is_against_moving: bool,
    is_over_ninety: bool,
    is_against_under_fifty: bool,
    is_against_downed: bool,
    is_flanking: bool,
    has_shield_damage: bool,
}

impl Hit<'_> {
    /// `GetActor(evt)` (`OutgoingDamageModifier.cs:176-179`,
    /// `IncomingDamageModifier.cs`): the dealing agent, or its final master
    /// when the definition asked for `.UsingActorFetchIsAlwaysMaster()`.
    fn actor_key(&self, def: &DamageModifierDef) -> u64 {
        if def.actor_always_master { self.actor_master_buff_key } else { self.actor_buff_key }
    }

    /// `GetFoe(evt)` (`OutgoingDamageModifier.cs:171-174`).
    fn foe_key(&self, def: &DamageModifierDef) -> u64 {
        if def.foe_always_master { self.foe_master_buff_key } else { self.foe_buff_key }
    }
}

/// Damage aggregates for one actor, in the shape
/// `OutgoingDamageModifier.GetTotalDamage` switches over.
#[derive(Debug, Clone, Copy, Default)]
struct DamageSums {
    all: u64,
    power: u64,
    strike: u64,
    condition: u64,
    life_leech: u64,
}

impl DamageSums {
    /// `DamageStatistics.ComputeDamageFrom`
    /// (`EIData/Statistics/DamageStatistics.cs:65-96`), branch for branch.
    ///
    /// **The buckets are NOT the same predicates `DamageType::keeps` uses,
    /// and that asymmetry is GW2EI's, not a simplification here.**
    /// `FilterDamageEvents` (`Actor.cs:446-479`) tests
    /// `ConditionDamageBased` on its own, so a DIRECT hit whose skill is in
    /// the condition catalog is eligible for a `Condition`-typed modifier;
    /// the aggregate below only credits `conditionDamage` for NON-DIRECT
    /// rows, putting that same hit in `strikeDamage`/`powerDamage`. Same for
    /// life-leech, which the aggregate credits only inside the non-direct,
    /// non-condition arm. Mirroring the C# exactly is the only way to keep
    /// `totalDamage` matching.
    fn add(&mut self, h: &Hit<'_>) {
        self.all += h.dmg;
        if h.is_strike {
            // `else` arm, `:91-95`: strike AND power.
            self.strike += h.dmg;
            self.power += h.dmg;
        } else if h.is_condition {
            // `:78-81`
            self.condition += h.dmg;
        } else {
            // `:82-89`
            self.power += h.dmg;
            if h.is_life_leech {
                self.life_leech += h.dmg;
            }
        }
    }

    /// `GetTotalDamage`'s `CompareType` switch. The combined arms are plain
    /// SUMS of two independently-accumulated aggregates -- exactly as GW2EI
    /// writes them (e.g. `StrikeDamage + ConditionDamage`,
    /// `OutgoingDamageModifier.cs:87-97`) -- so any hit that landed in both
    /// buckets would be counted twice there too. With the bucket
    /// construction above no hit ever can, since the arms are mutually
    /// exclusive.
    fn by_type(&self, t: DamageType) -> u64 {
        match t {
            DamageType::All => self.all,
            DamageType::Power => self.power,
            DamageType::Strike => self.strike,
            DamageType::Condition => self.condition,
            DamageType::LifeLeech => self.life_leech,
            DamageType::StrikeAndCondition => self.strike + self.condition,
            DamageType::ConditionAndLifeLeech => self.condition + self.life_leech,
            DamageType::StrikeAndLifeLeech => self.strike + self.life_leech,
            DamageType::StrikeAndConditionAndLifeLeech => {
                self.strike + self.condition + self.life_leech
            }
        }
    }
}

/// Per-actor denominators, split the three ways `GetTotalDamage` needs.
#[derive(Debug, Clone, Copy, Default)]
struct ActorTotals {
    /// `damageData.Actor*` -- the player's own hits only.
    actor_only: DamageSums,
    /// `damageData.*` -- player + minions.
    with_minions: DamageSums,
    /// `defenseData.*Taken`.
    taken: DamageSums,
}

/// Running accumulator for one `(actor, modifier)` pair.
#[derive(Debug, Clone, Copy, Default)]
struct Running {
    hit_count: u32,
    total_hit_count: u32,
    damage_gain: f64,
}

/// The `damageModMap` metadata for one emitted id -- GW2EI's `DamageModDesc`
/// (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs:308-322`), field for field.
///
/// Carried out of [`self::evaluate_full`] rather than looked up from
/// [`catalog::CATALOG`] by id at emission time, because a signed id is NOT
/// a unique key over the whole catalog: era variants of the same trait
/// share an id and are separated only by their build windows, so only the
/// set that survived `available`/`keep` for THIS log can answer "what does
/// `d174` mean here".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageModifierMeta {
    pub name: &'static str,
    pub icon: &'static str,
    /// `DamageModifier.Tooltip` -- see
    /// [`model::DamageModifierDef::tooltip`].
    pub description: String,
    pub non_multiplier: bool,
    pub is_counter: bool,
    pub skill_based: bool,
    pub approximate: bool,
    pub incoming: bool,
}

/// Everything [`self::evaluate_full`] produces: the whole-fight stats, the
/// optional per-target split, and the definition metadata for exactly the
/// ids that appear in either.
#[derive(Debug, Clone, Default)]
pub struct DamageModifierResults {
    /// `(player representative addr, signed modifier id)` -> stats over the
    /// whole fight against EVERY foe. GW2EI's `target == null` case
    /// (`Actor.GetDamageEvents`, `EIData/Actors/Actor.cs:141-143`).
    pub overall: BTreeMap<(u64, i32), DamageModifierStat>,
    /// `(player representative addr, ENEMY representative addr, signed
    /// modifier id)` -> stats restricted to damage exchanged with that one
    /// enemy. Empty unless [`self::evaluate_full`] was asked for it.
    ///
    /// GW2EI's per-target filter is by EXACT destination/source agent, not
    /// by "that actor and its minions": outgoing goes through
    /// `DamageEventByDst[target.EnglobingAgentItem]` and incoming through
    /// `DamageTakenEventsBySrc[target.EnglobingAgentItem]`
    /// (`Actor.cs:128-136`, `:161-169`), both keyed on the raw `To`/`From`
    /// agent. So an enemy MINION's damage is its own target's, never its
    /// owner's -- reproduced here by resolving the foe addr through the
    /// enemy roster only (a minion addr is not in it, so the hit
    /// contributes to `overall` and to no target).
    pub per_target: BTreeMap<(u64, u64, i32), DamageModifierStat>,
    /// `signed modifier id` -> [`DamageModifierMeta`], scoped to the ids
    /// actually present in `overall`/`per_target` -- GW2EI populates
    /// `damageModMap` the same lazy way, from inside the per-player
    /// emission loop (`JsonDamageModifierDataBuilder.cs:47-51`).
    pub meta: BTreeMap<i32, DamageModifierMeta>,
}

/// Evaluate `defs` over `raw` for every squad player in `enc`.
///
/// Returns `(player representative addr, signed modifier id) ->
/// [`DamageModifierStat`]`, containing only pairs with at least one
/// qualifying hit (GW2EI's own emission rule -- see the module doc).
///
/// Definitions that are unavailable for this log's builds
/// ([`model::DamageModifierDef::available`]), filtered out by mode
/// ([`model::DamageModifierDef::keep`]), or that use an unmodelled feature
/// (see the module doc's gap list) are skipped.
///
/// Thin wrapper over [`self::evaluate_full`] with the per-target split OFF; kept
/// as the calibration harness's and the unit tests' entry point.
pub fn evaluate(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
    defs: &[&DamageModifierDef],
) -> BTreeMap<(u64, i32), DamageModifierStat> {
    evaluate_full(raw, registry, enc, defs, false).overall
}

/// [`evaluate`] plus, when `per_target` is set, GW2EI's per-target split
/// (`damageModifiersTarget`/`incomingDamageModifiersTarget`) and the
/// `damageModMap` metadata for every emitted id.
///
/// `per_target` is a parameter rather than always-on because it is the
/// expensive half: it adds one `(actor, target)` denominator bucket and one
/// `(actor, target, definition)` accumulator per hit, and the reference WvW
/// capture has 57 targets, so the per-target maps dominate both time and
/// memory. Only `--modifiers`-style callers that actually serialize the
/// target arrays ask for it.
pub fn evaluate_full(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
    defs: &[&DamageModifierDef],
    per_target: bool,
) -> DamageModifierResults {
    let modes = ModeContext::from_encounter(enc);
    let (gw2, evtc) = (gw2_build(raw), evtc_build(raw));
    let active: Vec<&DamageModifierDef> = defs
        .iter()
        .copied()
        .filter(|d| d.available(gw2, evtc) && d.keep(modes.parse_mode, modes.skill_mode))
        .filter(|d| is_supported(d))
        .collect();
    // Catalog-transcription guard (M16 Task 1 fix round 1): combinations
    // GW2EI's own ctors reject can only arrive from a hand-written
    // definition table, so fail loudly in debug rather than silently
    // producing wrong numbers in release. `catalog.rs` is also validated by
    // a unit test, which is the gate that actually runs in CI.
    #[cfg(debug_assertions)]
    for d in &active {
        if let Err(e) = d.validate() {
            debug_assert!(false, "invalid damage-modifier definition: {e}");
        }
    }
    if active.is_empty() {
        return DamageModifierResults::default();
    }

    // Same squad/enemy/relog-folding construction `analysis::analyze` uses
    // -- see its own comments for why `squad` is the union of every addr an
    // account held while `addr_to_rep` folds them onto one representative.
    let squad: BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let enemies: BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    // Enemy-side twin of `addr_to_rep`, same construction
    // `skill_damage::build_with_registry` already uses for its own
    // `per_target` fold: every addr an enemy account held maps to
    // `Enemy::id`, which is what `Report::all_enemies[].id` -- and hence
    // the ei-json `targets[]` index the per-target arrays are positionally
    // keyed to -- carries.
    let enemy_addr_to_rep: BTreeMap<u64, u64> = enc
        .enemies
        .iter()
        .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
        .collect();

    // Perf follow-up (deferred, M16 Task 1 review): with a one-definition
    // catalog the inner `for def in &active` loop is trivial, but Task 2's
    // full table makes it O(events x definitions). Pre-bucketing `active`
    // by direction and `src_type` (and hoisting the `Trigger::Hit` constant
    // gain) turns most of that scan into a slice index. Left until there is
    // a real catalog to measure against.
    let scope = Scope {
        registry,
        states: BuffStates::build(raw, registry, &addr_to_rep, &wanted_buffs(&active)),
        post_era: raw.header.is_post_buff_rework(),
        squad,
        addr_to_rep,
        enemies,
    };

    // `SingleActorDamageModifierHelper.cs:67-88`: an actor is only ever
    // offered the four universal sources plus
    // `GetPersonalOutgoingModifiersPerSpec(Spec)`. Without this a Firebrand
    // would be credited with `Empowered` (a Warrior trait) on every hit --
    // the reference export has no such row, and the calibration says so
    // loudly. Precomputed per representative addr, as a bitmask over
    // `active`, so the hot loop is a single `&`-test per (hit, definition).
    let eligible_defs: BTreeMap<u64, Vec<bool>> = enc
        .players
        .iter()
        .map(|p| {
            (
                p.agent_addr,
                active.iter().map(|d| d.applies_to_spec(&p.profession, &p.elite_spec)).collect(),
            )
        })
        .collect();

    let mut totals: BTreeMap<u64, ActorTotals> = BTreeMap::new();
    let mut running: BTreeMap<(u64, i32), Running> = BTreeMap::new();
    // Per-target twins, populated only when asked -- see `evaluate_full`'s
    // doc comment for why this is opt-in.
    let mut totals_t: BTreeMap<(u64, u64), ActorTotals> = BTreeMap::new();
    let mut running_t: BTreeMap<(u64, u64, i32), Running> = BTreeMap::new();

    for ev in &raw.events {
        let Some(hit) = classify_hit(ev, &scope) else { continue };

        // `Enemy::id` for the OTHER party, or `None` when it is not a
        // rostered enemy at all (an enemy's minion, or an unaffiliated
        // agent hitting a squad member) -- see
        // `DamageModifierResults::per_target`'s doc comment.
        let target = if per_target {
            enemy_addr_to_rep.get(&hit.foe_addr).copied()
        } else {
            None
        };

        let entry = totals.entry(hit.actor).or_default();
        let accumulate = |entry: &mut ActorTotals| {
            if hit.incoming {
                entry.taken.add(&hit);
            } else {
                entry.with_minions.add(&hit);
                if !hit.from_minion {
                    entry.actor_only.add(&hit);
                }
            }
        };
        accumulate(entry);
        if let Some(t) = target {
            accumulate(totals_t.entry((hit.actor, t)).or_default());
        }

        let spec_ok = eligible_defs.get(&hit.actor);
        for (i, def) in active.iter().enumerate() {
            if spec_ok.is_some_and(|m| !m[i]) {
                continue;
            }
            if !is_eligible(def, &hit) {
                continue;
            }
            running.entry((hit.actor, def.json_id())).or_default().total_hit_count += 1;
            if let Some(t) = target {
                running_t.entry((hit.actor, t, def.json_id())).or_default().total_hit_count += 1;
            }

            // Order matters, and is GW2EI's: gain FIRST, then the checkers
            // (`BuffOnActorDamageModifier.cs:97`,
            // `ComputeGain(...) && CheckCondition(...)`).
            let Some(gain) = compute_gain(def, &hit, &scope.states) else { continue };
            if !checks_pass(def, &hit, &scope.states) {
                continue;
            }
            let credit = gain * hit.dmg as f64;
            let run = running.entry((hit.actor, def.json_id())).or_default();
            run.hit_count += 1;
            run.damage_gain += credit;
            if let Some(t) = target {
                let run = running_t.entry((hit.actor, t, def.json_id())).or_default();
                run.hit_count += 1;
                run.damage_gain += credit;
            }
        }
    }

    let def_by_id = |json_id: i32| -> &DamageModifierDef {
        active.iter().find(|d| d.json_id() == json_id).expect("running keys come from `active`")
    };
    let finish = |def: &DamageModifierDef, run: &Running, sums: &ActorTotals| DamageModifierStat {
        hit_count: run.hit_count,
        total_hit_count: run.total_hit_count,
        damage_gain: round_to_3(run.damage_gain),
        total_damage: total_damage(def, sums),
    };

    let overall: BTreeMap<(u64, i32), DamageModifierStat> = running
        .iter()
        .filter(|(_, run)| run.hit_count > 0)
        .map(|(&(actor, json_id), run)| {
            let sums = totals.get(&actor).copied().unwrap_or_default();
            ((actor, json_id), finish(def_by_id(json_id), run, &sums))
        })
        .collect();
    let per_target_out: BTreeMap<(u64, u64, i32), DamageModifierStat> = running_t
        .iter()
        .filter(|(_, run)| run.hit_count > 0)
        .map(|(&(actor, target, json_id), run)| {
            let sums = totals_t.get(&(actor, target)).copied().unwrap_or_default();
            ((actor, target, json_id), finish(def_by_id(json_id), run, &sums))
        })
        .collect();

    let meta = overall
        .keys()
        .map(|&(_, id)| id)
        .chain(per_target_out.keys().map(|&(_, _, id)| id))
        .map(|id| {
            let def = def_by_id(id);
            (id, DamageModifierMeta {
                name: def.name,
                icon: def.icon,
                description: def.tooltip(),
                non_multiplier: def.non_multiplier(),
                is_counter: def.is_counter,
                skill_based: def.skill_based(),
                approximate: def.approximate,
                incoming: def.incoming(),
            })
        })
        .collect();

    DamageModifierResults { overall, per_target: per_target_out, meta }
}

/// Convenience entry point over [`catalog::CATALOG`].
pub fn evaluate_catalog(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> BTreeMap<(u64, i32), DamageModifierStat> {
    evaluate(raw, registry, enc, catalog::CATALOG)
}

/// [`self::evaluate_full`] over [`catalog::CATALOG`] -- the emission entry point
/// (CLI `--modifiers` / SDK `modifiers: true`).
pub fn evaluate_catalog_full(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
    per_target: bool,
) -> DamageModifierResults {
    evaluate_full(raw, registry, enc, catalog::CATALOG, per_target)
}

/// Whether the engine models everything this definition asks for -- see the
/// module doc's gap list.
fn is_supported(def: &DamageModifierDef) -> bool {
    if def.with_absorbed_damage_events {
        return false;
    }
    match def.trigger {
        Trigger::BuffOnActor { from_foe, .. } => !from_foe,
        Trigger::BuffOnFoe { from_actor, .. } => !from_actor,
        _ => true,
    }
}

/// The `(buff id -> is_intensity)` set the active definitions need.
fn wanted_buffs(active: &[&DamageModifierDef]) -> BTreeMap<u32, bool> {
    let mut out = BTreeMap::new();
    fn add(out: &mut BTreeMap<u32, bool>, t: &model::BuffTracker) {
        for &id in t.ids {
            // Stacking kind is a property of the BUFF, so it comes from the
            // catalog's transcription of GW2EI's own `Buff` table rather
            // than from the definition that happens to watch it -- a
            // tracker over the twelve boons mixes intensity (Might,
            // Stability) and duration ids, so a per-tracker flag cannot be
            // right. See `catalog::buff_stack`.
            out.insert(id, catalog::buff_stack::is_intensity(id));
        }
    }
    for d in active {
        match &d.trigger {
            Trigger::BuffOnActor { tracker, .. } => add(&mut out, tracker),
            Trigger::BuffOnFoe { tracker, actor_check, .. } => {
                add(&mut out, tracker);
                if let Some(c) = actor_check {
                    add(&mut out, &c.tracker);
                }
            }
            Trigger::Hit | Trigger::Skill(_) => {}
        }
        // `HitCheck::DstLacksBuff` consults a timeline too.
        for id in d.checks.iter().filter_map(|c| c.buff_id()) {
            out.insert(id, catalog::buff_stack::is_intensity(id));
        }
    }
    out
}

/// The per-log context [`classify_hit`] resolves each event against --
/// built once by [`evaluate`].
struct Scope<'a> {
    registry: &'a InstidRegistry,
    squad: BTreeSet<u64>,
    addr_to_rep: BTreeMap<u64, u64>,
    enemies: BTreeSet<u64>,
    states: BuffStates,
    post_era: bool,
}

/// Turn one raw event into a [`Hit`], or `None` if it isn't a connected
/// squad-relevant damage row.
fn classify_hit<'a>(ev: &'a RawEvent, scope: &Scope<'_>) -> Option<Hit<'a>> {
    let Scope { registry, squad, addr_to_rep, enemies, states, post_era } = scope;
    let post_era = *post_era;
    if ev.is_statechange != 0 || ev.is_activation != 0 || ev.is_buffremove != 0 {
        return None;
    }
    // CC application rows reuse `value`/`buff_dmg` for a duration, not
    // damage -- excluded by every damage pass in this crate.
    if ev.result == result::CROWD_CONTROL {
        return None;
    }
    // `HasHit` + damage classification, reusing `analysis::hit_stats`'s
    // already-calibrated era-gated predicate verbatim.
    let c = hit_stats::classify(ev, post_era)?;

    // Zero-addr rows (found by M16 Task 1's `Moving Bonus` calibration: a
    // handful of real damage rows carry `dst_agent == 0` -- or
    // `src_agent == 0` -- with a perfectly good instid) are repaired for
    // EVERY pass by `evtc::repair`, GW2EI's `EvtcParser.CompleteAgents`
    // orphaned-instid rewrite, run as a `decode_raw` post-pass (MATTRIB
    // Task 1). This module therefore reads the addresses straight off the
    // row, exactly like `damage`/`hit_stats`/`defenses`: M16's module-local
    // `NonZeroAddrIndex` was retired with that milestone, since keeping a
    // second, differently-bounded repair here would make the modifier
    // engine see a different event stream than the metrics it annotates.
    let src_agent = ev.src_agent;
    let dst_agent = ev.dst_agent;
    let src_in_squad = squad.contains(&src_agent);
    let dst_in_squad = squad.contains(&dst_agent);
    // `AgentItem.GetFinalMaster()` stand-in: an agent's owner, via the same
    // time-aware master-instid resolution the pet-credit path uses. GW2EI
    // walks the master chain to its top; arcdps only ever reports one level
    // of `*_master_instid`, so this resolves the whole chain it can see.
    let final_master = |addr: u64, master_instid: u16| -> u64 {
        if master_instid == 0 {
            return states.actor_key(addr);
        }
        match registry.resolve_at(master_instid, ev.time) {
            Some(m) if m != 0 => states.actor_key(m),
            _ => states.actor_key(addr),
        }
    };

    let (actor, actor_buff_key, actor_master_buff_key, foe_buff_key, from_minion, incoming) =
        if src_in_squad && enemies.contains(&dst_agent) {
            let rep = addr_to_rep[&src_agent];
            (rep, rep, rep, states.actor_key(dst_agent), false, false)
        } else if !src_in_squad && enemies.contains(&dst_agent) {
            // A friendly pet/minion: resolve its owner through the same
            // time-aware `src_master_instid` lookup
            // `damage::pet_credit_events` uses. (Unlike that function this
            // does not additionally team-gate the source, because the
            // destination is already constrained to the resolved enemy set.)
            let owner = registry.resolve_at(ev.src_master_instid, ev.time)?;
            if !squad.contains(&owner) {
                return None;
            }
            let rep = addr_to_rep[&owner];
            // GW2EI reads the MINION's own buff graphs for a minion hit --
            // unless the definition asked for `ActorAlwaysMaster`, which is
            // what the master key alongside it is for.
            (rep, states.actor_key(src_agent), rep, states.actor_key(dst_agent), true, false)
        } else if dst_in_squad {
            // Incoming. Deliberately NOT `&& !src_in_squad`: GW2EI's
            // incoming modifiers run over the actor's whole damage-TAKEN
            // pool (`GetDamageTakenEvents`, no source filter), so a hit a
            // squad member takes from another squad member -- or from
            // THEMSELVES -- counts in the denominator. Requiring a
            // non-squad source was MATTRIB Task 2's finding for M16's
            // quarantined incoming deficit: the affected account's missing
            // rows are exactly 7 self-inflicted Bleeding (`736`) ticks
            // totalling 239 damage, which every other pass in this crate
            // (`defenses::condition_damage_taken` matches EI's 77/5699
            // exactly) already counted and only this one dropped.
            let rep = addr_to_rep[&dst_agent];
            (rep, rep, rep, states.actor_key(src_agent), false, true)
        } else {
            return None;
        };
    // `GetFoe` is `evt.To` outgoing / `evt.From` incoming; its master form
    // reads the corresponding `*_master_instid`.
    let foe_master_buff_key = if incoming {
        final_master(src_agent, ev.src_master_instid)
    } else {
        final_master(dst_agent, ev.dst_master_instid)
    };

    Some(Hit {
        ev,
        actor,
        actor_buff_key,
        actor_master_buff_key,
        foe_buff_key,
        foe_addr: if incoming { src_agent } else { dst_agent },
        foe_master_buff_key,
        // `dl.To` is the destination of the row: the foe outgoing, the
        // squad player incoming -- which is exactly `foe_buff_key` in the
        // first case and `actor` in the second.
        dst_buff_key: if incoming { actor } else { foe_buff_key },
        from_minion,
        incoming,
        dmg: c.dmg,
        // GW2EI's `!(x is NonDirectHealthDamageEvent)`: a `buff == 0` row.
        is_strike: c.is_direct_hit,
        is_condition: condition_catalog::is_condition_damage_based(ev.skillid),
        is_life_leech: c.is_life_leech_hit,
        is_crit: c.is_crit,
        is_glance: c.is_glance,
        is_src_moving: (ev.is_moving & 1) != 0,
        is_against_moving: c.is_against_moving,
        is_over_ninety: c.is_above_ninety,
        // `SkillEvent.cs:37`, `AgainstUnderFifty = evtcItem.IsFifty > 0`.
        is_against_under_fifty: ev.is_fifty != 0,
        is_against_downed: c.is_against_downed,
        // `SkillEvent.cs:40`, `IsFlanking = evtcItem.IsFlanking > 0`.
        is_flanking: ev.is_flanking != 0,
        // `DirectHealthDamageEvent.cs:17` / `NonDirectHealthDamageEvent.cs:17`
        // -- the shield amount is `OverstackValue` on a direct row and the
        // health damage itself on a non-direct one, so "> 0" is
        // `is_shields` plus a non-zero magnitude in the matching field.
        has_shield_damage: ev.is_shields != 0
            && if c.is_direct_hit { ev.overstack > 0 } else { c.dmg > 0 },
    })
}

/// `GetDamageEvents`: direction + `dmg_src` + `src_type`.
fn is_eligible(def: &DamageModifierDef, hit: &Hit<'_>) -> bool {
    let direction_ok = match def.dmg_src {
        DamageSource::Incoming => hit.incoming,
        DamageSource::All => !hit.incoming,
        DamageSource::NoPets => !hit.incoming && !hit.from_minion,
        DamageSource::PetsOnly => !hit.incoming && hit.from_minion,
    };
    direction_ok && def.src_type.keeps(hit.is_strike, hit.is_condition, hit.is_life_leech)
}

/// `ComputeGain` -- `None` when the hit does not qualify (GW2EI's
/// `return false` / `gain == 0`).
fn compute_gain(def: &DamageModifierDef, hit: &Hit<'_>, states: &BuffStates) -> Option<f64> {
    let base = match &def.trigger {
        // `DamageLogDamageModifier.cs:15-18`: constant, stack fixed at 1.
        Trigger::Hit => def.gain.compute_gain(def.gain_per_stack, 1),
        // `SkillDamageModifier.cs:50-57`: gain is exactly 1 for the right
        // skill, and the modifier is non-multiplier.
        Trigger::Skill(id) => {
            if hit.ev.skillid != *id {
                return None;
            }
            1.0
        }
        Trigger::BuffOnActor { tracker, .. } => {
            let stack = states.tracker_stack(tracker, hit.actor_key(def), hit.ev.time);
            def.gain.compute_gain(def.gain_per_stack, stack)
        }
        Trigger::BuffOnFoe { tracker, actor_check, .. } => {
            // `CheckActor` (`BuffOnFoeDamageModifier.cs:78-80`) gates the
            // whole hit before the foe-side gain is even used; `1.0` is
            // GW2EI's own dummy `gainPerStack` there (only the sign
            // matters).
            if let Some(check) = actor_check {
                let stack = states.tracker_stack(&check.tracker, hit.actor_key(def), hit.ev.time);
                let computer =
                    if check.by_absence { GainComputer::ByAbsence } else { GainComputer::ByPresence };
                if computer.compute_gain(1.0, stack) <= 0.0 {
                    return None;
                }
            }
            let stack = states.tracker_stack(tracker, hit.foe_key(def), hit.ev.time);
            def.gain.compute_gain(def.gain_per_stack, stack)
        }
    };
    if base == 0.0 {
        return None;
    }
    // `CounterOn{Actor,Foe}DamageModifier.cs:21-25`: the base gain decides
    // whether the hit qualifies, then the gain itself is overwritten with 1.
    Some(if def.is_counter { 1.0 } else { base })
}

/// `CheckCondition` -- every checker ANDed
/// (`DamageModifierDescriptor.cs:128-130`).
fn checks_pass(def: &DamageModifierDef, hit: &Hit<'_>, states: &BuffStates) -> bool {
    def.checks.iter().all(|c| match *c {
        HitCheck::SrcMoving => hit.is_src_moving,
        HitCheck::AgainstMoving => hit.is_against_moving,
        HitCheck::OverNinety => hit.is_over_ninety,
        HitCheck::AgainstUnderFifty => hit.is_against_under_fifty,
        HitCheck::AgainstDowned => hit.is_against_downed,
        HitCheck::Crit => hit.is_crit,
        HitCheck::Glance => hit.is_glance,
        HitCheck::Flanking => hit.is_flanking,
        HitCheck::ShieldDamage => hit.has_shield_damage,
        HitCheck::SkillId(id) => hit.ev.skillid == id,
        HitCheck::DstLacksBuff(id) => states.stack_at(hit.dst_buff_key, id, hit.ev.time) == 0,
    })
}

/// `GetTotalDamage`.
fn total_damage(def: &DamageModifierDef, sums: &ActorTotals) -> u64 {
    match def.dmg_src {
        DamageSource::Incoming => sums.taken.by_type(def.compare_type),
        DamageSource::NoPets => sums.actor_only.by_type(def.compare_type),
        // `All` and `PetsOnly` both read the actor+minion total -- the
        // minions-only subtraction is commented out in GW2EI
        // (`OutgoingDamageModifier.cs:35-36, 83-84`), a quirk reproduced on
        // purpose.
        DamageSource::All | DamageSource::PetsOnly => sums.with_minions.by_type(def.compare_type),
    }
}

/// `Math.Round(x, 3)` -- .NET's default is MidpointRounding.ToEven, so a
/// value landing exactly on a half at the third decimal rounds to the even
/// neighbour (`f64::round` would round half AWAY from zero).
fn round_to_3(x: f64) -> f64 {
    let scaled = x * 1000.0;
    if !scaled.is_finite() {
        return x;
    }
    let floor = scaled.floor();
    let frac = scaled - floor;
    // Exactly-half goes UP only when that lands on an even integer.
    let round_up = frac > 0.5 || (frac == 0.5 && (floor / 2.0).fract() != 0.0);
    (if round_up { floor + 1.0 } else { floor }) / 1000.0
}

#[cfg(test)]
mod tests;
