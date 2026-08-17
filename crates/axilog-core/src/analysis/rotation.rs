//! Per-player rotation (cast tracking) -- M14, Task 1.
//!
//! Reproduces GW2EI's `rotation[]` (the
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
//!     **Rounding note** (corrected by MSMALL item 3): GW2EI's
//!     `Math.Round` (both the bare `Math.Round(double)` used for
//!     `scaledExpectedDuration` and the 3-decimals
//!     `Math.Round(Acceleration, 3)`) default to `MidpointRounding.ToEven`
//!     ("banker's rounding").
//!
//!     M14 used `f64::round` (ties AWAY from zero) for both, on the grounds
//!     that Rust's stdlib has no built-in ties-to-even and that midpoints
//!     "essentially never" occur -- 0 measured occurrences on the committed
//!     golden fixture. That last part was true but was measured on the
//!     smaller fixture only. On the 10,878-cast local post-rework capture
//!     midpoints DO occur, and the divergence was observable downstream:
//!     `statsAll[0].timeSaved` came out 1ms high for 2 of 44 players.
//!
//!     `scaledExpectedDuration` therefore now uses a real ties-to-even
//!     [`round_ties_even`], which takes the per-cast `timeGained` delta
//!     against EI to EXACTLY 0 across all 10,878 casts (it was previously
//!     only within `rotation_golden`'s 1ms tolerance) and `timeSaved` to
//!     exact for all 44 players.
//!
//!     `quickness`'s 3-decimal [`round3`] is deliberately left on
//!     `f64::round`: it is measured at a 0.000000 max delta across the same
//!     10,878 casts, so no midpoint is reached there and changing it would
//!     be an unmeasured edit to an already-exact surface.
//!
//! # The instant-cast and weapon-swap merge
//!
//! GW2EI's real `rotation[]` is NOT limited to the `AnimatedCastEvent`
//! pipeline above. `SingleActor.InitCastEvents` (`GW2EIEvtcParser/EIData/
//! Actors/SingleActor.cs:599-619`) builds a player's cast list from THREE
//! sources, and [`build`] reproduces all three:
//!
//! ```text
//! CastEvents.AddRange(animationCastData);
//! CastEvents.AddRange(instantCastData);
//! foreach (WeaponSwapEvent wepSwap in log.CombatData.GetWeaponSwapData(AgentItem))
//! {
//!     if (CastEvents.Count > 0 && (wepSwap.Time - CastEvents.Last().Time) < ServerDelayConstant
//!         && CastEvents.Last().SkillID == WeaponSwap)
//!     {
//!         CastEvents[^1] = wepSwap;
//!     }
//!     else { CastEvents.Add(wepSwap); }
//! }
//! CastEvents.SortByTimeThenNegatedSwap();
//! ```
//!
//! 1. **Animated casts** -- the start/end pairing state machine above.
//! 2. **Instant casts** -- `analysis::instant_cast`'s port of the
//!    `InstantCastFinder` family (`GW2EIEvtcParser/EIData/
//!    InstantCastFinders/*`), passed in already computed so the one
//!    expensive finder pass is shared with `analysis::skill_map` rather
//!    than run twice.
//! 3. **Weapon swaps** -- the dedicated `CBTS_WEAPSWAP`(11) statechange,
//!    under GW2EI's pseudo skill id `-2` (`SkillIDs.WeaponSwap`, carried
//!    here as [`crate::analysis::skill_map::WEAPON_SWAP_SKILL_ID`], the
//!    same `-2i32 as u32` bit-cast every negative pseudo id in this
//!    codebase uses). Neither this nor (2) derives from
//!    `is_activation`/`ANIMATION_START`/`ANIMATION_STOP` at all.
//!
//! ## The swap dedup is transcribed literally, order and all
//!
//! The `CastEvents.Last()` the swap loop tests is the last element of the
//! still-UNSORTED `animated ++ instant` concatenation -- so on the first
//! iteration it is the latest instant cast (or the latest animated cast
//! when a player has no instants), and on every later iteration it is the
//! swap the previous iteration just appended. That second case is the one
//! that actually fires in practice: it collapses two `CBTS_WEAPSWAP` rows
//! landing within [`SERVER_DELAY_MS`] of each other down to the later one.
//! [`build`] therefore keeps the same three-part list identity rather than
//! pre-sorting, because a pre-sorted list would compare against a
//! different neighbour and drop a different swap.
//!
//! `SortByTimeThenNegatedSwap` (`CastEvent.cs:67`) is NOT reproduced: it
//! orders the flat list, and this module's output is grouped by skill id
//! with each group in time order (see [`RotationMetrics`]), a shape that
//! order cannot affect.
//!
//! Instant casts and swaps are merged AFTER the cross-skill `CutAt`
//! sanitize pass below, matching GW2EI, where that trim runs inside
//! `CombatEventFactory.CreateCastEvents` on animated data alone -- an
//! instant cast must not shorten a real animation it happens to fall
//! inside.
//!
//! ## Telling the two families apart in an EI export
//!
//! A cast's family is recoverable PER CAST (not per skill id) from
//! `duration` alone: `CombatEventFactory.CreateCastEvents` drops
//! `RemoveAll(x => x.Caster.IsPlayer && x.ActualDuration <= 1)`, while
//! `InstantCastEvent`'s and `WeaponSwapEvent`'s ctors both hardcode
//! `ActualDuration = 0`. So a surviving `duration > 1` entry can only be a
//! real `AnimatedCastEvent`, and a `duration <= 1` entry can only be an
//! instant or a swap. This is what the committed golden's own regeneration
//! join was re-verified against (`fixtures/wvw-small.ei.json`'s `_note`,
//! "M14 Task 1 ADDENDUM"), and it is a PER-CAST signal, not a per-skill-id
//! one -- confirmed on a real post-rework capture (`fixtures/local/
//! wvw-postrework.{zevtc,ei.json}`, gitignored): `Signet of Fury` (id
//! 14410) has two real, ~500ms-duration animated casts AND two duration-0
//! instant-proc entries on the SAME player, even though that capture's own
//! `skillMap["s14410"].isInstantCast` is `true` -- a coarser per-skill flag
//! that would wrongly exclude the two genuinely-animated entries too.
//!
//! ## Residual gap
//!
//! `WeaponSwapEvent.IgnoreOnRotationRender()` is `IsSpecialBundleSwap`,
//! set only by `LuminaryHelper.FlagLuminaryRadiantForgeWeaponSwapEvents`
//! for the Luminary elite spec. It does NOT drop the entry -- EI still
//! emits it and merely tags the JSON row `ignoreOnRotationRender: true`
//! (`JsonRotationBuilder.cs:22-25`) -- so no cast is missed by not
//! porting it; only that presentational flag is absent, and this project
//! does not model Luminary at all.
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
    /// How this cast's animation ended (MSMALL item 3). Mirrors GW2EI's
    /// `CastEvent.Status` (`AnimationStatus`), which is what
    /// `GameplayStatistics` counts `saved`/`wasted` off -- see
    /// [`AnimationStatus`] and [`aftercast_stats`].
    ///
    /// Deliberately NOT surfaced on `axilog_schema::CastOut`: the only
    /// consumer is the whole-player aggregate below, and adding a field to
    /// the emitted per-cast rows would change the `rotation` output for no
    /// consumer's benefit.
    pub status: AnimationStatus,
}

