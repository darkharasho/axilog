//! Calibration of [`axilog_core::analysis::ei_replay`] (M15 Task 1) against
//! GW2EI's own exported `players[].combatReplayData`.
//!
//! Two sources, both gated on the file being present (this suite is a
//! no-op-with-a-printed-`skip:` in CI):
//!
//! 1. **PRIMARY (local-only, gitignored):** the real post-rework
//!    build-20260718 WvW capture `fixtures/local/wvw-postrework.zevtc` and
//!    its GW2EI export `fixtures/local/wvw-postrework.ei.json` (Blue Alpine
//!    Borderlands, mapID 96, 348362ms). This is the hard calibration gate:
//!    `start`/`end`/`dc`/`down`/`dead` must match EXACTLY for every player,
//!    positions must land within 1 map pixel for >=99% of samples, and
//!    orientations within a documented tolerance.
//!
//!    Because a `git worktree` gets an EMPTY `fixtures/local/` (the
//!    captures are PII and are never committed or copied), point
//!    `AXILOG_LOCAL_FIXTURES` at the primary checkout's directory to run
//!    it from a worktree:
//!    ```sh
//!    AXILOG_LOCAL_FIXTURES=/path/to/axilog/fixtures/local \
//!        cargo test -p axilog-core --test ei_replay_golden -- --nocapture
//!    ```
//!    See `tests/common/mod.rs`'s `local_fixture`.
//!
//! 2. **SANITY (committed, both eras):** `fixtures/wvw-small.anon.zevtc`
//!    (pre-rework, anonymized) gets a structural sanity sweep -- non-empty
//!    tracks, a strictly monotone grid at the 300ms rate, no NaN/infinite
//!    coordinates, orientations in `[-180, 180]`, `dc` sentinel bracketing
//!    -- plus the same sweep over the post-rework capture when present.
//!
//! ## Calibration result (recorded 2026-08-09, GW2EI `master` @ `7a6fe03`)
//!
//! See the `EI_REPLAY CALIBRATION` line this test prints under
//! `--nocapture`; the numbers are reproduced in
//! `.superpowers/sdd/2026-08-09-m15-ei-replay/task-1-report.md`.

mod common;

use axilog_core::analysis::ei_replay::{build_ei_replay, EiTrack, MapTransform, EI_POLL_MS};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use std::collections::HashMap;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");

/// Task 1 brief's position gate: >=99% of samples within 1.0 map pixel.
const PIXEL_TOLERANCE: f64 = 1.0;
const MIN_POSITION_WITHIN_FRACTION: f64 = 0.99;

/// Orientation gate. GW2EI rounds the exported angle to 3 decimals AFTER
/// converting an `f32` facing vector through `atan2` (see
/// `ei_replay::facing_to_degrees`'s citation), so the achievable floor is
/// ~0.001 deg for samples that come straight off a raw facing event. Polled
/// samples that are LERPed between two facing vectors amplify any last-ULP
/// difference in the interpolation weight, and a handful of samples sit on
/// the +-180 deg branch cut where an epsilon of vector difference flips the
/// reported angle by 360 deg. Both are accounted for: the per-sample
/// comparison is done modulo 360 (so the branch cut is a non-event), and
/// the gate is a >=99% fraction within 1.0 deg rather than an all-samples
/// bound.
const ANGLE_TOLERANCE_DEG: f64 = 1.0;
const MIN_ANGLE_WITHIN_FRACTION: f64 = 0.99;

fn read_json(path: &str) -> Option<serde_json::Value> {
    common::read_json_or_skip(path, "M15 EI-replay calibration")
}

/// Shortest signed angular difference, in degrees -- `+179.999` and
/// `-179.999` are 0.002 deg apart, not 359.998.
fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

