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
    build_with_encounter().0
}

/// Same as `build()`, but also hands back the resolved `Encounter` model so
/// callers can pull ground-truth names (`Player::account`, `Player::
/// character`, `Enemy::name`) straight from the model instead of guessing
/// at their shape in the serialized output.
fn build_with_encounter() -> (serde_json::Value, axilog_core::model::Encounter) {
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
    // CC-strip-timelines Task 4: the per-player 1s CC/strip lanes on
    // `blocks.series.by_entity`. Supplied unconditionally here, like every
    // other pass in this all-gates-on harness; production gates it on
    // `--timeseries` because it is NOT cheap (`build_from` derives an
    // `InstidRegistry`, a full pass over `raw.events`, and the pass itself
    // makes several more scans on top of that).
    let entity_series = axilog_core::analysis::entity_series::build_from(&enc, &raw, &metrics);
    // Task 12: the two name-keyed passes, now keyed by source ADDRESS at
    // the source and by source ENTITY ID once native reprojects them.
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let target_conditions = axilog_core::analysis::target_conditions::build(&raw, &enc);
    let self_effects = axilog_core::analysis::self_effects::build(&raw, &enc);
    let cc_taken_events = axilog_core::analysis::cc::taken_events_for(&enc, &raw);
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
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
        &axilog_schema::v1::Passes {
            damage_mods: Some(&damage_mods),
            minions: Some(&minion_rollups),
            health_percents: Some(&health_percents),
            enemy_dist: Some(&enemy_dist),
            enemy_series: Some(&enemy_series),
            dist_outcomes: Some(&dist_outcomes),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            entity_series: Some(&entity_series),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: Some(&boon_states),
            target_conditions: Some(&target_conditions),
            self_effects: Some(&self_effects),
            cc_taken_events: Some(&cc_taken_events),
            squad_buffs: Some(&squad_buffs),
        },
    );
    (serde_json::to_value(&v1).expect("serializable"), enc)
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
                    // `blocks.damage_mods.personal` is keyed by SPEC name
                    // ("Firebrand"), the one dynamic map in the container
                    // whose keys are not ids -- collapse it the same way,
                    // or the golden would pin whichever specs this fixture
                    // happens to field.
                    let key = if k.chars().all(|c| c.is_ascii_digit() || c == '-') {
                        "<id>"
                    } else if prefix == "blocks.damage_mods.personal" {
                        "<spec>"
                    } else {
                        k
                    };
                    let path = if prefix.is_empty() { key.to_string() } else { format!("{prefix}.{key}") };
                    if !out.contains(&path) {
                        out.push(path.clone());
                    }
                    walk(val, &path, out);
                }
            }
            serde_json::Value::Array(items) => {
                // EVERY element, not just the first. Optional fields are
                // omitted when absent, so sampling `items.first()` records
                // whichever subset element 0 happens to carry -- and the
                // golden then churns whenever the sort order changes, while
                // silently failing to guard fields no first element ever
                // has. Unioning across all elements makes the key set a
                // property of the SHAPE rather than of element 0.
                for item in items {
                    walk(item, &format!("{prefix}[]"), out);
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
    // Compare line ENDINGS out of the picture. `.gitattributes` pins the
    // golden to LF, but a checkout made before that rule landed (or with
    // a global `core.autocrlf=true`) still has CRLF on disk, and this
    // test would then fail on Windows over nothing but `\r`.
    let expected = std::fs::read_to_string(golden_path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
        "the 1.0 key set changed. Adding keys is additive and fine -- re-run with \
         UPDATE_GOLDEN=1 and review the diff. REMOVING or RENAMING a key is a \
         breaking change requiring a major bump."
    );
}

/// After scrubbing/anonymization, no *account-shaped* string (`Name.NNNN`)
/// appears ANYWHERE in the serialized 1.0 document except as an `Anon`
/// placeholder.
///
/// This is deliberately narrower than "no identity leaks": the scanner
/// below only recognizes the account shape, so it cannot see a leaked
/// CHARACTER name (anonymized characters are plain `Anon<N>`, with no
/// numeric suffix, and real character/NPC names have no reliable
/// shape at all -- exactly the kind of thing a `_note` field could carry
/// invisibly). For that, see
/// `no_name_from_the_encounter_appears_outside_entities` below, which
/// checks the actual design claim (names live ONLY in `entities[]`)
/// exactly, against the ground-truth name set, rather than by shape
/// guessing. This test still earns its keep alongside that one: it also
/// catches a name that is PRESENT but WRONG (e.g. a real account slipped in
/// where an `Anon` placeholder should be), which the entities-exclusion
/// test would not flag since the wrong account would still be inside
/// `entities[]`.
///
/// The committed fixture (`fixtures/wvw-small.anon.zevtc`) is already
/// anonymized: every account is of the form `Anon<N>.<4 digits>` once
/// `model::normalize_account` has stripped the raw arcdps colon. So the
/// assertion is that every account-shaped string appearing anywhere in the
/// serialized document starts with `Anon` -- anything else is an
/// unscrubbed identity that leaked through some field other than
/// `entities[]`.
#[test]
fn no_unscrubbed_identity_survives_in_the_v1_document() {
    let v = build();
    let text = serde_json::to_string(&v).expect("stringify");

    let account_like = regex_lite_account_matches(&text);
    eprintln!("account-shaped strings found: {}", account_like.len());
    for a in &account_like {
        assert!(
            a.starts_with("Anon"),
            "unscrubbed account-shaped string {a:?} in the v1 document"
        );
    }

    // A scanner that finds nothing still "passes" the loop above -- guard
    // against a scanner that isn't actually scanning. The fixture has 42
    // players (each contributing at least one `AnonN.NNNN` account
    // reference), so a healthy scan should find dozens of hits, not a
    // handful. 40 is a floor comfortably below the 43 the scan currently
    // finds, leaving room for minor future fixture/shape changes without
    // masking a scanner regression.
    assert!(
        account_like.len() >= 40,
        "the scan found only {} account-shaped strings -- expected at least 40 for a \
         42-player fixture; the scanner may not actually be scanning",
        account_like.len()
    );
}

/// The scanner (`regex_lite_account_matches`) is hand-rolled, so it needs
/// its own test proving it can find the thing it's looking for. A scanner
/// that can't detect a known-bad account makes
/// `no_unscrubbed_identity_survives_in_the_v1_document` above worthless --
/// it would pass on a document full of leaks just as happily as on a clean
/// one.
#[test]
fn the_account_scanner_detects_a_known_bad_account() {
    let text = r#"{"_note":"RealName.1234","other":1}"#;
    let matches = regex_lite_account_matches(text);
    assert!(
        matches.iter().any(|m| m == "RealName.1234"),
        "scanner failed to find a planted unscrubbed account in {matches:?}"
    );

    // The colon spelling must still be found. `normalize_account` strips it
    // from `Player.account`, but a leak that arrives some other way could
    // carry it, and a scanner that only sees the post-strip spelling would
    // wave that one through.
    let colon = r#"{"_note":":RealName.1234"}"#;
    assert_eq!(
        regex_lite_account_matches(colon),
        vec!["RealName.1234".to_string()],
        "scanner must still catch a raw arcdps-spelled account"
    );

    // Sanity check the negative case too: a clean `Anon` account should
    // also be found (it's account-shaped), just not flagged as a leak by
    // the caller.
    let clean = r#"{"account":"Anon12.3456"}"#;
    let clean_matches = regex_lite_account_matches(clean);
    assert_eq!(clean_matches, vec!["Anon12.3456".to_string()]);

    // JSON numbers are not accounts. A float and a semver-ish version both
    // have a dot, and the version even has a digit run after it -- neither
    // may enter the scan, or the caller's `starts_with("Anon")` assertion
    // would fail on a perfectly clean document.
    let numeric = r#"{"dps":12.3456,"version":"0.3.6","ratio":1.23456}"#;
    assert_eq!(
        regex_lite_account_matches(numeric),
        Vec::<String>::new(),
        "scanner must not mistake JSON numbers for accounts"
    );
}

/// Minimal account-shape scanner: `Name.1234`. Avoids adding a `regex`
/// dependency to the test suite for one pattern.
///
/// This used to anchor on the leading `:` arcdps writes into the agent name
/// buffer. `model::normalize_account` now strips that colon, so anchoring on
/// it found ZERO accounts -- and the scan went silently blind rather than
/// failing, which is precisely what the caller's non-vacuity floor exists to
/// catch. It caught it.
///
/// The shape is therefore anchored on the `.` + exactly-four-digits suffix
/// instead, walking backwards over the name. Two guards keep JSON numbers out:
/// the suffix must not be followed by another digit or a dot (so `1.23456` and
/// a version like `0.3.6` are rejected), and the name part must contain at
/// least one ASCII letter (so a bare float like `12.3456` is rejected while
/// every real GW2 account, which always has letters, is kept).
fn regex_lite_account_matches(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'.' {
            continue;
        }
        let Some(digits) = bytes.get(i + 1..i + 5) else {
            continue;
        };
        if !digits.iter().all(u8::is_ascii_digit) {
            continue;
        }
        if bytes
            .get(i + 5)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'.')
        {
            continue;
        }
        let mut start = i;
        while start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
            start -= 1;
        }
        let name = &text[start..i];
        if !name.bytes().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        out.push(text[start..i + 5].to_string());
    }
    out
}

