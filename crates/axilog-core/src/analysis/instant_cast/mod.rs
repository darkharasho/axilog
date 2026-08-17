//! GW2EI's `InstantCastFinder` subsystem (MPROC).
//!
//! # What this is for
//!
//! Elite Insights' `skillMap` carries five booleans this project could not
//! emit: `isTraitProc`, `isGearProc`, `isUnconditionalProc`,
//! `isNotAccurate` and `isInstantCast`. They were filed for years under
//! "needs a GW2 skill database". They do not. They are a SIDE EFFECT of
//! instant-cast detection:
//!
//! * `CombatData.ComputeInstantCastEventsFromFinders` (`CombatData.cs:214-244`)
//!   walks every `InstantCastFinder` and, for each one whose
//!   `Available(this)` holds, files its skill id into a `TraitProc` /
//!   `GearProc` / `UnconditionalProc` / `NotAccurate` set by the finder's
//!   declared origin. `SkillData.cs:44-58` is then a bare `Contains`.
//! * `isInstantCast` is strictly stronger:
//!   `log.CombatData.GetInstantCastData(skill.ID).Any()`
//!   (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs:23`) -- "did a finder for
//!   this skill actually FIRE in this log". No availability shortcut
//!   reaches it; the finders have to be run for real.
//!
//! And availability itself is log-specific, not build-specific: a finder's
//! `_enableConditions` are predicates over the parsed log
//! (`InstantCastFinder.cs:138-150`). So there is no static id table that
//! reproduces EI's output. This module is the port.
//!
//! # Shape of the port
//!
//! GW2EI has 13 finder subclasses and 649 constructions across ~44
//! profession-helper files, but the subclasses are structurally identical:
//!
//! > take one event stream, group it by the agent the subclass calls the
//! > caster, drop events that fall inside the finder's ICD of the last
//! > accepted one, and emit an `InstantCastEvent` for each survivor.
//!
//! So the finders are DATA ([`model::FinderDef`]) and there is one engine
//! ([`compute`]) rather than 13. This mirrors `analysis::damage_mods`,
//! which made the same call about GW2EI's `DamageModifier` graph, and for
//! the same reason: it is what lets the catalog be machine-extracted from
//! the C# source with an accounting of everything it could not express.
//!
//! # Deliberate gaps
//!
//! * **Six `UsingNoAnimatedCastChecker` finders are skipped.** That
//!   checker asks whether the caster's animated cast WINDOW -- start plus
//!   actual duration -- contains a time (`CombatData.IsCasting`). This
//!   project pairs cast starts with stops in `analysis::rotation`, which
//!   is downstream of here, so the window is not available. Dropping the
//!   checker instead would widen those six onto casts EI excludes.
//! * **Spec checkers read a static spec per agent**, not GW2EI's
//!   `GetSpecAtTime`. An agent's spec cannot change within one agent addr
//!   (a build swap or relog produces a NEW addr, which this project
//!   already tracks per-addr), so the two agree except across a mid-log
//!   swap by one account, where GW2EI's per-addr agents make the
//!   distinction moot as well.

pub mod catalog;
pub mod model;

use std::collections::{BTreeMap, BTreeSet};

pub use model::{
    CastOrigin, Check, Enable, FinderDef, InstantCastEvent, LogCapabilities, Party, SwapSnap,
    Trigger, DEFAULT_ICD, END_OF_LIFE, EVTC_END_OF_LIFE, EVTC_START_OF_LIFE, SERVER_DELAY,
    START_OF_LIFE,
};

use crate::analysis::damage::{is_health_damage_result, InstidRegistry};
use crate::evtc::{result, sc, RawLog};
use crate::model::Encounter;

/// Probe the log for everything [`model::Enable`] conditions and build
/// gates read.
pub fn capabilities(raw: &RawLog) -> LogCapabilities {
    LogCapabilities {
        gw2_build: crate::analysis::damage_mods::gw2_build(raw),
        evtc_build: crate::analysis::damage_mods::evtc_build(raw),
        // The cheap probe rather than a full `effect::decode`, because
        // `available_flags` asks this question without ever running the
        // finders. The two agree by construction -- see
        // `evtc::effect::has_effect_data`.
        has_effect_data: crate::evtc::effect::has_effect_data(raw),
        has_missile_data: raw.events.iter().any(|e| {
            matches!(e.is_statechange, sc::MISSILE_CREATE | sc::MISSILE_LAUNCH | sc::MISSILE_REMOVE)
        }),
        has_ext_healing: crate::evtc::ext_healing::healing_extension_present(raw),
    }
}

