use crate::evtc::{RawLog, RawAgent};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentKind { Player, Npc, Gadget }

#[derive(Debug, Clone)]
pub struct Player { pub agent_addr: u64, pub account: String, pub character: String,
    pub profession: String, pub elite_spec: String, pub team: String,
    pub subgroup: u8, pub in_squad: bool, pub commander: bool,
    /// Every raw agent addr observed for this account (relogs / build
    /// swaps each get a new addr from arcdps). `agent_addr` above is the
    /// representative; this always contains at least that value.
    pub agent_addrs: Vec<u64> }
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
                    agent_addrs: vec![a.addr],
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
    use crate::evtc::{RawLog, RawHeader, RawAgent, RawEvent, sc};
    use super::resolve;
    fn agent(addr: u64, is_elite: u32, name: &[u8]) -> RawAgent {
        RawAgent { addr, prof: 5, is_elite,
            toughness:0, concentration:0, healing:0, hitbox_width:0,
            condition:0, hitbox_height:0, name_raw: name.to_vec() }
    }
    fn team_change(addr: u64, team: u16) -> RawEvent {
        RawEvent { time:0, src_agent:addr, dst_agent:0, value: team as i32, buff_dmg:0,
            overstack:0, skillid:0, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:0, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_statechange: sc::TEAM_CHANGE }
    }
    fn point_of_view(addr: u64) -> RawEvent {
        RawEvent { time:0, src_agent:addr, dst_agent:0, value:0, buff_dmg:0,
            overstack:0, skillid:0, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:0, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_statechange: sc::POINT_OF_VIEW }
    }
    #[test]
    fn splits_players_from_npcs() {
        // The friend/foe partition (Task 16A) needs WvW team ids and a
        // recorder (POINT_OF_VIEW) to tell squad players from enemy
        // players — is_elite alone can't do it, since enemy players also
        // have is_elite != 0xffffffff. Alice is on the recorder's team
        // (100); the "Enemy Zerg" NPC is on a different team (200), so it
        // stays classified as a hostile enemy.
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"), // player
                agent(2, 0xffff_ffff, b"Enemy Zerg\0"),   // npc/enemy
            ],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 100),
                team_change(2, 200),
            ],
        };
        let enc = resolve(&raw);
        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].account, ":Alice.1234");
        assert_eq!(enc.players[0].subgroup, 5);
        assert_eq!(enc.enemies.len(), 1);
        assert_eq!(enc.enemies[0].id, 2);
        assert!(!enc.enemies[0].is_player);
    }
}