/// Structural invariants every track must satisfy on every era, whether or
/// not a golden export is available to compare against.
fn assert_track_is_structurally_sound(tr: &EiTrack, log_duration: i64, label: &str) {
    let who = format!("{label}/{}", tr.name);
    assert!(tr.start <= tr.end, "{who}: start {} > end {}", tr.start, tr.end);
    assert!(tr.start >= 0, "{who}: negative start {}", tr.start);
    assert!(tr.end <= log_duration, "{who}: end {} past log duration {log_duration}", tr.end);

    // The grid: every sample count is exactly the number of `EI_POLL_MS`
    // multiples in `[start, end]` that are strictly below the log duration.
    let expected = axilog_core::analysis::ei_replay::grid_window(tr.start, tr.end, log_duration, EI_POLL_MS)
        .count();
    assert_eq!(tr.positions.len(), expected, "{who}: position count off the polling grid");
    assert!(
        tr.orientations.is_empty() || tr.orientations.len() == tr.positions.len(),
        "{who}: orientations must be empty or grid-aligned ({} vs {})",
        tr.orientations.len(),
        tr.positions.len()
    );

    for (i, xy) in tr.positions.iter().enumerate() {
        assert!(xy[0].is_finite() && xy[1].is_finite(), "{who}: non-finite position at {i}: {xy:?}");
    }
    for (i, a) in tr.orientations.iter().enumerate() {
        assert!(a.is_finite(), "{who}: non-finite orientation at {i}");
        assert!((-180.0..=180.0).contains(a), "{who}: orientation {a} out of range at {i}");
    }

    // `dc` bracketing: always opens with the `[i64::MIN, start]` head, and
    // every interval is ascending and non-overlapping.
    assert!(!tr.dc.is_empty(), "{who}: dc must always carry at least the leading sentinel");
    assert_eq!(tr.dc[0][0], i64::MIN, "{who}: dc must open with the i64::MIN sentinel");
    for w in tr.dc.windows(2) {
        assert!(w[0][1] <= w[1][0], "{who}: overlapping dc intervals {:?} {:?}", w[0], w[1]);
    }
    for iv in tr.dc.iter().chain(&tr.down).chain(&tr.dead) {
        assert!(iv[0] < iv[1], "{who}: degenerate interval {iv:?}");
    }
}