/// GW2EI's `CastEvent.AnimationStatus`
/// (`GW2EIEvtcParser/ParsedData/CombatEvents/CastEvents/CastEvent.cs:8`),
/// complete: the four values [`set_acceleration`] can produce, plus
/// [`AnimationStatus::Instant`] for the two synthesized zero-duration
/// families [`build`] merges in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationStatus {
    /// `SetAcceleration` never ran, or ran but matched no `case` -- a
    /// start-only ("dangling") cast, a RESURRECT, or an end row whose
    /// `is_activation` is outside {3,4,5,6}. GW2EI's default.
    #[default]
    Unknown,
    /// End `is_activation` was `Minimum(3)`/`NoData(6)`: the skill FIRED and
    /// its aftercast was skipped. This is the one `saved` counts.
    Reduced,
    /// End `is_activation` was `Cancel(4)`: the cast was aborted before
    /// firing. This is the one `wasted` counts.
    Interrupted,
    /// End `is_activation` was `Reset(5)`: the animation ran to completion.
    Full,
    /// Not an animation at all: a synthesized `InstantCastEvent` or a
    /// `WeaponSwapEvent`, both of whose ctors set `Status = Instant` and
    /// hardcode `ActualDuration = ExpectedDuration = 0`. Neutral to
    /// [`aftercast_stats`], which counts only `Reduced`/`Interrupted` --
    /// matching `GameplayStatistics`'s own switch, whose `Instant` casts
    /// fall through both cases with `SavedDuration == 0` regardless.
    Instant,
}

