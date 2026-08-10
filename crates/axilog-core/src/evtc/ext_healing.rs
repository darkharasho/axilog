//! arcdps healing-extension (`arcdps_healing_stats`) wire-format decode
//! (M10 Task 1) -- low-level, byte/flag-level decode only. Does NOT resolve
//! agent identity (see below for why that needs a separate, time-aware
//! pass) or fold accounts/squad membership -- see
//! `analysis::healing` for the aggregation layer built on top of this.
//!
//! ## Signature/dispatch mechanism
//!
//! The healing extension (like every arcdps extension) is NOT a bespoke
//! `is_statechange` payload of its own. It rides two existing statechanges,
//! [`crate::evtc::sc::EXTENSION`] (40) and [`crate::evtc::sc::
//! EXTENSION_COMBAT`] (49) -- see those constants' doc comments for the
//! arcdps-README ordinal verification. The dispatch recipe, verified
//! against GW2EI's `Extensions/ExtensionHelper.cs`
//! (`GetExtensionHandler`) and `EvtcParser.cs` (`IsValid`'s `IsExtension`
//! branch, `CombatData.cs`'s extension-routing branch):
//!
//! 1. **Registration row**: `is_statechange == EXTENSION` AND `pad == 0`
//!    (`RawEvent::pad`, the final 4 bytes of the 64-byte event struct).
//!    `src_agent`'s low 32 bits are the extension's SIGNATURE; bits 32-55
//!    (`(src_agent & 0x00FF_FFFF_0000_0000) >> 32`) are its REVISION
//!    (`ExtensionHelper.SigMask = 0x00000000FFFFFFFF`, `RevMask =
//!    0x00FFFFFF00000000`, `RevShift = 32`). The healing extension's
//!    signature is `0x9c9b3c99`
//!    (`HealingStatsExtensionHandler.EXT_HealingStats`); GW2EI accepts
//!    revisions 1 and 2 (`ExtensionHelper`'s `switch (rev) { case 1: case
//!    2: ... }`) -- both handled identically by
//!    `HealingStatsExtensionHandler` (no revision-gated branch exists
//!    anywhere in its decode/aggregation code), so this module does too.
//! 2. **Data rows**: any subsequent row on `EXTENSION` (with a NONZERO
//!    `pad`) OR on `EXTENSION_COMBAT` whose `pad` equals a registered
//!    signature is that extension's data. For the healing extension, a
//!    data row's payload REUSES the ordinary `CBTS_COMBAT` fields
//!    (`value`/`buff_dmg`/`buff`/`is_shields`/`is_offcycle`/`iff`), not a
//!    bespoke layout -- verified against
//!    `HealingStatsExtensionHandler.IsHealingEvent`/`IsBarrierEvent`/
//!    `InsertEIExtensionEvent`:
//!    - `is_shields == 0` => healing; `is_shields > 0` => barrier (both
//!      gated the same signature, distinguished purely by this flag).
//!    - `buff == 0 && value < 0` => DIRECT event, `amount = -value`.
//!    - `buff != 0 && value == 0 && buff_dmg < 0` => NON-DIRECT (regen/
//!      buff-tick) event, `amount = -buff_dmg`.
//!    - anything else does not match either shape and is not a healing/
//!      barrier data row (defensive -- not expected in practice).
//!
//! ## Agent identity: `src_instid`/`dst_instid`, NOT `src_agent`/`dst_agent`
//!
//! **Load-bearing decode fact**: on a data row, `src_agent`/`dst_agent` are
//! NOT valid agent addresses. GW2EI's `HealingStatsExtensionHandler.
//! AdjustCombatEvent` OVERRIDES both fields via an instid lookup BEFORE
//! constructing its event objects: `AgentItem src = agentData.
//! GetAgentByInstID(combatItem.SrcInstid, combatItem.Time); combatItem.
//! OverrideSrcAgent(src);` (and the same for `dst`/`DstInstid`) -- "Prefer
//! instid fetch for healing events" per that method's own comment. This is
//! DIFFERENT from ordinary damage/state events, whose `SkillEvent`
//! constructor resolves `From`/`To` directly from the raw `src_agent`/
//! `dst_agent` addresses (`agentData.GetAgent(evtcItem.SrcAgent, ...)`,
//! `ParsedData/CombatEvents/SkillEvent.cs`) -- i.e. the healing-stats addon
//! only knows instance ids for the events it reports, not raw pointers.
//! Callers of this module MUST resolve `src_instid`/`dst_instid` to a real
//! addr via a time-aware instid registry (this project's
//! `analysis::damage::InstidRegistry`, already used for the same reason by
//! pet/minion damage credit) before these events mean anything.
//!
//! ## Downed flag
//!
//! `AgainstDowned` (GW2EI's `EXTHealingExtensionEvent`/
//! `EXTDirectHealingEvent`/`EXTDirectBarrierEvent`), verified against
//! `Extensions/ExtensionCombatData/ExtensionCombatEvents/
//! EXTHealingExtensionEvent.cs`:
//! - Base (both direct and non-direct rows): bit 5 of `is_offcycle`
//!   (`HealingExtensionOffcycleBits.BuffDamageDstIsDowned = 1 << 5 =
//!   0x20`).
//! - DIRECT rows ONLY, additionally: `(is_offcycle & 0x3F) == 1` (i.e. the
//!   low 6 bits, with the two peer bits below masked off, equal exactly
//!   `1`) -- `EXTDirectHealingEvent`/`EXTDirectBarrierEvent`'s constructors
//!   both OR this extra check in (`AgainstDowned = AgainstDowned ||
//!   (evtcItem.IsOffcycle & ~PeerMask) == 1`), on top of the base check
//!   already applied by the shared base-class constructor.
//!
//! ## Peer flags / de-duplication
//!
//! `is_offcycle` also carries two "peer" bits used to de-duplicate reports
//! when more than one squad member's healing-stats addon instance reports
//! the same event (each squad member's addon can echo estimated data about
//! OTHER squad members it can see, alongside direct self-reports):
//! `SrcPeerMask = 1 << 7 = 0x80`, `DstPeerMask = 1 << 6 = 0x40`. If NEITHER
//! bit is set, `SrcIsPeer` defaults to `true` (`EXTHealingExtensionEvent`'s
//! constructor: `if (!SrcIsPeer && !DstIsPeer) { SrcIsPeer = true; }`) --
//! the ordinary case for a self-reported event. GW2EI's
//! `HealingStatsExtensionHandler.SanitizeForSrc` groups events by resolved
//! healer and, within each healer's group, drops every non-peer
//! (`!SrcIsPeer`) event IF that group contains at least one peer
//! (`SrcIsPeer == true`) event -- preferring the peer-reported version over
//! a duplicate self/estimated one. This module exposes `src_is_peer` per
//! event; `analysis::healing` applies the same per-healer sanitization
//! before summing (the "outgoing" query path this project needs only reads
//! GW2EI's SRC-grouped/sanitized view, `EXTHealingCombatData.GetHealData`
//! -- the DST-grouped `SanitizeForDst`/`GetHealReceivedData` path backs
//! EI's "incoming" stats, out of this task's scope).
//!
//! ## `ToFriendly`
//!
//! `EXTActorHealingHelper.InitHealEvents`/`InitIncomingHealEvents` both
//! filter `.Where(x => x.ToFriendly)` before anything else --
//! `ToFriendly` is `evtcItem.IFF == IFF.Friend`, i.e. `iff == 0` in this
//! project's convention (see `analysis::damage`/`analysis::cc`'s existing
//! `e.iff == 0` FRIEND checks). Exposed here as `to_friendly` per event.

