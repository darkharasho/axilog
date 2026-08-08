use crate::evtc::{RawLog, RawAgent};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentKind { Player, Npc, Gadget }

#[derive(Debug, Clone)]
pub struct Player { pub agent_addr: u64, pub account: String, pub character: String,
    pub profession: String, pub elite_spec: String, pub team: String,
    pub subgroup: u8, pub in_squad: bool, pub commander: bool }
#[derive(Debug, Clone)]
pub struct Enemy { pub id: u64, pub instid: u16, pub name: String,
    pub team: String, pub is_player: bool }
#[derive(Debug, Clone)]
pub struct Team { pub color: String, pub team_id: u16 }
#[derive(Debug, Clone)]
pub struct Encounter { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<Team>, pub players: Vec<Player>, pub enemies: Vec<Enemy> }

pub fn agent_kind(a: &RawAgent) -> AgentKind {
    if a.is_elite != 0xffff_ffff { AgentKind::Player }
    else if (a.prof >> 16) == 0xffff { AgentKind::Gadget }
    else { AgentKind::Npc }
}

pub fn profession_name(prof: u32, is_elite: u32) -> (String, String) {
    // Minimal core professions by prof code; elite spec by is_elite code.
    let base = match prof {
        1 => "Guardian", 2 => "Warrior", 3 => "Engineer", 4 => "Ranger",
        5 => "Thief", 6 => "Elementalist", 7 => "Mesmer", 8 => "Necromancer",
        9 => "Revenant", _ => "",
    };
    let base = if base.is_empty() { prof.to_string() } else { base.to_string() };
    let spec = if is_elite == 0 { String::new() } else { is_elite.to_string() };
    (base, spec)
}

pub fn resolve(raw: &RawLog) -> Encounter {
    let mut players = Vec::new();
    let mut enemies = Vec::new();
    for a in &raw.agents {
        match agent_kind(a) {
            AgentKind::Player => {
                let (character, account, sub) = a.name_parts();
                let (profession, elite_spec) = profession_name(a.prof, a.is_elite);
                players.push(Player {
                    agent_addr: a.addr, account, character, profession, elite_spec,
                    team: String::new(), subgroup: sub.unwrap_or(0),
                    in_squad: true, commander: false,
                });
            }
            _ => {
                let (name, _, _) = a.name_parts();
                enemies.push(Enemy { id: a.addr, instid: 0, name,
                    team: String::new(), is_player: false });
            }
        }
    }
    let duration_ms = raw.events.last().map(|e| e.time).unwrap_or(0)
        .saturating_sub(raw.events.first().map(|e| e.time).unwrap_or(0));
    let mut enc = Encounter {
        kind: "wvw".into(), map: "World vs World".into(), duration_ms,
        build: raw.header.build.clone(), revision: raw.header.revision,
        recorded_by: None, teams: Vec::new(), players, enemies,
    };
    crate::wvw::apply(&mut enc, raw);
    enc
}

#[cfg(test)]
mod tests {
    use crate::evtc::{RawLog, RawHeader, RawAgent, RawSkill};
    use super::resolve;
    fn agent(addr: u64, is_elite: u32, name: &[u8]) -> RawAgent {
        RawAgent { addr, prof: 5, is_elite,
            toughness:0, concentration:0, healing:0, hitbox_width:0,
            condition:0, hitbox_height:0, name_raw: name.to_vec() }
    }
    #[test]
    fn splits_players_from_npcs() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"), // player
                agent(2, 0xffff_ffff, b"Enemy Zerg\0"),   // npc/enemy
            ],
            skills: vec![], events: vec![],
        };
        let enc = resolve(&raw);
        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].account, ":Alice.1234");
        assert_eq!(enc.players[0].subgroup, 5);
    }
}
