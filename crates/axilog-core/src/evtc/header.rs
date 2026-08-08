use super::{EvtcError, HEADER_SIZE};

#[derive(Debug, Clone)]
pub struct RawHeader { pub build: String, pub revision: u8, pub boss_id: u16 }

pub fn decode_header(buf: &[u8]) -> Result<RawHeader, EvtcError> {
    if buf.len() < HEADER_SIZE {
        return Err(EvtcError::Truncated { need: HEADER_SIZE, at: 0, have: buf.len() });
    }
    if &buf[0..4] != b"EVTC" { return Err(EvtcError::BadMagic); }
    let build = String::from_utf8_lossy(&buf[4..12]).trim_end_matches('\0').to_string();
    let revision = buf[12];
    let boss_id = u16::from_le_bytes([buf[13], buf[14]]);
    Ok(RawHeader { build, revision, boss_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        // "EVTC" + "20260114" + rev 1 + boss_id 1 (LE u16) + skip 0
        let mut b = Vec::new();
        b.extend_from_slice(b"EVTC");
        b.extend_from_slice(b"20260114");
        b.push(1);                       // revision
        b.extend_from_slice(&1u16.to_le_bytes()); // boss id
        b.push(0);                       // skip
        b
    }
    #[test]
    fn parses_header_fields() {
        let h = decode_header(&sample()).unwrap();
        assert_eq!(h.build, "20260114");
        assert_eq!(h.revision, 1);
        assert_eq!(h.boss_id, 1);
    }
    #[test]
    fn rejects_bad_magic() {
        let mut b = sample(); b[0] = b'X';
        assert!(decode_header(&b).is_err());
    }
    #[test]
    fn rejects_short_buffer() {
        assert!(decode_header(b"EVTC").is_err());
    }
}
