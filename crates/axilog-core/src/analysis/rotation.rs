//! Per-player rotation (cast tracking) -- M14, Task 1.
//!
//! Reproduces the ANIMATED-CAST subset of GW2EI's `rotation[]` (the
//! `JsonRotation`/`JsonRotation.JsonSkill` shape:
//! `{ id, skills: [{ castTime, duration, timeGained, quickness }] }`),
//! grouped by skill id, one entry per squad player (account-folded via
//! `addr_to_rep`, same convention as every other per-player pass in this
//! module).
//!
//! # Wire-level source (verified against the live arcdps EVTC reference,
//! `curl https://www.deltaconnected.com/arcdps/evtc/README.txt`, 2026-08-09)
//!
//! Two independent wire shapes exist for "this agent started/stopped a
//! skill-cast animation", exactly mirroring the pre/post `is_post_buff_rework`
//! era split already used throughout this codebase (`support::apply` /
//! `apply_post_era`'s resurrect-cast scan is the closest existing analogue,
//! and this module's era predicates are the SAME ones, generalized from
//! "start-cast only" to "start+end pairing"):
//!
//! - **Pre-era** (`is_post_buff_rework() == false`): an ordinary `is_statechange
//!   == 0` combat event (`enum cbtanimation`, hand-counted from
//!   `ACTV_NONE = 0`): `is_activation == ACTV_START_DEFUNC(1)` or
//!   `ACTV_QUICKNESS_DEFUNC(2)` is a START row; `ACTV_MINIMUM(3)`,
//!   `ACTV_CANCEL(4)`, `ACTV_RESET(5)`, or `ACTV_NODATA(6)` is an END row.
//!   (`ACTV_UNKNOWN(7)` never appears as a start/end trigger.)
//! - **Post-era** (`is_post_buff_rework() == true`): the dedicated
//!   `sc::ANIMATION_START`(67)/`sc::ANIMATION_STOP`(68) statechanges (see
//!   their doc comments in `evtc::event` for the full ordinal citation).
//!   The END row's `is_activation` byte KEEPS its ordinary `cbtanimation`
//!   meaning here (`Minimum`/`Cancel`/`Reset`/`NoData`) -- it is NOT
//!   overloaded away the way e.g. `is_buffremove` is on other post-era
//!   statechanges.
//!
//! Both era's `value`/`buff_dmg` fields on start/end rows carry the SAME
//! roles (this is the one pair of fields the arcdps reference does NOT
//! separately document for the pre-era overloaded-`CBTS_COMBAT` shape --
//! GW2EI's `CombatItem` class reads them identically regardless of which
//! predicate classified the row as a start/end cast event, so GW2EI is the
//! arbiter for this parity, per this project's established citation
//! policy): start row `value`/`buff_dmg` = "ms duration until minimum
//! trigger point"/"ms duration when control returned to agent"; end row
//! `value`/`buff_dmg` = "ms duration spent in animation SCALED for
//! speed"/"...NOT scaled".
//!
//! This project's `is_post_buff_rework` `20260501` gate is a conservative
//! (never-under-shoots) proxy for GW2EI's own, earlier
//! `ArcDPSBuilds.AnimationAsStateChanges = 20260430` threshold that
//! actually governs this era split (see `sc::ANIMATION_START`'s doc
//! comment for the full explanation) -- the same one known gap (a log built
//! in `[20260430, 20260501)`) applies here too.
//!
//! # Cast-math arbiter: GW2EI `CombatEventFactory.CreateCastEvents` /
//! `AnimatedCastEvent` (verified against `baaron4/GW2-Elite-Insights-Parser`,
//! `master` as of 2026-08-09)
//!
//! - `GW2EIEvtcParser/CombatItem.cs` (`IsStartCastEvent`/`IsEndCastEvent`,
//!   the era-gated predicates cited above) and `IsCastEvent`.
//! - `GW2EIEvtcParser/ParsedData/CombatData.cs` (`castCombatEvents.AddToList
//!   (combatItem.SrcAgent, combatItem)` -- cast rows are bucketed by the
//!   RAW, un-resolved `SrcAgent`, i.e. the caster; `CreateCastEvents` is
//!   called once per whole log, its result later grouped by the RESOLVED
//!   `Caster` AgentItem for each actor's `GetCastEvents`).
//! - `GW2EIEvtcParser/ParsedData/CombatEvents/CombatEventFactory.cs`
//!   (`CreateCastEvents`/`CreateAnimatedCastEvent`): per raw caster addr,
//!   per skill id (in time order), a start/end PAIRING state machine:
//!   - START while a previous START is still pending (no END arrived) ->
//!     flush the pending one as a START-ONLY ("dangling") cast, then this
//!     one becomes the new pending START.
//!   - END while a START is pending -> pairs them into one cast.
//!   - END with NO pending START -> an END-ONLY cast, backdated
//!     (`Time = end.Time - ActualDuration`) -- kept ONLY if that backdated
//!     time is `< 0` (before this project's own log-start zero-point,
//!     `raw.events.first().time` -- the SAME `t0` convention `timeseries`/
//!     `cc::timeline` already use and are calibrated exact against). This
//!     is the plan brief's "pre-log cast, `cast_time` may be NEGATIVE"
//!     case.
//!   - Any pending START left after the skill's whole event list is
//!     exhausted -> flushed as START-ONLY too.
//!   - Per skill id, immediately after building that skill's cast list:
//!     `RemoveAll(x => x.Caster.IsPlayer && x.ActualDuration <= 1)` --
//!     every cast this module ever builds is for a squad player, so this
//!     always applies (a near-zero-duration animation, e.g. a double-fired
//!     activation byte, is dropped entirely).
//!   - THEN, across ALL skill ids for one caster together: sort by time,
//!     and for every ADJACENT pair, trim (`CutAt`) the earlier cast's
//!     duration if it would run past `next.Time + ServerDelayConstant(10ms)`
//!     -- but ONLY for a START-ONLY ("Unknown status") cast (a paired or
//!     END-ONLY cast never gets trimmed this way). A START-ONLY cast is
//!     ALSO capped once, at construction time, against the whole log's end
//!     (`logData.EvtcLogEnd`, this project's `raw.events.last().time`,
//!     zero-based the same way) -- `CutAt` only ever SHRINKS a duration,
//!     never grows it, so both caps combine to "whichever is smaller".
//!
//! - `AnimatedCastEvent`'s constructor + `SetAcceleration` (the exact
//!   `TimeGained`/`Quickness` derivation, reproduced field-for-field below):
//!   - `ExpectedDuration = start.BuffDmg > 0 ? start.BuffDmg : start.Value`
//!     when a start row exists; `= ActualDuration` (see below) otherwise.
//!   - **Both present**: `ActualDuration = end.Value`,
//!     `scaled_ref = end.BuffDmg`. Sanity check: if
//!     `|ActualDuration - (end.Time - start.Time)| > ServerDelayConstant(10)`,
//!     distrust the wire `ActualDuration` -- replace it with the observed
//!     wall-clock `end.Time - start.Time` and zero `scaled_ref` (no
//!     quickness estimate for this cast).
//!   - **Start missing** (backdated end-only cast): `ActualDuration =
//!     end.Value`, `scaled_ref = end.BuffDmg`, `ExpectedDuration =
//!     ActualDuration`, `Time = end.Time - ActualDuration`.
//!   - **Dodge special-case** (`SkillID == skillData.DodgeID`, this
//!     project's supported arcdps era always uses the post-2022-03-07
//!     custom id `23275` -- verified empirically against this project's own
//!     committed fixture: `analysis::defenses`'s `dodge_count` calibrated
//!     exactly to 56 against golden, and this SAME id's rotation-group cast
//!     count on the real source EI JSON is ALSO exactly 56, see this
//!     module's golden test): forces `ExpectedDuration = 750` (start-only,
//!     missing-end case) or `ExpectedDuration = ActualDuration;
//!     scaled_ref = 0` (either case where an end row exists) -- GW2EI never
//!     estimates quickness for a dodge.
//!   - `SetAcceleration` (start+end present, OR start-missing-backdated;
//!     NEVER called for a start-only dangling cast, which is why that case
//!     stays "Unknown status" and is the only one eligible for the
//!     cross-skill `CutAt` trim above):
//!     ```text
//!     ratio = 1.0
//!     quickness = 0.0
//!     if scaled_ref > 0 {
//!         ratio = scaled_ref / actual_duration            // f64 division
//!         quickness = (ratio - 1.0) / 0.5   if ratio > 1.0   // faster
//!                   = -(1.0 - ratio) / 0.6  otherwise         // slower
//!         quickness = clamp(quickness, -1.0, 1.0)
//!     }
//!     time_gained = 0
//!     if skill_id != RESURRECT_SKILL_ID(1066) {              // per GW2EI:
//!         match end_activation {                             // never scored
//!             Cancel(4) => time_gained = -actual_duration,
//!             Reset(5)  => {}                                 // stays 0
//!             Minimum(3) | NoData(6) =>
//!                 time_gained = max(round(expected_duration / ratio)
//!                                    - actual_duration, 0),
//!             _ => {}
//!         }
//!     }
//!     quickness = round(quickness, 3 decimals)
//!     ```
//!     **Rounding note**: GW2EI's `Math.Round` (both the bare
//!     `Math.Round(double)` used for `scaledExpectedDuration` and the
//!     3-decimals `Math.Round(Acceleration, 3)`) default to
//!     `MidpointRounding.ToEven` ("banker's rounding"); this module uses
//!     `f64::round` (ties away from zero) instead, since Rust's stdlib has
//!     no built-in ties-to-even rounding without a manual implementation.
//!     This is a real, acknowledged divergence, but negligible in
//!     practice: it only differs from GW2EI on an EXACT `x.5` midpoint,
//!     which a ratio of two independently-measured millisecond durations
//!     essentially never lands on exactly (0 measured occurrences across
//!     this project's golden fixture).
//!
//! # Deliberately OUT OF SCOPE: GW2EI's `InstantCastEvent` machinery
//!
//! GW2EI's real `rotation[]` is NOT limited to the `AnimatedCastEvent`
//! pipeline above -- `SingleActor.InitCastEvents` (`GW2EIEvtcParser/
//! EIData/Actors/SingleActor.cs:599-619`) additionally merges in
//! `_instantCastData` (a large, per-skill-id-registered family of
//! "InstantCastFinder"s -- `GW2EIEvtcParser/EIData/InstantCastFinders/*`,
//! one per instant-cast mechanic: weapon swaps, procs, extension-sourced
//! instant heals/barriers, marker/effect/buff/damage-triggered instants,
//! minion-spawn casts, etc, each keyed to specific skill ids across every
//! profession) plus a SEPARATE `WeaponSwapEvent` merge keyed off the
//! dedicated `CBTS_WEAPSWAP`(11) statechange (pseudo skill id `-2`,
//! `SkillIDs.WeaponSwap`) -- NEITHER of which derives from
//! `is_activation`/`ANIMATION_START`/`ANIMATION_STOP` at all, and neither
//! of which this module implements. Verified directly against this
//! project's own source EI JSON (`axibridge/test-fixtures/boon/
//! 20260117-181030.json`, the real dps.report export this project's
//! committed `fixtures/wvw-small.ei.json` fixture is itself extracted
//! from): of 1,732 total rotation-group cast entries across all 41
//! players, 510 (~29%) are NOT part of the `AnimatedCastEvent` pipeline
//! this module reproduces -- identified PER CAST (not per skill id) as
//! `id >= 0 && duration > 1`, grounded directly in
//! `CombatEventFactory.CreateCastEvents`'s own `RemoveAll(x => x.Caster.
//! IsPlayer && x.ActualDuration <= 1)` filter plus `InstantCastEvent`'s
//! ctor (`ActualDuration` always hardcoded to `0`): a surviving
//! `duration > 1` entry can only be a real `AnimatedCastEvent`; a
//! `duration <= 1` entry can only be an `InstantCastEvent`/
//! `WeaponSwapEvent`. This is a PER-CAST signal, not a per-skill-id one --
//! confirmed on a real post-rework capture (`fixtures/local/
//! wvw-postrework.{zevtc,ei.json}`, gitignored): `Signet of Fury` (id
//! 14410) has two real, ~500ms-duration animated casts AND two duration-0
//! instant-proc entries on the SAME player, even though that capture's own
//! `skillMap["s14410"].isInstantCast` is `true` -- a coarser per-skill flag
//! that would wrongly exclude the two genuinely-animated entries too (an
//! earlier version of this module's calibration used that flag directly;
//! it happened to agree with the per-cast signal on the smaller committed
//! fixture -- 0 disagreements across 1,732 entries -- but diverges on this
//! richer real capture, which is why the per-cast discriminator is the one
//! actually used). This is a documented, honest scope limit (mirrors M14's
//! own skillMap-name-gap precedent for Task 2): `rotation_golden.rs`
//! calibrates ONLY the remaining ~71% "animated" subset, not the full EI
//! `rotation[]`.
//!
//! # Account folding
//!
//! GW2EI's own state machine runs per RAW caster addr (never crossing a
//! relog boundary), with the results merged into one list per resolved
//! account ONLY AFTER each raw addr's own pairing completes. This module
//! takes a documented simplification: it folds every cast event onto its
//! account representative (`addr_to_rep`, the SAME convention every other
//! per-player pass in this codebase already uses) BEFORE running the
//! pairing state machine, rather than running one state machine per raw
//! addr and concatenating afterward. This can only produce a different
//! result than GW2EI's own approach if a cast starts on one raw addr and
//! its paired end event arrives from a DIFFERENT raw addr of the same
//! account (i.e. spans a relog) -- not something a real skill-cast
//! animation can do (the character despawns on logout, ending any
//! in-progress cast), so the two approaches are equivalent for any real
//! log. This project's committed WvW fixture, `wvw-small.anon.zevtc`,
//! calibrates this module's counts/timings exactly under this
//! simplification (see `rotation_golden.rs`).

