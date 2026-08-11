use axilog_core::analysis::Metrics;
use axilog_core::model::Encounter;
use serde::Serialize;
use std::collections::BTreeMap;

/// What an entity IS, replacing three overlapping signals the legacy shape
/// carried separately (`in_squad`, `is_player`, and membership in
/// `enemies[]` vs the `#[serde(skip)]` `ei_targets[]`).
///
/// Declaration order is the SORT order -- see `build_entities`.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Squad,
    /// Non-squad player on the squad's team -- GW2EI's
    /// `_nonSquadFriendlies`, which the legacy shape discarded entirely.
    FriendlyPlayer,
    EnemyPlayer,
    /// Every non-player enemy agent. `axilog_core::model::agent_kind`
    /// distinguishes gadgets from NPCs, but `model::Enemy` does not retain
    /// that, so a separate `Gadget` role would be unreachable. Adding one
    /// later is additive under the 1.x rules; see the spec's known
    /// simplifications.
    Npc,
}

/// One agent's IDENTITY. No statistics -- those live in `blocks`, keyed by
/// `id`.
///
/// This is the single place account and character names appear, which makes
/// the PII scrub a single pass rather than a hunt through nested structures.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct EntityOut {
    /// Dense index into `entities[]`, from 0. Stable WITHIN a report, not
    /// across reports -- join across logs on `account`.
    pub id: u32,
    pub role: Role,
    /// Players only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Players only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Non-player entities only -- they have neither account nor character.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Present exactly for player roles, preserving MENEMYPROF's property
    /// that presence is itself the "is this a real player" signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    /// Empty string when the agent has no elite spec, or one this project
    /// cannot name. Never a numeric spec id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elite_spec: Option<String>,
    pub team: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subgroup: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander: Option<CommanderOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// The arcdps agent address. A documented attribute, not a secret --
    /// a consumer correlating against raw arcdps or another tool needs it,
    /// and hiding it is what forced the legacy EI side channel to exist.
    pub agent_addr: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instid: Option<u16>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct CommanderOut {
    pub variant: String,
    pub guid: String,
}

/// Join tables from the two id spaces the analysis layer uses onto entity
/// ids. Built once with the roster so no block has to re-derive it.
#[derive(Debug, Default, Clone)]
pub struct EntityIndex {
    by_addr: BTreeMap<u64, u32>,
    by_enemy: BTreeMap<u64, u32>,
    role: BTreeMap<u32, Role>,
}

impl EntityIndex {
    pub fn by_agent_addr(&self, addr: u64) -> Option<u32> {
        self.by_addr.get(&addr).copied()
    }
    pub fn by_enemy_id(&self, enemy_id: u64) -> Option<u32> {
        self.by_enemy.get(&enemy_id).copied()
    }
    /// The entity's [`Role`], for callers that need to filter an aggregate
    /// down to a specific membership (e.g. a "squad" total must include
    /// only `Role::Squad` entities, not every friendly player).
    pub fn role_of(&self, entity_id: u32) -> Option<Role> {
        self.role.get(&entity_id).copied()
    }
}

