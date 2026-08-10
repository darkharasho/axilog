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
//! genuinely different from (and narrower than) "every buff==1 hit".
//!
//! ## MCONDCAT Task 1: the catalog is now reproduced, and the FOURTH BUCKET
//! with it
//!
//! Through M15 this module (and `hit_stats`) approximated
//! `ConditionDamageBased` as "buff==1 and not life-leech", because no
//! skill-id catalog existed here. `analysis::condition_catalog` now supplies
//! one -- GW2EI's complete, provably-log-independent 14-id
//! `BuffClassification.Condition` set (see that module's doc for the
//! exhaustive-scan provenance and the `BuffsByIDs`-membership proof) -- and
//! `classify` below probes it FIRST, exactly where GW2EI's ctor does,
//! ahead of the `is NonDirectHealthDamageEvent` type test. So a catalogued
//! skill id lands in `condition_*` regardless of its `buff` byte or
//! life-leech `result`, and `HitKind` gained a fourth variant:
//!
//! | wire shape | catalogued? | life-leech? | buckets incremented |
//! |---|---|---|---|
//! | `buff==0` | no  | -   | `strike_*` + `power_*` |
//! | any       | yes | -   | `condition_*` only |
//! | `buff==1` | no  | yes | `life_leech_*` + `power_*` |
//! | `buff==1` | no  | no  | **`power_*` ONLY** (`HitKind::PowerOnly`) |
//!
//! **`power_count`/`power_damage` == `strike_*` + `life_leech_*` therefore
//! NO LONGER HOLDS, deliberately** -- and it never held in GW2EI. The
//! fourth-bucket row increments the unconditional `PowerDamageTaken(Count)`
//! statement that sits OUTSIDE the `is NonDirectHealthDamageEvent` branch,
//! while hitting NEITHER `StrikeDamageTakenCount` (buff==0-only) NOR the
//! (buggy) `LifeLeechDamageTaken` increment (`IsLifeLeech`-gated). Every
//! in-module test and golden-test derivation that leaned on the old
//! identity was reworked in MCONDCAT Task 1; in particular
//! `defenses_golden.rs` no longer recovers the true life-leech reference as
//! `powerDamageTaken(Count) - strikeDamageTaken(Count)` (which now
//! over-counts by exactly the fourth bucket) -- it instead exploits the
//! double-increment bug directly, asserting `ours.life_leech_damage +
//! ours.life_leech_count == golden.lifeLeechDamageTaken`, which is
//! fourth-bucket-immune. See that test's doc comment.
//!
//! **Why this mattered (M13 Task 3's empirical finding, now resolved).**
//! The first real post-rework capture + dps.report export pair
//! (`fixtures/local/wvw-postrework.zevtc`/`.ei.json`, gitignored, dev-only)
//! showed the fourth bucket genuinely populated on a real fight: 33 of 44
//! joined accounts diverged on `power_count`/`condition_count` (up to
//! **51.4% relative** on the worst account, `power_count` -- see MCONDCAT
//! Task 1's report for the full flip table) while every field immune to the
//! buff==1 split stayed exact. It was hand-verified CONSERVED
//! in total (`ours condition + ours life_leech == golden's condition +
//! [true non-strike power]`), i.e. a pure RECLASSIFICATION, not a
//! dropped/extra event -- precisely what a missing catalog predicts. The
//! INCOMING side showed a substantially larger gap than the OUTGOING side
//! (2 of 44 accounts there), plausibly because incoming attackers span a
//! whole opposing WvW roster's build diversity rather than the recording
//! squad's own. With the catalog in place these fields are EXACT on that
//! capture for every joined account, and `defenses_golden.rs`'s real-capture
//! hook hard-fails on them again instead of merely reporting.
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

