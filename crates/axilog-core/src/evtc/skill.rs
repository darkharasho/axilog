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

    #[test]
    fn errors_when_count_exceeds_buffer() {
        let buf = vec![0u8; SKILL_SIZE];
        let result = decode_skills(&buf, 2);
        assert!(result.is_err());
        match result.unwrap_err() {
            EvtcError::Truncated {
                need,
                at: 0,
                have,
            } => {
                assert_eq!(need, 2 * SKILL_SIZE);
                assert_eq!(have, SKILL_SIZE);
            }
            _ => panic!("expected Truncated error"),
        }
    }

    #[test]
    fn decodes_multiple_skills() {
        let mut buf = vec![0u8; 2 * SKILL_SIZE];

        // First skill: id=100, name="Skill1"
        buf[0..4].copy_from_slice(&100u32.to_le_bytes());
        let name1 = b"Skill1";
        buf[4..4 + name1.len()].copy_from_slice(name1);

        // Second skill: id=200, name="Skill2"
        let offset = SKILL_SIZE;
        buf[offset..offset + 4].copy_from_slice(&200u32.to_le_bytes());
        let name2 = b"Skill2";
        buf[offset + 4..offset + 4 + name2.len()].copy_from_slice(name2);

        let skills = decode_skills(&buf, 2).unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id, 100);
        assert_eq!(skills[0].name, "Skill1");
        assert_eq!(skills[1].id, 200);
        assert_eq!(skills[1].name, "Skill2");
    }

    #[test]
    fn trims_null_padding() {
        let mut b = vec![0u8; SKILL_SIZE];
        b[0..4].copy_from_slice(&999u32.to_le_bytes());
        let name = b"Short";
        b[4..4 + name.len()].copy_from_slice(name);
        // Rest of name field is zero-padded (already initialized with zeros)

        let s = decode_skills(&b, 1).unwrap();
        assert_eq!(s[0].id, 999);
        assert_eq!(s[0].name, "Short");
        assert!(!s[0].name.ends_with('\0'));
    }
}