/// The exact form of the format's "names live in exactly one place" claim:
/// every account, character, and enemy/NPC name from the resolved
/// `Encounter` may appear ONLY inside the top-level `entities[]` array, and
/// nowhere else in the serialized 1.0 document.
///
/// Unlike `no_unscrubbed_identity_survives_in_the_v1_document` above, this
/// needs no shape heuristic: the ground-truth name set comes straight from
/// the model (`Player::account`, `Player::character`, enemy-PLAYER
/// `Enemy::name`), so it also catches a leaked CHARACTER name, which the
/// account-shape scanner cannot see at all (anonymized characters are plain
/// `Anon<N>`, with no digit suffix, and real character names
/// have no reliable shape to scan for).
#[test]
fn no_name_from_the_encounter_appears_outside_entities() {
    let (v, enc) = build_with_encounter();

    // Ground-truth name set, straight from the model -- PERSONAL identity
    // only.
    //
    // Skip empty/whitespace-only names (some enemies legitimately have
    // none). Skip names shorter than `MIN_NAME_LEN`: a one- or two-character
    // name is common enough as an ordinary JSON substring (hex digits, ids,
    // short enum tags like "Up"/"NA") that scanning for it would produce
    // false-positive "leaks" that are really just coincidental text, making
    // the test unreliable rather than precise. Every name in this fixture's
    // roster is well above that floor (accounts are `AnonN.NNNN`-shaped and
    // characters/enemy players carry real multi-character names), so the
    // floor costs no real coverage here.
    //
    // `Enemy::name` is included ONLY when `Enemy::is_player` is true. A
    // non-player `Enemy::name` (NPC/gadget/summon, e.g. a Ranger's "Frost
    // Spirit" or a Warrior's "Banner of Tactics") is not personal data --
    // it's a generic, profession-wide label that legitimately also appears
    // as the display name of the skill that summons it, in
    // `catalogs.skills[<id>].name`. Including those in the ground-truth set
    // produced false "leaks" in an earlier version of this test (fix round
    // 1): the collision was the ground truth being too broad, not a real
    // identity leak, and not a scanner bug either.
    const MIN_NAME_LEN: usize = 3;
    let mut names: Vec<String> = Vec::new();
    for p in &enc.players {
        for n in [&p.account, &p.character] {
            let n = n.trim();
            if n.len() >= MIN_NAME_LEN {
                names.push(n.to_string());
            }
        }
    }
    for e in &enc.enemies {
        if !e.is_player {
            continue;
        }
        let n = e.name.trim();
        if n.len() >= MIN_NAME_LEN {
            names.push(n.to_string());
        }
    }
    names.sort();
    names.dedup();
    eprintln!("ground-truth name set size: {}", names.len());
    assert!(
        names.len() >= 40,
        "expected dozens of names from a 42-player fixture, found only {} -- the ground-truth \
         collection may be broken",
        names.len()
    );

    // Remove the one place names are permitted, then scan what remains.
    let mut obj = v.as_object().expect("top-level object").clone();
    obj.remove("entities").expect("v1 document has an entities key");
    let rest = serde_json::Value::Object(obj);
    let text = serde_json::to_string(&rest).expect("stringify document minus entities");

    let leaked: Vec<&String> = names.iter().filter(|n| text.contains(n.as_str())).collect();
    assert!(
        leaked.is_empty(),
        "{} name(s) from the encounter leaked outside entities[]: {:?}",
        leaked.len(),
        leaked
    );
}


