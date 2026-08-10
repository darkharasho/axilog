//! Real-fixture sanity for `analysis::contribution` (M11 Task 2).
//!
//! No EI golden to calibrate against here -- EI computes down-contribution
//! with a DIFFERENT algorithm by design (see `analysis::contribution`'s
//! module doc); the whole point of this engine is to diverge from EI's
//! approximation and match the real arcdps methodology instead. These tests
//! instead check the engine is internally plausible on real logs, BOTH eras:
//! non-zero totals (a real WvW fight has real downs), a structural subset
//! bound (`sum(downs_contribution.damage) <= sum(damage_total)` -- every
//! credited damage event is also counted in that same player's overall
//! damage total, and no single real damage event can ever land in two
//! different targets' windows, since each is scoped to its own `dst_agent`
//! -- see `contribution`'s module doc), and a printed top-contributor
//! summary for human review.
//!
//! Runs against the committed, PII-safe `fixtures/wvw-small.anon.zevtc`
//! (pre-era, always present -- so this runs in CI too, mirroring `golden.rs`/
//! `health_fixture.rs`'s convention). The gitignored local fixtures
//! (`wvw-small.zevtc`, pre-era; `wvw-postrework.zevtc`, post-era) are
//! checked too, skipped gracefully when absent.

use axilog_core::analysis::analyze;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

mod common;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
fn local_small_fixture_path() -> String {
    common::local_fixture("wvw-small.zevtc")
}
fn local_postrework_fixture_path() -> String {
    common::local_fixture("wvw-postrework.zevtc")
}

fn read_local_or_skip(path: &str, test_name: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(_) => {
            println!("skip: {path} absent ({test_name} local-only extra check)");
            None
        }
    }
}

/// Runs the full sanity suite against a decoded fixture's bytes; shared by
/// the committed-fixture test (required) and the two local-fixture extras
/// (best-effort, skipped when absent).
fn check_contribution_sanity(bytes: &[u8], label: &str) {
    let raw = decode_raw(bytes).unwrap_or_else(|e| panic!("decode {label}: {e}"));
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let total_downs_dealt: u32 = metrics.players.iter().map(|p| p.downs_dealt).sum();
    let total_downs_taken: u32 = metrics.players.iter().map(|p| p.downs_taken).sum();
    assert!(
        total_downs_dealt > 0 && total_downs_taken > 0,
        "{label}: a real WvW fight fixture must have both outgoing and incoming downs to sanity-check the contribution engine against (got dealt={total_downs_dealt}, taken={total_downs_taken})"
    );

    let total_squad_damage: u64 = metrics.players.iter().map(|p| p.damage_total).sum();
    let total_downs_contribution_damage: u64 =
        metrics.players.iter().map(|p| p.downs_contribution.damage).sum();
    let total_downed_by_damage: u64 = metrics.players.iter().map(|p| p.downed_by.damage).sum();

    assert!(
        total_downs_contribution_damage > 0,
        "{label}: squad downed {total_downs_dealt} enemies but downs_contribution.damage totals zero -- engine likely broken"
    );
    assert!(
        total_downed_by_damage > 0,
        "{label}: squad took {total_downs_taken} downs but downed_by.damage totals zero -- engine likely broken"
    );

    // Structural subset bound (see module doc): every credited in-window
    // damage event is also counted in that SAME player's overall
    // damage_total (the identical predicate, plus pet-credit folding both
    // paths already apply), and a given real damage event can only ever
    // land in ONE target's window (scoped by `dst_agent`) -- so the squad-
    // wide sum can never exceed total squad damage dealt.
    assert!(
        total_downs_contribution_damage <= total_squad_damage,
        "{label}: downs_contribution.damage sum {total_downs_contribution_damage} exceeds total squad damage {total_squad_damage} -- structural subset bound violated"
    );

    // Printed per-player summary: top 5 outgoing contributors by damage.
    let mut ranked: Vec<_> = enc
        .players
        .iter()
        .zip(metrics.players.iter())
        .map(|(p, m)| (p.account.clone(), m.downs_contribution))
        .collect();
    ranked.sort_by_key(|(_, c)| std::cmp::Reverse(c.damage));
    println!(
        "{label}: downs_dealt={total_downs_dealt} downs_taken={total_downs_taken} \
         downs_contribution.damage={total_downs_contribution_damage} (of {total_squad_damage} \
         total squad damage) downed_by.damage={total_downed_by_damage}"
    );
    for (account, c) in ranked.iter().take(5) {
        println!(
            "{label}: top contributor {account}: damage={} cc={} strips={} movement_impairing={}",
            c.damage, c.cc, c.strips, c.movement_impairing
        );
    }
}

#[test]
fn committed_fixture_contribution_is_plausible() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    check_contribution_sanity(&bytes, "wvw-small.anon.zevtc (pre-era)");
}

#[test]
fn local_small_fixture_contribution_is_plausible_when_present() {
    if let Some(bytes) = read_local_or_skip(&local_small_fixture_path(), "local wvw-small") {
        check_contribution_sanity(&bytes, "local wvw-small.zevtc (pre-era)");
    }
}

#[test]
fn local_postrework_fixture_contribution_is_plausible_when_present() {
    if let Some(bytes) = read_local_or_skip(&local_postrework_fixture_path(), "local wvw-postrework") {
        check_contribution_sanity(&bytes, "local wvw-postrework.zevtc (post-era)");
    }
}
