//! GW2EI-shape fixed-rate combat replay (M15 Task 1): per-actor
//! `{ start, end, positions, orientations, dc }` in MAP PIXELS on GW2EI's
//! own polling grid, byte-compatible with GW2EI's exported
//! `players[].combatReplayData`.
//!
//! Sibling of [`crate::analysis::replay`] (M9), which produces this
//! project's *native* replay shape: raw world-unit samples, a grid that
//! stops at the last real position event, and half-open down/dead
//! intervals. That native shape is deliberately narrower than GW2EI's (it
//! only ever emits samples backed by real bracketing position data). This
//! module instead reproduces GW2EI's exported shape exactly -- same grid
//! bounds, same hold/interpolate branch, same map-pixel rounding, same
//! `dc` sentinel bracketing -- so `axilog-ei`'s EI-JSON output (M15 Task 3)
//! can be a straight copy. Both live side by side; neither is computed
//! unless a caller asks for it.
//!
//! # GW2EI algorithm citations
//!
//! Everything below is transcribed from a local clone of
//! <https://github.com/baaron4/GW2-Elite-Insights-Parser> (`master`, commit
//! `7a6fe03`). File + method are cited at every step; the arcdps wire
//! formats are NOT re-derived here -- they are reused verbatim from M9
//! (`crate::analysis::replay::decode_position` / `crate::evtc::sc::POSITION`).
//!
//! ## 1. Which raw events feed the tracks
//!
//! `GW2EIEvtcParser/EIData/Actors/SingleActor.cs`, `InitCombatReplay`:
//! ```csharp
//! foreach (MovementEvent movementEvent in log.CombatData.GetMovementData(AgentItem))
//! {
//!     movementEvent.AddPoint3D(CombatReplay);
//! }
//! CombatReplay.PollingRate(log.LogData.LogDuration, AgentItem.Type == AgentItem.AgentType.Player);
//! ```
//! The three `MovementEvent` subclasses filter their own payloads before
//! appending (`ParsedData/CombatEvents/StatusEvents/MovementEvents/`):
//! - `PositionEvent.AddPoint3D`: drops the point when
//!   `point.XYZ == default` (all three components exactly zero),
//!   `point.IsNaNOrInfinity()`, or `point.XYZ.XY().LengthSquared() > 16e8`
//!   (i.e. `|xy| > 40000`, outside any real map).
//! - `RotationEvent.AddPoint3D` / `VelocityEvent.AddPoint3D`: drop only on
//!   NaN/infinity (a zero facing/velocity vector IS kept -- and a zero
//!   velocity is load-bearing, it is exactly what triggers the "hold"
//!   branch in `HandlePosition` below).
//!
//! Reproduced by [`collect_movement`].
//!
//! ## 2. The polling grid
//!
//! `GW2EIEvtcParser/EIData/CombatReplay/CombatReplay.cs`, `PollingRate`:
//! ```csharp
//! if (_Positions.Count == 0 && forcePolling) { _Positions = [new(int.MinValue, int.MinValue, 0, 0)]; }
//! else if (_Positions.Count == 0) { return; }
//! int rate = ParserHelper.CombatReplayPollingRate;              // 300
//! bool doRotation = _Rotations.Count > 0;
//! long posStartOffset = Math.Min(0, rate * ((_Positions[0].Time / rate) - 1));
//! long rotStartOffset = doRotation ? Math.Min(0, rate * ((_Rotations[0].Time / rate) - 1)) : 0;
//! long startOffset = Math.Min(posStartOffset, rotStartOffset);
//! for (long t = startOffset; t < logDuration; t += rate) { HandlePosition(...); HandleRotation(...); }
//! ```
//! `ParserHelper.CombatReplayPollingRate = 300`
//! (`GW2EIEvtcParser/ParserHelpers/ParserHelper.cs:15`) -- a compile-time
//! constant, NOT fight-length dependent; independently confirmed by the
//! reference export's `combatReplayMetaData.pollingRate = 300`.
//!
//! Because C# integer division truncates toward zero and every axilog time
//! is log-relative (`>= 0`), `startOffset` is `0` when the first sample is
//! at or after `rate`, and `-rate` when it is before -- either way a
//! multiple of `rate`. So **the grid is exactly the multiples of `rate`**,
//! run while `t < logDuration`. `logDuration` is
//! `log.LogData.LogDuration`, this project's [`Encounter::duration_ms`].
//!
//! Note that polling happens over the WHOLE log for every actor; per-actor
//! narrowing happens in `Trim` (step 3), not here. Consequently
//! `orientations` is always the same length as `positions` whenever the
//! actor has ANY facing event, and empty otherwise -- there is no
//! independent rotation window.
//!
//! Reproduced by [`poll`].
//!
//! ## 3. Per-actor trimming -> `start` / `end`
//!
//! `SingleActor.cs`, `TrimCombatReplay`:
//! ```csharp
//! var actives = GetActiveSegmentsForCRTrim(log);   // VIRTUAL -- see below
//! long trimStart = FirstAware; long trimEnd = LastAware;
//! ... // trimStart = actives[0].Start, trimEnd = actives[^1].End, when actives is non-empty
//! replay.Trim(Math.Max(trimStart, FirstAware), Math.Min(trimEnd, LastAware));
//! ```
//! `GetActiveSegmentsForCRTrim` is VIRTUAL. The base implementation
//! (`SingleActor.cs:319-323`) returns just `actives`, but `PlayerActor`
//! OVERRIDES it (`GW2EIEvtcParser/EIData/Actors/PlayerActor.cs:57-66`):
//! ```csharp
//! protected override IReadOnlyList<Segment> GetActiveSegmentsForCRTrim(ParsedEvtcLog log)
//! {
//!     var (deads, downs, _, actives) = GetStatus(log);
//!     List<Segment> segments = [.. deads, .. downs, .. actives];
//!     segments.Sort((x, y) => x.Start.CompareTo(y.Start));
//!     return segments;
//! }
//! ```
//! -- so for PLAYERS (which is every actor this module builds) the trim
//! window spans dead and down time too, NOT just the active segments.
//! This is load-bearing: a player downed at `D` and never revived before
//! last-aware has `actives = [[FirstAware, D]]` but
//! `downs = [[D, LastAware]]`, so the merged list's highest-`Start`
//! segment ends at `LastAware` and GW2EI keeps polling to the end of the
//! fight; trimming on `actives` alone would truncate that player's track
//! at `D`. Likewise a player dead at the end gets a `dead` tail reaching
//! `i64::MAX`, which the `Math.Min(trimEnd, LastAware)` at the call site
//! then clamps back to `LastAware`.
//!
//! (`List<T>.Sort` is an UNSTABLE introsort, but two of these segments can
//! never share a `Start`: `FillStatus` emits a contiguous, non-degenerate
//! partition of `[FirstAware, LastAware]` plus at most one
//! `[LastAware, i64::MAX]` tail, so the ordering is total. This module
//! uses a stable `sort_by_key` anyway, keeping GW2EI's own
//! deads-then-downs-then-actives concatenation order as the tiebreak.)
//!
//! and `CombatReplay.Trim`:
//! ```csharp
//! _start = Math.Max(start, _start);                    // _start starts at LogData.LogStart
//! _end   = Math.Max(_start, Math.Min(end, _end));      // _end   starts at LogData.LogEnd
//! _PolledPositions = _PolledPositions.Where(x => x.Time >= _start && x.Time <= _end)...
//! _PolledRotations = ... same predicate ...
//! ```
//! `SingleActorCombatReplayDescription`'s exported `Start`/`End` are
//! exactly this trimmed `replay.TimeOffsets`.
//!
//! In this project's log-relative frame `LogStart = 0` and
//! `LogEnd = duration_ms`, so this reduces to
//! `start = max(0, first_segment_start.max(first_aware))` and
//! `end = max(start, last_segment_end.min(last_aware).min(duration_ms))`,
//! with the segment bounds defaulting to first/last aware when the merged
//! list is empty. Verified against all 48
//! players of the reference export (`fixtures/local/wvw-postrework.ei.json`):
//! every `combatReplayData.start` equals that player's first-aware time and
//! every `.end` its last-aware time, and the resulting sample count
//! (`|multiples of 300 in [start, end] that are < 348362|`) reproduces
//! GW2EI's own `positions.length` for all 48 (1159 for the 45 players whose
//! first-aware is in `1..=300`, 1160 for the 3 whose first-aware is exactly
//! `0`, 1032 for the one who joined at `t=38317`).
//!
//! Reproduced by [`player_cr_trim_segments`] + [`trim_bounds`] +
//! [`grid_window`].
//!
//! ## 3b. First-aware / last-aware
//!
//! Everything in step 3 (and all of step 4) hangs off GW2EI's
//! `AgentItem.FirstAware`/`LastAware`, which `GW2EIEvtcParser/EvtcParser.cs`
//! widens from BOTH the source and the destination side of every combat
//! item (the agent pass at lines ~1149-1214):
//! ```csharp
//! foreach (CombatItem c in _combatItems)
//! {
//!     if (c.SrcIsAgent()) { ... UpdateAgentData(agent, c.Time, c.SrcInstid, ...) ... }
//!     if (c.DstIsAgent()) { ... UpdateAgentData(agent, c.Time, c.DstInstid, ...) ... }
//! }
//! // UpdateAgentData (EvtcParser.cs:980-1003):
//! ag.OverrideAwareTimes(Math.Min(ag.FirstAware, logTime), Math.Max(ag.LastAware, logTime));
//! ```
//! So a player who is DAMAGED or buffed before their own first src-side
//! event -- or after their last -- has a correspondingly wider awareness
//! window, and therefore a wider `start`/`end` and different `dc`
//! sentinels. Keying off `src_agent` alone (as
//! [`crate::analysis::replay`]'s M9 `first_aware` scan does) understates
//! it.
//!
//! `SrcIsAgent()` / `DstIsAgent()` (`CombatItem.cs:240-318`) are fixed
//! statechange whitelists, transcribed into [`src_is_agent`] /
//! [`dst_is_agent`]. Both include `StateChange.Combat` (id `0` -- an
//! ordinary non-statechange combat event, whose src/dst are the attacker
//! and the target), which is what makes the dst side matter at all. Note
//! this pass calls the PLAIN overloads, not the `SrcIsAgent(extensions)`
//! ones, so arcdps extension events (`CBTS_EXTENSION` 40 /
//! `CBTS_EXTENSIONCOMBAT` 49 -- including the healing extension this crate
//! also parses) do NOT widen awareness here.
//!
//! Reproduced by [`collect_movement`].
//!
//! ## 4. `dc` (and `down` / `dead`) intervals
//!
//! `GW2EIEvtcParser/EIData/Actors/ActorsHelper/SingleActorStatusHelper.cs`,
//! `FillStatus` / `AddValueToStatusList`:
//! ```csharp
//! AddSegment(dc, long.MinValue, FirstAware);                        // always, first
//! if (status.Count == 0) { AddSegment(actives, FirstAware, LastAware);
//!                          AddSegment(dc, LastAware, long.MaxValue); return; }
//! status = status.OrderBy(x => x.Time).ToList();
//! for (int i = 0; i < status.Count - 1; i++) AddValueToStatusList(..., status[i], status[i+1], FirstAware, i);
//! AddValueToStatusList(..., status[^1], (LastAware, null), FirstAware, status.Count - 1);
//! if (status[^1].evt is DeadEvent) AddSegment(dead, LastAware, long.MaxValue);
//! else                             AddSegment(dc,   LastAware, long.MaxValue);
//! ```
//! with `AddSegment` a no-op unless `start < end`, and:
//! ```csharp
//! if (curEvt is DownEvent)    { if (index == 0) AddSegment(actives, minTime, cTime); AddSegment(down, cTime, next.Time); }
//! else if (curEvt is DeadEvent)   { if (index == 0) AddSegment(actives, minTime, cTime); AddSegment(dead, cTime, next.Time); }
//! else if (curEvt is DespawnEvent){ if (index == 0) AddSegment(actives, minTime, cTime); AddSegment(dc,   cTime, next.Time); }
//! else { if (index == 0 && cTime - minTime > 50) AddSegment(dc, minTime, cTime); AddSegment(actives, cTime, next.Time); }
//! ```
//! So the two `i64::MIN`/`i64::MAX` sentinels observed on the reference
//! export are the *always-present* `[MIN, FirstAware]` head and the
//! `[LastAware, MAX]` tail; genuine mid-fight disconnects are
//! `CBTS_DESPAWN` -> `[despawn_t, next_status_t]` segments in between, and
//! a late spawn more than 50ms after first-aware adds a
//! `[FirstAware, spawn_t]` segment. A player still dead at the end of the
//! log gets `[LastAware, MAX]` on `dead` instead of on `dc`.
//!
//! The status-event stream is `DownEvent` (`CBTS_CHANGEDOWN`, 5),
//! `AliveEvent` (`CBTS_CHANGEUP`, 3), `DeadEvent` (`CBTS_CHANGEDEAD`, 4),
//! `SpawnEvent` (`CBTS_SPAWN`, 6) and `DespawnEvent` (`CBTS_DESPAWN`, 7)
//! -- ids per `GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:258-267`
//! (`ChangeUp = 3, ChangeDead = 4, ChangeDown = 5, Spawn = 6, Despawn = 7`),
//! matching this crate's existing [`sc::CHANGE_UP`]/[`sc::CHANGE_DEAD`]/
//! [`sc::CHANGE_DOWN`] plus the two added for this module. `GetAgentStatus`
//! concatenates them in the order downs, alives, deads, spawns, despawns
//! and then `OrderBy(x => x.Time)` -- LINQ's `OrderBy` is a documented
//! STABLE sort, so that concatenation order is the same-timestamp tiebreak,
//! reproduced here by [`StatusKind`]'s discriminant ordering.
//!
//! Reproduced by [`fill_status`].
//!
//! ## 5. World -> map pixel
//!
//! `GW2EIEvtcParser/EIData/CombatReplay/CombatReplayMap.cs`,
//! `GetMapCoordRounded` + `GetPixelMapSize`:
//! ```csharp
//! var (width, height) = GetPixelMapSize();            // image size rescaled so max dim == 750
//! double scaleX = (double)width / _pixelSize.width;
//! double scaleY = (double)height / _pixelSize.height;
//! double x = (realX - _rectInMap.topX) / (_rectInMap.bottomX - _rectInMap.topX);
//! double y = (realY - _rectInMap.topY) / (_rectInMap.bottomY - _rectInMap.topY);
//! return new((float)Math.Round(scaleX * _pixelSize.width * x, 3),
//!            (float)Math.Round(scaleY * (_pixelSize.height - _pixelSize.height * y), 3));
//! ```
//! `ParserHelper.CombatReplayDataDigit = 3`
//! (`ParserHelpers/ParserHelper.cs:27`) -- confirming the reference
//! export's 3-decimal positions is ROUNDING, not truncation, and
//! specifically C#'s `Math.Round(double, int)`, which is round-half-to-EVEN
//! (`MidpointRounding.ToEven` is the documented default overload). Note the
//! `(float)` casts: GW2EI's exported positions/angles are single-precision,
//! so this module rounds in `f64`, narrows to `f32`, then widens back --
//! producing the exact `f32` GW2EI serializes. Reproduced by
//! [`round_ei`] / [`MapTransform::to_map_pixel`].
//!
//! The per-map `(_pixelSize, _rectInMap)` constants come from
//! `GW2EIEvtcParser/LogLogic/WvW/WvWLogic.cs`, `GetCombatMapInternal`, and
//! are transcribed into [`MapTransform::for_map_id`].
//!
//! ## 6. Orientations
//!
//! `SingleActorCombatReplayDescription`'s constructor:
//! ```csharp
//! foreach (var facing in replay.PolledRotations) { angles.Add(-facing.XYZ.GetRoundedZRotationDeg()); }
//! ```
//! and `EIData/MathUtils/VectorExtensions.cs:147` +
//! `EIData/MathUtils/Trigonometry.cs:5`:
//! ```csharp
//! public static float GetRoundedZRotationDeg(this in Vector3 facing)
//!     => (float)Math.Round(RadianToDegree(GetZRotationRadians(facing)), CombatReplayDataDigit);
//! public static float GetZRotationRadians(this in Vector3 facing) => (float)Math.Atan2(facing.Y, facing.X);
//! internal static double RadianToDegree(double radian) => radian * 180.0 / Math.PI;
//! ```
//! So the convention is `-round(degrees(atan2(y, x)), 3)` -- note the
//! leading MINUS (screen-space y grows downward, world-space y grows
//! upward), the `(float)` narrowing of the radian value BEFORE the degree
//! conversion, and that the negation happens AFTER rounding. Range is
//! `[-180, 180]`. Reproduced by [`facing_to_degrees`].
//!
//! # Calibration
//!
//! `crates/axilog-core/tests/ei_replay_golden.rs`, against the real
//! post-rework capture + its GW2EI export
//! (`fixtures/local/wvw-postrework.{zevtc,ei.json}`, Blue Alpine
//! Borderlands, 348362ms, 2026-08-09):
//!
//! - 44 squad players joined by account; `start`, `end`, `dc`, `down` and
//!   `dead` **exact for every one** (the brief's exactness gate).
//! - positions: **50999/50999 (100.00%)** within 1 map pixel, and in fact
//!   **50999/50999 (100.00%) bit-exact** once both sides are narrowed to
//!   the `f32` GW2EI serializes -- worst residual `3.2e-5 px`, which is
//!   purely `serde_json` widening GW2EI's decimal text back to `f64`. (M9's
//!   native `replay` module manages 99.77% within 1px on the same data; the
//!   difference is entirely the two `HandlePosition` subtleties documented
//!   on [`handle_position`] -- interpolating from the previous POLLED point
//!   and the velocity-gated hold branch.)
//! - orientations: **50999/50999 (100.00%)** bit-exact, worst residual
//!   `7.6e-6 deg` (same `f32` widening artefact). The achievable floor is
//!   set by GW2EI's own 3-decimal rounding; the committed gate is the
//!   looser "99% within 1 deg" because `atan2` is not bit-reproducible
//!   across platform libms.
//!
//! Both eras additionally get a structural sweep (monotone grid at the
//! declared rate, sample counts on the grid, no NaN/infinite coordinates,
//! angles in `[-180, 180]`, `dc` sentinel bracketing): the committed
//! pre-rework anonymized fixture (`fixtures/wvw-small.anon.zevtc`, Green
//! Alpine Borderlands, 49285ms) yields 74 plausible moving tracks, all
//! carrying orientations.
//!
//! See this module's `tests` for the algorithm-level unit tests
//! (interpolation, edge clamping, the hold branch, the empty track, `dc`
//! bracketing).

