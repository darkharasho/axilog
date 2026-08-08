//! PII-safe anonymization of a decoded (already-inflated) raw EVTC buffer,
//! plus a minimal valid-zip writer to re-package the result as a `.zevtc`.
//!
//! M2 Task 5: axilog's own golden fixture (`fixtures/local/wvw-small.zevtc`)
//! contains real GW2 account/character names and must never be committed.
//! This module rewrites every PLAYER agent's name buffer to a deterministic
//! `Anon<N>` placeholder so a byte-identical-except-for-names copy can be
//! committed and run in CI (see `fixtures/wvw-small.anon.zevtc`).
//!
//! Only the 64-byte name buffer (character\0account\0subgroup\0<padding>) at
//! offset 28 of each PLAYER agent record is touched; every other byte in the
//! file — including non-player agents, skills, and every combat event — is
//! left exactly as-is. In particular, no metric-bearing field is touched, so
//! `analysis::analyze` on the anonymized file must produce numbers identical
//! to the original (verified by `tests/anonymize.rs`).

use super::{EvtcError, AGENT_SIZE, HEADER_SIZE};
use flate2::Crc;

const NAME_BUF_OFFSET: usize = 28;
const NAME_BUF_LEN: usize = 64;
const IS_ELITE_OFFSET: usize = 12;
const PLAYER_SENTINEL: u32 = 0xffff_ffff;

/// Deterministic 4-digit numeric suffix for the anonymized account, derived
/// purely from the agent-table index `n` (stable across runs/machines).
fn anon_digits(n: usize) -> u32 {
    1000 + ((n as u32).wrapping_mul(37)) % 9000
}

/// `Anon<N>` character-name placeholder for agent-table index `n`.
pub fn anon_character(n: usize) -> String {
    format!("Anon{n}")
}

/// `:Anon<N>.<4 digits>` account placeholder for agent-table index `n`
/// (leading colon matches the raw arcdps account-name convention decoded by
/// `RawAgent::name_parts`).
pub fn anon_account(n: usize) -> String {
    format!(":Anon{n}.{:04}", anon_digits(n))
}

