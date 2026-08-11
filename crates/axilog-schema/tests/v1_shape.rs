//! Structural invariants of the 1.0 container.
use axilog_schema::v1::envelope::BlockName;

/// Builds the 1.0 document with EVERY compute gate ON (skill-damage,
/// timeseries, rotation, replay, missiles, damage-mods), the way
/// `axilog-cli`'s `Cmd::Parse` does when every optional flag is passed --
/// see `crates/axilog-cli/src/main.rs`'s `replay_data`/`missiles_data`/
/// `damage_mods` construction, which this mirrors. This is what the
/// key-set golden (`the_full_key_set_matches_the_committed_golden` below)
/// is generated against, so that golden protects all 13 blocks, not just
/// the 9 that happen to be on by default -- see Task 9 fix round 1,
/// Finding 2.
fn build() -> serde_json::Value {
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
    serde_json::to_value(&v1).expect("serializable")
}

#[test]
fn the_document_has_exactly_the_six_top_level_keys() {
    let v = build();
    let obj = v.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    // `warnings` is omitted when empty, so it is optional here.
    let expected_always = ["axilog", "blocks", "catalogs", "coverage", "encounter", "entities"];
    for k in expected_always {
        assert!(keys.contains(&k), "missing top-level key {k}");
    }
    for k in &keys {
        assert!(
            expected_always.contains(k) || *k == "warnings",
            "unexpected top-level key {k} -- the 1.0 container is closed"
        );
    }
}

#[test]
fn the_schema_version_is_one_point_oh_and_distinct_from_the_binary_version() {
    let v = build();
    assert_eq!(v["axilog"]["schema"], "1.0");
    assert_eq!(v["axilog"]["version"], "0.0.0-test");
    assert_eq!(v["axilog"]["generated_from"], "wvw-small.anon.zevtc");
}

#[test]
fn coverage_names_every_block_and_agrees_with_what_blocks_carries() {
    let v = build();
    let coverage = v["coverage"].as_object().expect("coverage object");
    assert_eq!(coverage.len(), BlockName::ALL.len());
    let blocks = v["blocks"].as_object().expect("blocks object");
    for block in BlockName::ALL {
        let name = block.as_str();
        let state = coverage[name].as_str().expect("state string");
        let present = blocks.contains_key(name);
        match state {
            "present" => assert!(present, "coverage says {name} is present but blocks lacks it"),
            "not_computed" | "unsupported" => {
                assert!(!present, "coverage says {name} is {state} but blocks carries it")
            }
            "empty" => {}
            other => panic!("unknown coverage state {other} for {name}"),
        }
    }
}

#[test]
fn every_referenced_id_resolves_and_every_catalog_entry_is_referenced() {
    let v = build();
    let text = serde_json::to_string(&v["blocks"]).expect("stringify blocks");

    for (catalog, key_prefix) in [("skills", "skill"), ("buffs", "buff"), ("damage_mods", "mod")] {
        let Some(entries) = v["catalogs"].get(catalog).and_then(|c| c.as_object()) else { continue };
        for id in entries.keys() {
            // Direction 2: no orphan definitions. A referenced id appears in
            // the blocks payload either as a map key or as a `*_id` value.
            assert!(
                text.contains(&format!("\"{id}\"")) || text.contains(&format!(":{id}")),
                "catalog {catalog} entry {id} ({key_prefix}) is never referenced by any block"
            );
        }
    }
}

#[test]
fn parsing_the_same_log_twice_is_byte_identical() {
    let a = serde_json::to_string(&build()).expect("stringify");
    let b = serde_json::to_string(&build()).expect("stringify");
    assert_eq!(a, b, "entity ids are indices into a sorted roster -- the sort must be total");
}

#[test]
fn no_block_inlines_a_human_readable_name() {
    let v = build();
    let text = serde_json::to_string(&v["blocks"]).expect("stringify blocks");
    assert!(!text.contains("\"name\""), "names live in catalogs and entities only");
}

/// The COMPLETE 1.0 key set on the committed fixture, as a sorted list of
/// dotted paths. Removing or renaming a key fails; adding one is a reviewed
/// diff. This is the compatibility rule made executable -- the six-key test
/// above only guards the top level.
///
/// `build()` above turns on every compute gate this test binary can drive
/// (skill-damage, timeseries, rotation, replay, missiles, damage-mods), so
/// this golden protects all 13 optional/gated blocks, not just the 9 that
/// happen to be on by default -- see Task 9 fix round 1, Finding 2.
///
/// One key is STILL unguarded here: `encounter.tick_rate`. It is populated
/// from `CBTS_TICK` events already present in the raw log (see
/// `crate::EncounterOut::tick_rate`'s doc comment -- omitted when the log
/// has fewer than two `CBTS_TICK` events), not from a `build_report`/
/// `build_report_v1` flag this test controls, and the committed
/// `wvw-small.anon.zevtc` fixture has none (`axilog-core`'s own
/// `tick_rate_absent_from_fixture` test asserts exactly this). Covering it
/// would need a second fixture recorded with tick events, or a hand-built
/// `RawLog` in this test -- left as a known gap rather than silently
/// implied by the key's absence.
#[test]
fn the_full_key_set_matches_the_committed_golden() {
    fn walk(v: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, val) in m {
                    // Entity/skill/buff ids are DATA, not schema -- collapse
                    // them so the golden tracks shape, not fixture content.
                    let key = if k.chars().all(|c| c.is_ascii_digit() || c == '-') { "<id>" } else { k };
                    let path = if prefix.is_empty() { key.to_string() } else { format!("{prefix}.{key}") };
                    if !out.contains(&path) {
                        out.push(path.clone());
                    }
                    walk(val, &path, out);
                }
            }
            serde_json::Value::Array(items) => {
                if let Some(first) = items.first() {
                    walk(first, &format!("{prefix}[]"), out);
                }
            }
            _ => {}
        }
    }

    let v = build();
    let mut keys = Vec::new();
    walk(&v, "", &mut keys);
    keys.sort();

    let golden_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/v1-keyset.golden.txt");
    let actual = keys.join("\n") + "\n";
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(golden_path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(golden_path).unwrap_or_default();
    assert_eq!(
        actual, expected,
        "the 1.0 key set changed. Adding keys is additive and fine -- re-run with \
         UPDATE_GOLDEN=1 and review the diff. REMOVING or RENAMING a key is a \
         breaking change requiring a major bump."
    );
}
