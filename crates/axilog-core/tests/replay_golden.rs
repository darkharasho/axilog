//! Combat replay position calibration vs GW2EI's exported
//! `combatReplayData` (M9 Task 1).
//!
//! ## World -> map-pixel transform
//!
//! `analysis::replay::build_replay` returns raw world (game-unit)
//! coordinates -- it does not know about any particular map's pixel image,
//! so it works the same way for every WvW map. GW2EI's own exported
//! `players[].combatReplayData.positions`, by contrast, ARE already in
//! map-pixel space. To compare the two, this test derives and applies
//! GW2EI's own world -> map-pixel transform for the ONE map both golden
//! fixtures happen to be on (Alpine Borderlands, mapID 95 "Green"/96
//! "Blue" -- both share the identical rect/pixel-size constants below, only
//! their decoration color/continent-rect differ, which this transform
//! never touches).
//!
//! Source: GW2EI's `WvWLogic.GetCombatMapInternal`
//! (`GW2EIEvtcParser/LogLogic/WvW/WvWLogic.cs`):
//! ```csharp
//! case GreenAlpineBorderland:
//!     crMap = new CombatReplayMap((697, 1000),
//!         (-30720, -43008, 30720, 43008), ...);
//! case BlueAlpineBorderland:
//!     crMap = new CombatReplayMap((697, 1000),
//!         (-30720, -43008, 30720, 43008), ...); // same pixel size + rect
//! ```
//! i.e. `pixelSize = (697, 1000)`, `rectInMap = (topX: -30720, topY:
//! -43008, bottomX: 30720, bottomY: 43008)` (real/world-unit bounds the
//! map image covers). `CombatReplayMap.GetPixelMapSize()` then rescales
//! that pixel size to a max dimension of 750 preserving aspect ratio:
//! `ratio = 697/1000 = 0.697 < 1.0` => `(round(0.697*750), 750) = (523,
//! 750)` -- exactly this log's `combatReplayMetaData.sizes` (`[523, 750]`)
//! and `inchToPixel` (`round(1 / ((61440/523)), 3) = 0.009`), both
//! independently confirmed against the real exported JSON before trusting
//! this transform (see `crates/axilog-core/src/analysis/replay.rs`'s
//! module doc for the calibration numbers this produced).
//!
//! `CombatReplayMap.GetMapCoordRounded` (`CombatReplayMap.cs`):
//! ```csharp
//! double x = (realX - rectInMap.topX) / (rectInMap.bottomX - rectInMap.topX);
//! double y = (realY - rectInMap.topY) / (rectInMap.bottomY - rectInMap.topY);
//! pixelX = round(scaleX * pixelSize.width * x, 3);   // scaleX*pixelSize.width == outputWidth
//! pixelY = round(scaleY * (pixelSize.height - pixelSize.height * y), 3); // == outputHeight*(1-y)
//! ```
//! Simplifies (since `scaleX * _pixelSize.width == outputWidth` and
//! likewise for Y) to the `world_to_map_pixel` function below.
//!
//! MapID confirmed by decoding this fixture's own `CBTS_MAPID` event
//! (`src_agent == 95`) and cross-checked against the golden JSON's
//! top-level `"mapID": 95` / `"fightName": "World vs World - Green Alpine
//! Borderlands"`.
//!
//! ## Time alignment
//!
//! GW2EI polls onto a grid of `poll_ms` (300) multiples, anchored to each
//! agent's own "first aware" time (the earliest event of ANY kind for that
//! agent, per the arcdps EVTC reference's agent-table processing recipe --
//! NOT that agent's first `CBTS_POSITION` event specifically, which is
//! usually a little later). `analysis::replay::build_replay`'s
//! `downsample` mirrors this exactly (see that module's `build_track` doc
//! comment for the real-fixture finding that drove it: account
//! `harasho.4281`'s first-ever event is at t=0 but its first position
//! event is at t=23 -- GW2EI's own exported `start` is `0`, matching
//! "first aware", not `ceil(23/300)*300=300`). Empirically confirmed exact
//! (both fixtures, multiple players, index-by-index) by the ad-hoc probe
//! used to derive this test.
//!
//! ## Calibration result
//!
//! PRIMARY fixture (`wvw-small.anon.zevtc` + this file's golden JSON,
//! Green Alpine Borderlands): 3 full tracks (165 samples each) plus 37
//! players' first/last-sample spot checks, 568/568 (100.00%) of all
//! checked samples within 1.0px.
//!
//! SECONDARY fixture (`fixtures/local/wvw-postrework.zevtc` +
//! `.ei.json`, Blue Alpine Borderlands, real build-20260718 capture, local
//! only): 44 full tracks (up to ~1159 samples each, ~5m48s fight),
//! 50737/50855 (99.77%) of all checked samples within 1.0px (a handful of
//! samples land just over the line during the fastest movement bursts --
//! this project's linear interpolation vs. GW2EI's own velocity-aware
//! hold/interpolate branch in `HandlePosition`, not a systematic offset).

