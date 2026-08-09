//! Incoming defenses (M13 Task 2) -- EI's `defenses[0]`
//! (`GW2EIJSON.JsonStatistics.JsonDefensesAll`, built by
//! `GW2EIEvtcParser.EIData.DefenseAllStatistics : DefensePerTargetStatistics`).
//! The mirror-image of `hit_stats` (M13 Task 1): classification of already-
//! decoded INCOMING events (`dst` == a squad player's own addr, from ANY
//! source -- unlike `hit_stats`'s `squad -> enemies` restriction, matching
//! this project's own established `damage::accumulate_damage_taken`'s
//! "any source" scope, which is what `DefenseAllStatistics`'s own `from:
//! null` ctor argument means) -- reuses `hit_stats`'s verified result-byte
//! classification directly (`DirectHealthDamageEvent`/
//! `NonDirectHealthDamageEvent`, same era gating), just applied to the
//! opposite direction and additionally tracking the NON-hit outcomes
//! (blocked/evaded/missed/invulned/interrupted) that `hit_stats` never
//! needed.
//!
//! Verified directly against GW2EI source (`baaron4/
//! GW2-Elite-Insights-Parser`, `master`, 2026-08-09):
//! `GW2EIEvtcParser/EIData/Statistics/DefensePerTargetStatistics.cs`
//! (`DefenseAllStatistics : DefensePerTargetStatistics`, called with
//! `from: null` for the whole-fight `defenses[0]` aggregate this module
//! calibrates against -- NOT `defensesTarget`'s per-attacker breakdown,
//! which this project doesn't compute), `DirectHealthDamageEvent.cs`,
//! `NonDirectHealthDamageEvent.cs`, `HealthDamageEvent.cs` (the shared base
//! flags), `SkillEvent.cs` (`ConditionDamageBased`), `SkillItem.cs`/
//! `SkillIDs.cs` (`IsDodge`), `ArcDPSEnums.cs` (`ArcDPSBuilds.
//! InternalSkillIDsChange`).
//!
//! ## Hit-outcome counts -- from `HealthDamageEvent`'s own flags, evaluated
//! per incoming damage-shaped row regardless of `HasHit`
//!
//! `DefensePerTargetStatistics`'s ctor loop checks `IsBlocked`/`IsBlind`/
//! `IsAbsorbed`/`IsEvaded`/`HasInterrupted` UNCONDITIONALLY on every row in
//! `GetDamageTakenEvents` (which, unlike `hit_stats`'s own scan, includes
//! the marker rows too -- `GetDamageTakenEvents` is an unfiltered
//! `HealthDamageEvent` list, `NoDamageHealthDamageEvent`s included), not
//! just on hits:
//! - `blocked_count`: `result == BLOCK` (3), buff==0 only (`DirectBlock`
//!   has no buff==1-row equivalent in `NonDirectHealthDamageEvent`'s ctors).
//! - `evaded_count`: `result == EVADE` (4), buff==0 only, SAME caveat.
//! - `missed_count`: `result == BLIND` (7) -- EI names this "missed", not
//!   "blind" (`DirectHealthDamageEvent.IsBlind = result ==
//!   DamageResult.DirectBlind`, feeding `DefensePerTargetStatistics.
//!   MissedCount`). buff==0 only.
//! - `interrupted_count`: `result == INTERRUPT` (5) -- lives on
//!   `NoDamageHealthDamageEvent.HasInterrupted`, NOT `HasHit`-gated.
//!   buff==0 only (post-era buff==1 rows with this result value are
//!   silently dropped entirely -- see below).
//! - `invulned_count`: `result == ABSORB` (6) OR `result == INVERT` (13) --
//!   `IsAbsorbed = result == DirectOrBuffAbsorb || result ==
//!   DirectOrBuffInvert`, verified present on BOTH `DirectHealthDamageEvent`
//!   AND `NonDirectHealthDamageEvent` (both era ctors) -- the ONE outcome
//!   that can be set by a buff==1 row (post-era: `result` 6/13 directly;
//!   pre-era `ConditionResult`: 1-4, "InvulByBuff"/"InvulByPlayerSkillN").
//!
//! **`blocked_count`/`evaded_count`/`missed_count`/`interrupted_count` can
//! ONLY come from buff==0 (direct) rows.** Post-era, a buff==1 row whose
//! `result` decodes to one of these values (3/4/5/7) is silently DROPPED by
//! GW2EI's `AddBuffDamageDamageEvent` (`default: break` -- no event object
//! at all, same "shared marker value collapse" `hit_stats`'s own module doc
//! already documents for the outgoing side); pre-era, `ConditionResult`
//! simply has no such semantics at all (only `ExpectedToHit`=0/four invuln
//! variants=1-4/`Unknown`>=5 exist).
//!
//! ## `dodge_count` -- NOT a result-byte outcome at all
//!
//! **The single most important nuance this task exists to document.**
//! Unlike every other field here, GW2EI computes `DodgeCount` from a
//! COMPLETELY SEPARATE mechanism: `actor.GetCastEvents(log, start, end)
//! .Count(x => x.Skill.IsDodge(log.SkillData))` -- i.e. it counts the
//! PLAYER'S OWN dodge-skill CAST events (a self-directed `CBTS_SKILLCAST`-
//! family row, `src_agent == actor`), NOT any property of an incoming
//! damage row's result byte at all. A dodge that connects with nothing
//! still counts; conversely, an `EVADE`-result incoming hit (which DOES
//! derive from the result byte, see `evaded_count` above) reflects the
//! ATTACKER's swing missing due to ANY evade-shaped source (a dodge roll,
//! Mesmer Sword 2, Thief Infiltrator's Signet, aegis-block-shaped
//! evades on some skills, etc) -- the two counters measure genuinely
//! different things and will diverge on a real log (verified: golden
//! fixture totals 122 `evadedCount` vs 56 `dodgeCount` across 41 players,
//! and per-player these don't track each other at all, e.g. one account
//! has evaded=13/dodge=1). This is the "dodge vs evade distinction" the
//! M13 plan brief calls out by name.
//!
//! `IsDodge(skillData) = ID == MirageCloakDodge (-17) || ID ==
//! skillData.DodgeID` (`SkillItem.cs:39-41`), where `DodgeID` resolves via
//! `SkillIDs.GetArcDPSCustomIDs`: arcdps' own synthetic "Dodge" pseudo-skill
//! id, `65001` pre-`ArcDPSBuilds.InternalSkillIDsChange` (build `20220304`)
//! or `23275` on/after it (`SkillIDs.cs:84-86`). This project's operating
//! build range (every fixture, `>= 20260114`) is UNCONDITIONALLY past that
//! 2022 threshold, so `DODGE_SKILL_ID` below is hardcoded to the post-change
//! value with no era gating needed -- the same "threshold far below this
//! project's operating range" reasoning `hit_stats::NON_CRITABLE_SKILLS`'s
//! own doc section already establishes for a different hardcoded table.
//! Mirage Cloak's negative pseudo-id (`-17`, a Mesmer elite-spec dodge
//! substitute) isn't modeled -- no Mirage-specific handling exists anywhere
//! else in this project either, and it's absent from the golden fixture's
//! observed skill ids.
//!
//! Sourced from a start-cast row exactly like `support::RESURRECT_SKILL_ID`
//! already established for resurrects (SAME era split: pre-era
//! `is_statechange == 0 && is_activation ∈ {Normal(1), Quickness(2)}`;
//! post-era the dedicated `sc::ANIMATION_START` statechange) -- reuses that
//! exact, already-calibrated pattern verbatim, just swapping the skill id
//! and the credited player-stat field. `src_agent` is the caster (raw, no
//! master/pet-credit resolution -- a squad player's own dodge, never
//! pet-credited, matching `support::apply`'s own resurrect-credit doc note).
//!
//! ## Damage-taken breakdown -- `strike`/`power`/`condition`/`life_leech`/
//! `barrier`
//!
//! `DefensePerTargetStatistics`'s ctor, per HIT event (`HasHit`, using the
//! SAME `HasHit` definition `hit_stats`'s module doc already fully
//! specifies -- reused verbatim here, not re-derived):
//! ```text
//! if (damageEvent.ConditionDamageBased(log)) {
//!     ConditionDamageTaken += HealthDamage; ConditionDamageTakenCount++;
//! } else {
//!     if (damageEvent is NonDirectHealthDamageEvent ndhd) {
//!         if (ndhd.IsLifeLeech) {
//!             LifeLeechDamageTaken += HealthDamage;
//!             LifeLeechDamageTaken++;   // <- BUG, see below
//!         }
//!     } else {
//!         StrikeDamageTaken += HealthDamage; StrikeDamageTakenCount++;
//!     }
//!     PowerDamageTaken += HealthDamage; PowerDamageTakenCount++;
//! }
//! if (damageEvent.ShieldDamage > 0) { DamageBarrier += ...; DamageBarrierCount++; }
//! ```
//! `ConditionDamageBased(log)` is a PER-SKILL-ID catalog lookup
//! (`SkillEvent.cs:43-50`: true iff the skill is registered
//! `Buff.BuffClassification.Condition` in `log.Buffs.BuffsByIDs`) --
//! genuinely different from (and narrower than) "every buff==1 hit", the
//! SAME simplification gap `hit_stats::condition_count`'s own module doc
//! already discloses for the outgoing side (this project has no such
//! skill-id catalog). `strike`/`power`/`life_leech` below reuse the exact
//! same buff==0-direct vs buff==1-non-lifeleech-vs-lifeleech split
//! `hit_stats` already verified, with `condition` standing in for
//! "everything buff==1 that isn't life-leech" (same documented divergence,
//! zero residual observed against this fixture on the outgoing side and,
//! **derived algebraically below**, also zero on the incoming side).
//!
//! **`PowerDamageTaken`/`Count` == `StrikeDamageTaken`/`Count` +
//! [TRUE] life-leech sum/count, ALWAYS, regardless of the `ConditionDamageBased`
//! catalog's completeness** -- `PowerDamageTaken += HealthDamage;
//! PowerDamageTakenCount++` sits OUTSIDE the `is NonDirectHealthDamageEvent`
//! branch, unconditionally incrementing for every non-condition hit
//! (buff==0 direct AND buff==1 life-leech alike). This means `power_count`/
//! `power_damage` below (computed the SAME way, from the SAME buff==0-vs-
//! life-leech split) reproduce EI's `powerDamageTaken(Count)` EXACTLY, with
//! NO dependency on the missing condition-skill-id catalog at all -- unlike
//! the analogous `condition_count`/`condition_damage` fields, which inherit
//! the same (empirically zero-residual-on-this-fixture, but not formally
//! guaranteed) simplification gap `hit_stats::condition_count` already
//! discloses.
//!
//! **`LifeLeechDamageTaken`/`LifeLeechDamageTakenCount` are a REAL,
//! REPRODUCIBLE GW2EI BUG, not modeled here.** Look closely at the ctor
//! snippet above: the inner life-leech branch increments
//! `LifeLeechDamageTaken` TWICE -- once correctly (`+= HealthDamage`), once
//! by a copy-paste mistake (`LifeLeechDamageTaken++`, clearly INTENDED to
//! be `LifeLeechDamageTakenCount++`, the sibling field, which is therefore
//! NEVER incremented at all). Verified directly against the golden fixture:
//! `lifeLeechDamageTakenCount == 0` for every one of the 41 players
//! (including several with substantial nonzero `lifeLeechDamageTaken`), and
//! algebraically confirmed as this exact bug (not e.g. a genuinely-empty
//! bucket) by cross-checking `powerDamageTaken(Count) -
//! strikeDamageTaken(Count)` against `lifeLeechDamageTaken(Count)`: the
//! COUNT gap (`pdtc - sdtc`) is consistently POSITIVE and non-trivial (2-16
//! across players) even though `lifeLeechDamageTakenCount` reports 0 for
//! all of them, and the reported (buggy) `lifeLeechDamageTaken` SUM is
//! consistently `(pdt - sdt) + (pdtc - sdtc)` -- i.e. exactly `[true sum] +
//! [true count]`, matching the double-increment exactly. This project
//! computes the CORRECT life-leech count/sum directly from the SAME
//! `is_offcycle`-as-`BuffCycle`(pre-era)/`result`-value(post-era) predicate
//! `hit_stats::life_leech_count` already uses (not reproducing GW2EI's own
//! bug) -- the golden test derives the TRUE reference value as
//! `powerDamageTaken(Count) - strikeDamageTaken(Count)` (unaffected by the
//! bug, per the paragraph above) rather than trusting the fixture's raw
//! `lifeLeechDamageTaken(Count)` fields directly.
//!
//! `barrier`/`damageBarrier`: `ShieldDamage` -- `is_shields != 0` gates a
//! wire-level barrier-absorption sub-amount, read from a DIFFERENT field
//! than `HealthDamage` (`RawEvent::overstack`, not `value`/`buff_dmg`) on
//! the SAME event row (a single hit can debit health AND barrier
//! simultaneously; `ShieldDamage` is ADDITIVE bookkeeping, never subtracted
//! from `HealthDamage`/`strike_damage`/`condition_damage`/`life_leech_damage`
//! -- `NegateShieldDamage()` exists on the base class but is never called
//! anywhere in this ctor path). Three slightly different per-shape formulas
//! (all reproduced exactly): `DirectHealthDamageEvent`: `IsShields>0 ?
//! OverstackValue : 0`. `NonDirectHealthDamageEvent` post-era: `IsShields>0
//! ? (OverstackValue>0 ? OverstackValue : HealthDamage) : 0` (falls back to
//! the full tick amount if `OverstackValue` itself is 0 but the flag is
//! set). Pre-era `ConditionResult` ctor: `IsShields>0 ? HealthDamage : 0`
//! (no `OverstackValue` field read at all in that ctor overload). Barrier is
//! only accumulated within the `HasHit` branch (unreachable on a
//! blocked/evaded/missed/invulned/interrupted row).
//!
//! ## `breakbar_count`/`breakbar_damage` -- a wholly separate accessor, not
//! part of the `HealthDamageEvent` loop at all
//!
//! `foreach (BreakbarDamageEvent brk in actor.GetBreakbarDamageTakenEvents(...))`
//! -- defiance-bar damage (`result == BREAKBAR_DAMAGE`, 10) is a SEPARATE
//! GW2EI event type (`BreakbarDamageEvent`, not `HealthDamageEvent`),
//! scanned independently and NOT gated behind any of the outcome/hit logic
//! above (in particular, `classify` below returns `None` for this result
//! value in the main scan -- it's picked up by its own dedicated pass,
//! reading the SAME `value` field the main scan reads `HealthDamage` from,
//! since breakbar rows are always buff==0/DirectHealthDamageEvent-shaped
//! wire rows). All-zero on the golden fixture (a real WvW zerg fight
//! against enemy players records no breakbar damage against squad players
//! at all in this log) -- covered by unit tests + a real-log sanity
//! assertion instead of golden-value calibration (see `defenses_golden.
//! rs`'s module doc for what IS/ISN'T calibrated).
//!
//! ## Scope note: any-source, not squad-vs-enemies
//!
//! `DefenseAllStatistics(..., from: null)` -- unlike `hit_stats`'s outgoing
//! `squad -> enemies` restriction, this reads EVERY incoming hit regardless
//! of source (mirroring `damage::accumulate_damage_taken`'s own established
//! "any source" scope for `PlayerMetrics::damage_taken`, which this module
//! does not touch or duplicate -- `defenses` is a purely ADDITIVE
//! classification breakdown alongside it, per the M13 Task 2 brief's
//! "extend, don't replace" instruction for the pre-existing
//! `downs_taken`/`deaths`/`damage_taken`/`cc` fields).