/// An agent's profession and elite spec, as [`Check::Spec`] compares them.
///
/// Both are the plain names this project already uses on
/// [`crate::model::Player`] (`"Guardian"`, `"Firebrand"`), which are also
/// GW2EI's `Spec` enum names -- so a transcribed checker can carry the C#
/// symbol verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSpec {
    base: String,
    elite: String,
}

/// Map every known agent addr to its spec, for [`Check::Spec`].
///
/// Built from the encounter's players rather than from `raw.agents`
/// because the profession/elite decode already lives there, and because
/// only players have specs at all: a checker asking "was the recipient a
/// Firebrand" is false for every NPC, which a missing entry gives for
/// free.
fn spec_index(enc: &Encounter) -> BTreeMap<u64, AgentSpec> {
    let mut out = BTreeMap::new();
    for p in &enc.players {
        let spec = AgentSpec { base: p.profession.clone(), elite: p.elite_spec.clone() };
        for &addr in &p.agent_addrs {
            out.insert(addr, spec.clone());
        }
    }
    out
}

/// One event that matched some finder's trigger, with everything the
/// checkers and the caster rule need.
///
/// The two agent slots are RAW addrs, deliberately un-folded: whether to
/// resolve through to a master is the FINDER's decision
/// ([`FinderDef::minions`]), not the stream's, and a stream is shared by
/// finders that disagree about it.
#[derive(Debug, Clone, Copy)]
struct TriggerHit {
    time: u64,
    /// [`Party::Key`] -- `To` on a buff event, `Src` on a damage or effect
    /// one.
    key: u64,
    /// [`Party::Other`] -- its counterpart. `0` for streams with only one
    /// party (spawns, minion casts), and for an effect with no anchor
    /// agent.
    other: u64,
    /// Applied/extended duration, for [`Check::Duration`] and
    /// [`Check::EffectDuration`]. `0` for triggers that carry no duration.
    duration: i64,
    /// Whether an effect hit is anchored to an agent, for
    /// [`Check::AroundDst`]. Always `false` on the non-effect streams,
    /// which carry no such notion and never see the check.
    around_dst: bool,
    /// Index into [`Ctx::effects`] for an effect hit; [`NOT_AN_EFFECT`]
    /// otherwise. Needed for the `other != evt` identity test inside
    /// [`Check::SecondaryEffect`], which is reference inequality in C#.
    effect_idx: usize,
}

/// [`TriggerHit::effect_idx`] for a hit that did not come from the effect
/// stream.
const NOT_AN_EFFECT: usize = usize::MAX;

impl TriggerHit {
    /// All-zero base, so a stream that has no notion of anchoring or
    /// effect identity writes only the fields it means.
    const NONE: TriggerHit = TriggerHit {
        time: 0,
        key: 0,
        other: 0,
        duration: 0,
        around_dst: false,
        effect_idx: NOT_AN_EFFECT,
    };

    fn party(&self, p: Party) -> u64 {
        match p {
            Party::Key => self.key,
            Party::Other => self.other,
        }
    }
}

/// The distinct event stream a [`Trigger`] watches, as a hashable key --
/// several finders routinely share one stream (e.g. 86 different
/// `MinionCommand` finders all read the same buff), and collecting each
/// stream once instead of once per finder turns a 658-pass scan into one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StreamKey {
    BuffApply(u32),
    BuffRemoveAll(u32),
    BuffExtend(u32),
    Damage(u32),
    BreakbarDamage(u32),
    AnimatedCast(u32),
    Spawn,
    Missile(u32),
    ExtHealing(u32),
    /// Keyed by the STABLE effect GUID, not the session-local effect id
    /// the wire carries: the id differs between two recordings of the same
    /// fight, so a catalog row could not name one.
    Effect([u8; 16]),
}

