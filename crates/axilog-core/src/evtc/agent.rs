use super::{EvtcError, AGENT_SIZE};

#[derive(Debug, Clone)]
pub struct RawAgent {
    pub addr: u64,
    pub prof: u32,
    pub is_elite: u32,
    pub toughness: i16,
    pub concentration: i16,
    pub healing: i16,
    pub hitbox_width: u16,
    pub condition: i16,
    pub hitbox_height: u16,
    pub name_raw: Vec<u8>,
}

impl RawAgent {
    /// name buffer = character \0 account \0 subgroup \0 (utf8, null-separated)
    pub fn name_parts(&self) -> (String, String, Option<u8>) {
        let mut it = self
            .name_raw
            .split(|&c| c == 0)
            .map(|s| String::from_utf8_lossy(s).to_string());
        let character = it.next().unwrap_or_default();
        let account = it.next().unwrap_or_default();
        let subgroup = it.next().and_then(|s| s.trim().parse::<u8>().ok());
        (character, account, subgroup)
    }

    pub fn is_player(&self) -> bool {
        self.is_elite != 0xffff_ffff
    }
}

pub fn decode_agents(buf: &[u8], count: usize) -> Result<Vec<RawAgent>, EvtcError> {
    let need = count * AGENT_SIZE;
    if buf.len() < need {
        return Err(EvtcError::Truncated {
            need,
            at: 0,
            have: buf.len(),
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let a = &buf[i * AGENT_SIZE..(i + 1) * AGENT_SIZE];
        let name_end = 28 + 64; // 64-byte name buffer at offset 28
        out.push(RawAgent {
            addr: u64::from_le_bytes(a[0..8].try_into().unwrap()),
            prof: u32::from_le_bytes(a[8..12].try_into().unwrap()),
            is_elite: u32::from_le_bytes(a[12..16].try_into().unwrap()),
            toughness: i16::from_le_bytes(a[16..18].try_into().unwrap()),
            concentration: i16::from_le_bytes(a[18..20].try_into().unwrap()),
            healing: i16::from_le_bytes(a[20..22].try_into().unwrap()),
            hitbox_width: u16::from_le_bytes(a[22..24].try_into().unwrap()),
            condition: i16::from_le_bytes(a[24..26].try_into().unwrap()),
            hitbox_height: u16::from_le_bytes(a[26..28].try_into().unwrap()),
            name_raw: a[28..name_end].to_vec(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player_agent() -> Vec<u8> {
        let mut b = vec![0u8; AGENT_SIZE];
        b[0..8].copy_from_slice(&0x1122u64.to_le_bytes()); // addr
        b[8..12].copy_from_slice(&5u32.to_le_bytes()); // prof (guardian)
        b[12..16].copy_from_slice(&27u32.to_le_bytes()); // is_elite (firebrand)
                                                          // name combo at offset 28: char \0 account \0 subgroup \0
        let name = b"Alice\x00:Alice.1234\x005\x00";
        b[28..28 + name.len()].copy_from_slice(name);
        b
    }

    #[test]
    fn decodes_one_player_agent() {
        let agents = decode_agents(&player_agent(), 1).unwrap();
        assert_eq!(agents.len(), 1);
        let (character, account, sub) = agents[0].name_parts();
        assert_eq!(character, "Alice");
        assert_eq!(account, ":Alice.1234");
        assert_eq!(sub, Some(5));
        assert_eq!(agents[0].prof, 5);
        assert_eq!(agents[0].is_elite, 27);
    }

    #[test]
    fn errors_when_count_exceeds_buffer() {
        assert!(decode_agents(&player_agent(), 2).is_err());
    }
}