use crate::evtc::{result, sc, RawEvent, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// `SkillIDs.ArcDPSDodge20220307` (`GW2EIEvtcParser/ParserHelpers/IDs/
/// SkillIDs.cs:86`) -- arcdps' own synthetic "Dodge" pseudo-skill id,
/// unconditionally in effect at this project's operating build range (see
/// this module's `dodge_count` doc section for the full build-threshold
/// citation). The pre-2022 value (`65001`) is NOT modeled -- no fixture
/// this project handles predates the 2022-03-04 threshold.
const DODGE_SKILL_ID: u32 = 23275;

/// `is_activation` byte values that start a skill-cast animation, pre-era --
/// identical constant to (and independently re-verified against the same
/// citation as) `support::ACTIVATION_START`; kept as a local copy rather
/// than exported cross-module to avoid coupling two independently-verified
/// modules to a single shared private item.
const ACTIVATION_START: [u8; 2] = [1, 2];

/// One squad player's incoming-defense totals (M13 Task 2) -- mirrors EI's
/// `defenses[0]` field-for-field (see this module's doc comment for the
/// exact per-field derivation/citation). All-zero (`Default`) for a player
/// who took no incoming damage-shaped events at all. Purely ADDITIVE
/// alongside the pre-existing `PlayerMetrics::downs_taken`/`deaths`/
/// `damage_taken`/`cc` fields -- none of those are touched or duplicated
/// here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefenseStats {
    pub blocked_count: u32,
    pub evaded_count: u32,
    /// See this module's doc comment -- NOT derived from any incoming
    /// event's result byte at all; a self-cast dodge-skill count.
    pub dodge_count: u32,
    pub missed_count: u32,
    pub interrupted_count: u32,
    pub invulned_count: u32,
    pub strike_count: u32,
    pub strike_damage: u64,
    /// `power_count`/`power_damage` == `strike_count`/`strike_damage` +
    /// `life_leech_count`/`life_leech_damage`, ALWAYS (see module doc) --
    /// kept as its own field (not a derived accessor) to mirror EI's own
    /// `powerDamageTaken(Count)` fields directly, field-for-field.
    pub power_count: u32,
    pub power_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    /// The TRUE life-leech count/sum (NOT reproducing GW2EI's own
    /// `LifeLeechDamageTakenCount` counting bug -- see module doc).
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub barrier_count: u32,
    pub barrier_damage: u64,
    pub breakbar_count: u32,
    pub breakbar_damage: u64,
}