/// No character name is used as a MAP KEY anywhere under `blocks`.
///
/// The enforcement for Task 12's redesign. Two passes
/// (`buffs::states`, `target_conditions`) keyed their per-source timelines
/// by the applier's character NAME; native re-keys both by source entity id
/// (`blocks.boons.by_entity.<id>.<id>.per_source.by_source` and
/// `blocks.conditions...`), and this is what holds that line.
///
/// It overlaps `no_name_from_the_encounter_appears_outside_entities` above,
/// which scans the serialized text and so would also see a name key -- but
/// only as an anonymous substring match. This one reports the exact JSON
/// PATH, which is the difference between "a name leaked somewhere" and
/// "`blocks.boons.by_entity.7.740.per_source.by_source.Anon12`". It is also
/// immune to the `MIN_NAME_LEN` floor that test needs: a two-character
/// character name is a plausible key and an implausible substring.
#[test]
fn no_block_key_is_a_character_name() {
    let (v, enc) = build_with_encounter();

    // Character names only -- accounts are not, and never were, a key
    // candidate for these maps. Empty names are skipped: `""` cannot be
    // distinguished from a legitimately absent name, and is not a key any
    // pass would produce.
    let names: Vec<String> = enc
        .players
        .iter()
        .map(|p| p.character.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    assert!(
        !names.is_empty(),
        "fixture must have named players for this test to mean anything"
    );

    fn walk(v: &serde_json::Value, names: &[String], path: &str, hits: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, child) in m {
                    if names.iter().any(|n| n == k) {
                        hits.push(format!("{path}/{k}"));
                    }
                    walk(child, names, &format!("{path}/{k}"), hits);
                }
            }
            serde_json::Value::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    walk(child, names, &format!("{path}/{i}"), hits);
                }
            }
            _ => {}
        }
    }

    let mut hits = Vec::new();
    walk(&v["blocks"], &names, "blocks", &mut hits);
    assert!(hits.is_empty(), "character name(s) used as block keys: {hits:?}");
}

