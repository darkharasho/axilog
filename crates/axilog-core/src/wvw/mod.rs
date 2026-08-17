use crate::evtc::{ContentType, RawEvent, RawLog, sc};
use crate::model::{Encounter, Team, Player, Enemy};
use crate::analysis::damage::InstidRegistry;
use std::collections::BTreeMap;

pub mod markers;
/// The per-WvW-map static table (M15 Task 2) -- display names, arena image
/// URLs and combat-replay geometry, all transcribed from GW2EI. axilog's
/// single source of truth for all three; see [`maps`]' module doc.
pub mod maps;
/// The static WvW objective catalog and `CBTS_WVWOBJECTIVESTATUS` (sc=75)
/// ownership timelines (MOBJ). See [`objectives`]' module doc.
pub mod objectives;
pub mod guilds;

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

/// Collapse the multiple agent rows a single enemy PERSON can occupy into
/// one entry, mirroring `dedupe_players` for the squad side -- but keyed
/// primarily on INSTID, not account (MINSTID).
///
/// ## Why not account (the pre-MINSTID rule, and the bug)
///
/// Squad players always reveal their own account, so `dedupe_players` can
/// safely key on `account` (falling back to `character`). Enemy players are
/// different: arcdps only reveals an enemy's account name when it happens
/// to be known/visible, and in WvW it essentially never is -- every enemy
/// player on this project's real captures has a BLANK account. `character`
/// is no substitute either: for an anonymised enemy arcdps substitutes the
/// profession/elite-spec label ("Druid", "Harbinger"), which is shared by
/// dozens of clearly-distinct agents.
///
/// So an account-keyed dedupe degenerates to "no dedupe at all" for enemy
/// players, and the multiple agent rows one person occupies (arcdps emits a
/// fresh agent address per (re)spawn/instance for the same instid) each
/// became their own `enemies[]` row: 71 rows over 56 real people on the
/// reference capture, 13 instids carrying 2 rows each.
///
/// ## GW2EI's rule, reproduced
///
/// GW2EI regroups NON-SQUAD player agents purely by `InstID`, before any
/// downstream roster building -- `AgentManipulationHelper.cs:467-474`:
///
/// ```text
/// var nonSquadPlayersByInstids = nonSquadPlayerAgents.GroupBy(x => x.InstID)...
/// foreach (...) if (agents.Count > 1) RegroupAgents(...);
/// ```
///
/// `RegroupAgents` (same file, :283) keeps the FIRST agent of the group as
/// the representative, widens its aware window to the min/max of the group,
/// and redirects every src/dst combat row of the other members onto it --
/// i.e. the merged actor is the UNION of its parts. This function's
/// equivalent is: keep the first-seen `Player` as representative (so
/// `agent_addr`, name, spec are the first agent's) and extend
/// `agent_addrs` with the merged members', which is what every downstream
/// consumer folds over.
///
/// **Ordering matters and is deliberate.** In GW2EI the account-keyed
/// regroup on the next block (`:479-489`) applies to `AgentType.Player`
/// only -- i.e. SQUAD players (`dedupe_players` here). Non-squad players
/// are *never* account-grouped by EI. So account is not a competing key at
/// the same level: it is only used here as a FALLBACK for an enemy agent
/// whose instid could not be resolved at all (an addr that never appeared
/// on a non-extension row carrying a nonzero instid). That keeps the old
/// behaviour for such rows -- two rows that really are the same known
/// account still collapse -- while making instid the rule wherever it is
/// available, which on real logs is everywhere.
///
/// An enemy with no resolvable instid and a blank account stays a distinct
/// entry, exactly as before: never merge on `character`.
///
/// ## Instid-reuse hazard
///
/// arcdps instids are recycled over a long log, and elsewhere this codebase
/// resolves instid->addr *time-awarely* ([`InstidRegistry::resolve_at`]).
/// GW2EI's non-squad regroup is deliberately NOT time-aware -- it is a flat
/// `GroupBy(InstID)` with no aware-window or position check (contrast its
/// NPC branch at `:338-366`, which does gate on positions). Two genuinely
/// different enemy players who occupy the same instid at different times in
/// a long log therefore merge into one actor in EI, and merge here too.
/// That is a known fidelity limit of EI's rule, reproduced on purpose:
/// parity is the goal, and diverging would put every enemy-keyed
/// calibration off the reference. The key used here is
/// [`InstidRegistry::instid_of`] (the FIRST instid an addr was registered
/// under), which is this project's reconstruction of EI's
/// `AgentItem.InstID` and the same value the ei-json adapter exports as
/// `instanceID` -- so the merge granularity and the exported id agree by
/// construction.
///
/// NPCs/gadgets never go through this function at all -- distinct spawns
/// are distinct.
fn dedupe_enemy_players(players: &mut Vec<Player>, registry: &InstidRegistry) {
    /// Merge key: instid where resolvable (GW2EI's rule), else a known
    /// account, else nothing (row stays distinct).
    enum Key { Instid(u16), Account(String) }

    let mut by_instid: BTreeMap<u16, usize> = BTreeMap::new();
    let mut by_account: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<Player> = Vec::new();
    for p in players.drain(..) {
        let key = match registry.instid_of(p.agent_addr) {
            Some(instid) => Key::Instid(instid),
            None if !p.account.is_empty() => Key::Account(p.account.clone()),
            None => { out.push(p); continue; }
        };
        let slot = match &key {
            Key::Instid(i) => by_instid.get(i).copied(),
            Key::Account(a) => by_account.get(a).copied(),
        };
        match slot {
            Some(i) => { out[i].agent_addrs.extend(p.agent_addrs); }
            None => {
                match key {
                    Key::Instid(i) => { by_instid.insert(i, out.len()); }
                    Key::Account(a) => { by_account.insert(a, out.len()); }
                }
                out.push(p);
            }
        }
    }
    *players = out;
}

