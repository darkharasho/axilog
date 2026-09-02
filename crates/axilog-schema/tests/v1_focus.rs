//! `blocks.focus` reaches the wire, with the right rows on the right ids.
//!
//! This block cannot be covered by `v1_shape.rs`'s golden the way every
//! other block is, and the reason is a property of the committed fixture
//! rather than of the block: `fixtures/wvw-small.anon.zevtc` contains ZERO
//! `CBTS_ANIMATIONSTART` events (32 enemy players, 0 cast-start rows --
//! the unanonymized original is identical, so this is how the log was
//! recorded, not something the scrubber removed). `focus` reads exactly
//! those rows, so on that fixture the whole block is legitimately empty and
//! the golden can only ever pin its always-present scalars.
//!
//! Rather than commit a second multi-megabyte WvW log for one block, this
//! file synthesizes the smallest encounter that exercises it. What it
//! guards that the golden cannot: the `skills[]` rows exist at all, the
//! positional `FocusDetail` -> entity-id join lands each row on the right
//! player, and `coverage.focus` reports `present` rather than `empty`.

use axilog_core::evtc::{RawEvent, RawHeader, RawLog};
use axilog_core::model::{Enemy, Encounter, Player};

fn base_event() -> RawEvent {
    RawEvent {
        time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
        skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
        dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
        is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0,
        is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
    }
}

fn cast(time: u64, src: u64, dst: u64, skill: u32) -> RawEvent {
    let mut e = base_event();
    e.time = time; e.src_agent = src; e.dst_agent = dst; e.skillid = skill;
    e.is_statechange = axilog_core::evtc::event::sc::ANIMATION_START;
    e
}

fn hit(time: u64, src: u64, dst: u64, skill: u32, dmg: i32) -> RawEvent {
    let mut e = base_event();
    e.time = time; e.src_agent = src; e.dst_agent = dst; e.skillid = skill; e.value = dmg;
    e
}

fn down(time: u64, who: u64) -> RawEvent {
    let mut e = base_event();
    e.time = time; e.src_agent = who;
    e.is_statechange = axilog_core::evtc::event::sc::CHANGE_DOWN;
    e
}

fn player(addr: u64, in_squad: bool) -> Player {
    Player {
        agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
        profession: "Guardian".into(), elite_spec: "".into(), team: "red".into(),
        subgroup: 1, in_squad, commander: false, marker: None, commander_tag: None,
        guild_id: None, agent_addrs: vec![addr],
    }
}

fn enemy(addr: u64) -> Enemy {
    Enemy {
        id: addr, instid: addr as u16, name: format!("E{addr}"), team: "blue".into(),
        is_player: true, marker: None, profession: Some("Necromancer".into()),
        elite_spec: Some("".into()), agent_addrs: vec![addr],
    }
}

/// Three friendlies -- two in the squad, one not -- and one enemy player.
/// P100 draws 3 casts, P101 one, and the non-squad friendly P102 draws two
/// that must not reach the block at all.
fn fixture() -> (Encounter, RawLog) {
    let enc = Encounter {
        kind: "wvw".into(), pve: None, map: "".into(), duration_ms: 60_000,
        build: "".into(), revision: 1, recorded_by: None, teams: vec![],
        players: vec![player(100, true), player(101, true), player(102, false)],
        enemies: vec![enemy(200)], markers: vec![], ground_markers: vec![],
        tick_rate: None, objectives: Vec::new(), started_at_unix: None,
        log_start_ms: 0, map_id: None,
    };
    let mut events = vec![
        cast(1_000, 200, 100, 9), cast(2_000, 200, 100, 9), cast(9_500, 200, 100, 31),
        cast(4_000, 200, 101, 9),
        cast(5_000, 200, 102, 9), cast(5_500, 200, 102, 9),
        // Two connecting strikes for skill 9 and one for 31, so the block's
        // per-skill damage is distinguishable from its cast counts.
        hit(2_100, 200, 100, 9, 1_000), hit(4_100, 200, 101, 9, 2_000),
        hit(9_600, 200, 100, 31, 9_000),
        down(10_000, 100),
    ];
    events.sort_by_key(|e| e.time);
    let raw = RawLog {
        header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
        agents: vec![], skills: vec![], events, guid_map: Default::default(),
    };
    (enc, raw)
}

fn build() -> serde_json::Value {
    let (enc, raw) = fixture();
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let focus = axilog_core::analysis::focus::build(&enc, &raw);
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &legacy, "0.0.0-test", None,
        &axilog_schema::v1::Passes { focus: Some(&focus), ..Default::default() },
    );
    serde_json::to_value(&v1).expect("serializable")
}

/// The entity id whose `entities[]` row has this character name. The block
/// is keyed by id, and ids are assigned by `build_entities`' own sort --
/// looking one up is the only honest way to assert which player a row is.
fn id_of(doc: &serde_json::Value, character: &str) -> String {
    doc["entities"]
        .as_array()
        .expect("entities is an array")
        .iter()
        .find(|e| e["character"] == character)
        .unwrap_or_else(|| panic!("no entity named {character}"))["id"]
        .as_u64()
        .expect("id is a number")
        .to_string()
}