use super::{RawEvent, RawLog};

/// The healing extension's signature
/// (`HealingStatsExtensionHandler.EXT_HealingStats`).
pub const HEALING_SIGNATURE: u32 = 0x9c9b_3c99;

/// A decoded extension signature-registration row (module doc, step 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registration {
    pub signature: u32,
    pub revision: u32,
}

/// Decode `e` as a registration row IF it is shaped like one
/// (`is_statechange == EXTENSION`, `pad == 0`) -- does not check the
/// signature/revision against anything; callers filter for
/// [`HEALING_SIGNATURE`] with revision 1 or 2 themselves (see
/// [`healing_extension_present`]).
pub fn decode_registration(e: &RawEvent) -> Option<Registration> {
    if e.is_statechange != super::sc::EXTENSION || e.pad != 0 {
        return None;
    }
    let signature = (e.src_agent & 0x0000_0000_FFFF_FFFF) as u32;
    let revision = ((e.src_agent & 0x00FF_FFFF_0000_0000) >> 32) as u32;
    Some(Registration { signature, revision })
}

/// Whether `raw` carries a valid healing-extension registration row
/// (signature [`HEALING_SIGNATURE`], revision 1 or 2 -- the only revisions
/// GW2EI's own `ExtensionHelper.GetExtensionHandler` accepts). This is the
/// gate `analysis::healing` uses to decide whether to scan for data rows at
/// all, and whether to omit the native schema's `healing` block / push the
/// "healing extension not present" warning.
pub fn healing_extension_present(raw: &RawLog) -> bool {
    raw.events.iter().filter_map(decode_registration).any(|r| {
        r.signature == HEALING_SIGNATURE && (r.revision == 1 || r.revision == 2)
    })
}