/// `encounter.map_id` and `blocks.replay.tracks.arena` are what let a
/// consumer put this log on a map without EI-shaped input. On the committed
/// fixture -- Green Alpine Borderlands, id 95 -- both must be there and must
/// carry the table's own values.
#[test]
fn the_map_id_and_arena_reach_the_document() {
    let doc = build();
    assert_eq!(doc["encounter"]["map"], "Green Alpine Borderlands");
    assert_eq!(doc["encounter"]["map_id"], 95);

    let arena = &doc["blocks"]["replay"]["tracks"]["arena"];
    assert_eq!(arena["image_width"], 697);
    assert_eq!(arena["image_height"], 1000);
    assert_eq!(arena["image_url"], "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-nVu2ivF.png");
    assert_eq!(arena["world_min_x"], -30720.0);
    assert_eq!(arena["world_min_y"], -43008.0);
    assert_eq!(arena["world_max_x"], 30720.0);
    assert_eq!(arena["world_max_y"], 43008.0);

    // The arena is a FIXED frame -- unlike `bounds`, which is the union of
    // the observed positions. Every sample must fall inside it, or the frame
    // is the wrong one for this map and every replay drawn from it would be
    // clipped.
    let tracks = doc["blocks"]["replay"]["tracks"]["by_entity"]
        .as_object()
        .expect("tracks under --replay");
    let mut checked = 0usize;
    for track in tracks.values() {
        for s in track["samples"].as_array().expect("samples") {
            let (x, y) = (s[1].as_f64().unwrap(), s[2].as_f64().unwrap());
            assert!((-30720.0..=30720.0).contains(&x), "x {x} outside the arena");
            assert!((-43008.0..=43008.0).contains(&y), "y {y} outside the arena");
            checked += 1;
        }
    }
    assert!(checked > 1000, "expected a real track set, checked {checked}");
}

/// `encounter.log_start_ms` is the `t0` that makes this document's two
/// session-time fields interpretable at all.
///
/// Before it existed, `markers[].time_ms` and
/// `entities[].commander.segments` were raw arcdps session times in a
/// container where everything else was log-relative, and the container
/// carried no origin to rebase them against -- so a consumer reading only
/// the JSON could not tell "the tag went up at second 0" from "at second
/// 33847". This asserts the round trip actually closes on the real
/// fixture: rebased, both fields land inside the fight.
#[test]
fn the_two_session_time_fields_rebase_into_the_fight_by_log_start_ms() {
    let doc = build();
    let enc = &doc["encounter"];

    let t0 = enc["log_start_ms"].as_i64().expect("log_start_ms is always present");
    let duration = enc["duration_ms"].as_i64().expect("duration_ms");
    // The fixture's own scale: a `t0` of ~9.4 hours of session uptime
    // against a 49-second fight. Nothing about the numbers themselves
    // reveals which base they are in, which is the whole problem.
    assert!(t0 > duration, "expected a session-time t0 far beyond the fight length");

    // Rebased marker times sit inside the fight. Half-open at the top:
    // the closing event can land on the last millisecond.
    let markers = enc["markers"].as_array().expect("markers");
    assert!(!markers.is_empty(), "fixture has marker assignments");
    for m in markers {
        let rebased = m["time_ms"].as_i64().expect("time_ms") - t0;
        assert!(
            (0..=duration).contains(&rebased),
            "marker at {rebased}ms is outside [0, {duration}]",
        );
    }

    // Same for commander segments. The lower bound is NOT asserted: a tag
    // held before the log's first event rebases negative, which is
    // ordinary and is exactly why this field is left for the consumer to
    // rebase rather than being clamped into `u64` here.
    let mut segments = 0usize;
    for e in doc["entities"].as_array().expect("entities") {
        let Some(commander) = e.get("commander") else { continue };
        for seg in commander["segments"].as_array().expect("segments") {
            let (on, off) = (seg[0].as_i64().unwrap() - t0, seg[1].as_i64().unwrap() - t0);
            assert!(on <= off, "segment [{on}, {off}] runs backwards");
            assert!(off <= duration, "segment ends at {off}ms, past the fight's {duration}ms");
            segments += 1;
        }
    }
    assert!(segments > 0, "fixture has a commander with resolved segments");
}