impl DefenseStats {
    fn merge(&mut self, o: &DefenseStats) {
        self.blocked_count += o.blocked_count;
        self.evaded_count += o.evaded_count;
        self.dodge_count += o.dodge_count;
        self.missed_count += o.missed_count;
        self.interrupted_count += o.interrupted_count;
        self.invulned_count += o.invulned_count;
        self.strike_count += o.strike_count;
        self.strike_damage += o.strike_damage;
        self.power_count += o.power_count;
        self.power_damage += o.power_damage;
        self.condition_count += o.condition_count;
        self.condition_damage += o.condition_damage;
        self.life_leech_count += o.life_leech_count;
        self.life_leech_damage += o.life_leech_damage;
        self.barrier_count += o.barrier_count;
        self.barrier_damage += o.barrier_damage;
        self.breakbar_count += o.breakbar_count;
        self.breakbar_damage += o.breakbar_damage;
    }
}

/// A HIT event's damage-bucket classification (see module doc's "Damage-
/// taken breakdown" section).
enum HitKind {
    Strike,
    Condition,
    LifeLeech,
}

/// One incoming damage-shaped row's classification -- unlike `hit_stats::
/// Classified` (which only exists for HITS), this covers every outcome
/// `DefensePerTargetStatistics` tracks, hit or not.
enum Outcome {
    Hit { dmg: u64, kind: HitKind, shield_dmg: u64 },
    Blocked,
    Evaded,
    Interrupted,
    Invulned,
    Missed,
}