use crate::analysis::replay::DEFAULT_POLL_MS;
use crate::evtc::{sc, RawEvent, RawLog};
use crate::model::Encounter;
use std::collections::BTreeSet;

/// GW2EI's `ParserHelper.CombatReplayPollingRate` (`ParserHelper.cs:15`),
/// re-exported here as an `i64` for this module's signed-time arithmetic.
/// Same value as [`DEFAULT_POLL_MS`]; kept as a separate constant so a
/// caller overriding the native replay's `poll_ms` cannot silently change
/// the EI-shape output's grid.
pub const EI_POLL_MS: i64 = DEFAULT_POLL_MS as i64;

/// GW2EI's `ArcDPSEnums.ArcDPSPollingRate` (`ArcDPSEnums.cs:6`): arcdps's
/// own movement-telemetry emission period. Used ONLY in `HandlePosition`/
/// `HandleRotation`'s "gap too big to interpolate across" test
/// (`nextPos.Time - last.Time > ArcDPSPollingRate + rate`). Coincidentally
/// equal to [`EI_POLL_MS`], but a semantically distinct constant.
pub const ARCDPS_POLL_MS: i64 = 300;

/// GW2EI's `ParserHelper.CombatReplayDataDigit` (`ParserHelper.cs:27`):
/// decimal places every exported combat-replay coordinate/angle is rounded
/// to.
pub const DATA_DIGITS: i32 = 3;

/// `10^DATA_DIGITS`, precomputed (matching .NET's own
/// `roundPower10Double[digits]` table lookup inside `Math.Round(double,
/// int)`).
const DATA_SCALE: f64 = 1000.0;

/// GW2EI's `CombatReplayMap.GetPixelMapSize`'s target max dimension.
const PIXEL_MAP_MAX_DIM: f64 = 750.0;

// -- statechange ids GW2EI's status stream needs that this crate's `sc`
//    module doesn't already name (M15 Task 1). Values per
//    `GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:258-267`
//    (`StateChange.Spawn = 6`, `StateChange.Despawn = 7`), which the M9
//    module doc already established as a faithful transcription of the
//    arcdps `cbtstatechange` enum ordering (`CBTS_SPAWN` is the 7th entry,
//    `CBTS_DESPAWN` the 8th, counting from `CBTS_NONE = 0`).

/// `CBTS_SPAWN` -- agent (re)entered tracking range.
const SC_SPAWN: u8 = 6;
/// `CBTS_DESPAWN` -- agent left tracking range. GW2EI's ONLY source of
/// mid-fight `dc` segments.
const SC_DESPAWN: u8 = 7;

/// GW2EI's `CombatItem.SrcIsAgent()` (`CombatItem.cs:240-291`): the
/// statechanges whose `src_agent` field really is an agent address (as
/// opposed to a map id, a server timestamp, a skill id, ...). Transcribed
/// id-by-id from that method against `ArcDPSEnums.StateChange`
/// (`ArcDPSEnums.cs:258-345`); `IsGeographical` (`CombatItem.cs:44-46`)
/// expands to `Position | Velocity | Rotation` = 19, 20, 21.
///
/// `StateChange.Combat = 0` is an ORDINARY combat event (damage,
/// activation, ...), not a statechange -- it is in both this list and
/// [`dst_is_agent`]'s.
///
/// Used only for the first/last-aware scan (see this module's doc comment,
/// section 3b). Deliberately NOT the `SrcIsAgent(extensions)` overload:
/// GW2EI's own awareness pass calls the plain one, so extension events
/// (`CBTS_EXTENSION` 40 / `CBTS_EXTENSIONCOMBAT` 49) do not widen
/// awareness.
fn src_is_agent(statechange: u8) -> bool {
    matches!(
        statechange,
        0     // Combat (an ordinary, non-statechange event)
        | 1   // EnterCombat
        | 2   // ExitCombat
        | 3   // ChangeUp
        | 4   // ChangeDead
        | 5   // ChangeDown
        | 6   // Spawn
        | 7   // Despawn
        | 8   // HealthUpdate
        | 11  // WeaponSwap
        | 12  // MaxHealthUpdate
        | 13  // PointOfView
        | 18  // BuffInitial
        | 19  // Position      \
        | 20  // Velocity      | IsGeographical
        | 21  // Rotation      /
        | 22  // TeamChange
        | 23  // AttackTarget
        | 24  // Targetable
        | 27  // StackActive
        | 28  // StackDeactive
        | 29  // Guild
        | 34  // BreakbarState
        | 35  // BreakbarPercent
        | 37  // Marker
        | 38  // BarrierUpdate
        | 44  // Last90BeforeDown
        | 45  // Effect_45
        | 51  // Effect_51
        | 55  // Glider
        | 56  // StunBreak
        | 57  // MissileCreate
        | 58  // MissileLaunch
        | 59  // MissileRemove
        | 60  // EffectGroundCreate
        | 62  // EffectAgentCreate
        | 67  // AnimationStart
        | 68  // AnimationStop
        | 69  // BuffApply
        | 71  // BuffRemoveSingle
        | 72  // BuffRemoveAll
        | 73  // Transformation
        | 76  // StealthChange
        | 77  // GadgetAnimation
        | 78  // GadgetNameVisible
        | 79  // EffectMissileCreate
        | 80  // GadgetCaptureOutlineShow
        | 81  // GadgetCaptureSplitPercent
        | 82  // GadgetCaptureOutlineHide
        | 83  // GadgetCaptureOutlinePoint
    )
}

/// GW2EI's `CombatItem.DstIsAgent()` (`CombatItem.cs:302-318`): the
/// statechanges whose `dst_agent` field really is an agent address. Much
/// shorter than [`src_is_agent`]'s list -- for most statechanges
/// `dst_agent` is a packed payload (movement coordinates, buff durations,
/// ...) rather than a second agent.
fn dst_is_agent(statechange: u8) -> bool {
    matches!(
        statechange,
        0     // Combat (an ordinary, non-statechange event -- dst is the target)
        | 18  // BuffInitial
        | 23  // AttackTarget
        | 45  // Effect_45
        | 47  // LogNPCUpdate
        | 51  // Effect_51
        | 58  // MissileLaunch
        | 62  // EffectAgentCreate
        | 67  // AnimationStart
        | 69  // BuffApply
        | 70  // BuffChange
        | 71  // BuffRemoveSingle
        | 72  // BuffRemoveAll
    )
}

/// One decoded movement point: log-relative time plus a 3-vector. Mirrors
/// GW2EI's `ParametricPoint3D`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3 {
    pub t: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3 {
    fn with_time(self, t: i64) -> Self {
        Self { t, ..self }
    }

    /// `System.Numerics.Vector3.Lerp(a, b, r)`, which .NET implements as
    /// `a * (1 - r) + b * r` (NOT `a + (b - a) * r`) -- the two differ in
    /// the last ULP, and this module reproduces GW2EI bit-for-bit where it
    /// cheaply can.
    fn lerp(a: Self, b: Self, r: f32, t: i64) -> Self {
        let inv = 1.0 - r;
        Self {
            t,
            x: a.x * inv + b.x * r,
            y: a.y * inv + b.y * r,
            z: a.z * inv + b.z * r,
        }
    }

    fn len(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

/// GW2EI's `CombatReplayMap`, reduced to the parts the world -> map-pixel
/// transform needs (the decoration/viewpoint machinery is out of scope).
///
/// `img_w`/`img_h` are the map IMAGE's native pixel size (GW2EI's
/// `_pixelSize`); `out_w`/`out_h` are `GetPixelMapSize()`'s rescale of that
/// to a 750px max dimension (GW2EI's exported
/// `combatReplayMetaData.sizes`). Both are retained because
/// `GetMapCoordRounded` multiplies by `out/img * img` rather than by `out`
/// directly, and that round-trip is not exactly the identity in binary
/// floating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapTransform {
    /// GW2EI's `_rectInMap.topX` -- world (inch) x of the image's left edge.
    pub top_x: f64,
    /// GW2EI's `_rectInMap.topY` -- world (inch) y of the image's BOTTOM
    /// edge (the y axis is flipped on output; see [`Self::to_map_pixel`]).
    pub top_y: f64,
    pub bottom_x: f64,
    pub bottom_y: f64,
    /// GW2EI's `_pixelSize.width`.
    pub img_w: f64,
    /// GW2EI's `_pixelSize.height`.
    pub img_h: f64,
    /// `GetPixelMapSize().width`.
    pub out_w: f64,
    /// `GetPixelMapSize().height`.
    pub out_h: f64,
}

impl MapTransform {
    /// Build from GW2EI's own `new CombatReplayMap((imgW, imgH), (topX,
    /// topY, bottomX, bottomY))` arguments, applying `GetPixelMapSize`:
    /// ```csharp
    /// double ratio = (double)_pixelSize.width / _pixelSize.height;
    /// const int pixelSize = 750;
    /// if (ratio > 1.0)      return (pixelSize, (int)Math.Round(pixelSize / ratio));
    /// else if (ratio < 1.0) return ((int)Math.Round(ratio * pixelSize), pixelSize);
    /// else                  return (pixelSize, pixelSize);
    /// ```
    /// (`Math.Round(double)` is round-half-to-even, matching
    /// [`round_ties_even`].)
    pub fn new(img: (u32, u32), rect: (f64, f64, f64, f64)) -> Self {
        let img_w = f64::from(img.0);
        let img_h = f64::from(img.1);
        let ratio = img_w / img_h;
        let (out_w, out_h) = if ratio > 1.0 {
            (PIXEL_MAP_MAX_DIM, round_ties_even(PIXEL_MAP_MAX_DIM / ratio))
        } else if ratio < 1.0 {
            (round_ties_even(ratio * PIXEL_MAP_MAX_DIM), PIXEL_MAP_MAX_DIM)
        } else {
            (PIXEL_MAP_MAX_DIM, PIXEL_MAP_MAX_DIM)
        };
        let (top_x, top_y, bottom_x, bottom_y) = rect;
        Self { top_x, top_y, bottom_x, bottom_y, img_w, img_h, out_w, out_h }
    }

    /// GW2EI's `WvWLogic.GetCombatMapInternal`
    /// (`GW2EIEvtcParser/LogLogic/WvW/WvWLogic.cs:102-148`) per-map
    /// `(_pixelSize, _rectInMap)` constants.
    ///
    /// M15 Task 2: the constants themselves now live in
    /// [`crate::wvw::maps::WVW_MAPS`] -- axilog's single source of truth
    /// for WvW map geometry, shared with `wvw::map_name`, the arena image
    /// URLs [`combat_replay_meta`] exports, and the native replay's golden
    /// test. That module's doc comment carries the full `switch`
    /// transcription and per-value citations.
    ///
    /// Returns `None` for GW2EI's `default` branch (any map id not in the
    /// table): GW2EI then derives the rect from the union of every squad
    /// player's polled positions (`CombatReplayMap.ComputeBoundingBox` +
    /// `FixAspectRatio`), which [`bounding_box_transform`] implements
    /// separately because it needs the polled tracks that this transform is
    /// an input to.
    pub fn for_map_id(map_id: u32) -> Option<Self> {
        let def = crate::wvw::maps::map_def(map_id)?;
        Some(Self::new(def.pixel_size, def.rect))
    }

    /// The `CBTS_MAPID` (`sc::MAP_ID`, 25) statechange's `src_agent` is the
    /// WvW map id -- same lookup `crate::wvw::apply` already does for
    /// `Encounter::map`. `None` when the log has no map-id event or the id
    /// isn't one GW2EI has an image for.
    pub fn from_log(raw: &RawLog) -> Option<Self> {
        Self::for_map_id(map_id_from_log(raw)?)
    }

    /// GW2EI's exported `combatReplayMetaData.sizes`.
    pub fn sizes(&self) -> [i64; 2] {
        [self.out_w as i64, self.out_h as i64]
    }

    /// GW2EI's `CombatReplayMap.GetInchToPixel`:
    /// ```csharp
    /// float ratio = (float)(_rectInMap.bottomX - _rectInMap.topX) / GetPixelMapSize().width;
    /// return (float)Math.Round(1.0f / ratio, 3);
    /// ```
    /// (All-`f32` arithmetic in GW2EI, mirrored here.) For Alpine
    /// Borderlands this yields `round(1 / (61440 / 523), 3) = 0.009`, which
    /// is exactly the reference export's `inchToPixel`.
    pub fn inch_to_pixel(&self) -> f64 {
        let ratio = ((self.bottom_x - self.top_x) as f32) / (self.out_w as f32);
        f64::from(round_ei(f64::from(1.0f32 / ratio)))
    }

    /// GW2EI's `CombatReplayMap.GetMapCoordRounded(float realX, float
    /// realY)` -- see this module's doc comment (section 5) for the
    /// transcription. Note the y flip (`out_h * (1 - y_frac)`): world y
    /// grows northward, image y grows downward.
    pub fn to_map_pixel(&self, x: f32, y: f32) -> [f64; 2] {
        let scale_x = self.out_w / self.img_w;
        let scale_y = self.out_h / self.img_h;
        let fx = (f64::from(x) - self.top_x) / (self.bottom_x - self.top_x);
        let fy = (f64::from(y) - self.top_y) / (self.bottom_y - self.top_y);
        [
            f64::from(round_ei(scale_x * self.img_w * fx)),
            f64::from(round_ei(scale_y * (self.img_h - self.img_h * fy))),
        ]
    }
}

/// One entry of GW2EI's exported `combatReplayMetaData.maps` --
/// `JsonCombatReplayMetaData.CombatReplayMap`
/// (`GW2EIJSON/JsonCombatReplayMetaData.cs:10-28`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombatReplayMapImage {
    /// The arena image URL (GW2EI's `ArenaDecorationMetadataDescription.
    /// Image`, i.e. the `LogImages` constant the map's `ArenaDecoration`
    /// was constructed with).
    pub url: &'static str,
    /// `[start, end]` in log-relative ms: the window this image is the
    /// active backdrop for. GW2EI's
    /// `[mapItem.Start, mapItem.End]`, which is the decoration's lifespan.
    pub interval: [i64; 2],
    /// `[x, y]`, in MAP pixels, of the image's top-left corner.
    pub position: [f64; 2],
}