use crate::analysis::support::RESURRECT_SKILL_ID;
use crate::evtc::{sc, RawLog};
use std::collections::BTreeMap;

/// arcdps's own custom pseudo skill id for a dodge-roll animation, on any
/// build at/after `ArcDPSBuilds.InternalSkillIDsChange` (2022-03-07) --
/// verified against GW2EI's `SkillItem.GetArcDPSCustomIDs`/`SkillIDs.
/// ArcDPSDodge20220307` (`GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs:86`).
/// This project's supported arcdps era (every fixture/golden log is a 2026
/// build) is always past that 2022 threshold, so the legacy pre-2022-03-07
/// id (`65001`) is out of scope, same convention as every other
/// project-floor era assumption already made elsewhere in this codebase.
pub const DODGE_SKILL_ID: u32 = 23275;

const SERVER_DELAY_MS: i64 = 10;

/// One recorded cast of a skill -- mirrors GW2EI's `JsonRotation.JsonSkill`
/// field-for-field (`castTime`/`duration`/`timeGained`/`quickness`), see
/// this module's doc comment for the exact derivation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cast {
    /// Cast start time in ms, relative to this project's log-start zero
    /// point (`raw.events.first().time`, the same `t0` convention
    /// `timeseries`/`cc::timeline` use). May be NEGATIVE for a cast whose
    /// start predates the log (backdated from an end-only event, see the
    /// module doc's "end with NO pending START" case).
    pub cast_time_ms: i64,
    /// The animation's actual (wall-clock) duration in ms. `0` for an
    /// effectively-instant animation.
    pub duration_ms: i64,
    /// Time saved (positive) or lost (negative -- an interrupted cast) vs
    /// the skill's expected/tooltip duration. Mirrors GW2EI's
    /// `SavedDuration`/JSON `timeGained`.
    pub time_gained_ms: i64,
    /// `-1.0` (100% slow) to `1.0` (100% quickness); `0.0` when no
    /// scaled/unscaled duration pair was available to estimate from.
    /// Mirrors GW2EI's `Acceleration`/JSON `quickness`.
    pub quickness: f64,
}

