use super::{EvtcError, EVENT_SIZE_REV1};

pub mod sc {
    // is_statechange values (verified against arcdps cbtstatechange enum order)
    pub const NONE: u8 = 0;
    pub const ENTER_COMBAT: u8 = 1;
    pub const EXIT_COMBAT: u8 = 2;
    /// Agent is alive at time of event -- the counterpart to `CHANGE_DOWN`/
    /// `CHANGE_DEAD` that closes a down or (via respawn) a dead interval
    /// (M9 Task 1, `analysis::replay`). Verified against the arcdps EVTC
    /// reference by hand-counting `enum cbtstatechange` from `CBTS_COMBAT =
    /// 0`: `CBTS_CHANGEUP` is the 4th entry (index 3), immediately before
    /// `CBTS_CHANGEDEAD` (4, already used by this project as `CHANGE_DEAD`)
    /// and `CBTS_CHANGEDOWN` (5, `CHANGE_DOWN`) -- a strong internal
    /// cross-check, since those two ordinals were already independently
    /// verified. Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.ChangeUp = 3`
    /// (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs`). Payload, per the
    /// arcdps reference: `src_agent`: relates to agent.
    pub const CHANGE_UP: u8 = 3;
    pub const CHANGE_DEAD: u8 = 4;
    pub const CHANGE_DOWN: u8 = 5;
    pub const LOG_START: u8 = 9;
    pub const LOG_END: u8 = 10;
    /// Per-agent health-percentage change (M11 Task 1 -- health tracking +
    /// the arcdps-methodology contribution family's "over-99 anchor"). See
    /// `crate::analysis::health`'s module doc for the full ordinal + payload
    /// citation trail (curl'd `arcdps/evtc/README.txt`, 2026-08-09:
    /// `CBTS_HEALTHPCTUPDATE` is index 8, immediately after `CBTS_DESPAWN`
    /// (7) and before this project's already-independently-verified
    /// `LOG_START`/`CBTS_SQCOMBATSTART` (9); cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.HealthUpdate = 8`) and the percent-encoding
    /// derivation (`dst_agent` is percent * 100, per GW2EI's
    /// `HealthUpdateEvent.GetHealthPercent` and `EvtcParser`'s own
    /// event-filter comment -- NOT the reference text's literal, internally
    /// inconsistent "* 10000" prose).
    pub const HEALTH_UPDATE: u8 = 8;
    pub const MAX_HEALTH: u8 = 12;
    pub const POINT_OF_VIEW: u8 = 13;
    /// `CBTS_GWBUILD` -- the GW2 game build this log was recorded on, in
    /// `src_agent`. Decoded starting M16 Task 1 (damage modifiers gate their
    /// availability on a half-open GW2-build window; see
    /// `analysis::damage_mods::gw2_build`). Ordinal verified against GW2EI's
    /// `StateChange` enum (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs`:
    /// `PointOfView = 13, Language = 14, GWBuild = 15`) and its payload
    /// against `ParsedData/CombatEvents/MetaDataEvents/Version/
    /// GW2BuildEvent.cs:12-15` (`return evtcItem.SrcAgent`). GW2EI treats a
    /// zero build as absent (`EvtcParser.cs:881`).
    pub const GW2_BUILD: u8 = 15;
    pub const TEAM_CHANGE: u8 = 22;
    pub const MAP_ID: u8 = 25;
    /// Content-local-id -> stable-GUID association (Task 2b). Verified
    /// against the arcdps EVTC reference
    /// (deltaconnected.com/arcdps/evtc/README.txt): counting
    /// `enum cbtstatechange` entries from `CBTS_COMBAT = 0`,
    /// `CBTS_IDTOGUID` is the 47th entry (index 46). Cross-checked against
    /// GW2EI's `ArcDPSEnums.StateChange.IDToGUID = 46`.
    pub const ID_TO_GUID: u8 = 46;
    /// WvW team association (red/blue/green shard+team ids). Verified
    /// against the arcdps EVTC reference the same way: `CBTS_WVWTEAMS` is
    /// index 74 in `enum cbtstatechange`. Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.WvWTeams = 74`.
    pub const WVW_TEAMS: u8 = 74;
    /// Stun-break notification ("disable stopped early" per the arcdps
    /// reference comment). Verified against the arcdps EVTC reference the
    /// same way: `CBTS_STUNBREAK` is index 56 in `enum cbtstatechange`.
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.StunBreak =
    /// 56`. Payload (from the arcdps reference + GW2EI's `StunBreakEvent`,
    /// which reads `RemainingDuration = evtcItem.Value`): `src_agent` is
    /// the agent whose stun broke early; `value` is the remaining stun
    /// duration in ms that was cancelled by the break (0 if none is
    /// reported).
    pub const STUN_BREAK: u8 = 56;
    /// Missile creation (M10 Task 2). Verified against the arcdps EVTC
    /// reference (`curl https://www.deltaconnected.com/arcdps/evtc/README.txt`,
    /// 2026-08-09) by a full line-by-line hand-count of every `CBTS_*`
    /// enumerator in `enum cbtstatechange` from `CBTS_COMBAT = 0` (see this
    /// module's doc comment for the complete ordinal table): `CBTS_MISSILECREATE`
    /// is the 58th enumerator (index 57), immediately after `CBTS_STUNBREAK`
    /// (56, already independently verified above) and before
    /// `CBTS_MISSILELAUNCH` (58). Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.MissileCreate = 57`
    /// (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:317`).
    ///
    /// Payload, per the arcdps reference (`CBTS_MISSILECREATE` block):
    /// ```text
    /// CBTS_MISSILECREATE, // create a missile
    /// // src_agent: related to agent
    /// // value: (int16*)&value is int16[3], location x/y/z, divided by 10
    /// // overstack_value: skin id (player only)
    /// // skillid: missile skill id
    /// // pad61: (uint32_t*)&pad61 is uint32[1], trackable id
    /// ```
    /// `src_agent` is the missile's OWNER (cross-checked against GW2EI's
    /// `MissileEvent` ctor, which decodes `Skill = skillData.Get(evtcItem.SkillID)`
    /// and inherits `Src` from the base `StatusEvent(evtcItem, agentData)`
    /// ctor's `src_agent` read -- `ParsedData/CombatEvents/StatusEvents/MissileEvents/MissileEvent.cs`).
    /// `pad` (this project's `pad61` field) is the trackable id correlating
    /// CREATE/LAUNCH/REMOVE events for the same missile instance -- see
    /// `crate::analysis::missiles`.
    pub const MISSILE_CREATE: u8 = 57;
    /// Missile launch/relaunch (M10 Task 2). Verified the same way:
    /// `CBTS_MISSILELAUNCH` is the 59th enumerator (index 58), immediately
    /// after `CBTS_MISSILECREATE` (57) and before `CBTS_MISSILEREMOVE` (59).
    /// Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.MissileLaunch = 58` (`ArcDPSEnums.cs:318`).
    ///
    /// Payload, per the arcdps reference (`CBTS_MISSILELAUNCH` block):
    /// ```text
    /// CBTS_MISSILELAUNCH, // launch missile
    /// // src_agent: related to agent
    /// // dst_agent: at agent, if set and in range
    /// // value: (int16*)&value is int16[6], target x/y/z, current x/y/z, divided by 10
    /// // skillid: missile skill id
    /// // iff: (uint8_t*)&iff is uint8_t[1], launch motion. unknown, from client
    /// // result: (int16_t*)&result is int16[1], motion radius
    /// // is_buffremove: (uint32_t*)&is_buffremove is uint32_t[1], launch flags. unknown, from client
    /// // is_src_flanking: non-zero if first launch
    /// // is_shields: (int16_t*)&is_shields is int16[1], missile speed
    /// // pad61: (uint32_t*)&pad61 is uint32[1], trackable id
    /// ```
    /// `dst_agent` is the launch's TARGET agent (only meaningful "if set and
    /// in range" per the reference -- `0` otherwise). `is_src_flanking`
    /// (this project's `is_flanking`, offset 57) is GW2EI's
    /// `IsFirstLaunch = evtcItem.IsFlanking > 0`
    /// (`MissileLaunchEvent.cs`) -- a missile trackable id with MORE than
    /// one launch event is a relaunch (bounce/reflect), and GW2EI derives a
    /// `MaybeReflected` heuristic from it: `Missile.Src.Is(TargetedAgent) &&
    /// !IsFirstLaunch` (the relaunch's target is the ORIGINAL owner, i.e.
    /// the missile is heading back at its own caster). This is explicitly a
    /// heuristic (the "Maybe" prefix is GW2EI's own naming) -- neither the
    /// arcdps reference nor GW2EI decode WHO caused the relaunch; `iff`
    /// ("launch motion") and `is_buffremove` ("launch flags") are both
    /// documented by arcdps itself as "unknown, from client". See
    /// `crate::analysis::missiles`'s module doc for what this project does
    /// and does NOT attribute from this payload.
    pub const MISSILE_LAUNCH: u8 = 58;
    /// Missile removal -- the terminal event for a missile instance (M10
    /// Task 2). Verified the same way: `CBTS_MISSILEREMOVE` is the 60th
    /// enumerator (index 59), immediately after `CBTS_MISSILELAUNCH` (58)
    /// and before `CBTS_EFFECTGROUNDCREATE` (60). Cross-checked against
    /// GW2EI's `ArcDPSEnums.StateChange.MissileRemove = 59`
    /// (`ArcDPSEnums.cs:319`).
    ///
    /// Payload, per the arcdps reference (`CBTS_MISSILEREMOVE` block):
    /// ```text
    /// CBTS_MISSILEREMOVE, // remove missile
    /// // src_agent: related to agent
    /// // value: friendly fire damage total
    /// // skillid: missile skill id
    /// // buff_dmg: (int16*)&value is int16[3], location x/y/z, divided by 10
    /// // is_src_flanking: hit at least one enemy along the way
    /// // pad61: (uint32_t*)&pad61 is uint32[1], trackable id
    /// ```
    /// CRITICAL FINDING (documented precisely so no caller assumes more than
    /// this payload actually supports): there is NO reason code on this
    /// event distinguishing "blocked" vs "reflected" vs "destroyed" vs
    /// "expired naturally" -- the arcdps reference's own comment text for
    /// `is_src_flanking` is the ONLY outcome signal ("hit at least one enemy
    /// along the way"), a plain hit/no-hit boolean. GW2EI's
    /// `MissileRemoveEvent` ctor confirms this: it decodes exactly `DidHit =
    /// evtcItem.IsFlanking > 0`, `FriendlyFireTotalDamage = evtcItem.Value`,
    /// and an optional `DamagingAgent` from `src_agent` (only set when
    /// nonzero) -- no reason/cause enum anywhere
    /// (`ParsedData/CombatEvents/StatusEvents/MissileEvents/MissileRemoveEvent.cs`).
    /// `DamagingAgent` is registered by GW2EI's `CombatEventFactory` into a
    /// `MissileDamagingEventsBySrc` map ONLY when `DidHit` is true
    /// (`CombatEventFactory.cs:483-486`) -- i.e. GW2EI's own model treats
    /// `src_agent` here as identifying who the missile damaged on a
    /// SUCCESSFUL hit, not who denied it on a miss; that map has no other
    /// callers anywhere in GW2EI's codebase (dead/speculative API surface).
    /// There is consequently no wire-level "denier" field either. See
    /// `crate::analysis::missiles`'s module doc for the resulting scope
    /// decision (fired/hit/not-hit and the reflected-heuristic only -- no
    /// blocked/reflected/destroyed breakdown).
    pub const MISSILE_REMOVE: u8 = 59;
    /// Apply a visual effect to an in-flight missile (M10 Task 2) --
    /// distinct from `CBTS_EFFECTAGENTCREATE`/`CBTS_EFFECTGROUNDCREATE`
    /// (ordinary agent/ground VFX) and not decoded by this project (no
    /// analytic use for a purely visual effect-on-missile event). Verified
    /// by the SAME full hand-count as the other missile ordinals above:
    /// `CBTS_MISSILEEFFECT` is the 81st enumerator (index 79), well after
    /// `CBTS_GADGETNAME` (78) and before `CBTS_GADGETCAPTUREOUTLINESHOW`
    /// (80). Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.EffectMissileCreate = 79`
    /// (`ArcDPSEnums.cs:339`) -- GW2EI names this enumerator differently
    /// (`EffectMissileCreate` vs the reference text's `CBTS_MISSILEEFFECT`)
    /// but the ordinal (79) and the reference's own comment ("apply effect
    /// to missile") agree exactly. Payload, per the arcdps reference:
    /// `dst_agent`: owner of missile; `skillid`: effect id; `value`:
    /// duration; `pad61`: trackable id. Kept for completeness (the ordinal
    /// count table below requires it) but not consumed anywhere in this
    /// project's analysis -- purely cosmetic VFX, no analytic content.
    pub const MISSILE_EFFECT: u8 = 79;
    /// Above-target squad marker assignment/removal on an agent (Task 7,
    /// M2 -- arcdps-dev guidance items 4/5). Verified against the arcdps
    /// EVTC reference by hand-counting `enum cbtstatechange` from
    /// `CBTS_COMBAT = 0`: `CBTS_MARKER` is index 37. Cross-checked against
    /// GW2EI's `ArcDPSEnums.StateChange.Marker = 37`.
    ///
    /// Payload, per the arcdps EVTC reference:
    /// ```text
    /// CBTS_MARKER, // one event per marker on an agent
    /// // src_agent: relates to agent
    /// // value: markerdef id. if value is 0, remove all markers presently on agent
    /// // buff: marker is a commander tag
    /// ```
    /// `value` is a content-local id (`n_contentlocal` MARKER=1), resolved
    /// to a stable GUID via `CBTS_IDTOGUID` -- see `crate::wvw::markers`.
    /// `buff == 1` flags the marker as a commander tag (cross-checked
    /// against the real WvW fixture: the two commander-tag-GUID marker
    /// events there both carry `buff == 1`; GW2EI's `MarkerGUIDEvent`
    /// independently corroborates this by checking GUID membership in its
    /// own `MarkerGUIDs.CommanderTagMarkersHexGUIDs` set, which matches the
    /// same events).
    ///
    /// Distinct from `CBTS_SQUADMARKER_GROUND` (index 53, "squad ground
    /// markers" -- a different, position-based ground-placement marker
    /// system keyed by a fixed `skillid` index, not a GUID). Out of scope
    /// for this task, which covers only above-target/agent markers per the
    /// arcdps-dev guidance.
    pub const MARKER: u8 = 37;
    /// Server tick-rate telemetry (Task 7, M2 -- arcdps-dev guidance item
    /// 7). Verified against the arcdps EVTC reference by hand-counting
    /// `enum cbtstatechange` from `CBTS_COMBAT = 0`: `CBTS_TICK` is index
    /// 84. Cross-checked against GW2EI's `ArcDPSEnums.StateChange.Tick =
    /// 84`.
    ///
    /// Payload, per the arcdps EVTC reference:
    /// ```text
    /// CBTS_TICK, // tick, every 25 ticks
    /// // src_agent: current extrapolated tick (ticks may go backwards if real update is lower than extrapolation)
    /// // dst_agent: ticks since last real tick update
    /// ```
    /// See `crate::wvw::markers::resolve_tick_rate` for how the ticks/sec
    /// rate is derived from this payload (the extrapolated-tick-counter
    /// delta between consecutive events, divided by real elapsed time --
    /// deliberately not relying on the unverified "every 25 ticks" cadence
    /// claim, or on `dst_agent`, whose exact semantics beyond the one-line
    /// comment above aren't independently corroborated anywhere we could
    /// find in GW2EI).
    pub const TICK: u8 = 84;
    /// Pre-existing-stack buff application, for stacks that were already on
    /// an agent at the moment the log started recording (M3, Task 1).
    /// Verified against the arcdps EVTC reference by hand-counting `enum
    /// cbtstatechange` from `CBTS_COMBAT = 0`: `CBTS_BUFFINITIAL` is index
    /// 18. Cross-checked against GW2EI's `ArcDPSEnums.StateChange.BuffInitial
    /// = 18` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs`).
    ///
    /// IMPORTANT version note (verified against both sources): the arcdps
    /// reference fetched live from deltaconnected.com today additionally
    /// documents `CBTS_BUFFAPPLY`/`CBTS_BUFFCHANGE`/`CBTS_BUFFREMOVE_SINGLE`/
    /// `CBTS_BUFFREMOVE_ALL` as their OWN dedicated `is_statechange` values
    /// (69-72) -- but that is the *current* (2026-05+) arcdps wire format.
    /// GW2EI's own `CombatItem.IsBuffApplyEvent`/`IsBuffRemoveEvent`
    /// (`GW2EIEvtcParser/CombatItem.cs`) gate on
    /// `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges = 20260501` (the
    /// SAME build as `ArcDPSBuilds.ResultEnumRework`, already documented on
    /// `result::CROWD_CONTROL` above): only builds `>= 20260501` use that
    /// dedicated-statechange shape. This project's golden/calibration
    /// fixture is build 20260114 -- BEFORE that threshold -- so apply/remove
    /// events there use the OLDER shape this module already implements:
    /// ordinary `is_statechange == 0` combat events, apply flagged by
    /// `buff == 1` (see `sc::COMBAT`/`decode_events` struct layout) and
    /// removal flagged by `is_buffremove != 0` (see `buff_remove` module).
    /// `CBTS_BUFFINITIAL` itself is NOT affected by this split -- it is
    /// ordinal 18 in both eras (confirmed by the same hand-count against
    /// both the live reference and `ArcDPSEnums.cs`), so `analysis::buffs`
    /// treats `is_statechange == BUFF_INITIAL` as an apply event regardless
    /// of build era.
    pub const BUFF_INITIAL: u8 = 18;
    /// Per-buff metadata arcdps emits once per tracked skill id in every
    /// log (M3 Task 2). Verified against the arcdps EVTC reference
    /// (hand-counted ordinal 30 from `CBTS_COMBAT = 0`) and cross-checked
    /// against GW2EI's `ArcDPSEnums.StateChange.BuffInfo = 30`. Payload,
    /// per the arcdps reference: `overstack_value: max combined duration`,
    /// `skillid: skilldef id of buff`, `src_master_instid: stacking
    /// limit`, `is_offcycle: category`, `pad61: buff stacking type`.
    /// **Load-bearing**: GW2EI's `Buff.CreateSimulator`
    /// (`GW2EIEvtcParser/EIData/Buffs/Buff.cs`) uses `src_master_instid`
    /// (its `BuffInfoEvent.MaxStacks`) as the simulator's REAL capacity
    /// whenever it's present, `> 0`, and different from GW2EI's own
    /// hardcoded `CommonBuffs` table default -- i.e. arcdps's own
    /// per-build-reported stack cap OVERRIDES the hardcoded guess. This
    /// project's fixture reports `src_master_instid == 99` for most
    /// Queue-type boons (Fury, Quickness, Alacrity, Protection, Vigor,
    /// Resistance, Resolution, Swiftness) -- far above the hardcoded
    /// 5-9 `simulator::capacity_for` previously assumed -- see
    /// `analysis::buffs::events::extract_buff_capacities`.
    pub const BUFF_INFO: u8 = 30;
    /// `CBTS_STACKACTIVE` -- "a buff stack became the active one".
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.StackActive
    /// = 27` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:287`). GW2EI's
    /// `BuffStackActiveEvent` reads its stack instance id from `DstAgent`
    /// (NOT `pad`, unlike every other buff event -- `BuffStacks/
    /// BuffStackActiveEvent.cs:10`).
    ///
    /// This project does not simulate activation (`BuffStackActiveEvent.
    /// IsBuffSimulatorCompliant` is `false` in the NoID family for
    /// everything except Regeneration, whose `HealingLogic` is a deferred
    /// MBUFFSIM follow-up). It IS consumed for two things:
    /// `CombatData.HasStackIDs` (`ParsedData/CombatData.cs:610`, the gate on
    /// the `StackingConditionalLoss` band aid) and that band aid's own
    /// `totalDuration` reconstruction (`EIData/Buffs/BuffsContainer.cs:230-234`).
    pub const STACK_ACTIVE: u8 = 27;
    /// `CBTS_STACKRESET`, GW2EI's `StackDeactive = 28`
    /// (`ArcDPSEnums.cs:288`, "Formerly as StackReset"). Instance id in
    /// `pad`, reset-to duration in `value`
    /// (`BuffStacks/BuffStackDeactiveEvent.cs:8-12`). Consumed only by
    /// `CombatData.HasStackIDs` here -- see [`STACK_ACTIVE`].
    pub const STACK_DEACTIVE: u8 = 28;
    /// Cast-animation START statechange (M4 Task 2, `support::apply`'s
    /// resurrect-cast detection). Verified against the live arcdps EVTC
    /// reference (`curl https://www.deltaconnected.com/arcdps/evtc/README.txt`,
    /// 2026-08-08): hand-counting `enum cbtstatechange` entries from
    /// `CBTS_COMBAT = 0`, `CBTS_ANIMATIONSTART` is the 68th entry (index
    /// 67), immediately followed by `CBTS_ANIMATIONSTOP` (68) then
    /// `CBTS_BUFFAPPLY` (69, already independently hand-counted in Task 1) --
    /// a second, independent confirmation of this ordinal's position.
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.AnimationStart
    /// = 67` (`ArcDPSEnums.cs:327`).
    ///
    /// **IMPORTANT version-threshold finding (M4 Task 2)**: this statechange
    /// is gated by a DIFFERENT, EARLIER GW2EI build threshold than the
    /// `BuffAppliesAndRemovesAsStateChanges`/`ResultEnumRework` pair this
    /// project's `RawHeader::is_post_buff_rework` (`20260501`) already
    /// checks -- `ArcDPSBuilds.AnimationAsStateChanges = 20260430`
    /// (`ArcDPSEnums.cs:38`). `CombatItem.IsStartCastEvent()`
    /// (`GW2EIEvtcParser/CombatItem.cs:404-415`) gates on THAT threshold:
    /// `if (_version.Build >= ArcDPSBuilds.AnimationAsStateChanges) { return
    /// IsStateChange == StateChange.AnimationStart; }` -- else falls back to
    /// the older `IsActivation == Normal || IsActivation == Quickness` shape
    /// on an ordinary `is_statechange == 0` combat event (the ONLY shape
    /// this project's pre-M4 `support::ACTIVATION_START`-based resurrect
    /// scan implements). Since `20260430 < 20260501`, every log this
    /// project classifies `is_post_buff_rework() == true` is ALSO on/after
    /// `AnimationAsStateChanges` -- so decoding cast-start events via
    /// `is_activation` (the pre-era shape) on such a log would silently miss
    /// every skill-cast-start row, INCLUDING resurrects, which now arrive as
    /// this dedicated statechange instead. This project has no separate
    /// `AnimationAsStateChanges` header flag (only the single
    /// `is_post_buff_rework` threshold, `20260501`), so `support::apply`
    /// reuses that same flag to gate resurrect-cast decoding -- safe/
    /// conservative because it never UNDER-shoots the real, earlier
    /// threshold (a log built in the `[20260430, 20260501)` window, which
    /// this project would still classify pre-era, is the one known gap this
    /// single-threshold design leaves; out of scope to fix without adding a
    /// second header field this project doesn't otherwise need).
    ///
    /// Payload, per the arcdps reference (`CBTS_ANIMATIONSTART` block):
    /// `src_agent`: agent beginning animation; `dst_agent`: target agent if
    /// applicable; `value`: ms duration until minimum of last significant
    /// trigger point and tooltip time; `buff_dmg`: ms duration when control
    /// is returned to agent; `overstack_value`: reference data (CSK enum);
    /// `skillid`: skill id. Field roles verified against GW2EI's
    /// `AnimatedCastEvent`/`CombatData.CreateCastEvents`
    /// (`ParsedData/CombatEvents/CastEvents/AnimatedCastEvent.cs`,
    /// `ParsedData/CombatData.cs:538-562`): cast events are grouped by the
    /// raw, UNRESOLVED `combatItem.SrcAgent` (`castCombatEvents.AddToList(
    /// combatItem.SrcAgent, combatItem)`) -- the SAME field role as the
    /// pre-era shape's `src_agent` (the caster), so `support::apply`'s
    /// resurrect credit (by raw `src_agent`, no master/pet resolution) needs
    /// no change beyond swapping which `is_statechange`/predicate selects
    /// the row. `ExpectedDuration = startItem.BuffDmg > 0 ? startItem.
    /// BuffDmg : startItem.Value` (`AnimatedCastEvent.cs:60`) is not
    /// consumed here (this project counts start-cast rows directly, per
    /// `SupportMetrics::resurrects`'s doc comment, not full duration
    /// pairing) -- included for completeness only.
    ///
    /// Distinct from `CBTS_ANIMATIONSTOP` (ordinal 68, the paired end-cast
    /// statechange) -- not decoded by this project, mirroring the pre-era
    /// resurrect scan's existing simplification of counting only start-cast
    /// rows (see `SupportMetrics::resurrects`'s doc comment for why this is
    /// count-equivalent on this project's calibration fixture).
    pub const ANIMATION_START: u8 = 67;
    /// Cast-animation STOP statechange (M14 Task 1, `analysis::rotation`) --
    /// the post-era pair to `ANIMATION_START` above, ordinal 68 (immediately
    /// after `ANIMATION_START`=67, immediately before `BUFF_APPLY`=69 --
    /// hand-counted the same way from the live arcdps EVTC reference,
    /// `curl https://www.deltaconnected.com/arcdps/evtc/README.txt`,
    /// 2026-08-09: `CBTS_ANIMATIONSTOP` is the 69th entry (index 68)).
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.AnimationStop
    /// = 68` (`ArcDPSEnums.cs:328`). Gated by the SAME
    /// `ArcDPSBuilds.AnimationAsStateChanges = 20260430` threshold as
    /// `ANIMATION_START` (subsumed by this project's single
    /// `is_post_buff_rework` `20260501` gate) -- `CombatItem.
    /// IsEndCastEvent()` (`GW2EIEvtcParser/CombatItem.cs:417-428`): `if
    /// (_version.Build >= ArcDPSBuilds.AnimationAsStateChanges) { return
    /// IsStateChange == StateChange.AnimationStop; }` -- else falls back to
    /// the pre-era shape: an ordinary `is_statechange == 0` combat event
    /// with `is_activation` one of `Minimum`(3)/`Cancel`(4)/`Reset`(5)/
    /// `NoData`(6) (`crate::analysis::rotation`'s era split mirrors this
    /// exactly).
    ///
    /// Payload, per the arcdps reference (`CBTS_ANIMATIONSTOP` block):
    /// `src_agent`: agent beginning animation; `value`: ms duration spent
    /// in animation SCALED for speed (i.e. the real wall-clock duration
    /// after quickness/slows); `buff_dmg`: ms duration spent in animation
    /// NOT scaled (the nominal/unaccelerated duration); `skillid`: skill id
    /// of the previous animation start; `is_activation`: "simple progress
    /// check from cbtanimation" -- per the arcdps reference this is the
    /// SAME `cbtanimation` byte the pre-era shape's END rows carry
    /// (`Minimum`/`Cancel`/`Reset`/`NoData`), confirmed load-bearing by
    /// GW2EI's `AnimatedCastEvent.SetAcceleration` (`ParsedData/
    /// CombatEvents/CastEvents/AnimatedCastEvent.cs:14-52`), which switches
    /// on `endItem.IsActivation` identically regardless of which era
    /// produced the end row -- i.e. `is_activation` is NOT overloaded away
    /// on this statechange the way it is on `BUFF_APPLY`/etc; it keeps its
    /// ordinary `cbtanimation` meaning here. `value`/`buff_dmg`'s roles
    /// (scaled vs unscaled) are what `AnimatedCastEvent`'s ctor reads as
    /// `ActualDuration = endItem.Value` / `_scaledActualDuration =
    /// endItem.BuffDmg` -- see `analysis::rotation`'s module doc for the
    /// full quickness/`TimeGained` derivation these two fields feed.
    pub const ANIMATION_STOP: u8 = 68;
    /// Post-rework (arcdps build >= 20260501, GW2EI's
    /// `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges`) dedicated buff
    /// STACK APPLICATION statechange -- see `BUFF_INITIAL`'s doc comment
    /// for the full version-split background. Verified against the live
    /// arcdps EVTC reference (`curl
    /// https://www.deltaconnected.com/arcdps/evtc/README.txt`,
    /// 2026-08-08): hand-counting `enum cbtstatechange` entries from
    /// `CBTS_COMBAT = 0`, `CBTS_BUFFAPPLY` is the 70th entry (index 69).
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.BuffApply =
    /// 69` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:329`).
    ///
    /// Payload, per the arcdps reference (`README.txt`, `CBTS_BUFFAPPLY`
    /// block): `src_agent`: agent applying the stack; `dst_agent`: agent
    /// the stack was applied to; `value`: ms duration applied; `skillid`:
    /// buff skill id; `is_shields`: non-zero if buff is active when
    /// applied; `pad61`: trackable id (per-stack instance id -- NOT
    /// decoded by this project, same simplification already documented on
    /// `RawEvent::is_offcycle`/the pre-era `Extend` doc comment for
    /// `pad61`/instance-id fields).
    ///
    /// Field roles verified identical to the pre-era `buff == 1` apply
    /// shape (owner = `dst_agent`, applier = `src_agent`): GW2EI's
    /// `AbstractBuffApplyEvent(CombatItem evtcItem, ...)` ctor
    /// (`ParsedData/CombatEvents/BuffEvents/BuffApplies/
    /// AbstractBuffApplyEvent.cs`) reads `By = SrcAgent`, `To = DstAgent`
    /// unconditionally from the raw event -- this is the SAME constructor
    /// used for both eras (`BuffApplyEvent`/`BuffExtensionEvent` both
    /// derive from it), so the src/dst roles do NOT flip post-rework.
    /// `is_shields` likewise round-trips through the same shared
    /// `BuffApplyEvent` ctor (`_addedActive = evtcItem.IsShields > 0`),
    /// unaffected by era.
    ///
    /// Dispatch: GW2EI's `CombatEventFactory.AddBuffApplyEvent`'s
    /// post-`BuffAppliesAndRemovesAsStateChanges` branch
    /// (`CombatEventFactory.cs:645-654`) routes every apply-shaped
    /// statechange (`BuffApply` OR `BuffInitial`) EXCEPT `BuffChange`
    /// (see `BUFF_CHANGE` below) to a plain `BuffApplyEvent` -- i.e.
    /// unlike the pre-era shape, `is_offcycle` is NOT consulted at all for
    /// post-era apply routing (the dedicated `BuffChange` statechange
    /// replaces that flag).
    pub const BUFF_APPLY: u8 = 69;
    /// Post-rework dedicated buff STACK EXTENSION statechange (active-stack
    /// duration change) -- the post-era equivalent of the pre-era
    /// `is_offcycle != 0` apply-shaped event (see
    /// `analysis::buffs::events::BuffEventKind::Extend`'s doc comment).
    /// Verified against the arcdps EVTC reference: `CBTS_BUFFCHANGE` is the
    /// 71st entry (index 70) counting from `CBTS_COMBAT = 0`. Cross-checked
    /// against GW2EI's `ArcDPSEnums.StateChange.BuffChange = 70 //
    /// Extension` (`ArcDPSEnums.cs:330`).
    ///
    /// Payload, per the arcdps reference (`CBTS_BUFFCHANGE` block):
    /// `dst_agent`: relates to agent (the buff owner); `value`: duration
    /// difference; `overstack_value`: new ms duration; `skillid`: buff
    /// skill id; `pad61`: trackable id. NOTE: the live reference text does
    /// NOT list `src_agent` in this block (unlike `CBTS_BUFFAPPLY`'s
    /// explicit listing) -- but GW2EI's `BuffExtensionEvent` uses the SAME
    /// `AbstractBuffApplyEvent(CombatItem evtcItem, ...)` ctor as
    /// `BuffApplyEvent` (`ParsedData/CombatEvents/BuffEvents/BuffApplies/
    /// BuffExtensionEvent.cs`, `AbstractBuffApplyEvent.cs`), which
    /// unconditionally reads `By = SrcAgent`. Per this module's own
    /// verification policy (GW2EI is the arbiter where the arcdps reference
    /// is ambiguous/incomplete), `src_agent` IS the applier here too --
    /// treated as a doc omission in the arcdps reference, not a real
    /// absence of the field on the wire.
    ///
    /// `extended_ms` = raw event's `value` field (GW2EI:
    /// `ExtendedDuration = Math.Max(evtcItem.Value, 0)` -- same clamp as
    /// the pre-era `Extend` decode already applies), `new_duration_ms` =
    /// raw event's `overstack` field (GW2EI: `NewDuration =
    /// evtcItem.OverstackValue`) -- both fields map onto the SAME
    /// `BuffEventKind::Extend { extended_ms, new_duration_ms }` shape the
    /// pre-era decode already produces.
    pub const BUFF_CHANGE: u8 = 70;
    /// Post-rework dedicated SINGLE buff-stack removal statechange.
    /// Verified against the arcdps EVTC reference: `CBTS_BUFFREMOVE_SINGLE`
    /// is the 72nd entry (index 71) counting from `CBTS_COMBAT = 0`.
    /// Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.BuffRemoveSingle = 71 // Single or Manual`
    /// (`ArcDPSEnums.cs:331`) -- note the GW2EI comment itself flags that
    /// this ordinal carries BOTH removal kinds, confirmed by
    /// `CombatEventFactory.AddBuffRemoveEvent`'s post-era branch
    /// (`CombatEventFactory.cs:675-693`): unlike `BuffRemoveAll` (its own
    /// dedicated statechange, see below), Single vs Manual on THIS
    /// statechange is disambiguated by the existing `is_buffremove` byte
    /// (`buff_remove::SINGLE` / `buff_remove::MANUAL`) exactly as the
    /// pre-era shape already does -- only `Single` is simulator-compliant
    /// and extracted (Manual is skipped, mirroring `buff_remove::MANUAL`'s
    /// doc comment); unlike the pre-era shape, post-era removal dispatch
    /// does NOT additionally require `is_activation == 0` (GW2EI's
    /// post-era `IsBuffRemoveEvent()` checks only the statechange kind).
    ///
    /// Payload, per the arcdps reference (`CBTS_BUFFREMOVE_SINGLE` block):
    /// `src_agent`: agent with buff removed (owner); `dst_agent`: agent
    /// removing the buff (remover); `value`: ms duration removed;
    /// `skillid`: buff skill id; `is_buffremove`: of enum cbtbuffremove;
    /// `pad61`: trackable id. Field roles verified identical to the
    /// pre-era SINGLE removal shape (owner = `src_agent`, remover =
    /// `dst_agent` -- the OPPOSITE of apply): GW2EI's
    /// `AbstractBuffRemoveEvent(CombatItem evtcItem, ...)` ctor
    /// (`ParsedData/CombatEvents/BuffEvents/BuffRemoves/
    /// AbstractBuffRemoveEvent.cs`) reads `By = DstAgent`, `To = SrcAgent`
    /// unconditionally -- the SAME constructor shared by
    /// `BuffRemoveSingleEvent`/`BuffRemoveManualEvent` in both eras.
    /// `removed_duration_ms` = raw event's `value` field (`RemovedDuration
    /// = evtcItem.Value`), same as pre-era.
    pub const BUFF_REMOVE_SINGLE: u8 = 71;
    /// Post-rework dedicated ALL buff-stacks-of-skillid removal
    /// statechange. Verified against the arcdps EVTC reference:
    /// `CBTS_BUFFREMOVE_ALL` is the 73rd entry (index 72) counting from
    /// `CBTS_COMBAT = 0`. Cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.BuffRemoveAll = 72` (`ArcDPSEnums.cs:332`).
    ///
    /// Payload, per the arcdps reference (`CBTS_BUFFREMOVE_ALL` block):
    /// `src_agent`: agent with buffs removed (owner); `dst_agent`: agent
    /// removing the buffs (remover); `value`: ms duration removed
    /// (duration calc); `buff_dmg`: ms duration removed (intensity calc);
    /// `skillid`: buff skill id; `is_buffremove`: of enum cbtbuffremove.
    /// Dispatch verified against `CombatEventFactory.AddBuffRemoveEvent`'s
    /// post-era branch: a `BuffRemoveAll`-statechange row unconditionally
    /// becomes a `BuffRemoveAllEvent` -- `is_buffremove`'s value is NOT
    /// consulted for this statechange (unlike `BUFF_REMOVE_SINGLE` above).
    /// Field roles (owner = `src_agent`, remover = `dst_agent`) verified
    /// identical to pre-era ALL removal via the same shared
    /// `AbstractBuffRemoveEvent` ctor as `BUFF_REMOVE_SINGLE`. This
    /// project's simplified `BuffEventKind::RemoveAll` carries no duration
    /// fields (same as the pre-era decode), so `value`/`buff_dmg` are not
    /// extracted here either.
    pub const BUFF_REMOVE_ALL: u8 = 72;
    /// Agent position changed (M9 Task 1, `analysis::replay`). Verified
    /// against the live arcdps EVTC reference (`curl
    /// https://www.deltaconnected.com/arcdps/evtc/README.txt`, 2026-08-08):
    /// hand-counting `enum cbtstatechange` entries from `CBTS_COMBAT = 0`,
    /// `CBTS_POSITION` is the 20th entry (index 19). Cross-checked against
    /// GW2EI's `ArcDPSEnums.StateChange.Position = 19`
    /// (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:279`). Independently
    /// corroborated by event-count fingerprint on this project's real
    /// post-rework local fixture (`fixtures/local/wvw-postrework.zevtc`):
    /// statechange 19/20/21 carry 61336/57333/50182 events respectively
    /// (position updates most frequent, facing least -- movement-shaped
    /// counts, not a coincidence), matching `POSITION`/`VELOCITY`/`FACING`
    /// in that order.
    ///
    /// Payload, per the arcdps reference (`CBTS_POSITION` block):
    /// `src_agent`: relates to agent; `dst_agent`: "`(float*)&dst_agent` is
    /// `float[3]`, x/y/z" -- i.e. `dst_agent`'s 8 raw bytes hold TWO
    /// packed f32s (x at bytes 0-3, y at bytes 4-7), and the THIRD float
    /// (z) does NOT fit in `dst_agent` alone; GW2EI's `MovementEvent` (the
    /// shared base for `PositionEvent`/`VelocityEvent`/`RotationEvent`,
    /// `GW2EIEvtcParser/ParsedData/CombatEvents/StatusEvents/MovementEvents/
    /// MovementEvent.cs`) is the arbiter for where z lives, since the
    /// arcdps reference text doesn't spell it out: `PackMovementData(x, y,
    /// z)` packs `x`/`y` into a `ulong` (`dst_agent`) via
    /// `BitConverter.ToUInt64([...xBytes, ...yBytes])` (little-endian x
    /// then y) and z separately via `BitConverter.SingleToInt32Bits(z)`
    /// into the SAME event's `value` (i32) field --
    /// `UnpackMovementData(ulong packedXY, int intZ)` reverses this:
    /// `x = *(float*)&packedXY`, `y = *((float*)&packedXY + 1)`, `z =
    /// *(float*)&intZ`. So: `x = f32::from_le_bytes(dst_agent[0..4])`,
    /// `y = f32::from_le_bytes(dst_agent[4..8])` (both from the SAME
    /// little-endian `dst_agent` u64, low/high halves), `z =
    /// f32::from_bits(value as u32)` (bit-reinterpret, not a numeric
    /// conversion). Confirmed by decode probe against the real post-rework
    /// fixture: consecutive squad members' positions cluster within a few
    /// hundred units of each other and z sits in the plausible in-game
    /// height range (~-2900..-2700 for the sampled cluster) -- a wrong
    /// offset (e.g. reading z from `overstack` or failing to bit-reinterpret)
    /// produces nonsense (huge/NaN/garbage) values instead, which the unit
    /// tests below assert against.
    pub const POSITION: u8 = 19;
    /// Agent velocity changed -- same packed-float payload shape as
    /// `POSITION` (M9 Task 1), one ordinal later. Verified the same way:
    /// `CBTS_VELOCITY` is the 21st entry (index 20). Cross-checked against
    /// GW2EI's `ArcDPSEnums.StateChange.Velocity = 20`. Not decoded into
    /// `analysis::replay::build_replay`'s output (position + down/dead
    /// intervals only, per the Task 1 brief); documented for completeness
    /// and because GW2EI's own polling algorithm
    /// (`CombatReplay.HandlePosition`) consults velocity events to decide
    /// whether to hold-last vs interpolate across a large position gap --
    /// out of scope here, see `analysis::replay`'s module doc for the
    /// simplification this project makes instead.
    pub const VELOCITY: u8 = 20;
    /// Agent facing direction changed (GW2EI calls this `Rotation`).
    /// Verified the same way: `CBTS_FACING` is the 22nd entry (index 21).
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.Rotation =
    /// 21`. IMPORTANT difference from `POSITION`/`VELOCITY`: the arcdps
    /// reference documents `dst_agent` here as `float[2]` (x/y direction
    /// components only, no z) -- but GW2EI's `RotationEvent` still derives
    /// from the same `MovementEvent` base and calls the same
    /// `GetParametricPoint3D()` 3-float unpack, so the `value` field's bits
    /// get reinterpreted as a "z" that has no documented meaning for a 2D
    /// facing vector (GW2EI's own callers only ever read the XY of a
    /// rotation). Not decoded by this project (no facing/orientation field
    /// in `analysis::replay`'s output); documented for completeness per the
    /// Task 1 brief's "verify velocity/facing if adjacent" instruction.
    pub const FACING: u8 = 21;
    /// Extension-registration/signature marker (M10 Task 1). Verified
    /// against the live arcdps EVTC reference (`curl
    /// https://www.deltaconnected.com/arcdps/evtc/README.txt`, 2026-08-08):
    /// hand-counting `enum cbtstatechange` entries from `CBTS_COMBAT = 0`,
    /// `CBTS_EXTENSION` is the 41st entry (index 40), immediately after
    /// `CBTS_STATRESET_DEFUNC` (39) and before `CBTS_APIDELAYED_DEFUNC` (41)
    /// -- both retired/defunct neighbors independently pin the count.
    /// Cross-checked against GW2EI's `ArcDPSEnums.StateChange.Extension =
    /// 40` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:300`). The arcdps
    /// reference itself documents no payload beyond "for extension use. not
    /// managed by arcdps" -- GW2EI's `Extensions/ExtensionHelper.cs` is the
    /// payload arbiter: a ROW ON THIS EXACT STATECHANGE with `pad == 0`
    /// (`RawEvent::pad`, the final 4 bytes of the 64-byte event struct,
    /// undecoded before this task -- see `BUFF_APPLY`'s doc comment, which
    /// already flagged offset 60 as "pad61") is a one-time REGISTRATION row:
    /// `src_agent`'s low 32 bits are the extension's signature (e.g. the
    /// healing extension's `0x9c9b3c99`, `HealingStatsExtensionHandler.
    /// EXT_HealingStats`), bits 32-55 (`(src_agent & 0x00FFFFFF00000000) >>
    /// 32`) are its revision (`ExtensionHelper.SigMask`/`RevMask`/
    /// `RevShift`). Once registered, EVERY SUBSEQUENT row on `EXTENSION`
    /// (with a NONZERO `pad`) OR on `EXTENSION_COMBAT` (49, see below) whose
    /// `pad` equals that same signature is a DATA row belonging to that
    /// extension -- see `crate::evtc::ext_healing`'s module doc for the
    /// healing extension's own data-row payload shape (which reuses the
    /// ordinary `CBTS_COMBAT`-shaped value/buff_dmg/is_shields/is_offcycle
    /// fields, not a bespoke layout).
    pub const EXTENSION: u8 = 40;
    /// Extension DATA-row statechange (M10 Task 1) -- the more common shape
    /// extension addons use for their per-event payloads, alongside
    /// nonzero-`pad` `EXTENSION` rows (both route through the same `pad ==
    /// signature` dispatch -- see `EXTENSION`'s doc comment). Verified
    /// against the live arcdps EVTC reference the same way: hand-counting
    /// from `CBTS_COMBAT = 0`, `CBTS_EXTENSIONCOMBAT` is the 50th entry
    /// (index 49), immediately after `CBTS_IDLEEVENT` (48, itself right
    /// after the already-verified `CBTS_IDTOGUID = 46`/`CBTS_LOGNPCUPDATE =
    /// 47` pair -- three independently-anchored neighbors). Cross-checked
    /// against GW2EI's `ArcDPSEnums.StateChange.ExtensionCombat = 49`.
    pub const EXTENSION_COMBAT: u8 = 49;
}