impl Trigger {
    fn stream_key(&self) -> StreamKey {
        match *self {
            // `BuffGain`, `BuffGive` and `MinionCommand` all read buff
            // APPLIES; they differ only in which party they call the
            // caster and in their checkers, which is applied per finder
            // below rather than per stream.
            Trigger::BuffGain { buff_id } | Trigger::BuffGive { buff_id } => {
                StreamKey::BuffApply(buff_id)
            }
            Trigger::MinionCommand { .. } => StreamKey::BuffApply(MINION_COMMAND_BUFF),
            Trigger::BuffLoss { buff_id } => StreamKey::BuffRemoveAll(buff_id),
            Trigger::BuffExtend { buff_id } => StreamKey::BuffExtend(buff_id),
            Trigger::Damage { skill_id } => StreamKey::Damage(skill_id),
            Trigger::BreakbarDamage { skill_id } => StreamKey::BreakbarDamage(skill_id),
            Trigger::MinionCast { skill_id } => StreamKey::AnimatedCast(skill_id),
            Trigger::MinionSpawn { .. } => StreamKey::Spawn,
            Trigger::Missile { skill_id } => StreamKey::Missile(skill_id),
            Trigger::ExtHealing { skill_id } => StreamKey::ExtHealing(skill_id),
            // Both effect subclasses share one stream per GUID; they
            // differ only in which of the two agents they call the caster.
            Trigger::Effect { guid, .. } => StreamKey::Effect(*guid),
        }
    }
}

/// GW2EI `SkillIDs.MinionCommandBuff` (`SkillIDs.cs:3578`) -- the shared
/// buff every `MinionCommandCastFinder` watches, with the commanded
/// minion's SPECIES doing the narrowing.
///
/// The source comment is worth carrying: "applied to minion, from player
/// for necro/ele, from pet for ranger, not used for mechanist". So the
/// APPLIER is not reliably the commander, which is exactly why the finder
/// credits the recipient's master instead.
pub const MINION_COMMAND_BUFF: u32 = 59536;

/// Everything the engine needs about the log, resolved once.
struct Ctx<'a> {
    raw: &'a RawLog,
    specs: BTreeMap<u64, AgentSpec>,
    /// addr -> species id, for [`Trigger::MinionSpawn`] and
    /// [`Trigger::MinionCommand`]. Only non-player agents appear.
    species: BTreeMap<u64, u32>,
    /// addr -> owning player's representative addr, for the agents that
    /// have a master. Absent for masterless agents.
    master: BTreeMap<u64, u64>,
    /// Weapon-swap times per agent, in event (chronological) order --
    /// what `GetTime`'s snapping arm searches. Empty for a log with no
    /// `CBTS_WEAPSWAP` rows, which makes the snap inert rather than
    /// wrong.
    swaps: BTreeMap<u64, Vec<u64>>,
    post_era: bool,
    /// Decoded effect events plus their GUID table. Empty unless some
    /// active finder is an effect finder -- the decode allocates one entry
    /// per effect row, and a WvW log carries thousands.
    effects: crate::evtc::effect::EffectIndex,
    /// Effect-list indices grouped by the effect's session-local id, so
    /// both the stream collection and [`Check::SecondaryEffect`] can go
    /// from a GUID to its events in two lookups.
    effects_by_id: BTreeMap<u32, Vec<usize>>,
}

impl Ctx<'_> {
    /// GW2EI's `AgentItem.GetFinalMaster()`: the owning player for a
    /// minion/pet, or the agent itself when it has no master.
    fn final_master(&self, addr: u64) -> u64 {
        *self.master.get(&addr).unwrap_or(&addr)
    }

    /// `InstantCastFinder.GetTime` (`:117-129`): the offset time, pulled
    /// to the requested side of a nearby weapon swap by the same caster.
    ///
    /// GW2EI takes the FIRST swap within `ServerDelayConstant / 2`
    /// (`wepSwaps[0]`), not the nearest -- reproduced, because with two
    /// swaps in a 75ms window the two rules disagree.
    fn snap(&self, caster: u64, time: u64, snap: SwapSnap) -> u64 {
        if snap == SwapSnap::None {
            return time;
        }
        let window = (SERVER_DELAY / 2) as u64;
        let Some(swaps) = self.swaps.get(&caster) else { return time };
        let Some(&s) = swaps.iter().find(|&&s| s.abs_diff(time) < window) else { return time };
        match snap {
            SwapSnap::Before => time.min(s.saturating_sub(1)),
            SwapSnap::After => time.max(s.saturating_add(1)),
            SwapSnap::None => time,
        }
    }

    /// The effects a finder's GUID names, as indices into
    /// [`Ctx::effects`]. Empty when this log never mapped that GUID.
    fn effects_with_guid(&self, guid: &[u8; 16]) -> &[usize] {
        self.effects
            .id_for_guid(guid)
            .and_then(|id| self.effects_by_id.get(&id))
            .map_or(&[][..], |v| v.as_slice())
    }
}

