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
    // `data_start`/`csize` are attacker/corruption-controlled fields read
    // straight from the file; bound-check both before slicing so a
    // truncated or malformed zevtc returns an error instead of panicking
    // (Finding #2).
    let data_start = 30usize
        .checked_add(namelen)
        .and_then(|v| v.checked_add(extralen))
        .ok_or_else(|| EvtcError::Container("zip local header overflow".into()))?;
    if data_start > b.len() {
        return Err(EvtcError::Container("zip local header exceeds buffer".into()));
    }
    let data_end = data_start
        .checked_add(csize)
        .ok_or_else(|| EvtcError::Container("zip entry size overflow".into()))?;
    if data_end > b.len() {
        return Err(EvtcError::Container("zip entry data exceeds buffer".into()));
    }
    let data = &b[data_start..data_end];
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

    /// Builds a 16-byte header: "EVTC" + "20260114" + revision + boss_id (LE u16) + skip byte.
    fn build_header(revision: u8) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER_SIZE);
        b.extend_from_slice(b"EVTC20260114");
        b.push(revision);
        b.extend_from_slice(&1u16.to_le_bytes());
        b.push(0);
        assert_eq!(b.len(), HEADER_SIZE);
        b
    }

    /// Builds a synthetic raw EVTC buffer with 2 agents, 1 skill, 2 events so the
    /// offset walk in `decode_raw` can be exercised without an external fixture.
    fn build_synthetic_log() -> Vec<u8> {
        let mut b = build_header(1);

        // agent_count = 2
        b.extend_from_slice(&2u32.to_le_bytes());
        // agent 0: addr = 0x1111, is_elite = 27 (player-ish)
        let mut a0 = vec![0u8; AGENT_SIZE];
        a0[0..8].copy_from_slice(&0x1111u64.to_le_bytes());
        a0[12..16].copy_from_slice(&27u32.to_le_bytes());
        b.extend_from_slice(&a0);
        // agent 1: addr = 0x2222, is_elite = 27
        let mut a1 = vec![0u8; AGENT_SIZE];
        a1[0..8].copy_from_slice(&0x2222u64.to_le_bytes());
        a1[12..16].copy_from_slice(&27u32.to_le_bytes());
        b.extend_from_slice(&a1);

        // skill_count = 1
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut s0 = vec![0u8; SKILL_SIZE];
        s0[0..4].copy_from_slice(&12345u32.to_le_bytes());
        let name = b"Fireball";
        s0[4..4 + name.len()].copy_from_slice(name);
        b.extend_from_slice(&s0);

        // 2 events, distinct `time` fields
        let mut e0 = vec![0u8; EVENT_SIZE_REV1];
        e0[0..8].copy_from_slice(&1000u64.to_le_bytes());
        b.extend_from_slice(&e0);
        let mut e1 = vec![0u8; EVENT_SIZE_REV1];
        e1[0..8].copy_from_slice(&2000u64.to_le_bytes());
        b.extend_from_slice(&e1);

        b
    }

    #[test]
    fn decode_raw_walks_offsets_correctly() {
        let buf = build_synthetic_log();
        let raw = decode_raw(&buf).unwrap();

        assert_eq!(raw.header.revision, 1);

        assert_eq!(raw.agents.len(), 2);
        assert_eq!(raw.agents[0].addr, 0x1111);
        assert_eq!(raw.agents[0].is_elite, 27);
        assert_eq!(raw.agents[1].addr, 0x2222);
        assert_eq!(raw.agents[1].is_elite, 27);

        assert_eq!(raw.skills.len(), 1);
        assert_eq!(raw.skills[0].id, 12345);
        assert_eq!(raw.skills[0].name, "Fireball");

        assert_eq!(raw.events.len(), 2);
        assert_eq!(raw.events[0].time, 1000);
        assert_eq!(raw.events[1].time, 2000);
    }

    #[test]
    fn decode_raw_rejects_unsupported_revision() {
        let mut buf = build_header(0);
        // agent_count = 0, skill_count = 0, no events
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let err = decode_raw(&buf).unwrap_err();
        match err {
            EvtcError::UnsupportedRevision(rev) => assert_eq!(rev, 0),
            other => panic!("expected UnsupportedRevision, got {other:?}"),
        }
    }

    #[test]
    fn inflate_zevtc_bare_deflate_fallback() {
        use std::io::Write;
        let original = b"hello arcdps combat log bytes";
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(original).unwrap();
        let compressed = enc.finish().unwrap();

        // Not "EVTC" and not "PK" -> falls back to bare deflate decode.
        assert_ne!(&compressed[0..2], b"PK");
        let out = inflate_zevtc(&compressed).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn inflate_zevtc_zip_wrapped_deflate() {
        use std::io::Write;
        let original = b"raw evtc payload";
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(original).unwrap();
        let compressed = enc.finish().unwrap();

        let name = b"log.evtc";
        let mut zip = Vec::new();
        zip.extend_from_slice(b"PK\x03\x04"); // local file header signature
        zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
        zip.extend_from_slice(&0u16.to_le_bytes()); // flags
        zip.extend_from_slice(&8u16.to_le_bytes()); // method = deflate
        zip.extend_from_slice(&0u32.to_le_bytes()); // mod time+date
        zip.extend_from_slice(&0u32.to_le_bytes()); // crc32 (unchecked by our reader)
        zip.extend_from_slice(&(compressed.len() as u32).to_le_bytes()); // csize
        zip.extend_from_slice(&(original.len() as u32).to_le_bytes()); // usize
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes()); // namelen
        zip.extend_from_slice(&0u16.to_le_bytes()); // extralen
        zip.extend_from_slice(name);
        zip.extend_from_slice(&compressed);

        let out = inflate_zevtc(&zip).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn zip_read_rejects_csize_larger_than_buffer() {
        // A corrupted/truncated zevtc whose local file header claims a
        // csize bigger than the actual buffer must return Err, not panic
        // via out-of-bounds slicing (Finding #2).
        let name = b"log.evtc";
        let mut zip = Vec::new();
        zip.extend_from_slice(b"PK\x03\x04");
        zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
        zip.extend_from_slice(&0u16.to_le_bytes()); // flags
        zip.extend_from_slice(&8u16.to_le_bytes()); // method = deflate
        zip.extend_from_slice(&0u32.to_le_bytes()); // mod time+date
        zip.extend_from_slice(&0u32.to_le_bytes()); // crc32
        zip.extend_from_slice(&1_000_000u32.to_le_bytes()); // csize: way bigger than buffer
        zip.extend_from_slice(&0u32.to_le_bytes()); // usize
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes()); // namelen
        zip.extend_from_slice(&0u16.to_le_bytes()); // extralen
        zip.extend_from_slice(name);
        // No entry data at all — buffer ends right after the header+name.

        let err = inflate_zevtc(&zip).unwrap_err();
        match err {
            EvtcError::Container(_) => {}
            other => panic!("expected Container error, got {other:?}"),
        }
    }

    #[test]
    fn zip_read_rejects_header_fields_exceeding_buffer() {
        // namelen/extralen alone push data_start past the buffer end.
        let mut zip = Vec::new();
        zip.extend_from_slice(b"PK\x03\x04");
        zip.extend_from_slice(&20u16.to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(&8u16.to_le_bytes());
        zip.extend_from_slice(&0u32.to_le_bytes());
        zip.extend_from_slice(&0u32.to_le_bytes());
        zip.extend_from_slice(&0u32.to_le_bytes()); // csize
        zip.extend_from_slice(&0u32.to_le_bytes()); // usize
        zip.extend_from_slice(&60_000u16.to_le_bytes()); // namelen: far bigger than buffer
        zip.extend_from_slice(&0u16.to_le_bytes()); // extralen
        // buffer ends at exactly 30 bytes, no name bytes present

        let err = inflate_zevtc(&zip).unwrap_err();
        match err {
            EvtcError::Container(_) => {}
            other => panic!("expected Container error, got {other:?}"),
        }
    }
}