/// Rewrites every PLAYER agent's (`is_elite != 0xffffffff`) 64-byte name
/// buffer in `data` (an already-inflated raw EVTC buffer, i.e. the output of
/// `inflate_zevtc`) to `Anon<N>` / `:Anon<N>.<NNNN>`, where `N` is the
/// agent's index in the agent table. The subgroup field (third
/// null-separated component of the name buffer) is preserved exactly;
/// all bytes outside the 64-byte name buffers of PLAYER agents are left
/// byte-identical. Returns the number of PLAYER agents rewritten.
pub fn anonymize_raw_evtc(data: &mut [u8]) -> Result<usize, EvtcError> {
    let read_u32 = |d: &[u8], off: usize| -> Result<u32, EvtcError> {
        d.get(off..off + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .ok_or(EvtcError::Truncated { need: off + 4, at: off, have: d.len() })
    };
    let agent_count = read_u32(data, HEADER_SIZE)? as usize;
    let agents_start = HEADER_SIZE + 4;
    let need = agents_start
        .checked_add(agent_count.checked_mul(AGENT_SIZE).ok_or_else(|| {
            EvtcError::Container("agent table size overflow".into())
        })?)
        .ok_or_else(|| EvtcError::Container("agent table offset overflow".into()))?;
    if data.len() < need {
        return Err(EvtcError::Truncated { need, at: agents_start, have: data.len() });
    }

    let mut anonymized = 0usize;
    for i in 0..agent_count {
        let agent_off = agents_start + i * AGENT_SIZE;
        let is_elite = read_u32(data, agent_off + IS_ELITE_OFFSET)?;
        if is_elite == PLAYER_SENTINEL {
            continue; // not a player (npc/gadget/etc.)
        }

        let name_off = agent_off + NAME_BUF_OFFSET;
        let name_buf = &data[name_off..name_off + NAME_BUF_LEN];

        // Preserve the subgroup field (3rd null-separated token, same
        // tokenization `RawAgent::name_parts` uses) verbatim; character/
        // account (1st/2nd tokens) are replaced. Note: unlike `splitn(3,
        // ..)`, a full `split` stops the subgroup token at its own null
        // terminator instead of swallowing the rest of the zero-padded
        // buffer into it.
        let mut parts = name_buf.split(|&b| b == 0);
        let _old_character = parts.next().unwrap_or(&[]);
        let _old_account = parts.next().unwrap_or(&[]);
        let subgroup = parts.next().unwrap_or(&[]);

        let character = anon_character(i);
        let account = anon_account(i);

        let needed = character.len() + 1 + account.len() + 1 + subgroup.len() + 1;
        if needed > NAME_BUF_LEN {
            return Err(EvtcError::Container(format!(
                "anonymized name buffer for agent {i} overflows {NAME_BUF_LEN} bytes ({needed} needed)"
            )));
        }

        let mut new_buf = [0u8; NAME_BUF_LEN];
        let mut w = 0usize;
        new_buf[w..w + character.len()].copy_from_slice(character.as_bytes());
        w += character.len() + 1; // + null terminator (already zero)
        new_buf[w..w + account.len()].copy_from_slice(account.as_bytes());
        w += account.len() + 1; // + null terminator (already zero)
        new_buf[w..w + subgroup.len()].copy_from_slice(subgroup);
        // remaining bytes (including the subgroup's null terminator and any
        // trailing padding) are already zero in `new_buf`.

        data[name_off..name_off + NAME_BUF_LEN].copy_from_slice(&new_buf);
        anonymized += 1;
    }
    Ok(anonymized)
}

/// Writes `data` as the single "stored" (uncompressed, method 0) entry
/// `entry_name` of a minimal-but-valid zip archive: local file header +
/// data + central directory + end-of-central-directory record, with a real
/// CRC-32 (so any standard zip tool, not just `axilog_core::evtc::inflate_zevtc`,
/// can open it).
pub fn zip_stored(entry_name: &str, data: &[u8]) -> Vec<u8> {
    zip_write(entry_name, data, 0, data)
}

/// Same as [`zip_stored`], but deflates `data` (method 8) before writing —
/// keeps large `.zevtc` fixtures (mostly combat-event bytes, which compress
/// well) small enough to commit to git.
pub fn zip_deflate(entry_name: &str, data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(data).expect("write to in-memory deflate encoder");
    let compressed = enc.finish().expect("finish in-memory deflate encoder");
    zip_write(entry_name, data, 8, &compressed)
}

/// Shared zip-archive writer: `data` is the original (uncompressed) bytes
/// (used for the CRC-32, which is always computed over uncompressed data),
/// `stored` is what actually gets written into the entry (identical to
/// `data` for `method == 0`, deflated for `method == 8`).
fn zip_write(entry_name: &str, data: &[u8], method: u16, stored: &[u8]) -> Vec<u8> {
    let mut crc = Crc::new();
    crc.update(data);
    let crc32 = crc.sum();

    let name = entry_name.as_bytes();
    let usize_ = data.len() as u32;
    let csize = stored.len() as u32;

    let mut out = Vec::with_capacity(stored.len() + name.len() * 2 + 128);
    let local_header_offset = out.len() as u32;

    // Local file header
    out.extend_from_slice(b"PK\x03\x04");
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // mod time+date
    out.extend_from_slice(&crc32.to_le_bytes());
    out.extend_from_slice(&csize.to_le_bytes());
    out.extend_from_slice(&usize_.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extralen
    out.extend_from_slice(name);
    out.extend_from_slice(stored);

    let central_dir_offset = out.len() as u32;

    // Central directory header
    out.extend_from_slice(b"PK\x01\x02");
    out.extend_from_slice(&20u16.to_le_bytes()); // version made by
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // mod time+date
    out.extend_from_slice(&crc32.to_le_bytes());
    out.extend_from_slice(&csize.to_le_bytes());
    out.extend_from_slice(&usize_.to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra len
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number start
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    out.extend_from_slice(&local_header_offset.to_le_bytes());
    out.extend_from_slice(name);

    let central_dir_size = (out.len() as u32) - central_dir_offset;

    // End of central directory record
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with central dir
    out.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    out.extend_from_slice(&1u16.to_le_bytes()); // total entries
    out.extend_from_slice(&central_dir_size.to_le_bytes());
    out.extend_from_slice(&central_dir_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{decode_agents, inflate_zevtc};

    fn player_agent(addr: u64, is_elite: u32, name: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; AGENT_SIZE];
        b[0..8].copy_from_slice(&addr.to_le_bytes());
        b[12..16].copy_from_slice(&is_elite.to_le_bytes());
        b[28..28 + name.len()].copy_from_slice(name);
        b
    }

    fn npc_agent(addr: u64) -> Vec<u8> {
        let mut b = vec![0u8; AGENT_SIZE];
        b[0..8].copy_from_slice(&addr.to_le_bytes());
        b[12..16].copy_from_slice(&PLAYER_SENTINEL.to_le_bytes());
        b[28..28 + 6].copy_from_slice(b"Boss\0\0");
        b
    }

    fn synthetic_raw(agents: &[Vec<u8>]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"EVTC20260114");
        b.push(1); // revision
        b.extend_from_slice(&1u16.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&(agents.len() as u32).to_le_bytes());
        for a in agents {
            b.extend_from_slice(a);
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // skill_count = 0
        b
    }

    #[test]
    fn rewrites_player_names_preserves_subgroup_and_npc() {
        let mut data = synthetic_raw(&[
            npc_agent(0xB0),
            player_agent(0x1, 27, b"Alice\0:Alice.1234\05\0"),
            player_agent(0x2, 5, b"Bob\0:Bob.9999\012\0"),
        ]);
        let agents_start_pre = HEADER_SIZE + 4;
        let before_npc = data[agents_start_pre..agents_start_pre + AGENT_SIZE].to_vec(); // agent 0's full record

        let n = anonymize_raw_evtc(&mut data).unwrap();
        assert_eq!(n, 2);

        // NPC (index 0) untouched byte-for-byte.
        let agents_start = HEADER_SIZE + 4;
        assert_eq!(&data[agents_start..agents_start + AGENT_SIZE], &before_npc[..]);

        let agent_count = 3;
        let agents = decode_agents(&data[agents_start..], agent_count).unwrap();
        let (c0, a0, s0) = agents[1].name_parts();
        assert_eq!(c0, "Anon1");
        assert_eq!(a0, ":Anon1.1037"); // 1000 + (1*37)%9000 = 1037
        assert_eq!(s0, Some(5));

        let (c1, a1, s1) = agents[2].name_parts();
        assert_eq!(c1, "Anon2");
        assert_eq!(a1, ":Anon2.1074"); // 1000 + (2*37)%9000 = 1074
        assert_eq!(s1, Some(12));
    }

    #[test]
    fn zip_stored_roundtrips_through_inflate_zevtc() {
        let payload = b"EVTCfake-payload-bytes".to_vec();
        let zipped = zip_stored("log.evtc", &payload);
        assert_eq!(&zipped[0..2], b"PK");
        let out = inflate_zevtc(&zipped).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn zip_deflate_roundtrips_and_is_smaller() {
        // Repeated bytes compress well, so deflate output should shrink.
        let payload: Vec<u8> = std::iter::repeat(b'A').take(4096).collect();
        let zipped = zip_deflate("log.evtc", &payload);
        assert_eq!(&zipped[0..2], b"PK");
        assert!(zipped.len() < payload.len());
        let out = inflate_zevtc(&zipped).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn errors_on_truncated_agent_table() {
        let mut data = synthetic_raw(&[player_agent(0x1, 27, b"Alice\0:Alice.1234\05\0")]);
        data.truncate(data.len() - 10); // chop into the agent table
        assert!(anonymize_raw_evtc(&mut data).is_err());
    }
}