/// `is_buffremove` enum values (arcdps `enum cbtbuffremove`). Verified
/// against GW2EI's `ArcDPSEnums.BuffRemove`
/// (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs`): `None = 0, All = 1,
/// Single = 2, Manual = 3`. Used on ordinary `is_statechange == 0` combat
/// events (pre-`BuffAppliesAndRemovesAsStateChanges` era -- see
/// `sc::BUFF_INITIAL` docs) to distinguish a buff-removal combat event from
/// a plain strike/buff-apply/buff-damage-tick one, and to pick the removal
/// kind.
pub mod buff_remove {
    pub const NONE: u8 = 0;
    pub const ALL: u8 = 1;
    pub const SINGLE: u8 = 2;
    /// A manual removal (e.g. dodge-cancelling your own buff via a trait,
    /// or certain UI-driven self-cleanses). GW2EI's `BuffRemoveManualEvent`
    /// explicitly excludes these from the stack simulator entirely
    /// (`IsBuffSimulatorCompliant` returns `false`, `UpdateSimulator` is a
    /// no-op -- `GW2EIEvtcParser/ParsedData/CombatEvents/BuffEvents/
    /// BuffRemoves/BuffRemoveManualEvent.cs`); `analysis::buffs` mirrors
    /// this by not extracting Manual removals as simulator events at all.
    pub const MANUAL: u8 = 3;
}

