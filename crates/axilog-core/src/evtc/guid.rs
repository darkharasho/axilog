use super::event::{sc, RawEvent};

/// arcdps `n_contentlocal` enum — the content type carried by a
/// `CBTS_IDTOGUID` event's `overstack_value` field (our `overstack`).
///
/// Verified against the arcdps EVTC reference
/// (deltaconnected.com/arcdps/evtc/README.txt), which lists:
/// `CONTENTLOCAL_EFFECT, _MARKER, _SKILL, _SPECIES_NOT_GADGET, _EMOTE,
/// _TRANSFORMATION` — no `TEAM` entry. But GW2EI's actively-maintained
/// `ArcDPSEnums.ContentLocal` (which tracks arcdps changes closely, and is
/// updated well past this hosted doc's freshness — its enum runs through
/// 2026-06 arcdps builds) has `Team = 4` between `Species = 3` and
/// `Emote = 5`, and its `CombatEventFactory` dispatch switch decodes a
/// `TeamGUIDEvent` for it. Task 2b's brief (relaying direct guidance from
/// the arcdps developer) also specifies `TEAM = 4`. Conclusion: the hosted
/// README is stale on this one point; GW2EI + the direct dev guidance are
/// authoritative. This enum matches GW2EI's ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Effect,
    Marker,
    Skill,
    Species,
    Team,
    Emote,
    Transformation,
    Unknown(u32),
}

impl ContentType {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => ContentType::Effect,
            1 => ContentType::Marker,
            2 => ContentType::Skill,
            3 => ContentType::Species,
            4 => ContentType::Team,
            5 => ContentType::Emote,
            6 => ContentType::Transformation,
            other => ContentType::Unknown(other),
        }
    }
}

/// One `CBTS_IDTOGUID` mapping: a session-local content id -> stable
/// 16-byte content GUID. `local_id` is the arcdps-assigned id used
/// elsewhere in the log for this content (e.g. a WvW team id carried on
/// `TEAM_CHANGE` events, or a skill id); `guid` is the id's stable
/// cross-log/cross-session identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidMapping {
    pub content_type: ContentType,
    pub local_id: u32,
    pub guid: [u8; 16],
}

impl GuidMapping {
    /// Lowercase hex encoding of the 16-byte GUID (no dashes), e.g. for
    /// JSON schema output.
    pub fn guid_hex(&self) -> String {
        let mut s = String::with_capacity(32);
        for b in &self.guid {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// Decode all `CBTS_IDTOGUID` (sc=46) events into `GuidMapping` entries.
///
/// Payload, per the arcdps EVTC reference:
/// ```text
/// CBTS_IDTOGUID
/// // src_agent: (uint8_t*)&src_agent is uint8_t[16] guid of content
/// // overstack_value: is of enum contentlocal
/// ```
/// `src_agent` and `dst_agent` are adjacent `u64` fields in the raw
/// `cbtevent` struct, so the 16-byte GUID spans both:
/// `src_agent_bytes ++ dst_agent_bytes` (little-endian, since that's the
/// struct's native in-memory layout on the x86/x64 arcdps host). Verified
/// against GW2EI's `GUID(ulong first8, ulong last8)` (`GW2EIEvtcParser/
/// GUID.cs`) and `IDToGUIDEvent` (`GUID = new(evtcItem.SrcAgent,
/// evtcItem.DstAgent)`), which construct the same 16 bytes the same way.
///
/// The content-local id being mapped (e.g. `TeamGUIDEvent.TeamID`) is
/// *not* in the README's two documented fields — cross-checking GW2EI's
/// `IDToGUIDEvent` (`ContentID = evtcItem.SkillID`) shows it's carried in
/// `skillid`.
pub fn decode_guid_mappings(events: &[RawEvent]) -> Vec<GuidMapping> {
    events
        .iter()
        .filter(|e| e.is_statechange == sc::ID_TO_GUID)
        .map(|e| {
            let mut guid = [0u8; 16];
            guid[0..8].copy_from_slice(&e.src_agent.to_le_bytes());
            guid[8..16].copy_from_slice(&e.dst_agent.to_le_bytes());
            GuidMapping { content_type: ContentType::from_u32(e.overstack), local_id: e.skillid, guid }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic CBTS_IDTOGUID event: src_agent/dst_agent hold a
    /// known 16-byte pattern, overstack=content type, skillid=local id.
    fn idtoguid_event(content_type: u32, local_id: u32, guid_bytes: [u8; 16]) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: u64::from_le_bytes(guid_bytes[0..8].try_into().unwrap()),
            dst_agent: u64::from_le_bytes(guid_bytes[8..16].try_into().unwrap()),
            value: 0,
            buff_dmg: 0,
            overstack: content_type,
            skillid: local_id,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_statechange: sc::ID_TO_GUID,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    #[test]
    fn decodes_team_guid_mapping() {
        let guid_bytes: [u8; 16] = [
            0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
            0xaa, 0xbb,
        ];
        let events = vec![idtoguid_event(4, 2767, guid_bytes)];
        let mappings = decode_guid_mappings(&events);
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].content_type, ContentType::Team);
        assert_eq!(mappings[0].local_id, 2767);
        assert_eq!(mappings[0].guid, guid_bytes);
        assert_eq!(mappings[0].guid_hex(), "deadbeef00112233445566778899aabb");
        assert_eq!(mappings[0].guid_hex().len(), 32);
    }

    #[test]
    fn ignores_non_idtoguid_events() {
        let mut e = idtoguid_event(2, 42, [0u8; 16]);
        e.is_statechange = sc::TEAM_CHANGE; // not IDTOGUID
        assert!(decode_guid_mappings(&[e]).is_empty());
    }

    #[test]
    fn maps_all_content_types_by_position() {
        for (v, expected) in [
            (0u32, ContentType::Effect),
            (1, ContentType::Marker),
            (2, ContentType::Skill),
            (3, ContentType::Species),
            (4, ContentType::Team),
            (5, ContentType::Emote),
            (6, ContentType::Transformation),
        ] {
            assert_eq!(ContentType::from_u32(v), expected);
        }
        assert_eq!(ContentType::from_u32(99), ContentType::Unknown(99));
    }
}
