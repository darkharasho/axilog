use crate::evtc::{ContentType, RawEvent, RawLog, sc};
use crate::model::{Encounter, Team, Player, Enemy};
use std::collections::BTreeMap;

pub mod markers;

/// Collapse relog/build-swap duplicates: one Player per account (fallback character).
pub fn dedupe_players(players: &mut Vec<Player>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        let key = if p.account.is_empty() { p.character.clone() } else { p.account.clone() };
        match seen.get(&key) {
            Some(&i) => {
                out[i].in_squad |= p.in_squad;
                out[i].commander |= p.commander;
                out[i].agent_addrs.extend(p.agent_addrs);
            }
            None => { seen.insert(key, out.len()); out.push(p); }
        }
    }
    *players = out;
}

/// Collapse relog/build-swap duplicates among enemy players, mirroring
/// `dedupe_players` for the squad side -- with one deliberate difference.
///
/// Squad players always reveal their own account, so `dedupe_players` can
/// safely fall back to `character` as a key when `account` happens to be
/// empty. Enemy players are different: arcdps only reveals an enemy's
/// account name when it happens to be known/visible, and when it isn't,
/// `character` is NOT a real display name -- arcdps substitutes the
/// enemy's profession/elite-spec label instead (verified against the WvW
/// golden fixture: every enemy player there has a blank account, and
/// `character` is a shared spec label like "Druid" or "Harbinger" repeated
/// across dozens of clearly-distinct agents). Falling back to `character`
/// in that case would wrongly merge unrelated enemies who simply share a
/// class. So: only dedupe enemy players with a known, non-empty account;
/// everyone else is left as a distinct entry. NPCs/gadgets never go
/// through this function at all -- distinct spawns are distinct.
fn dedupe_enemy_players(players: &mut Vec<Player>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        if p.account.is_empty() {
            out.push(p);
            continue;
        }
        match seen.get(&p.account) {
            Some(&i) => { out[i].agent_addrs.extend(p.agent_addrs); }
            None => { seen.insert(p.account.clone(), out.len()); out.push(p); }
        }
    }
    *players = out;
}

// WvW map id (MAP_ID statechange `src_agent`) → display name.
// Ids/names cross-checked against GW2EI's `MapIDs` (LogLogic/WvW) and the
// golden fixture (Green Alpine Borderlands, id 95). Unknown ids fall back
// to the generic "World vs World" label rather than guessing.
fn map_name(map_id: u32) -> &'static str {
    match map_id {
        38 => "Eternal Battlegrounds",
        95 => "Green Alpine Borderlands",
        96 => "Blue Alpine Borderlands",
        1099 => "Red Desert Borderlands",
        968 => "Edge of the Mists",
        _ => "World vs World",
    }
}

// WvW team id → color, from a fixed id table (Task 2, M2).
//
// GW2EI itself derives team colors dynamically from the CBTS_WVWTEAMS
// statechange (sc=74, `WvWTeamsEvent`), which carries the log's actual
// red/blue/green team ids. That event is only emitted by arcdps builds from
// ~May 2026 onward — EI has no static id→color fallback for older logs, and
// our golden fixture (Jan 2026) predates the event (verified: no sc=74
// events in `fixtures/local/wvw-small.zevtc`). So, like axibridge
// (`src/shared/wvwTeams.ts`), we fall back to a fixed table for logs
// without the event. Table reconciled by axibridge from two community
// tools: Drevarr/EVTC_parser/gw2_data.py and
// Drevarr/GW2_EI_log_combiner/config.py.
//
// Ids outside all three sets resolve to "unknown" — never silently
// defaulted to "green" (the pre-M2 placeholder bug), since that would
// mislabel neutral/unrecognized agents (e.g. team id 0 on non-WvW-team
// agents) as friendly-colored.
const RED_TEAM_IDS: &[u16] = &[697, 705, 706, 707, 882, 885, 886, 2520, 2543];
const GREEN_TEAM_IDS: &[u16] = &[39, 2739, 2741, 2752, 2763, 2767];
const BLUE_TEAM_IDS: &[u16] = &[432, 433, 1277, 1282, 1989];

