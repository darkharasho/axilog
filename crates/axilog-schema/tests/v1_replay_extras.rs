//! Wire-shape coverage for the replay eye-candy families on
//! `blocks.replay`.
//!
//! The committed fixture cannot carry three of the four. It is arcdps build
//! `20260114`, and the capture-point family (`CBTS_GADGETCAPTURE*`, sc
//! 80-83) only starts being emitted at build `20260602`; it also happens to
//! contain no `CBTS_TRANSFORMATION` (73) rows at all. What it DOES have is
//! two `CBTS_GLIDER` (55) rows, which is why `blocks.replay.gliding` appears
//! in `v1-keyset.golden.txt` and the other three do not.
//!
//! So this file does for `transformations`/`captures`/`decorations` what
//! `axilog-core`'s own hand-built wire tests do for the decode: pin the
//! SHAPE against a synthetic document. It is the same gap class already
//! documented for `encounter.tick_rate` and the sc=74/75 WvW fields -- and
//! the same reason those got hand-built tests instead of golden diffs.
//!
//! What this file is responsible for is the SCHEMA layer only: the entity
//! join, the omit-when-empty rules, and the JSON encoding of the two tagged
//! shape enums. The semantics of the decode itself (interval closing,
//! progress splitting, colour selection) are `axilog-core`'s and are tested
//! there.

use axilog_core::analysis::agent_states::{AgentStates, GliderInterval, TransformationInterval};
use axilog_core::analysis::decorations::{Decoration, DecorationKind, DecorationShape};
use axilog_core::analysis::gadget_capture::{
    CaptureShape, GadgetCapture, Owner, OwnerState, ProgressState,
};
use axilog_core::analysis::replay_extras::ReplayExtras;
use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
use axilog_core::model::{Encounter, Player};

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

/// A one-player document with the supplied extras attached, serialized.
fn document(extras: &ReplayExtras) -> serde_json::Value {
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
        boon_generation: Default::default(),
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
        &axilog_schema::v1::Passes { replay_extras: Some(extras), ..Default::default() },
    );
    serde_json::to_value(&v1).expect("serializable")
}

/// The eye-candy families ride NEITHER replay gate, so supplying them alone
/// -- with no activity pass and no position tracks -- must still produce the
/// block. Before the gate was widened, `blocks.replay` was built only when
/// `activity` or `legacy.replay` was present, which would have silently
/// dropped every one of these rows on a caller that supplied only extras.
#[test]
fn the_replay_block_exists_when_only_the_eye_candy_families_are_supplied() {
    let extras = ReplayExtras {
        agent_states: AgentStates {
            gliding: vec![GliderInterval { agent_addr: 1, start_ms: 10, end_ms: Some(20) }],
            transformations: vec![],
        },
        ..Default::default()
    };
    let doc = document(&extras);
    assert!(doc["blocks"]["replay"].is_object(), "the block must exist");
    assert_eq!(doc["coverage"]["replay"], "present");
    assert!(doc["blocks"]["replay"].get("by_entity").is_some());
    assert!(doc["blocks"]["replay"].get("tracks").is_none(), "no position gate was on");
}

/// Every one of the four families is omitted rather than emitted empty, so a
/// consumer never has to distinguish `[]` from absent for data that is
/// simply not in this log.
#[test]
fn empty_families_are_omitted_not_emitted_as_empty_arrays() {
    let doc = document(&ReplayExtras::default());
    // With nothing at all supplied the block is still built (the pass ran),
    // but reports itself empty rather than pretending to carry rows.
    let replay = &doc["blocks"]["replay"];
    for key in ["gliding", "transformations", "captures", "decorations"] {
        assert!(replay.get(key).is_none(), "{key} must be omitted, not `[]`");
    }
    assert_eq!(doc["coverage"]["replay"], "empty");
}