/// GW2EI's `EffectCastFinder.GetAgent` / `GetOtherAgent`
/// (`EffectCastFinder.cs:18-36`) for one effect: the finder's key agent and
/// its counterpart, minion-folded if the finder asked for it.
///
/// Both are instance methods on the FINDER, so the `by_dst` swap applies to
/// every effect the finder inspects -- including the secondary effects a
/// [`Check::SecondaryEffect`] compares against, not just the triggering one.
fn effect_agents(ctx: &Ctx<'_>, f: &FinderDef, ev: &crate::evtc::EffectEvent) -> (u64, u64) {
    let by_dst = matches!(f.trigger, Trigger::Effect { by_dst: true, .. });
    let (k, o) = if by_dst { (ev.dst.unwrap_or(0), ev.src) } else { (ev.src, ev.dst.unwrap_or(0)) };
    if f.minions {
        (ctx.final_master(k), ctx.final_master(o))
    } else {
        (k, o)
    }
}

/// Run every AVAILABLE finder over the log and return the instant casts
/// they recover, sorted by `(time, skill, caster)`.
///
/// This is GW2EI's `ComputeInstantCastEventsFromFinders`
/// (`CombatData.cs:214-244`) minus the parts that only exist to build its
/// four id sets -- those are derived from finder availability alone, so
/// [`available_flags`] computes them without running anything.
pub fn compute(raw: &RawLog, enc: &Encounter, finders: &[FinderDef]) -> Vec<InstantCastEvent> {
    let caps = capabilities(raw);
    let active: Vec<&FinderDef> = finders.iter().filter(|f| f.available(&caps)).collect();
    if active.is_empty() {
        return Vec::new();
    }

    let need_effects = active.iter().any(|f| matches!(f.trigger, Trigger::Effect { .. }));
    let ctx = build_ctx(raw, enc, need_effects);
    let wanted: BTreeSet<StreamKey> = active.iter().map(|f| f.trigger.stream_key()).collect();
    let streams = collect_streams(&ctx, &wanted);

    let mut out = Vec::new();
    for f in active {
        let Some(hits) = streams.get(&f.trigger.stream_key()) else { continue };
        emit_for_finder(&ctx, f, hits, &mut out);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The four availability-derived id sets `SkillData.cs:44-58` reads, as
/// `(trait_procs, gear_procs, unconditional_procs, not_accurate)`.
///
/// Deliberately separate from [`compute`]: these depend only on which
/// finders are AVAILABLE, so a caller that wants only the proc flags never
/// pays for the event scan. `isInstantCast` is the one flag that cannot be
/// answered here -- it needs [`compute`]'s output.
pub fn available_flags(
    raw: &RawLog,
    finders: &[FinderDef],
) -> (BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>) {
    let caps = capabilities(raw);
    let (mut trait_p, mut gear_p, mut uncond_p, mut not_acc) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for f in finders.iter().filter(|f| f.available(&caps)) {
        match f.origin {
            CastOrigin::Trait => {
                trait_p.insert(f.skill_id);
            }
            CastOrigin::Gear => {
                gear_p.insert(f.skill_id);
            }
            CastOrigin::Unconditional => {
                uncond_p.insert(f.skill_id);
            }
            // The default origin sets no flag (`CombatData.cs:238-243`
            // has no `Skill` case).
            CastOrigin::Skill => {}
        }
        if f.is_not_accurate() {
            not_acc.insert(f.skill_id);
        }
    }
    (trait_p, gear_p, uncond_p, not_acc)
}

fn build_ctx<'a>(raw: &'a RawLog, enc: &Encounter, need_effects: bool) -> Ctx<'a> {
    let registry = InstidRegistry::build(raw);

    let effects = if need_effects {
        crate::evtc::effect::decode(raw)
    } else {
        crate::evtc::EffectIndex::default()
    };
    let mut effects_by_id: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, ev) in effects.events.iter().enumerate() {
        effects_by_id.entry(ev.effect_id).or_default().push(i);
    }

    // `RawAgent::prof` is the species id for a non-player agent, which is
    // what `MinionSpawnCastFinder`/`MinionCommandCastFinder` match on.
    let mut species = BTreeMap::new();
    for a in &raw.agents {
        if !a.is_player() {
            species.insert(a.addr, a.prof);
        }
    }

    // Ownership is derived per row from `*_master_instid`, the same field
    // `analysis::minions` reads, and for the same reason: arcdps stamps
    // the master instid on the minion's own rows, and a minion that never
    // acts has no ownership to recover anyway.
    let mut master = BTreeMap::new();
    for e in &raw.events {
        for (mi, addr) in
            [(e.src_master_instid, e.src_agent), (e.dst_master_instid, e.dst_agent)]
        {
            if mi == 0 || addr == 0 || master.contains_key(&addr) {
                continue;
            }
            if let Some(owner) = registry.resolve_at(mi, e.time) {
                master.insert(addr, owner);
            }
        }
    }

    let mut swaps: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for e in raw.events.iter().filter(|e| e.is_statechange == sc::WEAPON_SWAP) {
        swaps.entry(e.src_agent).or_default().push(e.time);
    }

    Ctx {
        raw,
        specs: spec_index(enc),
        swaps,
        species,
        master,
        post_era: raw.header.is_post_buff_rework(),
        effects,
        effects_by_id,
    }
}