/// `iff` enum values (arcdps `enum iff`). Verified against GW2EI's
/// `ArcDPSEnums.IFF` (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:618-624`):
/// `Friend = 0, Foe = 1, Unknown` (i.e. `2`; the enum's last member is
/// implicitly `Foe + 1`), with `ArcDPSEnums.GetIFF` clamping anything else
/// to `Unknown`.
pub mod iff {
    pub const FRIEND: u8 = 0;
    pub const FOE: u8 = 1;
    /// arcdps could not attribute the event to a friend or a foe. On a
    /// `BuffRemove.Single` row this is half of GW2EI's
    /// `BuffRemoveSingleEvent.OverstackOrNaturalEnd` test (the other half
    /// being `dst_agent == 0`) -- see `analysis::buffs::events`'s
    /// `is_overstack_or_natural_end`.
    pub const UNKNOWN: u8 = 2;
}
pub mod result {
    // combat result values (`enum cbtresult`) -- verified by hand-counting
    // every enumerator, in order, from the live arcdps EVTC reference
    // (`curl https://www.deltaconnected.com/arcdps/evtc/README.txt`,
    // 2026-08-09): `CBTR_STRIKE_DAMAGENORMAL=0, CBTR_STRIKE_DAMAGECRIT=1,
    // CBTR_STRIKE_DAMAGEGLANCE=2, CBTR_BLOCK=3, CBTR_EVADE=4,
    // CBTR_INTERRUPT=5, CBTR_ABSORB=6, CBTR_BLIND=7, CBTR_KILLINGBLOW=8,
    // CBTR_DOWNED=9, CBTR_DEFIANCE_DAMAGENORMAL=10, CBTR_SKILLCAST=11,
    // CBTR_CROWDCONTROL=12, CBTR_INVERT=13, CBTR_BUFF_DAMAGECYCLE=14,
    // CBTR_BUFF_DAMAGENOTCYCLE=15, CBTR_BUFF_DAMAGENOTCYCLEDMGTOTARGETONHIT=16,
    // CBTR_BUFF_DAMAGENOTCYCLEDMGTOSOURCEONHIT=17,
    // CBTR_BUFF_DAMAGENOTCYCLEDMGTOTARGETONSTACKREMOVE=18, CBTR_UNKNOWN=19`.
    // Cross-checked byte-for-byte against GW2EI's `ArcDPSEnums.DamageResult`
    // (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:211-232`): `DirectNormal
    // =0, DirectCrit=1, DirectGlance=2, DirectBlock=3, DirectEvade=4,
    // Interrupt=5, DirectOrBuffAbsorb=6, DirectBlind=7, KillingBlow=8,
    // Downed=9, BreakbarDamage=10, Activation=11, CrowdControl=12,
    // DirectOrBuffInvert=13, BuffCycle=14, BuffNotCycle=15,
    // BuffNotCycle_DamageToTargetOnHit=16, BuffNotCycle_DamageToSourceOnHit
    // =17, BuffNotCycle_DamageToTargetOnStackRemove=18, Unknown=19` --
    // identical ordinals, values 0-18 inclusive. This is the SAME unified
    // enum both `buff == 0` (direct/strike) rows AND post-`ResultEnumRework`
    // `buff == 1` (buff/condition-damage) rows decode `result` through (see
    // `cc::is_cc`'s doc comment for the full pre/post-era split citation --
    // pre-era `buff == 1` rows instead decode through the separate, narrower,
    // now-retired `ConditionResult` enum, values 0-4 only, anything >= 5
    // (including this enum's own byte values from here) reads as `Unknown`
    // and is silently dropped by GW2EI, never becoming any kind of event at
    // all -- see `analysis::hit_stats`'s module doc for the classification
    // consequences on a pre-era log).
    pub const NORMAL: u8 = 0;
    pub const CRIT: u8 = 1;
    /// Glancing strike -- reduced (typically 50%) damage. M13 Task 1
    /// (`analysis::hit_stats`): a `buff == 0` "hit" outcome (still connects,
    /// still deals reduced direct damage), distinct from `BLOCK`/`EVADE`
    /// (which deal zero damage).
    pub const GLANCE: u8 = 2;
    /// Attack was blocked (e.g. Mesmer Shield 4) -- deals zero damage, does
    /// NOT connect. M13 Task 1.
    pub const BLOCK: u8 = 3;
    /// Attack was evaded (dodge, or a skill-based evade e.g. Mesmer Sword 2)
    /// -- deals zero damage, does NOT connect. M13 Task 1.
    pub const EVADE: u8 = 4;
    /// The struck agent's skill-cast was interrupted by this hit -- a
    /// marker result, not a damage-dealing one (GW2EI routes it to a
    /// `NoDamageHealthDamageEvent`/`IsNotADamageEvent`, contributing to
    /// none of `analysis::hit_stats`'s counters). M13 Task 1.
    pub const INTERRUPT: u8 = 5;
    /// Attack was invulnerable/absorbed (e.g. a Guardian elite skill) --
    /// deals zero effective damage, does NOT connect. Shared by both
    /// `buff == 0` (`DirectOrBuffAbsorb`) and post-era `buff == 1` rows
    /// (same enum value). M13 Task 1.
    pub const ABSORB: u8 = 6;
    /// Attack missed entirely (a "blind"/miss outcome) -- deals zero
    /// damage, does NOT connect. M13 Task 1.
    pub const BLIND: u8 = 7;
    pub const KILLING_BLOW: u8 = 8;
    pub const DOWNED: u8 = 9;
    /// Damage applied to a breakbar/defiance bar, not health -- a separate
    /// (non-`HealthDamageEvent`) accounting path in GW2EI, out of scope for
    /// `analysis::hit_stats`'s outgoing HEALTH-damage counters. M13 Task 1
    /// (documented for completeness of the enum, not consumed).
    pub const BREAKBAR_DAMAGE: u8 = 10;
    /// "On-skill-use signal event" (`CBTS_SKILLCAST` per the arcdps
    /// reference; GW2EI names the same ordinal `Activation`) -- a cast
    /// marker, not a damage-dealing result; unrelated to this project's own
    /// `RawEvent::is_activation` byte (a different field entirely). Not a
    /// damage event (M13 Task 1, documented for completeness).
    pub const ACTIVATION: u8 = 11;
    /// Crowd-control application marker. arcdps synthesizes these under
    /// generic pseudo-skills (e.g. "Generic Knockback and Pull", "Generic
    /// Launch", "Generic Control Effect From Buff"); `value`/`buff_dmg` on
    /// these events encode CC duration in ms, not damage. Excluded from
    /// damage accumulation — calibrated against the golden WvW fixture
    /// (Task 16A): including them over-counted squadTotalDamage.
    pub const CROWD_CONTROL: u8 = 12;
    /// Damage was inverted (e.g. reflected back at its own source) -- GW2EI
    /// treats this identically to `ABSORB` for the struck agent (zero
    /// effective damage, does not connect; `IsAbsorbed = result ==
    /// DirectOrBuffAbsorb || result == DirectOrBuffInvert`). M13 Task 1.
    pub const INVERT: u8 = 13;
    /// Post-era (`buff == 1` only) condition/buff damage tick that happened
    /// on its regular tick timer (a normal DoT tick, e.g. an ordinary
    /// Bleeding/Burning tick). A "hit" outcome for `analysis::hit_stats`.
    /// M13 Task 1.
    pub const BUFF_CYCLE: u8 = 14;
    /// Post-era `buff == 1` buff damage that happened OFF its regular tick
    /// timer (resistable proc damage, e.g. an on-hit condition proc). A
    /// "hit" outcome. M13 Task 1.
    pub const BUFF_NOT_CYCLE: u8 = 15;
    /// Post-era `buff == 1` buff damage dealt TO the target ON HIT -- one of
    /// the two life-leech-shaped result values (GW2EI: `IsLifeLeech = result
    /// == BuffNotCycle_DamageToTargetOnHit || result ==
    /// BuffNotCycle_DamageToTargetOnStackRemove`). M13 Task 1
    /// (`analysis::hit_stats::life_leech_count`/`life_leech_damage`).
    pub const BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT: u8 = 16;
    /// Post-era `buff == 1` buff damage dealt TO the SOURCE on hitting the
    /// target (e.g. a thorns/retaliation-shaped effect) -- a "hit" outcome,
    /// NOT life-leech (GW2EI's `IsLifeLeech` check does not include this
    /// value). M13 Task 1.
    pub const BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT: u8 = 17;
    /// Post-era `buff == 1` buff damage dealt TO the target on the SOURCE
    /// losing a stack (the other life-leech-shaped result value). M13 Task
    /// 1 (`analysis::hit_stats::life_leech_count`/`life_leech_damage`).
    pub const BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE: u8 = 18;
}