use axilog_core::analysis::ei_replay::MapTransform;
use axilog_core::analysis::replay::build_replay;
use axilog_core::evtc::{anon_account, decode_raw};
use axilog_core::model::resolve;

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const GOLDEN_JSON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");
const LOCAL_POSTREWORK_ZEVTC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-postrework.zevtc"
);
const LOCAL_POSTREWORK_JSON: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/local/wvw-postrework.ei.json"
);

/// GW2EI's Alpine Borderland (Green mapID=95 / Blue mapID=96)
/// combat-replay geometry, per this file's module doc citation.
///
/// M15 Task 2: these four rect components and the two output dimensions
/// used to be spelled out as literals in this file, a second copy of the
/// same GW2EI constants that `MapTransform::for_map_id` carries. They are
/// now read from `wvw::maps::WVW_MAPS` through that constructor -- the
/// project's single source of truth for map geometry -- so the native
/// replay's calibration and the EI-shape engine can no longer be calibrated
/// against different rects. `alpine_geometry_matches_the_historical_literals`
/// below pins the exact values this file used to hard-code, so the
/// unification is provably behaviour-preserving.
///
/// Both fixtures are Alpine Borderlands (the committed one is Green /
/// mapID 95, the local post-rework one is Blue / mapID 96); GW2EI gives
/// the two ids identical `_pixelSize`/`_rectInMap`, which
/// `alpine_geometry_matches_the_historical_literals` also asserts.
fn alpine_transform() -> MapTransform {
    MapTransform::for_map_id(95).expect("Green Alpine Borderlands is in the WvW map table")
}

/// GW2EI's `CombatReplayMap.GetMapCoordRounded`, specialized to the Alpine
/// Borderland geometry above (see this file's module doc for the
/// derivation). Kept as this file's own un-rounded simplification rather
/// than calling `MapTransform::to_map_pixel`, so that the native replay
/// stays measured against the plain algebra it was calibrated with; the
/// geometry it reads is now shared, which is what Task 2 required.
fn world_to_map_pixel(x: f32, y: f32) -> (f64, f64) {
    let m = alpine_transform();
    let nx = (x as f64 - m.top_x) / (m.bottom_x - m.top_x);
    let ny = (y as f64 - m.top_y) / (m.bottom_y - m.top_y);
    let [out_w, out_h] = m.sizes();
    (out_w as f64 * nx, out_h as f64 * (1.0 - ny))
}

/// Pins the literals this file carried before M15 Task 2 unified the map
/// geometry into `wvw::maps::WVW_MAPS`. If the shared table ever drifts,
/// this fails here rather than silently re-baselining the native replay's
/// calibration numbers quoted in this file's module doc.
#[test]
fn alpine_geometry_matches_the_historical_literals() {
    let m = alpine_transform();
    assert_eq!(m.top_x, -30720.0);
    assert_eq!(m.top_y, -43008.0);
    assert_eq!(m.bottom_x, 30720.0);
    assert_eq!(m.bottom_y, 43008.0);
    // `CombatReplayMap.GetPixelMapSize()` for `pixelSize = (697, 1000)`:
    // `ratio = 0.697 < 1.0` => `(round(0.697*750), 750)`.
    assert_eq!(m.sizes(), [523, 750]);
    assert_eq!(
        Some(m),
        MapTransform::for_map_id(96),
        "Green and Blue Alpine share one arena image and one rect"
    );
}

const PIXEL_TOLERANCE: f64 = 1.0;
/// Fraction of samples that must be within `PIXEL_TOLERANCE` px, per the
/// Task 1 brief's calibration gate.
const MIN_WITHIN_TOLERANCE_FRACTION: f64 = 0.95;

