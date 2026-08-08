//! Golden EI parity test (Task 16B).
//!
//! Source log: axibridge testdata/20260117-181030.zevtc (WvW skirmish,
//! Green Alpine Borderlands). The golden JSON at
//! `fixtures/wvw-small.ei.json` is the anonymized dps.report Elite Insights
//! (EI) output for that same log — account names have been replaced with
//! synthetic values, but the aggregate numbers (duration, player counts,
//! damage totals) are untouched, so this test verifies axilog's analysis
//! pipeline reproduces EI's ground truth within tolerance.
//!
//! Runs only when the local (PII, not committed) raw log is present at
//! `fixtures/local/wvw-small.zevtc`. When absent (e.g. in CI), this prints
//! a skip message and returns successfully rather than failing the build.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

const RELATIVE_TOLERANCE: f64 = 0.005; // 0.5%
const FRIENDLY_COUNT_TOLERANCE: i64 = 2; // ±2

/// True if `a` and `b` are within `RELATIVE_TOLERANCE` of each other,
/// relative to `b` (the golden/expected value).
fn rel_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= RELATIVE_TOLERANCE * b.abs().max(1.0)
}

#[test]
fn golden_ei_parity() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run golden parity)"
            );
            return;
        }
    };

    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    let golden: serde_json::Value =
        serde_json::from_str(&golden_str).expect("parse golden EI JSON");

    let golden_duration_ms = golden["durationMS"].as_f64().expect("durationMS");
    let golden_friendly_players = golden["friendlyPlayerCount"].as_i64().expect("friendlyPlayerCount");
    let golden_squad_damage = golden["squadTotalDamage"].as_f64().expect("squadTotalDamage");

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let duration = enc.duration_ms as f64;
    assert!(
        rel_close(duration, golden_duration_ms),
        "duration_ms {duration} not within {RELATIVE_TOLERANCE} relative of golden {golden_duration_ms}"
    );

    let friendly = enc.players.len() as i64;
    assert!(
        (friendly - golden_friendly_players).abs() <= FRIENDLY_COUNT_TOLERANCE,
        "friendly player count {friendly} not within ±{FRIENDLY_COUNT_TOLERANCE} of golden {golden_friendly_players}"
    );

    let squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    let squad_damage = squad_damage as f64;
    assert!(
        rel_close(squad_damage, golden_squad_damage),
        "squad damage {squad_damage} not within {RELATIVE_TOLERANCE} relative of golden {golden_squad_damage}"
    );

    println!(
        "golden parity: duration_ms={duration} (golden {golden_duration_ms}), \
         friendly={friendly} (golden {golden_friendly_players}), \
         squad_damage={squad_damage} (golden {golden_squad_damage})"
    );
}

/// Finding #4: `cc::timeline`'s squad_damage buckets and `downs::apply`'s
/// down_contribution windowed-damage loop must exclude
/// `result::CROWD_CONTROL` rows (which carry CC duration ms, not damage) —
/// exactly like `damage::accumulate` already does. After that fix,
/// `sum(timeline.squad_damage)` should equal `sum(player.damage_total)`
/// on the golden log, since both now use the same damage predicate (and
/// the timeline also folds in the same friendly pet/minion credit that
/// per-player totals get).
#[test]
fn golden_timeline_matches_player_damage_sum() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run golden parity)"
            );
            return;
        }
    };

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let timeline_sum: u64 = metrics.timeline.squad_damage.iter().sum();
    let player_sum: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    assert_eq!(
        timeline_sum, player_sum,
        "sum(timeline.squad_damage)={timeline_sum} != sum(player.damage_total)={player_sum}"
    );
    println!("golden timeline/player damage sum equality: {timeline_sum}");
}

/// Task 1 (M2): profession/elite-spec name calibration against the EI
/// golden fixture.
///
/// `fixtures/local/wvw-small.zevtc` is the *real*, unanonymized raw log (PII,
/// gitignored, present only for local/dev runs). `fixtures/wvw-small.ei.json`
/// is the *committed* golden fixture, whose `account` fields were anonymized
/// by axibridge's `scripts/obfuscate-accounts.mjs` (deterministic
/// sha256(account) -> "{Adjective}{Noun}.{4 digits}", from a fixed
/// adjective/noun word list). To join a real decoded account to its golden
/// row, this test reproduces that exact deterministic transform rather than
/// comparing account strings directly (which never matches: one side is real
/// names, the other is anonymized).
mod ei_account_obfuscation {
    use sha2::{Digest, Sha256};