/// All recorded casts of one skill id, for one player.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillRotation {
    pub skill_id: u32,
    /// In cast-start-time order.
    pub casts: Vec<Cast>,
}

/// One player's full rotation: every skill they cast, sorted by skill id
/// ascending (an axilog-native ordering choice -- GW2EI's own `rotation[]`
/// order is "first appearance in the time-sorted cast list", which this
/// project doesn't need to reproduce since calibration compares by skill id
/// key, not array position).
pub type RotationMetrics = Vec<SkillRotation>;

/// One classified cast-boundary row, already resolved to a project-relative
/// (`t0`-subtracted) time.
#[derive(Debug, Clone, Copy)]
enum ItemKind {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
struct Item {
    kind: ItemKind,
    time: i64,
    value: i64,
    buff_dmg: i64,
    activation: u8,
}

/// Working accumulator for one (possibly still start-only/"unknown") cast,
/// before the cross-skill `CutAt` sanitize pass runs.
#[derive(Debug, Clone, Copy)]
struct CastAcc {
    skill_id: u32,
    time: i64,
    actual_duration: i64,
    time_gained: i64,
    quickness: f64,
    /// True only for a start-only ("dangling") cast -- GW2EI's
    /// `AnimationStatus.Unknown`, i.e. `SetAcceleration` was never called
    /// for it. Only these are eligible for the later adjacent-pair `CutAt`
    /// trim.
    unknown: bool,
}

fn expected_duration_from(value: i64, buff_dmg: i64) -> i64 {
    if buff_dmg > 0 {
        buff_dmg
    } else {
        value
    }
}

/// Rounds to the nearest integer, ties away from zero (`f64::round`'s
/// native behavior) -- see the module doc's rounding note for the
/// documented, negligible divergence from GW2EI's own ties-to-even
/// `Math.Round`.
fn round_ties_away(x: f64) -> i64 {
    x.round() as i64
}

/// Rounds to 3 decimal places, ties away from zero (same caveat as
/// `round_ties_away`) -- mirrors GW2EI's `Math.Round(Acceleration,
/// ParserHelper.AccelerationDigit)` (`AccelerationDigit = 3`).
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// GW2EI's `AnimatedCastEvent.SetAcceleration`: derives `(quickness,
/// time_gained)` from the expected/actual/scaled-reference durations and
/// the END row's `is_activation` byte. See the module doc for the full
/// citation and formula.
fn set_acceleration(
    skill_id: u32,
    expected_duration: i64,
    actual_duration: i64,
    scaled_ref: i64,
    end_activation: u8,
) -> (f64, i64) {
    let mut ratio = 1.0_f64;
    let mut quickness = 0.0_f64;
    if scaled_ref > 0 {
        ratio = scaled_ref as f64 / actual_duration as f64;
        quickness = if ratio > 1.0 {
            (ratio - 1.0) / 0.5
        } else {
            -(1.0 - ratio) / 0.6
        };
        quickness = quickness.clamp(-1.0, 1.0);
    }
    let mut time_gained = 0_i64;
    if skill_id != RESURRECT_SKILL_ID {
        match end_activation {
            4 => time_gained = -actual_duration,   // Cancel -> Interrupted
            5 => {}                                 // Reset -> Full, stays 0
            3 | 6 => {
                // Minimum | NoData -> Reduced
                let scaled_expected = round_ties_away(expected_duration as f64 / ratio);
                time_gained = (scaled_expected - actual_duration).max(0);
            }
            _ => {}
        }
    }
    (round3(quickness), time_gained)
}

/// A pending START with no END yet (mid-loop new-START flush, or leftover
/// after the skill's event list is exhausted). Capped once against the
/// whole log's end (`log_end_rel`) immediately, mirroring
/// `AnimatedCastEvent`'s own constructor-time `CutAt(logData.EvtcLogEnd)`
/// call for this case -- see the module doc.
fn finalize_missing_end(skill_id: u32, start: &Item, log_end_rel: i64) -> CastAcc {
    let mut expected = expected_duration_from(start.value, start.buff_dmg);
    if skill_id == DODGE_SKILL_ID {
        expected = 750;
    }
    let mut actual = expected;
    let end = start.time + actual;
    if end > log_end_rel {
        actual = log_end_rel - start.time;
    }
    // Pre-era only: a START row itself carrying `ACTV_QUICKNESS_DEFUNC(2)`
    // is a legacy (pre-2019-11-07, per `JsonRotation.JsonSkill.Quickness`'s
    // own doc comment) 0/1 quickness hint, applied ONLY when no end row
    // ever arrives to compute a real ratio-based estimate. Post-era START
    // rows are the dedicated `ANIMATION_START` statechange and never take
    // this branch (see `AnimatedCastEvent`'s ctor: the `else` arm is
    // reached only when `startItem.IsStateChange != AnimationStart`).
    let quickness = if start.activation == 2 { 1.0 } else { 0.0 };
    CastAcc {
        skill_id,
        time: start.time,
        actual_duration: actual,
        time_gained: 0,
        quickness,
        unknown: true,
    }
}

/// An END row with no pending START -- backdated: the assumed start time is
/// `end.time - actual_duration`. Caller keeps this only if that backdated
/// time is `< 0` (before this project's log-start zero point), matching
/// GW2EI's own `toCheck.Time < logData.EvtcLogStart` filter.
fn finalize_missing_start(skill_id: u32, end: &Item) -> CastAcc {
    let actual = end.value;
    let mut scaled_ref = end.buff_dmg;
    // ExpectedDuration == ActualDuration in this branch, always (the dodge
    // special-case here only zeroes `scaled_ref`, per the module doc).
    let expected = actual;
    if skill_id == DODGE_SKILL_ID {
        scaled_ref = 0;
    }
    let time = end.time - actual;
    let (quickness, time_gained) = set_acceleration(skill_id, expected, actual, scaled_ref, end.activation);
    CastAcc {
        skill_id,
        time,
        actual_duration: actual,
        time_gained,
        quickness,
        unknown: false,
    }
}

/// Both a START and its paired END row are present.
fn finalize_both(skill_id: u32, start: &Item, end: &Item) -> CastAcc {
    let expected_from_start = expected_duration_from(start.value, start.buff_dmg);
    let mut actual = end.value;
    let mut scaled_ref = end.buff_dmg;
    let observed = end.time - start.time;
    if (actual - observed).abs() > SERVER_DELAY_MS {
        actual = observed;
        scaled_ref = 0;
    }
    let mut expected = expected_from_start;
    if skill_id == DODGE_SKILL_ID {
        expected = actual;
        scaled_ref = 0;
    }
    let (quickness, time_gained) = set_acceleration(skill_id, expected, actual, scaled_ref, end.activation);
    CastAcc {
        skill_id,
        time: start.time,
        actual_duration: actual,
        time_gained,
        quickness,
        unknown: false,
    }
}

/// The per-skill start/end pairing state machine (see module doc). Returns
/// casts in time order, already filtered for the `ActualDuration <= 1`
/// (player-only, always applicable here) drop.
fn process_skill(skill_id: u32, items: &[Item], log_end_rel: i64) -> Vec<CastAcc> {
    let mut out = Vec::new();
    let mut pending: Option<&Item> = None;
    for it in items {
        match it.kind {
            ItemKind::Start => {
                if let Some(prev) = pending.take() {
                    out.push(finalize_missing_end(skill_id, prev, log_end_rel));
                }
                pending = Some(it);
            }
            ItemKind::End => {
                if let Some(start) = pending.take() {
                    out.push(finalize_both(skill_id, start, it));
                } else {
                    let cand = finalize_missing_start(skill_id, it);
                    if cand.time < 0 {
                        out.push(cand);
                    }
                }
            }
        }
    }
    if let Some(start) = pending {
        out.push(finalize_missing_end(skill_id, start, log_end_rel));
    }
    out.retain(|c| c.actual_duration > 1);
    out
}

/// Computes [`RotationMetrics`] for every squad player addr present in
/// `addr_to_rep` (i.e. every squad player, account-folded onto its
/// representative addr -- see the module doc's "Account folding" section).
pub fn build(raw: &RawLog, addr_to_rep: &BTreeMap<u64, u64>) -> BTreeMap<u64, RotationMetrics> {
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0) as i64;
    let log_end_rel = raw.events.last().map(|e| e.time as i64 - t0).unwrap_or(0);
    let post_era = raw.header.is_post_buff_rework();