/// GW2EI's exported `combatReplayMetaData` object
/// (`GW2EIJSON/JsonCombatReplayMetaData.cs`).
#[derive(Debug, Clone, PartialEq)]
pub struct CombatReplayMeta {
    /// `CombatReplayMap.GetInchToPixel()` -- see
    /// [`MapTransform::inch_to_pixel`].
    ///
    /// `f32`, not `f64`, deliberately: GW2EI's `JsonCombatReplayMetaData.
    /// InchToPixel` is a C# `float`, so the reference export's text is
    /// `0.009` -- the shortest decimal that round-trips through single
    /// precision. Widened to `f64` the same value prints as
    /// `0.008999999612569809`, which would not be byte-identical to EI's
    /// output. Keeping the narrow type here means the JSON writer gets it
    /// right by construction.
    pub inch_to_pixel: f32,
    /// `ParserHelper.CombatReplayPollingRate` -- always [`EI_POLL_MS`].
    pub polling_rate: i64,
    /// `[width, height]` of the replay canvas in map pixels --
    /// `GetPixelMapSize()`, see [`MapTransform::sizes`].
    pub sizes: [i64; 2],
    /// The arena backdrop image(s). A `Vec` rather than a single image
    /// because GW2EI's shape allows time-sliced backdrops (Adina's arena
    /// swaps image per phase --
    /// `LogLogic/Raids/RaidWings/W7/Bosses/Adina.cs:575-600` pushes one
    /// `ArenaDecoration` per phase); WvW always yields exactly one, so this
    /// is a one-element `Vec` for every id in [`crate::wvw::maps::WVW_MAPS`].
    pub maps: Vec<CombatReplayMapImage>,
}

/// GW2EI's `JsonCombatReplayMetaDataBuilder.BuildJsonCombatReplayMetaData`
/// (`GW2EIBuilders/JsonModels/JsonCombatReplayMetaDataBuilder.cs`),
/// specialized to WvW:
///
/// ```csharp
/// CombatReplayMap map = log.LogData.Logic.GetCombatReplayMap(log);
/// var jsonCR = new JsonCombatReplayMetaData()
/// {
///     InchToPixel = map.GetInchToPixel(),
///     Sizes       = [map.GetPixelMapSize().width, map.GetPixelMapSize().height],
///     PollingRate = ParserHelper.CombatReplayPollingRate,
///     Maps        = maps
/// };
/// foreach (var mapItem in mapDecorations) {
///     if (mapItem.ConnectedTo is PositionConnectorDescription posConnector && ...) {
///         maps.Add(new JsonCombatReplayMetaData.CombatReplayMap() {
///             Url      = metadata.Image,
///             Interval = [mapItem.Start, mapItem.End],
///             Position = posConnector.Position
///         });
///     }
/// }
/// ```
///
/// The three per-image fields resolve, for WvW, as:
///
/// - `Url` -- `WvWLogic.GetCombatMapInternal` builds the map's single
///   `ArenaDecoration` with the `LogImages` constant tabulated in
///   [`crate::wvw::maps::WvwMapDef::image_url`].
/// - `Interval` -- the decoration's lifespan, and `GetCombatMapInternal`
///   builds it with `var lifespan = (log.LogData.LogStart,
///   log.LogData.LogEnd)`, i.e. the whole fight. In log-relative ms that is
///   `[0, fight_end_ms]`, which the reference export confirms
///   (`[0, 348362]` for a 348362ms log).
///   `DecorationRenderingDescription`'s constructor throws on
///   `End <= Start`, so a non-positive `fight_end_ms` yields an EMPTY
///   `maps` list here rather than a degenerate entry.
/// - `Position` -- `PositionConnectorDescription` runs the connector
///   through `map.GetMapCoordRounded`, and `ArenaDecoration`'s
///   `CombatReplayMap` overload connects at `new Vector3(crMap.TopX,
///   crMap.BottomY, 0)` -- the world-space corner that maps to the image's
///   top-left, hence `[0, 0]`. Computed rather than hard-coded so it stays
///   honest if a future map's rect is not corner-aligned.
///
/// `None` for any map id absent from [`crate::wvw::maps::WVW_MAPS`]: GW2EI
/// would fall back to the computed bounding box, which has no arena image
/// and therefore no meaningful `url`/`position` to export. Callers should
/// omit `combatReplayMetaData` entirely in that case (the EI-JSON field is
/// nullable).
pub fn combat_replay_meta(map_id: u32, fight_end_ms: i64) -> Option<CombatReplayMeta> {
    let def = crate::wvw::maps::map_def(map_id)?;
    let map = MapTransform::new(def.pixel_size, def.rect);
    let mut maps = Vec::new();
    if fight_end_ms > 0 {
        maps.push(CombatReplayMapImage {
            url: def.image_url,
            interval: [0, fight_end_ms],
            // `new Vector3(crMap.TopX, crMap.BottomY, 0)` -- note GW2EI's
            // `Vector3` is `float`, so the world corner is narrowed to
            // `f32` before the transform, exactly as here.
            position: map.to_map_pixel(def.rect.0 as f32, def.rect.3 as f32),
        });
    }
    Some(CombatReplayMeta {
        inch_to_pixel: map.inch_to_pixel() as f32,
        polling_rate: EI_POLL_MS,
        sizes: map.sizes(),
        maps,
    })
}

/// C#'s `(float)Math.Round(value, 3)`. .NET's `Math.Round(double, int)`
/// with the default `MidpointRounding.ToEven` scales by `10^digits`, calls
/// the half-to-even `Math.Round(double)`, and unscales; GW2EI then narrows
/// the result to `float` before serializing. Ties are vanishingly unlikely
/// on real coordinates, but reproducing the exact mode costs nothing and
/// removes a whole class of "off by one in the last digit" parity noise.
fn round_ei(v: f64) -> f32 {
    (round_ties_even(v * DATA_SCALE) / DATA_SCALE) as f32
}

/// `f64::round_ties_even`, hand-rolled: the standard-library method is only
/// stable since Rust 1.77 and this crate's MSRV is 1.74 (`Cargo.toml`'s
/// `rust-version`), which `clippy::incompatible_msrv` correctly flags.
///
/// `f64::round` is round-half-AWAY-from-zero, so it already agrees with
/// round-half-to-even everywhere except at an exact `.5`; there, step the
/// result one toward zero when it came out odd.
fn round_ties_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - v.signum()
    } else {
        r
    }
}

/// GW2EI's `-facing.XYZ.GetRoundedZRotationDeg()` -- see this module's doc
/// comment (section 6). The `(float)` narrowing of `Math.Atan2`'s `double`
/// result BEFORE the radian -> degree conversion is deliberate and
/// reproduced: skipping it shifts the last exported decimal on a
/// meaningful fraction of samples.
fn facing_to_degrees(x: f32, y: f32) -> f64 {
    let rad = f64::from(y).atan2(f64::from(x)) as f32;
    let deg = f64::from(rad) * 180.0 / std::f64::consts::PI;
    f64::from(-round_ei(deg))
}

/// Which GW2EI `StatusEvent` subclass a statechange maps to. The
/// discriminant order is GW2EI's own `GetAgentStatus` concatenation order
/// (downs, alives, deads, spawns, despawns), which LINQ's STABLE
/// `OrderBy(x => x.Time)` preserves as the same-timestamp tiebreak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StatusKind {
    Down,
    Alive,
    Dead,
    Spawn,
    Despawn,
}

/// A closed `[start, end]` interval in GW2EI's exported shape: a plain
/// two-element array of (possibly sentinel) i64 milliseconds.
pub type EiInterval = [i64; 2];

/// One actor's GW2EI-shape `combatReplayData`. Field names/semantics match
/// GW2EI's `JsonActorCombatReplayData` exactly (M15 Task 3 maps this 1:1
/// into the EI-JSON output); `agent_addr`/`name`/`is_squad` are the join
/// keys back to [`Encounter::players`]/[`Encounter::enemies`], mirroring
/// [`crate::analysis::replay::Track`]'s own.
#[derive(Debug, Clone, PartialEq)]
pub struct EiTrack {
    /// Representative raw agent addr (first of `agent_addrs`).
    pub agent_addr: u64,
    /// `Player::character` / `Enemy::name`.
    pub name: String,
    /// `true` for squad players, `false` for enemy-player representatives.
    pub is_squad: bool,
    /// GW2EI's `combatReplayData.start` -- the trimmed replay window's
    /// start (in practice this actor's first-aware time).
    pub start: i64,
    /// GW2EI's `combatReplayData.end`.
    pub end: i64,
    /// GW2EI's `combatReplayData.positions`: one `[x, y]` MAP-PIXEL pair
    /// per grid point in `[start, end]`.
    pub positions: Vec<[f64; 2]>,
    /// GW2EI's `combatReplayData.orientations`: degrees, same length as
    /// `positions` when the actor has any facing telemetry, EMPTY
    /// otherwise (GW2EI polls rotations only when `_Rotations.Count > 0`).
    pub orientations: Vec<f64>,
    /// GW2EI's `combatReplayData.dc`, including the `[i64::MIN,
    /// first_aware]` / `[last_aware, i64::MAX]` sentinel bracketing.
    pub dc: Vec<EiInterval>,
    /// GW2EI's `combatReplayData.down`.
    pub down: Vec<EiInterval>,
    /// GW2EI's `combatReplayData.dead`.
    pub dead: Vec<EiInterval>,
}

/// Build GW2EI-shape combat-replay data for every squad player and every
/// enemy-player representative, in exactly the order
/// [`crate::analysis::replay::build_replay`] uses (all of `enc.players`,
/// then the `is_player` entries of `enc.enemies`) -- so the two are
/// positionally interchangeable and Task 3 can join either way.
///
/// `map` is the world -> map-pixel transform; get it from
/// [`MapTransform::from_log`], or from [`bounding_box_transform`] when the
/// log's map isn't one GW2EI ships an image for.
///
/// Deterministic: no hashing anywhere on this path (agent folding uses
/// [`BTreeSet`], every other collection is an ordered `Vec` built by a
/// single forward scan).
pub fn build_ei_replay(raw: &RawLog, enc: &Encounter, map: &MapTransform) -> Vec<EiTrack> {
    let world = build_world_tracks(raw, enc);
    world
        .into_iter()
        .map(|w| w.into_ei_track(map))
        .collect()
}

/// Everything `axilog-ei` needs to emit GW2EI's whole combat-replay
/// surface for one log (M15 Task 3): the per-actor tracks plus the
/// top-level `combatReplayMetaData` that describes the arena image they're
/// drawn on.
#[derive(Debug, Clone, PartialEq)]
pub struct EiReplay {
    /// Per-actor tracks, in [`build_ei_replay`]'s order (every
    /// `enc.players` entry, then the `is_player` entries of `enc.enemies`).
    pub tracks: Vec<EiTrack>,
    /// The log's WvW map id (`CBTS_MAPID`), when it has one at all.
    pub map_id: Option<u32>,
    /// GW2EI's `combatReplayMetaData`, or `None` when the map id isn't one
    /// GW2EI ships an arena image for -- see [`combat_replay_meta`]. The
    /// `tracks` above are still fully populated in that case (via
    /// [`bounding_box_transform`]); only the metadata is unavailable, and
    /// the EI-JSON field is nullable, so consumers omit it.
    pub meta: Option<CombatReplayMeta>,
}

/// The log's WvW map id, from the `CBTS_MAPID` (`sc::MAP_ID`, 25)
/// statechange's `src_agent` -- the same lookup [`crate::wvw::apply`] does
/// for [`Encounter::map`]. `None` when the log carries no map-id event.
pub fn map_id_from_log(raw: &RawLog) -> Option<u32> {
    raw.events
        .iter()
        .find(|e| e.is_statechange == sc::MAP_ID)
        .map(|e| e.src_agent as u32)
}

/// One-call combat replay for a whole log: picks the world -> map-pixel
/// transform the way GW2EI itself does, applies it to every polled track,
/// and pairs the result with the matching `combatReplayMetaData`.
///
/// Transform selection mirrors `LogLogic.GetCombatReplayMap`:
///
/// 1. The log's map id has a built-in `CombatReplayMap`
///    ([`crate::wvw::maps::WVW_MAPS`]) -> use it, and export
///    [`combat_replay_meta`] alongside.
/// 2. Otherwise GW2EI falls back to `new CombatReplayMap((800, 800), (0, 0,
///    0, 0))` + `ComputeBoundingBox(log)` -> [`bounding_box_transform`],
///    with NO arena image and therefore no metadata (`meta: None`).
/// 3. If even that is unavailable, the tracks keep their grid/interval
///    data but their `positions`/`orientations` are emptied rather than
///    filled with the NaNs a degenerate zero-area rect would produce.
///    GW2EI has no equivalent guard because it never renders such a log;
///    axilog serializes unconditionally, so it needs one. This branch is
///    narrow: `forcePolling` gives every SQUAD player a sentinel position
///    even with no position event anywhere in the log, so one squad member
///    is enough for the box, and only a squad-less roster (enemy players
///    only) reaches it.
///
/// Deterministic; see [`build_ei_replay`].
pub fn build_ei_replay_auto(raw: &RawLog, enc: &Encounter) -> EiReplay {
    let world = build_world_tracks(raw, enc);
    let map_id = map_id_from_log(raw);
    let built_in = map_id.and_then(MapTransform::for_map_id);
    let meta = built_in
        .is_some()
        .then(|| combat_replay_meta(map_id.expect("built-in transform implies a map id"), enc.duration_ms as i64))
        .flatten();
    let transform = match built_in {
        Some(m) => Some(m),
        None => bounding_box_transform(&world),
    };
    let tracks = match transform {
        Some(m) => world.into_iter().map(|w| w.into_ei_track(&m)).collect(),
        None => world
            .into_iter()
            .map(|mut w| {
                w.positions.clear();
                w.rotations.clear();
                // Any transform works now that both sample lists are empty;
                // the unit square keeps the arithmetic finite.
                w.into_ei_track(&MapTransform::new((1, 1), (0.0, 0.0, 1.0, 1.0)))
            })
            .collect(),
    };
    EiReplay { tracks, map_id, meta }
}

