//! Damage modifiers -- definition framework + evaluation engine (M16,
//! Task 1).
//!
//! This is the Rust counterpart of GW2EI's
//! `GW2EIEvtcParser/EIData/DamageModifiers/` subsystem: the "+X% while
//! under condition Y" bookkeeping that produces EI's per-player
//! `damageModifiers` / `incomingDamageModifiers` blocks and the top-level
//! `damageModMap` descriptor table.
//!
//! **Scope of Task 1: framework + engine only.** The catalog is one real
//! definition ([`catalog::MOVING_BONUS`], Task 2 fills the table) and
//! NOTHING is emitted anywhere -- no native-schema field, no ei-json key,
//! no CLI/SDK surface. This module is a pure, standalone analysis (like
//! `analysis::health` or `analysis::replay` before they were wired), so
//! every existing output is byte-identical with it present.
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
//!   and by `dmg_src`. Two GW2EI quirks are reproduced deliberately:
//!   `WithAbsorbedDamageEvents` modifiers report `0`
//!   (`OutgoingDamageModifier.cs:20-23`), and `PetsOnly` reports the
//!   actor+minion total because GW2EI's minions-only subtraction is
//!   commented out (`:35-36, 83-84`).
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
//!   (`BuffOnActorDamageModifier.cs:42`). This project's buff extraction
//!   records the applier but has no per-applier stack simulation, so a
//!   definition requesting it is REJECTED by [`evaluate`] (logged as
//!   unsupported and skipped) rather than silently evaluated against
//!   total stacks.
//! - **Early-exit checkers** (`_earlyExitCheckers`, ORed, abort the whole
//!   modifier for an actor) and **gain adjusters** (`DamageGainAdjuster`,
//!   e.g. the vulnerability adjuster) are not modelled; no definition needs
//!   them yet. Both are additive to [`model::DamageModifierDef`].
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
    /// Buff-timeline key for the other party (`GetFoe`).
    foe_buff_key: u64,
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
    is_against_downed: bool,
}

/// Damage aggregates for one actor, in the shape
/// `OutgoingDamageModifier.GetTotalDamage` switches over.
#[derive(Debug, Clone, Copy, Default)]
struct DamageSums {
    all: u64,
    strike: u64,
    condition: u64,
    life_leech: u64,
}

impl DamageSums {
    fn add(&mut self, h: &Hit<'_>) {
        self.all += h.dmg;
        if h.is_strike {
            self.strike += h.dmg;
        }
        if h.is_condition {
            self.condition += h.dmg;
        }
        if h.is_life_leech {
            self.life_leech += h.dmg;
        }
    }

