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
        // is_buffremove@52, is_statechange@56
        b[50] = result::CRIT; // result
        b[56] = sc::ENTER_COMBAT; // is_statechange
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
    }
}
