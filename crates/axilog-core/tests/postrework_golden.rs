//! M4 Task 3: real-capture calibration hook for post-2026-05-01
//! (buff-statechange-rework) logs.
//!
//! M4 Tasks 1-2 era-gated buff/support/CC extraction against GW2EI source
//! and synthetic era-equivalence tests (see `analysis::buffs::events`'s and
//! `analysis::support`'s module docs) -- verified BY CONSTRUCTION, since no
//! real post-rework capture existed at the time. This file is the promised
//! follow-up: the moment a real post-rework `.zevtc` is dropped at
//! `fixtures/local/wvw-postrework.zevtc` (gitignored, PII, dev-only -- same
//! policy as `fixtures/local/wvw-small.zevtc`, see the README's "Fixture
//! policy"), these tests pick it up automatically and print a compact
//! summary table so the numbers are immediately visible, without anyone
//! having to write a new test first. Skips gracefully (prints `skip: ...
//! absent`) when the fixture isn't present -- exactly like every other
//! `fixtures/local/*`-gated test in this suite, so this file is a no-op in
//! CI.
//!
//! If a `fixtures/local/wvw-postrework.ei.json` sidecar (the dps.report
//! `getJson` output for that SAME log) is ALSO present, an additional test
//! checks duration/damage parity within the M2/M3 0.5% relative tolerance
//! and asserts the four support sums (condi cleanses / self-cleanses / boon
//! strips / resurrects) match EXACTLY -- mirroring `golden.rs`'s
//! `check_golden_ei_parity` and `support_golden.rs`'s
//! `check_support_matches_ei_golden`, but without those files' hardcoded
//! expected numbers (this fixture's actual values aren't known yet).

mod common;

use axilog_core::analysis::analyze;
use axilog_core::analysis::buffs::{ALACRITY, FURY, MIGHT, PROTECTION, QUICKNESS};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use common::{account_key, rel_close, read_bytes_or_skip, read_json_or_skip, RELATIVE_TOLERANCE};

fn zevtc_path() -> String {
    common::local_fixture("wvw-postrework.zevtc")
}
fn ei_json_path() -> String {
    common::local_fixture("wvw-postrework.ei.json")
}

/// Prints the "first real capture immediately surfaces numbers" summary
/// table the brief asks for: players, duration, squad damage, Might average
/// stacks (squad mean), Quickness presence % (squad mean), and the three
/// support totals.
fn print_summary(metrics: &axilog_core::analysis::Metrics, enc: &axilog_core::model::Encounter) {
    let players = enc.players.len();
    let duration_s = enc.duration_ms as f64 / 1000.0;
    let squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();

    let might_avg: f64 = {
        let vals: Vec<f64> = enc
            .players
            .iter()
            .filter_map(|p| metrics.boon_uptime.get(&(p.agent_addr, MIGHT)).map(|u| u.avg_stacks))
            .collect();
        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
    };
    let quickness_pct: f64 = {
        let vals: Vec<f64> = enc
            .players
            .iter()
            .filter_map(|p| metrics.boon_uptime.get(&(p.agent_addr, QUICKNESS)).map(|u| u.presence_pct))
            .collect();
        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
    };

    let cleanses: u32 = metrics.players.iter().map(|p| p.support.cleanses).sum();
    let strips: u32 = metrics.players.iter().map(|p| p.support.strips).sum();
    let resurrects: u32 = metrics.players.iter().map(|p| p.support.resurrects).sum();

    println!("post-rework calibration summary:");
    println!("  players         : {players}");
    println!("  duration        : {duration_s:.1}s");
    println!("  squad damage    : {squad_damage}");
    println!("  Might avg stacks: {might_avg:.2} (squad mean)");
    println!("  Quickness %     : {quickness_pct:.1}% (squad mean presence)");
    println!("  cleanses        : {cleanses}");
    println!("  strips          : {strips}");
    println!("  resurrects      : {resurrects}");
}