    /// `GetTotalDamage`'s `CompareType` switch. `Power` is
    /// "everything that is not condition-based", mirroring
    /// `damageData.PowerDamage`.
    fn by_type(&self, t: DamageType) -> u64 {
        match t {
            DamageType::All => self.all,
            DamageType::Power => self.all.saturating_sub(self.condition),
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
pub fn evaluate(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
    defs: &[&DamageModifierDef],
) -> BTreeMap<(u64, i32), DamageModifierStat> {
    let modes = ModeContext::from_encounter(enc);
    let (gw2, evtc) = (gw2_build(raw), evtc_build(raw));
    let active: Vec<&DamageModifierDef> = defs
        .iter()
        .copied()
        .filter(|d| d.available(gw2, evtc) && d.keep(modes.parse_mode, modes.skill_mode))
        .filter(|d| is_supported(d))
        .collect();
    if active.is_empty() {
        return BTreeMap::new();
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

    let scope = Scope {
        registry,
        states: BuffStates::build(raw, registry, &addr_to_rep, &wanted_buffs(&active)),
        addr_index: NonZeroAddrIndex::build(raw),
        post_era: raw.header.is_post_buff_rework(),
        squad,
        addr_to_rep,
        enemies,
    };

    let mut totals: BTreeMap<u64, ActorTotals> = BTreeMap::new();
    let mut running: BTreeMap<(u64, i32), Running> = BTreeMap::new();

    for ev in &raw.events {
        let Some(hit) = classify_hit(ev, &scope) else { continue };

        let entry = totals.entry(hit.actor).or_default();
        if hit.incoming {
            entry.taken.add(&hit);
        } else {
            entry.with_minions.add(&hit);
            if !hit.from_minion {
                entry.actor_only.add(&hit);
            }
        }

        for def in &active {
            if !is_eligible(def, &hit) {
                continue;
            }
            let run = running.entry((hit.actor, def.json_id())).or_default();
            run.total_hit_count += 1;

            // Order matters, and is GW2EI's: gain FIRST, then the checkers
            // (`BuffOnActorDamageModifier.cs:97`,
            // `ComputeGain(...) && CheckCondition(...)`).
            let Some(gain) = compute_gain(def, &hit, &scope.states) else { continue };
            if !checks_pass(def, &hit) {
                continue;
            }
            run.hit_count += 1;
            run.damage_gain += gain * hit.dmg as f64;
        }
    }

    running
        .into_iter()
        .filter(|(_, run)| run.hit_count > 0)
        .map(|((actor, json_id), run)| {
            let def = active
                .iter()
                .find(|d| d.json_id() == json_id)
                .expect("running keys come from `active`");
            let sums = totals.get(&actor).copied().unwrap_or_default();
            ((actor, json_id), DamageModifierStat {
                hit_count: run.hit_count,
                total_hit_count: run.total_hit_count,
                damage_gain: round_to_3(run.damage_gain),
                total_damage: total_damage(def, &sums),
            })
        })
        .collect()
}

/// Convenience entry point over [`catalog::CATALOG`].
pub fn evaluate_catalog(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> BTreeMap<(u64, i32), DamageModifierStat> {
    evaluate(raw, registry, enc, catalog::CATALOG)
}

/// Whether the engine models everything this definition asks for -- see the
/// module doc's gap list.
fn is_supported(def: &DamageModifierDef) -> bool {
    match def.trigger {
        Trigger::BuffOnActor { from_foe, .. } => !from_foe,
        _ => true,
    }
}

/// The `(buff id -> is_intensity)` set the active definitions need.
fn wanted_buffs(active: &[&DamageModifierDef]) -> BTreeMap<u32, bool> {
    let mut out = BTreeMap::new();
    let mut add = |t: &model::BuffTracker| {
        for &id in t.ids {
            // An id declared intensity by ANY definition is simulated as
            // intensity; the flag is a property of the buff, so a
            // disagreement between two definitions is a catalog bug, not a
            // per-definition choice.
            let e = out.entry(id).or_insert(t.intensity);
            *e = *e || t.intensity;
        }
    };
    for d in active {
        match &d.trigger {
            Trigger::BuffOnActor { tracker, .. } => add(tracker),
            Trigger::BuffOnFoe { tracker, actor_check } => {
                add(tracker);
                if let Some(c) = actor_check {
                    add(&c.tracker);
                }
            }
            Trigger::Hit | Trigger::Skill(_) => {}
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
    addr_index: NonZeroAddrIndex,
    post_era: bool,
}

/// Turn one raw event into a [`Hit`], or `None` if it isn't a connected
/// squad-relevant damage row.
fn classify_hit<'a>(ev: &'a RawEvent, scope: &Scope<'_>) -> Option<Hit<'a>> {
    let Scope { registry, squad, addr_to_rep, enemies, states, addr_index, post_era } = scope;
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

    // Zero-addr repair (M16 Task 1, found by the `Moving Bonus`
    // calibration): a handful of real damage rows carry `dst_agent == 0`
    // (or `src_agent == 0`) with a perfectly good instid. GW2EI never sees
    // these -- its whole parser is instid-driven, so the row lands on the
    // right actor regardless -- while this project keys friend/foe sets by
    // ADDR and would silently drop the hit. Resolving the missing addr
    // through the same time-aware `InstidRegistry` every other pass already
    // uses recovers it.
    //
    // Deliberately scoped to the `== 0` case and to THIS module: the shared
    // `damage`/`hit_stats`/`defenses` predicates are calibrated as they
    // stand and must not move. Measured impact on the reference capture:
    // exactly one hit, on one account, which is the difference between
    // `Moving Bonus` matching that account's four fields exactly and being
    // off by 1 hit / 1430 damage.
    let src_agent = resolve_zero_addr(ev.src_agent, ev.src_instid, ev.time, addr_index);
    let dst_agent = resolve_zero_addr(ev.dst_agent, ev.dst_instid, ev.time, addr_index);
    let src_in_squad = squad.contains(&src_agent);
    let dst_in_squad = squad.contains(&dst_agent);
    let (actor, actor_buff_key, foe_buff_key, from_minion, incoming) = if src_in_squad
        && enemies.contains(&dst_agent)
    {
        let rep = addr_to_rep[&src_agent];
        (rep, rep, states.actor_key(dst_agent), false, false)
    } else if !src_in_squad && enemies.contains(&dst_agent) {
        // A friendly pet/minion: resolve its owner through the same
        // time-aware `src_master_instid` lookup `damage::pet_credit_events`
        // uses. (Unlike that function this does not additionally team-gate
        // the source, because the destination is already constrained to the
        // resolved enemy set.)
        let owner = registry.resolve_at(ev.src_master_instid, ev.time)?;
        if !squad.contains(&owner) {
            return None;
        }
        let rep = addr_to_rep[&owner];
        // GW2EI reads the MINION's own buff graphs for a minion hit.
        (rep, states.actor_key(src_agent), states.actor_key(dst_agent), true, false)
    } else if dst_in_squad && !src_in_squad {
        let rep = addr_to_rep[&dst_agent];
        (rep, rep, states.actor_key(src_agent), false, true)
    } else {
        return None;
    };

    Some(Hit {
        ev,
        actor,
        actor_buff_key,
        foe_buff_key,
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
        is_against_downed: c.is_against_downed,
    })
}

/// Time-aware `instid -> NON-ZERO agent addr` index, used only to repair
/// damage rows that carry a zeroed `src_agent`/`dst_agent`.
///
/// [`InstidRegistry`] cannot serve this: it registers every observed
/// `(instid, addr)` pair INCLUDING the zeroed ones, so on exactly the rows
/// that need repairing it resolves the instid straight back to `0`.
/// Registration follows the same rules as `InstidRegistry::build`
/// (chronological, out-of-order-safe insert, `sc::EXTENSION`/
/// `EXTENSION_COMBAT` rows excluded because their addr fields are
/// untrustworthy -- see that function's doc comment), minus the zeros.
#[derive(Debug, Default)]
struct NonZeroAddrIndex {
    by_instid: BTreeMap<u16, Vec<(u64, u64)>>,
}

impl NonZeroAddrIndex {
    fn build(raw: &RawLog) -> Self {
        let mut idx = NonZeroAddrIndex::default();
        for e in &raw.events {
            if e.is_statechange == sc::EXTENSION || e.is_statechange == sc::EXTENSION_COMBAT {
                continue;
            }
            if e.src_instid != 0 && e.src_agent != 0 {
                idx.register(e.src_instid, e.time, e.src_agent);
            }
            if e.dst_instid != 0 && e.dst_agent != 0 {
                idx.register(e.dst_instid, e.time, e.dst_agent);
            }
        }
        idx
    }

    fn register(&mut self, instid: u16, time: u64, addr: u64) {
        let entries = self.by_instid.entry(instid).or_default();
        if let Some(&(last_time, last_addr)) = entries.last() {
            if last_addr == addr {
                return;
            }
            if time < last_time {
                let pos = entries.partition_point(|&(t, _)| t <= time);
                entries.insert(pos, (time, addr));
                return;
            }
        }
        entries.push((time, addr));
    }

    /// The latest non-zero addr registered at or before `t`, falling back to
    /// the EARLIEST known one when the instid is only ever seen later (a
    /// zeroed row can precede the agent's first properly-addressed row).
    fn resolve_at(&self, instid: u16, t: u64) -> Option<u64> {
        let entries = self.by_instid.get(&instid)?;
        let i = entries.partition_point(|&(time, _)| time <= t);
        if i == 0 { entries.first().map(|&(_, a)| a) } else { Some(entries[i - 1].1) }
    }
}

/// A damage row with a zeroed `src_agent`/`dst_agent` but a live instid,
/// repaired through [`NonZeroAddrIndex`]. Non-zero addrs are returned
/// untouched, and an unresolvable instid leaves the zero in place (the hit
/// is then dropped by the membership tests, exactly as before).
fn resolve_zero_addr(addr: u64, instid: u16, t: u64, index: &NonZeroAddrIndex) -> u64 {
    if addr != 0 {
        return addr;
    }
    index.resolve_at(instid, t).unwrap_or(0)
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
            let stack = states.tracker_stack(tracker, hit.actor_buff_key, hit.ev.time);
            def.gain.compute_gain(def.gain_per_stack, stack)
        }
        Trigger::BuffOnFoe { tracker, actor_check } => {
            // `CheckActor` (`BuffOnFoeDamageModifier.cs:78-80`) gates the
            // whole hit before the foe-side gain is even used; `1.0` is
            // GW2EI's own dummy `gainPerStack` there (only the sign
            // matters).
            if let Some(check) = actor_check {
                let stack = states.tracker_stack(&check.tracker, hit.actor_buff_key, hit.ev.time);
                let computer =
                    if check.by_absence { GainComputer::ByAbsence } else { GainComputer::ByPresence };
                if computer.compute_gain(1.0, stack) <= 0.0 {
                    return None;
                }
            }
            let stack = states.tracker_stack(tracker, hit.foe_buff_key, hit.ev.time);
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
fn checks_pass(def: &DamageModifierDef, hit: &Hit<'_>) -> bool {
    def.checks.iter().all(|c| match *c {
        HitCheck::SrcMoving => hit.is_src_moving,
        HitCheck::AgainstMoving => hit.is_against_moving,
        HitCheck::OverNinety => hit.is_over_ninety,
        HitCheck::AgainstDowned => hit.is_against_downed,
        HitCheck::Crit => hit.is_crit,
        HitCheck::Glance => hit.is_glance,
        HitCheck::SkillId(id) => hit.ev.skillid == id,
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
