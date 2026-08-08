use super::{
    decode_agents, decode_events, decode_header, decode_skills, EvtcError, RawLog, AGENT_SIZE,
    EVENT_SIZE_REV1, HEADER_SIZE, SKILL_SIZE,
};
use std::io::Read;

/// zevtc is a zip whose single entry is the raw EVTC; some tools emit bare deflate.
/// If the buffer already starts with "EVTC", it is raw — return as-is.
pub fn inflate_zevtc(bytes: &[u8]) -> Result<Vec<u8>, EvtcError> {
    if bytes.len() >= 4 && &bytes[0..4] == b"EVTC" {
        return Ok(bytes.to_vec());
    }
    if bytes.len() >= 2 && &bytes[0..2] == b"PK" {
        let reader = std::io::Cursor::new(bytes);
        let zip = zip_read(reader)?;
        return Ok(zip);
    }
    // fallback: raw deflate stream
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| EvtcError::Container(e.to_string()))?;
    Ok(out)
}

// Minimal zip: read the first local file entry, deflate-inflate its data.
fn zip_read(mut cur: std::io::Cursor<&[u8]>) -> Result<Vec<u8>, EvtcError> {
    use std::io::{Seek, SeekFrom};
    let b = *cur.get_ref();
    // local file header: sig(4) ver(2) flag(2) method(2) modtime(4) crc(4)
    // csize(4) usize(4) namelen(2) extralen(2)
    if b.len() < 30 || &b[0..4] != b"PK\x03\x04" {
        return Err(EvtcError::Container("bad zip".into()));
    }
    let method = u16::from_le_bytes([b[8], b[9]]);
    let csize = u32::from_le_bytes([b[18], b[19], b[20], b[21]]) as usize;
    let namelen = u16::from_le_bytes([b[26], b[27]]) as usize;
    let extralen = u16::from_le_bytes([b[28], b[29]]) as usize;
    let data_start = 30 + namelen + extralen;
    let data = &b[data_start..data_start + csize];
    let _ = cur.seek(SeekFrom::Start(0));
    match method {
        0 => Ok(data.to_vec()), // stored
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| EvtcError::Container(e.to_string()))?;
            Ok(out)
        }
        m => Err(EvtcError::Container(format!("unsupported zip method {m}"))),
    }
}

pub fn decode_raw(bytes: &[u8]) -> Result<RawLog, EvtcError> {
    let data = inflate_zevtc(bytes)?;
    let header = decode_header(&data)?;
    if header.revision != 1 {
        return Err(EvtcError::UnsupportedRevision(header.revision));
    }
    let read_u32 = |off: usize| -> Result<u32, EvtcError> {
        data.get(off..off + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .ok_or(EvtcError::Truncated { need: off + 4, at: off, have: data.len() })
    };
    let mut off = HEADER_SIZE;
    let agent_count = read_u32(off)? as usize;
    off += 4;
    let agents = decode_agents(&data[off..], agent_count)?;
    off += agent_count * AGENT_SIZE;
    let skill_count = read_u32(off)? as usize;
    off += 4;
    let skills = decode_skills(&data[off..], skill_count)?;
    off += skill_count * SKILL_SIZE;
    let remaining = data.len() - off;
    let event_count = remaining / EVENT_SIZE_REV1;
    let events = decode_events(&data[off..], event_count)?;
    Ok(RawLog { header, agents, skills, events })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passthrough_raw_evtc() {
        // A raw buffer already starting with EVTC is returned unchanged.
        let mut b = Vec::new();
        b.extend_from_slice(b"EVTC20260114");
        b.push(1);
        b.extend_from_slice(&1u16.to_le_bytes());
        b.push(0);
        let out = inflate_zevtc(&b).unwrap();
        assert_eq!(&out[0..4], b"EVTC");
    }
}