#[derive(Debug, Clone)]
pub struct RawEvent {
    pub time: u64,
    pub src_agent: u64,
    pub dst_agent: u64,
    pub value: i32,
    pub buff_dmg: i32,
    pub overstack: u32,
    pub skillid: u32,
    pub src_instid: u16,
    pub dst_instid: u16,
    pub src_master_instid: u16,
    pub dst_master_instid: u16,
    pub iff: u8,
    pub buff: u8,
    pub result: u8,
    pub is_activation: u8,
    pub is_buffremove: u8,
    /// Offset 53, between `is_buffremove` (52) and `is_fifty` (54, NOT
    /// decoded -- unused by any analysis in this project so far). Decoded
    /// starting M13 Task 1 (`analysis::hit_stats`'s above-90 fields need
    /// it). Per the live arcdps EVTC reference (`curl
    /// https://www.deltaconnected.com/arcdps/evtc/README.txt`, 2026-08-09,
    /// `CBTS_COMBAT` block): "`is_ninety: src is above 90% health`" --
    /// **the damage SOURCE's (attacker's) own health**, NOT the target's,
    /// despite the field living on the target-directed damage row. Cross-
    /// checked against GW2EI's `CombatItem.IsNinety`/`SkillEvent.
    /// IsOverNinety = evtcItem.IsNinety > 0`
    /// (`GW2EIEvtcParser/ParsedData/CombatEvents/SkillEvent.cs:36`), which
    /// is what `OffensiveStatistics`'s `PowerDamageAbove90HPCount`/
    /// `ConditionDamageAbove90HPCount` (EI's `statsAll[0].
    /// connectedPowerAbove90HPCount`/`connectedConditionAbove90HPCount`)
    /// read directly -- see `analysis::hit_stats`'s module doc for the full
    /// "source's own health, not the target's" nuance writeup (this
    /// deviates from a naive reading of "above90" as being about the
    /// target).
    pub is_ninety: u8,
    /// Offset 54, between `is_ninety` (53) and `is_moving` (55). Decoded
    /// starting M16 Task 2: three catalogued damage modifiers gate on it
    /// (`Mod_RelicOfTheEagle`, `Mod_CloseToDeath`, `Mod_BoltToTheHeart`).
    /// Per the live arcdps EVTC reference (`CBTS_COMBAT` block):
    /// "`is_fifty: target is below 50% health`" -- unlike `is_ninety` this
    /// one IS about the TARGET, which is why GW2EI names the two asymmetrically
    /// (`SkillEvent.cs:36-37`, `IsOverNinety = evtcItem.IsNinety > 0` vs
    /// `AgainstUnderFifty = evtcItem.IsFifty > 0`).
    pub is_fifty: u8,
    /// Offset 55, between `is_fifty` (54) and `is_statechange`
    /// (56). Decoded starting M13 Task 1 (`analysis::hit_stats`'s
    /// `moving_count` needs it). Per the live arcdps EVTC reference
    /// (`CBTS_COMBAT` block): "`is_moving: bit0 set if src is moving, bit1
    /// set if dst is moving`" -- a two-bit flag field, NOT a plain boolean.
    /// Cross-checked against GW2EI's `SkillEvent` ctor: `IsMoving =
    /// (evtcItem.IsMoving & 1) > 0` (source moving, not consumed by this
    /// project), `AgainstMoving = (evtcItem.IsMoving & 2) > 0` (target
    /// moving -- what EI's `statsAll[0].againstMovingRate` counts, and what
    /// `analysis::hit_stats::moving_count` mirrors).
    pub is_moving: u8,
    pub is_statechange: u8,
    /// Offset 57, between `is_statechange` (56) and `is_shields` (58) --
    /// decoded starting M10 Task 2 because the missile-family payloads need
    /// it: `CBTS_MISSILELAUNCH`'s "is_src_flanking: non-zero if first
    /// launch" and `CBTS_MISSILEREMOVE`'s "is_src_flanking: hit at least
    /// one enemy along the way" (arcdps EVTC reference,
    /// `README.txt`). Cross-checked against GW2EI's
    /// `MissileLaunchEvent`/`MissileRemoveEvent` ctors, both of which read
    /// `evtcItem.IsFlanking > 0`
    /// (`GW2EIEvtcParser/ParsedData/CombatEvents/StatusEvents/MissileEvents/`).
    /// On an ordinary `CBTS_COMBAT` strike event this is the "src is
    /// flanking dst" flag (unused by this project outside the missile
    /// path). See `crate::analysis::missiles` for how the two missile-event
    /// meanings are used.
    pub is_flanking: u8,
    /// Verified against the arcdps EVTC reference struct layout (`iff`
    /// through `is_offcycle` are single bytes at offsets 48-59; see the
    /// offset table in `decode_events` below): `is_shields` sits at offset
    /// 58, between `is_flanking` (57) and `is_offcycle` (59). On a
    /// `CBTS_BUFFAPPLY`-shaped event (`buff == 1`, apply -- see
    /// `sc::BUFF_INITIAL` docs for this project's pre-rework event shape),
    /// the arcdps reference documents it as "non-zero if buff is active
    /// when applied". Cross-checked against GW2EI's `BuffApplyEvent`:
    /// `_addedActive = evtcItem.IsShields > 0;`
    /// (`ParsedData/CombatEvents/BuffEvents/BuffApplies/BuffApplyEvent.cs`),
    /// which decides whether the new stack is inserted as the immediately
    /// ACTIVE (ticking) one or appended to the back of the frozen queue --
    /// see `analysis::buffs::simulator`'s duration-boon fix-round-1 rework.
    pub is_shields: u8,
    /// Offset 59, immediately after `is_shields` (M3 Task 2). On an
    /// apply-shaped event (`buff == 1`, an `IsBuffApplyEvent`-matching
    /// row), a nonzero value routes it to GW2EI's `BuffExtensionEvent`
    /// instead of a plain `BuffApplyEvent` -- verified against
    /// `GW2EIEvtcParser/ParsedData/CombatEvents/CombatEventFactory.cs`,
    /// `AddBuffApplyEvent`'s pre-`ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges`
    /// branch: `if (buffEvent.IsOffcycle > 0) { ... new BuffExtensionEvent(...) }
    /// else { ... new BuffApplyEvent(...) }`. An extension event EXTENDS an
    /// already-active stack's remaining duration in place (or becomes a
    /// fresh active stack if none is active) rather than pushing a new
    /// queued stack -- see `analysis::buffs::events::BuffEventKind::Extend`
    /// and `simulator`'s `Extend` handling.
    pub is_offcycle: u8,
    /// Final 4 bytes of the 64-byte event struct (offset 60-63) -- arcdps's
    /// reference calls this a generic "pad" with no fixed meaning of its
    /// own; GW2EI's `BuffApplyEvent` doc already flagged it as "pad61" (a
    /// per-stack trackable id, not decoded by this project) for
    /// `sc::BUFF_APPLY` rows. Decoded starting M10 Task 1 because
    /// EXTENSION-family statechanges (`sc::EXTENSION`/`sc::
    /// EXTENSION_COMBAT`) repurpose it as the extension SIGNATURE dispatch
    /// key -- see `crate::evtc::ext_healing`'s module doc.
    pub pad: u32,
}

