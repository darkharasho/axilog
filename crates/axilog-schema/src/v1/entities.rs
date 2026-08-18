use crate::v1::order::SourceOrder;
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
    /// Every `enc.enemies`-derived entity -- `Role::Npc` AND
    /// `Role::EnemyPlayer` alike, neither of which has an `account` or
    /// `character`. Enemy players are still players, but their identity
    /// comes from the encounter's foe-side roster (`Enemy::name`), not the
    /// friend-side one (`Player::account`/`Player::character`), so they get
    /// this field instead of those two rather than going label-less. See
    /// `build_entities`'s enemy loop for the full rationale.
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
    /// The commander tag this entity carries, when it carries one.
    ///
    /// Derived from `Player::commander_tag` ALONE. The legacy shape also
    /// carries a plain `PlayerOut::commander: bool`, which is deliberately
    /// not reprojected -- and unlike [`Role::FriendlyPlayer`], this costs
    /// nothing even in principle, because the bool is not independent data:
    /// `wvw::apply` assigns `p.commander = p.commander_tag.is_some()`
    /// unconditionally, after dedupe, as the last write to either field
    /// (`crates/axilog-core/src/wvw/mod.rs`). The non-WvW `model::resolve`
    /// path leaves both at their `false`/`None` defaults, so the identity
    /// holds there too.
    ///
    /// So `commander.is_some()` here IS `PlayerOut::commander`, exactly,
    /// and `v1_equivalence.rs`'s completeness checklist asserts that
    /// identity per player rather than taking this comment's word for it.
    /// Carrying the bool separately would only create a second commander
    /// signal that could drift out of agreement with the tag.
    ///
    /// If the upstream derivation ever changes so the two can differ, that
    /// assertion fails and this decision gets remade deliberately instead
    /// of being rediscovered as a data loss.
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
    /// Whether this entity interacted with the squad at all -- it dealt
    /// damage to the squad, took damage from the squad, or took CC from the
    /// squad. This is the predicate behind the legacy report's
    /// combat-participant `enemies[]` view, preserved here so that view
    /// stays expressible as a filter over `entities[]` rather than being
    /// lost -- see `Metrics::combat_participant_enemies`'s doc comment for
    /// the exact criteria. Always `true` for squad members (and non-squad
    /// friendlies, which are never filtered by this predicate on the
    /// legacy side either). Never optional: absence would be ambiguous
    /// between "did not participate" and "not computed".
    pub combat_participant: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct CommanderOut {
    pub variant: String,
    pub guid: String,
    /// Terminated, half-open `[tag-on, tag-off)` windows in **arcdps
    /// session time** -- the same base as `markers[].time_ms`, NOT
    /// log-relative and NOT clipped to `[0, duration_ms]`. Passed through
    /// from `model::CommanderTag::segments` with no rebase; a consumer that
    /// wants encounter-relative values must subtract the log's `t0` itself,
    /// which this document carries as
    /// [`crate::v1::EncounterOut::log_start_ms`] (as `analysis::distance`
    /// does internally). Literal per-instance commander-tag
    /// holds, not a coalesced whole-fight span (see
    /// `model::CommanderTag::segments` for the full GW2EI-sourced
    /// rationale: no coverage threshold, no log-end fallback beyond a
    /// genuinely still-open instance).
    ///
    /// An empty vec on a *present* `CommanderOut` means the tag was
    /// detected (this player held it at some point) but its windows could
    /// not be resolved from this map alone -- NOT that the player never
    /// commanded. `commander` being present at all is the "did they ever
    /// command" signal; `segments` is "when".
    pub segments: Vec<(u64, u64)>,
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
pub fn build_entities(
    enc: &Encounter,
    metrics: &Metrics,
) -> (Vec<EntityOut>, EntityIndex, SourceOrder) {
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
                    segments: c.segments.clone(),
                }),
                guild_id: p.guild_id.clone(),
                marker: p.marker.clone(),
                agent_addr: p.agent_addr,
                instid: metrics.instance_ids.get(&p.agent_addr).copied(),
                // Players (squad and non-squad friendly alike) are never
                // filtered by the legacy combat-participant predicate --
                // that filter only ever applies to `enc.enemies`.
                combat_participant: true,
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
        pending.push(Pending {
            key: (role, e.team.clone(), 0, String::new(), e.name.clone(), e.id),
            entity: EntityOut {
                id: 0,
                role,
                account: None,
                character: None,
                // Unconditional, for every `enc.enemies` row regardless of
                // role -- NOT gated on `!is_player_role` as an earlier
                // version of this had it. `Role::EnemyPlayer` needs a label
                // too: ei-json emits `targets[].name` unconditionally (EI
                // parity is a floor per the program's decision 3), and spec
                // #1's design claims `entities[]` is the single roster
                // carrying identity -- both claims were false for this role
                // until this line, since an enemy-player entity previously
                // carried neither `character` nor `name`, making it
                // label-less. This is not a PII regression: it is still the
                // one place names live (`entities[]`), matching every other
                // row here.
                name: Some(e.name.clone()),
                profession: e.profession.clone(),
                elite_spec: e.elite_spec.clone(),
                team: e.team.clone(),
                subgroup: None,
                commander: None,
                guild_id: None,
                marker: e.marker.clone(),
                agent_addr: e.id,
                instid: metrics.instance_ids.get(&e.id).copied(),
                // The single definition of this predicate lives on
                // `Metrics::combat_participant_enemies` -- see its doc
                // comment for the exact criteria (dealt damage to the
                // squad, took damage from the squad, or took CC from the
                // squad). `crate::build_report`'s `Report.enemies` filter
                // reads the same set, so this is not a second definition.
                combat_participant: metrics.combat_participant_enemies.contains(&e.id),
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

    // Source order is derived AFTER ids are assigned, by re-walking the
    // encounter in its own order and resolving each agent through the
    // index. Deriving it from the index rather than tracking it through
    // the sort keeps the sort logic untouched -- and it means a lookup
    // miss is impossible by construction, since every encounter agent
    // produced a `Pending`.
    let players = enc
        .players
        .iter()
        .filter_map(|p| index.by_agent_addr(p.agent_addr))
        .collect::<Vec<_>>();
    debug_assert_eq!(
        players.len(),
        enc.players.len(),
        "every encounter player must resolve to an entity"
    );

    let targets = enc
        .enemies
        .iter()
        .filter(|e| e.is_player)
        .filter_map(|e| index.by_enemy_id(e.id))
        .collect::<Vec<_>>();

    (entities, index, SourceOrder::new(players, targets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{Encounter, Enemy, Player};
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
        Encounter { log_start_ms: 0,
            kind: "wvw".into(),
            map: "Green Alpine Borderlands".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players,
            enemies,
            markers: vec![], ground_markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
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
        let (entities, _, _) = build_entities(&enc, &Metrics::default());

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
        let (entities, _, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities[0].role, Role::Squad);
        assert_eq!(entities[1].role, Role::FriendlyPlayer);
    }

    #[test]
    fn npcs_carry_a_name_and_no_account_or_profession() {
        let enc = encounter(vec![], vec![enemy(90, "Footman", false, None)]);
        let (entities, _, _) = build_entities(&enc, &Metrics::default());
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
        let (entities, _, _) = build_entities(&enc, &Metrics::default());
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
        let (entities, index, _) = build_entities(&enc, &Metrics::default());
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
        let (entities, index, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities.len(), 1, "relogs are one person, not three");
        for addr in [10, 11, 12] {
            assert_eq!(index.by_agent_addr(addr), Some(0), "addr {addr} must resolve");
        }
    }
}