use crate::analysis::condition_catalog;
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
    /// EI's `powerDamageTaken(Count)`: every non-condition hit, whatever
    /// its wire shape. **NOT** `strike_* + life_leech_*` -- as of MCONDCAT
    /// Task 1 the fourth bucket (`HitKind::PowerOnly`: buff==1, outside the
    /// condition catalog, not life-leech) increments these and nothing
    /// else, exactly as GW2EI does. Kept as its own field (not a derived
    /// accessor) to mirror EI field-for-field.
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
    /// **OUTGOING**, unlike every other field on this struct (MEIGAP2 row
    /// 6): the raw `value` sum of this player's own dealt defiance-bar
    /// damage, minion-INCLUSIVE -- GW2EI's `dpsAll[0].breakbarDamage`
    /// (`DamageStatistics.cs:60`, `Math.Round(actor.
    /// GetBreakbarDamageEvents(target, log, start, end).Sum(x =>
    /// x.BreakbarDamage), 1)` over the minion-folded
    /// `SingleActor.InitBreakbarDamageEvents` list, `!ToFriendly`-filtered
    /// at `SingleActor.cs:869`).
    ///
    /// It lives here, on the *incoming* stats struct, for one reason:
    /// dealt and taken breakbar rows are the same result byte, so this
    /// shares [`accumulate_breakbar_and_received_cc`]'s existing scan
    /// instead of paying for a second full pass over the event list -- the
    /// same fold that already put incoming CC in this scan (see that
    /// function's doc comment). Consumers read it as an outgoing number:
    /// `axilog_schema::PlayerOut::breakbar_damage_dealt` ->
    /// `dpsAll[0].breakbarDamage`.
    ///
    /// **Raw arcdps units**, i.e. ten times GW2EI's own number:
    /// `BreakbarDamageEvent`'s ctor is `BreakbarDamage = Math.Round(
    /// evtcItem.Value / 10.0, 1)`
    /// (`ParsedData/CombatEvents/NonDamageEvents/BreakbarDamageEvent.cs:8`).
    /// The `/10` is applied once, at the EI-adapter boundary, so this
    /// field stays an exact integer sum -- the same raw-unit convention
    /// `breakbar_damage` above uses. (MEIGAP2 review: the adapter converts
    /// BOTH of them now. It used to emit `breakbarDamageTaken` unconverted,
    /// which was wrong on the same citation --
    /// `DefensePerTargetStatistics.cs:143-148` sums the already-divided
    /// `brk.BreakbarDamage` -- and invisible, since both reference exports
    /// report 0 for it.)
    pub breakbar_damage_dealt: u64,
    /// EI's `defenses[].receivedCrowdControl` -- the number of crowd-
    /// control applications this player RECEIVED (MEIGAP Task 1c).
    ///
    /// The exact mirror of `PlayerMetrics::cc_applied`'s outgoing count,
    /// off the same already-verified `cc::is_cc` predicate: GW2EI's
    /// `DefensePerTargetStatistics.cs:136-141` counts one per
    /// `CrowdControlEvent` in `GetIncomingCrowdControlEvents`, and a
    /// `CrowdControlEvent` is precisely a damage-shaped row whose `Result`
    /// is `DamageResult.CrowdControl` (`CombatEventFactory.cs:811-813`,
    /// reachable from both the direct and buff damage paths) -- which is
    /// what `cc::is_cc` already decides, era-gated.
    ///
    /// **Two deliberate asymmetries vs the outgoing count**, both read
    /// straight off GW2EI's own source and both reproduced here:
    ///
    /// - **No source filter at all.** `SingleActor.cs:935-943` builds the
    ///   incoming list from `CombatData.GetIncomingCrowdControlData(
    ///   AgentItem)` with no `ToFriendly`/`ToFoe` predicate, unlike the
    ///   OUTGOING init at `:918` which does `.Where(x => !x.ToFriendly)`.
    ///   So friendly-sourced and unknown-sourced CC on a squad player
    ///   counts here, where `cc::apply_cc`'s outgoing pass restricts to
    ///   `enemies` recipients.
    /// - **No pet/minion fold.** The outgoing side additionally merges the
    ///   actor's minions' CC (`SingleActor.cs:919-923`, reproduced by
    ///   `cc::pet_credit_cc_events`); the incoming side does not, so this
    ///   is keyed purely on the event's own destination.
    pub received_cc_count: u32,
    /// EI's `defenses[].receivedCrowdControlDuration`, in MILLISECONDS --
    /// the sum of `CrowdControlEvent.Duration`, itself the raw
    /// `evtcItem.Value` (`CrowdControlEvent.cs:12`), with no `/1000` and no
    /// rounding anywhere on the incoming path
    /// (`DefensePerTargetStatistics.cs:139`). Same ms convention as
    /// `PlayerMetrics::cc_duration_ms` on the outgoing side.
    pub received_cc_duration_ms: u64,
    /// EI's `defenses[].boonStrips` -- the number of BOONS stripped OFF
    /// this player (MEIGAP Task 1c). The incoming counterpart of
    /// `support::SupportMetrics::strips`, sharing that module's
    /// already-calibrated `BuffRemoveAllEvent` machinery and its inverted
    /// role convention (`owner = src_agent`, `remover = dst_agent`, both
    /// read RAW -- see `support`'s module doc for the full citation).
    ///
    /// GW2EI: `DefensePerTargetStatistics.cs:149` ->
    /// `GetStripData(BuffsByClassification[Boon], ..., excludeSelf: true)`
    /// at `:48-70`, which walks `GetBuffRemoveAllEventsFromByID` (i.e.
    /// `CombatData.GetBuffRemoveAllDataByDst(AgentItem)` -- removals whose
    /// VICTIM is this actor) and skips a removal when
    /// `brae.CreditedBy.IsUnknown` or `brae.CreditedBy.Is(actor.AgentItem)`.
    ///
    /// **Both skips test `CreditedBy`, not the raw remover.** `CreditedBy`
    /// is `By.GetFinalMaster()` (`BuffEvent.cs:13`), so the MINION fold
    /// happens first and both tests then apply to the resolved owner: a
    /// player's own minion's strip on himself is a self-removal, and a
    /// removal is "unknown" only when the OWNER cannot be resolved. Both
    /// are reproduced in [`incoming_boon_strips`], through the same
    /// single-hop `InstidRegistry` master fold every other pet-crediting
    /// pass in this crate uses.
    pub boon_strips_taken: u32,
    /// The sum of `BuffRemoveAllEvent.RemovedDuration` (raw
    /// `evtcItem.Value`, ms) over exactly the removals
    /// [`boon_strips_taken`] counts -- i.e. how much boon duration was
    /// destroyed on this player.
    ///
    /// **This is deliberately NOT a reproduction of EI's own
    /// `boonStripsTime`**, which is affected by a real, verified GW2EI
    /// arithmetic bug: `DefensePerTargetStatistics.cs:63` reads
    /// `currentBoonStripTime = Math.Max(currentBoonStripTime +
    /// brae.RemovedDuration, log.LogData.LogDuration)` -- a `Max` where a
    /// `Min` (clamping one removal's remaining duration to the log length)
    /// was plainly intended. As written, the FIRST qualifying strip of any
    /// boon pins that boon's accumulator to at least the whole log
    /// duration, so the exported number is essentially
    /// `distinct_boons_stripped * logDuration`, not a duration sum at all.
    /// Verified on this project's reference export: a player with
    /// `boonStrips: 4` across 4 distinct boons reports `boonStripsTime:
    /// 1393.448` on a 348,362 ms log -- exactly `4 * 348.362`.
    ///
    /// This field therefore carries the TRUE sum, the same "axilog is more
    /// correct here, not less" convention `life_leech_count` above already
    /// establishes for GW2EI's other verified counting bug in this same
    /// class. The adapter documents the divergence, and
    /// `crates/axilog-ei/tests/meigap_ei_golden.rs` calibrates against the
    /// export by reconstructing EI's buggy formula from this project's own
    /// per-boon strip data -- which pins the removal SET exactly without
    /// enshrining the bug in the output.
    ///
    /// Milliseconds (EI reports its own value in seconds; the conversion
    /// lives in the adapter, matching how `removed_stun_duration_ms` is
    /// already handled).
    pub boon_strips_taken_duration_ms: u64,
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
        self.breakbar_damage_dealt += o.breakbar_damage_dealt;
        self.received_cc_count += o.received_cc_count;
        self.received_cc_duration_ms += o.received_cc_duration_ms;
        self.boon_strips_taken += o.boon_strips_taken;
        self.boon_strips_taken_duration_ms += o.boon_strips_taken_duration_ms;
    }
}

