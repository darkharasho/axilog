use crate::evtc::{RawLog, RawAgent};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentKind { Player, Npc, Gadget }

#[derive(Debug, Clone)]
pub struct Player { pub agent_addr: u64, pub account: String, pub character: String,
    pub profession: String, pub elite_spec: String, pub team: String,
    pub subgroup: u8, pub in_squad: bool, pub commander: bool,
    /// The player's current squad marker (Task 7, M2 -- `CBTS_MARKER`), by
    /// resolved name (e.g. `"arrow"`) or, when the marker's GUID isn't one
    /// of the known squad-marker GUIDs, its raw hex GUID. `None` when no
    /// marker is currently assigned (or the log has no `CBTS_MARKER`
    /// events at all). See `crate::wvw::markers`.
    pub marker: Option<String>,
    /// The commander tag's colour/variant (incl. cat tags), when this
    /// player currently has one assigned -- `CBTS_MARKER` with `buff == 1`
    /// (Task 7, M2, arcdps-dev guidance item 5). Kept alongside `commander`
    /// above (which stays a plain presence bool for compatibility);
    /// `commander_tag` is the native-only richer form. `None` when the
    /// player never held a commander tag anywhere in the log.
    ///
    /// M4 post-rework EI-parity fix: prefers a still-open commander-tag
    /// instance (sharpest "who's commander right now"), but falls back to
    /// the most recent one ever observed even if since removed with no
    /// reassignment -- matching GW2EI's own `Player.IsCommander`/
    /// `hasCommanderTag`, which is "was this player EVER assigned a
    /// commander-tag marker in the log" rather than "is one still open at
    /// the log's last event". See `wvw::markers::MarkerResolution::
    /// ever_commander`'s doc comment for the real-log finding that drove
    /// this.
    pub commander_tag: Option<CommanderTag>,
    /// Every raw agent addr observed for this account (relogs / build
    /// swaps each get a new addr from arcdps). `agent_addr` above is the
    /// representative; this always contains at least that value.
    pub agent_addrs: Vec<u64> }
#[derive(Debug, Clone)]
pub struct Enemy { pub id: u64, pub instid: u16, pub name: String,
    pub team: String, pub is_player: bool,
    /// The enemy's current squad marker, mirroring `Player::marker` (Task
    /// 7, M2). Enemy commanders don't get a `commander_tag` breakdown --
    /// only `Player` does, per the Task 7 brief.
    pub marker: Option<String>,
    /// Every raw agent addr folded into this enemy. For NPCs/gadgets this is
    /// always exactly `[id]` (distinct spawns are distinct, never deduped).
    /// For enemy players relogging under the same account (Task 4, M2), this
    /// holds every one of that account's raw addrs; `id` is the
    /// representative. Analysis uses this so damage against any of the
    /// account's addrs (not just the representative) is still counted.
    pub agent_addrs: Vec<u64> }