/// Post-rework calibration + sanity. Skips when the local capture/export
/// pair is absent.
#[test]
fn ei_replay_matches_gw2ei_export_on_the_local_postrework_capture() {
    let zevtc = common::local_fixture("wvw-postrework.zevtc");
    let json = common::local_fixture("wvw-postrework.ei.json");
    let Some(bytes) = common::read_bytes_or_skip(&zevtc, "M15 EI-replay calibration") else {
        return;
    };
    let Some(golden) = read_json(&json) else { return };

    assert_eq!(golden["mapID"].as_i64(), Some(96), "expected Blue Alpine Borderlands (mapID 96)");
    let golden_players = golden["players"].as_array().expect("players array");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let log_duration = enc.duration_ms as i64;
    assert_eq!(
        log_duration,
        golden["durationMS"].as_i64().expect("durationMS"),
        "the polling grid's upper bound is GW2EI's LogDuration -- a mismatch here \
         invalidates every sample count below"
    );

    let map = MapTransform::from_log(&raw).expect("mapID 96 has a built-in transform");
    assert_eq!(
        map.sizes(),
        [
            golden["combatReplayMetaData"]["sizes"][0].as_i64().unwrap(),
            golden["combatReplayMetaData"]["sizes"][1].as_i64().unwrap()
        ],
        "GetPixelMapSize must reproduce the export's combatReplayMetaData.sizes"
    );
    assert_eq!(
        map.inch_to_pixel() as f32,
        golden["combatReplayMetaData"]["inchToPixel"].as_f64().unwrap() as f32,
        "GetInchToPixel must reproduce the export's combatReplayMetaData.inchToPixel"
    );
    assert_eq!(
        golden["combatReplayMetaData"]["pollingRate"].as_i64(),
        Some(EI_POLL_MS),
        "GW2EI's CombatReplayPollingRate is a compile-time 300, not fight-length dependent"
    );

    let tracks = build_ei_replay(&raw, &enc, &map);
    for tr in &tracks {
        assert_track_is_structurally_sound(tr, log_duration, "postrework");
    }

    // Squad players join to the export by account (the four non-squad
    // players are anonymized to "Non Squad Player N" in the export and have
    // no account to join on).
    let account_by_addr: HashMap<u64, String> = raw
        .agents
        .iter()
        .filter(|a| a.is_player())
        .map(|a| {
            let (_, account, _) = a.name_parts();
            (a.addr, common::account_key(&account).to_string())
        })
        .collect();
    let golden_by_account: HashMap<&str, &serde_json::Value> = golden_players
        .iter()
        .filter_map(|p| p["account"].as_str().map(|a| (common::account_key(a), p)))
        .collect();

    let mut players_checked = 0usize;
    let (mut pos_within, mut pos_total) = (0usize, 0usize);
    let (mut ang_within, mut ang_total) = (0usize, 0usize);
    // Stronger-than-required observation: how many samples are BIT-exact
    // once both sides are narrowed to the `f32` GW2EI actually serializes
    // (its JSON text is an `f32` printed as decimal, which `serde_json`
    // widens to a nearby-but-unequal `f64`).
    let (mut pos_exact, mut ang_exact) = (0usize, 0usize);
    let mut worst_pos = (0.0f64, String::new());
    let mut worst_ang = (0.0f64, String::new());
    let mut per_player_outliers: Vec<(String, usize, usize)> = Vec::new();

    for tr in &tracks {
        let Some(account) = account_by_addr.get(&tr.agent_addr) else { continue };
        let Some(gp) = golden_by_account.get(account.as_str()) else { continue };
        let Some(crd) = gp.get("combatReplayData") else { continue };
        players_checked += 1;

        // -- EXACT gates --
        assert_eq!(crd["start"].as_i64(), Some(tr.start), "{account}: combatReplayData.start");
        assert_eq!(crd["end"].as_i64(), Some(tr.end), "{account}: combatReplayData.end");
        assert_eq!(
            intervals(crd.get("dc")),
            tr.dc,
            "{account}: combatReplayData.dc (sentinel bracketing + mid-fight disconnects)"
        );
        assert_eq!(intervals(crd.get("down")), tr.down, "{account}: combatReplayData.down");
        assert_eq!(intervals(crd.get("dead")), tr.dead, "{account}: combatReplayData.dead");

        // -- positions --
        let gpos = crd["positions"].as_array().expect("positions");
        assert_eq!(gpos.len(), tr.positions.len(), "{account}: position sample count");
        let mut outliers = 0usize;
        for (i, (ours, theirs)) in tr.positions.iter().zip(gpos).enumerate() {
            let gx = theirs[0].as_f64().unwrap();
            let gy = theirs[1].as_f64().unwrap();
            let err = ((ours[0] - gx).powi(2) + (ours[1] - gy).powi(2)).sqrt();
            pos_total += 1;
            if err <= PIXEL_TOLERANCE {
                pos_within += 1;
            } else {
                outliers += 1;
            }
            if ours[0] as f32 == gx as f32 && ours[1] as f32 == gy as f32 {
                pos_exact += 1;
            }
            if err > worst_pos.0 {
                worst_pos = (err, format!("{account}[{i}] ours={ours:?} ei=[{gx}, {gy}]"));
            }
        }
        if outliers > 0 {
            per_player_outliers.push((account.clone(), outliers, gpos.len()));
        }

        // -- orientations --
        let gang = crd["orientations"].as_array().expect("orientations");
        assert_eq!(gang.len(), tr.orientations.len(), "{account}: orientation sample count");
        for (i, (ours, theirs)) in tr.orientations.iter().zip(gang).enumerate() {
            let g = theirs.as_f64().unwrap();
            let d = angle_diff(*ours, g);
            ang_total += 1;
            if d <= ANGLE_TOLERANCE_DEG {
                ang_within += 1;
            }
            if *ours as f32 == g as f32 {
                ang_exact += 1;
            }
            if d > worst_ang.0 {
                worst_ang = (d, format!("{account}[{i}] ours={ours} ei={g}"));
            }
        }
    }

    assert!(players_checked >= 40, "expected to join ~44 squad players, got {players_checked}");

    let pos_frac = pos_within as f64 / pos_total as f64;
    let ang_frac = ang_within as f64 / ang_total as f64;
    println!(
        "EI_REPLAY CALIBRATION (postrework, Blue Alpine Borderlands, {log_duration}ms):\n\
         \x20 players_checked={players_checked} (start/end/dc/down/dead EXACT for all)\n\
         \x20 positions: {pos_within}/{pos_total} ({:.4}%) within {PIXEL_TOLERANCE}px, \
         {pos_exact}/{pos_total} ({:.4}%) f32-EXACT, worst {:.3e}px @ {}\n\
         \x20 orientations: {ang_within}/{ang_total} ({:.4}%) within {ANGLE_TOLERANCE_DEG}deg, \
         {ang_exact}/{ang_total} ({:.4}%) f32-EXACT, worst {:.3e}deg @ {}\n\
         \x20 per-player position outliers: {:?}",
        pos_frac * 100.0,
        pos_exact as f64 / pos_total as f64 * 100.0,
        worst_pos.0,
        worst_pos.1,
        ang_frac * 100.0,
        ang_exact as f64 / ang_total as f64 * 100.0,
        worst_ang.0,
        worst_ang.1,
        per_player_outliers,
    );

    assert!(
        pos_frac >= MIN_POSITION_WITHIN_FRACTION,
        "positions gate: {pos_within}/{pos_total} ({:.4}%) within {PIXEL_TOLERANCE}px, \
         need >={:.2}%",
        pos_frac * 100.0,
        MIN_POSITION_WITHIN_FRACTION * 100.0
    );
    assert!(
        ang_frac >= MIN_ANGLE_WITHIN_FRACTION,
        "orientations gate: {ang_within}/{ang_total} ({:.4}%) within {ANGLE_TOLERANCE_DEG}deg, \
         need >={:.2}%",
        ang_frac * 100.0,
        MIN_ANGLE_WITHIN_FRACTION * 100.0
    );

    // The engine is a faithful transcription of GW2EI's own arithmetic, so
    // in practice it reproduces the export BIT-for-bit. Gate that too, but
    // one nine below observed (positions are pure IEEE lerp/divide and are
    // reproducible everywhere; orientations route through the platform's
    // `atan2`, which is permitted to differ by an ULP).
    assert!(
        pos_exact as f64 / pos_total as f64 >= 0.999,
        "positions f32-exactness regressed: {pos_exact}/{pos_total}"
    );
    assert!(
        ang_exact as f64 / ang_total as f64 >= 0.999,
        "orientations f32-exactness regressed: {ang_exact}/{ang_total}"
    );
}