/// A HIT event's damage-bucket classification (see module doc's "Damage-
/// taken breakdown" section). One variant per branch of
/// `DefensePerTargetStatistics`'s ctor -- including `PowerOnly`, the FOURTH
/// BUCKET that MCONDCAT Task 1 made representable.
pub(crate) enum HitKind {
    /// `buff == 0`, not catalogued: `strike_*` AND `power_*`.
    Strike,
    /// Skill id ∈ `condition_catalog`: `condition_*` ONLY (never power).
    Condition,
    /// `buff == 1`, not catalogued, life-leech `result`/`BuffCycle`:
    /// `life_leech_*` AND `power_*`.
    LifeLeech,
    /// **The fourth bucket.** `buff == 1`, NOT catalogued, NOT life-leech:
    /// `power_*` ONLY -- GW2EI's `StrikeDamageTakenCount` is buff==0-only
    /// and its (buggy) life-leech increment is `IsLifeLeech`-gated, so
    /// neither fires, while the unconditional `PowerDamageTaken(Count)`
    /// statement outside the inner `if` does. This is what breaks the
    /// `power == strike + life_leech` identity -- in GW2EI, and now
    /// faithfully here too.
    PowerOnly,
}

/// One incoming damage-shaped row's classification -- unlike `hit_stats::
/// Classified` (which only exists for HITS), this covers every outcome
/// `DefensePerTargetStatistics` tracks, hit or not.
pub(crate) enum Outcome {
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
pub(crate) fn classify_outcome(e: &RawEvent, post_era: bool) -> Option<Outcome> {
    classify(e, post_era)
}

fn classify(e: &RawEvent, post_era: bool) -> Option<Outcome> {
    let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };

    // MCONDCAT Task 1: GW2EI probes `ConditionDamageBased` FIRST, ahead of
    // its `is NonDirectHealthDamageEvent` type test, so the catalog wins
    // over the wire shape on every era and both `buff` values -- see
    // `condition_catalog::is_condition_damage_based`'s doc comment.
    let catalogued = condition_catalog::is_condition_damage_based(e.skillid);