    // rep -> skill_id -> time-ordered start/end rows.
    let mut per_rep: BTreeMap<u64, BTreeMap<u32, Vec<Item>>> = BTreeMap::new();
    for e in &raw.events {
        let Some(&rep) = addr_to_rep.get(&e.src_agent) else { continue };
        let kind = if post_era {
            if e.is_statechange == sc::ANIMATION_START {
                Some(ItemKind::Start)
            } else if e.is_statechange == sc::ANIMATION_STOP {
                Some(ItemKind::End)
            } else {
                None
            }
        } else if e.is_statechange != 0 {
            None
        } else if e.is_activation == 1 || e.is_activation == 2 {
            Some(ItemKind::Start)
        } else if matches!(e.is_activation, 3..=6) {
            Some(ItemKind::End)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        per_rep.entry(rep).or_default().entry(e.skillid).or_default().push(Item {
            kind,
            time: e.time as i64 - t0,
            value: e.value as i64,
            buff_dmg: e.buff_dmg as i64,
            activation: e.is_activation,
        });
    }

    let mut result: BTreeMap<u64, RotationMetrics> = BTreeMap::new();
    for (rep, by_skill) in per_rep {
        let mut flat: Vec<CastAcc> = Vec::new();
        for (skill_id, items) in &by_skill {
            flat.extend(process_skill(*skill_id, items, log_end_rel));
        }
        flat.sort_by_key(|c| c.time);
        // Cross-skill CutAt sanitize pass: trim a still-"unknown" (start-only)
        // cast's duration if it would run past the NEXT cast (any skill id)
        // + the 10ms server-delay slack.
        for i in 0..flat.len().saturating_sub(1) {
            if !flat[i].unknown {
                continue;
            }
            let max_end = flat[i + 1].time + SERVER_DELAY_MS;
            let end = flat[i].time + flat[i].actual_duration;
            if end > max_end {
                flat[i].actual_duration = max_end - flat[i].time;
            }
        }
        let mut by_id: BTreeMap<u32, Vec<Cast>> = BTreeMap::new();
        for c in flat {
            by_id.entry(c.skill_id).or_default().push(Cast {
                cast_time_ms: c.time,
                duration_ms: c.actual_duration,
                time_gained_ms: c.time_gained,
                quickness: c.quickness,
            });
        }
        let rotation: RotationMetrics = by_id
            .into_iter()
            .map(|(skill_id, casts)| SkillRotation { skill_id, casts })
            .collect();
        result.insert(rep, rotation);
    }
    result
}

/// Total cast count across every skill, for one player's [`RotationMetrics`]
/// -- a convenience used by `--view rotation` (M14 Task 3) and this
/// module's own tests.
pub fn total_casts(rotation: &RotationMetrics) -> usize {
    rotation.iter().map(|s| s.casts.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(build: &str, events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: build.into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn addr_map(addrs: &[u64]) -> BTreeMap<u64, u64> {
        addrs.iter().map(|&a| (a, a)).collect()
    }

    // ---- pre-era synthetic sequences ----

    #[test]
    fn pre_era_simple_start_reset() {
        let events = vec![
            RawEvent { time: 100, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 1100, src_agent: 1, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let rot = &out[&1];
        assert_eq!(rot.len(), 1);
        assert_eq!(rot[0].skill_id, 500);
        assert_eq!(rot[0].casts.len(), 1);
        let c = rot[0].casts[0];
        // t0 == this event's own time (it's the first event in the log),
        // so cast_time_ms is 0, not the raw absolute 100.
        assert_eq!(c.cast_time_ms, 0);
        assert_eq!(c.duration_ms, 1000);
        assert_eq!(c.time_gained_ms, 0);
        assert_eq!(c.quickness, 0.0);
    }

    #[test]
    fn pre_era_cancel_fire_sets_negative_time_gained() {
        // Start expects 1000ms, cancelled (end.value) after 400ms.
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 400, src_agent: 1, skillid: 500, is_activation: 4, value: 400, buff_dmg: 400, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.duration_ms, 400);
        assert_eq!(c.time_gained_ms, -400);
    }

    #[test]
    fn pre_era_minimum_reduced_gains_time() {
        // Expected 1000ms (no acceleration: scaled_ref==buff_dmg==actual => ratio 1.0),
        // ends early (Minimum) at 700ms -> time_gained = max(1000-700,0) = 300.
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 700, src_agent: 1, skillid: 500, is_activation: 3, value: 700, buff_dmg: 700, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.duration_ms, 700);
        assert_eq!(c.time_gained_ms, 300);
    }

    #[test]
    fn pre_era_reset_full_no_time_gained() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 1000, src_agent: 1, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.time_gained_ms, 0);
    }

    #[test]
    fn quickness_faster_than_expected_is_positive() {
        // scaled_ref (buff_dmg, "unscaled") = 1000, actual = 500 -> ratio=2.0 -> quickness=(2-1)/0.5=2 clamped to 1.
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 500, src_agent: 1, skillid: 500, is_activation: 5, value: 500, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.quickness, 1.0);
    }