/// One pass over the log, bucketing every event that some active finder's
/// trigger wants.
///
/// Each stream is stored in event order, which is chronological, so the
/// per-finder ICD walk below can assume sorted input the way GW2EI's does.
fn collect_streams(
    ctx: &Ctx<'_>,
    wanted: &BTreeSet<StreamKey>,
) -> BTreeMap<StreamKey, Vec<TriggerHit>> {
    let want_spawn = wanted.contains(&StreamKey::Spawn);
    let mut out: BTreeMap<StreamKey, Vec<TriggerHit>> = BTreeMap::new();
    let mut push = |k: StreamKey, h: TriggerHit| {
        if wanted.contains(&k) {
            out.entry(k).or_default().push(h);
        }
    };

    for e in &ctx.raw.events {
        // --- buff applies -------------------------------------------------
        // `BuffGainCastFinder` excludes `Initial` applies unconditionally
        // (`BuffGainCastFinder.cs:11`), and every other apply-reading
        // subclass inherits that ctor, so `BUFF_INITIAL` rows are dropped
        // here rather than modelled and filtered per finder.
        let is_apply = if ctx.post_era {
            e.is_statechange == sc::BUFF_APPLY
        } else {
            crate::analysis::buffs::events::is_pre_era_apply_shaped(e) && e.is_offcycle == 0
        };
        if is_apply {
            // Apply rows: `dst_agent` holds the stack, `src_agent` applied
            // it (see `buffs::events::BuffEvent::owner`'s doc comment for
            // why removal rows are the other way round). GW2EI's
            // `GetKeyAgent` for the apply-reading subclasses is the
            // RECIPIENT (`BuffGainCastFinder.cs:13-16`), so that is `key`;
            // `BuffGiveCastFinder` overrides it to the applier
            // (`BuffGiveCastFinder.cs:16-19`), which `caster_for` reads
            // off `other`.
            push(
                StreamKey::BuffApply(e.skillid),
                TriggerHit {
                    time: e.time,
                    key: e.dst_agent,
                    other: e.src_agent,
                    duration: i64::from(e.value),
                    ..TriggerHit::NONE
                },
            );
        }

        // --- buff removals / extensions -----------------------------------
        let is_remove_all = if ctx.post_era {
            e.is_statechange == sc::BUFF_REMOVE_ALL
        } else {
            e.is_statechange == 0 && e.is_buffremove == crate::evtc::buff_remove::ALL
        };
        if is_remove_all {
            // Removal rows invert the roles: `src_agent` is the OWNER
            // (the key agent), `dst_agent` the remover.
            push(
                StreamKey::BuffRemoveAll(e.skillid),
                TriggerHit {
                    time: e.time,
                    key: e.src_agent,
                    other: e.dst_agent,
                    duration: 0,
                    ..TriggerHit::NONE
                },
            );
        }
        let is_extend = if ctx.post_era {
            e.is_statechange == sc::BUFF_CHANGE
        } else {
            crate::analysis::buffs::events::is_pre_era_apply_shaped(e) && e.is_offcycle != 0
        };
        if is_extend {
            push(
                StreamKey::BuffExtend(e.skillid),
                TriggerHit {
                    time: e.time,
                    key: e.dst_agent,
                    other: e.src_agent,
                    duration: i64::from(e.value),
                    ..TriggerHit::NONE
                },
            );
        }

        // --- damage -------------------------------------------------------
        let damage_shaped =
            e.is_statechange == 0 && e.is_activation == 0 && e.is_buffremove == 0;
        if damage_shaped {
            // `HealthDamageEvent.From` is the RAW dealing agent -- GW2EI's
            // `DamageCastFinder` does no minion folding, so a pet's hit
            // yields a cast by the PET. Faithful on purpose.
            let hit =
                TriggerHit { time: e.time, key: e.src_agent, other: e.dst_agent, ..TriggerHit::NONE };
            if is_health_damage_result(e.result) {
                push(StreamKey::Damage(e.skillid), hit);
            } else if e.result == result::BREAKBAR_DAMAGE {
                push(StreamKey::BreakbarDamage(e.skillid), hit);
            }
        }

        // --- animated casts ------------------------------------------------
        // `MinionCastCastFinder` reads cast STARTS only, and only from
        // agents that have a master. The era split is the same one
        // `analysis::rotation::build` applies.
        let is_cast_start = if ctx.post_era {
            e.is_statechange == sc::ANIMATION_START
        } else {
            e.is_statechange == 0 && (e.is_activation == 1 || e.is_activation == 2)
        };
        if is_cast_start && ctx.master.contains_key(&e.src_agent) {
            push(
                StreamKey::AnimatedCast(e.skillid),
                TriggerHit { time: e.time, key: e.src_agent, other: 0, ..TriggerHit::NONE },
            );
        }

        // --- spawns --------------------------------------------------------
        // One shared stream: the species narrowing is per finder, and
        // `MinionSpawnCastFinder` can name several species at once.
        if want_spawn && e.is_statechange == sc::SPAWN && ctx.master.contains_key(&e.src_agent) {
            push(
                StreamKey::Spawn,
                // The SPAWNED agent is the key: the per-finder species
                // test reads it, and `caster_for` folds it to its master.
                TriggerHit { time: e.time, key: e.src_agent, other: 0, ..TriggerHit::NONE },
            );
        }

        // --- missiles ------------------------------------------------------
        if e.is_statechange == sc::MISSILE_CREATE {
            push(
                StreamKey::Missile(e.skillid),
                TriggerHit { time: e.time, key: e.src_agent, other: e.dst_agent, ..TriggerHit::NONE },
            );
        }

        // --- healing extension ---------------------------------------------
        // `src_is_peer` is `HealingStatsExtensionHandler.SanitizeForSrc`
        // (`HealingStatsExtensionHandler.cs:175-183`), which EVERY
        // ext-healing finder runs on its per-source group before looking at
        // a single event (`EXTHealingCastFinder.cs:25`,
        // `EXTBarrierCastFinder.cs:25`):
        //
        // ```text
        // if (events.Any(x => x.SrcIsPeer)) { events.RemoveAll(x => !x.SrcIsPeer); return true; }
        // return false;   // caller: `continue` -- the whole group is skipped
        // ```
        //
        // Both arms collapse to the same per-EVENT predicate: a non-peer
        // event is dropped when its source has a peer event (arm 1) and
        // also when it does not (arm 2, the whole group goes), so no
        // grouping pass is needed here. The rule is per (skill, source) and
        // identical for every finder reading this stream, which is why it
        // belongs at collection rather than in `emit_for_finder`.
        //
        // Without it the ext-healing finders roughly DOUBLE-count: the
        // same heal is reported once by the healer's own arcdps and once by
        // the recipient's, and `RawExtHealEvent::src_is_peer` (which
        // defaults to `true` when neither peer bit is set -- see its own
        // doc comment) is the flag that picks one.
        if let Some(h) = crate::evtc::ext_healing::decode_data_event(
            e,
            crate::evtc::ext_healing::HEALING_SIGNATURE,
        ) {
            if h.src_is_peer {
                push(
                    StreamKey::ExtHealing(h.skill_id),
                    TriggerHit { time: e.time, key: e.src_agent, other: e.dst_agent, ..TriggerHit::NONE },
                );
            }
        }
    }

    // --- effects ----------------------------------------------------------
    // Pulled per WANTED GUID rather than by scanning the effect list, which
    // is GW2EI's own path: `GetEffectGUIDEventByGUID(guid)` to get the
    // session-local id, then `GetEffectEventsByEffectID(id)`
    // (`EffectCastFinder.cs:283-285`). A GUID this log never mapped simply
    // yields no stream, and the finder never fires.
    for k in wanted {
        let StreamKey::Effect(guid) = k else { continue };
        let hits: Vec<TriggerHit> = ctx
            .effects_with_guid(guid)
            .iter()
            .map(|&i| {
                let ev = &ctx.effects.events[i];
                TriggerHit {
                    time: ev.time,
                    key: ev.src,
                    other: ev.dst.unwrap_or(0),
                    duration: ev.duration,
                    around_dst: ev.dst.is_some(),
                    effect_idx: i,
                }
            })
            .collect();
        if !hits.is_empty() {
            out.insert(*k, hits);
        }
    }
    out
}