pub fn decode_events(buf: &[u8], count: usize) -> Result<Vec<RawEvent>, EvtcError> {
    let need = count * EVENT_SIZE_REV1;
    if buf.len() < need {
        return Err(EvtcError::Truncated { need, at: 0, have: buf.len() });
    }
    let u64le = |s: &[u8]| u64::from_le_bytes(s.try_into().unwrap());
    let i32le = |s: &[u8]| i32::from_le_bytes(s.try_into().unwrap());
    let u32le = |s: &[u8]| u32::from_le_bytes(s.try_into().unwrap());
    let u16le = |s: &[u8]| u16::from_le_bytes(s.try_into().unwrap());
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let e = &buf[i * EVENT_SIZE_REV1..(i + 1) * EVENT_SIZE_REV1];
        out.push(RawEvent {
            time: u64le(&e[0..8]),
            src_agent: u64le(&e[8..16]),
            dst_agent: u64le(&e[16..24]),
            value: i32le(&e[24..28]),
            buff_dmg: i32le(&e[28..32]),
            overstack: u32le(&e[32..36]),
            skillid: u32le(&e[36..40]),
            src_instid: u16le(&e[40..42]),
            dst_instid: u16le(&e[42..44]),
            src_master_instid: u16le(&e[44..46]),
            dst_master_instid: u16le(&e[46..48]),
            // Single-byte fields, in arcdps cbtevent struct order, starting at
            // offset 48 (after the four u16 instids): iff, buff, result,
            // is_activation, is_buffremove, is_ninety, is_fifty, is_moving,
            // is_statechange, is_flanking, is_shields, is_offcycle.
            iff: e[48],
            buff: e[49],
            result: e[50],
            is_activation: e[51],
            is_buffremove: e[52],
            is_ninety: e[53],
            is_fifty: e[54],
            is_moving: e[55],
            is_statechange: e[56],
            is_flanking: e[57],
            is_shields: e[58],
            is_offcycle: e[59],
            pad: u32le(&e[60..64]),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn strike_event() -> Vec<u8> {
        let mut b = vec![0u8; EVENT_SIZE_REV1];
        b[0..8].copy_from_slice(&1000u64.to_le_bytes()); // time
        b[8..16].copy_from_slice(&0xAAAAu64.to_le_bytes()); // src_agent
        b[16..24].copy_from_slice(&0xBBBBu64.to_le_bytes()); // dst_agent
        b[24..28].copy_from_slice(&500i32.to_le_bytes()); // value (damage)
        b[28..32].copy_from_slice(&321i32.to_le_bytes()); // buff_dmg
        b[32..36].copy_from_slice(&654u32.to_le_bytes()); // overstack
        b[36..40].copy_from_slice(&77u32.to_le_bytes()); // skillid
        b[40..42].copy_from_slice(&111u16.to_le_bytes()); // src_instid
        b[48] = 1; // iff = FOE
        b[49] = 3; // buff (distinguishable probe value)
        // offsets: iff@48, buff@49, result@50, is_activation@51,
        // is_buffremove@52, is_ninety@53, is_fifty@54, is_moving@55,
        // is_statechange@56, is_flanking@57, is_shields@58, is_offcycle@59.
        b[50] = result::CRIT; // result
        // Wire-level probe for `is_ninety` (offset 53) / `is_moving` (offset
        // 55), decoded starting M13 Task 1: distinct nonzero values at each
        // field under test AND at their immediate neighbors (is_buffremove@52,
        // the undecoded is_fifty@54 gap on both sides), so a ±1 decoder
        // offset bug fails the assertion in `decodes_strike` below rather
        // than silently passing.
        b[53] = 13; // is_ninety (the field under test, M13 Task 1)
        b[55] = 15; // is_moving (the field under test, M13 Task 1)
        b[56] = sc::ENTER_COMBAT; // is_statechange
        // Wire-level probe for `is_shields` (offset 58): distinct nonzero
        // values at is_shields itself AND at both immediate neighbors
        // (is_flanking@57, is_offcycle@59), so a ±1 decoder offset bug
        // (reading either neighbor instead of 58) fails the assertion in
        // `decodes_strike` below rather than silently passing.
        b[57] = 9; // is_flanking (the field under test, M10 Task 2)
        b[58] = 7; // is_shields (the field under test)
        b[59] = 11; // is_offcycle (the OTHER field under test)
        b
    }
    #[test]
    fn decodes_strike() {
        let ev = decode_events(&strike_event(), 1).unwrap();
        let e = &ev[0];
        assert_eq!(e.time, 1000);
        assert_eq!(e.src_agent, 0xAAAA);
        assert_eq!(e.dst_agent, 0xBBBB);
        assert_eq!(e.value, 500);
        assert_eq!(e.buff_dmg, 321);
        assert_eq!(e.overstack, 654);
        assert_eq!(e.skillid, 77);
        assert_eq!(e.src_instid, 111);
        assert_eq!(e.iff, 1);
        assert_eq!(e.buff, 3);
        assert_eq!(e.result, result::CRIT);
        assert_eq!(e.is_ninety, 13, "is_ninety must decode from offset 53, not a ±1 neighbor");
        assert_eq!(e.is_moving, 15, "is_moving must decode from offset 55, not a ±1 neighbor");
        assert_eq!(e.is_statechange, sc::ENTER_COMBAT);
        assert_eq!(e.is_flanking, 9, "is_flanking must decode from offset 57, not a ±1 neighbor");
        assert_eq!(e.is_shields, 7, "is_shields must decode from offset 58, not a ±1 neighbor");
        assert_eq!(e.is_offcycle, 11, "is_offcycle must decode from offset 59, not a ±1 neighbor");
    }
}