/// The pre-transform half of [`build_ei_replay`]: polled tracks still in
/// WORLD (game-inch) units. Split out because
/// [`bounding_box_transform`] needs them to derive a transform, which is
/// then applied to these same tracks -- GW2EI has the identical
/// chicken-and-egg (`CombatReplayMap.ComputeBoundingBox` reads
/// `p.GetCombatReplayPolledPositions(log)`).
pub fn build_world_tracks(raw: &RawLog, enc: &Encounter) -> Vec<WorldTrack> {
    let t0 = raw.log_start_ms();
    let log_duration = enc.duration_ms as i64;

    let mut out = Vec::with_capacity(enc.players.len() + enc.enemies.len());
    for p in &enc.players {
        out.push(build_world_track(
            raw,
            t0,
            log_duration,
            &p.agent_addrs,
            p.character.clone(),
            true,
        ));
    }
    for en in &enc.enemies {
        if !en.is_player {
            continue; // GW2EI's `players[]` array is player actors only
        }
        out.push(build_world_track(
            raw,
            t0,
            log_duration,
            &en.agent_addrs,
            en.name.clone(),
            false,
        ));
    }
    out
}

/// One actor's polled track, still in world units. See
/// [`build_world_tracks`].
#[derive(Debug, Clone, PartialEq)]
pub struct WorldTrack {
    pub agent_addr: u64,
    pub name: String,
    pub is_squad: bool,
    pub start: i64,
    pub end: i64,
    /// Polled world positions, already trimmed to `[start, end]`.
    pub positions: Vec<Point3>,
    /// Polled world facing vectors, already trimmed to `[start, end]`;
    /// empty when the actor emitted no `CBTS_FACING` events.
    pub rotations: Vec<Point3>,
    pub dc: Vec<EiInterval>,
    pub down: Vec<EiInterval>,
    pub dead: Vec<EiInterval>,
}

impl WorldTrack {
    /// Apply the world -> map-pixel transform and GW2EI's angle
    /// convention, yielding the exported shape.
    pub fn into_ei_track(self, map: &MapTransform) -> EiTrack {
        EiTrack {
            agent_addr: self.agent_addr,
            name: self.name,
            is_squad: self.is_squad,
            start: self.start,
            end: self.end,
            positions: self.positions.iter().map(|p| map.to_map_pixel(p.x, p.y)).collect(),
            orientations: self.rotations.iter().map(|r| facing_to_degrees(r.x, r.y)).collect(),
            dc: self.dc,
            down: self.down,
            dead: self.dead,
        }
    }
}

/// GW2EI's fallback map rect for logs whose map id it has no image for
/// (`LogLogic.GetCombatMapInternal`'s `new CombatReplayMap((800, 800), (0,
/// 0, 0, 0))` followed by `GetCombatReplayMap`'s
/// `Map.ComputeBoundingBox(log)`):
/// ```csharp
/// foreach (Player p in log.PlayerList) {                 // SQUAD players only
///     if (!p.HasCombatReplayPositions(log)) continue;
///     var pos = p.GetCombatReplayPolledPositions(log);   // trimmed, polled
///     _rectInMap.topX    = Math.Min(Math.Floor(pos.Min(x => x.XYZ.X)) - 250, _rectInMap.topX);
///     _rectInMap.topY    = Math.Min(Math.Floor(pos.Min(x => x.XYZ.Y)) - 250, _rectInMap.topY);
///     _rectInMap.bottomX = Math.Max(Math.Floor(pos.Max(x => x.XYZ.X)) + 250, _rectInMap.bottomX);
///     _rectInMap.bottomY = Math.Max(Math.Floor(pos.Max(x => x.XYZ.Y)) + 250, _rectInMap.bottomY);
/// }
/// FixAspectRatio();   // pad the shorter axis symmetrically until width == height
/// ```
/// (The seed values are `int.MaxValue`/`int.MinValue`, so the first player
/// simply establishes the box.) With `width == height` after
/// `FixAspectRatio`, `GetPixelMapSize` returns `(750, 750)`.
///
/// Returns `None` when no squad player has any polled position (nothing to
/// bound).
pub fn bounding_box_transform(tracks: &[WorldTrack]) -> Option<MapTransform> {
    let mut top_x = f64::INFINITY;
    let mut top_y = f64::INFINITY;
    let mut bottom_x = f64::NEG_INFINITY;
    let mut bottom_y = f64::NEG_INFINITY;
    let mut any = false;
    for tr in tracks.iter().filter(|t| t.is_squad && !t.positions.is_empty()) {
        any = true;
        let min_x = tr.positions.iter().map(|p| f64::from(p.x)).fold(f64::INFINITY, f64::min);
        let min_y = tr.positions.iter().map(|p| f64::from(p.y)).fold(f64::INFINITY, f64::min);
        let max_x = tr.positions.iter().map(|p| f64::from(p.x)).fold(f64::NEG_INFINITY, f64::max);
        let max_y = tr.positions.iter().map(|p| f64::from(p.y)).fold(f64::NEG_INFINITY, f64::max);
        top_x = top_x.min(min_x.floor() - 250.0);
        top_y = top_y.min(min_y.floor() - 250.0);
        bottom_x = bottom_x.max(max_x.floor() + 250.0);
        bottom_y = bottom_y.max(max_y.floor() + 250.0);
    }
    if !any {
        return None;
    }
    // `CombatReplayMap.FixAspectRatio`.
    let width = bottom_x - top_x;
    let height = bottom_y - top_y;
    let pad = (width - height).abs() / 2.0;
    if width > height {
        top_y -= pad;
        bottom_y += pad;
    } else if height > width {
        top_x -= pad;
        bottom_x += pad;
    }
    Some(MapTransform::new((800, 800), (top_x, top_y, bottom_x, bottom_y)))
}

fn build_world_track(
    raw: &RawLog,
    t0: u64,
    log_duration: i64,
    addrs: &[u64],
    name: String,
    is_squad: bool,
) -> WorldTrack {
    let addr_set: BTreeSet<u64> = addrs.iter().copied().collect();
    let agent_addr = addrs.first().copied().unwrap_or(0);

    let (positions, velocities, rotations, statuses, aware) = collect_movement(raw, t0, &addr_set);
    // GW2EI's `AgentItem.FirstAware`/`LastAware`. An agent with no events
    // at all is degenerate (it wouldn't be in the roster); fall back to an
    // empty `[0, 0]` window rather than leaking sentinels.
    let (first_aware, last_aware) = aware.unwrap_or((0, 0));

    let status = fill_status(&statuses, first_aware, last_aware);
    // Every actor this module builds is a PLAYER actor, so the trim uses
    // `PlayerActor`'s override of `GetActiveSegmentsForCRTrim` (deads U
    // downs U actives), NOT the `actives`-only base implementation -- see
    // this module's doc comment, section 3.
    let cr_segments = player_cr_trim_segments(&status);
    let (start, end) = trim_bounds(&cr_segments, first_aware, last_aware, log_duration);

    // GW2EI's `AgentItem.AgentType` distinguishes `Player` from
    // `NonSquadPlayer` (`AgentItem.cs:45`); `SingleActor.cs:415` passes
    // `forcePolling = AgentItem.Type == AgentItem.AgentType.Player` (NOT
    // `IsPlayer`, which both share), and WvW enemy players are tagged
    // `NonSquadPlayer` (`WvWLogic.cs:327`). So GW2EI only forces a sentinel
    // for SQUAD players; an enemy-player representative with zero raw
    // position events gets `forcePolling = false` and an empty polled
    // track instead.
    let (polled_pos, polled_rot) = poll(&positions, &velocities, &rotations, log_duration, is_squad, EI_POLL_MS);

    let keep = |p: &&Point3| p.t >= start && p.t <= end;
    WorldTrack {
        agent_addr,
        name,
        is_squad,
        start,
        end,
        positions: polled_pos.iter().filter(keep).copied().collect(),
        rotations: polled_rot.iter().filter(keep).copied().collect(),
        dc: status.dc,
        down: status.down,
        dead: status.dead,
    }
}

/// Decode one folded agent's `CBTS_POSITION`/`CBTS_VELOCITY`/`CBTS_FACING`
/// payloads and collect its status events + first/last-aware bounds in a
/// single pass over `raw.events`.
///
/// Payload decode is M9's, unchanged: `dst_agent`'s low/high f32 halves are
/// x/y, `value`'s bit pattern is z (see
/// [`crate::analysis::replay`]'s module doc and `sc::POSITION`'s for the
/// citation trail). The per-kind validity filters are GW2EI's
/// `PositionEvent`/`RotationEvent`/`VelocityEvent.AddPoint3D` -- see this
/// module's doc comment, section 1.
///
/// Returns `(positions, velocities, rotations, statuses, aware)` with every
/// list ascending by time; `aware` is `None` only when the agent has no
/// events at all.
#[allow(clippy::type_complexity)]
fn collect_movement(
    raw: &RawLog,
    t0: u64,
    addr_set: &BTreeSet<u64>,
) -> (
    Vec<Point3>,
    Vec<Point3>,
    Vec<Point3>,
    Vec<(i64, StatusKind)>,
    Option<(i64, i64)>,
) {
    let mut positions = Vec::new();
    let mut velocities = Vec::new();
    let mut rotations = Vec::new();
    let mut statuses = Vec::new();
    let mut first_aware = i64::MAX;
    let mut last_aware = i64::MIN;
    let mut any = false;

    for e in raw.events.iter() {
        let t = time_rel(e, t0);
        // Awareness widens from BOTH sides -- see this module's doc
        // comment, section 3b. A player damaged before their own first
        // src-side event is "aware" from that damage on, and GW2EI's
        // `start`/`end`/`dc` all follow from that.
        let is_src = src_is_agent(e.is_statechange) && addr_set.contains(&e.src_agent);
        let is_dst = dst_is_agent(e.is_statechange) && addr_set.contains(&e.dst_agent);
        if is_src || is_dst {
            any = true;
            first_aware = first_aware.min(t);
            last_aware = last_aware.max(t);
        }
        if !is_src {
            continue; // movement + status telemetry is src-keyed
        }
        match e.is_statechange {
            sc::POSITION => {
                let p = decode_point(e, t);
                // `PositionEvent.AddPoint3D`: drop all-zero, non-finite, or
                // |xy| > 40000 points.
                let zero = p.x == 0.0 && p.y == 0.0 && p.z == 0.0;
                // GW2EI evaluates `XY().LengthSquared() > 16e8` in `f32`;
                // this widens to `f64` first, which is strictly more
                // accurate. The two can only disagree for a point whose
                // squared length is within an `f32` ULP of 16e8 (|xy| =
                // 40000 to ~5 significant figures) -- i.e. a point already
                // 40000 units from the origin, far outside any real WvW map.
                let far = f64::from(p.x) * f64::from(p.x) + f64::from(p.y) * f64::from(p.y) > 16e8;
                if !zero && p.is_finite() && !far {
                    positions.push(p);
                }
            }
            sc::VELOCITY => {
                let p = decode_point(e, t);
                if p.is_finite() {
                    velocities.push(p);
                }
            }
            sc::FACING => {
                let p = decode_point(e, t);
                if p.is_finite() {
                    rotations.push(p);
                }
            }
            sc::CHANGE_DOWN => statuses.push((t, StatusKind::Down)),
            sc::CHANGE_UP => statuses.push((t, StatusKind::Alive)),
            sc::CHANGE_DEAD => statuses.push((t, StatusKind::Dead)),
            SC_SPAWN => statuses.push((t, StatusKind::Spawn)),
            SC_DESPAWN => statuses.push((t, StatusKind::Despawn)),
            _ => {}
        }
    }

    // `raw.events` is already time-ascending, but folding across relog
    // addrs and defending against a non-monotone capture both argue for an
    // explicit stable sort. `sort_by_key` IS stable, so equal-time entries
    // keep their original relative order (which for `statuses` is then
    // re-tiebroken by `StatusKind` to match GW2EI's concatenation order --
    // see this module's doc comment, section 4).
    positions.sort_by_key(|p| p.t);
    velocities.sort_by_key(|p| p.t);
    rotations.sort_by_key(|p| p.t);
    statuses.sort_by_key(|s| (s.0, s.1));

    let aware = if any { Some((first_aware, last_aware)) } else { None };
    (positions, velocities, rotations, statuses, aware)
}

fn time_rel(e: &RawEvent, t0: u64) -> i64 {
    e.time as i64 - t0 as i64
}

/// M9's `decode_position`, generalized to all three movement statechanges
/// (they share `MovementEvent`'s packed layout in GW2EI, and `CBTS_VELOCITY`
/// / `CBTS_FACING` in arcdps).
fn decode_point(e: &RawEvent, t: i64) -> Point3 {
    let dst = e.dst_agent.to_le_bytes();
    Point3 {
        t,
        x: f32::from_le_bytes(dst[0..4].try_into().unwrap()),
        y: f32::from_le_bytes(dst[4..8].try_into().unwrap()),
        z: f32::from_bits(e.value as u32),
    }
}

/// GW2EI's `SingleActorStatusHelper.FillStatus` output.
#[derive(Debug, Clone, PartialEq, Default)]
struct Status {
    dead: Vec<EiInterval>,
    down: Vec<EiInterval>,
    dc: Vec<EiInterval>,
    actives: Vec<EiInterval>,
}

/// GW2EI's `AddSegment`: append `[start, end]` only when `start < end`.
fn add_segment(v: &mut Vec<EiInterval>, start: i64, end: i64) {
    if start < end {
        v.push([start, end]);
    }
}

/// GW2EI's `SingleActorStatusHelper.FillStatus` + `AddValueToStatusList` --
/// see this module's doc comment, section 4, for the full transcription.
/// `statuses` must be ascending by `(time, kind)`.
fn fill_status(statuses: &[(i64, StatusKind)], first_aware: i64, last_aware: i64) -> Status {
    let mut s = Status::default();
    add_segment(&mut s.dc, i64::MIN, first_aware);

    if statuses.is_empty() {
        add_segment(&mut s.actives, first_aware, last_aware);
        add_segment(&mut s.dc, last_aware, i64::MAX);
        return s;
    }

    for i in 0..statuses.len() {
        let (t, kind) = statuses[i];
        let next_t = if i + 1 < statuses.len() { statuses[i + 1].0 } else { last_aware };
        match kind {
            StatusKind::Down => {
                if i == 0 {
                    add_segment(&mut s.actives, first_aware, t);
                }
                add_segment(&mut s.down, t, next_t);
            }
            StatusKind::Dead => {
                if i == 0 {
                    add_segment(&mut s.actives, first_aware, t);
                }
                add_segment(&mut s.dead, t, next_t);
            }
            StatusKind::Despawn => {
                if i == 0 {
                    add_segment(&mut s.actives, first_aware, t);
                }
                add_segment(&mut s.dc, t, next_t);
            }
            StatusKind::Alive | StatusKind::Spawn => {
                // GW2EI's 50ms grace: an alive/spawn event within 50ms of
                // first-aware is treated as "was there all along", not as a
                // late join.
                if i == 0 && t - first_aware > 50 {
                    add_segment(&mut s.dc, first_aware, t);
                }
                add_segment(&mut s.actives, t, next_t);
            }
        }
    }

    // GW2EI's tail: an actor still DEAD at last-aware extends `dead` to the
    // sentinel; everything else extends `dc`.
    if statuses[statuses.len() - 1].1 == StatusKind::Dead {
        add_segment(&mut s.dead, last_aware, i64::MAX);
    } else {
        add_segment(&mut s.dc, last_aware, i64::MAX);
    }
    s
}