/// Which agent a given finder calls the caster for a hit on its stream.
///
/// `None` drops the hit -- the minion-reading subclasses require the
/// agent to actually HAVE a master, because a masterless one has no owner
/// to credit the cast to (`MinionCommandCastFinder.cs:22`,
/// `MinionCastCastFinder.cs:20`, `MinionSpawnCastFinder.cs:30`).
/// The subclass's `GetKeyAgent(evt)`, un-folded.
///
/// `BuffGiveCastFinder` overrides it to the APPLIER, not the recipient
/// the shared apply stream keys on.
fn key_agent_of(f: &FinderDef, hit: &TriggerHit) -> u64 {
    match f.trigger {
        // Both of these name the stream's SECOND party as their caster:
        // the applier on a buff give, the anchor agent on a by-dst effect.
        Trigger::BuffGive { .. } | Trigger::Effect { by_dst: true, .. } => hit.other,
        _ => hit.key,
    }
}

fn caster_for(ctx: &Ctx<'_>, f: &FinderDef, hit: &TriggerHit) -> Option<u64> {
    let base = key_agent_of(f, hit);
    // `ComputeInstantCast` skips the group whose key `IsUnknown`
    // (`EffectCastFinder.cs:287-290`). The case that matters is a by-dst
    // finder meeting a ground-anchored effect, which has no anchor agent
    // at all.
    if matches!(f.trigger, Trigger::Effect { .. }) && base == 0 {
        return None;
    }
    if !(f.minions || f.trigger.forces_minions()) {
        return Some(base);
    }
    let owner = ctx.final_master(base);
    match f.trigger {
        Trigger::MinionCommand { .. } | Trigger::MinionCast { .. } | Trigger::MinionSpawn { .. } => {
            (owner != base).then_some(owner)
        }
        // `.WithMinions()` on a buff or missile finder only WIDENS the
        // credit; a masterless agent stays its own caster.
        _ => Some(owner),
    }
}

