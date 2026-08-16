pub mod agent;
pub mod anonymize;
pub mod container;
/// arcdps effect events (`CBTS_EFFECT*`), all three encodings folded into
/// one type -- see the module doc for why a purely visual event stream is
/// load-bearing here.
pub mod effect;
pub mod event;
/// arcdps healing-extension wire-format decode (M10 Task 1) -- see this
/// module's own doc comment for the signature/dispatch mechanism.
pub mod ext_healing;
pub mod guid;
pub mod header;
/// Orphaned-instid attribution repair (MATTRIB Task 1) -- GW2EI's
/// `EvtcParser.CompleteAgents` addr-0 rewrite, run as a [`decode_raw`]
/// post-pass so every consumer sees the repaired stream. See the module doc
/// for the rule-by-rule transcription.
pub mod repair;
pub mod skill;

pub use agent::{decode_agents, RawAgent};
pub use anonymize::{anon_account, anon_character, anonymize_raw_evtc, zip_deflate, zip_stored};
pub use container::{decode_raw, inflate_zevtc};
pub use effect::{EffectEvent, EffectIndex};
pub use event::{buff_remove, decode_events, iff, result, sc, RawEvent};
pub use ext_healing::{
    decode_data_event, decode_registration, healing_extension_present, RawExtHealEvent,
    Registration, HEALING_SIGNATURE,
};
pub use guid::{decode_guid_mappings, ContentType, GuidMapping};
pub use header::{decode_header, is_post_buff_rework, RawHeader};
pub use repair::{dst_is_agent, repair_orphaned_agents, src_is_agent, RepairStats};
pub use skill::{decode_skills, RawSkill};

pub const HEADER_SIZE: usize = 16;
pub const AGENT_SIZE:  usize = 96;
pub const SKILL_SIZE:  usize = 68;
// Real arcdps `cbtevent` (revision 0/1) is 64 bytes: three u64 + four i32/u32
// + four u16 + twelve u8 + 4 pad bytes = 64, already 8-byte aligned (no extra
// padding). This was previously 96, which silently misaligned every event
// after the first in real captures (decode "succeeded" but every iff/skill/
// team/etc field downstream was garbage) — found while calibrating the WvW
// friend/foe partition against the golden fixture (Task 16A).
pub const EVENT_SIZE_REV1: usize = 64;

#[derive(Debug, Clone)]
pub struct RawLog {
    pub header: RawHeader,
    pub agents: Vec<RawAgent>,
    pub skills: Vec<RawSkill>,
    pub events: Vec<RawEvent>,
    /// `CBTS_IDTOGUID` (sc=46) content-local-id -> stable-GUID associations
    /// decoded from `events` (Task 2b). Retained for TEAM (used by
    /// `wvw::apply` to attach a stable GUID to each detected team) as well
    /// as SKILL/SPECIES/EFFECT/MARKER/EMOTE/TRANSFORMATION, which are
    /// unused today but kept for M3 (stable buff/skill identity).
    pub guid_map: Vec<GuidMapping>,
}

impl RawLog {
    /// The log-start anchor: the absolute arcdps timestamp that every
    /// log-relative time in this project is measured from.
    ///
    /// This one expression -- `events.first().time`, or `0` for an empty
    /// log -- was open-coded at 16 call sites across `analysis` and `wvw`
    /// under four different local names (`t0`, `t0_ms`, `log_start`,
    /// `log_start_ms`), which is what made the duplication invisible: the
    /// two sites the roadmap happened to name (`distance.rs`/`replay.rs`)
    /// were not a pair, they were 2 of 16. Centralised 2026-08-16.
    ///
    /// Deliberately NOT `header.log_start` or any encounter-start notion:
    /// this is the first event in the file, which is what the whole
    /// timeline convention is anchored to (`analysis::rotation`,
    /// `analysis::timeseries`, `analysis::replay` and `buffs` all document
    /// the same `t0`). The `unwrap_or(0)` branch is reachable only for a
    /// log with no events at all, where every relative time is trivially
    /// `0` and any anchor would do.
    ///
    /// Note the two lookalikes that are NOT this: `buffs::generation` and
    /// `buffs::simulator` each start a clock from `events.first()` on a
    /// FILTERED local slice, not on `self.events`. Those are per-series
    /// start times and must not be folded in here.
    pub fn log_start_ms(&self) -> u64 {
        self.events.first().map(|e| e.time).unwrap_or(0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EvtcError {
    #[error("buffer too short: need {need} bytes at offset {at}, have {have}")]
    Truncated { need: usize, at: usize, have: usize },
    #[error("not an evtc file: bad magic")]
    BadMagic,
    #[error("unsupported evtc revision {0} (only revision 1 is supported)")]
    UnsupportedRevision(u8),
    #[error("zevtc container error: {0}")]
    Container(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::header::RawHeader;

    fn at(time: u64) -> RawEvent {
        RawEvent {
            time,
            src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0,
            pad: 0,
        }
    }

    fn log(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    #[test]
    fn log_start_is_the_first_events_time_not_the_smallest() {
        // arcdps writes events in capture order, so `first()` IS the
        // anchor -- but this must stay a positional read, not a `min()`.
        // The 16 call sites this method replaced all relied on that, and a
        // "safer" min() would silently re-anchor every relative time in the
        // project if a log ever carried an out-of-order row.
        assert_eq!(log(vec![at(33847418), at(33847000), at(33848000)]).log_start_ms(), 33847418);
    }

    #[test]
    fn log_start_of_an_empty_log_is_zero() {
        assert_eq!(log(vec![]).log_start_ms(), 0);
    }
}