/// GW2EI's `PlayerActor.GetActiveSegmentsForCRTrim`
/// (`GW2EIEvtcParser/EIData/Actors/PlayerActor.cs:57-66`): the segments
/// `TrimCombatReplay` measures a PLAYER's replay window against, which is
/// `deads ++ downs ++ actives` sorted by start -- see this module's doc
/// comment, section 3, for why the dead/down segments must be included.
///
/// GW2EI concatenates in that order and then `List<T>.Sort`s (an unstable
/// introsort) by `Start`; this uses a stable `sort_by_key`, which agrees
/// because no two of these segments can share a `Start` (and if a future
/// GW2EI change made that possible, keeping their concatenation order is
/// the closest match available).
fn player_cr_trim_segments(status: &Status) -> Vec<EiInterval> {
    let mut segments: Vec<EiInterval> = status
        .dead
        .iter()
        .chain(&status.down)
        .chain(&status.actives)
        .copied()
        .collect();
    segments.sort_by_key(|iv| iv[0]);
    segments
}

/// GW2EI's `SingleActor.TrimCombatReplay` + `CombatReplay.Trim`, reduced to
/// the `[start, end]` window they produce -- see this module's doc comment,
/// section 3. `segments` is [`player_cr_trim_segments`]' output (NOT bare
/// `actives`). `log_start` is `0` in this project's log-relative frame and
/// `log_end` is the log duration.
fn trim_bounds(segments: &[EiInterval], first_aware: i64, last_aware: i64, log_end: i64) -> (i64, i64) {
    let trim_start = segments.first().map(|a| a[0]).unwrap_or(first_aware);
    let trim_end = segments.last().map(|a| a[1]).unwrap_or(last_aware);
    let start = trim_start.max(first_aware).max(0);
    let end = start.max(trim_end.min(last_aware).min(log_end));
    (start, end)
}

/// The multiples of `poll_ms` that survive [`trim_bounds`] -- i.e. the grid
/// points GW2EI actually exports. Exposed for tests/callers that want the
/// sample TIMES (the exported JSON only carries the values); equivalent to
/// `(start..=end).step_by(poll)` intersected with the `t < log_duration`
/// polling loop bound.
pub fn grid_window(start: i64, end: i64, log_duration: i64, poll_ms: i64) -> impl Iterator<Item = i64> {
    let poll = poll_ms.max(1);
    // First multiple of `poll` at or after `start` (ceiling division that
    // is also correct for negative `start`).
    let first = start.div_euclid(poll) * poll + if start.rem_euclid(poll) == 0 { 0 } else { poll };
    // Last multiple of `poll` at or before `end` AND strictly below
    // `log_duration` (the `t < logDuration` loop bound).
    let cap = end.min(log_duration - 1);
    let last = cap.div_euclid(poll) * poll;
    let n = if last >= first { (last - first) / poll + 1 } else { 0 };
    (0..n).map(move |i| first + i * poll)
}

/// GW2EI's `CombatReplay.PollingRate` (+ `HandlePosition`/`HandleRotation`)
/// -- see this module's doc comment, section 2. Returns the UNTRIMMED
/// polled position and rotation tracks.
///
/// `force` is GW2EI's `forcePolling`: when the actor has no usable position
/// events at all, GW2EI synthesizes the single sentinel point
/// `new(int.MinValue, int.MinValue, 0, 0)` for players so the actor still
/// gets a (degenerate, far-off-map) track rather than vanishing from the
/// replay. Faithfully reproduced -- callers that would rather drop such
/// actors should filter on an empty raw-position list themselves.
fn poll(
    positions: &[Point3],
    velocities: &[Point3],
    rotations: &[Point3],
    log_duration: i64,
    force: bool,
    rate: i64,
) -> (Vec<Point3>, Vec<Point3>) {
    let rate = rate.max(1);
    let sentinel;
    let positions: &[Point3] = if positions.is_empty() {
        if !force {
            return (Vec::new(), Vec::new());
        }
        sentinel = [Point3 { t: 0, x: i32::MIN as f32, y: i32::MIN as f32, z: 0.0 }];
        &sentinel
    } else {
        positions
    };

    let do_rotation = !rotations.is_empty();
    // C# integer division truncates toward zero; `i64::div_euclid` does
    // NOT, so use the truncating `/` (Rust's default) to match.
    let pos_start_offset = 0.min(rate * ((positions[0].t / rate) - 1));
    let rot_start_offset = if do_rotation { 0.min(rate * ((rotations[0].t / rate) - 1)) } else { 0 };
    let start_offset = pos_start_offset.min(rot_start_offset);

    // Deliberate, documented divergence from a real GW2EI bug (audit
    // follow-up, not reproduced here): `CombatReplay.cs:210` allocates its
    // backing array at capacity `(logDuration - startOffset) / rate + 1`,
    // but the polling loop (`:216`-`:224`, the same `t < logDuration` bound
    // used below) only ever fills `cap - 1` of those slots whenever
    // `logDuration % rate == 0` -- the loop's own upper bound excludes the
    // exact-multiple endpoint, so the final pre-sized array slot is never
    // written and keeps its C# default `(0, 0, 0)` `Vector3`. For an actor
    // whose `first_aware == 0`, that stray zeroed sample can survive
    // `CombatReplay`'s own `Trim` (`:72`-`:74` only clamps by INDEX
    // position, not by re-validating each kept sample's value), and can
    // even push the visibly-exported track's last real timestamp past the
    // actor's own `end` -- a genuinely wrong `(0, 0, 0)`-at-`t=0` point
    // riding along with the real polled data.
    //
    // `cap` here is sized identically for parity with GW2EI's formula (kept
    // as a capacity hint only -- Rust's `Vec` never exposes uninitialized
    // slots the way a pre-sized C# array does), but the `while t <
    // log_duration` loop below pushes exactly one `Point3` per iteration,
    // so `polled_pos`/`polled_rot` end up with precisely `cap` (or `cap -
    // 1` when `log_duration % rate == 0`, matching GW2EI's own fill count)
    // REAL samples and never a leftover default point. No `(0, 0, 0)`
    // sample is ever emitted, and no track's samples ever run past `end`.
    let cap = ((log_duration - start_offset) / rate + 1).max(0) as usize;
    let mut polled_pos: Vec<Point3> = Vec::with_capacity(cap);
    let mut polled_rot: Vec<Point3> = Vec::with_capacity(if do_rotation { cap } else { 0 });

    let mut pos_i = 0usize;
    let mut vel_i = 0i64;
    let mut rot_i = 0usize;

    let mut t = start_offset;
    while t < log_duration {
        handle_position(t, positions, velocities, &mut polled_pos, &mut pos_i, &mut vel_i, rate);
        if do_rotation {
            handle_rotation(t, rotations, &mut polled_rot, &mut rot_i, rate);
        }
        t += rate;
    }
    (polled_pos, polled_rot)
}

/// GW2EI's `CombatReplay.HandlePosition`, tail recursion turned into a
/// loop:
/// ```csharp
/// ParametricPoint3D pos = _Positions[positionTableIndex];
/// if (t <= pos.Time) { emit pos@t; }                                  // before the track starts: hold
/// else if (positionTableIndex == _Positions.Count - 1) { emit pos@t; }// after it ends: hold
/// else {
///     ParametricPoint3D nextPos = _Positions[positionTableIndex + 1];
///     if (nextPos.Time < t) { positionTableIndex++; recurse; }
///     else {
///         var last = _PolledPositions[polledPositionTableIndex - 1].Time > pos.Time
///                  ? _PolledPositions[polledPositionTableIndex - 1] : pos;
///         velocityTableIndex = UpdateVelocityIndex(_Velocities, t, velocityTableIndex);
///         var velocity = (index in range) ? _Velocities[velocityTableIndex] : default;
///         if (nextPos.Time - last.Time > ArcDPSPollingRate + rate && velocity.XYZ.Length() < 1e-3)
///              emit pos@t;                                            // stale gap + standing still: hold
///         else emit Vector3.Lerp(last.XYZ, nextPos.XYZ, (t - last.Time) / (nextPos.Time - last.Time))@t;
///     }
/// }
/// ```
/// Two details worth spelling out, because a naive "lerp between the two
/// bracketing raw events" (which is what M9's native
/// [`crate::analysis::replay::downsample`] does) gets them wrong:
///
/// 1. **`last` is the PREVIOUS POLLED point when that is later than the
///    current raw point.** While the bracketing segment is unchanged this
///    is algebraically identical to interpolating from the raw point
///    (successive lerps toward a fixed target with matching ratios lie on
///    the same line), but after a HOLD it is not -- the track then eases
///    from where it was frozen toward the next real sample instead of
///    snapping onto the raw segment.
/// 2. **The hold branch.** When the next raw sample is more than
///    `ArcDPSPollingRate + rate` (600ms) away AND the most recent velocity
///    sample is ~zero, GW2EI refuses to interpolate and freezes the actor
///    at `pos`. This is the "player stood still, arcdps stopped emitting
///    position events" case; interpolating across it would drag the icon
///    smoothly toward a position it teleported to.
fn handle_position(
    t: i64,
    positions: &[Point3],
    velocities: &[Point3],
    out: &mut Vec<Point3>,
    pos_i: &mut usize,
    vel_i: &mut i64,
    rate: i64,
) {
    loop {
        let pos = positions[*pos_i];
        if t <= pos.t || *pos_i == positions.len() - 1 {
            out.push(pos.with_time(t));
            return;
        }
        let next = positions[*pos_i + 1];
        if next.t < t {
            *pos_i += 1;
            continue;
        }
        // GW2EI indexes `_PolledPositions[polledPositionTableIndex - 1]`
        // unguarded; that index is always valid in practice because the
        // very first grid point (`startOffset <= 0 <= positions[0].t`)
        // always takes the `t <= pos.Time` branch above. `last()` keeps us
        // panic-free if a future caller feeds negative times.
        let last = match out.last() {
            Some(l) if l.t > pos.t => *l,
            _ => pos,
        };
        let vel_len = update_velocity_index(velocities, t, vel_i)
            .map(|v| v.len())
            .unwrap_or(0.0);
        if next.t - last.t > ARCDPS_POLL_MS + rate && vel_len < 1e-3 {
            out.push(pos.with_time(t));
        } else if next.t == last.t {
            // GW2EI divides by `nextPos.Time - last.Time` unguarded. That
            // is safe there for the same reason it is here -- `last` is
            // either `pos` (and `next.t >= t > pos.t`) or the previous
            // polled point at `t - rate` (and `next.t >= t > t - rate`), so
            // the denominator is always positive. This arm only exists so a
            // future caller feeding an out-of-contract track (unsorted, or
            // negative times) gets the sensible endpoint instead of a NaN.
            out.push(next.with_time(t));
        } else {
            let ratio = (t - last.t) as f32 / (next.t - last.t) as f32;
            out.push(Point3::lerp(last, next, ratio, t));
        }
        return;
    }
}

/// GW2EI's `CombatReplay.HandleRotation` -- identical to
/// [`handle_position`] minus the velocity term (the hold branch fires on
/// the stale-gap test alone).
fn handle_rotation(t: i64, rotations: &[Point3], out: &mut Vec<Point3>, rot_i: &mut usize, rate: i64) {
    loop {
        let rot = rotations[*rot_i];
        if t <= rot.t || *rot_i == rotations.len() - 1 {
            out.push(rot.with_time(t));
            return;
        }
        let next = rotations[*rot_i + 1];
        if next.t < t {
            *rot_i += 1;
            continue;
        }
        let last = match out.last() {
            Some(l) if l.t > rot.t => *l,
            _ => rot,
        };
        if next.t - last.t > ARCDPS_POLL_MS + rate {
            out.push(rot.with_time(t));
        } else if next.t == last.t {
            // Unreachable for the same reason as in `handle_position`
            // (see the note there); kept as NaN insurance, not as GW2EI
            // parity -- GW2EI's `HandleRotation` divides unguarded.
            out.push(next.with_time(t));
        } else {
            let ratio = (t - last.t) as f32 / (next.t - last.t) as f32;
            out.push(Point3::lerp(last, next, ratio, t));
        }
        return;
    }
}

