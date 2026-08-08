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
//! Runs against the committed, PII-safe `fixtures/wvw-small.anon.zevtc`
//! (always present, so these tests run in CI too — Task 5, M2): player
//! names don't feed any metric, so the anonymized fixture reproduces
//! exactly the same numbers as the real log. When the real local raw
//! fixture is also present (gitignored, PII, dev-only), it is checked too
//! as a belt-and-braces extra.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const LOCAL_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-small.zevtc"
);
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

const RELATIVE_TOLERANCE: f64 = 0.005; // 0.5%
const FRIENDLY_COUNT_TOLERANCE: i64 = 2; // ±2
// Task 3 (M2) brief: CC totals calibrated within 2% (looser than the 0.5%
// used for damage/duration — CC application is synthesized by arcdps under
// generic pseudo-skills and pet-credit resolution is a heuristic, see
// `analysis::cc::pet_credit_cc_events`).
const CC_RELATIVE_TOLERANCE: f64 = 0.02; // 2%

/// True if `a` and `b` are within `RELATIVE_TOLERANCE` of each other,
/// relative to `b` (the golden/expected value).
fn rel_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= RELATIVE_TOLERANCE * b.abs().max(1.0)
}

/// True if `a` and `b` are within `CC_RELATIVE_TOLERANCE` of each other,
/// relative to `b` (the golden/expected value).
fn rel_close_cc(a: f64, b: f64) -> bool {
    (a - b).abs() <= CC_RELATIVE_TOLERANCE * b.abs().max(1.0)
}

fn read_local_fixture_or_skip(test_name: &str) -> Option<Vec<u8>> {
    match std::fs::read(LOCAL_FIXTURE_PATH) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {LOCAL_FIXTURE_PATH} absent ({test_name} local-only extra check)");
            None
        }
    }
}

fn read_anon_fixture() -> Vec<u8> {
    std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"))
}

fn read_golden_json() -> serde_json::Value {
    let golden_str = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&golden_str).expect("parse golden EI JSON")
}

fn check_golden_ei_parity(bytes: &[u8], golden: &serde_json::Value) {
    let golden_duration_ms = golden["durationMS"].as_f64().expect("durationMS");
    let golden_friendly_players = golden["friendlyPlayerCount"].as_i64().expect("friendlyPlayerCount");
    let golden_squad_damage = golden["squadTotalDamage"].as_f64().expect("squadTotalDamage");

    let raw = decode_raw(bytes).expect("decode WvW fixture");
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

#[test]
fn golden_ei_parity() {
    let golden = read_golden_json();
    check_golden_ei_parity(&read_anon_fixture(), &golden);
}

#[test]
fn golden_ei_parity_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("golden_ei_parity") else { return };
    let golden = read_golden_json();
    check_golden_ei_parity(&bytes, &golden);
}

/// Finding #4: `cc::timeline`'s squad_damage buckets and `downs::apply`'s
/// down_contribution windowed-damage loop must exclude
/// `result::CROWD_CONTROL` rows (which carry CC duration ms, not damage) —
/// exactly like `damage::accumulate` already does. After that fix,
/// `sum(timeline.squad_damage)` should equal `sum(player.damage_total)`
/// on the golden log, since both now use the same damage predicate (and
/// the timeline also folds in the same friendly pet/minion credit that
/// per-player totals get).
fn check_golden_timeline_matches_player_damage_sum(bytes: &[u8]) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
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

#[test]
fn golden_timeline_matches_player_damage_sum() {
    check_golden_timeline_matches_player_damage_sum(&read_anon_fixture());
}

#[test]
fn golden_timeline_matches_player_damage_sum_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("golden_timeline_matches_player_damage_sum") else { return };
    check_golden_timeline_matches_player_damage_sum(&bytes);
}

/// Task 1 (M2): sha256-based real-account -> synthetic-EI-account
/// obfuscation, as produced by axibridge's `scripts/obfuscate-accounts.mjs`.
///
/// Historical/reference only as of Task 5 (M2): this was originally used at
/// test time to join a real decoded account to its golden JSON row (the
/// golden JSON's `account` field held the sha-obfuscated form). Since Task
/// 5, `fixtures/wvw-small.ei.json`'s `account` fields hold `Anon<N>.<4
/// digits>` values instead (matching the committed, PII-safe
/// `fixtures/wvw-small.anon.zevtc`), so `professions_match_ei_golden` below
/// joins by raw agent-table index instead and no longer calls `obfuscate`.
/// This module (and its own fixture-independent unit test) is kept because
/// it's what was used to derive the current `wvw-small.ei.json` `account`
/// values from the real accounts in the one-time Task 5 regeneration, and
/// may still be useful for a future real-fixture refresh.
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
    #[allow(dead_code)]
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
        // Spot-checked against the original (pre-Task-5) golden JSON for
        // this same encounter, before its accounts were regenerated to
        // Anon<N> form.
        assert_eq!(obfuscate(":Arx.9785").as_deref(), Some("ZephyrLagoon.2752"));
        assert_eq!(
            obfuscate(":Astronauta.1087").as_deref(),
            Some("AshenLaurel.2994")
        );
        assert_eq!(obfuscate(""), None);
    }
}