    const ADJECTIVES: &[&str] = &[
        "Amber", "Arctic", "Ashen", "Bold", "Brisk", "Bright", "Calm", "Cinder", "Cloud",
        "Crimson", "Daring", "Dusky", "Echo", "Ember", "Fable", "Feral", "Frost", "Gilded",
        "Grand", "Harbor", "Hidden", "Iron", "Ivory", "Jade", "Keen", "Lively", "Lunar", "Merry",
        "Misty", "Nimble", "Nova", "Oak", "Onyx", "Placid", "Prime", "Quick", "Quiet", "Raven",
        "Royal", "Rustic", "Sable", "Scarlet", "Shaded", "Silver", "Solar", "Stone", "Storm",
        "Swift", "Umber", "Velvet", "Verdant", "Vivid", "Wild", "Winter", "Wise", "Woven",
        "Young", "Zephyr",
    ];
    const NOUNS: &[&str] = &[
        "Arrow", "Beacon", "Bloom", "Brook", "Canyon", "Cedar", "Cipher", "Comet", "Creek",
        "Crest", "Dawn", "Drift", "Ember", "Falcon", "Field", "Flare", "Forest", "Forge",
        "Garden", "Glen", "Grove", "Harbor", "Haven", "Hollow", "Horizon", "Jet", "Journey",
        "Keeper", "Lagoon", "Lane", "Laurel", "Leaf", "Light", "Meadow", "Mesa", "Morrow",
        "North", "Oak", "Pine", "Quill", "Range", "Ridge", "River", "Rune", "Sage", "Shore",
        "Sky", "Song", "Spark", "Spruce", "Star", "Summit", "Thorn", "Vale", "Vista", "Wave",
        "Willow", "Wisp",
    ];

    /// Only account-shaped strings get obfuscated by the source script
    /// (`^[A-Za-z][A-Za-z0-9 _'-]{1,31}\.\d{4}$`); anything else (e.g. the
    /// empty/blank accounts our decoder emits for a few relog stragglers)
    /// has no obfuscated counterpart to look up.
    fn looks_like_account(s: &str) -> bool {
        let bytes = s.as_bytes();
        if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
            return false;
        }
        let Some(dot) = s.rfind('.') else { return false };
        let (name, suffix) = (&s[..dot], &s[dot + 1..]);
        if name.is_empty() || name.len() > 32 {
            return false;
        }
        if suffix.len() != 4 || !suffix.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        name.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'\'' | b'-'))
    }

    /// Reproduces `buildFakeAccount` from axibridge's
    /// `scripts/obfuscate-accounts.mjs`.
    pub fn obfuscate(real_account_with_colon: &str) -> Option<String> {
        let real = real_account_with_colon.trim_start_matches(':');
        if !looks_like_account(real) {
            return None;
        }
        let digest = Sha256::digest(real.as_bytes());
        let left = u16::from_be_bytes([digest[0], digest[1]]) as usize;
        let right = u16::from_be_bytes([digest[2], digest[3]]) as usize;
        let num = u16::from_be_bytes([digest[4], digest[5]]) as u32;
        let adjective = ADJECTIVES[left % ADJECTIVES.len()];
        let noun = NOUNS[right % NOUNS.len()];
        let suffix = (num % 9000) + 1000;
        Some(format!("{adjective}{noun}.{suffix:04}"))
    }

    #[test]
    fn matches_known_axibridge_mapping() {
        // Spot-checked against fixtures/wvw-small.ei.json / the axibridge
        // golden JSON for this same encounter.
        assert_eq!(obfuscate(":Arx.9785").as_deref(), Some("ZephyrLagoon.2752"));
        assert_eq!(
            obfuscate(":Astronauta.1087").as_deref(),
            Some("AshenLaurel.2994")
        );
        assert_eq!(obfuscate(""), None);
    }
}