/// One decoded healing/barrier extension DATA row (module doc, step 2) --
/// agent identity is left as raw instids; see the module doc's "Agent
/// identity" section for why `analysis::healing` must resolve these via a
/// time-aware instid registry rather than trusting `src_agent`/`dst_agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawExtHealEvent {
    pub time: u64,
    pub src_instid: u16,
    pub dst_instid: u16,
    /// Wire `src_master_instid`, UNMODIFIED by the extension's src-agent
    /// adjustment (fix round, minion/pet folding). Verified against GW2EI's
    /// `CombatItem.OverrideSrcAgent(AgentItem)` (the overload
    /// `HealingStatsExtensionHandler.AdjustCombatEvent` calls): it sets
    /// `SrcAgent`/`SrcInstid` to the resolved (englobing) agent's own
    /// identity, but never touches `SrcMasterInstid` -- so this field still
    /// carries whatever raw pet/minion-ownership instid arcdps originally
    /// reported, exactly like an ordinary (non-extension) combat row.
    /// `EvtcParser.cs`'s "Linking minions to their masters" pass
    /// (`FindAgentMaster(c.Time, c.SrcMasterInstid, c.SrcAgent)`) runs on
    /// EVERY combat item whose `SrcIsAgent()` is true -- which, for
    /// extension rows, dispatches to the SAME `HealingStatsExtensionHandler.
    /// SrcIsAgent` this decoder already gates on (`IsHealingEvent ||
    /// IsBarrierEvent`) -- so a healing-extension row from a player's pet/
    /// minion links to its owner via this field exactly like ordinary pet
    /// damage does (`analysis::damage::pet_credit_events`'s own
    /// `src_master_instid` use). 0 when absent (no owner link), matching
    /// every other instid field's "0 = none" convention already used
    /// elsewhere in this project.
    pub src_master_instid: u16,
    /// Heal or barrier amount, always positive (already sign-flipped from
    /// the wire's negative `value`/`buff_dmg`).
    pub amount: u64,
    pub is_barrier: bool,
    pub against_downed: bool,
    pub src_is_peer: bool,
    pub dst_is_peer: bool,
    pub to_friendly: bool,
}