/// Task 1 (M2): profession/elite-spec name calibration against the EI
/// golden fixture.
///
/// Task 5 (M2): joins by raw agent-table index rather than account text.
/// `fixtures/wvw-small.ei.json`'s `players[].account` values are
/// `Anon<N>.<4 digits>` (`N` = raw agent-table index), the exact same
/// deterministic value `axilog_core::evtc::anon_account` computes for
/// index `N` — this holds whether the raw bytes being decoded are the
/// committed anonymized fixture (whose own agent table literally has
/// `Anon<N>` names at index `N`) or the real local raw fixture (whose
/// agent table has the *same* agent order/count, just with real names at
/// each index) — either way, "the player at raw agent-table index N" is
/// the same person, and `anon_account(N)` is what golden.json calls them.
fn check_professions_match_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let golden_players = golden["players"].as_array().expect("players array");
    let mut profession_by_account = std::collections::HashMap::new();
    for p in golden_players {
        let account = p["account"].as_str().expect("account");
        let profession = p["profession"].as_str().expect("profession");
        profession_by_account.insert(account.to_string(), profession.to_string());
    }

    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let by_addr: std::collections::HashMap<u64, &axilog_core::model::Player> =
        enc.players.iter().map(|p| (p.agent_addr, p)).collect();

    let mut matched = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (i, agent) in raw.agents.iter().enumerate() {
        if !agent.is_player() {
            continue;
        }
        let expected_account = axilog_core::evtc::anon_account(i);
        let key = expected_account.trim_start_matches(':');
        let Some(golden_profession) = profession_by_account.get(key) else {
            continue;
        };
        let Some(p) = by_addr.get(&agent.addr) else {
            continue; // relog-absorbed / not the account's representative addr
        };
        // EI convention: `profession` is the elite-spec name when active,
        // else the base profession.
        let ei_style = if p.elite_spec.is_empty() { &p.profession } else { &p.elite_spec };
        matched += 1;
        if ei_style != golden_profession {
            mismatches.push(format!(
                "agent[{i}] addr={:#x} (prof={}, elite_spec={:?}): got {ei_style:?}, golden {golden_profession:?}",
                agent.addr, p.profession, p.elite_spec
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "profession mismatches vs EI golden:\n{}",
        mismatches.join("\n")
    );
    // The fixture has 41 friendly players; a handful of golden rows are
    // "Non Squad Player N" placeholders with no real-account origin, plus a
    // few relog stragglers whose representative addr differs from the raw
    // index being probed. Require strong, not total, coverage so this stays
    // meaningful without being fragile to fixture churn.
    assert!(
        matched >= 30,
        "expected at least 30 accounts to join to the EI golden fixture, got {matched}"
    );
    println!("professions_match_ei_golden: {matched} accounts joined, 0 mismatches");
}

#[test]
fn professions_match_ei_golden() {
    let golden = read_golden_json();
    check_professions_match_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn professions_match_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("professions_match_ei_golden") else { return };
    let golden = read_golden_json();
    check_professions_match_ei_golden(&bytes, &golden);
}

/// Task 2 (M2): real WvW map-name and team-id→color tables, calibrated
/// against the golden fixture (a Green Alpine Borderlands skirmish; EI's
/// `fightName` is "World vs World - Green Alpine Borderlands").
fn check_map_and_team_colors_match_ei_golden(bytes: &[u8]) {
    let raw = decode_raw(bytes).expect("decode WvW fixture");
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

#[test]
fn map_and_team_colors_match_ei_golden() {
    check_map_and_team_colors_match_ei_golden(&read_anon_fixture());
}

#[test]
fn map_and_team_colors_match_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("map_and_team_colors_match_ei_golden") else { return };
    check_map_and_team_colors_match_ei_golden(&bytes);
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
fn check_guid_mappings_decode(bytes: &[u8]) {
    use axilog_core::evtc::ContentType;

    let raw = decode_raw(bytes).expect("decode WvW fixture");

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

#[test]
fn guid_mappings_decode_from_local_fixture() {
    check_guid_mappings_decode(&read_anon_fixture());
}

#[test]
fn guid_mappings_decode_from_local_fixture_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("guid_mappings_decode_from_local_fixture") else { return };
    check_guid_mappings_decode(&bytes);
}

/// Task 3 (M2): CC metrics calibration against the EI golden fixture.
///
/// `analysis::cc::apply_cc` credits both direct player-sourced CC and
/// pet/minion-sourced CC (folded onto the owning squad player, matching
/// GW2EI's `SingleActor.InitOutgoingCrowdControlEvents`, which adds each
/// player's minions' outgoing CC into the player's own totals). This test
/// confirms squad-wide `cc_applied`/`cc_duration_ms` sums land within 2% of
/// EI's golden `statsAll[0].appliedCrowdControl`/`appliedCrowdControlDuration`
/// sums (34 / 50460ms) — pet-credit inclusion is required to match; without
/// it the squad sum undercounts (real players alone don't reach 34/50460 on
/// this log).
fn check_cc_matches_ei_golden(bytes: &[u8], golden: &serde_json::Value) {
    let golden_cc = golden["squadAppliedCrowdControl"].as_f64().expect("squadAppliedCrowdControl");
    let golden_cc_dur =
        golden["squadAppliedCrowdControlDuration"].as_f64().expect("squadAppliedCrowdControlDuration");

    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let cc_applied: u32 = metrics.players.iter().map(|p| p.cc_applied).sum();
    let cc_duration_ms: u64 = metrics.players.iter().map(|p| p.cc_duration_ms).sum();

    assert!(
        rel_close_cc(cc_applied as f64, golden_cc),
        "squad cc_applied {cc_applied} not within {CC_RELATIVE_TOLERANCE} relative of golden {golden_cc}"
    );
    assert!(
        rel_close_cc(cc_duration_ms as f64, golden_cc_dur),
        "squad cc_duration_ms {cc_duration_ms} not within {CC_RELATIVE_TOLERANCE} relative of golden {golden_cc_dur}"
    );

    println!(
        "cc_matches_ei_golden: cc_applied={cc_applied} (golden {golden_cc}), \
         cc_duration_ms={cc_duration_ms} (golden {golden_cc_dur})"
    );
}

#[test]
fn cc_matches_ei_golden() {
    let golden = read_golden_json();
    check_cc_matches_ei_golden(&read_anon_fixture(), &golden);
}

#[test]
fn cc_matches_ei_golden_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("cc_matches_ei_golden") else { return };
    let golden = read_golden_json();
    check_cc_matches_ei_golden(&bytes, &golden);
}

/// Task 3 (M2): CBTS_STUNBREAK sanity check against the real fixture.
///
/// The golden EI JSON reports 20 stun breaks / ~16.9s (16907ms) of removed
/// stun duration across the 41 friendly players in this log
/// (`support[0].stunBreak`/`removedStunDuration`, summed). This asserts our
/// own decode of the real `CBTS_STUNBREAK` (sc=56) event stream is nonzero
/// and lands close to that same total — not a strict equality requirement
/// (arcdps/EI may attribute a small number of edge-case events, e.g.
/// ally-redirected stun breaks, differently), but "plausible" per the task
/// brief.
fn check_stun_breaks_are_plausible(bytes: &[u8], golden: &serde_json::Value) {
    let golden_sb = golden["squadStunBreak"].as_f64().expect("squadStunBreak");
    let golden_rsd_ms = golden["squadRemovedStunDuration"].as_f64().expect("squadRemovedStunDuration") * 1000.0;

    let raw = decode_raw(bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let stun_breaks: u32 = metrics.players.iter().map(|p| p.stun_breaks).sum();
    let removed_stun_duration_ms: u64 = metrics.players.iter().map(|p| p.removed_stun_duration_ms).sum();

    assert!(stun_breaks > 0, "expected at least one CBTS_STUNBREAK event in the fixture");
    assert!(
        rel_close_cc(stun_breaks as f64, golden_sb),
        "squad stun_breaks {stun_breaks} not within {CC_RELATIVE_TOLERANCE} relative of golden {golden_sb}"
    );
    assert!(
        rel_close_cc(removed_stun_duration_ms as f64, golden_rsd_ms),
        "squad removed_stun_duration_ms {removed_stun_duration_ms} not within {CC_RELATIVE_TOLERANCE} relative of golden {golden_rsd_ms}"
    );

    println!(
        "stun_breaks_are_plausible_on_local_fixture: stun_breaks={stun_breaks} (golden {golden_sb}), \
         removed_stun_duration_ms={removed_stun_duration_ms} (golden {golden_rsd_ms})"
    );
}

#[test]
fn stun_breaks_are_plausible_on_local_fixture() {
    let golden = read_golden_json();
    check_stun_breaks_are_plausible(&read_anon_fixture(), &golden);
}

#[test]
fn stun_breaks_are_plausible_on_local_fixture_local_raw_when_present() {
    let Some(bytes) = read_local_fixture_or_skip("stun_breaks_are_plausible_on_local_fixture") else { return };
    let golden = read_golden_json();
    check_stun_breaks_are_plausible(&bytes, &golden);
}