#[test]
fn professions_match_ei_golden() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run professions_match_ei_golden)"
            );
            return;
        }
    };

    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    let golden: serde_json::Value =
        serde_json::from_str(&golden_str).expect("parse golden EI JSON");
    let golden_players = golden["players"].as_array().expect("players array");

    let mut profession_by_fake_account = std::collections::HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account");
        let profession = p["profession"].as_str().expect("profession");
        profession_by_fake_account.insert(account.to_string(), profession.to_string());
    }

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);

    let mut matched = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for p in &enc.players {
        let Some(fake) = ei_account_obfuscation::obfuscate(&p.account) else {
            continue;
        };
        let Some(golden_profession) = profession_by_fake_account.get(&fake) else {
            continue;
        };
        // EI convention: `profession` is the elite-spec name when active,
        // else the base profession.
        let ei_style = if p.elite_spec.is_empty() {
            &p.profession
        } else {
            &p.elite_spec
        };
        matched += 1;
        if ei_style != golden_profession {
            mismatches.push(format!(
                "{} (prof={}, elite_spec={:?}): got {ei_style:?}, golden {golden_profession:?}",
                p.account, p.profession, p.elite_spec
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "profession mismatches vs EI golden:\n{}",
        mismatches.join("\n")
    );
    // The fixture has 41 friendly players; a handful are enemy players (also
    // real-named, so also obfuscate-able) with no golden row, plus a few
    // relog stragglers with a blank account. Require strong, not total,
    // coverage so this stays meaningful without being fragile to fixture
    // churn.
    assert!(
        matched >= 30,
        "expected at least 30 accounts to join to the EI golden fixture, got {matched}"
    );
    println!("professions_match_ei_golden: {matched} accounts joined, 0 mismatches");
}

/// Task 2 (M2): real WvW map-name and team-id→color tables, calibrated
/// against the golden fixture (a Green Alpine Borderlands skirmish; EI's
/// `fightName` is "World vs World - Green Alpine Borderlands").
#[test]
fn map_and_team_colors_match_ei_golden() {
    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run map_and_team_colors_match_ei_golden)"
            );
            return;
        }
    };

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");
    let enc = resolve(&raw);

    assert_eq!(
        enc.map, "Green Alpine Borderlands",
        "MAP_ID (src_agent=95) should resolve to Green Alpine Borderlands"
    );

    // Friendly players' team must resolve to a known color, never "unknown"
    // or the pre-M2 default-to-green placeholder masking an unrecognized id.
    assert!(
        !enc.players.is_empty(),
        "expected friendly players in the fixture"
    );
    let friendly_colors: std::collections::BTreeSet<&str> =
        enc.players.iter().map(|p| p.team.as_str()).collect();
    assert_eq!(
        friendly_colors.len(),
        1,
        "expected exactly one friendly team color, got {friendly_colors:?}"
    );
    let friendly_color = *friendly_colors.iter().next().unwrap();
    assert!(
        matches!(friendly_color, "red" | "green" | "blue"),
        "friendly team color should be a known color, got {friendly_color:?}"
    );

    // At least one enemy player must resolve to a *different* known color
    // (not "unknown", not the same color as the friendly squad).
    let enemy_colors: std::collections::BTreeSet<&str> = enc
        .enemies
        .iter()
        .filter(|e| e.is_player)
        .map(|e| e.team.as_str())
        .collect();
    assert!(
        enemy_colors
            .iter()
            .any(|&c| matches!(c, "red" | "green" | "blue") && c != friendly_color),
        "expected at least one enemy player team color distinct from friendly {friendly_color:?}, got {enemy_colors:?}"
    );

    println!(
        "map_and_team_colors_match_ei_golden: map={:?}, friendly={friendly_color:?}, enemies={enemy_colors:?}",
        enc.map
    );
}

/// Task 2b: CBTS_IDTOGUID decoding against the real fixture.
///
/// This fixture (Jan 2026) predates arcdps emitting CBTS_WVWTEAMS *and* TEAM
/// (content type 4) CBTS_IDTOGUID mappings — confirmed by inspecting the raw
/// event stream directly (zero sc=74 events; the sc=46 events present only
/// have content types 0/1/2/3 — Effect/Marker/Skill/Species). So this test
/// only makes conditional assertions: it requires the mapping machinery to
/// actually decode *something* real (proving the sc=46/skillid/overstack
/// wiring works end to end on real data, not just synthetic unit tests),
/// but does not assert on TEAM mappings specifically, since none exist in
/// this log — every team in `enc.teams` should have `guid: None`.
#[test]
fn guid_mappings_decode_from_local_fixture() {
    use axilog_core::evtc::ContentType;

    let bytes = match std::fs::read(FIXTURE_PATH) {
        Ok(b) => b,
        Err(_) => {
            println!(
                "skip: fixtures/local/wvw-small.zevtc absent (set up local fixture to run guid_mappings_decode_from_local_fixture)"
            );
            return;
        }
    };

    let raw = decode_raw(&bytes).expect("decode local WvW fixture");

    assert!(
        !raw.guid_map.is_empty(),
        "expected at least some CBTS_IDTOGUID mappings in the fixture"
    );
    let has_skill = raw.guid_map.iter().any(|g| g.content_type == ContentType::Skill);
    let has_species = raw.guid_map.iter().any(|g| g.content_type == ContentType::Species);
    assert!(has_skill, "expected at least one Skill GUID mapping");
    assert!(has_species, "expected at least one Species GUID mapping");

    // Every decoded GUID should be a well-formed 32-char lowercase hex string.
    for g in &raw.guid_map {
        let hex = g.guid_hex();
        assert_eq!(hex.len(), 32, "guid_hex should be 32 hex chars, got {hex:?}");
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "guid_hex should be lowercase hex, got {hex:?}"
        );
    }

    // This fixture predates TEAM-content-type CBTS_IDTOGUID mappings, so
    // every detected team should resolve to no GUID.
    let no_team_mappings = !raw.guid_map.iter().any(|g| g.content_type == ContentType::Team);
    assert!(no_team_mappings, "fixture unexpectedly has a Team GUID mapping (update this test)");

    let enc = resolve(&raw);
    assert!(!enc.teams.is_empty(), "expected detected teams in the fixture");
    for t in &enc.teams {
        assert_eq!(t.guid, None, "team {} unexpectedly has a guid in this GUID-less fixture", t.team_id);
    }

    println!(
        "guid_mappings_decode_from_local_fixture: {} total mappings, {} teams (all guid=None as expected)",
        raw.guid_map.len(),
        enc.teams.len()
    );
}