/// Classify one already-filtered (non-statechange/activation/buffremove,
/// non-`CROWD_CONTROL`) incoming event. Returns `None` for every dropped/
/// irrelevant outcome (breakbar -- handled by a dedicated separate scan,
/// see module doc; the marker-only KillingBlow/Downed/Activation values;
/// any buff==1 row whose `result` GW2EI silently drops with no event at
/// all). Reuses `hit_stats::classify`'s exact era/result-byte semantics,
/// just widened to also surface the non-hit outcomes.
fn classify(e: &RawEvent, post_era: bool) -> Option<Outcome> {
    let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };

    if e.buff == 0 {
        return match e.result {
            result::NORMAL | result::CRIT | result::GLANCE => {
                let shield_dmg = if e.is_shields != 0 { e.overstack as u64 } else { 0 };
                Some(Outcome::Hit { dmg, kind: HitKind::Strike, shield_dmg })
            }
            result::BLOCK => Some(Outcome::Blocked),
            result::EVADE => Some(Outcome::Evaded),
            result::INTERRUPT => Some(Outcome::Interrupted),
            result::ABSORB | result::INVERT => Some(Outcome::Invulned),
            result::BLIND => Some(Outcome::Missed),
            // KILLING_BLOW/DOWNED/BREAKBAR_DAMAGE/ACTIVATION/CROWD_CONTROL
            // (the latter already filtered by the caller) -- marker rows
            // with no defense-outcome meaning in this scan.
            _ => None,
        };
    }

    // buff == 1 -- era-gated exactly like `hit_stats::classify`.
    if post_era {
        match e.result {
            result::BUFF_CYCLE
            | result::BUFF_NOT_CYCLE
            | result::BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT => {
                let shield_dmg = shield_damage_nondirect_post(e, dmg);
                Some(Outcome::Hit { dmg, kind: HitKind::Condition, shield_dmg })
            }
            result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT
            | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE => {
                let shield_dmg = shield_damage_nondirect_post(e, dmg);
                Some(Outcome::Hit { dmg, kind: HitKind::LifeLeech, shield_dmg })
            }
            result::ABSORB | result::INVERT => Some(Outcome::Invulned),
            // Every other post-era buff==1 result value (including 3/4/5/7,
            // the direct-only Block/Evade/Interrupt/Blind values) is
            // silently dropped by GW2EI's `default: break` -- no event
            // constructed at all, per `hit_stats`'s own doc.
            _ => None,
        }
    } else {
        // Pre-era `buff == 1`: same apply-vs-tick disambiguator
        // `hit_stats::classify` already established (`value == 0` gates out
        // ordinary boon/condition APPLY rows, which otherwise default
        // `result` to 0/`ExpectedToHit` and would be misread as a hit).
        if e.value != 0 {
            return None;
        }
        match e.result {
            0 => {
                // ConditionResult::ExpectedToHit.
                let is_life_leech = matches!(e.is_offcycle, 3 | 5);
                let shield_dmg = if e.is_shields != 0 { dmg } else { 0 };
                let kind = if is_life_leech { HitKind::LifeLeech } else { HitKind::Condition };
                Some(Outcome::Hit { dmg, kind, shield_dmg })
            }
            1..=4 => Some(Outcome::Invulned), // InvulByBuff/InvulByPlayerSkill{1,2,3}
            // >=5 decodes as `Unknown` and is silently dropped by GW2EI's
            // pre-era switch -- no event at all (same collapse `hit_stats`
            // documents for its own pre-era `Unknown` case).
            _ => None,
        }
    }
}

