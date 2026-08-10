//! Guild GUID decoding from `CBTS_GUILD` (MEIGAP Task 3c) -- GW2EI's
//! `players[].guildID`.
//!
//! ## It is on the wire, not an API lookup
//!
//! GW2EI builds the field at
//! `GW2EIBuilders/JsonModels/JsonActors/JsonPlayerBuilder.cs:46-50`:
//!
//! ```csharp
//! GuildEvent? guildEvent = log.CombatData.GetGuildEvents(player.AgentItem).FirstOrDefault();
//! if (guildEvent != null) { jsonPlayer.GuildID = guildEvent.APIString; }
//! ```
//!
//! and `GuildEvent`
//! (`GW2EIEvtcParser/ParsedData/CombatEvents/MetaDataEvents/GuildEvent.cs`)
//! assembles the 16-byte guild GUID purely from the event's own bytes --
//! `dst_agent` (8 bytes), `value` (4) and `buff_dmg` (4). No GW2 API call,
//! no external data. So this is fully derivable here.
//!
//! `APIString` returns `""` when the event was anonymized
//! (`GuildEvent.Anonymize`, which GW2EI applies under its own anonymous
//! mode); this decoder has no such mode, and the all-zero GUID that a
//! guildless player reports is passed through verbatim, exactly as EI does
//! (the reference export carries
//! `"00000000-0000-0000-0000-000000000000"` for unrepresented players).
//!
//! ## The byte permutation, transcribed
//!
//! `GuildEvent`'s ctor is deliberately NOT a straight copy -- it undoes
//! arcdps' mixed-endian packing:
//!
//! ```csharp
//! *(UInt32*)guid = BinaryPrimitives.ReverseEndianness(*(UInt32*)dstAgent);
//! guid[4] = dstAgent[5];  guid[5] = dstAgent[4];
//! guid[6] = dstAgent[7];  guid[7] = dstAgent[6];
//! *(Int32*)(guid + 8)  = evtcItem.Value;
//! *(Int32*)(guid + 12) = evtcItem.BuffDmg;
//! ```
//!
//! with `dstAgent` the LITTLE-endian bytes of the `u64` `dst_agent` field
//! (that is what `(byte*)&dstAgent_` yields on the x86/ARM targets GW2EI
//! and this crate both run on). Rendered as
//! `8-4-4-4-12` uppercase hex from guid bytes `[0..4] [4..6] [6..8]
//! [8..10] [10..16]` (`ParserHelper.AppendHexString` emits uppercase).
//!
//! ## Which event wins
//!
//! GW2EI takes `FirstOrDefault()` over that agent's guild events in log
//! order -- so the FIRST one observed, not the last. Reproduced.
//!
//! ## Privacy
//!
//! A guild GUID identifies a guild, not a person, and is the same value
//! GW2EI itself publishes in every dps.report permalink. It is emitted
//! only for squad players, i.e. exactly the set whose account names the
//! payload already carries. No new PII class.

use crate::evtc::{sc, RawEvent};
use std::collections::BTreeMap;

/// `CBTS_GUILD`. Index 29 in arcdps' `enum cbtstatechange` (counted from
/// `CBTS_COMBAT = 0`), cross-checked against GW2EI's
/// `ArcDPSEnums.StateChange.Guild = 29` -- the same value
/// `evtc::repair::src_is_agent` already lists.
pub const GUILD_STATECHANGE: u8 = 29;

/// Assemble one `CBTS_GUILD` row's 16-byte GUID into GW2EI's
/// `APIString` rendering (uppercase, dash-separated 8-4-4-4-12).
pub fn decode_guild_guid(e: &RawEvent) -> String {
    let d = e.dst_agent.to_le_bytes();
    let mut guid = [0u8; 16];
    // `ReverseEndianness(*(UInt32*)dstAgent)`: source indices 3,2,1,0.
    guid[0] = d[3];
    guid[1] = d[2];
    guid[2] = d[1];
    guid[3] = d[0];
    guid[4] = d[5];
    guid[5] = d[4];
    guid[6] = d[7];
    guid[7] = d[6];
    guid[8..12].copy_from_slice(&e.value.to_le_bytes());
    guid[12..16].copy_from_slice(&e.buff_dmg.to_le_bytes());

    let hex = |bytes: &[u8]| -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()
    };
    format!(
        "{}-{}-{}-{}-{}",
        hex(&guid[0..4]),
        hex(&guid[4..6]),
        hex(&guid[6..8]),
        hex(&guid[8..10]),
        hex(&guid[10..16])
    )
}

