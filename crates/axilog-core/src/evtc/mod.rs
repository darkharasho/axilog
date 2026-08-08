pub mod header;
pub use header::{decode_header, RawHeader};

pub const HEADER_SIZE: usize = 16;
pub const AGENT_SIZE:  usize = 96;
pub const SKILL_SIZE:  usize = 68;
pub const EVENT_SIZE_REV1: usize = 96;

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
