//! Statistic blocks. Every block is an aggregate slot plus an entity-keyed
//! map, so a consumer learns the access pattern once.
use serde::Serialize;
use std::collections::BTreeMap;

pub mod damage;
pub mod defense;

/// The uniform entity-keyed map every block uses.
///
/// Keys serialize as decimal strings (`serde_json`'s integer-key
/// stringification), matching the catalogs.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ByEntity<T>(pub BTreeMap<u32, T>);

impl<T> Default for ByEntity<T> {
    fn default() -> Self {
        ByEntity(BTreeMap::new())
    }
}

impl<T> ByEntity<T> {
    pub fn insert(&mut self, entity_id: u32, value: T) {
        self.0.insert(entity_id, value);
    }
    pub fn get(&self, entity_id: u32) -> Option<&T> {
        self.0.get(&entity_id)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use crate::v1::entities::{build_entities, EntityIndex};

    /// The shared fixture every block test builds on: the committed
    /// anonymized WvW log, run through the real pipeline. Using the real
    /// fixture rather than hand-built structs is what makes these tests
    /// catch reprojection bugs on realistic shapes (sparse per-target maps,
    /// relogged players, NPC enemies).
    pub fn fixture_report() -> (crate::Report, EntityIndex) {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/wvw-small.anon.zevtc"
        ))
        .expect("read committed fixture");
        let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
        let enc = axilog_core::model::resolve(&raw);
        let metrics = axilog_core::analysis::analyze(&enc, &raw);
        let report =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
        let (_, index) = build_entities(&enc, &metrics);
        (report, index)
    }

    /// A minimal two-player report: `players[0]` is IN squad, `players[1]`
    /// is a non-squad friendly. Their agent addrs are 1 and 2.
    ///
    /// Every statistic starts at zero; a test sets only the fields it
    /// asserts on, then checks that a `squad` aggregate counted `players[0]`
    /// alone while `by_entity` kept both rows.
    ///
    /// The committed fixture CANNOT serve this purpose: every player in it
    /// is in-squad, so a fixture-based test passes whether or not the filter
    /// exists. That is exactly how the defect reached review in Task 5.
    pub fn two_player_report() -> (crate::Report, EntityIndex) {
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        use axilog_core::model::{Encounter, Player};

        fn player(addr: u64, account: &str, in_squad: bool) -> Player {
            Player {
                agent_addr: addr,
                account: account.into(),
                character: format!("Char{addr}"),
                profession: "Guardian".into(),
                elite_spec: "Firebrand".into(),
                team: "red".into(),
                subgroup: 1,
                in_squad,
                commander: false,
                marker: None,
                commander_tag: None,
                guild_id: None,
                agent_addrs: vec![addr],
            }
        }

        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![player(1, ":Squaddie.1", true), player(2, ":Pug.2", false)],
            enemies: vec![],
            markers: vec![],
            tick_rate: None,
        };
        let metrics = Metrics {
            players: vec![
                PlayerMetrics { agent_addr: 1, ..Default::default() },
                PlayerMetrics { agent_addr: 2, ..Default::default() },
            ],
            timeline: Timeline {
                resolution_ms: 1000,
                squad_damage: vec![0],
                cc_applied: vec![0],
                downs: vec![0],
            },
            boons: Default::default(),
            boon_uptime: Default::default(),
            boon_generation: Default::default(),
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            instance_ids: Default::default(),
            enemy_damage_out: Default::default(),
            skill_map: Default::default(),
        };
        let report =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
        let (_, index) = build_entities(&enc, &metrics);
        (report, index)
    }
}
