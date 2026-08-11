//! Bytes per block on the committed fixture.
//!
//! Reducing payload is part of this spec's point (catalog dedup + RLE);
//! unmeasured, it will regress. Both documents are built with EVERY
//! compute gate on (replay, missiles, skill-damage, timeseries, rotation,
//! damage-mods) -- see `v1_shape.rs`'s `build_with_encounter` -- so the
//! comparison is apples-to-apples rather than a full 1.0 document against
//! a partial legacy one.

fn build() -> (String, String) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay_data = axilog_core::analysis::replay::build_replay(
        &raw,
        &enc,
        axilog_core::analysis::replay::DEFAULT_POLL_MS,
    );
    let missiles_data = axilog_core::analysis::missiles::build_missiles(&raw, &enc);
    let damage_mods = axilog_core::analysis::damage_mods::evaluate_catalog_full(
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enc,
        false,
    );
    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        Some(&replay_data),
        Some(&missiles_data),
        true,
        true,
        true,
        Some(&damage_mods),
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        Some("wvw-small.anon.zevtc"),
        Some(&damage_mods),
    );
    (
        serde_json::to_string(&legacy).expect("legacy serializes"),
        serde_json::to_string(&v1).expect("v1 serializes"),
    )
}

#[test]
fn the_one_point_oh_document_is_not_larger_than_the_legacy_one() {
    let (legacy, v1) = build();
    // Catalog dedup and RLE DO make 1.0 smaller on the same content, despite
    // 1.0 carrying strictly more data than legacy (enemy stats that were
    // `#[serde(skip)]` in legacy, a `combat_participant` flag, per-skill
    // `crit_hits`/`flank_hits`): measured ratio on the committed fixture is
    // ~0.55 (see docs/BENCHMARKS.md). The bound below is set at 0.70,
    // leaving headroom above the measured ratio for normal per-fixture
    // drift while still catching a regression that erodes most of the win
    // (e.g. dedup or RLE silently breaking on a code path).
    assert!(
        v1.len() <= legacy.len() * 7 / 10,
        "1.0 is {} bytes vs legacy {} (ratio {:.3}) -- expected the 1.0 document to be meaningfully \
         smaller via catalog dedup + RLE; see docs/BENCHMARKS.md",
        v1.len(),
        legacy.len(),
        v1.len() as f64 / legacy.len() as f64
    );
    println!("SIZE legacy={} v1={} ratio={:.3}", legacy.len(), v1.len(), v1.len() as f64 / legacy.len() as f64);
}

#[test]
fn per_block_sizes_are_reported_for_the_benchmarks_doc() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay_data = axilog_core::analysis::replay::build_replay(
        &raw,
        &enc,
        axilog_core::analysis::replay::DEFAULT_POLL_MS,
    );
    let missiles_data = axilog_core::analysis::missiles::build_missiles(&raw, &enc);
    let damage_mods = axilog_core::analysis::damage_mods::evaluate_catalog_full(
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enc,
        false,
    );
    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        Some(&replay_data),
        Some(&missiles_data),
        true,
        true,
        true,
        Some(&damage_mods),
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        Some("wvw-small.anon.zevtc"),
        Some(&damage_mods),
    );
    let v = serde_json::to_value(&v1).expect("serializable");

    for (name, value) in v["blocks"].as_object().expect("blocks").iter() {
        let size = serde_json::to_string(value).expect("stringify").len();
        println!("BLOCK {name} {size}");
    }
    let cats = serde_json::to_string(&v["catalogs"]).expect("stringify").len();
    let ents = serde_json::to_string(&v["entities"]).expect("stringify").len();
    println!("BLOCK catalogs {cats}");
    println!("BLOCK entities {ents}");
}
