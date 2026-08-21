//! The EI adapter's `buffUptimes` is the CONCATENATION of two native
//! blocks.
//!
//! Elite Insights keeps boons, conditions and the long tail of
//! sigils/relics/food in one `buffUptimes` array per player. This project
//! splits that population across `blocks.boons` and `blocks.squad_buffs`
//! (the condition family lands on `targets[].buffs` and
//! `blocks.self_effects` instead), so the adapter has to rebuild the single
//! array -- and EI's array has exactly one entry per id.
//!
//! Until `blocks.squad_buffs` existed the adapter emitted the 12 boons
//! alone, which is why axibridge's Special Buffs and Sigil/Relic Uptime
//! sections rendered empty: both read exactly the ids this file asserts are
//! now present.

use axilog_core::analysis::buffs::BOON_IDS;
use serde_json::Value;
use std::collections::BTreeSet;

/// The committed fixture rendered as EI JSON, with the squad-buffs pass
/// wired exactly as the production callers wire it (always-on).
fn ei_doc() -> Value {
    let bytes =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
            .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, true, true, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes {
            activity: Some(&activity),
            squad_buffs: Some(&squad_buffs),
            ..Default::default()
        },
    );
    axilog_ei::to_ei_json(&v1, axilog_ei::EiReplayInput::default())
}

fn players(doc: &Value) -> &Vec<Value> {
    doc["players"].as_array().expect("players")
}

fn uptime_ids(player: &Value) -> Vec<u32> {
    player["buffUptimes"]
        .as_array()
        .map(|rows| rows.iter().map(|r| r["id"].as_u64().expect("id") as u32).collect())
        .unwrap_or_default()
}

/// The symptom, stated directly: some squad player must carry a buff that
/// is neither a boon nor a condition.
#[test]
fn buff_uptimes_carries_non_boon_ids() {
    let doc = ei_doc();
    let boons: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let extra: BTreeSet<u32> = players(&doc)
        .iter()
        .flat_map(uptime_ids)
        .filter(|id| !boons.contains(id))
        .collect();
    assert!(
        !extra.is_empty(),
        "buffUptimes carries only the 12 boons -- the sigil/relic/food tail is missing, \
         which is exactly the empty-section bug this block was added to fix"
    );
    for id in &extra {
        assert!(
            axilog_core::analysis::squad_buffs::is_squad_buff(*id),
            "buff {id} is in buffUptimes but belongs to neither family"
        );
    }
}

/// EI's array has one entry per id. A duplicate would come from the two
/// source blocks overlapping, and would make any consumer that folds the
/// array into a map silently keep whichever it saw last.
#[test]
fn no_player_lists_an_id_twice() {
    let doc = ei_doc();
    for p in players(&doc) {
        let ids = uptime_ids(p);
        let unique: BTreeSet<u32> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "a player's buffUptimes repeats an id: {ids:?}"
        );
    }
}

/// The boon rows keep their `BOON_IDS` order and stay at the FRONT: the
/// existing EI goldens pin that order, and appending must not disturb it.
#[test]
fn the_boon_rows_remain_a_leading_prefix_in_boon_ids_order() {
    let doc = ei_doc();
    let order: Vec<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    for p in players(&doc) {
        let ids = uptime_ids(p);
        let boon_prefix: Vec<u32> =
            ids.iter().copied().take_while(|id| order.contains(id)).collect();
        let expected: Vec<u32> =
            order.iter().copied().filter(|id| boon_prefix.contains(id)).collect();
        assert_eq!(boon_prefix, expected, "boon rows lost their BOON_IDS order");
        assert!(
            !ids[boon_prefix.len()..].iter().any(|id| order.contains(id)),
            "a boon row appears after the squad-buff rows"
        );
    }
}

/// Every id in the array must resolve in `buffMap`. axibridge's
/// `resolveBuffMetaById` DROPS a row whose id misses, so an unresolvable
/// id is indistinguishable from the bug this change fixes.
#[test]
fn every_listed_id_resolves_in_the_buff_map() {
    let doc = ei_doc();
    let map = doc["buffMap"].as_object().expect("buffMap");
    for p in players(&doc) {
        for id in uptime_ids(p) {
            let key = format!("b{id}");
            let entry = map.get(&key).unwrap_or_else(|| panic!("buffMap lacks {key}"));
            let name = entry["name"].as_str().unwrap_or("");
            assert!(!name.is_empty(), "{key} resolves to an empty name");
        }
    }
}

/// An intensity buff reports its mean stack count in `uptime` and its
/// presence in `presence`; a duration buff reports percent uptime in
/// `uptime` and leaves `presence` at 0. The same two spellings
/// `blocks.boons` already round-trips, applied to the appended rows.
#[test]
fn the_two_buff_data_spellings_follow_the_stack_type() {
    let doc = ei_doc();
    let boons: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let mut intensity_seen = 0;
    let mut duration_seen = 0;
    for p in players(&doc) {
        for row in p["buffUptimes"].as_array().into_iter().flatten() {
            let id = row["id"].as_u64().expect("id") as u32;
            if boons.contains(&id) {
                continue;
            }
            let (is_intensity, _) = axilog_core::analysis::buffs::stacking(id);
            let presence = row["buffData"][0]["presence"].as_f64().expect("presence");
            if is_intensity {
                intensity_seen += 1;
            } else {
                duration_seen += 1;
                assert_eq!(presence, 0.0, "buff {id} is duration-stacking; presence must be 0");
            }
        }
    }
    assert!(
        intensity_seen > 0 && duration_seen > 0,
        "both branches must be exercised: {intensity_seen} intensity, {duration_seen} duration"
    );
}
