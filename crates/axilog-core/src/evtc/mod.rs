pub mod agent;
pub mod container;
pub mod event;
pub mod header;
pub mod skill;

pub use agent::{decode_agents, RawAgent};
pub use container::{decode_raw, inflate_zevtc};
pub use event::{decode_events, result, sc, RawEvent};
pub use header::{decode_header, RawHeader};
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
