//! Wire-shape coverage for `blocks.boons.<entity>.<boon>.sources`.
//!
//! The estimate only runs on post-2026-05 (buff-rework) logs and the
//! committed WvW fixture predates the rework, so the key-set golden can never
//! see the field. This pins the schema half: the entity join, omit-when-empty,
//! and the skill catalog reference. The attribution itself is tested in
//! `axilog_core::analysis::boon_sources`.

use axilog_core::analysis::boon_sources::BoonSourceSplit;
use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
use axilog_core::model::{Encounter, Player};
use std::collections::BTreeMap;

const RESISTANCE: u32 = 26980;
const STABILITY: u32 = 1122;
const LUMINANCE: u32 = 73132;

fn player(addr: u64, account: &str) -> Player {
    Player {
        agent_addr: addr,
        account: account.into(),
        character: format!("Char{addr}"),
        profession: "Guardian".into(),
        elite_spec: "Firebrand".into(),
        team: "red".into(),
        subgroup: 1,
        in_squad: true,
        commander: false,
        marker: None,
        commander_tag: None,
        guild_id: None,
        agent_addrs: vec![addr],
    }
}

/// A one-player document with the supplied boon sources, serialized.
fn document(sources: BTreeMap<(u64, u32), BoonSourceSplit>) -> serde_json::Value {
    let enc = Encounter { log_start_ms: 0,
        kind: "wvw".into(), pve: None,
        map: String::new(),
        duration_ms: 1000,
        build: String::new(),
        revision: 1,
        recorded_by: None,
        teams: vec![],
        players: vec![player(1, ":Squaddie.1")],
        enemies: vec![],
        markers: vec![], ground_markers: vec![],
        tick_rate: None,
        objectives: Vec::new(),
        started_at_unix: None, map_id: None,
    };
    let metrics = Metrics {
        players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
        timeline: Timeline {
            resolution_ms: 1000,
            squad_damage: vec![0],
            cc_applied: vec![0],
            downs: vec![0],
            strips: vec![0],
        },
        boons: Default::default(),
        boon_uptime: Default::default(),
        boon_generation: Default::default(), boon_sources: sources,
        warnings: Default::default(),
        healing_extension: Default::default(),
        combat_participant_enemies: Default::default(),
        instance_ids: Default::default(),
        enemy_damage_out: Default::default(),
        skill_map: Default::default(),
        log_skill_names: Default::default(),
    };
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, false, false, false, None);
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &Default::default(),
    );
    serde_json::to_value(&v1).expect("serializable")
}

fn entity_boons(doc: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    let by_entity = doc["blocks"]["boons"]["by_entity"].as_object().expect("boons.by_entity");
    assert_eq!(by_entity.len(), 1);
    by_entity.values().next().unwrap().as_object().unwrap()
}

#[test]
fn sources_land_on_the_appliers_boon_row_and_reference_the_skill() {
    let split = BoonSourceSplit { by_skill: [(LUMINANCE, 1200)].into_iter().collect(), other_ms: 800 };
    let doc = document([((1, RESISTANCE), split)].into_iter().collect());
    let row = &entity_boons(&doc)[&RESISTANCE.to_string()];
    assert_eq!(row["sources"]["by_skill"][&LUMINANCE.to_string()], 1200);
    assert_eq!(row["sources"]["other_ms"], 800);
    assert!(
        doc["catalogs"]["skills"].get(LUMINANCE.to_string()).is_some(),
        "attributed skill ids must resolve through catalogs.skills"
    );
    // A boon with no split stays unmarked rather than reading as 0% skill.
    assert!(entity_boons(&doc)[&STABILITY.to_string()].get("sources").is_none());
}

#[test]
fn an_empty_split_is_omitted() {
    let doc = document([((1, RESISTANCE), BoonSourceSplit::default())].into_iter().collect());
    assert!(entity_boons(&doc)[&RESISTANCE.to_string()].get("sources").is_none());
}