fn pixel_err(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn read_json(path: &str) -> serde_json::Value {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse JSON {path}: {e}"))
}

/// Checks one player's downsampled, transformed track against a golden
/// `combatReplay` entry (`{start, count, first, last}` always;
/// `positions` for the 3 full tracks). Returns
/// `(within_tolerance_count, total_checked)`.
fn check_track(
    our_samples: &[axilog_core::analysis::replay::Sample],
    golden: &serde_json::Value,
) -> (usize, usize) {
    let mut within = 0;
    let mut total = 0;

    let golden_first = golden["first"].as_array().expect("first");
    let golden_last = golden["last"].as_array().expect("last");
    let ei_first = (golden_first[0].as_f64().unwrap(), golden_first[1].as_f64().unwrap());
    let ei_last = (golden_last[0].as_f64().unwrap(), golden_last[1].as_f64().unwrap());

    assert!(!our_samples.is_empty(), "expected at least one sample for a matched player");
    let our_first = world_to_map_pixel(our_samples[0].x, our_samples[0].y);
    total += 1;
    if pixel_err(our_first, ei_first) <= PIXEL_TOLERANCE {
        within += 1;
    }

    // "last" spot check: compare our own last sample against EI's last
    // sample IF our track reaches at least as far as EI's own last
    // timestamp; `build_replay` deliberately stops at the track's own last
    // *observed* position rather than holding to log end (see
    // `analysis::replay`'s module doc), so a player active to the very end
    // of the fight will have a last sample close to EI's; skip the "last"
    // check entirely when our track is clearly shorter (that's an
    // intentional design difference documented above, not a decode bug).
    let our_last_t = our_samples.last().unwrap().t_ms;
    let golden_count = golden["count"].as_u64().unwrap_or(0);
    let golden_start = golden["start"].as_i64().unwrap_or(0).max(0) as u64;
    let ei_last_t = golden_start + golden_count.saturating_sub(1) * 300;
    if our_last_t + 300 >= ei_last_t {
        let our_last = world_to_map_pixel(our_samples.last().unwrap().x, our_samples.last().unwrap().y);
        total += 1;
        if pixel_err(our_last, ei_last) <= PIXEL_TOLERANCE {
            within += 1;
        }
    }

    // Full track, index-aligned (both grids start at the same `ceil(first
    // observed / 300) * 300` multiple -- see module doc).
    if let Some(positions) = golden.get("positions").and_then(|v| v.as_array()) {
        for (idx, p) in positions.iter().enumerate() {
            let Some(sample) = our_samples.get(idx) else { break };
            let ex = p[0].as_f64().unwrap();
            let ey = p[1].as_f64().unwrap();
            let ours = world_to_map_pixel(sample.x, sample.y);
            total += 1;
            if pixel_err(ours, (ex, ey)) <= PIXEL_TOLERANCE {
                within += 1;
            }
        }
    }

    (within, total)
}

#[test]
fn replay_calibrated_against_ei_combat_replay_data() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_json(GOLDEN_JSON_PATH);
    let golden_players = golden["players"].as_array().expect("players array");

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let replay = build_replay(&raw, &enc, 300);
    assert_eq!(replay.poll_ms, 300);

    // Join: raw agent-table index -> `Anon<N>.<4 digits>` (deterministic,
    // matches this committed anon fixture) -> golden player row -- same
    // join `professions_match_ei_golden` (`golden.rs`) already uses.
    let addr_to_index: std::collections::HashMap<u64, usize> =
        raw.agents.iter().enumerate().map(|(i, a)| (a.addr, i)).collect();

    let mut total_within = 0usize;
    let mut total_checked = 0usize;
    let mut full_tracks_checked = 0usize;
    let mut players_checked = 0usize;

    for track in &replay.tracks {
        if !track.is_squad || track.samples.is_empty() {
            continue;
        }
        let Some(&idx) = addr_to_index.get(&track.agent_addr) else { continue };
        let expected_account = anon_account(idx);
        let key = expected_account.trim_start_matches(':');
        let Some(gp) = golden_players.iter().find(|p| p["account"].as_str() == Some(key)) else {
            continue;
        };
        let Some(cr) = gp.get("combatReplay") else { continue };

        let (within, total) = check_track(&track.samples, cr);
        total_within += within;
        total_checked += total;
        players_checked += 1;
        if cr.get("positions").is_some() {
            full_tracks_checked += 1;
        }
    }

    println!(
        "replay_golden (primary, Green Alpine Borderlands): players_checked={players_checked} \
         full_tracks_checked={full_tracks_checked} within_1px={total_within}/{total_checked} \
         ({:.2}%)",
        100.0 * total_within as f64 / total_checked.max(1) as f64
    );

    assert!(players_checked >= 30, "expected at least 30 matched players, got {players_checked}");
    assert!(full_tracks_checked >= 3, "expected at least 3 full tracks, got {full_tracks_checked}");
    assert!(
        (total_within as f64) >= MIN_WITHIN_TOLERANCE_FRACTION * (total_checked as f64),
        "only {total_within}/{total_checked} samples within {PIXEL_TOLERANCE}px \
         ({:.2}%), need >= {:.0}%",
        100.0 * total_within as f64 / total_checked.max(1) as f64,
        100.0 * MIN_WITHIN_TOLERANCE_FRACTION
    );
}

