pub mod agent;
pub mod anonymize;
pub mod container;
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