/// `combatReplayData.{dc,down,dead}` -> `Vec<[i64; 2]>` (absent/null => empty).
fn intervals(v: Option<&serde_json::Value>) -> Vec<[i64; 2]> {
    v.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|iv| [iv[0].as_i64().expect("interval start"), iv[1].as_i64().expect("interval end")])
                .collect()
        })
        .unwrap_or_default()
}

/// Pre-rework era sanity on the COMMITTED anonymized fixture: the engine
/// must produce plausible fixed-rate tracks there too (this fixture has no
/// GW2EI `combatReplayData` export to diff against, so the checks are
/// structural + "the tracks actually move").
#[test]
fn ei_replay_produces_plausible_tracks_on_the_committed_prerework_fixture() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH).expect("committed anon fixture must exist");
    let raw = decode_raw(&bytes).expect("decode anon fixture");
    let enc = resolve(&raw);
    let log_duration = enc.duration_ms as i64;

    let map = MapTransform::from_log(&raw).expect("Green Alpine Borderlands (mapID 95)");
    assert_eq!(map.sizes(), [523, 750]);

    let tracks = build_ei_replay(&raw, &enc, &map);
    assert!(!tracks.is_empty(), "the fixture has players");

    let mut moving = 0usize;
    let mut with_orientations = 0usize;
    for tr in &tracks {
        assert_track_is_structurally_sound(tr, log_duration, "prerework-anon");
        assert!(!tr.positions.is_empty(), "{}: every player gets a track (forcePolling)", tr.name);
        // Every sample must be inside (a generous margin around) the map
        // image -- catches a botched transform or a leaked `forcePolling`
        // sentinel.
        for xy in &tr.positions {
            assert!(
                (-50.0..=573.0).contains(&xy[0]) && (-50.0..=800.0).contains(&xy[1]),
                "{}: position {xy:?} outside the 523x750 map image",
                tr.name
            );
        }
        if tr.positions.iter().any(|p| *p != tr.positions[0]) {
            moving += 1;
        }
        if !tr.orientations.is_empty() {
            with_orientations += 1;
        }
    }
    println!(
        "EI_REPLAY SANITY (prerework anon, {} tracks, {log_duration}ms): \
         {moving} tracks actually move, {with_orientations} carry orientations",
        tracks.len()
    );
    assert!(moving > 0, "at least some players moved during the fight");
    assert!(with_orientations > 0, "the pre-rework era emits CBTS_FACING too");
}