/// GATE: a real post-rework capture must decode+analyze warning-free (the
/// M4 Task 3 downgraded warning -- see `analysis::mod`'s doc comment --
/// only fires on genuinely zero extracted buff events, which a real fight
/// with any boon activity at all won't hit), and must produce non-zero
/// boon timelines / support counts, not the M3-era silent-zero this whole
/// milestone exists to fix.
#[test]
fn postrework_fixture_decodes_with_nonzero_boons_and_support() {
    let Some(bytes) = read_bytes_or_skip(&zevtc_path(), "postrework calibration") else { return };

    let raw = decode_raw(&bytes).expect("decode post-rework WvW fixture");
    assert!(
        raw.header.is_post_buff_rework(),
        "fixtures/local/wvw-postrework.zevtc build {:?} is not on/after the 20260501 buff-rework \
         threshold -- wrong fixture for this hook (use golden.rs/boons_golden.rs/support_golden.rs \
         for a pre-rework log instead)",
        raw.header.build
    );

    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    assert!(
        metrics.warnings.is_empty(),
        "post-rework fixture produced warnings (should be empty for a real fight with any boon \
         activity -- see analysis::mod's Metrics::warnings doc comment): {:?}",
        metrics.warnings
    );

    // Non-zero boon timelines for a handful of near-universal WvW boons.
    // Might is asserted hard (near-guaranteed in any organized squad, same
    // precedent as `golden.rs`'s `boons_smoke_nonempty_might_on_multiple_players`);
    // the rest are reported for visibility but not hard-failed, since a real
    // capture's exact boon mix (Quickness/Alacrity/Fury uptime depends on
    // squad composition) isn't something this hook can predict in advance.
    let players_with_boon = |buff_id: u32| -> usize {
        metrics
            .boons
            .iter()
            .filter(|&(&(_, id), tl)| id == buff_id && !tl.states.is_empty())
            .count()
    };
    let might_players = players_with_boon(MIGHT);
    assert!(
        might_players > 0,
        "expected Might to have a non-empty timeline for at least one squad player on a real \
         post-rework capture, got 0 -- post-era buff extraction may not be working"
    );
    println!(
        "post-rework boon smoke: Might={might_players}, Fury={}, Quickness={}, Protection={}, \
         Alacrity={} players with non-empty timelines",
        players_with_boon(FURY),
        players_with_boon(QUICKNESS),
        players_with_boon(PROTECTION),
        players_with_boon(ALACRITY),
    );

    // Non-zero support counts. Cleanses and strips are near-universal in any
    // real WvW fight with condition damage/boon-strip skills present;
    // resurrects legitimately can be zero (no deaths in the window), so it's
    // reported in the summary but not hard-asserted.
    let cleanses: u32 = metrics.players.iter().map(|p| p.support.cleanses).sum();
    let strips: u32 = metrics.players.iter().map(|p| p.support.strips).sum();
    assert!(
        cleanses > 0 || strips > 0,
        "expected at least one condi cleanse or boon strip on a real post-rework capture, got \
         cleanses=0 strips=0 -- post-era support extraction may not be working"
    );

    print_summary(&metrics, &enc);
}