/// Record `e` into `out` if it is a `CBTS_GUILD` row, keeping the FIRST
/// row per agent (GW2EI's `FirstOrDefault`). Returns whether the row was a
/// guild row, so a caller sharing its event loop can `continue`.
///
/// This is the ONLY guild-collection implementation in the crate. It is a
/// per-event helper rather than a whole-stream pass because the live caller
/// is `wvw::markers::resolve_markers_and_guilds`, which folds guild rows
/// into the marker scan it already runs: a standalone
/// `raw.events.iter()` pass measured +12% on fixture `model::resolve` and
/// +18% on the real log for a single `u8` compare per event (see
/// `docs/BENCHMARKS.md`). Keeping one implementation, called from that
/// loop, is what stops the decode and its tests from drifting apart.
pub fn collect_guild_event(e: &RawEvent, out: &mut BTreeMap<u64, String>) -> bool {
    if e.is_statechange != GUILD_STATECHANGE {
        return false;
    }
    out.entry(e.src_agent).or_insert_with(|| decode_guild_guid(e));
    true
}

/// Keeps `sc`'s named constants honest: this module's own constant must
/// not drift from the statechange namespace the rest of the crate uses.
const _: () = assert!(GUILD_STATECHANGE != sc::NONE);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;

    fn ev(src: u64, dst: u64, value: i32, buff_dmg: i32) -> RawEvent {
        RawEvent {
            time: 0, src_agent: src, dst_agent: dst, value, buff_dmg, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: GUILD_STATECHANGE, is_flanking: 0, is_shields: 0,
            is_offcycle: 0, pad: 0,
        }
    }

    /// The permutation, checked against `GuildEvent`'s own worked example
    /// comment (`8f55c4ee-09cc-4b0d-896c-d81e58be0042`), reconstructed
    /// backwards: pick the wire bytes that MUST produce that GUID and
    /// assert the decoder produces it.
    #[test]
    fn decodes_gw2ei_worked_example() {
        // guid[0..4] = 8f 55 c4 ee  <- d[3],d[2],d[1],d[0]  => d[0..4] = ee c4 55 8f
        // guid[4..6] = 09 cc        <- d[5],d[4]            => d[4]=cc, d[5]=09
        // guid[6..8] = 4b 0d        <- d[7],d[6]            => d[6]=0d, d[7]=4b
        let d = [0xee, 0xc4, 0x55, 0x8f, 0xcc, 0x09, 0x0d, 0x4b];
        let dst = u64::from_le_bytes(d);
        // guid[8..12]  = 89 6c d8 1e  (value, little-endian)
        let value = i32::from_le_bytes([0x89, 0x6c, 0xd8, 0x1e]);
        // guid[12..16] = 58 be 00 42  (buff_dmg, little-endian)
        let buff_dmg = i32::from_le_bytes([0x58, 0xbe, 0x00, 0x42]);
        assert_eq!(
            decode_guild_guid(&ev(1, dst, value, buff_dmg)),
            "8F55C4EE-09CC-4B0D-896C-D81E58BE0042"
        );
    }

    /// A guildless player reports the all-zero GUID, and it is passed
    /// through rather than turned into `None` -- the reference export does
    /// exactly the same.
    #[test]
    fn all_zero_guid_is_passed_through() {
        assert_eq!(
            decode_guild_guid(&ev(1, 0, 0, 0)),
            "00000000-0000-0000-0000-000000000000"
        );
    }

    /// GW2EI takes `FirstOrDefault()`, so a later re-report never wins.
    #[test]
    fn first_event_per_agent_wins() {
        let mut g = BTreeMap::new();
        for e in [ev(7, 1, 0, 0), ev(7, 2, 0, 0)] {
            assert!(collect_guild_event(&e, &mut g));
        }
        assert_eq!(g.len(), 1);
        assert_eq!(g[&7], decode_guild_guid(&ev(7, 1, 0, 0)));
    }

    /// Non-guild statechanges contribute nothing, and the helper reports
    /// that it did not consume them (so the shared marker loop falls
    /// through to its own handling).
    #[test]
    fn ignores_other_statechanges() {
        let mut other = ev(7, 1, 0, 0);
        other.is_statechange = crate::evtc::sc::TEAM_CHANGE;
        let mut g = BTreeMap::new();
        assert!(!collect_guild_event(&other, &mut g));
        assert!(g.is_empty());
    }
}