fn passes(ctx: &Ctx<'_>, f: &FinderDef, hit: &TriggerHit) -> bool {
    // The species narrowing lives on the trigger rather than in `checks`
    // because both subclasses take it as a CTOR argument, not a builder
    // call, and a finder without it would match every minion in the log.
    match f.trigger {
        Trigger::MinionCommand { species_id } if ctx.species.get(&hit.key) != Some(&species_id) => {
            return false;
        }
        Trigger::MinionSpawn { species_ids } => {
            let Some(sp) = ctx.species.get(&hit.key) else { return false };
            if !species_ids.contains(sp) {
                return false;
            }
        }
        _ => {}
    }
    f.checks.iter().all(|c| match *c {
        Check::Spec { party, specs, base, negated } => {
            let matched = ctx.specs.get(&hit.party(party)).is_some_and(|s| {
                let have = if base { &s.base } else { &s.elite };
                specs.iter().any(|w| have.eq_ignore_ascii_case(w))
            });
            matched != negated
        }
        Check::Duration { duration, epsilon } => (hit.duration - duration).abs() < epsilon,
        Check::AroundDst { negated } => hit.around_dst != negated,
        // Inclusive on BOTH ends -- see `Check::EffectDuration`.
        Check::EffectDuration { min, max } => hit.duration >= min && hit.duration <= max,
        Check::SecondaryEffect { guid, inverted_src, type_rel, time_offset, epsilon, negated } => {
            secondary_effect(ctx, f, hit, guid, inverted_src, type_rel, time_offset, epsilon)
                != negated
        }
    })
}

