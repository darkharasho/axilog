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
        // Side-channel absorption Task 6: deliberately WITHOUT `minions`
        // and `health_percents`, unlike the per-block report below.
        //
        // This assertion is a ratio against the legacy document, so it is
        // only meaningful while both documents describe the same
        // measurements. Those two passes have no legacy counterpart at all
        // -- they reached ei-json through the side channel and the legacy
        // `Report` has nowhere to put them -- so including them here
        // compares a document carrying them against one that cannot, and
        // the ratio moves 0.800 -> 0.885 without a single byte of encoding
        // getting worse. That would be a bound eroded by absorbing data,
        // which is exactly the growth this program exists to cause, and
        // widening the bound to absorb it would retire the test one task
        // at a time.
        //
        // Their cost is not going unmeasured: `per_block_sizes_are_
        // reported_for_the_benchmarks_doc` below builds the FULL document
        // and prints both (`minions` 63,231 bytes; `series` grows ~75,063
        // for `health_percents`, on the committed fixture).
        &axilog_schema::v1::Passes { damage_mods: Some(&damage_mods), ..Default::default() },
    );
    (
        serde_json::to_string(&legacy).expect("legacy serializes"),
        serde_json::to_string(&v1).expect("v1 serializes"),
    )
}

#[test]
fn the_one_point_oh_document_is_not_larger_than_the_legacy_one() {
    let (legacy, v1) = build();
    // Catalog dedup and RLE DO make 1.0 smaller on the same content:
    // measured ratio on the committed fixture is ~0.80 (see
    // docs/BENCHMARKS.md).
    //
    // That figure was ~0.55 until the final whole-branch review, which is
    // the cautionary tale this comment exists to carry: the earlier number
    // was measured while five legacy field families had no 1.0 destination
    // at all and were being silently dropped -- including the bulkiest one
    // in the schema, the per-(player, target, skill) breakdown. A size
    // comparison against an incomplete document flatters itself, and no
    // test here could see the difference, because every test asserted that
    // what was PRESENT matched and none asserted that nothing was ABSENT
    // (see `v1_equivalence.rs`'s completeness checklist, added with the
    // fix). Closing the gaps moved the ratio 0.552 -> 0.800.
    //
    // The bound is 0.85, down from the pre-fix 0.70 -- which the real,
    // complete document no longer passes. 0.85 kept ~6% relative headroom
    // above the then-measured 0.800 for per-fixture drift while still
    // asserting a genuine win: a regression that erodes even a quarter of
    // the remaining 20% reduction trips it, and dedup or RLE breaking
    // outright would push the ratio well past 1.0. If additive 1.x growth
    // eats it, re-measure and re-justify rather than widening the bound
    // reflexively; a bound loose enough to always pass is what the
    // pre-measurement 1.20 draft already proved worthless.
    //
    // MEASURED 0.837 as of side-channel absorption Task 6 -- the headroom
    // is down to ~1.5%, and this is a WARNING to whoever trips it next.
    // The drift is not an encoding regression. Tasks 4 and 5 absorbed two
    // quantities (`breakbar_damage_dealt`, enemy outgoing damage) that the
    // legacy document carries as `#[serde(skip)]` fields, so 1.0 now
    // serializes real measurements the baseline structurally omits.
    // Tasks 7-12 absorb six more passes with no legacy counterpart at all.
    // The passes that enter through `Passes` can be (and are) excluded
    // above to keep the comparison honest; the ones that land on always-on
    // blocks cannot. When this finally trips: verify the growth is
    // absorbed-data growth rather than encoding regression (the per-block
    // report below is how), then consider whether a ratio against a
    // document that no longer describes the same thing is still the right
    // test -- rather than nudging 0.85 upward one task at a time.
    //
    // TRIPPED by Phase B's native-format-gap-closure Task 4: widening
    // `PerTargetStatsOut`/`PerTargetDetail` from 8/7 to 24/23 fields moved
    // the ratio 0.837 -> 0.867 on the committed fixture (legacy
    // 1,646,041 -> 1,976,124 bytes; 1.0 1,384,150 -> 1,714,263 bytes).
    // Confirmed absorbed-data growth, not an encoding regression: the two
    // structs mirror each other field-for-field (same 16 field names, same
    // values), so the added bytes are near-identical in both documents --
    // legacy grew by 330,083 bytes, 1.0 by 330,113, a 30-byte difference on
    // a third of a megabyte. Growing two documents by an equal ABSOLUTE
    // amount always pushes their ratio toward 1.0 when the smaller one is
    // already below it (that is arithmetic, not dedup breaking) -- this
    // block gets none of catalog dedup's benefit because per-target detail
    // rows are high-entropy per-(player, target) counters, not repeated
    // catalog entries. Bound moved 0.85 -> 0.88 to restore ~1.4 points of
    // headroom above the new 0.867 measurement, per this comment's own
    // "re-measure and re-justify" instruction rather than a reflexive nudge.
    assert!(
        v1.len() <= legacy.len() * 88 / 100,
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
    let minion_rollups = axilog_core::analysis::minions::build(&raw, &enc);
    let health_percents = axilog_core::analysis::health::ei_health_percents(&raw, &enc);
    let (enemy_dist, enemy_series) = {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (
            axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &rep),
            axilog_core::analysis::timeseries::build_enemy_series(
                &enc,
                &raw,
                &axilog_core::analysis::damage::InstidRegistry::build(&raw),
                &enemies,
                &rep,
            ),
        )
    };
    // Task 9: the outcome columns on both player-side distributions.
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    // Task 10: the healing detail feeds two families under two different
    // flags; with every gate on, both are set from the one pass.
    let healing_detail = axilog_core::analysis::healing_detail::build(&raw, &enc);
    // Task 11: ungated, like every real caller -- `blocks.replay.by_entity`
    // is the always-on half of that block.
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
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
    // Task 12: this per-block report is the all-gates one, so both
    // timeline passes run here (the ratio test above deliberately
    // excludes every absorbed pass -- see its comment).
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let target_conditions = axilog_core::analysis::target_conditions::build(&raw, &enc);
    let self_effects = axilog_core::analysis::self_effects::build(&raw, &enc);
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        Some("wvw-small.anon.zevtc"),
        &axilog_schema::v1::Passes {
            damage_mods: Some(&damage_mods),
            minions: Some(&minion_rollups),
            health_percents: Some(&health_percents),
            enemy_dist: Some(&enemy_dist),
            enemy_series: Some(&enemy_series),
            dist_outcomes: Some(&dist_outcomes),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: Some(&boon_states),
            target_conditions: Some(&target_conditions),
            self_effects: Some(&self_effects),
        },
    );
    let v = serde_json::to_value(&v1).expect("serializable");

    // Reported, not asserted -- this is the input to `docs/BENCHMARKS.md`.
    for (name, value) in v["blocks"].as_object().expect("blocks").iter() {
        let size = serde_json::to_string(value).expect("stringify").len();
        println!("BLOCK {name} {size}");
    }
    let cats = serde_json::to_string(&v["catalogs"]).expect("stringify").len();
    let ents = serde_json::to_string(&v["entities"]).expect("stringify").len();
    println!("BLOCK catalogs {cats}");
    println!("BLOCK entities {ents}");
}