    #[test]
    fn quickness_slower_than_expected_is_negative() {
        // scaled_ref = 500, actual = 1000 -> ratio=0.5 -> quickness = -(1-0.5)/0.6 = -0.8333... clamp/round(3) = -0.833.
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 500, buff_dmg: 500, ..base_event() },
            RawEvent { time: 1000, src_agent: 1, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 500, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.quickness, -0.833);
    }

    #[test]
    fn pre_log_start_negative_cast_time_from_missing_start() {
        // Only an END row exists, near the very start of the recorded log
        // (t0 = 50), with a large duration so it backdates before t0.
        let events = vec![
            RawEvent { time: 50, src_agent: 1, skillid: 500, is_activation: 5, value: 2000, buff_dmg: 2000, ..base_event() },
            // A later, unrelated event keeps the log from being a single row
            // (not load-bearing, just realism).
            RawEvent { time: 3000, src_agent: 1, skillid: 999, is_activation: 1, value: 100, buff_dmg: 100, ..base_event() },
            RawEvent { time: 3100, src_agent: 1, skillid: 999, is_activation: 5, value: 100, buff_dmg: 100, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let rot = &out[&1];
        let skill500 = rot.iter().find(|s| s.skill_id == 500).unwrap();
        assert_eq!(skill500.casts.len(), 1);
        let c = skill500.casts[0];
        // t0 = 50 (first event), so cast_time = (50 - 2000) - 50 = -2000.
        assert_eq!(c.cast_time_ms, -2000);
        assert_eq!(c.duration_ms, 2000);
    }

    #[test]
    fn missing_start_end_not_before_log_start_is_dropped() {
        // Backdated time would be 50 - 100 = -50 relative to raw time, but
        // t0 == 50 too (first event), so relative backdated time is
        // (50-100)-50 = -100 -- still negative in THIS case; construct one
        // where it lands >= 0 instead: end row is NOT the first event, and
        // its own actual_duration is small enough the backdated time stays
        // at/after the log's own t0.
        let events = vec![
            RawEvent { time: 50, src_agent: 1, skillid: 999, is_activation: 1, value: 100, buff_dmg: 100, ..base_event() },
            RawEvent { time: 150, src_agent: 1, skillid: 999, is_activation: 5, value: 100, buff_dmg: 100, ..base_event() },
            // Dangling end for skill 500: backdated time = 200 - 50 = 150
            // (relative to t0=50: 200-50=150 absolute-rel, minus 50 duration
            // = 100) -- not negative, so dropped entirely.
            RawEvent { time: 200, src_agent: 1, skillid: 500, is_activation: 5, value: 50, buff_dmg: 50, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let rot = &out[&1];
        assert!(rot.iter().all(|s| s.skill_id != 500));
    }

    #[test]
    fn dangling_start_no_end_at_all_is_capped_to_log_end() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 5000, buff_dmg: 5000, ..base_event() },
            RawEvent { time: 300, src_agent: 1, skillid: 999, is_activation: 1, value: 10, buff_dmg: 10, ..base_event() },
            RawEvent { time: 320, src_agent: 1, skillid: 999, is_activation: 5, value: 20, buff_dmg: 20, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let rot = &out[&1];
        let skill500 = rot.iter().find(|s| s.skill_id == 500).unwrap();
        assert_eq!(skill500.casts.len(), 1);
        let c = skill500.casts[0];
        assert_eq!(c.cast_time_ms, 0);
        // Trimmed by the adjacent-pair CutAt: next cast (skill 999) starts
        // at 300, so max_end = 300 + 10(server delay) = 310.
        assert_eq!(c.duration_ms, 310);
    }

    #[test]
    fn near_instant_cast_is_dropped() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1, buff_dmg: 1, ..base_event() },
            RawEvent { time: 1, src_agent: 1, skillid: 500, is_activation: 5, value: 1, buff_dmg: 1, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        assert!(out.get(&1).map(|r| r.is_empty()).unwrap_or(true));
    }

    #[test]
    fn non_squad_source_is_ignored() {
        let events = vec![
            RawEvent { time: 0, src_agent: 42, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 1000, src_agent: 42, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1])); // 42 not in squad
        assert!(out.get(&1).map(|r| r.is_empty()).unwrap_or(true));
        assert!(!out.contains_key(&42));
    }

    #[test]
    fn resurrect_skill_never_scores_time_gained() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: RESURRECT_SKILL_ID, is_activation: 1, value: 800, buff_dmg: 800, ..base_event() },
            RawEvent { time: 300, src_agent: 1, skillid: RESURRECT_SKILL_ID, is_activation: 4, value: 300, buff_dmg: 300, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.time_gained_ms, 0); // would be -300 for any other skill id
    }

    // ---- post-era (ANIMATION_START/STOP statechanges) ----

    #[test]
    fn post_era_start_stop_matches_pre_era_math() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_statechange: sc::ANIMATION_START, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 700, src_agent: 1, skillid: 500, is_statechange: sc::ANIMATION_STOP, is_activation: 3, value: 700, buff_dmg: 700, ..base_event() },
        ];
        let raw = raw_from("20260501", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.cast_time_ms, 0);
        assert_eq!(c.duration_ms, 700);
        assert_eq!(c.time_gained_ms, 300);
    }

    #[test]
    fn post_era_ignores_pre_era_shaped_rows() {
        // is_statechange==0 with is_activation set should NOT be picked up
        // post-era (post-era cast rows are statechanges, not overloaded
        // combat rows).
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 1000, src_agent: 1, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260501", events);
        let out = build(&raw, &addr_map(&[1]));
        assert!(out.get(&1).map(|r| r.is_empty()).unwrap_or(true));
    }

    #[test]
    fn dodge_forces_no_quickness_estimate() {
        // Even with a scaled_ref that would otherwise imply quickness, the
        // dodge special-case zeroes it.
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: DODGE_SKILL_ID, is_activation: 1, value: 750, buff_dmg: 750, ..base_event() },
            RawEvent { time: 500, src_agent: 1, skillid: DODGE_SKILL_ID, is_activation: 5, value: 500, buff_dmg: 750, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let c = out[&1][0].casts[0];
        assert_eq!(c.quickness, 0.0);
        assert_eq!(c.duration_ms, 500);
    }

    #[test]
    fn multiple_casts_grouped_by_skill_id() {
        let events = vec![
            RawEvent { time: 0, src_agent: 1, skillid: 500, is_activation: 1, value: 100, buff_dmg: 100, ..base_event() },
            RawEvent { time: 100, src_agent: 1, skillid: 500, is_activation: 5, value: 100, buff_dmg: 100, ..base_event() },
            RawEvent { time: 200, src_agent: 1, skillid: 500, is_activation: 1, value: 100, buff_dmg: 100, ..base_event() },
            RawEvent { time: 300, src_agent: 1, skillid: 500, is_activation: 5, value: 100, buff_dmg: 100, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]));
        let rot = &out[&1];
        assert_eq!(rot.len(), 1);
        assert_eq!(rot[0].casts.len(), 2);
        assert_eq!(total_casts(rot), 2);
    }
}