/// GW2EI's `GameplayStatistics` aftercast counters, for one player's whole
/// rotation (MSMALL item 3).
///
/// Transcribed from `GW2EIEvtcParser/EIData/Statistics/
/// GameplayStatistics.cs:81-99`, whose entire body for these four numbers
/// is one switch over each cast's `Status`:
///
/// ```text
/// case AnimationStatus.Interrupted:
///     SkillAnimationInterruptedCount++;
///     SkillAnimationInterruptedDuration += cl.SavedDuration;
///     break;
/// case AnimationStatus.Reduced:
///     SkillAnimationAfterCastInterruptedCount++;
///     SkillAnimationAfterCastInterruptedDuration += cl.SavedDuration;
///     break;
/// ```
///
/// followed by the two normalizations on lines 98-99 -- note the LEADING
/// MINUS on the interrupted one, which is what turns our already-negative
/// `time_gained_ms` for an interrupted cast back into a positive
/// "time wasted" figure:
///
/// ```text
/// SkillAnimationAfterCastInterruptedDuration =
///      Math.Round(SkillAnimationAfterCastInterruptedDuration / 1000.0, TimeDigit);
/// SkillAnimationInterruptedDuration =
///     -Math.Round(SkillAnimationInterruptedDuration / 1000.0, TimeDigit);
/// ```
///
/// `cl.SavedDuration` is exactly this module's [`Cast::time_gained_ms`]
/// (see its doc comment), so no new event scan is needed -- these four
/// numbers fall straight out of the cast list `build` already produces.
/// Durations stay in MILLISECONDS here; the `/1000.0` + 3-decimal rounding
/// is a serialization concern, applied by the ei-json adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AftercastStats {
    /// `SkillAnimationAfterCastInterruptedCount` -> JSON `saved`: the number
    /// of casts that skipped their aftercast.
    pub saved_count: u32,
    /// `SkillAnimationAfterCastInterruptedDuration` (pre-`/1000`) -> JSON
    /// `timeSaved`. Always >= 0 (`time_gained_ms` is `.max(0)`-clamped on
    /// the `Reduced` path).
    pub saved_ms: i64,
    /// `SkillAnimationInterruptedCount` -> JSON `wasted`: the number of
    /// casts interrupted before firing.
    pub wasted_count: u32,
    /// `SkillAnimationInterruptedDuration` (pre-`/1000`, and pre-NEGATION):
    /// `-1` times the sum of the (negative) `time_gained_ms` of interrupted
    /// casts, i.e. already the positive "time lost" figure -> JSON
    /// `timeWasted`.
    pub wasted_ms: i64,
}