fn team_color(team_id: u16) -> String {
    if RED_TEAM_IDS.contains(&team_id) {
        "red".into()
    } else if GREEN_TEAM_IDS.contains(&team_id) {
        "green".into()
    } else if BLUE_TEAM_IDS.contains(&team_id) {
        "blue".into()
    } else {
        "unknown".into()
    }
}

// --- Task 2b: CBTS_WVWTEAMS (sc=74) dynamic team ids ---
//
// GW2EI's real source of truth for team colors (see the Task 2 comment
// above) is the CBTS_WVWTEAMS statechange event, which carries the log's
// *actual* red/blue/green team ids directly — no guessing needed. When
// present, it takes priority over the static table (which remains the
// fallback for logs recorded before arcdps started emitting this event).
//
// Payload, verified against the arcdps EVTC reference
// (deltaconnected.com/arcdps/evtc/README.txt):
//   CBTS_WVWTEAMS
//   // src_agent: (uint32_t*)&src_agent is uint32[6], redshard id,
//   //   blueshard id, greenshard id, redteam id, blueteam id, greenteam id
// `src_agent`/`dst_agent` are adjacent u64 fields in the raw cbtevent
// struct, so those six little-endian u32s span src_agent (2), dst_agent
// (2), value (1), buff_dmg (1), in that order — cross-checked against
// GW2EI's `WvWTeamsEvent`, which decodes the same 6-uint32 buffer from
// exactly `[SrcAgent, DstAgent, Value, BuffDmg]` and labels the resulting
// slots `[RedShardID, BlueShardID, GreenShardID, RedTeamID, BlueTeamID,
// GreenTeamID]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DynamicWvwTeamIds { red: u32, blue: u32, green: u32 }

fn parse_wvw_teams_event(e: &RawEvent) -> DynamicWvwTeamIds {
    let red_team = (e.dst_agent >> 32) as u32; // uint32[3]
    let blue_team = e.value as u32;            // uint32[4]
    let green_team = e.buff_dmg as u32;        // uint32[5]
    DynamicWvwTeamIds { red: red_team, blue: blue_team, green: green_team }
}

/// Like `team_color`, but prefers the log's own dynamically-observed
/// red/blue/green ids (from CBTS_WVWTEAMS) when available, falling back to
/// the static table for any id the dynamic event doesn't cover (or when the
/// event is absent entirely).
fn team_color_with(team_id: u16, dynamic: Option<&DynamicWvwTeamIds>) -> String {
    if let Some(d) = dynamic {
        let id = team_id as u32;
        if id == d.red {
            return "red".into();
        } else if id == d.blue {
            return "blue".into();
        } else if id == d.green {
            return "green".into();
        }
    }
    team_color(team_id)
}

/// Parse per-agent WvW team ids and the POINT_OF_VIEW (recording) agent from
/// raw combat events. Shared by `apply` (friend/foe partition, below) and by
/// the analysis layer (pet/minion damage attribution in `analysis::damage`).
pub fn resolve_teams(raw: &RawLog) -> (BTreeMap<u64, u16>, Option<u64>) {
    let mut agent_team: BTreeMap<u64, u16> = BTreeMap::new();
    let mut recorded_by: Option<u64> = None;
    for e in &raw.events {
        if e.is_statechange == sc::TEAM_CHANGE {
            // The WvW team id is carried in the `value` field (i32 @ offset
            // 24), not `dst_agent` — verified against the golden fixture's
            // teamID (Task 16A). Every agent (players, NPCs, gadgets) gets
            // exactly one TEAM_CHANGE event.
            agent_team.insert(e.src_agent, e.value as u16);
        } else if e.is_statechange == sc::POINT_OF_VIEW {
            recorded_by = Some(e.src_agent);
        }
    }
    (agent_team, recorded_by)
}