/// `NonDirectHealthDamageEvent`'s post-era `ShieldDamage` formula:
/// `IsShields>0 ? (OverstackValue>0 ? OverstackValue : HealthDamage) : 0`.
fn shield_damage_nondirect_post(e: &RawEvent, dmg: u64) -> u64 {
    if e.is_shields == 0 {
        return 0;
    }
    if e.overstack > 0 {
        e.overstack as u64
    } else {
        dmg
    }
}

/// Fold one classified outcome into the destination's running `DefenseStats`.
fn record(stats: &mut DefenseStats, outcome: Outcome) {
    match outcome {
        Outcome::Hit { dmg, kind, shield_dmg } => {
            match kind {
                HitKind::Strike => {
                    stats.strike_count += 1;
                    stats.strike_damage += dmg;
                    stats.power_count += 1;
                    stats.power_damage += dmg;
                }
                HitKind::Condition => {
                    stats.condition_count += 1;
                    stats.condition_damage += dmg;
                }
                HitKind::LifeLeech => {
                    stats.life_leech_count += 1;
                    stats.life_leech_damage += dmg;
                    stats.power_count += 1;
                    stats.power_damage += dmg;
                }
            }
            if shield_dmg > 0 {
                stats.barrier_count += 1;
                stats.barrier_damage += shield_dmg;
            }
        }
        Outcome::Blocked => stats.blocked_count += 1,
        Outcome::Evaded => stats.evaded_count += 1,
        Outcome::Interrupted => stats.interrupted_count += 1,
        Outcome::Invulned => stats.invulned_count += 1,
        Outcome::Missed => stats.missed_count += 1,
    }
}

/// Per-raw-destination-addr incoming-defense totals, over ANY source ->
/// squad events (mirrors `damage::accumulate_damage_taken`'s "any source"
/// scope, NOT `hit_stats`'s squad-vs-enemies restriction -- see module doc).
fn accumulate(events: &[RawEvent], squad: &BTreeSet<u64>, post_era: bool) -> BTreeMap<u64, DefenseStats> {
    let mut out: BTreeMap<u64, DefenseStats> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        // CROWD_CONTROL rows carry CC duration in value/buff_dmg, not
        // damage -- same exclusion `damage::accumulate_damage_taken` already
        // applies (CC is handled entirely by `cc::apply_cc`).
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        if !squad.contains(&e.dst_agent) {
            continue;
        }
        let Some(outcome) = classify(e, post_era) else { continue };
        let stats = out.entry(e.dst_agent).or_default();
        record(stats, outcome);
    }
    out
}

/// Defiance-bar damage taken -- a wholly separate scan from `accumulate`
/// above (see module doc's `breakbar_count`/`breakbar_damage` section).
fn accumulate_breakbar(events: &[RawEvent], squad: &BTreeSet<u64>, out: &mut BTreeMap<u64, DefenseStats>) {
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result != result::BREAKBAR_DAMAGE {
            continue;
        }
        if !squad.contains(&e.dst_agent) {
            continue;
        }
        let dmg = e.value.max(0) as u64;
        let stats = out.entry(e.dst_agent).or_default();
        stats.breakbar_count += 1;
        stats.breakbar_damage += dmg;
    }
}