/// A player's resolved commander-tag colour/variant (Task 7, M2). See
/// `crate::wvw::markers::COMMANDER_TAG_VARIANTS`.
#[derive(Debug, Clone, PartialEq)]
pub struct CommanderTag { pub variant: String, pub guid: String }
/// One `CBTS_MARKER` assignment (not removal) event, resolved to a display
/// name (Task 7, M2). `Encounter.markers` holds every one observed in the
/// log, across all agents (squad, enemy, and NPC/gadget alike -- arcdps
/// doesn't restrict `CBTS_MARKER` to squad members).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerAssignment { pub agent_addr: u64, pub marker: String, pub time_ms: u64 }
/// Server tick-rate telemetry derived from `CBTS_TICK` (Task 7, M2,
/// arcdps-dev guidance item 7). See
/// `crate::wvw::markers::resolve_tick_rate` for the derivation and its
/// caveats.
#[derive(Debug, Clone, PartialEq)]
pub struct TickRate { pub avg: f64, pub min: f64, pub per_second: Vec<f64> }
#[derive(Debug, Clone)]
pub struct Team {
    pub color: String,
    pub team_id: u32,
    /// Stable content GUID for this team id, when a `CBTS_IDTOGUID` (TEAM
    /// content type) mapping was present in the log (Task 2b). Lowercase
    /// hex, no dashes. `None` for logs without the event (arcdps builds
    /// before it existed, or a team id with no GUID mapping emitted).
    pub guid: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Encounter { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<Team>, pub players: Vec<Player>, pub enemies: Vec<Enemy>,
    /// Every `CBTS_MARKER` assignment observed in the log, in stream order
    /// (Task 7, M2). Empty when the log has no marker events.
    pub markers: Vec<MarkerAssignment>,
    /// Tick-rate telemetry from `CBTS_TICK` (Task 7, M2), `None` when the
    /// log has fewer than two such events.
    pub tick_rate: Option<TickRate> }

pub fn agent_kind(a: &RawAgent) -> AgentKind {
    if a.is_elite != 0xffff_ffff { AgentKind::Player }
    else if (a.prof >> 16) == 0xffff { AgentKind::Gadget }
    else { AgentKind::Npc }
}

pub fn profession_name(prof: u32, is_elite: u32) -> (String, String) {
    // Core professions by prof code.
    let base = match prof {
        1 => "Guardian", 2 => "Warrior", 3 => "Engineer", 4 => "Ranger",
        5 => "Thief", 6 => "Elementalist", 7 => "Mesmer", 8 => "Necromancer",
        9 => "Revenant", _ => "",
    };
    let base = if base.is_empty() { prof.to_string() } else { base.to_string() };

    // Elite specializations keyed by GW2 API specialization id (`is_elite`),
    // covering HoT/PoF/EoD/SotO plus specs observed beyond that era in the
    // calibration fixture (73/75/80/81 — see fixtures/wvw-small.ei.json /
    // `professions_match_ei_golden`, which verified these against EI's
    // golden `profession` field for the same accounts).
    let spec = if is_elite == 0 {
        String::new()
    } else {
        match is_elite {
            5 => "Druid",           // Ranger (HoT)
            7 => "Daredevil",       // Thief (HoT)
            18 => "Berserker",      // Warrior (HoT)
            27 => "Dragonhunter",   // Guardian (HoT)
            34 => "Reaper",         // Necromancer (HoT)
            40 => "Chronomancer",   // Mesmer (HoT)
            43 => "Scrapper",       // Engineer (HoT)
            48 => "Tempest",        // Elementalist (HoT)
            52 => "Herald",         // Revenant (HoT)
            55 => "Soulbeast",      // Ranger (PoF)
            56 => "Weaver",         // Elementalist (PoF)
            57 => "Holosmith",      // Engineer (PoF)
            58 => "Deadeye",        // Thief (PoF)
            59 => "Mirage",         // Mesmer (PoF)
            60 => "Scourge",        // Necromancer (PoF)
            61 => "Spellbreaker",   // Warrior (PoF)
            62 => "Firebrand",      // Guardian (PoF)
            63 => "Renegade",       // Revenant (PoF)
            64 => "Harbinger",      // Necromancer (EoD)
            65 => "Willbender",     // Guardian (EoD)
            66 => "Virtuoso",       // Mesmer (EoD)
            67 => "Catalyst",       // Elementalist (EoD)
            68 => "Bladesworn",     // Warrior (EoD)
            69 => "Vindicator",     // Revenant (EoD)
            70 => "Mechanist",      // Engineer (EoD)
            71 => "Specter",        // Thief (EoD)
            72 => "Untamed",        // Ranger (SotO)
            73 => "Troubadour",     // Mesmer (post-SotO; fixture-verified)
            74 => "Paragon",        // Warrior (post-SotO; fixture-verified,
                                    // M15 Task 2): the local post-rework
                                    // capture has exactly one unmapped
                                    // elite id on a Warrior (74) and EI's
                                    // export for that log reports exactly
                                    // one Warrior spec we did not name,
                                    // "Paragon" -- same by-elimination
                                    // method used for 73/75/80/81 above.
                                    // EI's own `Spec.Paragon` is grouped
                                    // under Warrior in
                                    // `ParserIcons.BaseResProfIcons`.
            75 => "Amalgam",        // Engineer (post-SotO; fixture-verified)
            80 => "Evoker",         // Elementalist (post-SotO; fixture-verified)
            81 => "Luminary",       // Guardian (post-SotO; fixture-verified)
            _ => "",
        }
        .to_string()
    };
    let spec = if spec.is_empty() && is_elite != 0 { is_elite.to_string() } else { spec };
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
                    in_squad: true, commander: false, marker: None, commander_tag: None,
                    agent_addrs: vec![a.addr],
                });
            }
            _ => {
                let (name, _, _) = a.name_parts();
                enemies.push(Enemy { id: a.addr, instid: 0, name,
                    team: String::new(), is_player: false, marker: None,
                    agent_addrs: vec![a.addr] });
            }
        }
    }
    let duration_ms = raw.events.last().map(|e| e.time).unwrap_or(0)
        .saturating_sub(raw.events.first().map(|e| e.time).unwrap_or(0));
    let mut enc = Encounter {
        kind: "wvw".into(), map: "World vs World".into(), duration_ms,
        build: raw.header.build.clone(), revision: raw.header.revision,
        recorded_by: None, teams: Vec::new(), players, enemies,
        markers: Vec::new(), tick_rate: None,
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
    fn team_change(addr: u64, team: u32) -> RawEvent {
        RawEvent { time:0, src_agent:addr, dst_agent:0, value: team as i32, buff_dmg:0,
            overstack:0, skillid:0, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:0, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::TEAM_CHANGE, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    fn point_of_view(addr: u64) -> RawEvent {
        RawEvent { time:0, src_agent:addr, dst_agent:0, value:0, buff_dmg:0,
            overstack:0, skillid:0, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:0, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::POINT_OF_VIEW, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
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
            guid_map: vec![],
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