/// GW2EI's `CombatReplay.UpdateVelocityIndex`:
/// ```csharp
/// if (velocities.Count == 0) return -1;
/// int res = Math.Max(currentIndex, 0);
/// ParametricPoint3D cuvVelocity = velocities[res];
/// while (res < velocities.Count && cuvVelocity.Time < time) { res++; if (res < velocities.Count) cuvVelocity = velocities[res]; }
/// return res - 1;
/// ```
/// i.e. advance a monotone cursor to the first velocity at or after `time`
/// and report the one before it (`None` when `time` precedes every
/// velocity sample -- GW2EI's `default(ParametricPoint3D)`, a zero vector,
/// which reads as "standing still" in the hold test).
fn update_velocity_index(velocities: &[Point3], time: i64, cursor: &mut i64) -> Option<Point3> {
    if velocities.is_empty() {
        *cursor = -1;
        return None;
    }
    let mut res = (*cursor).max(0) as usize;
    while res < velocities.len() && velocities[res].t < time {
        res += 1;
    }
    let idx = res as i64 - 1;
    *cursor = idx;
    if idx >= 0 && (idx as usize) < velocities.len() {
        Some(velocities[idx as usize])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(t: i64, x: f32, y: f32) -> Point3 {
        Point3 { t, x, y, z: 0.0 }
    }

    // -- map transform (GW2EI `CombatReplayMap`) --

    /// `GetPixelMapSize` for Alpine Borderlands' `(697, 1000)` image:
    /// `ratio = 0.697 < 1` => `(round(0.697 * 750), 750) = (523, 750)`,
    /// exactly the reference export's `combatReplayMetaData.sizes`, and
    /// `GetInchToPixel` => `0.009`, exactly its `inchToPixel`.
    #[test]
    fn alpine_borderlands_matches_exported_metadata() {
        let m = MapTransform::for_map_id(96).expect("Blue Alpine Borderlands");
        assert_eq!(m.sizes(), [523, 750]);
        // GW2EI computes (and serializes) this as an `f32`, so the exact
        // `f64` here is `0.009f32` widened, not the decimal 0.009.
        assert_eq!(m.inch_to_pixel() as f32, 0.009f32, "got {}", m.inch_to_pixel());
    }

    #[test]
    fn pixel_map_size_handles_both_aspect_ratio_branches() {
        // Wider than tall -> width pinned to 750.
        let wide = MapTransform::new((2000, 1000), (0.0, 0.0, 1.0, 1.0));
        assert_eq!(wide.sizes(), [750, 375]);
        // Square -> 750x750, no rounding.
        let square = MapTransform::for_map_id(1099).expect("Red Desert Borderlands");
        assert_eq!(square.sizes(), [750, 750]);
        // Edge of the Mists: 3556/3646 < 1 -> (round(0.97531 * 750), 750).
        let eotm = MapTransform::for_map_id(968).expect("Edge of the Mists");
        assert_eq!(eotm.sizes(), [731, 750]);
    }

    /// The corners of the world rect must land on the corners of the output
    /// image, with the y axis FLIPPED (world north == image top).
    #[test]
    fn to_map_pixel_maps_rect_corners_with_flipped_y() {
        let m = MapTransform::for_map_id(95).expect("Green Alpine Borderlands");
        assert_eq!(m.to_map_pixel(-30720.0, -43008.0), [0.0, 750.0]);
        assert_eq!(m.to_map_pixel(30720.0, 43008.0), [523.0, 0.0]);
        assert_eq!(m.to_map_pixel(0.0, 0.0), [261.5, 375.0]);
    }

    #[test]
    fn unknown_map_id_has_no_builtin_transform() {
        assert!(MapTransform::for_map_id(1).is_none());
    }

    // -- combatReplayMetaData (M15 Task 2) --

    /// The controller-verified reference values for mapID 96, reproduced
    /// offline (the fixture-backed version of this assertion lives in
    /// `tests/ei_replay_golden.rs`).
    #[test]
    fn combat_replay_meta_reproduces_the_blue_alpine_reference() {
        let meta = combat_replay_meta(96, 348362).expect("mapID 96");
        assert_eq!(meta.inch_to_pixel, 0.009f32);
        assert_eq!(meta.polling_rate, 300);
        assert_eq!(meta.sizes, [523, 750]);
        assert_eq!(
            meta.maps,
            vec![CombatReplayMapImage {
                url: "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-nVu2ivF.png",
                interval: [0, 348362],
                position: [0.0, 0.0],
            }]
        );
    }

    /// Every map in the table exports one image whose `position` is the
    /// canvas origin -- the `ArenaDecoration` is connected at `(TopX,
    /// BottomY)`, the world corner that transforms to the image's top-left
    /// -- and whose `interval` spans the whole log.
    #[test]
    fn every_wvw_map_exports_one_full_length_origin_anchored_image() {
        for def in crate::wvw::maps::WVW_MAPS {
            let meta = combat_replay_meta(def.map_id, 1234).expect("table entry");
            assert_eq!(meta.polling_rate, EI_POLL_MS);
            assert_eq!(meta.maps.len(), 1, "map {}", def.map_id);
            let img = meta.maps[0];
            assert_eq!(img.url, def.image_url, "map {}", def.map_id);
            assert_eq!(img.interval, [0, 1234], "map {}", def.map_id);
            assert_eq!(img.position, [0.0, 0.0], "map {}", def.map_id);
            assert_eq!(
                meta.sizes,
                MapTransform::for_map_id(def.map_id).unwrap().sizes(),
                "map {}: metadata sizes must be the transform's",
                def.map_id
            );
            assert!(meta.inch_to_pixel > 0.0, "map {}", def.map_id);
        }
    }

    /// `DecorationRenderingDescription`'s constructor throws on
    /// `End <= Start`, so a zero-or-negative duration must yield no image
    /// rather than a degenerate one. The scalars are still meaningful.
    #[test]
    fn non_positive_duration_yields_no_arena_image() {
        for d in [0i64, -1] {
            let meta = combat_replay_meta(95, d).expect("mapID 95");
            assert!(meta.maps.is_empty(), "duration {d}");
            assert_eq!(meta.sizes, [523, 750]);
        }
    }

    #[test]
    fn combat_replay_meta_is_none_for_maps_gw2ei_has_no_image_for() {
        // Obsidian Sanctum / Armistice Bastion: named by GW2EI, empty
        // `LogImages` constant, no `GetCombatMapInternal` case.
        assert!(combat_replay_meta(899, 1000).is_none());
        assert!(combat_replay_meta(1315, 1000).is_none());
        assert!(combat_replay_meta(0, 1000).is_none());
    }

    /// 3 decimals, round-half-to-even, narrowed to `f32` -- GW2EI's
    /// `(float)Math.Round(v, ParserHelper.CombatReplayDataDigit)`.
    /// The hand-rolled `f64::round_ties_even` stand-in (MSRV 1.74) must
    /// agree with the standard library's semantics on every tie, on both
    /// signs, and be a plain round everywhere else.
    #[test]
    fn round_ties_even_matches_the_std_semantics() {
        for (input, want) in [
            (0.5f64, 0.0f64),
            (1.5, 2.0),
            (2.5, 2.0),
            (3.5, 4.0),
            (-0.5, 0.0),
            (-1.5, -2.0),
            (-2.5, -2.0),
            (0.4, 0.0),
            (0.6, 1.0),
            (-0.6, -1.0),
            (12.0, 12.0),
        ] {
            assert_eq!(round_ties_even(input), want, "round_ties_even({input})");
        }
    }

    #[test]
    fn round_ei_is_three_digit_half_to_even() {
        assert_eq!(round_ei(1.23456), 1.235);
        assert_eq!(round_ei(-1.23456), -1.235);
        assert_eq!(round_ei(0.0025), 0.002, "half-to-even rounds .0025 down to .002");
        assert_eq!(round_ei(0.0035), 0.004, "half-to-even rounds .0035 up to .004");
    }

    // -- orientation convention --

    /// `-round(degrees(atan2(y, x)), 3)`: facing +x is 0, facing +y is
    /// -90 (the sign flip is GW2EI's, see the module doc's section 6).
    #[test]
    fn facing_to_degrees_uses_negated_atan2_y_x() {
        assert_eq!(facing_to_degrees(1.0, 0.0), 0.0);
        assert_eq!(facing_to_degrees(0.0, 1.0), -90.0);
        assert_eq!(facing_to_degrees(-1.0, 0.0), -180.0);
        assert_eq!(facing_to_degrees(0.0, -1.0), 90.0);
        assert!((facing_to_degrees(1.0, 1.0) - -45.0).abs() < 1e-9);
    }

    // -- the polling grid --

    #[test]
    fn grid_window_is_the_multiples_of_the_poll_rate_inside_start_end() {
        let g: Vec<i64> = grid_window(23, 1000, 100_000, 300).collect();
        assert_eq!(g, vec![300, 600, 900]);
        // start exactly on a grid point is INCLUSIVE.
        let g: Vec<i64> = grid_window(0, 600, 100_000, 300).collect();
        assert_eq!(g, vec![0, 300, 600]);
    }

    /// The reference export's own sample counts (see the module doc,
    /// section 3): 1159 for a player first-aware at 23, 1160 for one
    /// first-aware at 0, 1032 for the one who joined at 38317 -- all with
    /// `end` in `347736..=347977` and `logDuration = 348362`.
    #[test]
    fn grid_window_reproduces_the_reference_export_sample_counts() {
        const DUR: i64 = 348_362;
        assert_eq!(grid_window(23, 347_977, DUR, 300).count(), 1159);
        assert_eq!(grid_window(0, 347_976, DUR, 300).count(), 1160);
        assert_eq!(grid_window(2, 347_736, DUR, 300).count(), 1159);
        assert_eq!(grid_window(38_317, 347_977, DUR, 300).count(), 1032);
    }

    /// The `t < logDuration` loop bound: a grid point exactly AT the log
    /// duration is never polled, even if `end` would allow it.
    #[test]
    fn grid_window_excludes_a_point_exactly_at_log_duration() {
        let g: Vec<i64> = grid_window(0, 900, 900, 300).collect();
        assert_eq!(g, vec![0, 300, 600]);
    }

    #[test]
    fn grid_window_is_empty_when_start_is_past_end() {
        assert_eq!(grid_window(500, 400, 100_000, 300).count(), 0);
    }

    // -- interpolation / edge clamping --

    /// Two raw samples 300ms apart (inside the 600ms stale-gap threshold,
    /// so the hold branch never fires): the grid points between them are a
    /// straight linear interpolation.
    #[test]
    fn interpolates_linearly_between_two_bracketing_samples() {
        let pos = [p(0, 0.0, 0.0), p(300, 30.0, 60.0)];
        let (pp, pr) = poll(&pos, &[], &[], 400, true, 100);
        // Grid: startOffset = min(0, 100*((0/100)-1)) = -100, then
        // -100, 0, 100, 200, 300 (t < 400).
        let times: Vec<i64> = pp.iter().map(|q| q.t).collect();
        assert_eq!(times, vec![-100, 0, 100, 200, 300]);
        assert_eq!((pp[0].x, pp[0].y), (0.0, 0.0), "held before the track starts");
        assert_eq!((pp[2].x, pp[2].y), (10.0, 20.0));
        assert_eq!((pp[3].x, pp[3].y), (20.0, 40.0));
        assert_eq!((pp[4].x, pp[4].y), (30.0, 60.0));
        assert!(pr.is_empty(), "no facing events -> no polled rotations");
    }

    /// Before the first and after the last raw sample GW2EI HOLDS the
    /// nearest endpoint (it never extrapolates).
    #[test]
    fn clamps_by_holding_the_first_and_last_sample() {
        let pos = [p(500, 10.0, 20.0), p(800, 40.0, 20.0)];
        let (pp, _) = poll(&pos, &[], &[], 2000, true, 300);
        // startOffset = min(0, 300*((500/300)-1)) = min(0, 0) = 0.
        assert_eq!(pp.first().unwrap().t, 0);
        assert_eq!((pp[0].x, pp[0].y), (10.0, 20.0), "held at the first sample");
        let last = pp.last().unwrap();
        assert_eq!(last.t, 1800);
        assert_eq!((last.x, last.y), (40.0, 20.0), "held at the last sample");
    }

    /// The hold branch: a >600ms gap to the next raw sample WITH a ~zero
    /// velocity freezes the actor instead of sliding it. Same inputs with
    /// a non-zero velocity interpolate instead -- this is the assertion
    /// that a plain-lerp implementation fails.
    #[test]
    fn holds_across_a_stale_gap_only_when_velocity_is_zero() {
        let pos = [p(0, 0.0, 0.0), p(3000, 3000.0, 0.0)];
        let still = [Point3 { t: 0, x: 0.0, y: 0.0, z: 0.0 }];
        let (held, _) = poll(&pos, &still, &[], 1000, true, 300);
        assert!(
            held.iter().all(|q| q.x == 0.0),
            "zero velocity + 3000ms gap (> ArcDPSPollingRate + rate) must hold: {held:?}"
        );

        let moving = [Point3 { t: 0, x: 5.0, y: 0.0, z: 0.0 }];
        let (lerped, _) = poll(&pos, &moving, &[], 1000, true, 300);
        let at_300 = lerped.iter().find(|q| q.t == 300).expect("t=300");
        assert!((at_300.x - 300.0).abs() < 1e-3, "non-zero velocity must interpolate: {at_300:?}");
    }

    /// REGRESSION: `HandlePosition` anchors the interpolation at the
    /// PREVIOUS POLLED point whenever that is later than the current raw
    /// point -- `_PolledPositions[i-1].Time > pos.Time ? _PolledPositions[i-1] : pos`.
    /// While the bracketing segment is unchanged this is algebraically the
    /// same as anchoring at the raw point, so only a run that exits a HOLD
    /// can tell them apart: the track must ease out of where it was frozen,
    /// not snap onto the raw `A -> B` line.
    ///
    /// Setup: `A@0` and `B@3000` (a 3000ms gap, over the 600ms stale
    /// threshold), velocity zero at `t=0` and non-zero from `t=1000`. Grid
    /// points 300/600/900 hold at `A`; `t=1200` is the first to interpolate,
    /// and it must do so from the held point at `t=900` -- ratio
    /// `(1200-900)/(3000-900) = 1/7`, giving `x = 3000/7 = 428.57`, NOT the
    /// `1200` a `last = pos` anchor would give.
    #[test]
    fn interpolation_anchors_at_the_previous_polled_point_after_a_hold() {
        let pos = [p(0, 0.0, 0.0), p(3000, 3000.0, 0.0)];
        let vels = [
            Point3 { t: 0, x: 0.0, y: 0.0, z: 0.0 },
            Point3 { t: 1000, x: 9.0, y: 0.0, z: 0.0 },
        ];
        let (pp, _) = poll(&pos, &vels, &[], 2000, true, 300);
        let at = |t: i64| pp.iter().find(|q| q.t == t).unwrap_or_else(|| panic!("t={t}")).x;

        assert_eq!(at(300), 0.0, "zero velocity + 3000ms gap -> hold at A");
        assert_eq!(at(900), 0.0, "still held (the velocity burst starts at t=1000)");

        let x1200 = at(1200);
        assert!(
            (x1200 - 3000.0 / 7.0).abs() < 1e-2,
            "must interpolate from the HELD polled point at t=900, got {x1200} (want ~428.57)"
        );
        assert!(
            (x1200 - 1200.0).abs() > 1.0,
            "a `last = pos` anchor would have produced 1200 here -- the previous-polled-point \
             semantic is not being exercised"
        );

        // ...and it keeps easing from each successive polled point: the
        // anchor is the t=1200 sample, so the denominator is
        // `3000 - 1200 = 1800`, not `3000 - 1500`.
        let x1500 = at(1500);
        let expect1500 = x1200 + (3000.0 - x1200) * (300.0 / 1800.0);
        assert!((x1500 - expect1500).abs() < 1e-2, "got {x1500}, want ~{expect1500}");
    }

    /// A velocity sample only counts once its own timestamp is at or
    /// before the grid point (GW2EI's `UpdateVelocityIndex` reports the
    /// last velocity strictly BEFORE `t`), so a movement burst that starts
    /// after the grid point still reads as "standing still".
    #[test]
    fn velocity_cursor_reports_the_sample_before_the_grid_point() {
        let vels = [
            Point3 { t: 100, x: 0.0, y: 0.0, z: 0.0 },
            Point3 { t: 700, x: 9.0, y: 0.0, z: 0.0 },
        ];
        let mut cursor = 0i64;
        assert!(update_velocity_index(&vels, 50, &mut cursor).is_none(), "nothing before t=50");
        let mut cursor = 0i64;
        assert_eq!(update_velocity_index(&vels, 500, &mut cursor).map(|v| v.x), Some(0.0));
        assert_eq!(update_velocity_index(&vels, 900, &mut cursor).map(|v| v.x), Some(9.0));
    }

    /// Rotations are polled on the SAME grid as positions, and the
    /// combined `startOffset` is the earlier of the two tracks' own
    /// offsets -- so `orientations.len() == positions.len()` always (before
    /// trimming, and therefore after it too, since both are trimmed with
    /// the same predicate).
    #[test]
    fn rotations_are_polled_on_the_same_grid_as_positions() {
        let pos = [p(0, 0.0, 0.0), p(600, 6.0, 0.0)];
        let rot = [p(0, 1.0, 0.0), p(600, 0.0, 1.0)];
        let (pp, pr) = poll(&pos, &[], &rot, 900, true, 300);
        assert_eq!(pp.len(), pr.len());
        assert_eq!(pr.iter().map(|q| q.t).collect::<Vec<_>>(), pp.iter().map(|q| q.t).collect::<Vec<_>>());
    }

    /// GW2EI's `forcePolling` sentinel: a player with NO usable position
    /// events still gets a full-length track, pinned at
    /// `(int.MinValue, int.MinValue)`.
    #[test]
    fn empty_position_track_yields_the_force_polling_sentinel() {
        let (pp, _) = poll(&[], &[], &[], 900, true, 300);
        assert_eq!(pp.len(), 4, "grid -300, 0, 300, 600 (t < 900)");
        assert!(pp.iter().all(|q| q.x == i32::MIN as f32 && q.y == i32::MIN as f32));
        // Without forcePolling (GW2EI's non-player actors) the track is
        // simply empty.
        let (none, _) = poll(&[], &[], &[], 900, false, 300);
        assert!(none.is_empty());
    }

    /// A rotation track with no positions polls NOTHING -- GW2EI's
    /// `PollingRate` returns before the loop when `_Positions` is empty and
    /// `forcePolling` is false.
    #[test]
    fn rotations_without_positions_and_without_force_poll_nothing() {
        let rot = [p(0, 1.0, 0.0)];
        let (pp, pr) = poll(&[], &[], &rot, 900, false, 300);
        assert!(pp.is_empty() && pr.is_empty());
    }

    #[test]
    fn polling_is_deterministic() {
        let pos = [p(0, 0.0, 0.0), p(137, 5.0, -5.0), p(401, 12.0, 8.0), p(900, 0.0, 0.0)];
        let a = poll(&pos, &[], &[], 1200, true, 300);
        let b = poll(&pos, &[], &[], 1200, true, 300);
        assert_eq!(a, b);
    }

    #[test]
    fn polled_positions_are_never_nan() {
        let pos = [p(0, 1.0, 2.0), p(50, 1.0, 2.0), p(50, 3.0, 4.0), p(900, 9.0, 9.0)];
        let (pp, _) = poll(&pos, &[], &[], 1200, true, 300);
        assert!(pp.iter().all(|q| q.x.is_finite() && q.y.is_finite()), "{pp:?}");
    }

    /// Deliberate, documented divergence from a real GW2EI bug (see `poll`'s
    /// doc comment above `cap`): when `logDuration % rate == 0`
    /// (`CombatReplay.cs:210`/`:216`/`:224`), GW2EI's own pre-sized backing
    /// array leaves its last, never-written slot at the C# default `(0, 0,
    /// 0)`, and that stray sample can survive `Trim` (`:72`-`:74`) for an
    /// actor whose `first_aware == 0`. axilog's `Vec`-based loop has no such
    /// pre-sized slot to leave stale, so it must never emit that spurious
    /// point, and no sample may run past the loop's own `t < log_duration`
    /// bound either.
    #[test]
    fn log_duration_multiple_of_the_poll_rate_leaves_no_spurious_zero_sample() {
        let pos = [p(0, 5.0, 5.0), p(600, 9.0, 9.0)];
        let (pp, _) = poll(&pos, &[], &[], 900, true, 300); // 900 % 300 == 0
        // Grid starts one poll rate before the first sample (`start_offset`,
        // same convention as `empty_position_track_yields_the_force_polling_
        // sentinel`); t=900 is excluded by `t < log_duration`.
        assert_eq!(pp.iter().map(|q| q.t).collect::<Vec<_>>(), vec![-300, 0, 300, 600]);
        assert!(pp.iter().all(|q| q.t < 900), "no sample may run past log_duration: {pp:?}");
        assert!(
            !pp.iter().any(|q| q.x == 0.0 && q.y == 0.0 && q.z == 0.0),
            "no default (0,0,0) sample may survive: {pp:?}"
        );
    }

    // -- dc / down / dead bracketing --

    /// The reference export's universal shape: no status events at all ->
    /// `dc = [[i64::MIN, first_aware], [last_aware, i64::MAX]]`.
    #[test]
    fn dc_brackets_the_awareness_window_with_i64_sentinels() {
        let s = fill_status(&[], 23, 347_977);
        assert_eq!(s.dc, vec![[i64::MIN, 23], [347_977, i64::MAX]]);
        assert_eq!(s.actives, vec![[23, 347_977]]);
        assert!(s.down.is_empty() && s.dead.is_empty());
    }

    /// Down/up cycles land on `down` and leave `dc` at just the two
    /// sentinels -- reproducing the two-down-cycle shape one of the
    /// reference export's players has
    /// (`down = [[183019, 185181], [188193, 199231]]`).
    #[test]
    fn down_up_cycles_fill_down_not_dc() {
        let st = [
            (183_019, StatusKind::Down),
            (185_181, StatusKind::Alive),
            (188_193, StatusKind::Down),
            (199_231, StatusKind::Alive),
        ];
        let s = fill_status(&st, 0, 347_977);
        assert_eq!(s.down, vec![[183_019, 185_181], [188_193, 199_231]]);
        assert_eq!(s.dc, vec![[i64::MIN, 0], [347_977, i64::MAX]]);
    }

    /// The reference export's down -> dead -> respawn shape (the one
    /// player in that log who died and came back).
    #[test]
    fn down_then_dead_then_respawn() {
        let st = [
            (300_150, StatusKind::Down),
            (304_801, StatusKind::Dead),
            (310_123, StatusKind::Alive),
        ];
        let s = fill_status(&st, 2, 347_941);
        assert_eq!(s.down, vec![[300_150, 304_801]]);
        assert_eq!(s.dead, vec![[304_801, 310_123]]);
        assert_eq!(s.dc, vec![[i64::MIN, 2], [347_941, i64::MAX]]);
    }

    /// Still dead at the end of the log: the trailing sentinel goes on
    /// `dead`, NOT on `dc`.
    #[test]
    fn dead_at_log_end_extends_dead_to_the_sentinel_instead_of_dc() {
        let st = [(1000, StatusKind::Down), (2000, StatusKind::Dead)];
        let s = fill_status(&st, 0, 5000);
        assert_eq!(s.dead, vec![[2000, 5000], [5000, i64::MAX]]);
        assert_eq!(s.dc, vec![[i64::MIN, 0]], "no trailing dc sentinel when dead at the end");
    }

    /// A genuine mid-fight disconnect is `CBTS_DESPAWN` -> a `dc` segment
    /// between the two sentinels, ending at the respawn.
    #[test]
    fn mid_fight_despawn_adds_an_interior_dc_segment() {
        let st = [
            (1000, StatusKind::Despawn),
            (9000, StatusKind::Spawn),
        ];
        let s = fill_status(&st, 0, 20_000);
        assert_eq!(s.dc, vec![[i64::MIN, 0], [1000, 9000], [20_000, i64::MAX]]);
        assert_eq!(s.actives, vec![[0, 1000], [9000, 20_000]]);
    }

    /// A late spawn more than 50ms after first-aware is a leading `dc`
    /// segment; within 50ms it is GW2EI's grace window and adds nothing.
    #[test]
    fn late_spawn_grace_window_is_fifty_milliseconds() {
        let late = fill_status(&[(500, StatusKind::Alive)], 0, 5000);
        assert_eq!(late.dc, vec![[i64::MIN, 0], [0, 500], [5000, i64::MAX]]);

        let prompt = fill_status(&[(50, StatusKind::Alive)], 0, 5000);
        assert_eq!(prompt.dc, vec![[i64::MIN, 0], [5000, i64::MAX]], "50ms is NOT > 50");
    }

    /// Same-timestamp status events break ties in GW2EI's own
    /// concatenation order (downs, alives, deads, spawns, despawns), which
    /// its stable `OrderBy` preserves.
    #[test]
    fn status_kind_ordering_matches_gw2ei_concatenation_order() {
        let mut v = vec![
            StatusKind::Despawn,
            StatusKind::Dead,
            StatusKind::Down,
            StatusKind::Spawn,
            StatusKind::Alive,
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                StatusKind::Down,
                StatusKind::Alive,
                StatusKind::Dead,
                StatusKind::Spawn,
                StatusKind::Despawn
            ]
        );
    }

    // -- trimming --

    #[test]
    fn trim_bounds_defaults_to_the_awareness_window() {
        assert_eq!(trim_bounds(&[[23, 347_977]], 23, 347_977, 348_362), (23, 347_977));
        // No actives at all (degenerate) -> first/last aware.
        assert_eq!(trim_bounds(&[], 5, 900, 10_000), (5, 900));
    }

    #[test]
    fn trim_bounds_clamps_to_the_log_end_and_never_inverts() {
        // `end` past the log duration is clamped down...
        assert_eq!(trim_bounds(&[[0, 99_999]], 0, 99_999, 5_000), (0, 5_000));
        // ...and `end` can never precede `start`.
        assert_eq!(trim_bounds(&[[900, 100]], 900, 100, 10_000), (900, 900));
    }

    /// Mid-fight despawn/respawn: GW2EI trims to the FIRST active
    /// segment's start and the LAST one's end, spanning the gap.
    #[test]
    fn trim_bounds_spans_a_mid_fight_dc_gap() {
        let actives = [[0, 1000], [9000, 20_000]];
        assert_eq!(trim_bounds(&actives, 0, 20_000, 30_000), (0, 20_000));
    }

    /// REGRESSION: `PlayerActor.GetActiveSegmentsForCRTrim` merges
    /// `deads ++ downs ++ actives`, so a player who goes down at `D` and is
    /// NEVER revived before last-aware keeps their track to the END of the
    /// fight. Trimming on `actives` alone (the `SingleActor` BASE
    /// implementation, which does not apply to players) would truncate at
    /// `D` -- the second assertion pins exactly that difference.
    #[test]
    fn trim_bounds_keeps_a_player_who_is_still_down_at_last_aware() {
        let status = fill_status(&[(1000, StatusKind::Down)], 0, 5000);
        assert_eq!(status.actives, vec![[0, 1000]]);
        assert_eq!(status.down, vec![[1000, 5000]]);

        let segments = player_cr_trim_segments(&status);
        assert_eq!(segments, vec![[0, 1000], [1000, 5000]], "deads U downs U actives, sorted by start");
        assert_eq!(
            trim_bounds(&segments, 0, 5000, 10_000),
            (0, 5000),
            "still-down player must be polled to last-aware"
        );
        assert_eq!(
            trim_bounds(&status.actives, 0, 5000, 10_000),
            (0, 1000),
            "actives-only (the base-class rule) would truncate at the down -- \
             this is the bug the PlayerActor override prevents"
        );
    }

    /// Same, for a player DEAD at the end of the log: `FillStatus` gives
    /// `dead` an `[LastAware, i64::MAX]` tail, which becomes the
    /// highest-start segment; the `Math.Min(trimEnd, LastAware)` at GW2EI's
    /// call site clamps it back to last-aware rather than letting the
    /// sentinel escape.
    #[test]
    fn trim_bounds_keeps_a_player_who_is_dead_at_last_aware() {
        let status = fill_status(&[(1000, StatusKind::Down), (2000, StatusKind::Dead)], 0, 5000);
        assert_eq!(status.dead, vec![[2000, 5000], [5000, i64::MAX]]);

        let segments = player_cr_trim_segments(&status);
        assert_eq!(segments, vec![[0, 1000], [1000, 2000], [2000, 5000], [5000, i64::MAX]]);
        assert_eq!(trim_bounds(&segments, 0, 5000, 10_000), (0, 5000), "i64::MAX must be clamped away");
        assert_eq!(
            trim_bounds(&status.actives, 0, 5000, 10_000),
            (0, 1000),
            "actives-only would truncate at the down"
        );
    }

    // -- bounding-box fallback --

    #[test]
    fn bounding_box_transform_pads_and_squares_the_squad_extent() {
        let tr = WorldTrack {
            agent_addr: 1,
            name: "A".into(),
            is_squad: true,
            start: 0,
            end: 300,
            positions: vec![p(0, 0.0, 0.0), p(300, 1000.0, 500.0)],
            rotations: vec![],
            dc: vec![],
            down: vec![],
            dead: vec![],
        };
        let m = bounding_box_transform(&[tr]).expect("one squad track");
        // x: [0-250, 1000+250] = [-250, 1250] (width 1500)
        // y: [0-250,  500+250] = [-250,  750] (height 1000) -> pad 250 each side
        assert_eq!((m.top_x, m.bottom_x), (-250.0, 1250.0));
        assert_eq!((m.top_y, m.bottom_y), (-500.0, 1000.0));
        assert_eq!(m.sizes(), [750, 750], "square rect -> square output");
    }

    #[test]
    fn bounding_box_transform_ignores_enemies_and_empty_tracks() {
        let enemy = WorldTrack {
            agent_addr: 2,
            name: "E".into(),
            is_squad: false,
            start: 0,
            end: 300,
            positions: vec![p(0, 5.0, 5.0)],
            rotations: vec![],
            dc: vec![],
            down: vec![],
            dead: vec![],
        };
        assert!(bounding_box_transform(&[enemy]).is_none());
    }

    // -- end-to-end over a synthetic log --

    use crate::evtc::{RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn mov_event(time: u64, src: u64, statechange: u8, x: f32, y: f32, z: f32) -> RawEvent {
        let mut dst = [0u8; 8];
        dst[0..4].copy_from_slice(&x.to_le_bytes());
        dst[4..8].copy_from_slice(&y.to_le_bytes());
        RawEvent {
            time,
            src_agent: src,
            dst_agent: u64::from_le_bytes(dst),
            value: z.to_bits() as i32,
            is_statechange: statechange,
            ..blank()
        }
    }

    fn state_event(time: u64, src: u64, statechange: u8) -> RawEvent {
        RawEvent { time, src_agent: src, is_statechange: statechange, ..blank() }
    }

    fn blank() -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: 0,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
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

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn player(addr: u64, name: &str) -> Player {
        Player {
            agent_addr: addr,
            account: format!(":{name}.1"),
            character: name.into(),
            profession: "Thief".into(),
            elite_spec: String::new(),
            team: "red".into(),
            subgroup: 1,
            in_squad: true,
            commander: false,
            marker: None,
            commander_tag: None, guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    fn enc_with(players: Vec<Player>, enemies: Vec<Enemy>, duration_ms: u64) -> Encounter {
        Encounter {
            kind: "wvw".into(), pve: None,
            map: "Blue Alpine Borderlands".into(),
            duration_ms,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players,
            enemies,
            markers: vec![], ground_markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    /// M15 Task 3: `build_ei_replay_auto` on a log whose map GW2EI ships an
    /// image for -- the transform comes from the map table, and the
    /// metadata is exported alongside.
    #[test]
    fn auto_replay_uses_the_built_in_map_and_exports_metadata() {
        let enc = enc_with(vec![player(1, "Alice")], vec![], 1200);
        let mut events = vec![state_event(0, 96, sc::MAP_ID)];
        events.push(mov_event(0, 1, sc::POSITION, 0.0, 0.0, 1.0));
        events.push(mov_event(600, 1, sc::POSITION, 15360.0, 21504.0, 1.0));
        let raw = raw_from(events);

        let auto = build_ei_replay_auto(&raw, &enc);
        assert_eq!(auto.map_id, Some(96));
        let meta = auto.meta.expect("mapID 96 has an arena image");
        assert_eq!(meta.sizes, [523, 750]);
        assert_eq!(meta.inch_to_pixel, 0.009);
        assert_eq!(meta.maps.len(), 1);
        assert_eq!(meta.maps[0].interval, [0, 1200]);
        // Same tracks the explicit-transform entry point produces.
        let explicit = build_ei_replay(&raw, &enc, &MapTransform::for_map_id(96).unwrap());
        assert_eq!(auto.tracks, explicit);
    }

    /// M15 Task 3: a map id GW2EI has no image for (Obsidian Sanctum, 899)
    /// still gets full tracks -- via the computed bounding box, exactly as
    /// GW2EI does -- but NO metadata, so the ei-json omits
    /// `combatReplayMetaData` while keeping `combatReplayData`.
    #[test]
    fn auto_replay_falls_back_to_the_bounding_box_without_metadata() {
        let enc = enc_with(vec![player(1, "Alice")], vec![], 1200);
        let raw = raw_from(vec![
            state_event(0, 899, sc::MAP_ID),
            mov_event(0, 1, sc::POSITION, 100.0, 100.0, 1.0),
            mov_event(600, 1, sc::POSITION, 900.0, 500.0, 1.0),
        ]);

        let auto = build_ei_replay_auto(&raw, &enc);
        assert_eq!(auto.map_id, Some(899));
        assert!(auto.meta.is_none(), "no arena image => no combatReplayMetaData");
        assert!(!auto.tracks[0].positions.is_empty(), "positions still come out, via the bounding box");
        for xy in &auto.tracks[0].positions {
            assert!(xy[0].is_finite() && xy[1].is_finite());
            assert!((0.0..=750.0).contains(&xy[0]) && (0.0..=750.0).contains(&xy[1]));
        }
    }

    /// M15 Task 3: no map image AND no SQUAD player to bound the box with
    /// (an enemy-only roster) -- there is nothing to compute a transform
    /// from, so the coordinates are dropped rather than emitted as the NaNs
    /// a zero-area rect would produce. Everything else about the track
    /// survives.
    ///
    /// Note how narrow this branch is: GW2EI's `forcePolling` gives every
    /// squad PLAYER a sentinel position even when the log holds no position
    /// event for them at all, so a single squad member is enough for
    /// `bounding_box_transform` to succeed (it centres the sentinel, see
    /// `auto_replay_falls_back_to_the_bounding_box_without_metadata`).
    #[test]
    fn auto_replay_drops_coordinates_when_no_transform_can_be_derived() {
        let enc = enc_with(
            vec![],
            vec![Enemy {
                id: 9, instid: 0, name: "Foe".into(), team: "blue".into(),
                is_player: true, marker: None,
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                agent_addrs: vec![9],
            }],
            1200,
        );
        let raw = raw_from(vec![
            state_event(0, 899, sc::MAP_ID),
            mov_event(0, 9, sc::POSITION, 100.0, 100.0, 1.0),
        ]);

        let auto = build_ei_replay_auto(&raw, &enc);
        assert!(auto.meta.is_none());
        assert_eq!(auto.tracks.len(), 1);
        assert!(auto.tracks[0].positions.is_empty(), "no transform => no coordinates, and no NaNs");
        assert!(auto.tracks[0].orientations.is_empty());
        assert!(!auto.tracks[0].dc.is_empty(), "the dc sentinel bracketing still applies");
    }

    #[test]
    fn end_to_end_track_is_in_map_pixels_on_the_ei_grid() {
        let enc = enc_with(vec![player(1, "Alice")], vec![], 1200);
        let raw = raw_from(vec![
            // NB: `z = 1.0`, because GW2EI drops an all-zero position
            // payload (`point.XYZ == default`) -- see `collect_movement`.
            mov_event(0, 1, sc::POSITION, 0.0, 0.0, 1.0),
            mov_event(0, 1, sc::FACING, 1.0, 0.0, 0.0),
            // Three quarters across the map rect on both axes. (Not the
            // rect CORNER: |xy| there is 52857, past GW2EI's 40000 validity
            // radius, so that payload would be dropped.)
            mov_event(600, 1, sc::POSITION, 15360.0, 21504.0, 1.0),
            mov_event(600, 1, sc::FACING, 0.0, 1.0, 0.0),
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks.len(), 1);
        let tr = &tracks[0];
        assert_eq!((tr.start, tr.end), (0, 600));
        // Grid points 0, 300, 600 (all within [0, 600], all < 1200).
        assert_eq!(tr.positions.len(), 3);
        assert_eq!(tr.orientations.len(), 3);
        assert_eq!(tr.positions[0], [261.5, 375.0], "world origin -> map center");
        assert_eq!(tr.positions[2], [392.25, 187.5], "3/4 across the rect -> 3/4 across the image");
        assert_eq!(tr.positions[1], [326.875, 281.25], "midpoint grid sample is linearly interpolated");
        assert_eq!(tr.orientations[0], 0.0);
        assert_eq!(tr.dc, vec![[i64::MIN, 0], [600, i64::MAX]]);
        assert!(tr.positions.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
    }

    /// Relog folding: both raw addrs' movement AND status events land on
    /// the same track, exactly like [`crate::analysis::replay::build_track`].
    #[test]
    fn folds_events_across_relog_addrs() {
        let mut p1 = player(1, "Alice");
        p1.agent_addrs = vec![1, 2];
        let enc = enc_with(vec![p1], vec![], 1200);
        let raw = raw_from(vec![
            mov_event(0, 1, sc::POSITION, 0.0, 0.0, 1.0),
            state_event(100, 1, sc::CHANGE_DOWN),
            state_event(200, 2, sc::CHANGE_UP),
            mov_event(600, 2, sc::POSITION, 600.0, 0.0, 1.0),
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].down, vec![[100, 200]]);
        assert_eq!(tracks[0].positions.len(), 3, "positions from both addrs on one grid");
    }

    #[test]
    fn only_enemy_players_get_tracks_not_npcs() {
        let enc = enc_with(
            vec![],
            vec![
                Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(),
                    is_player: true, marker: None,
                    profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                    agent_addrs: vec![9] },
                Enemy { id: 10, instid: 0, name: "Sentry".into(), team: "blue".into(),
                    is_player: false, marker: None, profession: None, elite_spec: None,
                    agent_addrs: vec![10] },
            ],
            900,
        );
        let raw = raw_from(vec![
            mov_event(0, 9, sc::POSITION, 1.0, 1.0, 0.0),
            mov_event(0, 10, sc::POSITION, 2.0, 2.0, 0.0),
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].agent_addr, 9);
        assert!(!tracks[0].is_squad);
    }

    /// GW2EI's `PositionEvent.AddPoint3D` validity filters: an all-zero
    /// point and an out-of-range one are both dropped, so they cannot drag
    /// the interpolation off the real track.
    #[test]
    fn invalid_position_payloads_are_dropped() {
        let enc = enc_with(vec![player(1, "Alice")], vec![], 900);
        let raw = raw_from(vec![
            mov_event(0, 1, sc::POSITION, 100.0, 100.0, 1.0),
            mov_event(300, 1, sc::POSITION, 0.0, 0.0, 0.0), // all-zero -> dropped
            mov_event(600, 1, sc::POSITION, 99_999.0, 0.0, 1.0), // |xy| > 40000 -> dropped
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        // Only the t=0 point survives, so every grid sample holds it.
        let first = tracks[0].positions[0];
        assert!(tracks[0].positions.iter().all(|q| *q == first), "{:?}", tracks[0].positions);
    }

    /// A SQUAD player with NO position events still gets a full-length track
    /// (GW2EI's `forcePolling` sentinel, since `AgentItem.Type ==
    /// AgentType.Player` for squad members -- `SingleActor.cs:415`) rather
    /// than being dropped -- and nothing NaNs out on the way through the map
    /// transform. This sentinel is load-bearing beyond just "not dropped":
    /// it also feeds `bounding_box_transform` (see
    /// `auto_replay_falls_back_to_the_bounding_box_without_metadata`), which
    /// only works because a squad player with zero raw positions still
    /// produces a (degenerate, off-map) polled point to bound.
    #[test]
    fn player_without_position_events_still_gets_a_track() {
        let enc = enc_with(vec![player(1, "Ghost")], vec![], 900);
        let raw = raw_from(vec![
            state_event(0, 1, sc::CHANGE_UP),
            state_event(800, 1, sc::CHANGE_DOWN),
        ]);
        // Pre-transform: the world track carries GW2EI's raw
        // `(int.MinValue, int.MinValue)` sentinel verbatim.
        let world = build_world_tracks(&raw, &enc);
        assert_eq!(world.len(), 1);
        assert!(world[0].is_squad);
        assert!(
            world[0].positions.iter().all(|q| q.x == i32::MIN as f32 && q.y == i32::MIN as f32),
            "{:?}",
            world[0].positions
        );

        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].positions.len(), 3, "grid 0, 300, 600 within [0, 800]");
        assert!(tracks[0].positions.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
        assert!(tracks[0].orientations.is_empty(), "no facing events -> no orientations");
    }

    /// REGRESSION (audit fix): an ENEMY player -- GW2EI's `NonSquadPlayer`
    /// (`WvWLogic.cs:327`), not `AgentType.Player` -- with zero raw position
    /// events must NOT get the `forcePolling` sentinel: `SingleActor.cs:415`
    /// passes `forcePolling = AgentItem.Type == AgentType.Player`, which is
    /// `false` for `NonSquadPlayer`. Facing events alone (no positions) must
    /// not synthesize a track either -- `poll` returns empty for both lists
    /// the moment the position list is empty and `force` is `false`. Pins
    /// the pre-fix bug (which passed `force = true` unconditionally for
    /// every roster entry, squad and enemy alike) as the explicit wrong
    /// answer: before this fix, this track would have carried 4 sentinel
    /// points at `(int.MinValue, int.MinValue)`.
    #[test]
    fn enemy_player_without_position_events_gets_no_sentinel_track() {
        let enc = enc_with(
            vec![],
            vec![Enemy {
                id: 9, instid: 0, name: "Foe".into(), team: "blue".into(),
                is_player: true, marker: None,
                profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()),
                agent_addrs: vec![9],
            }],
            900,
        );
        let raw = raw_from(vec![
            // Facing events present, but NO position events at all.
            mov_event(0, 9, sc::FACING, 1.0, 0.0, 0.0),
            mov_event(600, 9, sc::FACING, 0.0, 1.0, 0.0),
        ]);

        let world = build_world_tracks(&raw, &enc);
        assert_eq!(world.len(), 1);
        assert!(!world[0].is_squad);
        assert!(
            world[0].positions.is_empty(),
            "no forcePolling sentinel for a NonSquadPlayer enemy: {:?}",
            world[0].positions
        );
        assert!(
            world[0].rotations.is_empty(),
            "GW2EI's PollingRate returns before polling rotations too, once positions is empty and force is false"
        );

        // Same, through the full EI-JSON-shaped pipeline.
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].positions.is_empty());
        assert!(tracks[0].orientations.is_empty());
        assert!(
            !tracks[0]
                .positions
                .iter()
                .any(|q| q[0] < -1.0e9 || q[1] < -1.0e9 || q[0] > 1.0e9 || q[1] > 1.0e9),
            "no far-off-map sentinel coordinates should ever surface here"
        );
    }

    /// REGRESSION, end to end: a player who goes down at t=600 and is never
    /// revived must still be polled to their last-aware time, because
    /// `PlayerActor`'s CR-trim override counts down time.
    #[test]
    fn player_still_down_at_the_end_is_polled_to_last_aware() {
        let enc = enc_with(vec![player(1, "Alice")], vec![], 3000);
        let raw = raw_from(vec![
            mov_event(0, 1, sc::POSITION, 100.0, 100.0, 1.0),
            state_event(600, 1, sc::CHANGE_DOWN),
            mov_event(2400, 1, sc::POSITION, 200.0, 100.0, 1.0),
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!((tracks[0].start, tracks[0].end), (0, 2400));
        assert_eq!(tracks[0].positions.len(), 9, "grid 0..=2400 step 300");
        assert_eq!(tracks[0].down, vec![[600, 2400]]);
        assert_eq!(tracks[0].dc, vec![[i64::MIN, 0], [2400, i64::MAX]]);
    }

    /// REGRESSION: first/last-aware widen from the DESTINATION side too, so
    /// a player who is damaged before their own first event (and after
    /// their last) has a correspondingly wider `start`/`end`/`dc`.
    /// `sc::NONE` (statechange 0) is an ordinary combat event, which is in
    /// both `SrcIsAgent` and `DstIsAgent`.
    #[test]
    fn awareness_widens_from_destination_side_events() {
        let enc = enc_with(vec![player(2, "Victim")], vec![], 3000);
        let mut hit_early = blank();
        hit_early.time = 0;
        hit_early.src_agent = 1; // some attacker
        hit_early.dst_agent = 2; // our player, as the TARGET
        let mut hit_late = blank();
        hit_late.time = 2400;
        hit_late.src_agent = 1;
        hit_late.dst_agent = 2;

        let raw = raw_from(vec![
            hit_early,
            mov_event(900, 2, sc::POSITION, 100.0, 100.0, 1.0),
            hit_late,
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(
            (tracks[0].start, tracks[0].end),
            (0, 2400),
            "dst-side hits at 0 and 2400 must widen awareness beyond the lone src event at 900"
        );
        assert_eq!(tracks[0].dc, vec![[i64::MIN, 0], [2400, i64::MAX]]);
    }

    /// ...but only for statechanges whose `dst_agent` really IS an agent.
    /// A `CBTS_POSITION` event's `dst_agent` is packed x/y floats, so it
    /// must never be read as an agent address (here those packed bytes are
    /// chosen to collide with a real roster addr).
    #[test]
    fn packed_movement_payloads_are_not_mistaken_for_destination_agents() {
        assert!(!dst_is_agent(sc::POSITION));
        assert!(!dst_is_agent(sc::VELOCITY));
        assert!(!dst_is_agent(sc::FACING));
        // Non-agent-src statechanges must not widen awareness either: a
        // `CBTS_MAPID` event carries the map id in `src_agent`.
        assert!(!src_is_agent(sc::MAP_ID));
        assert!(!src_is_agent(sc::LOG_START));
        assert!(!src_is_agent(sc::WVW_TEAMS));
        // ...while the ones the engine actually consumes are all in.
        for scx in [sc::POSITION, sc::VELOCITY, sc::FACING, sc::CHANGE_UP, sc::CHANGE_DEAD,
                    sc::CHANGE_DOWN, SC_SPAWN, SC_DESPAWN] {
            assert!(src_is_agent(scx), "statechange {scx} must be src-agent");
        }

        // End to end: a MAP_ID event whose `src_agent` (95) collides with a
        // roster addr must not drag that player's first-aware to t=0.
        let enc = enc_with(vec![player(95, "Collide")], vec![], 3000);
        let mut map_id = blank();
        map_id.time = 0;
        map_id.src_agent = 95;
        map_id.is_statechange = sc::MAP_ID;
        let raw = raw_from(vec![map_id, mov_event(900, 95, sc::POSITION, 100.0, 100.0, 1.0)]);
        let map = MapTransform::for_map_id(96).unwrap();
        let tracks = build_ei_replay(&raw, &enc, &map);
        assert_eq!(tracks[0].start, 900, "the MAP_ID event's src_agent is a map id, not an agent");
    }

    #[test]
    fn build_ei_replay_is_deterministic() {
        let enc = enc_with(vec![player(1, "Alice"), player(2, "Bob")], vec![], 2400);
        let raw = raw_from(vec![
            mov_event(0, 1, sc::POSITION, 10.0, 10.0, 0.0),
            mov_event(0, 2, sc::POSITION, -10.0, 10.0, 0.0),
            mov_event(137, 1, sc::FACING, 1.0, 0.0, 0.0),
            mov_event(400, 1, sc::VELOCITY, 3.0, 0.0, 0.0),
            mov_event(900, 1, sc::POSITION, 900.0, 10.0, 0.0),
            mov_event(1500, 2, sc::POSITION, -900.0, 10.0, 0.0),
        ]);
        let map = MapTransform::for_map_id(96).unwrap();
        assert_eq!(build_ei_replay(&raw, &enc, &map), build_ei_replay(&raw, &enc, &map));
    }

    #[test]
    fn map_transform_from_log_reads_the_map_id_statechange() {
        let raw = raw_from(vec![RawEvent {
            time: 0,
            src_agent: 95,
            is_statechange: sc::MAP_ID,
            ..blank()
        }]);
        assert_eq!(MapTransform::from_log(&raw), MapTransform::for_map_id(95));
    }
}