/// Build the roster and its join index.
///
/// The sort key is FULLY specified rather than left to encounter order,
/// because `id` is the join key for every block and the goldens are
/// byte-exact diffs: role, then team, then subgroup, then account, then
/// character/name, then `agent_addr` as the final tiebreak.
pub fn build_entities(enc: &Encounter, metrics: &Metrics) -> (Vec<EntityOut>, EntityIndex) {
    // (sort key, entity-without-id, every addr that should resolve to it,
    //  enemy id when it came from `enc.enemies`)
    struct Pending {
        key: (Role, String, u8, String, String, u64),
        entity: EntityOut,
        addrs: Vec<u64>,
        enemy_id: Option<u64>,
    }

    let mut pending: Vec<Pending> = Vec::with_capacity(enc.players.len() + enc.enemies.len());

    for p in &enc.players {
        let role = if p.in_squad { Role::Squad } else { Role::FriendlyPlayer };
        pending.push(Pending {
            key: (
                role,
                p.team.clone(),
                p.subgroup,
                p.account.clone(),
                p.character.clone(),
                p.agent_addr,
            ),
            entity: EntityOut {
                id: 0,
                role,
                account: Some(p.account.clone()),
                character: Some(p.character.clone()),
                name: None,
                profession: Some(p.profession.clone()),
                elite_spec: Some(p.elite_spec.clone()),
                team: p.team.clone(),
                subgroup: Some(p.subgroup),
                commander: p.commander_tag.as_ref().map(|c| CommanderOut {
                    variant: c.variant.clone(),
                    guid: c.guid.clone(),
                }),
                guild_id: p.guild_id.clone(),
                marker: p.marker.clone(),
                agent_addr: p.agent_addr,
                instid: metrics.instance_ids.get(&p.agent_addr).copied(),
            },
            addrs: p.agent_addrs.clone(),
            enemy_id: None,
        });
    }

    for e in &enc.enemies {
        // `is_player` is the friend/foe-split roster's player flag;
        // `profession.is_some()` (MENEMYPROF) agrees with it on every real
        // log and is the signal consumers use.
        let role = if e.is_player { Role::EnemyPlayer } else { Role::Npc };
        let is_player_role = matches!(role, Role::EnemyPlayer);
        pending.push(Pending {
            key: (role, e.team.clone(), 0, String::new(), e.name.clone(), e.id),
            entity: EntityOut {
                id: 0,
                role,
                account: None,
                character: None,
                name: (!is_player_role).then(|| e.name.clone()),
                profession: e.profession.clone(),
                elite_spec: e.elite_spec.clone(),
                team: e.team.clone(),
                subgroup: None,
                commander: None,
                guild_id: None,
                marker: e.marker.clone(),
                agent_addr: e.id,
                instid: metrics.instance_ids.get(&e.id).copied(),
            },
            addrs: e.agent_addrs.clone(),
            enemy_id: Some(e.id),
        });
    }

    pending.sort_by(|a, b| a.key.cmp(&b.key));

    let mut index = EntityIndex::default();
    let mut entities = Vec::with_capacity(pending.len());
    for (i, mut p) in pending.into_iter().enumerate() {
        let id = i as u32;
        p.entity.id = id;
        for addr in p.addrs {
            index.by_addr.insert(addr, id);
        }
        index.by_addr.insert(p.entity.agent_addr, id);
        if let Some(enemy_id) = p.enemy_id {
            index.by_enemy.insert(enemy_id, id);
        }
        index.role.insert(id, p.entity.role);
        entities.push(p.entity);
    }

    (entities, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{CommanderTag, Encounter, Enemy, Player};
    use axilog_core::analysis::Metrics;

    fn player(addr: u64, account: &str, in_squad: bool, subgroup: u8) -> Player {
        Player {
            agent_addr: addr,
            account: account.into(),
            character: format!("Char{addr}"),
            profession: "Guardian".into(),
            elite_spec: "Firebrand".into(),
            team: "red".into(),
            subgroup,
            in_squad,
            commander: false,
            marker: None,
            commander_tag: None,
            guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    fn enemy(id: u64, name: &str, is_player: bool, profession: Option<&str>) -> Enemy {
        Enemy {
            id,
            instid: id as u16,
            name: name.into(),
            team: "green".into(),
            is_player,
            marker: None,
            profession: profession.map(|s| s.into()),
            elite_spec: profession.map(|_| String::new()),
            agent_addrs: vec![id],
        }
    }

    fn encounter(players: Vec<Player>, enemies: Vec<Enemy>) -> Encounter {
        Encounter {
            kind: "wvw".into(),
            map: "Green Alpine Borderlands".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players,
            enemies,
            markers: vec![],
            tick_rate: None,
        }
    }

    #[test]
    fn assigns_dense_ids_in_deterministic_role_then_team_then_subgroup_order() {
        // Deliberately inserted out of order: an enemy first, then squad
        // players with subgroups descending. Ids must not depend on input
        // order, because they are the join key for every block and the
        // goldens are byte-exact diffs.
        let enc = encounter(
            vec![player(20, ":Bea.2", true, 3), player(10, ":Al.1", true, 1)],
            vec![enemy(90, "Gold Invader", true, Some("Reaper"))],
        );
        let (entities, _) = build_entities(&enc, &Metrics::default());

        let ids: Vec<u32> = entities.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![0, 1, 2], "ids are dense array indices from 0");

        let accounts: Vec<&str> = entities.iter().map(|e| e.account.as_deref().unwrap_or("")).collect();
        assert_eq!(accounts, vec![":Al.1", ":Bea.2", ""], "squad sorts before enemy, subgroup ascending");
        assert_eq!(entities[2].role, Role::EnemyPlayer);
    }

    #[test]
    fn role_separates_squad_from_non_squad_friendly_players() {
        let enc = encounter(
            vec![player(10, ":Al.1", true, 1), player(11, ":Pug.9", false, 0)],
            vec![],
        );
        let (entities, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities[0].role, Role::Squad);
        assert_eq!(entities[1].role, Role::FriendlyPlayer);
    }

    #[test]
    fn npcs_carry_a_name_and_no_account_or_profession() {
        let enc = encounter(vec![], vec![enemy(90, "Footman", false, None)]);
        let (entities, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities[0].role, Role::Npc);
        assert_eq!(entities[0].name.as_deref(), Some("Footman"));
        assert!(entities[0].account.is_none(), "an NPC has no account");
        assert!(entities[0].profession.is_none(), "an NPC has no profession");

        let v = serde_json::to_value(&entities[0]).expect("serializable");
        assert!(v.get("account").is_none(), "absent fields are omitted, never null");
        assert!(v.get("character").is_none());
    }

    #[test]
    fn player_entities_carry_account_and_character_not_name() {
        let enc = encounter(vec![player(10, ":Al.1", true, 1)], vec![]);
        let (entities, _) = build_entities(&enc, &Metrics::default());
        let v = serde_json::to_value(&entities[0]).expect("serializable");
        assert_eq!(v["account"], ":Al.1");
        assert_eq!(v["character"], "Char10");
        assert!(v.get("name").is_none(), "players use account/character, not name");
    }

    #[test]
    fn the_index_joins_both_agent_addrs_and_enemy_ids_to_entity_ids() {
        let enc = encounter(
            vec![player(10, ":Al.1", true, 1)],
            vec![enemy(90, "Gold Invader", true, Some("Reaper"))],
        );
        let (entities, index) = build_entities(&enc, &Metrics::default());
        assert_eq!(index.by_agent_addr(10), Some(entities[0].id));
        assert_eq!(index.by_enemy_id(90), Some(entities[1].id));
        assert_eq!(index.by_agent_addr(9999), None);
    }

    #[test]
    fn every_agent_addr_of_a_relogged_player_resolves_to_one_entity() {
        // arcdps issues a new addr per relog; `agent_addrs` holds them all
        // and `agent_addr` is the representative. A block keyed by any of
        // them must land on the same entity.
        let mut p = player(10, ":Al.1", true, 1);
        p.agent_addrs = vec![10, 11, 12];
        let enc = encounter(vec![p], vec![]);
        let (entities, index) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities.len(), 1, "relogs are one person, not three");
        for addr in [10, 11, 12] {
            assert_eq!(index.by_agent_addr(addr), Some(0), "addr {addr} must resolve");
        }
    }
}