/// Reduces one player's [`RotationMetrics`] to its [`AftercastStats`] -- see
/// that struct's doc comment for the GW2EI transcription.
///
/// # The `cast_time_ms >= 0` window filter
///
/// `GameplayStatistics` does not iterate the raw cast list: it iterates
/// `actor.GetCastEvents(log, start, end)`, which is
/// `CastEvents.Where(x => x.Time >= start && x.Time <= end)`
/// (`GW2EIEvtcParser/EIData/Actors/Actor.cs:407`), and for the whole-fight
/// phase `start` is `log.LogData.LogStart` -- this project's `t0` zero
/// point.
///
/// That excludes the BACKDATED pre-log casts this module deliberately keeps
/// (an end row with no pending start gets `Time = end.Time -
/// ActualDuration`, which can land before `t0`; see [`Cast::cast_time_ms`]).
/// Measured on `fixtures/local/wvw-postrework.zevtc` against that log's EI
/// export: without this filter `saved` was `ei + 1` for 11 of 44 players
/// (never +2, never -1) and `timeSaved` was correspondingly 1-4ms high for 6
/// of them; with it, all four counters below are EXACT for all 44.
///
/// The upper bound (`x.Time <= end`) is satisfied by construction -- every
/// cast start time here comes from an event inside the log -- so only the
/// lower bound needs applying.
pub fn aftercast_stats(rotation: &RotationMetrics) -> AftercastStats {
    let mut out = AftercastStats::default();
    for skill in rotation {
        for c in skill.casts.iter().filter(|c| c.cast_time_ms >= 0) {
            match c.status {
                AnimationStatus::Reduced => {
                    out.saved_count += 1;
                    out.saved_ms += c.time_gained_ms;
                }
                AnimationStatus::Interrupted => {
                    out.wasted_count += 1;
                    // `time_gained_ms` is `-actual_duration` here; GW2EI's
                    // line 99 negates the accumulated sum, so accumulate the
                    // already-negated value.
                    out.wasted_ms -= c.time_gained_ms;
                }
                AnimationStatus::Unknown | AnimationStatus::Full | AnimationStatus::Instant => {}
            }
        }
    }
    out
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

/// One entry of the instant-cast / weapon-swap half of `InitCastEvents`'s
/// merge (see the module doc). Both families are `ActualDuration = 0` by
/// construction, so a skill id and a time are the whole event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MergedCast {
    skill_id: u32,
    /// Project-relative (`t0`-based) ms, same clock as [`Cast::cast_time_ms`].
    time: i64,
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
    status: AnimationStatus,
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

/// Rounds to the nearest integer, ties to EVEN -- .NET's
/// `Math.Round(double)` (the single-argument overload GW2EI's
/// `SetAcceleration` uses for `(int)Math.Round(ExpectedDuration /
/// nonScaledToScaledRatio)`), whose documented default is
/// `MidpointRounding.ToEven`.
///
/// **MSMALL item 3 corrected this.** It was `f64::round` (ties AWAY from
/// zero) with a module-doc note calling the divergence negligible, on the
/// grounds that Rust's stdlib has no built-in ties-to-even. It is not
/// negligible and it is not hard to implement: measured on
/// `fixtures/local/wvw-postrework.zevtc`, ties-away put `timeSaved` 1ms
/// high for 2 of 44 players (18.701 vs EI's 18.700, 19.944 vs 19.943) --
/// i.e. exactly one cast per affected player landed on a `.5` boundary and
/// rounded the other way. With ties-to-even all 44 are exact.
fn round_ties_even(x: f64) -> i64 {
    let r = x.round(); // ties away from zero
    // Only a true .5 midpoint can differ between the two modes; there,
    // ties-to-even keeps the even neighbour.
    if (x - x.trunc()).abs() == 0.5 && (r as i64) % 2 != 0 {
        (r - x.signum()) as i64
    } else {
        r as i64
    }
}

/// Rounds to 3 decimal places, ties away from zero -- mirrors GW2EI's
/// `Math.Round(Acceleration, ParserHelper.AccelerationDigit)`
/// (`AccelerationDigit = 3`), which is `MidpointRounding.ToEven`.
///
/// Left on ties-away deliberately: unlike the integer
/// `scaledExpectedDuration` rounding (see [`round_ties_even`] and the
/// module doc's rounding note), the emitted `quickness` is measured at a
/// 0.000000 max delta against EI across 10,878 casts, so no `.0005`
/// midpoint is reached and switching it would be an unmeasured edit to an
/// already-exact surface.
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
) -> (f64, i64, AnimationStatus) {
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
    // GW2EI leaves `Status` at its `AnimationStatus.Unknown` default unless
    // this switch assigns it -- including for RESURRECT, where the whole
    // `if (SkillID != SkillIDs.Resurrect)` block is skipped, and for an end
    // `is_activation` outside {3,4,5,6}, where no `case` matches.
    let mut status = AnimationStatus::Unknown;
    if skill_id != RESURRECT_SKILL_ID {
        match end_activation {
            4 => {
                // Cancel -> Interrupted
                time_gained = -actual_duration;
                status = AnimationStatus::Interrupted;
            }
            5 => status = AnimationStatus::Full, // Reset -> Full, time_gained stays 0
            3 | 6 => {
                // Minimum | NoData -> Reduced
                let scaled_expected = round_ties_even(expected_duration as f64 / ratio);
                time_gained = (scaled_expected - actual_duration).max(0);
                status = AnimationStatus::Reduced;
            }
            _ => {}
        }
    }
    (round3(quickness), time_gained, status)
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
        // `SetAcceleration` is never called for a start-only cast, so
        // GW2EI's `Status` keeps its `Unknown` default -- the same
        // condition `unknown` below already records.
        status: AnimationStatus::Unknown,
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
    let (quickness, time_gained, status) =
        set_acceleration(skill_id, expected, actual, scaled_ref, end.activation);
    CastAcc {
        skill_id,
        time,
        actual_duration: actual,
        time_gained,
        quickness,
        status,
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
    let (quickness, time_gained, status) =
        set_acceleration(skill_id, expected, actual, scaled_ref, end.activation);
    CastAcc {
        skill_id,
        time: start.time,
        actual_duration: actual,
        time_gained,
        quickness,
        status,
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
///
/// `instants` is `analysis::instant_cast::compute`'s already-computed
/// output for this same log, merged in per the module doc's "instant-cast
/// and weapon-swap merge" section. It is a parameter rather than an
/// internal call so the one finder pass is shared with
/// `analysis::skill_map`, which needs the same events for its
/// `is_instant_cast` flag. Pass `&[]` to get the animated pipeline alone.
pub fn build(
    raw: &RawLog,
    addr_to_rep: &BTreeMap<u64, u64>,
    instants: &[crate::analysis::instant_cast::InstantCastEvent],
) -> BTreeMap<u64, RotationMetrics> {
    let t0 = raw.log_start_ms() as i64;
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

    // The other two cast families, bucketed per representative addr and
    // kept in the chronological order GW2EI's own `GetInstantCastData` /
    // `GetWeaponSwapData` hand back (see the module doc). `compute` returns
    // its events sorted by `(time, skill, caster)` and `raw.events` is
    // chronological, so both fall out already ordered.
    let mut instants_by_rep: BTreeMap<u64, Vec<MergedCast>> = BTreeMap::new();
    for ic in instants {
        let Some(&rep) = addr_to_rep.get(&ic.caster) else { continue };
        instants_by_rep
            .entry(rep)
            .or_default()
            .push(MergedCast { skill_id: ic.skill_id, time: ic.time as i64 - t0 });
    }
    let mut swaps_by_rep: BTreeMap<u64, Vec<MergedCast>> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != sc::WEAPON_SWAP {
            continue;
        }
        let Some(&rep) = addr_to_rep.get(&e.src_agent) else { continue };
        swaps_by_rep.entry(rep).or_default().push(MergedCast {
            skill_id: crate::analysis::skill_map::WEAPON_SWAP_SKILL_ID,
            time: e.time as i64 - t0,
        });
    }

    // Every rep that produced ANY of the three families -- a player who only
    // ever swapped weapons still gets a rotation.
    let reps: std::collections::BTreeSet<u64> = per_rep
        .keys()
        .chain(instants_by_rep.keys())
        .chain(swaps_by_rep.keys())
        .copied()
        .collect();

    let mut result: BTreeMap<u64, RotationMetrics> = BTreeMap::new();
    for rep in reps {
        let by_skill = per_rep.remove(&rep).unwrap_or_default();
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
                status: c.status,
            });
        }
        // -- `InitCastEvents`, transcribed: `animated ++ instant`, then the
        // swap loop's replace-or-append against that list's LAST element.
        // Only `(skill_id, time)` matters, since both merged families are
        // zero-duration by construction.
        //
        // The animated half of that concatenation never needs materializing
        // here: the replace arm additionally requires
        // `CastEvents.Last().SkillID == WeaponSwap`, and the animated
        // pipeline keys casts off `RawEvent::skillid`, which cannot carry
        // the `-2i32 as u32` sentinel. So whenever the trailing element is
        // an animated cast the arm is unreachable and GW2EI appends --
        // exactly what an empty `merged` does below.
        let mut merged: Vec<MergedCast> = instants_by_rep.remove(&rep).unwrap_or_default();
        for sw in swaps_by_rep.remove(&rep).unwrap_or_default() {
            match merged.last() {
                Some(l)
                    if sw.time - l.time < SERVER_DELAY_MS
                        && l.skill_id == crate::analysis::skill_map::WEAPON_SWAP_SKILL_ID =>
                {
                    // `CastEvents[^1] = wepSwap`.
                    *merged.last_mut().expect("matched on `Some`") = sw;
                }
                _ => merged.push(sw),
            }
        }
        for m in merged {
            by_id.entry(m.skill_id).or_default().push(Cast {
                cast_time_ms: m.time,
                duration_ms: 0,
                time_gained_ms: 0,
                quickness: 0.0,
                status: AnimationStatus::Instant,
            });
        }
        for casts in by_id.values_mut() {
            casts.sort_by_key(|c| c.cast_time_ms);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
        assert!(out.get(&1).map(|r| r.is_empty()).unwrap_or(true));
    }

    #[test]
    fn non_squad_source_is_ignored() {
        let events = vec![
            RawEvent { time: 0, src_agent: 42, skillid: 500, is_activation: 1, value: 1000, buff_dmg: 1000, ..base_event() },
            RawEvent { time: 1000, src_agent: 42, skillid: 500, is_activation: 5, value: 1000, buff_dmg: 1000, ..base_event() },
        ];
        let raw = raw_from("20260114", events);
        let out = build(&raw, &addr_map(&[1]), &[]); // 42 not in squad
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
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
        let out = build(&raw, &addr_map(&[1]), &[]);
        let rot = &out[&1];
        assert_eq!(rot.len(), 1);
        assert_eq!(rot[0].casts.len(), 2);
        assert_eq!(total_casts(rot), 2);
    }
}