/// The `Using[No]SecondaryEffect*Checker` predicate, before the
/// `negated` flip: is there another effect under `guid`, belonging to this
/// finder's caster, near this hit?
#[allow(clippy::too_many_arguments)]
fn secondary_effect(
    ctx: &Ctx<'_>,
    f: &FinderDef,
    hit: &TriggerHit,
    guid: &[u8; 16],
    inverted_src: bool,
    type_rel: model::TypeRel,
    time_offset: i64,
    epsilon: i64,
) -> bool {
    let Some(this) = ctx.effects.events.get(hit.effect_idx) else { return false };
    let (mine, _) = effect_agents(ctx, f, this);
    ctx.effects_with_guid(guid).iter().any(|&i| {
        // Reference inequality in C#; two effects at the same index are
        // the same event, which the same-GUID case makes reachable.
        if i == hit.effect_idx {
            return false;
        }
        let other = &ctx.effects.events[i];
        let type_ok = match type_rel {
            model::TypeRel::Any => true,
            model::TypeRel::Same => other.dst.is_some() == this.dst.is_some(),
            model::TypeRel::Inverted => other.dst.is_some() != this.dst.is_some(),
        };
        if !type_ok {
            return false;
        }
        let (k, o) = effect_agents(ctx, f, other);
        let theirs = if inverted_src { o } else { k };
        theirs == mine
            && (other.time as i64 - time_offset - this.time as i64).abs() < epsilon
    })
}

/// The per-finder walk: group by caster, apply the ICD, emit.
///
/// The grouping is what makes the ICD per-CASTER rather than global -- two
/// different players proccing the same trait 10ms apart are two casts, not
/// one. GW2EI gets this from `GroupBy(x => caster)`; here the hits arrive
/// interleaved, so the last accepted time is tracked per caster instead.
fn emit_for_finder(
    ctx: &Ctx<'_>,
    f: &FinderDef,
    hits: &[TriggerHit],
    out: &mut Vec<InstantCastEvent>,
) {
    let mut last: BTreeMap<u64, u64> = BTreeMap::new();
    let icd = f.icd.max(0) as u64;
    let icd_first = f.trigger.icd_before_checks();

    for hit in hits {
        let Some(caster) = caster_for(ctx, f, hit) else { continue };

        // The two orders are NOT equivalent: with checks first, an event
        // that fails a checker leaves `lastTime` untouched and so cannot
        // suppress a later passing event. See `Trigger::icd_before_checks`.
        let within_icd = last.get(&caster).is_some_and(|&t| hit.time.saturating_sub(t) < icd);
        if icd_first {
            if within_icd {
                last.insert(caster, hit.time);
                continue;
            }
            if !passes(ctx, f, hit) {
                continue;
            }
        } else {
            if !passes(ctx, f, hit) {
                continue;
            }
            if within_icd {
                last.insert(caster, hit.time);
                continue;
            }
        }
        last.insert(caster, hit.time);
        out.push(InstantCastEvent {
            // `GetTime` (`InstantCastFinder.cs:117-129`): offset first,
            // then snapped to the requested side of a nearby swap.
            // GW2EI passes `GetKeyAgent(evt)` here, NOT the (possibly
            // minion-folded) caster it files the event under -- so the
            // swap being searched for is the one on the agent that
            // actually swapped.
            time: ctx.snap(
                key_agent_of(f, hit),
                hit.time.saturating_add_signed(f.time_offset),
                f.swap_snap,
            ),
            skill_id: f.skill_id,
            caster,
        });
    }
}

#[cfg(test)]
mod tests;