/// The entity join is best-effort and per-row: a roster agent resolves, a
/// non-roster agent keeps `agent_addr` and omits `entity_id`. Dropping the
/// latter is the failure this shape exists to prevent -- `CBTS_GLIDER` is
/// not restricted to the squad.
#[test]
fn rows_survive_whether_or_not_their_agent_is_a_tracked_entity() {
    let extras = ReplayExtras {
        agent_states: AgentStates {
            gliding: vec![
                GliderInterval { agent_addr: 1, start_ms: 10, end_ms: Some(20) },
                // Never a roster player: e.g. a passing enemy that never
                // took a hostile hit.
                GliderInterval { agent_addr: 99, start_ms: 30, end_ms: None },
            ],
            transformations: vec![TransformationInterval {
                agent_addr: 99,
                transformation_id: 4242,
                guid: Some("abababababababababababababababab".into()),
                start_ms: 40,
                end_ms: None,
            }],
        },
        ..Default::default()
    };
    let doc = document(&extras);
    let gliding = doc["blocks"]["replay"]["gliding"].as_array().expect("gliding array");
    assert_eq!(gliding.len(), 2, "both rows survive, not just the resolvable one");

    assert_eq!(gliding[0]["entity_id"], 0, "agent 1 is the sole roster entity");
    assert_eq!(gliding[0]["agent_addr"], 1);
    assert_eq!(gliding[0]["end_ms"], 20);

    assert!(gliding[1].get("entity_id").is_none(), "agent 99 is not a tracked entity");
    assert_eq!(gliding[1]["agent_addr"], 99);
    assert!(
        gliding[1].get("end_ms").is_none(),
        "a still-open window omits end_ms rather than fabricating one at log end"
    );

    let t = &doc["blocks"]["replay"]["transformations"][0];
    assert!(t.get("entity_id").is_none());
    assert_eq!(t["transformation_id"], 4242);
    assert_eq!(t["guid"], "abababababababababababababababab");
    assert!(t.get("end_ms").is_none());
}

/// An unresolved transformation GUID must be OMITTED, never `null` and never
/// a placeholder: the session-local id alone is not portable across logs, so
/// a consumer has to be able to tell "we could not resolve this" apart from
/// "here is its identity".
#[test]
fn an_unresolved_transformation_omits_its_guid() {
    let extras = ReplayExtras {
        agent_states: AgentStates {
            gliding: vec![],
            transformations: vec![TransformationInterval {
                agent_addr: 1,
                transformation_id: 7,
                guid: None,
                start_ms: 0,
                end_ms: Some(5),
            }],
        },
        ..Default::default()
    };
    let doc = document(&extras);
    let t = &doc["blocks"]["replay"]["transformations"][0];
    assert!(t.get("guid").is_none(), "must be absent, not null");
    assert_eq!(t["transformation_id"], 7);
}

/// The capture decode's own vocabulary has to survive onto the wire: the
/// wrbg owner names, the circle-vs-polygon shape tag, and `decaying` carried
/// explicitly rather than left for the reader to infer from `by == "white"`.
#[test]
fn a_capture_serializes_its_owner_vocabulary_and_shape_tag() {
    let extras = ReplayExtras {
        captures: vec![GadgetCapture {
            agent_addr: 500,
            start_ms: 0,
            end_ms: Some(900),
            original_owner: Owner::Blue,
            shape: Some(CaptureShape::Circle { radius: 300.0 }),
            owner_states: vec![OwnerState { time_ms: 100, from: Owner::Blue, by: Owner::Red }],
            progress_states: vec![
                ProgressState { from: Owner::Blue, by: Owner::Red, progress: vec![(100, 25.5)] },
                ProgressState { from: Owner::Red, by: Owner::White, progress: vec![(400, 90.0)] },
            ],
        }],
        ..Default::default()
    };
    let doc = document(&extras);
    let c = &doc["blocks"]["replay"]["captures"][0];
    assert_eq!(c["agent_addr"], 500);
    assert!(c.get("entity_id").is_none(), "a capture gadget is not a tracked entity");
    assert_eq!(c["original_owner"], "blue");
    assert_eq!(c["shape"]["kind"], "circle");
    assert_eq!(c["shape"]["radius"], 300.0);
    assert!(c["shape"].get("points").is_none(), "the circle variant carries no vertex list");

    assert_eq!(c["owner_states"][0]["from"], "blue");
    assert_eq!(c["owner_states"][0]["by"], "red");
    assert_eq!(c["owner_states"][0]["time_ms"], 100);

    assert_eq!(c["progress_states"][0]["decaying"], false, "red is capping");
    assert_eq!(c["progress_states"][0]["progress"][0][1], 25.5);
    assert_eq!(c["progress_states"][1]["decaying"], true, "nobody is capping");
}