    if e.buff == 0 {
        return match e.result {
            result::NORMAL | result::CRIT | result::GLANCE => {
                let shield_dmg = if e.is_shields != 0 { e.overstack as u64 } else { 0 };
                let kind = if catalogued { HitKind::Condition } else { HitKind::Strike };
                Some(Outcome::Hit { dmg, kind, shield_dmg })
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
                // Catalogued -> Condition; otherwise the FOURTH BUCKET
                // (power-only), NOT "condition by default" as pre-MCONDCAT.
                let kind = if catalogued { HitKind::Condition } else { HitKind::PowerOnly };
                Some(Outcome::Hit { dmg, kind, shield_dmg })
            }
            result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT
            | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE => {
                let shield_dmg = shield_damage_nondirect_post(e, dmg);
                // The catalog still wins over life-leech: GW2EI's condition
                // `if` fires before `ndhd.IsLifeLeech` is ever consulted.
                let kind = if catalogued { HitKind::Condition } else { HitKind::LifeLeech };
                Some(Outcome::Hit { dmg, kind, shield_dmg })
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
                let kind = if catalogued {
                    HitKind::Condition
                } else if is_life_leech {
                    HitKind::LifeLeech
                } else {
                    HitKind::PowerOnly
                };
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
                // The fourth bucket: power ONLY. No strike, no condition,
                // no life-leech -- see `HitKind::PowerOnly`'s doc comment.
                HitKind::PowerOnly => {
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
/// Breakbar damage taken, and (MEIGAP Task 1c) incoming crowd control --
/// two result-byte classifications over the same `dst in squad` event
/// family, so they share one scan rather than paying for two full passes
/// over a 583k-event log (see `docs/BENCHMARKS.md`'s MEIGAP entry).
fn accumulate_breakbar_and_received_cc(
    events: &[RawEvent],
    registry: &crate::analysis::damage::InstidRegistry,
    squad: &BTreeSet<u64>,
    post_era: bool,
    out: &mut BTreeMap<u64, DefenseStats>,
) {
    for e in events {
        // Byte-level classification BEFORE the `squad` set lookup: the
        // membership test is a `BTreeSet<u64>` probe and by far the most
        // expensive thing in this loop, while both classifications are a
        // handful of byte compares that reject the overwhelming majority
        // of rows. (Measured: hoisting the `squad` check to the top of the
        // loop instead cost +4% on `analysis::analyze` over the real log.)
        //
        // `cc::is_cc` carries its own era-gated statechange/buff handling
        // (see [`DefenseStats::received_cc_count`]); a CC row is never a
        // breakbar row.
        let is_cc = crate::analysis::cc::is_cc(e, post_era);
        let is_breakbar = !is_cc
            && e.is_statechange == 0
            && e.is_activation == 0
            && e.is_buffremove == 0
            && e.result == result::BREAKBAR_DAMAGE;
        if !is_cc && !is_breakbar {
            continue;
        }
        // MEIGAP2 row 6: the OUTGOING half of the breakbar rows this scan
        // already visits (see `DefenseStats::breakbar_damage_dealt`). Kept
        // above the `dst in squad` gate on purpose -- a squad player's
        // breakbar damage lands on an ENEMY, so the incoming test below
        // would reject every row this needs. GW2EI's `!ToFriendly` filter
        // (`SingleActor.cs:869`) is the `iff` byte, and the minion fold is
        // the same `src_master_instid` -> owner resolution the squad-side
        // damage pet credit uses.
        if is_breakbar && e.iff != 0 {
            let owner = if squad.contains(&e.src_agent) {
                Some(e.src_agent)
            } else if e.src_master_instid != 0 {
                registry.resolve_at(e.src_master_instid, e.time).filter(|a| squad.contains(a))
            } else {
                None
            };
            if let Some(owner) = owner {
                out.entry(owner).or_default().breakbar_damage_dealt += e.value.max(0) as u64;
            }
        }
        if !squad.contains(&e.dst_agent) {
            continue;
        }
        let stats = out.entry(e.dst_agent).or_default();
        if is_cc {
            stats.received_cc_count += 1;
            stats.received_cc_duration_ms += e.value.max(0) as u64;
        } else {
            stats.breakbar_count += 1;
            stats.breakbar_damage += e.value.max(0) as u64;
        }
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
/// Every boon-strip this squad player SUFFERED, as `(boon id, removed
/// duration ms)` in log order, keyed by the player's account-representative
/// addr (MEIGAP Task 1c).
///
/// The shared primitive behind [`DefenseStats::boon_strips_taken`] and
/// [`DefenseStats::boon_strips_taken_duration_ms`] -- see those fields for
/// the GW2EI citation trail (`DefensePerTargetStatistics.cs:48-70,149`),
/// and `support`'s module doc for the `BuffRemoveAllEvent` role inversion
/// and era dispatch this mirrors on the incoming side.
///
/// Public because the strip DETAIL (not just its two rollups) is what the
/// ei-json calibration needs: GW2EI's own exported `boonStripsTime` is
/// computed by a buggy per-boon accumulator (see
/// [`DefenseStats::boon_strips_taken_duration_ms`]), and reconstructing
/// that formula from this list is how the reference export is joined
/// without reproducing the bug in axilog's own output.
pub fn incoming_boon_strips(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, Vec<(u32, u64)>> {
    incoming_boon_strips_with_registry(
        raw,
        &crate::analysis::damage::InstidRegistry::build(raw),
        squad,
        addr_to_rep,
    )
}

/// [`incoming_boon_strips`] against a caller-supplied, already-built
/// [`InstidRegistry`] (MPERF Task 2 convention) -- the registry is what
/// resolves `CreditedBy`'s minion fold. The `raw`-only wrapper above stays
/// for standalone/test callers.
pub fn incoming_boon_strips_with_registry(
    raw: &RawLog,
    registry: &crate::analysis::damage::InstidRegistry,
    squad: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, Vec<(u32, u64)>> {
    let post_era = raw.header.is_post_buff_rework();
    let boon_ids: BTreeSet<u32> =
        crate::analysis::buffs::BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    // `CreditedBy.IsUnknown` (`DefensePerTargetStatistics.cs:60`): GW2EI's
    // `AgentItem.Unknown` is the placeholder it hands out for an addr that
    // never appeared in the log's agent table, so membership in that table
    // is exactly the test -- applied to the MASTER-FOLDED remover, since
    // `CreditedBy` is `By.GetFinalMaster()`.
    let known_agents: BTreeSet<u64> = raw.agents.iter().map(|a| a.addr).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    let mut out: BTreeMap<u64, Vec<(u32, u64)>> = BTreeMap::new();
    for e in &raw.events {
        // Era-dispatched removal predicate, identical to `support::apply`/
        // `support::apply_post_era`'s own (see that module's doc comment):
        // pre-era an ordinary combat row with `is_buffremove == ALL`,
        // post-era the dedicated `sc::BUFF_REMOVE_ALL` statechange.
        let is_remove_all = if post_era {
            e.is_statechange == crate::evtc::sc::BUFF_REMOVE_ALL
        } else {
            e.is_statechange == 0
                && e.is_activation == 0
                && e.is_buffremove == crate::evtc::buff_remove::ALL
        };
        if !is_remove_all || !boon_ids.contains(&e.skillid) {
            continue;
        }
        // Role inversion (see `support`'s module doc): the removal's VICTIM
        // is `src_agent`, the REMOVER is `dst_agent`.
        let victim = e.src_agent;
        let remover = e.dst_agent;
        if !squad.contains(&victim) {
            continue;
        }
        // `CreditedBy = By.GetFinalMaster()` (`BuffEvent.cs:13`): fold the
        // remover onto its owner BEFORE either skip test, through the same
        // single-hop `*_master_instid` -> registry resolution
        // `damage::pet_credit_events`/`cc::pet_credit_cc_events`/
        // `downs::credited_squad_source` all use. Roles are inverted on a
        // removal row, so it is the `dst_*` triple that describes the
        // remover -- the same read `contribution::credit_window`'s strips
        // branch already makes. A removal with no master link resolves to
        // the remover itself, which is what `GetFinalMaster` returns for a
        // masterless agent.
        let credited = registry.resolve_at(e.dst_master_instid, e.time).unwrap_or(remover);
        if !known_agents.contains(&credited) {
            continue; // `CreditedBy.IsUnknown`
        }
        if rep(credited) == rep(victim) {
            continue; // `excludeSelf` (`CreditedBy.Is(actor.AgentItem)`)
        }
        out.entry(rep(victim)).or_default().push((e.skillid, e.value.max(0) as u64));
    }
    out
}

pub fn build(raw: &RawLog, squad: &BTreeSet<u64>, addr_to_rep: &BTreeMap<u64, u64>) -> BTreeMap<u64, DefenseStats> {
    build_with_registry(raw, &crate::analysis::damage::InstidRegistry::build(raw), squad, addr_to_rep)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`]
/// (MPERF Task 2 convention) -- needed since MEIGAP Task 1c, whose incoming
/// boon strips resolve `CreditedBy`'s minion fold through it. The
/// `raw`-only wrapper above stays for standalone/test callers.
pub fn build_with_registry(
    raw: &RawLog,
    registry: &crate::analysis::damage::InstidRegistry,
    squad: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, DefenseStats> {
    let post_era = raw.header.is_post_buff_rework();
    let mut by_addr = accumulate(&raw.events, squad, post_era);
    accumulate_breakbar_and_received_cc(&raw.events, registry, squad, post_era, &mut by_addr);
    accumulate_dodges(raw, squad, post_era, &mut by_addr);

    let mut by_rep: BTreeMap<u64, DefenseStats> = BTreeMap::new();
    for (addr, stats) in by_addr {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        by_rep.entry(rep).or_default().merge(&stats);
    }
    // Incoming boon strips fold themselves (their shared primitive is
    // already keyed by representative addr, since the self-exclusion needs
    // the relog fold to decide "self" correctly in the first place).
    for (rep, strips) in incoming_boon_strips_with_registry(raw, registry, squad, addr_to_rep) {
        let stats = by_rep.entry(rep).or_default();
        stats.boon_strips_taken += strips.len() as u32;
        stats.boon_strips_taken_duration_ms += strips.iter().map(|&(_, ms)| ms).sum::<u64>();
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
            is_ninety: 0, is_fifty: 0,
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

    /// A buff==1 damage row carrying an UNCATALOGUED skill id (`base`'s
    /// default `skillid: 1`). Post-MCONDCAT this is a FOURTH-BUCKET
    /// (`HitKind::PowerOnly`) row unless its `result`/`BuffCycle` makes it
    /// life-leech -- use `condi_dmg_event` for the condition bucket.
    fn buff_dmg_event(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = base(src, dst);
        e.buff = 1;
        e.result = result_;
        e.buff_dmg = dmg;
        e
    }

    /// Same as `buff_dmg_event`, but with a CATALOGUED skill id (Bleeding,
    /// 736) -- one GW2EI's `ConditionDamageBased` returns true for.
    fn condi_dmg_event(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = buff_dmg_event(src, dst, result_, dmg);
        e.skillid = condition_catalog::BLEEDING;
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

    /// A minimal agent-table row, so `incoming_boon_strips`'
    /// `CreditedBy.IsUnknown` check has something to resolve against.
    fn agent(addr: u64) -> crate::evtc::RawAgent {
        crate::evtc::RawAgent {
            addr, prof: 1, is_elite: 0, toughness: 0, concentration: 0, healing: 0,
            hitbox_width: 0, condition: 0, hitbox_height: 0, name_raw: vec![],
        }
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
    /// A CC row is excluded from every DAMAGE/outcome counter -- and, since
    /// MEIGAP Task 1c, is the sole input to the two `received_cc_*` fields
    /// (see [`DefenseStats::received_cc_count`]). `value` carries the CC
    /// duration in ms, never damage.
    fn crowd_control_result_only_feeds_the_received_cc_fields() {
        let mut e = direct(9, 1, result::CROWD_CONTROL, 0);
        e.value = 1500; // CC duration ms, not damage
        let raw = raw_from(vec![e]);
        let d = get(&raw);
        assert_eq!(
            d,
            DefenseStats {
                received_cc_count: 1,
                received_cc_duration_ms: 1500,
                ..DefenseStats::default()
            }
        );
    }

    /// The incoming CC count is deliberately NOT source-filtered (GW2EI's
    /// `SingleActor.cs:935-943` has no `ToFriendly` predicate on the
    /// incoming list, unlike the outgoing one at `:918`) -- a friendly-
    /// sourced CC on a squad player still counts.
    #[test]
    fn received_cc_counts_friendly_sourced_crowd_control() {
        let mut e = direct(1, 2, result::CROWD_CONTROL, 0);
        e.value = 700;
        e.iff = 0; // FRIEND
        let raw = raw_from(vec![e]);
        let squad: BTreeSet<u64> = [1u64, 2].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(by_rep[&2].received_cc_count, 1);
        assert_eq!(by_rep[&2].received_cc_duration_ms, 700);
        assert!(!by_rep.contains_key(&1), "CC is credited to the victim, never the source");
    }

    /// Incoming boon strips: a hostile `BUFFREMOVE_ALL` of a boon off a
    /// squad player counts, with the removal's remaining duration; a
    /// SELF-removal of the same shape does not (`excludeSelf`,
    /// `DefensePerTargetStatistics.cs:60`).
    #[test]
    fn incoming_boon_strips_count_hostile_removals_and_skip_self() {
        let mut hostile = base(1, 9); // victim = src_agent, remover = dst_agent
        hostile.skillid = crate::analysis::buffs::MIGHT;
        hostile.is_buffremove = crate::evtc::buff_remove::ALL;
        hostile.value = 2400;
        let mut own = base(1, 1);
        own.skillid = crate::analysis::buffs::MIGHT;
        own.is_buffremove = crate::evtc::buff_remove::ALL;
        own.value = 9000;
        let mut raw = raw_from(vec![hostile, own]);
        // `CreditedBy.IsUnknown` is decided by agent-table membership, so
        // both removers must be real agents for this test to exercise the
        // self-exclusion rather than the unknown-exclusion.
        raw.agents = vec![agent(1), agent(9)];
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(by_rep[&1].boon_strips_taken, 1);
        assert_eq!(by_rep[&1].boon_strips_taken_duration_ms, 2400);
    }

    /// `CreditedBy = By.GetFinalMaster()`: a squad player's own MINION
    /// stripping a boon off him is a SELF-removal and must not count, even
    /// though the raw remover addr differs from the victim's
    /// (`DefensePerTargetStatistics.cs:61`'s `excludeSelf` test is on
    /// `CreditedBy`, not on `By`).
    #[test]
    fn incoming_boon_strips_exclude_a_players_own_minion_via_the_master_fold() {
        // The minion (addr 50, instid 5) is registered as owned by player 1
        // (instid 1) through an ordinary earlier event.
        let mut intro = base(50, 9);
        intro.src_instid = 5;
        intro.src_master_instid = 1;
        let mut owner_intro = base(1, 9);
        owner_intro.src_instid = 1;

        let mut minion_strip = base(1, 50); // victim = 1, remover = the minion
        minion_strip.skillid = crate::analysis::buffs::MIGHT;
        minion_strip.is_buffremove = crate::evtc::buff_remove::ALL;
        minion_strip.value = 3000;
        minion_strip.dst_instid = 5;
        minion_strip.dst_master_instid = 1; // -> resolves to player 1 == the victim

        let mut raw = raw_from(vec![owner_intro, intro, minion_strip]);
        raw.agents = vec![agent(1), agent(50), agent(9)];
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(
            by_rep.get(&1).map(|d| d.boon_strips_taken).unwrap_or(0),
            0,
            "a player's own minion's strip is a self-removal under CreditedBy"
        );
    }

    /// The mirror: an ENEMY's minion still counts, and is credited by the
    /// victim (the strip is an incoming stat, so the owner's identity only
    /// matters for the two skip tests).
    #[test]
    fn incoming_boon_strips_count_an_enemy_minion_after_the_master_fold() {
        let mut owner_intro = base(9, 1);
        owner_intro.src_instid = 9;
        let mut strip = base(1, 50);
        strip.skillid = crate::analysis::buffs::MIGHT;
        strip.is_buffremove = crate::evtc::buff_remove::ALL;
        strip.value = 3000;
        strip.dst_instid = 5;
        strip.dst_master_instid = 9; // -> enemy 9, not the victim
        let mut raw = raw_from(vec![owner_intro, strip]);
        raw.agents = vec![agent(1), agent(9), agent(50)];
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(by_rep[&1].boon_strips_taken, 1);
        assert_eq!(by_rep[&1].boon_strips_taken_duration_ms, 3000);
    }

    /// A removal credited to an agent the log never enumerated is dropped
    /// (`brae.CreditedBy.IsUnknown`, `DefensePerTargetStatistics.cs:60`).
    #[test]
    fn incoming_boon_strips_skip_unknown_removers() {
        let mut e = base(1, 12345);
        e.skillid = crate::analysis::buffs::MIGHT;
        e.is_buffremove = crate::evtc::buff_remove::ALL;
        e.value = 2400;
        let mut raw = raw_from(vec![e]);
        raw.agents = vec![agent(1)]; // 12345 is not in the agent table
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(by_rep.get(&1).map(|d| d.boon_strips_taken).unwrap_or(0), 0);
    }

    /// Only BOONS -- a condition's `BUFFREMOVE_ALL` is a cleanse, not a
    /// strip (`BuffsByClassification[Boon]`,
    /// `DefensePerTargetStatistics.cs:149`).
    #[test]
    fn incoming_boon_strips_ignore_condition_removals() {
        let mut e = base(1, 9);
        e.skillid = crate::analysis::condition_catalog::BLEEDING;
        e.is_buffremove = crate::evtc::buff_remove::ALL;
        e.value = 2400;
        let mut raw = raw_from(vec![e]);
        raw.agents = vec![agent(1), agent(9)];
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let by_rep = build(&raw, &squad, &BTreeMap::new());
        assert_eq!(by_rep.get(&1).map(|d| d.boon_strips_taken).unwrap_or(0), 0);
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
        let raw = raw_from(vec![condi_dmg_event(9, 1, 0, 30)]);
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
        let mut e = condi_dmg_event(9, 1, 0, 40);
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
        let raw = raw_post(vec![condi_dmg_event(9, 1, result::BUFF_CYCLE, 45)]);
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

    // ---- MCONDCAT Task 1: the four buckets ----

    /// Bucket 1 (strike): buff==0, uncatalogued -> strike AND power.
    #[test]
    fn bucket_strike_uncatalogued_direct_hit() {
        let raw = raw_post(vec![direct(9, 1, result::NORMAL, 100)]);
        let d = get(&raw);
        assert_eq!((d.strike_count, d.strike_damage), (1, 100));
        assert_eq!((d.power_count, d.power_damage), (1, 100));
        assert_eq!(d.condition_count, 0);
        assert_eq!(d.life_leech_count, 0);
    }

    /// Bucket 2 (condition): catalogued -> condition ONLY, never power.
    #[test]
    fn bucket_condition_is_decided_by_the_catalog() {
        let raw = raw_post(vec![condi_dmg_event(9, 1, result::BUFF_CYCLE, 45)]);
        let d = get(&raw);
        assert_eq!((d.condition_count, d.condition_damage), (1, 45));
        assert_eq!((d.power_count, d.power_damage), (0, 0));
        assert_eq!(d.strike_count, 0);
        assert_eq!(d.life_leech_count, 0);
    }

    /// Bucket 3 (life-leech): buff==1, uncatalogued, life-leech `result`
    /// -> life-leech AND power.
    #[test]
    fn bucket_life_leech_uncatalogued_leech_result() {
        let raw =
            raw_post(vec![buff_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 33)]);
        let d = get(&raw);
        assert_eq!((d.life_leech_count, d.life_leech_damage), (1, 33));
        assert_eq!((d.power_count, d.power_damage), (1, 33));
        assert_eq!(d.strike_count, 0);
        assert_eq!(d.condition_count, 0);
    }

    /// **Bucket 4 (the fourth bucket)**: buff==1, UNCATALOGUED skill id, NOT
    /// life-leech -> `power_*` ONLY. Pre-MCONDCAT this row was silently
    /// counted as a condition hit; GW2EI's ctor reaches neither
    /// `StrikeDamageTakenCount` (buff==0-only) nor the `IsLifeLeech`-gated
    /// increment, but DOES execute the unconditional
    /// `PowerDamageTaken(Count)` statement outside the inner `if`.
    #[test]
    fn bucket_fourth_uncatalogued_buff_hit_counts_power_only() {
        let raw = raw_post(vec![buff_dmg_event(9, 1, result::BUFF_CYCLE, 45)]);
        let d = get(&raw);
        assert_eq!((d.power_count, d.power_damage), (1, 45), "fourth bucket IS power");
        assert_eq!(d.condition_count, 0, "fourth bucket must NOT count as condition");
        assert_eq!(d.condition_damage, 0);
        assert_eq!(d.strike_count, 0, "fourth bucket must NOT count as strike");
        assert_eq!(d.life_leech_count, 0, "fourth bucket must NOT count as life-leech");
        // The old by-construction identity is now legitimately broken --
        // exactly as it is in real GW2EI's own output.
        assert_ne!(
            d.power_count,
            d.strike_count + d.life_leech_count,
            "`power == strike + life_leech` must NOT hold on a fourth-bucket row"
        );
    }

    /// Same pre-era: `is_offcycle` is a `BuffCycle` byte that is neither 3
    /// nor 5, so the row is not life-leech, and the skill id is not
    /// catalogued -> fourth bucket.
    #[test]
    fn bucket_fourth_pre_era_uncatalogued_tick_counts_power_only() {
        let raw = raw_from(vec![buff_dmg_event(9, 1, 0, 30)]);
        let d = get(&raw);
        assert_eq!((d.power_count, d.power_damage), (1, 30));
        assert_eq!(d.condition_count, 0);
        assert_eq!(d.strike_count, 0);
        assert_eq!(d.life_leech_count, 0);
    }

    /// The catalog probe runs BEFORE the `is NonDirectHealthDamageEvent`
    /// type test, so a buff==0 STRIKE row with a catalogued skill id is a
    /// condition hit and contributes to NEITHER strike nor power.
    #[test]
    fn catalogued_skill_id_on_a_direct_row_still_counts_as_condition() {
        let mut e = direct(9, 1, result::CRIT, 50);
        e.skillid = condition_catalog::BURNING;
        let raw = raw_post(vec![e]);
        let d = get(&raw);
        assert_eq!((d.condition_count, d.condition_damage), (1, 50));
        assert_eq!(d.strike_count, 0);
        assert_eq!(d.power_count, 0);
    }

    /// And the catalog beats life-leech too (GW2EI's condition `if` fires
    /// before `ndhd.IsLifeLeech` is consulted at all).
    #[test]
    fn catalogued_skill_id_beats_the_life_leech_result() {
        let raw =
            raw_post(vec![condi_dmg_event(9, 1, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 21)]);
        let d = get(&raw);
        assert_eq!((d.condition_count, d.condition_damage), (1, 21));
        assert_eq!(d.life_leech_count, 0);
        assert_eq!(d.power_count, 0);
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
