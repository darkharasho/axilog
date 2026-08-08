use super::{EvtcError, SKILL_SIZE};

#[derive(Debug, Clone)]
pub struct RawSkill {
    pub id: u32,
    pub name: String,
}

pub fn decode_skills(buf: &[u8], count: usize) -> Result<Vec<RawSkill>, EvtcError> {
    let need = count * SKILL_SIZE;
    if buf.len() < need {
        return Err(EvtcError::Truncated {
            need,
            at: 0,
            have: buf.len(),
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let s = &buf[i * SKILL_SIZE..(i + 1) * SKILL_SIZE];
        let id = u32::from_le_bytes(s[0..4].try_into().unwrap());
        let name = String::from_utf8_lossy(&s[4..SKILL_SIZE])
            .trim_end_matches('\0')
            .to_string();
        out.push(RawSkill { id, name });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_skill() -> Vec<u8> {
        let mut b = vec![0u8; SKILL_SIZE];
        b[0..4].copy_from_slice(&12345u32.to_le_bytes());
        let name = b"Fireball";
        b[4..4 + name.len()].copy_from_slice(name);
        b
    }

    #[test]
    fn decodes_skill() {
        let s = decode_skills(&one_skill(), 1).unwrap();
        assert_eq!(s[0].id, 12345);
        assert_eq!(s[0].name, "Fireball");
    }
}