#[test]
fn the_block_reaches_the_wire_with_squad_scoped_totals() {
    let doc = build();
    assert_eq!(doc["coverage"]["focus"], "present");
    let b = &doc["blocks"]["focus"];
    // Four casts, not six: the two aimed at the non-squad friendly are
    // outside both the numerator and the `squad_size` denominator.
    assert_eq!(b["squad_size"], 2);
    assert_eq!(b["total_casts"], 4);
    assert_eq!(b["pre_down_window_ms"], axilog_core::analysis::focus::PRE_DOWN_WINDOW_MS);
    // (1000 + 2000 + 9000) / 3 strikes.
    assert_eq!(b["mean_strike_damage"], 4000.0);
}

#[test]
fn each_row_lands_on_the_player_it_was_measured_for() {
    let doc = build();
    let rows = &doc["blocks"]["focus"]["by_entity"];
    let p100 = &rows[id_of(&doc, "P100")];
    let p101 = &rows[id_of(&doc, "P101")];
    assert_eq!(p100["casts_drawn"], 3);
    assert_eq!(p101["casts_drawn"], 1);
    // 3/4 of the casts against an even 1/2 share.
    assert_eq!(p100["focus_index"], 1.5);
    assert_eq!(p101["focus_index"], 0.5);
    assert_eq!(p100["downs"], 1);
    // Skill 31 at 9500ms is inside the 3s window before the 10000ms down;
    // the two at 1000/2000ms are not.
    assert_eq!(p100["pre_down_casts"], 1);
    assert_eq!(p101["downs"], 0);
    // The non-squad friendly gets no row: it is neither counted nor
    // reported, so a consumer cannot mistake a zero for "never targeted".
    assert!(
        !rows.as_object().expect("by_entity is an object").contains_key(&id_of(&doc, "P102")),
        "a non-squad friendly must not appear in blocks.focus"
    );
}

#[test]
fn skill_rows_carry_pooled_pairs_and_resolve_through_the_catalog() {
    let doc = build();
    let skills = doc["blocks"]["focus"]["skills"].as_array().expect("skills is an array");
    let row = |id: u64| {
        skills.iter().find(|s| s["skill"] == id).unwrap_or_else(|| panic!("no skill {id} row"))
    };
    // Cast counts and damage are measured on different event streams --
    // skill 9 has 3 casts but only 2 connecting strikes.
    assert_eq!(row(9)["casts_at_squad"], 3);
    assert_eq!(row(9)["hits"], 2);
    assert_eq!(row(9)["damage_total"], 3_000);
    assert_eq!(row(31)["casts_at_squad"], 1);
    assert_eq!(row(31)["hits"], 1);
    assert_eq!(row(31)["damage_total"], 9_000);
    // No mean on the wire: two logs' (hits, damage_total) pairs add, their
    // means do not, and the median enemy skill connects three times in a
    // real log.
    assert!(row(9).get("mean_damage").is_none(), "the wire carries the pair, not a mean");
    // Every referenced id is a catalog key -- the same contract
    // `v1_shape::every_referenced_id_resolves` enforces for other blocks.
    for s in skills {
        let id = s["skill"].as_u64().expect("skill id").to_string();
        assert!(
            doc["catalogs"]["skills"].get(&id).is_some(),
            "skill {id} referenced by blocks.focus is missing from catalogs.skills"
        );
    }
}

/// A log with enemy players but no cast-start rows -- the shape of the
/// committed WvW fixture -- must report `empty`, not `present` with zeros.
#[test]
fn a_log_with_no_cast_rows_is_empty_not_present() {
    let (enc, mut raw) = fixture();
    raw.events
        .retain(|e| e.is_statechange != axilog_core::evtc::event::sc::ANIMATION_START);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let focus = axilog_core::analysis::focus::build(&enc, &raw);
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &legacy, "0.0.0-test", None,
        &axilog_schema::v1::Passes { focus: Some(&focus), ..Default::default() },
    );
    let doc = serde_json::to_value(&v1).expect("serializable");
    assert_eq!(doc["coverage"]["focus"], "empty");
    assert_eq!(doc["blocks"]["focus"]["total_casts"], 0);
}

/// No pass supplied at all is `not_computed`, and the block is omitted --
/// the distinction the whole `coverage` map exists to make.
#[test]
fn no_pass_is_not_computed_and_omits_the_block() {
    let (enc, raw) = fixture();
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &legacy, "0.0.0-test", None,
        &axilog_schema::v1::Passes::default(),
    );
    let doc = serde_json::to_value(&v1).expect("serializable");
    assert_eq!(doc["coverage"]["focus"], "not_computed");
    assert!(doc["blocks"].get("focus").is_none());
}