/// GATE (only runs when the optional EI JSON sidecar is present too): the
/// same duration/squad-damage parity check as `golden.rs`'s
/// `check_golden_ei_parity`, and exact support-sum parity like
/// `support_golden.rs`'s `check_support_matches_ei_golden` -- but against
/// whatever real numbers this fixture actually has (no hardcoded
/// expectations, unlike those two files' committed fixtures).
#[test]
fn postrework_fixture_matches_ei_json_when_present() {
    let Some(bytes) = read_bytes_or_skip(&zevtc_path(), "postrework EI parity") else { return };
    let Some(golden) = read_json_or_skip(&ei_json_path(), "postrework EI parity") else { return };

    let raw = decode_raw(&bytes).expect("decode post-rework WvW fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    // Duration / squad damage, within the same 0.5% relative tolerance
    // `golden.rs` uses.
    let golden_duration_ms = golden["durationMS"].as_f64().expect("durationMS");
    let duration_ms = enc.duration_ms as f64;
    assert!(
        rel_close(duration_ms, golden_duration_ms, RELATIVE_TOLERANCE),
        "duration_ms {duration_ms} not within {RELATIVE_TOLERANCE} relative of golden \
         {golden_duration_ms}"
    );

    let golden_squad_damage = golden["squadTotalDamage"].as_f64().expect("squadTotalDamage");
    let squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    assert!(
        rel_close(squad_damage as f64, golden_squad_damage, RELATIVE_TOLERANCE),
        "squad damage {squad_damage} not within {RELATIVE_TOLERANCE} relative of golden \
         {golden_squad_damage}"
    );

    // Support sums: exact, per the M3 Task 3 calibration finding this
    // project carries forward (see `analysis::support::apply`'s doc
    // comment) -- post-era extraction reuses the identical credit logic
    // (M4 Task 2's `apply_post_era`), so exact parity is the expectation
    // here too, not just "plausible".
    let squad_cleanses: u64 = metrics.players.iter().map(|p| p.support.cleanses as u64).sum();
    let squad_cleanses_self: u64 =
        metrics.players.iter().map(|p| p.support.cleanses_self as u64).sum();
    let squad_strips: u64 = metrics.players.iter().map(|p| p.support.strips as u64).sum();
    let squad_resurrects: u64 = metrics.players.iter().map(|p| p.support.resurrects as u64).sum();

    let golden_cleanses = golden["squadCondiCleanse"].as_i64().expect("squadCondiCleanse") as u64;
    let golden_cleanses_self =
        golden["squadCondiCleanseSelf"].as_i64().expect("squadCondiCleanseSelf") as u64;
    let golden_strips = golden["squadBoonStrips"].as_i64().expect("squadBoonStrips") as u64;
    let golden_resurrects = golden["squadResurrects"].as_i64().expect("squadResurrects") as u64;

    assert_eq!(squad_cleanses, golden_cleanses, "squad condiCleanse");
    assert_eq!(squad_cleanses_self, golden_cleanses_self, "squad condiCleanseSelf");
    assert_eq!(squad_strips, golden_strips, "squad boonStrips");
    assert_eq!(squad_resurrects, golden_resurrects, "squad resurrects");

    // Sanity that the join key convention (`account_key`) this module
    // shares with `golden.rs`/`support_golden.rs` still applies to a real
    // (non-anonymized) fixture's account strings -- exercised here even
    // though this test doesn't do a per-player join (squad sums are enough
    // for this hook), so a future per-player extension has a working
    // starting point.
    if let Some(players) = golden.get("players").and_then(|p| p.as_array()) {
        if let Some(first) = players.first().and_then(|p| p["account"].as_str()) {
            assert!(!account_key(first).is_empty(), "golden EI JSON account field unexpectedly empty");
        }
    }

    println!(
        "postrework_fixture_matches_ei_json: duration_ms={duration_ms} (golden \
         {golden_duration_ms}), squad_damage={squad_damage} (golden {golden_squad_damage}), \
         support=({squad_cleanses}/{squad_cleanses_self}/{squad_strips}/{squad_resurrects}) exact"
    );
}

/// M10 Task 2 sanity gate: opt-in missile analytics against the same real
/// post-rework capture. `fixtures/local/wvw-postrework.ei.json`'s own
/// `defenses` block (`blockedCount`/`evadedCount`/`missedCount`/etc.) was
/// checked and is NOT a comparable signal -- those are whole-fight,
/// all-attack-types (melee included) counters derived from `CBTS_COMBAT`
/// strike `result` values, not per-missile-instance fired/hit/denied
/// counts from `CBTS_MISSILECREATE`/`LAUNCH`/`REMOVE`. No EI field maps
/// onto this project's native missile stats, so this stays a pure sanity
/// gate (non-zero fired, `denied <= fired` at squad level), not a
/// calibration -- per the M10 Task 2 brief ("document native-only ...
/// do NOT invent EI fields").
#[test]
fn postrework_fixture_missile_sanity() {
    let Some(bytes) = read_bytes_or_skip(&zevtc_path(), "postrework missile sanity") else { return };

    let raw = decode_raw(&bytes).expect("decode post-rework WvW fixture");
    let enc = resolve(&raw);
    let missiles = axilog_core::analysis::missiles::build_missiles(&raw, &enc);

    assert!(
        missiles.squad.fired > 0,
        "expected at least one CBTS_MISSILECREATE event owned by a squad player on a real WvW \
         capture, got 0 -- missile decode/attribution may be broken"
    );
    assert!(
        missiles.squad.denied <= missiles.squad.fired,
        "squad denied ({}) must not exceed squad fired ({})",
        missiles.squad.denied,
        missiles.squad.fired
    );
    assert!(
        missiles.squad.hit + missiles.squad.denied <= missiles.squad.fired,
        "hit ({}) + denied ({}) must not exceed fired ({}) -- unresolved instances (still in \
         flight at log end) must not be double counted",
        missiles.squad.hit,
        missiles.squad.denied,
        missiles.squad.fired
    );
    assert!(
        missiles.squad.incoming_denied <= missiles.squad.incoming_fired,
        "incoming_denied ({}) must not exceed incoming_fired ({})",
        missiles.squad.incoming_denied,
        missiles.squad.incoming_fired
    );

    let mut top: Vec<_> = missiles.players.iter().filter(|p| p.fired > 0).collect();
    top.sort_by_key(|p| std::cmp::Reverse(p.fired));

    println!("post-rework missile summary:");
    println!(
        "  squad: fired={} hit={} denied={} incoming_fired={} incoming_denied={}",
        missiles.squad.fired,
        missiles.squad.hit,
        missiles.squad.denied,
        missiles.squad.incoming_fired,
        missiles.squad.incoming_denied
    );
    println!("  players with missile activity: {}", top.len());
    println!("  {:<10} {:>6} {:>6} {:>6} {:>10}", "addr", "fired", "hit", "denied", "reflected");
    for p in top.iter().take(10) {
        println!("  {:<10} {:>6} {:>6} {:>6} {:>10}", p.agent_addr, p.fired, p.hit, p.denied, p.reflected_at_self);
    }
}