/// Self-cast dodge-skill count -- a wholly separate scan (see module doc's
/// `dodge_count` section): NOT tied to any incoming event at all, just the
/// player's OWN dodge-skill start-cast rows, era-split exactly like
/// `support::apply`'s resurrect-cast detection.
fn accumulate_dodges(raw: &RawLog, squad: &BTreeSet<u64>, post_era: bool, out: &mut BTreeMap<u64, DefenseStats>) {
    for e in &raw.events {
        let is_dodge_cast = if post_era {
            e.is_statechange == sc::ANIMATION_START && e.skillid == DODGE_SKILL_ID
        } else {
            e.is_statechange == 0
                && e.skillid == DODGE_SKILL_ID
                && ACTIVATION_START.contains(&e.is_activation)
        };
        if is_dodge_cast && squad.contains(&e.src_agent) {
            out.entry(e.src_agent).or_default().dodge_count += 1;
        }
    }
}

/// Compute per-squad-player incoming-defense stats (M13 Task 2),
/// account-folded via `addr_to_rep` (relog fold, same convention every
/// other pass in this codebase uses).
pub fn build(raw: &RawLog, squad: &BTreeSet<u64>, addr_to_rep: &BTreeMap<u64, u64>) -> BTreeMap<u64, DefenseStats> {
    let post_era = raw.header.is_post_buff_rework();
    let mut by_addr = accumulate(&raw.events, squad, post_era);
    accumulate_breakbar(&raw.events, squad, &mut by_addr);
    accumulate_dodges(raw, squad, post_era, &mut by_addr);

    let mut by_rep: BTreeMap<u64, DefenseStats> = BTreeMap::new();
    for (addr, stats) in by_addr {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        by_rep.entry(rep).or_default().merge(&stats);
    }
    by_rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawHeader, RawLog};

    fn base(src: u64, dst: u64) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: src,
            dst_agent: dst,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 1,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0,
            is_moving: 0,
            is_statechange: 0,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn direct(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = base(src, dst);
        e.result = result_;
        e.value = dmg;
        e
    }

    fn buff_dmg_event(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = base(src, dst);
        e.buff = 1;
        e.result = result_;
        e.buff_dmg = dmg;
        e
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn raw_post(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn squad1() -> BTreeSet<u64> {
        [1u64].into_iter().collect()
    }
    fn no_rep() -> BTreeMap<u64, u64> {
        BTreeMap::new()
    }

    fn get(raw: &RawLog) -> DefenseStats {
        build(raw, &squad1(), &no_rep()).get(&1).copied().unwrap_or_default()
    }

    // ---- direct (buff==0) result-byte outcome classes ----

    #[test]
    fn direct_normal_counts_strike_and_power() {
        let raw = raw_from(vec![direct(9, 1, result::NORMAL, 100)]);
        let d = get(&raw);
        assert_eq!(d.strike_count, 1);
        assert_eq!(d.strike_damage, 100);
        assert_eq!(d.power_count, 1);
        assert_eq!(d.power_damage, 100);
        assert_eq!(d, DefenseStats { strike_count: 1, strike_damage: 100, power_count: 1, power_damage: 100, ..Default::default() });
    }

    #[test]
    fn direct_crit_and_glance_also_count_strike() {
        let raw = raw_from(vec![direct(9, 1, result::CRIT, 200), direct(9, 1, result::GLANCE, 50)]);
        let d = get(&raw);
        assert_eq!(d.strike_count, 2);
        assert_eq!(d.strike_damage, 250);
    }

    #[test]
    fn direct_block_counts_blocked_only() {
        let raw = raw_from(vec![direct(9, 1, result::BLOCK, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { blocked_count: 1, ..Default::default() });
    }

    #[test]
    fn direct_evade_counts_evaded_only() {
        let raw = raw_from(vec![direct(9, 1, result::EVADE, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { evaded_count: 1, ..Default::default() });
    }

    #[test]
    fn direct_interrupt_counts_interrupted_only() {
        let raw = raw_from(vec![direct(9, 1, result::INTERRUPT, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { interrupted_count: 1, ..Default::default() });
    }

    #[test]
    fn direct_absorb_counts_invulned_only() {
        let raw = raw_from(vec![direct(9, 1, result::ABSORB, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { invulned_count: 1, ..Default::default() });
    }

    #[test]
    fn direct_invert_also_counts_invulned() {
        // `IsAbsorbed = result == DirectOrBuffAbsorb || result ==
        // DirectOrBuffInvert` -- INVERT is NOT a separate "reflected" bucket.
        let raw = raw_from(vec![direct(9, 1, result::INVERT, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { invulned_count: 1, ..Default::default() });
    }

    #[test]
    fn direct_blind_counts_missed_only() {
        // EI names this "missed", not "blind".
        let raw = raw_from(vec![direct(9, 1, result::BLIND, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { missed_count: 1, ..Default::default() });
    }

    #[test]
    fn killing_blow_downed_breakbar_marker_activation_do_not_set_any_outcome() {
        // BREAKBAR_DAMAGE is handled by the dedicated scan, exercised
        // separately below -- confirm it does NOT ALSO leak into the main
        // outcome counters here (dst still squad, but result value routed
        // through `classify`'s wildcard `_ => None`).
        let evs = vec![
            direct(9, 1, result::KILLING_BLOW, 999),
            direct(9, 1, result::DOWNED, 999),
            direct(9, 1, result::ACTIVATION, 999),
        ];
        let raw = raw_from(evs);
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default());
    }

    #[test]
    fn crowd_control_result_is_excluded_entirely() {
        let mut e = direct(9, 1, result::CROWD_CONTROL, 0);
        e.value = 1500; // CC duration ms, not damage
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default());
    }

    // ---- barrier (is_shields / overstack) ----

    #[test]
    fn direct_hit_with_shields_counts_barrier_from_overstack() {
        let mut e = direct(9, 1, result::NORMAL, 100);
        e.is_shields = 1;
        e.overstack = 30;
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.strike_damage, 100, "HealthDamage is NOT reduced by ShieldDamage");
        assert_eq!(d.barrier_count, 1);
        assert_eq!(d.barrier_damage, 30);
    }

    #[test]
    fn blocked_hit_never_contributes_barrier() {
        // Barrier is only accumulated inside the HasHit branch.
        let mut e = direct(9, 1, result::BLOCK, 999);
        e.is_shields = 1;
        e.overstack = 30;
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.barrier_count, 0);
        assert_eq!(d, DefenseStats { blocked_count: 1, ..Default::default() });
    }

    // ---- pre-era buff==1 (ConditionResult) ----

    #[test]
    fn pre_era_condition_tick_counts_condition() {
        let raw = raw_from(vec![buff_dmg_event(9, 1, 0, 30)]);
        let d = get(&raw);
        assert_eq!(d.condition_count, 1);
        assert_eq!(d.condition_damage, 30);
        assert_eq!(d.power_count, 0, "condition hits do not count toward power");
    }

    #[test]
    fn pre_era_life_leech_counts_power_and_life_leech_not_condition() {
        let mut e = buff_dmg_event(9, 1, 0, 25);
        e.is_offcycle = 3; // BuffCycle::NotCycle_DamageToTargetOnHit
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.life_leech_count, 1);
        assert_eq!(d.life_leech_damage, 25);
        assert_eq!(d.power_count, 1, "life-leech hits DO count toward power");
        assert_eq!(d.power_damage, 25);
        assert_eq!(d.condition_count, 0);
    }

    #[test]
    fn pre_era_invuln_variants_count_invulned_only() {
        let evs = vec![
            buff_dmg_event(9, 1, 1, 999),
            buff_dmg_event(9, 1, 2, 999),
            buff_dmg_event(9, 1, 3, 999),
            buff_dmg_event(9, 1, 4, 999),
        ];
        let raw = raw_from(evs);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { invulned_count: 4, ..Default::default() });
    }

    #[test]
    fn pre_era_unknown_result_is_dropped_not_counted() {
        let raw = raw_from(vec![buff_dmg_event(9, 1, result::CROWD_CONTROL, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default());
    }

    #[test]
    fn pre_era_buff_apply_row_value_nonzero_is_skipped() {
        let mut e = buff_dmg_event(9, 1, 0, 0);
        e.value = 500; // apply-shaped row, not a damage tick
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default());
    }

    #[test]
    fn pre_era_shields_on_condition_tick_counts_full_amount_as_barrier() {
        let mut e = buff_dmg_event(9, 1, 0, 40);
        e.is_shields = 1;
        // Pre-era ctor has no OverstackValue fallback -- barrier == full HealthDamage.
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.barrier_count, 1);
        assert_eq!(d.barrier_damage, 40);
    }

    // ---- post-era buff==1 (DamageResult, unified) ----

    #[test]
    fn post_era_buff_cycle_counts_condition() {
        let raw = raw_post(vec![buff_dmg_event(9, 1, result::BUFF_CYCLE, 45)]);
        let d = get(&raw);
        assert_eq!(d.condition_count, 1);
        assert_eq!(d.condition_damage, 45);
    }

    #[test]
    fn post_era_life_leech_dmg_to_target_on_hit_counts_power_and_life_leech() {
        let raw = raw_post(vec![buff_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 33)]);
        let d = get(&raw);
        assert_eq!(d.life_leech_count, 1);
        assert_eq!(d.life_leech_damage, 33);
        assert_eq!(d.power_count, 1);
        assert_eq!(d.power_damage, 33);
        assert_eq!(d.condition_count, 0);
    }

    #[test]
    fn post_era_life_leech_dmg_to_target_on_stack_remove_also_counts() {
        let raw = raw_post(vec![buff_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE, 18)]);
        let d = get(&raw);
        assert_eq!(d.life_leech_count, 1);
    }

    #[test]
    fn post_era_buff_absorb_and_invert_count_invulned() {
        let raw = raw_post(vec![
            buff_dmg_event(9, 1, result::ABSORB, 999),
            buff_dmg_event(9, 1, result::INVERT, 999),
        ]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { invulned_count: 2, ..Default::default() });
    }

    #[test]
    fn post_era_direct_only_result_on_buff_row_is_dropped_entirely() {
        // A buff==1 row with a direct-only-shaped result (BLOCK) is dropped
        // entirely post-era -- must NOT leak into blocked_count.
        let raw = raw_post(vec![buff_dmg_event(9, 1, result::BLOCK, 999)]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default(), "post-era buff==1 BLOCK-shaped row must be dropped, not counted as blocked");
    }

    #[test]
    fn post_era_shields_life_leech_falls_back_to_full_amount_when_overstack_zero() {
        let mut e = buff_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 60);
        e.is_shields = 1;
        e.overstack = 0; // OverstackValue == 0 -> fall back to full HealthDamage
        let raw = raw_post(vec![e]);
        let d = get(&raw);
        assert_eq!(d.barrier_damage, 60);
    }

    #[test]
    fn post_era_shields_life_leech_prefers_overstack_when_nonzero() {
        let mut e = buff_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 60);
        e.is_shields = 1;
        e.overstack = 12;
        let raw = raw_post(vec![e]);
        let d = get(&raw);
        assert_eq!(d.barrier_damage, 12);
    }

    // ---- breakbar (separate scan) ----

    #[test]
    fn breakbar_damage_taken_is_counted_via_dedicated_scan() {
        let raw = raw_from(vec![direct(9, 1, result::BREAKBAR_DAMAGE, 250)]);
        let d = get(&raw);
        assert_eq!(d.breakbar_count, 1);
        assert_eq!(d.breakbar_damage, 250);
        assert_eq!(d.strike_count, 0, "breakbar damage must not also count as a strike hit");
    }

    // ---- dodge (self-cast, independent of incoming damage) ----

    #[test]
    fn pre_era_own_dodge_cast_counts_dodge_regardless_of_any_incoming_damage() {
        let mut e = base(1, 1); // self-directed cast row
        e.skillid = DODGE_SKILL_ID;
        e.is_activation = 1; // Normal start-cast
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d, DefenseStats { dodge_count: 1, ..Default::default() });
    }

    #[test]
    fn pre_era_quickness_activation_also_counts_dodge() {
        let mut e = base(1, 1);
        e.skillid = DODGE_SKILL_ID;
        e.is_activation = 2; // Quickness start-cast
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.dodge_count, 1);
    }

    #[test]
    fn post_era_dodge_uses_animation_start_statechange_not_is_activation() {
        let mut e = base(1, 1);
        e.skillid = DODGE_SKILL_ID;
        e.is_statechange = sc::ANIMATION_START;
        let raw = raw_post(vec![e]);
        let d = get(&raw);
        assert_eq!(d.dodge_count, 1);
    }

    #[test]
    fn pre_era_animation_start_statechange_does_not_count_without_post_era_gate() {
        // Sanity: the pre-era predicate requires is_statechange == 0; a
        // stray ANIMATION_START row on a pre-era log must not double-count.
        let mut e = base(1, 1);
        e.skillid = DODGE_SKILL_ID;
        e.is_statechange = sc::ANIMATION_START;
        let raw = raw_from(vec![e]); // pre-era header
        let d = get(&raw);
        assert_eq!(d.dodge_count, 0);
    }

    #[test]
    fn dodge_of_a_different_skill_id_does_not_count() {
        let mut e = base(1, 1);
        e.skillid = DODGE_SKILL_ID + 1;
        e.is_activation = 1;
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(d.dodge_count, 0);
    }

    #[test]
    fn evaded_incoming_hit_and_own_dodge_cast_are_independent_counters() {
        // The "dodge vs evade distinction" the M13 plan brief calls out: an
        // EVADE-result incoming hit and the player's own dodge-cast count
        // are unrelated and can diverge arbitrarily.
        let mut dodge_cast = base(1, 1);
        dodge_cast.skillid = DODGE_SKILL_ID;
        dodge_cast.is_activation = 1;
        let evaded_hit = direct(9, 1, result::EVADE, 999);
        let raw = raw_from(vec![dodge_cast, evaded_hit]);
        let d = get(&raw);
        assert_eq!(d.dodge_count, 1);
        assert_eq!(d.evaded_count, 1);
    }

    // ---- misc plumbing ----

    #[test]
    fn relog_folds_across_account_addrs() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64), (2u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![
            direct(9, 1, result::NORMAL, 10),
            direct(9, 2, result::BLOCK, 999),
        ]);
        let d = build(&raw, &squad, &addr_to_rep)[&1];
        assert_eq!(d.strike_count, 1);
        assert_eq!(d.strike_damage, 10);
        assert_eq!(d.blocked_count, 1);
    }

    #[test]
    fn any_source_counts_not_just_enemies() {
        // Unlike `hit_stats`, this scan has no `enemies` restriction --
        // ANY source hitting a squad player counts (matches
        // `damage::accumulate_damage_taken`'s own established scope).
        let raw = raw_from(vec![direct(999, 1, result::NORMAL, 40)]); // src not a modeled enemy at all
        let d = get(&raw);
        assert_eq!(d.strike_count, 1);
        assert_eq!(d.strike_damage, 40);
    }

    #[test]
    fn non_squad_destination_is_ignored() {
        let raw = raw_from(vec![direct(9, 2, result::NORMAL, 999)]); // dst 2 not in squad
        let d = get(&raw);
        assert_eq!(d, DefenseStats::default());
    }
}