/// SECONDARY golden source (local-only, gitignored): a real, un-anonymized
/// post-rework build-20260718 capture with its own dps.report-shaped
/// `combatReplayData`. Skips gracefully when either local file is absent
/// (dev-only extra, mirrors `postrework_golden.rs`'s pattern) -- nothing
/// from these files is extracted into any committed fixture.
#[test]
fn replay_calibrated_against_ei_combat_replay_data_local_postrework_when_present() {
    let bytes = match std::fs::read(LOCAL_POSTREWORK_ZEVTC) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {LOCAL_POSTREWORK_ZEVTC} absent (local-only extra check)");
            return;
        }
    };
    let golden = match std::fs::read_to_string(LOCAL_POSTREWORK_JSON) {
        Ok(s) => serde_json::from_str::<serde_json::Value>(&s).expect("parse local golden JSON"),
        Err(_) => {
            println!("skip: {LOCAL_POSTREWORK_JSON} absent (local-only extra check)");
            return;
        }
    };
    let golden_players = golden["players"].as_array().expect("players array");
    assert_eq!(golden["mapID"].as_i64(), Some(96), "expected Blue Alpine Borderlands (mapID 96)");

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let replay = build_replay(&raw, &enc, 300);

    let addr_to_account: std::collections::HashMap<u64, String> = raw
        .agents
        .iter()
        .filter(|a| a.is_player())
        .map(|a| {
            let (_, account, _) = a.name_parts();
            (a.addr, account.trim_start_matches(':').to_string())
        })
        .collect();

    let mut total_within = 0usize;
    let mut total_checked = 0usize;
    let mut full_tracks_checked = 0usize;
    let mut players_checked = 0usize;

    for track in &replay.tracks {
        if track.samples.is_empty() {
            continue;
        }
        let Some(account) = addr_to_account.get(&track.agent_addr) else { continue };
        let Some(gp) = golden_players.iter().find(|p| p["account"].as_str() == Some(account.as_str())) else {
            continue;
        };
        let Some(crd) = gp.get("combatReplayData") else { continue };
        let positions = match crd.get("positions").and_then(|v| v.as_array()) {
            Some(p) if !p.is_empty() => p,
            _ => continue,
        };
        // Build a `{start, count, first, last}` view matching `check_track`'s
        // expected shape, reusing the same function as the primary test.
        let synthetic = serde_json::json!({
            "start": crd["start"],
            "count": positions.len(),
            "first": positions[0],
            "last": positions[positions.len() - 1],
            "positions": positions,
        });
        let (within, total) = check_track(&track.samples, &synthetic);
        total_within += within;
        total_checked += total;
        players_checked += 1;
        full_tracks_checked += 1; // every matched player here has a full `positions` array
    }

    println!(
        "replay_golden (secondary, local postrework, Blue Alpine Borderlands): \
         players_checked={players_checked} full_tracks_checked={full_tracks_checked} \
         within_1px={total_within}/{total_checked} ({:.2}%)",
        100.0 * total_within as f64 / total_checked.max(1) as f64
    );

    assert!(players_checked >= 3, "expected at least 3 matched players, got {players_checked}");
    assert!(
        (total_within as f64) >= MIN_WITHIN_TOLERANCE_FRACTION * (total_checked as f64),
        "only {total_within}/{total_checked} samples within {PIXEL_TOLERANCE}px \
         ({:.2}%), need >= {:.0}%",
        100.0 * total_within as f64 / total_checked.max(1) as f64,
        100.0 * MIN_WITHIN_TOLERANCE_FRACTION
    );
}