pub fn apply(enc: &mut Encounter, raw: &RawLog) {
    let (agent_team, recorded_by) = resolve_teams(raw);

    // MAP_ID statechange carries the WvW map id in `src_agent` (Task 2, M2).
    if let Some(map_id) = raw.events.iter()
        .find(|e| e.is_statechange == sc::MAP_ID)
        .map(|e| e.src_agent as u32)
    {
        enc.map = map_name(map_id).to_string();
    }

    // CBTS_WVWTEAMS (sc=74): the log's real red/blue/green team ids, when
    // arcdps emitted the event (Task 2b). `find` takes the first — arcdps
    // emits this once per log, at most.
    let dynamic = raw.events.iter()
        .find(|e| e.is_statechange == sc::WVW_TEAMS)
        .map(parse_wvw_teams_event);

    // CBTS_IDTOGUID (sc=46) TEAM mappings: team id -> stable content GUID
    // (Task 2b).
    let team_guids: BTreeMap<u16, String> = raw.guid_map.iter()
        .filter(|g| g.content_type == ContentType::Team)
        .map(|g| (g.local_id as u16, g.guid_hex()))
        .collect();

    let mut team_ids: Vec<u16> = agent_team.values().copied().collect();
    team_ids.sort_unstable(); team_ids.dedup();
    enc.teams = team_ids.iter().map(|&id| Team {
        color: team_color_with(id, dynamic.as_ref()),
        team_id: id,
        guid: team_guids.get(&id).cloned(),
    }).collect();

    // Friend/foe partition (Task 16A calibration fix).
    //
    // `model::resolve` classifies agents purely from the EVTC agent block
    // (`is_elite != 0xffffffff` => Player), which cannot distinguish squad
    // members from enemy players in WvW — both are real players. It stuffs
    // every player agent into `enc.players` and every NPC/gadget into
    // `enc.enemies`. Here we use each agent's WvW team id relative to the
    // recorder's own team (POINT_OF_VIEW) to split real squad members from
    // enemy players, and to drop friendly-side NPCs/gadgets (pets, siege,
    // guards on our own team) out of `enc.enemies`.
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());

    let mut friendly_players = Vec::new();
    let mut enemy_players: Vec<Player> = Vec::new();
    for p in enc.players.drain(..) {
        let is_friendly = match agent_team.get(&p.agent_addr) {
            Some(&t) => Some(t) == friendly_team,
            // Unconstrained player agent (no observed team): not confirmed
            // to be on the recorder's team, so default to enemy — safer
            // than silently inflating the squad.
            None => false,
        };
        if is_friendly {
            friendly_players.push(p);
        } else {
            enemy_players.push(p);
        }
    }
    enc.players = friendly_players;

    // Enemy relog dedupe (Task 4, M2): collapse enemy players relogging
    // under the same known account into one Enemy, aggregating their raw
    // addrs -- see `dedupe_enemy_players` docs for why this only keys on
    // `account` (never falls back to `character` like the squad-side
    // dedupe does).
    dedupe_enemy_players(&mut enemy_players);
    for p in enemy_players {
        let team = agent_team.get(&p.agent_addr).map(|&t| team_color_with(t, dynamic.as_ref())).unwrap_or_default();
        enc.enemies.push(Enemy {
            id: p.agent_addr,
            instid: 0,
            name: p.character,
            team,
            is_player: true,
            marker: None,
            agent_addrs: p.agent_addrs,
        });
    }

    // M4 post-rework real-log calibration finding: objective NPCs (Keep/
    // Camp/Tower Lords) can emit a SECOND `TEAM_CHANGE` mid-recording when
    // the objective flips ownership (e.g. the squad captures the keep the
    // Lord belongs to). `agent_team` above is a static last-write-wins map
    // (`resolve_teams`), so an agent whose *final* recorded team happens to
    // equal `friendly_team` reads as "ours" for its ENTIRE presence in the
    // log — including the damage dealt to it while it was still on a
    // hostile team, before the flip. Verified against a real post-rework
    // WvW capture (`fixtures/local/wvw-postrework.zevtc`): its Keep Lord
    // had `TEAM_CHANGE` to team 433 (hostile) followed ~0.5s later by
    // `TEAM_CHANGE` to 2767 (the recorder's own team), so the static map
    // resolved it as friendly and this retain dropped it entirely — even
    // though arcdps' own per-event `iff` byte tagged every one of the
    // 7144 combat events targeting it as FOE (`iff == 1`) throughout the
    // whole ~5m48s fight. That single dropped NPC accounted for ~6.3M of
    // the ~6.4M squadTotalDamage gap against the EI golden JSON (98%+ of
    // the miss).
    //
    // Fix: for NPC/gadget agents, trust arcdps' own per-event `iff` (Foe)
    // on combat events squad members dealt to them as an override signal,
    // the same "iff is more reliable than a static team lookup" precedent
    // `analysis::damage::accumulate_pet_credit`'s docs already establish
    // for pet/minion damage attribution. An NPC/gadget that squad members
    // ever landed an `iff == FOE` hit on is kept as hostile regardless of
    // what its LAST `TEAM_CHANGE` says; only agents with neither iff
    // evidence nor a non-friendly static team fall back to the pre-existing
    // "no team record at all => drop as neutral" rule (still needed so
    // truly friendly-side untracked NPCs -- our own siege, pets, guards --
    // don't inflate squad damage totals).
    let squad_addrs: std::collections::BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let iff_confirmed_hostile_npcs: std::collections::BTreeSet<u64> = raw
        .events
        .iter()
        .filter(|e| e.is_statechange == 0 && e.iff == 1 && squad_addrs.contains(&e.src_agent))
        .map(|e| e.dst_agent)
        .collect();
    enc.enemies.retain(|en| {
        if en.is_player {
            return true; // already resolved above
        }
        if iff_confirmed_hostile_npcs.contains(&en.id) {
            return true;
        }
        match agent_team.get(&en.id) {
            Some(&t) => friendly_team.map(|ft| t != ft).unwrap_or(true),
            None => false,
        }
    });

    for p in &mut enc.players {
        if let Some(&t) = agent_team.get(&p.agent_addr) { p.team = team_color_with(t, dynamic.as_ref()); }
    }
    for en in &mut enc.enemies {
        if let Some(&t) = agent_team.get(&en.id) { en.team = team_color_with(t, dynamic.as_ref()); }
    }
    if let Some(addr) = recorded_by {
        if let Some(p) = enc.players.iter().find(|p| p.agent_addr == addr) {
            enc.recorded_by = Some(p.account.clone());
        }
    }
    dedupe_players(&mut enc.players);

    // CBTS_MARKER (sc=37) / CBTS_TICK (sc=84): Task 7, M2. Runs last, after
    // dedupe, so `agent_addrs` on each final Player/Enemy already covers
    // every raw addr an account owns (relog/build-swap) -- `final_marker`/
    // `final_commander_tag` pick the freshest state across all of them.
    let marker_res = markers::resolve_markers(raw);
    for p in &mut enc.players {
        p.marker = markers::final_marker(&marker_res.open, &p.agent_addrs);
        p.commander_tag =
            markers::final_commander_tag(&marker_res.open, &marker_res.ever_commander, &p.agent_addrs);
        p.commander = p.commander_tag.is_some();
    }
    for en in &mut enc.enemies {
        en.marker = markers::final_marker(&marker_res.open, &en.agent_addrs);
    }
    enc.markers = marker_res.assignments;
    enc.tick_rate = markers::resolve_tick_rate(raw, enc.duration_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Encounter, Player};
    use crate::evtc::{RawAgent, RawHeader};
    fn player(addr: u64, acc: &str) -> Player {
        Player { agent_addr: addr, account: acc.into(), character: "C".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![addr] }
    }
    fn agent(addr: u64, is_elite: u32, name: &[u8]) -> RawAgent {
        RawAgent { addr, prof: 5, is_elite,
            toughness: 0, concentration: 0, healing: 0, hitbox_width: 0,
            condition: 0, hitbox_height: 0, name_raw: name.to_vec() }
    }
    fn team_change(addr: u64, team: u16) -> RawEvent {
        RawEvent { time: 0, src_agent: addr, dst_agent: 0, value: team as i32, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: sc::TEAM_CHANGE, is_shields: 0, is_offcycle: 0 }
    }
    fn point_of_view(addr: u64) -> RawEvent {
        RawEvent { time: 0, src_agent: addr, dst_agent: 0, value: 0, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: sc::POINT_OF_VIEW, is_shields: 0, is_offcycle: 0 }
    }
    /// Synthetic CBTS_WVWTEAMS event. Packs (red, blue, green) into the
    /// same 6xu32 layout `parse_wvw_teams_event` reads back
    /// (shard ids left as 0 — unused by axilog today).
    fn wvw_teams_event(red: u32, blue: u32, green: u32) -> RawEvent {
        let dst_agent = (red as u64) << 32; // uint32[2]=green_shard(0), uint32[3]=red_team
        RawEvent { time: 0, src_agent: 0, dst_agent, value: blue as i32, buff_dmg: green as i32,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: sc::WVW_TEAMS, is_shields: 0, is_offcycle: 0 }
    }
    /// Synthetic CBTS_IDTOGUID event mapping a WvW team id to a stable GUID
    /// (content type TEAM = 4).
    fn team_guid_event(team_id: u32, guid: [u8; 16]) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: u64::from_le_bytes(guid[0..8].try_into().unwrap()),
            dst_agent: u64::from_le_bytes(guid[8..16].try_into().unwrap()),
            value: 0, buff_dmg: 0, overstack: 4, skillid: team_id,
            src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 0, buff: 0, result: 0, is_activation: 0, is_buffremove: 0,
            is_statechange: sc::ID_TO_GUID,
            is_shields: 0, is_offcycle: 0,
        }
    }
    #[test]
    fn dedupes_players_by_account() {
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1"), player(3, ":B.2")],
            enemies: vec![], markers: vec![], tick_rate: None };
        dedupe_players(&mut enc.players);
        assert_eq!(enc.players.len(), 2);
    }
    #[test]
    fn dedupe_collects_all_agent_addrs_for_relog() {
        // Same account, two raw agent addrs (relog / build swap). The
        // survivor must retain BOTH addrs so downstream analysis can sum
        // damage across the full account, not just the representative.
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1")],
            enemies: vec![], markers: vec![], tick_rate: None };
        dedupe_players(&mut enc.players);
        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].agent_addr, 1);
        let mut addrs = enc.players[0].agent_addrs.clone();
        addrs.sort_unstable();
        assert_eq!(addrs, vec![1, 2]);
    }

    /// Synthetic direct-damage combat event (`is_statechange == 0`),
    /// mirroring the shape `analysis::damage::accumulate` reads.
    fn strike(src: u64, dst: u64, iff: u8, value: i32) -> RawEvent {
        RawEvent { time: 0, src_agent: src, dst_agent: dst, value, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: 0, is_shields: 0, is_offcycle: 0 }
    }

    /// M4 post-rework real-log finding: an objective NPC (Keep/Camp/Tower
    /// Lord) whose LAST recorded `TEAM_CHANGE` happens to equal the
    /// recorder's own team (e.g. the objective flips to the squad's color
    /// moments after the Lord's own team briefly registered) must still be
    /// classified as a hostile enemy, since arcdps' own per-event `iff`
    /// tags squad-sourced hits on it as FOE throughout. The static
    /// last-write-wins `agent_team` map alone would wrongly drop it as
    /// "friendly", silently discarding every point of damage dealt to it
    /// (reproduces the real fixture's ~6.3M/6.4M squadTotalDamage gap at
    /// unit-test scale). See the long doc comment on the `retain` call this
    /// guards in `apply` above.
    #[test]
    fn npc_with_stale_friendly_team_change_stays_enemy_when_iff_confirms_hostile() {
        let raw = RawLog {
            header: RawHeader { build: "20260718".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"), // squad player
                agent(2, 0xffff_ffff, b"Keep Lord\0"),     // objective NPC
            ],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 100),        // recorder's team
                team_change(2, 200),        // Lord starts on a hostile team
                strike(1, 2, 1, 500),        // squad hits it while still hostile (iff=FOE)
                team_change(2, 100),        // keep flips to the recorder's own color afterward
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);

        assert_eq!(enc.enemies.len(), 1, "the Lord must stay classified as an enemy");
        assert_eq!(enc.enemies[0].id, 2);
        assert!(!enc.enemies[0].is_player);
    }

    /// Counterpart: an NPC/gadget that squad members never actually landed
    /// an `iff == FOE` hit on, and whose last team resolves to the
    /// recorder's own team, is still correctly dropped as friendly/neutral
    /// -- the iff-override doesn't just blanket-keep every NPC regardless
    /// of team.
    #[test]
    fn friendly_npc_with_no_hostile_iff_evidence_stays_dropped() {
        let raw = RawLog {
            header: RawHeader { build: "20260718".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"), // squad player
                agent(2, 0xffff_ffff, b"Friendly Siege\0"), // our own siege
            ],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 100),
                team_change(2, 100), // same team as recorder, never flips
                // no combat events targeting it at all
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);
        assert_eq!(enc.enemies.len(), 0, "friendly-team NPC with no hostile iff evidence must not appear as an enemy");
    }

    /// Synthetic `CBTS_MARKER` event (`sc::MARKER`), mirroring
    /// `markers::tests::marker_ev`.
    fn marker(time: u64, src: u64, value: i32, buff: u8) -> RawEvent {
        RawEvent { time, src_agent: src, dst_agent: 0, value, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: sc::MARKER, is_shields: 0, is_offcycle: 0 }
    }
    fn marker_guid(local_id: u32, guid_bytes: [u8; 16]) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: u64::from_le_bytes(guid_bytes[0..8].try_into().unwrap()),
            dst_agent: u64::from_le_bytes(guid_bytes[8..16].try_into().unwrap()),
            value: 0, buff_dmg: 0, overstack: 1, skillid: local_id,
            src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 0, buff: 0, result: 0, is_activation: 0, is_buffremove: 0,
            is_statechange: sc::ID_TO_GUID, is_shields: 0, is_offcycle: 0,
        }
    }

    /// M4 post-rework real-log calibration finding, end-to-end through
    /// `model::resolve`: a commander is assigned a recognized commander-tag
    /// marker near the start of the log, then a removal event with NO
    /// reassignment for the rest of the (much longer) fight. Before the
    /// `ever_commander` fallback, this silently produced zero commanders --
    /// reproduces the real fixture's exact anomaly at unit-test scale (see
    /// `markers::MarkerResolution::ever_commander`'s doc comment for the
    /// full real-log finding this guards).
    #[test]
    fn commander_detected_after_early_removal_with_no_reassignment_for_rest_of_fight() {
        // PurpleCommanderTag GUID, same bytes `markers::tests` uses.
        let guid_bytes: [u8; 16] = [
            0x19, 0x93, 0xfa, 0xdb, 0x6f, 0xb7, 0x0e, 0x43,
            0x83, 0xa2, 0x23, 0xa5, 0x4d, 0x31, 0x1f, 0x7d,
        ];
        let raw = RawLog {
            header: RawHeader { build: "20260718".into(), revision: 1, boss_id: 1 },
            agents: vec![agent(1, 27, b"Alice\0:Alice.1234\05\0")],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 100),
                marker_guid(3201, guid_bytes),
                marker(100, 1, 3201, 1), // commander tag assigned near log start
                marker(350, 1, 0, 0),    // removed moments later
                // ... 5+ minutes of fight with no further marker activity ...
            ],
            guid_map: vec![],
        };
        let mut raw = raw;
        raw.guid_map = crate::evtc::decode_guid_mappings(&raw.events);

        let enc = crate::model::resolve(&raw);
        assert_eq!(enc.players.len(), 1);
        assert!(enc.players[0].commander, "commander must still be detected despite the early, unreciprocated removal");
        let tag = enc.players[0].commander_tag.as_ref().expect("commander_tag must be Some");
        assert_eq!(tag.variant, "purple-commander");
    }

    /// Task 2b: a CBTS_WVWTEAMS event, when present, resolves team colors
    /// directly from its red/blue/green ids -- even for ids that aren't in
    /// the static fallback table at all. This proves the dynamic path is
    /// actually driving the result (not just coincidentally agreeing with
    /// the static table).
    #[test]
    fn dynamic_wvwteams_event_overrides_static_table() {
        let raw = RawLog {
            header: RawHeader { build: "20260701".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"),
                agent(2, 27, b"Bob\0:Bob.5678\05\0"),
            ],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 5001), // recorder -- not in any static id set
                team_change(2, 6002), // enemy -- not in any static id set
                wvw_teams_event(/*red*/ 6002, /*blue*/ 7003, /*green*/ 5001),
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);

        assert_eq!(enc.players.len(), 1);
        assert_eq!(enc.players[0].team, "green", "recorder's team (5001) is the dynamic green id");

        assert_eq!(enc.enemies.len(), 1);
        assert!(enc.enemies[0].is_player);
        assert_eq!(enc.enemies[0].team, "red", "enemy team (6002) is the dynamic red id");

        let team_5001 = enc.teams.iter().find(|t| t.team_id == 5001).expect("team 5001 present");
        assert_eq!(team_5001.color, "green");
        let team_6002 = enc.teams.iter().find(|t| t.team_id == 6002).expect("team 6002 present");
        assert_eq!(team_6002.color, "red");
    }

    /// Without a CBTS_WVWTEAMS event, team ids outside the static table
    /// resolve to "unknown" -- the fallback-of-last-resort, never silently
    /// green (regression guard for the pre-M2 placeholder bug).
    #[test]
    fn no_wvwteams_event_falls_back_to_static_table() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![agent(1, 27, b"Alice\0:Alice.1234\05\0")],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 55555), // not in any static id set, no dynamic event either
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);
        assert_eq!(enc.players[0].team, "unknown");
    }

    /// Task 4 (M2): two enemy-player entries sharing the same known account
    /// (a relog/build-swap mid-recording) collapse into one, aggregating
    /// both raw addrs -- mirroring `dedupes_players_by_account` for the
    /// squad side.
    #[test]
    fn dedupe_enemy_players_collapses_same_account() {
        let mut enemies = vec![player(9, ":Foe.1"), player(10, ":Foe.1"), player(11, ":Bar.2")];
        dedupe_enemy_players(&mut enemies);
        assert_eq!(enemies.len(), 2);
        let foe = enemies.iter().find(|p| p.account == ":Foe.1").expect("foe present");
        assert_eq!(foe.agent_addr, 9, "representative is first-seen addr");
        let mut addrs = foe.agent_addrs.clone();
        addrs.sort_unstable();
        assert_eq!(addrs, vec![9, 10]);
    }

    /// Task 4 (M2): enemy players with an empty (unknown) account are never
    /// merged, even if they happen to share a `character` value -- WvW
    /// enemy players without a visible account get a generic
    /// profession/elite-spec placeholder as `character`, which is not a
    /// real, distinguishing name (see `dedupe_enemy_players` docs). This is
    /// the deliberate divergence from `dedupe_players`, which falls back to
    /// `character` for the squad side.
    #[test]
    fn dedupe_enemy_players_does_not_merge_blank_accounts() {
        let mut a = player(9, "");
        a.character = "Druid".into();
        let mut b = player(10, "");
        b.character = "Druid".into(); // same generic spec label, different agent
        let mut enemies = vec![a, b];
        dedupe_enemy_players(&mut enemies);
        assert_eq!(enemies.len(), 2, "blank-account enemies must stay distinct");
    }

    /// Task 2b: a CBTS_IDTOGUID (content type TEAM) mapping attaches a
    /// stable GUID to the matching `enc.teams[]` entry; a team id with no
    /// such mapping gets `guid: None` rather than a stand-in value.
    #[test]
    fn team_guid_mapping_attaches_to_matching_team() {
        let guid_bytes: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let raw = RawLog {
            header: RawHeader { build: "20260701".into(), revision: 1, boss_id: 1 },
            agents: vec![
                agent(1, 27, b"Alice\0:Alice.1234\05\0"),
                agent(2, 27, b"Bob\0:Bob.5678\05\0"),
            ],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 2767),
                team_change(2, 433),
                team_guid_event(2767, guid_bytes),
                // no GUID mapping for 433
            ],
            guid_map: vec![],
        };
        // guid_map is normally populated by decode_raw; wire it up here to
        // mirror what decode_raw does, since this RawLog is built by hand.
        let mut raw = raw;
        raw.guid_map = crate::evtc::decode_guid_mappings(&raw.events);

        let enc = crate::model::resolve(&raw);
        let team_2767 = enc.teams.iter().find(|t| t.team_id == 2767).expect("team 2767 present");
        assert_eq!(team_2767.guid.as_deref(), Some("0102030405060708090a0b0c0d0e0f10"));
        let team_433 = enc.teams.iter().find(|t| t.team_id == 433).expect("team 433 present");
        assert_eq!(team_433.guid, None);
    }
}