// WvW map id (MAP_ID statechange `src_agent`) → display name.
// Ids/names cross-checked against GW2EI's `MapIDs` (LogLogic/WvW) and the
// golden fixture (Green Alpine Borderlands, id 95). Unknown ids fall back
// to the generic "World vs World" label rather than guessing.
//
// M15 Task 2: the id→name pairs moved into `maps::WVW_MAPS` so that the
// display name, the arena image URL and the combat-replay geometry can no
// longer disagree about which id is which map. Same five ids, same five
// strings, same fallback -- purely a de-duplication.
fn map_name(map_id: u32) -> &'static str {
    match maps::map_def(map_id) {
        Some(def) => def.name,
        None => "World vs World",
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
// M10 Task 3: widened u16 -> u32 alongside every other WvW team id -- these
// fixed ids are all small (well under u16::MAX today), but keeping the
// table's element type in lockstep with `Team::team_id`/`agent_team` avoids
// a truncating cast at every comparison site below.
const RED_TEAM_IDS: &[u32] = &[697, 705, 706, 707, 882, 885, 886, 2520, 2543];
const GREEN_TEAM_IDS: &[u32] = &[39, 2739, 2741, 2752, 2763, 2767];
const BLUE_TEAM_IDS: &[u32] = &[432, 433, 1277, 1282, 1989];

fn team_color(team_id: u32) -> String {
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
struct DynamicWvwTeamIds {
    red: u32,
    blue: u32,
    green: u32,
    /// The first three uint32 slots, `[red, blue, green]` shard ids (MOBJ).
    /// Decoded alongside the team ids because they are the same six-uint32
    /// payload; before MOBJ they were skipped as "unused by axilog today",
    /// which is no longer true -- `wvWMapData`'s `redShardID`/`blueShardID`/
    /// `greenShardID` are exactly these.
    red_shard: u32,
    blue_shard: u32,
    green_shard: u32,
}

fn parse_wvw_teams_event(e: &RawEvent) -> DynamicWvwTeamIds {
    let red_shard = e.src_agent as u32; // uint32[0]
    let blue_shard = (e.src_agent >> 32) as u32; // uint32[1]
    let green_shard = e.dst_agent as u32; // uint32[2]
    let red_team = (e.dst_agent >> 32) as u32; // uint32[3]
    let blue_team = e.value as u32; // uint32[4]
    let green_team = e.buff_dmg as u32; // uint32[5]
    DynamicWvwTeamIds {
        red: red_team,
        blue: blue_team,
        green: green_team,
        red_shard,
        blue_shard,
        green_shard,
    }
}

impl DynamicWvwTeamIds {
    /// The shard id for a TEAM ID this event itself names.
    ///
    /// Keyed on the team id and not on the resolved colour string, which
    /// would be wrong in a real case: `team_color_with` falls back to the
    /// static id table for any team id this event does not name, so a
    /// stale-table id could come back `"red"` while this event's own red
    /// team is a different id -- and would then be handed a shard belonging
    /// to some other world.
    fn shard_for_team(&self, team_id: u32) -> Option<u32> {
        if team_id == self.red {
            Some(self.red_shard)
        } else if team_id == self.blue {
            Some(self.blue_shard)
        } else if team_id == self.green {
            Some(self.green_shard)
        } else {
            None
        }
    }
}

/// Like `team_color`, but prefers the log's own dynamically-observed
/// red/blue/green ids (from CBTS_WVWTEAMS) when available, falling back to
/// the static table for any id the dynamic event doesn't cover (or when the
/// event is absent entirely).
fn team_color_with(team_id: u32, dynamic: Option<&DynamicWvwTeamIds>) -> String {
    if let Some(d) = dynamic {
        if team_id == d.red {
            return "red".into();
        } else if team_id == d.blue {
            return "blue".into();
        } else if team_id == d.green {
            return "green".into();
        }
    }
    team_color(team_id)
}

/// Parse per-agent WvW team ids and the POINT_OF_VIEW (recording) agent from
/// raw combat events. Shared by `apply` (friend/foe partition, below) and by
/// the analysis layer (pet/minion damage attribution in `analysis::damage`).
pub fn resolve_teams(raw: &RawLog) -> (BTreeMap<u64, u32>, Option<u64>) {
    let mut agent_team: BTreeMap<u64, u32> = BTreeMap::new();
    let mut recorded_by: Option<u64> = None;
    for e in &raw.events {
        if e.is_statechange == sc::TEAM_CHANGE {
            // The WvW team id is carried in the `value` field (i32 @ offset
            // 24), not `dst_agent` — verified against the golden fixture's
            // teamID (Task 16A). Every agent (players, NPCs, gadgets) gets
            // exactly one TEAM_CHANGE event.
            //
            // M10 Task 3: widened to `u32` (was a lossy `as u16` truncation)
            // -- dynamic WVWTEAMS ids (`DynamicWvwTeamIds`, parsed below)
            // are already u32, so a team id large enough to lose bits in the
            // old cast would have silently mismatched against them.
            agent_team.insert(e.src_agent, e.value as u32);
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
        enc.map_id = Some(map_id);
    }

    // CBTS_WVWTEAMS (sc=74): the log's real red/blue/green team ids, when
    // arcdps emitted the event (Task 2b). `find` takes the first — arcdps
    // emits this once per log, at most.
    let dynamic = raw.events.iter()
        .find(|e| e.is_statechange == sc::WVW_TEAMS)
        .map(parse_wvw_teams_event);

    // CBTS_SQCOMBATSTART (sc=9, LOG_START): wall-clock log start (Phase B
    // Task 8). See `sc::LOG_START`'s doc comment for the payload citation
    // trail -- `value` is the SERVER unix timestamp, `buff_dmg` the local
    // one, and the server field is what gets emitted. `as u32` first mirrors
    // arcdps's own `uint32_t` wire type before widening, matching
    // `DynamicWvwTeamIds`'s casts just above.
    enc.started_at_unix = raw.events.iter()
        .find(|e| e.is_statechange == sc::LOG_START)
        .map(|e| e.value as u32 as u64);

    // CBTS_IDTOGUID (sc=46) TEAM mappings: team id -> stable content GUID
    // (Task 2b). M10 Task 3: `local_id` is already `u32` on `GuidMapping` --
    // this used to truncate it to u16 for no reason (never a real precision
    // issue for team ids, but a lossy cast all the same).
    let team_guids: BTreeMap<u32, String> = raw.guid_map.iter()
        .filter(|g| g.content_type == ContentType::Team)
        .map(|g| (g.local_id, g.guid_hex()))
        .collect();

    let mut team_ids: Vec<u32> = agent_team.values().copied().collect();
    team_ids.sort_unstable(); team_ids.dedup();
    enc.teams = team_ids.iter().map(|&id| {
        let color = team_color_with(id, dynamic.as_ref());
        // Shards come from the same sc=74 event as the dynamic team ids, so
        // a team whose colour was resolved by the STATIC fallback table
        // (i.e. the event is absent, or names other ids) gets `None` rather
        // than a shard belonging to some other match. See `shard_for_team`.
        let shard_id = dynamic.as_ref().and_then(|d| d.shard_for_team(id));
        Team { color, team_id: id, guid: team_guids.get(&id).cloned(), shard_id }
    }).collect();

    // CBTS_WVWOBJECTIVESTATUS (sc=75) objective ownership timelines (MOBJ).
    enc.objectives = objectives::objectives(raw);

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

    // Enemy agent-row dedupe (Task 4, M2; rekeyed on instid by MINSTID):
    // collapse the several agent rows one enemy PERSON occupies into a
    // single Enemy, aggregating their raw addrs -- GW2EI's non-squad
    // `GroupBy(x => x.InstID)` regroup. See `dedupe_enemy_players` for the
    // key order (instid, then a known account as fallback, never
    // `character`) and the instid-reuse hazard EI's rule carries.
    dedupe_enemy_players(&mut enemy_players, &InstidRegistry::build(raw));
    for p in enemy_players {
        let team = agent_team.get(&p.agent_addr).map(|&t| team_color_with(t, dynamic.as_ref())).unwrap_or_default();
        enc.enemies.push(Enemy {
            id: p.agent_addr,
            instid: 0,
            name: p.character,
            team,
            is_player: true,
            marker: None,
            // MENEMYPROF: these agents were resolved as full `Player`s by
            // `model::resolve` (they ARE players -- arcdps fills their
            // `prof`/`is_elite` agent-table columns exactly as it does for
            // squad members), and this loop is the ONLY hop where that gets
            // discarded. Carry both across. Without this the enemy roster is
            // class-less and consumers grouping enemies by profession fall
            // back to the `name` string, which in WvW is the player's RANK
            // title ("Mithril Scout"), not their class.
            profession: Some(p.profession),
            elite_spec: Some(p.elite_spec),
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
    // CBTS_GUILD (sc=29): MEIGAP Task 3c. Also after dedupe, for the same
    // reason -- a relogged account's guild row may sit on any of its addrs,
    // so the lookup walks `agent_addrs` and takes the first hit in ADDR
    // order (deterministic; `guilds::collect_guild_event`, called from the
    // marker scan below, already applied GW2EI's per-agent
    // `FirstOrDefault` in event order).
    let mut guild_by_addr: BTreeMap<u64, String> = BTreeMap::new();
    let marker_res = markers::resolve_markers_and_guilds(raw, &mut guild_by_addr);
    for p in &mut enc.players {
        p.guild_id =
            p.agent_addrs.iter().find_map(|addr| guild_by_addr.get(addr)).cloned();
        p.marker = markers::final_marker(&marker_res.open, &p.agent_addrs);
        p.commander_tag = markers::final_commander_tag(
            &marker_res.open,
            &marker_res.ever_commander,
            &marker_res.commander_segments,
            &p.agent_addrs,
        );
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
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![addr] }
    }
    /// A log with no events -- so `InstidRegistry::instid_of` resolves
    /// nothing and `dedupe_enemy_players` exercises its account fallback.
    fn empty_log() -> crate::evtc::RawLog {
        crate::evtc::RawLog {
            header: RawHeader { build: "20260101".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events: vec![], guid_map: vec![],
        }
    }
    /// A plain (non-statechange) row registering `addr` under `instid`,
    /// which is all `InstidRegistry::instid_of` needs.
    fn damage_event(addr: u64, instid: u16) -> RawEvent {
        RawEvent { time: 0, src_agent: addr, dst_agent: 0, value: 1, buff_dmg: 0,
            overstack: 0, skillid: 1, src_instid: instid, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    fn agent(addr: u64, is_elite: u32, name: &[u8]) -> RawAgent {
        RawAgent { addr, prof: 5, is_elite,
            toughness: 0, concentration: 0, healing: 0, hitbox_width: 0,
            condition: 0, hitbox_height: 0, name_raw: name.to_vec() }
    }
    fn team_change(addr: u64, team: u32) -> RawEvent {
        RawEvent { time: 0, src_agent: addr, dst_agent: 0, value: team as i32, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::TEAM_CHANGE, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    fn point_of_view(addr: u64) -> RawEvent {
        RawEvent { time: 0, src_agent: addr, dst_agent: 0, value: 0, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::POINT_OF_VIEW, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    /// Synthetic CBTS_WVWTEAMS event with all-zero shard ids. Most callers
    /// only care about team ids; see `wvw_teams_event_with_shards` for the
    /// full 6xu32 payload.
    fn wvw_teams_event(red: u32, blue: u32, green: u32) -> RawEvent {
        wvw_teams_event_with_shards(red, blue, green, 0, 0, 0)
    }
    /// Synthetic CBTS_WVWTEAMS event. Packs all six ids into the same
    /// `uint32[6]` layout `parse_wvw_teams_event` reads back:
    /// `[red_shard, blue_shard, green_shard, red_team, blue_team,
    /// green_team]` spanning `src_agent`(2), `dst_agent`(2), `value`(1),
    /// `buff_dmg`(1).
    fn wvw_teams_event_with_shards(
        red: u32, blue: u32, green: u32,
        red_shard: u32, blue_shard: u32, green_shard: u32,
    ) -> RawEvent {
        let src_agent = red_shard as u64 | ((blue_shard as u64) << 32);
        let dst_agent = green_shard as u64 | ((red as u64) << 32);
        RawEvent { time: 0, src_agent, dst_agent, value: blue as i32, buff_dmg: green as i32,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::WVW_TEAMS, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
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
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::ID_TO_GUID,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }
    #[test]
    fn dedupes_players_by_account() {
        let mut enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:0,
            build:"".into(), revision:1, recorded_by:None, teams:vec![],
            players: vec![player(1, ":A.1"), player(2, ":A.1"), player(3, ":B.2")],
            enemies: vec![], markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None };
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
            enemies: vec![], markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None };
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
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
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
                agent(1, 27, b"Alice\x00:Alice.1234\x005\x00"), // squad player
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
                agent(1, 27, b"Alice\x00:Alice.1234\x005\x00"), // squad player
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
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::MARKER, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    fn marker_guid(local_id: u32, guid_bytes: [u8; 16]) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: u64::from_le_bytes(guid_bytes[0..8].try_into().unwrap()),
            dst_agent: u64::from_le_bytes(guid_bytes[8..16].try_into().unwrap()),
            value: 0, buff_dmg: 0, overstack: 1, skillid: local_id,
            src_instid: 0, dst_instid: 0, src_master_instid: 0, dst_master_instid: 0,
            iff: 0, buff: 0, result: 0, is_activation: 0, is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::ID_TO_GUID, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
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
            agents: vec![agent(1, 27, b"Alice\x00:Alice.1234\x005\x00")],
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
                agent(1, 27, b"Alice\x00:Alice.1234\x005\x00"),
                agent(2, 27, b"Bob\x00:Bob.5678\x005\x00"),
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

    /// MOBJ: the sc=74 payload's first three uint32 slots are the shard
    /// ids, and each lands on the team the SAME event names -- keyed on
    /// team id, never on the resolved colour string.
    #[test]
    fn wvwteams_shard_ids_attach_to_the_teams_that_event_names() {
        let raw = RawLog {
            header: RawHeader { build: "20260701".into(), revision: 1, boss_id: 1 },
            agents: vec![agent(1, 27, b"Alice\x00:Alice.1234\x005\x00")],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 5001),
                // A team id the event does NOT name, but which the STATIC
                // table calls red (705 is in RED_TEAM_IDS). It must come
                // back shardless: the event's red shard belongs to 6002.
                team_change(2, 705),
                wvw_teams_event_with_shards(
                    /*red*/ 6002, /*blue*/ 7003, /*green*/ 5001,
                    /*red_shard*/ 1008, /*blue_shard*/ 1009, /*green_shard*/ 1010,
                ),
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);
        let team = |id: u32| enc.teams.iter().find(|t| t.team_id == id).expect("team present");

        assert_eq!(team(5001).shard_id, Some(1010), "green team gets the green shard");
        assert_eq!(
            (team(705).color.as_str(), team(705).shard_id),
            ("red", None),
            "a statically-coloured team the event does not name gets no shard"
        );
    }

    /// MOBJ: `apply` fills `enc.objectives` from sc=75, so the ownership
    /// timelines reach the encounter the same way markers and teams do.
    /// `wvw::objectives`' own tests cover the merge/catalog rules.
    #[test]
    fn apply_fills_objectives_from_status_events() {
        let status = |map_id: i32, objective_id: u32, team: i32, time: u64| RawEvent {
            time, src_agent: 0, dst_agent: 0, value: map_id, buff_dmg: team,
            overstack: 0, skillid: objective_id, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::WVW_OBJECTIVE_STATUS,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        };
        let raw = RawLog {
            header: RawHeader { build: "20260701".into(), revision: 1, boss_id: 1 },
            agents: vec![agent(1, 27, b"Alice\x00:Alice.1234\x005\x00")],
            skills: vec![],
            events: vec![
                point_of_view(1),
                team_change(1, 5001),
                status(96, 37, 433, 0),      // Blue Garrison, first sighting
                status(96, 37, 2767, 44684), // ... recaptured
            ],
            guid_map: vec![],
        };
        let enc = crate::model::resolve(&raw);
        assert_eq!(enc.objectives.len(), 1);
        assert_eq!(enc.objectives[0].kind, objectives::ObjectiveType::Keep);
        assert_eq!(enc.objectives[0].owners, vec![(433, 0), (2767, 44684)]);
    }

    /// Without a CBTS_WVWTEAMS event, team ids outside the static table
    /// resolve to "unknown" -- the fallback-of-last-resort, never silently
    /// green (regression guard for the pre-M2 placeholder bug).
    #[test]
    fn no_wvwteams_event_falls_back_to_static_table() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![agent(1, 27, b"Alice\x00:Alice.1234\x005\x00")],
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
        dedupe_enemy_players(&mut enemies, &InstidRegistry::build(&empty_log()));
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
        dedupe_enemy_players(&mut enemies, &InstidRegistry::build(&empty_log()));
        assert_eq!(enemies.len(), 2, "blank-account enemies must stay distinct");
    }

    /// MINSTID: two anonymised enemy agent rows (blank account, generic
    /// spec `character`) that share an INSTID are one person and collapse
    /// into one entry -- GW2EI's non-squad `GroupBy(x => x.InstID)` regroup
    /// (`AgentManipulationHelper.cs:467-474`). A third agent on a different
    /// instid stays distinct. This is the case the old account-keyed rule
    /// could not see at all, since WvW anonymisation blanks every enemy
    /// account.
    #[test]
    fn dedupe_enemy_players_merges_shared_instid_when_accounts_are_blank() {
        let mut a = player(9, "");
        a.character = "Druid".into();
        let mut b = player(10, "");
        b.character = "Druid".into();
        let mut c = player(11, "");
        c.character = "Druid".into();
        // addrs 9 and 10 both register under instid 7; addr 11 under 8.
        let mut raw = empty_log();
        raw.events = vec![
            damage_event(9, 7),
            damage_event(10, 7),
            damage_event(11, 8),
        ];
        let mut enemies = vec![a, b, c];
        dedupe_enemy_players(&mut enemies, &InstidRegistry::build(&raw));
        assert_eq!(enemies.len(), 2, "same-instid enemy agent rows are one person");
        assert_eq!(enemies[0].agent_addr, 9, "representative is the first agent row");
        let mut addrs = enemies[0].agent_addrs.clone();
        addrs.sort_unstable();
        assert_eq!(addrs, vec![9, 10], "merged row is the union of its parts");
        assert_eq!(enemies[1].agent_addrs, vec![11]);
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
                agent(1, 27, b"Alice\x00:Alice.1234\x005\x00"),
                agent(2, 27, b"Bob\x00:Bob.5678\x005\x00"),
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

    /// A statechange event carrying none of the payload any particular
    /// `sc::` constant cares about -- callers fill in only the field(s)
    /// their statechange actually uses (Phase B Task 8).
    fn base_state_event(time: u64, is_statechange: u8) -> RawEvent {
        RawEvent { time, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    /// Writes the SERVER unix timestamp onto a `LOG_START` event's payload
    /// -- `value`, per `sc::LOG_START`'s doc comment (`buff_dmg` carries the
    /// local/recording-machine timestamp instead). The one place this field
    /// assignment appears in these tests.
    fn set_log_start_server_time(ev: &mut RawEvent, unix_seconds: u32) {
        ev.value = unix_seconds as i32;
    }
    fn resolve_encounter_from(events: Vec<RawEvent>) -> Encounter {
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        };
        crate::model::resolve(&raw)
    }

    /// arcdps records a wall clock at log start. axilog defined the
    /// ordinal and never read it, which is why axibridge infers the start
    /// time from the .zevtc file's mtime -- wrong for any copied or
    /// restored file.
    #[test]
    fn extracts_the_log_start_wall_clock() {
        let mut ev = base_state_event(0, sc::LOG_START);
        set_log_start_server_time(&mut ev, 1_760_000_000);
        let enc = resolve_encounter_from(vec![ev]);
        assert_eq!(enc.started_at_unix, Some(1_760_000_000));
    }

    /// Absence is a real state -- a truncated or synthetic log may carry no
    /// LOG_START at all, and that must stay distinguishable from epoch
    /// zero.
    #[test]
    fn reports_absence_not_zero_without_a_log_start() {
        let enc = resolve_encounter_from(vec![]);
        assert_eq!(enc.started_at_unix, None);
    }
}
