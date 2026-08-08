use super::{EvtcError, EVENT_SIZE_REV1};

pub mod sc {
    // is_statechange values (verified against arcdps cbtstatechange enum order)
    pub const NONE: u8 = 0;
    pub const ENTER_COMBAT: u8 = 1;
    pub const EXIT_COMBAT: u8 = 2;
    pub const CHANGE_DEAD: u8 = 4;
    pub const CHANGE_DOWN: u8 = 5;
    pub const LOG_START: u8 = 9;
    pub const LOG_END: u8 = 10;
    pub const MAX_HEALTH: u8 = 12;
    pub const POINT_OF_VIEW: u8 = 13;
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
pub mod result {
    // combat result values (verified against arcdps cbtresult enum order)
    pub const NORMAL: u8 = 0;
    pub const CRIT: u8 = 1;
    pub const KILLING_BLOW: u8 = 8;
    pub const DOWNED: u8 = 9;
    /// Crowd-control application marker. arcdps synthesizes these under
    /// generic pseudo-skills (e.g. "Generic Knockback and Pull", "Generic
    /// Launch", "Generic Control Effect From Buff"); `value`/`buff_dmg` on
    /// these events encode CC duration in ms, not damage. Excluded from
    /// damage accumulation — calibrated against the golden WvW fixture
    /// (Task 16A): including them over-counted squadTotalDamage.
    pub const CROWD_CONTROL: u8 = 12;
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
    pub is_statechange: u8,
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
            is_statechange: e[56],
            is_shields: e[58],
            is_offcycle: e[59],
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
        b[56] = sc::ENTER_COMBAT; // is_statechange
        // Wire-level probe for `is_shields` (offset 58): distinct nonzero
        // values at is_shields itself AND at both immediate neighbors
        // (is_flanking@57, is_offcycle@59), so a ±1 decoder offset bug
        // (reading either neighbor instead of 58) fails the assertion in
        // `decodes_strike` below rather than silently passing.
        b[57] = 9; // is_flanking (neighbor probe, not asserted on RawEvent -- not decoded)
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
        assert_eq!(e.is_statechange, sc::ENTER_COMBAT);
        assert_eq!(e.is_shields, 7, "is_shields must decode from offset 58, not a ±1 neighbor");
        assert_eq!(e.is_offcycle, 11, "is_offcycle must decode from offset 59, not a ±1 neighbor");
    }
}