/// Decode `e` as a healing-extension data row, if it matches the dispatch
/// recipe in this module's doc comment for `signature` (typically
/// [`HEALING_SIGNATURE`]). `None` for anything that doesn't match --
/// including a row on the right statechange/pad but whose value/buff_dmg/
/// buff shape doesn't match either the direct or non-direct pattern
/// (defensive; not expected on a real log).
pub fn decode_data_event(e: &RawEvent, signature: u32) -> Option<RawExtHealEvent> {
    let is_extension_data_row = (e.is_statechange == super::sc::EXTENSION && e.pad != 0)
        || e.is_statechange == super::sc::EXTENSION_COMBAT;
    if !is_extension_data_row || e.pad != signature {
        return None;
    }

    let is_direct = e.buff == 0;
    let amount: u64 = if is_direct {
        if e.value >= 0 {
            return None;
        }
        (-e.value) as u64
    } else {
        if !(e.value == 0 && e.buff_dmg < 0) {
            return None;
        }
        (-e.buff_dmg) as u64
    };

    let is_barrier = e.is_shields > 0;

    // Bit layout, per this module's doc comment ("Downed flag" /
    // "Peer flags"): bit5 = BuffDamageDstIsDowned, bit6 = DstPeerMask,
    // bit7 = SrcPeerMask.
    const BUFF_DAMAGE_DST_IS_DOWNED: u8 = 1 << 5;
    const DST_PEER_MASK: u8 = 1 << 6;
    const SRC_PEER_MASK: u8 = 1 << 7;
    const PEER_MASK: u8 = DST_PEER_MASK | SRC_PEER_MASK;

    let mut against_downed = (e.is_offcycle & BUFF_DAMAGE_DST_IS_DOWNED) != 0;
    if is_direct {
        against_downed = against_downed || (e.is_offcycle & !PEER_MASK) == 1;
    }

    let mut src_is_peer = (e.is_offcycle & SRC_PEER_MASK) != 0;
    let dst_is_peer = (e.is_offcycle & DST_PEER_MASK) != 0;
    if !src_is_peer && !dst_is_peer {
        src_is_peer = true;
    }

    Some(RawExtHealEvent {
        time: e.time,
        src_instid: e.src_instid,
        dst_instid: e.dst_instid,
        src_master_instid: e.src_master_instid,
        amount,
        is_barrier,
        against_downed,
        src_is_peer,
        dst_is_peer,
        to_friendly: e.iff == 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_event() -> RawEvent {
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
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn registration_event(signature: u32, revision: u32) -> RawEvent {
        let src_agent = (signature as u64) | ((revision as u64) << 32);
        RawEvent { is_statechange: super::super::sc::EXTENSION, pad: 0, src_agent, ..base_event() }
    }

    #[test]
    fn decodes_registration_signature_and_revision() {
        let e = registration_event(HEALING_SIGNATURE, 2);
        let r = decode_registration(&e).expect("registration row");
        assert_eq!(r.signature, HEALING_SIGNATURE);
        assert_eq!(r.revision, 2);
    }

    /// Wrong-field-mapping probe: reading signature/revision from the wrong
    /// bit range must NOT reproduce the packed values -- swap what would
    /// happen if signature were read from the high bits or revision from
    /// the low bits.
    #[test]
    fn registration_signature_and_revision_are_not_interchangeable() {
        let e = registration_event(HEALING_SIGNATURE, 2);
        let r = decode_registration(&e).unwrap();
        assert_ne!(r.signature as u64, r.revision as u64);
        assert_ne!(r.signature, HEALING_SIGNATURE >> 8, "must not be a shifted mis-decode");
    }

    #[test]
    fn registration_requires_zero_pad() {
        let mut e = registration_event(HEALING_SIGNATURE, 1);
        e.pad = 7; // a data row, not a registration row
        assert!(decode_registration(&e).is_none());
    }

    #[test]
    fn registration_requires_extension_statechange() {
        let mut e = registration_event(HEALING_SIGNATURE, 1);
        e.is_statechange = super::super::sc::EXTENSION_COMBAT; // wrong statechange
        assert!(decode_registration(&e).is_none());
    }

    #[test]
    fn healing_extension_present_true_for_valid_signature_and_revision() {
        let raw = RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![registration_event(HEALING_SIGNATURE, 1)],
            guid_map: vec![],
        };
        assert!(healing_extension_present(&raw));
    }

    #[test]
    fn healing_extension_absent_for_unknown_signature() {
        let raw = RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![registration_event(0xDEAD_BEEF, 1)],
            guid_map: vec![],
        };
        assert!(!healing_extension_present(&raw));
    }

    #[test]
    fn healing_extension_absent_for_unsupported_revision() {
        let raw = RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![registration_event(HEALING_SIGNATURE, 3)],
            guid_map: vec![],
        };
        assert!(!healing_extension_present(&raw));
    }

    fn direct_heal_event(value: i32, is_shields: u8, is_offcycle: u8, iff: u8) -> RawEvent {
        RawEvent {
            is_statechange: super::super::sc::EXTENSION_COMBAT,
            pad: HEALING_SIGNATURE,
            buff: 0,
            value,
            is_shields,
            is_offcycle,
            iff,
            src_instid: 11,
            dst_instid: 22,
            time: 1000,
            ..base_event()
        }
    }

    #[test]
    fn decodes_direct_heal_amount_from_negated_value() {
        let e = direct_heal_event(-450, 0, 0, 0);
        let ev = decode_data_event(&e, HEALING_SIGNATURE).expect("direct heal row");
        assert_eq!(ev.amount, 450);
        assert!(!ev.is_barrier);
        assert!(ev.to_friendly);
        assert_eq!(ev.src_instid, 11);
        assert_eq!(ev.dst_instid, 22);
    }

    /// Wrong-field-mapping probe: amount must come from the NEGATED
    /// `value`, not `buff_dmg` (which is untouched/zero here) or a raw
    /// (non-negated, hence negative) read of `value`.
    #[test]
    fn direct_heal_amount_is_not_read_from_buff_dmg_or_unnegated() {
        let mut e = direct_heal_event(-450, 0, 0, 0);
        e.buff_dmg = 999; // decoy -- must not leak into `amount`
        let ev = decode_data_event(&e, HEALING_SIGNATURE).unwrap();
        assert_eq!(ev.amount, 450, "must ignore buff_dmg on a direct row");
        assert_ne!(ev.amount as i64, -450i64, "amount must be positive (negated), not raw `value`");
    }

    #[test]
    fn decodes_barrier_via_is_shields_flag() {
        let e = direct_heal_event(-100, 1, 0, 0);
        let ev = decode_data_event(&e, HEALING_SIGNATURE).unwrap();
        assert!(ev.is_barrier);
    }

    fn nondirect_heal_event(buff_dmg: i32, is_offcycle: u8) -> RawEvent {
        RawEvent {
            is_statechange: super::super::sc::EXTENSION_COMBAT,
            pad: HEALING_SIGNATURE,
            buff: 1,
            value: 0,
            buff_dmg,
            is_shields: 0,
            is_offcycle,
            iff: 0,
            src_instid: 5,
            dst_instid: 6,
            time: 500,
            ..base_event()
        }
    }

    #[test]
    fn decodes_nondirect_heal_amount_from_negated_buff_dmg() {
        let e = nondirect_heal_event(-77, 0);
        let ev = decode_data_event(&e, HEALING_SIGNATURE).expect("non-direct heal row");
        assert_eq!(ev.amount, 77);
    }

    /// Wrong-field-mapping probe: a non-direct row (`buff != 0`) must read
    /// `amount` from `buff_dmg`, NOT `value` (which is required to be zero
    /// on a real non-direct row and would silently yield the wrong
    /// `amount` -- here forced nonzero as a decoy -- if the decoder read
    /// the wrong field or failed to require `value == 0`).
    #[test]
    fn nondirect_row_with_nonzero_value_is_rejected() {
        let mut e = nondirect_heal_event(-77, 0);
        e.value = -9999; // decoy: would look like a huge direct heal if misrouted
        assert!(decode_data_event(&e, HEALING_SIGNATURE).is_none());
    }

    #[test]
    fn against_downed_base_bit_applies_to_both_direct_and_nondirect() {
        let direct = direct_heal_event(-1, 0, 0x20, 0);
        assert!(decode_data_event(&direct, HEALING_SIGNATURE).unwrap().against_downed);
        let nondirect = nondirect_heal_event(-1, 0x20);
        assert!(decode_data_event(&nondirect, HEALING_SIGNATURE).unwrap().against_downed);
    }

    /// The extra `(is_offcycle & 0x3F) == 1` downed check is DIRECT-only --
    /// a non-direct row with `is_offcycle == 1` must NOT be treated as
    /// against-downed (only the base bit-5 check applies to it).
    #[test]
    fn extra_downed_bit_only_applies_to_direct_rows() {
        let direct = direct_heal_event(-1, 0, 1, 0);
        assert!(decode_data_event(&direct, HEALING_SIGNATURE).unwrap().against_downed);
        let nondirect = nondirect_heal_event(-1, 1);
        assert!(!decode_data_event(&nondirect, HEALING_SIGNATURE).unwrap().against_downed);
    }

    #[test]
    fn peer_bits_decode_and_default_src_peer_true_when_neither_set() {
        let neither = direct_heal_event(-1, 0, 0, 0);
        let ev = decode_data_event(&neither, HEALING_SIGNATURE).unwrap();
        assert!(ev.src_is_peer, "defaults to true when neither peer bit is set");
        assert!(!ev.dst_is_peer);

        let src_peer = direct_heal_event(-1, 0, 0x80, 0);
        let ev = decode_data_event(&src_peer, HEALING_SIGNATURE).unwrap();
        assert!(ev.src_is_peer);
        assert!(!ev.dst_is_peer);

        let dst_peer = direct_heal_event(-1, 0, 0x40, 0);
        let ev = decode_data_event(&dst_peer, HEALING_SIGNATURE).unwrap();
        assert!(!ev.src_is_peer);
        assert!(ev.dst_is_peer);
    }

    #[test]
    fn to_friendly_reflects_iff_zero() {
        let friend = direct_heal_event(-1, 0, 0, 0);
        assert!(decode_data_event(&friend, HEALING_SIGNATURE).unwrap().to_friendly);
        let foe = direct_heal_event(-1, 0, 0, 1);
        assert!(!decode_data_event(&foe, HEALING_SIGNATURE).unwrap().to_friendly);
    }

    #[test]
    fn wrong_signature_pad_is_rejected() {
        let mut e = direct_heal_event(-100, 0, 0, 0);
        e.pad = 0xDEAD_BEEF;
        assert!(decode_data_event(&e, HEALING_SIGNATURE).is_none());
    }

    #[test]
    fn extension_row_with_zero_pad_is_registration_not_data() {
        // is_statechange == EXTENSION with pad == 0 is a registration row,
        // not a data row -- decode_data_event must reject it even though
        // its statechange matches, since a registration row's `value`/
        // `buff_dmg`/etc. don't carry a real heal payload at all.
        let mut e = direct_heal_event(-100, 0, 0, 0);
        e.is_statechange = super::super::sc::EXTENSION;
        e.pad = 0;
        assert!(decode_data_event(&e, HEALING_SIGNATURE).is_none());
    }

    #[test]
    fn ordinary_combat_row_is_not_a_data_event() {
        let mut e = direct_heal_event(-100, 0, 0, 0);
        e.is_statechange = 0; // CBTS_COMBAT, not an extension row at all
        assert!(decode_data_event(&e, HEALING_SIGNATURE).is_none());
    }
}