/// A capture with no geometry keeps `shape` OMITTED. This is GW2EI's
/// `IsValid` being false, and it is the state that makes a capture produce
/// no decoration at all -- so it must stay distinguishable on the wire
/// rather than defaulting to a zero-radius circle.
#[test]
fn a_capture_with_no_geometry_omits_its_shape() {
    let extras = ReplayExtras {
        captures: vec![GadgetCapture {
            agent_addr: 500,
            start_ms: 0,
            end_ms: None,
            original_owner: Owner::White,
            shape: None,
            owner_states: vec![],
            progress_states: vec![],
        }],
        ..Default::default()
    };
    let doc = document(&extras);
    let c = &doc["blocks"]["replay"]["captures"][0];
    assert!(c.get("shape").is_none());
    assert!(c.get("end_ms").is_none());
    assert_eq!(c["owner_states"].as_array().expect("array").len(), 0);
}

/// An owner index arcdps adds later must stay visible as `unknown_<n>`
/// rather than folding into `white` -- otherwise a new capture faction reads
/// as "unowned" to every consumer.
#[test]
fn an_unknown_owner_index_survives_onto_the_wire() {
    let extras = ReplayExtras {
        captures: vec![GadgetCapture {
            agent_addr: 500,
            start_ms: 0,
            end_ms: Some(10),
            original_owner: Owner::Unknown(7),
            shape: Some(CaptureShape::Polygon { points: vec![(1.0, 2.0), (3.0, 4.0)] }),
            owner_states: vec![],
            progress_states: vec![],
        }],
        ..Default::default()
    };
    let doc = document(&extras);
    let c = &doc["blocks"]["replay"]["captures"][0];
    assert_eq!(c["original_owner"], "unknown_7");
    assert_eq!(c["shape"]["kind"], "polygon");
    assert_eq!(c["shape"]["points"], serde_json::json!([[1.0, 2.0], [3.0, 4.0]]));
    assert!(c["shape"].get("radius").is_none());
}

/// All three decoration shapes encode as an internally tagged object, and
/// the progress bar carries both colour slots. The negative `start_ms` is
/// the real one -- a progress split landing on log-relative 0 backfills its
/// outgoing run at `-1` -- so the field must be signed on the wire.
#[test]
fn decorations_encode_all_three_shapes_and_tolerate_a_negative_time() {
    let extras = ReplayExtras {
        decorations: vec![
            Decoration {
                kind: DecorationKind::CaptureOutline,
                start_ms: 0,
                end_ms: 100,
                anchor: (10.0, 20.0),
                color: "rgba(255,0,0,0.3)".into(),
                secondary_color: None,
                shape: DecorationShape::Circle { radius: 300.0, filled: false },
            },
            Decoration {
                kind: DecorationKind::CaptureOutline,
                start_ms: 100,
                end_ms: 200,
                anchor: (10.0, 20.0),
                color: "rgba(0,140,255,0.3)".into(),
                secondary_color: None,
                shape: DecorationShape::Polygon {
                    points: vec![(1.0, 2.0), (3.0, 4.0)],
                    filled: false,
                },
            },
            Decoration {
                kind: DecorationKind::CaptureProgress,
                start_ms: -1,
                end_ms: 200,
                anchor: (10.0, 20.0),
                color: "rgba(0,255,0,0.3)".into(),
                secondary_color: Some("rgba(255,255,255,0.6)".into()),
                shape: DecorationShape::ProgressBar {
                    width: 200,
                    height: 30,
                    progress: vec![(-1, 100.0), (50, 75.0)],
                },
            },
        ],
        ..Default::default()
    };
    let doc = document(&extras);
    let d = doc["blocks"]["replay"]["decorations"].as_array().expect("decorations array");
    assert_eq!(d.len(), 3);

    assert_eq!(d[0]["kind"], "capture_outline");
    assert_eq!(d[0]["anchor"], serde_json::json!([10.0, 20.0]));
    assert_eq!(d[0]["shape"]["kind"], "circle");
    assert_eq!(d[0]["shape"]["filled"], false);
    assert!(d[0].get("secondary_color").is_none(), "an outline has one colour");

    assert_eq!(d[1]["shape"]["kind"], "polygon");
    assert_eq!(d[1]["shape"]["points"], serde_json::json!([[1.0, 2.0], [3.0, 4.0]]));

    assert_eq!(d[2]["kind"], "capture_progress");
    assert_eq!(d[2]["start_ms"], -1, "the wire field must be signed");
    assert_eq!(d[2]["shape"]["kind"], "progress_bar");
    assert_eq!(d[2]["shape"]["width"], 200);
    assert_eq!(d[2]["shape"]["progress"], serde_json::json!([[-1, 100.0], [50, 75.0]]));
    assert_eq!(d[2]["secondary_color"], "rgba(255,255,255,0.6)");
}
