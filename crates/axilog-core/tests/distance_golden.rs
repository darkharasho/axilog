//! Calibration of `analysis::distance` against GW2EI's own `distToCom` /
//! `stackDist` (Phase B Task 7).
//!
//! ## Why the exactness gate does not run on a decoded track
//!
//! The task's bar is exact agreement, not a tolerance. Running the
//! reduction end to end -- decode the `.zevtc`, downsample positions,
//! reduce -- cannot meet that bar and cannot even *diagnose* a failure
//! against it, because two independent things sit between the raw log and
//! the number: this project's own polling/interpolation
//! (`analysis::replay`, calibrated to within one map pixel of GW2EI's, and
//! deliberately trimmed at each track's last observed position rather than
//! held to log end) and the reduction under test. A 2% end-to-end
//! disagreement says nothing about which of the two is wrong.
//!
//! So the gate is split:
//!
//! - [`distance_semantics_match_ei_exactly`] feeds the reduction **GW2EI's
//!   own exported positions** -- `players[].combatReplayData.positions`,
//!   already committed in this fixture in GW2EI map-pixel space -- inverted
//!   back through the Alpine Borderlands transform, along with GW2EI's own
//!   `down`/`dead`/`dc` windows. Everything but the reduction is then
//!   GW2EI's, so any disagreement is a semantic one. Residual: **0.0104 /
//!   0.0073 world inches, worst case over 41 actors**, which is under the
//!   floor set by the fixture's positions being rounded to three decimal
//!   places in map-pixel space (one such unit is ~0.117 inches on the x
//!   axis; `PIXEL_ROUNDING_FLOOR` below asserts against `0.02`, roughly 2x
//!   that measured residual, deliberately with headroom rather than tuned
//!   to it).
//!
//!   This gate does not certify all eight semantics equally. Directly, on
//!   this fixture, it certifies three: mean-not-median, the squad-centre
//!   `sampleCount - 1` poll cap, and the squad centre's active-vs-raw
//!   split against the commander reference -- each moves this number by
//!   0.4 to 103 inches when dropped, far above the floor. A fourth, the
//!   `activePlayers` participation filter, is certified by the end-to-end
//!   test instead (its effect here is a 262% swing, not visible against
//!   this exact-position gate at all, because the one non-participating
//!   player in the fixture never has a `down`/`dead`/`dc` window that this
//!   test's inputs would expose). The remaining rules -- the active-poll
//!   boundary/merge rule, the commander-overlap resolution, the `dc`
//!   contribution, and the float widths -- move this number by exactly
//!   `0.000000` on this fixture, because it does not exercise the cases
//!   they govern (only 2 of 41 players have a `down` window, none have
//!   `dead`, and the fixture's `dc` arrays hold only GW2EI's clamped
//!   pre-spawn/post-despawn bookends). Those rules rest on their own unit
//!   tests in `analysis::distance` and on the GW2EI source citations there,
//!   not on this gate -- each unit test has been independently verified to
//!   fail under the mutation it exists to catch.
//!
//! - [`distance_from_a_decoded_log_beats_the_side_channel_derivation`] runs
//!   the whole pipeline from the committed `.zevtc` and pins the end-to-end
//!   error, which is dominated by the replay pipeline rather than by this
//!   reduction. It exists to catch a regression in the join, not to certify
//!   the arithmetic.
//!
//! ## What the end-to-end number is, and what it is made of
//!
//! axibridge's `deriveDistanceScalars` -- the side-channel derivation this
//! task replaces -- measured **3.7% / 4.3%** mean error against these same
//! EI values (`axibridge:docs/axilog-cutover-report.md` §5). Engine-side
//! the same fixture gives **1.69% / 2.22%**, and that remainder is not the
//! reduction: it is `analysis::replay` disagreeing with GW2EI's polled
//! positions. One player accounts for most of it on its own -- their raw
//! position events stop at t=44,100 while GW2EI holds their last known
//! position to log end, so GW2EI averages over 17 polls this project does
//! not have, and that single row lands 35% off while the median row is
//! under 1%.

use axilog_core::analysis::distance::{self, DistanceScalars};
use axilog_core::analysis::ei_replay::MapTransform;
use axilog_core::analysis::replay::{build_replay, Interval, Replay, Sample, Track};
use axilog_core::evtc::{anon_account, decode_raw, iff, RawEvent, RawHeader, RawLog};
use axilog_core::model::{CommanderTag, Encounter, Player};

const ANON_FIXTURE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
const GOLDEN_JSON_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.ei.json");

/// This log's own `LogEnd`, from the fixture's `combatReplayMetaData`
/// interval -- the bound GW2EI sizes the squad-centre reference with.
const LOG_END_MS: u64 = 49285;
const POLL_MS: u64 = 300;

/// Worst-case world-inch disagreement the fixture's 3-decimal map-pixel
/// rounding can produce, with headroom -- this constant is `0.02`, roughly
/// 2x the measured worst case below, not a value tuned to match it. One
/// pixel unit at the third decimal is 61440/523/1000 = 0.117 inches on x
/// and 86016/750/1000 = 0.115 on y, which is the per-coordinate half-unit;
/// a single two-point separation carries up to ~2x that, ~0.16 inches, of
/// worst-case error, and averaged over a 164-poll track the measured worst
/// case is 0.0104. Independently re-derived from `wvw::maps.rs:153` +
/// `analysis::ei_replay.rs:582` rather than from this test's own
/// arithmetic, that bound comes out to ~0.013 inches -- bracketing the
/// measured 0.0104 and confirming the floor is real, not an artefact of
/// how this file computed it. This is NOT a semantic tolerance -- see this
/// file's module doc.
const PIXEL_ROUNDING_FLOOR: f64 = 0.02;

fn read_golden() -> serde_json::Value {
    let s = std::fs::read_to_string(GOLDEN_JSON_PATH)
        .unwrap_or_else(|e| panic!("read {GOLDEN_JSON_PATH}: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("parse {GOLDEN_JSON_PATH}: {e}"))
}

/// The inverse of `replay_golden.rs`'s `world_to_map_pixel`, over the same
/// shared `wvw::maps` geometry, so the two files cannot drift onto
/// different rects.
fn map_pixel_to_world(px: f64, py: f64) -> (f32, f32) {
    let m = MapTransform::for_map_id(95).expect("Green Alpine Borderlands is in the WvW map table");
    let [out_w, out_h] = m.sizes();
    let x = px / out_w as f64 * (m.bottom_x - m.top_x) + m.top_x;
    let y = (1.0 - py / out_h as f64) * (m.bottom_y - m.top_y) + m.top_y;
    (x as f32, y as f32)
}

fn intervals(v: &serde_json::Value) -> Vec<Interval> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let (s, e) = (p[0].as_i64()?, p[1].as_i64()?);
                    // GW2EI brackets `dc` with `[long.MinValue, FirstAware]`
                    // and `[LastAware, long.MaxValue]` sentinels. Clamping
                    // them into the log's own range keeps their real
                    // endpoint (the only end that can touch a poll) without
                    // carrying a saturating value into unsigned arithmetic.
                    let start = s.max(0) as u64;
                    let end = e.clamp(0, LOG_END_MS as i64 * 2) as u64;
                    (start < end).then_some(Interval { start_ms: start, end_ms: end })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A log in which every listed agent dealt damage to a foe, so GW2EI's
/// participation gate (`analysis::distance::participants`) keeps them all.
/// The real gate is exercised end to end by the second test; here the
/// roster is GW2EI's own `players[]`, which has already had that filter
/// applied to it.
fn raw_with_participants(addrs: &[u64]) -> RawLog {
    let events = addrs
        .iter()
        .map(|&src| RawEvent {
            time: 0,
            src_agent: src,
            dst_agent: 1,
            value: 1,
            buff_dmg: 0,
            overstack: 0,
            skillid: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: iff::FOE,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0,
            is_fifty: 0,
            is_moving: 0,
            is_statechange: 0,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        })
        .collect();
    RawLog {
        header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
        agents: vec![],
        skills: vec![],
        events,
        guid_map: vec![],
    }
}

#[test]
fn distance_semantics_match_ei_exactly() {
    let golden = read_golden();
    let players_json = golden["players"].as_array().expect("players array");
    let scalars_json = golden["distanceScalars"].as_array().expect("distanceScalars array");
    assert_eq!(players_json.len(), scalars_json.len(), "positionally joined");

    let mut tracks = Vec::new();
    let mut players = Vec::new();
    for (i, p) in players_json.iter().enumerate() {
        let addr = 1000 + i as u64;
        let cr = &p["combatReplayData"];
        // GW2EI polls onto multiples of the polling rate and then trims to
        // the actor's own aware window, so `positions[k]` sits at the k-th
        // multiple at or after `start` (`CombatReplay.PollingRate`/`Trim`).
        // The same rule `analysis::replay::downsample` anchors its grid with.
        let grid0 = (cr["start"].as_i64().expect("start").max(0) as u64).div_ceil(POLL_MS) * POLL_MS;
        let samples: Vec<Sample> = cr["positions"]
            .as_array()
            .expect("positions")
            .iter()
            .enumerate()
            .map(|(k, q)| {
                let (x, y) = map_pixel_to_world(
                    q[0].as_f64().expect("px"),
                    q[1].as_f64().expect("py"),
                );
                Sample { t_ms: grid0 + POLL_MS * k as u64, x, y, z: 0.0 }
            })
            .collect();
        assert!(!samples.is_empty(), "every fixture player has an exported track");

        let row = &scalars_json[i];
        let not_in_squad = row["notInSquad"].as_bool().expect("notInSquad");
        // The one commander in this log holds the tag from their first
        // marker event to the log's end; `wvw::markers` resolves the same
        // window from the raw stream, and the end-to-end test below reads
        // it from there rather than from this literal.
        let commander_tag = row["hasCommanderTag"].as_bool().expect("hasCommanderTag").then(|| {
            CommanderTag { variant: "purple-commander".into(), guid: "g".into(), segments: vec![(103, LOG_END_MS)] }
        });
        tracks.push(Track {
            agent_addr: addr,
            name: String::new(),
            team: "green".into(),
            commander: commander_tag.is_some(),
            is_squad: true,
            samples,
            down_intervals: intervals(&cr["down"]),
            dead_intervals: intervals(&cr["dead"]),
            dc_intervals: intervals(&cr["dc"]),
        });
        players.push(Player {
            agent_addr: addr,
            account: format!("a{i}"),
            // `players[]` is already in GW2EI's own `PlayerList` order
            // (`OrderBy(Group)` over a character-name-ordered list), so the
            // row index reproduces that order for the float summation --
            // see `distance::squad_tracks`.
            character: String::new(),
            profession: String::new(),
            elite_spec: String::new(),
            team: "green".into(),
            subgroup: i as u8,
            in_squad: !not_in_squad,
            commander: commander_tag.is_some(),
            marker: None,
            commander_tag,
            guild_id: None,
            agent_addrs: vec![addr],
        });
    }

    let addrs: Vec<u64> = players.iter().map(|p| p.agent_addr).collect();
    let raw = raw_with_participants(&addrs);
    let enc = Encounter {
        kind: "wvw".into(), pve: None,
        map: String::new(),
        duration_ms: LOG_END_MS,
        build: String::new(),
        revision: 1,
        recorded_by: None,
        teams: vec![],
        players,
        enemies: vec![],
        markers: vec![], ground_markers: vec![],
        tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
    };
    let replay = Replay { poll_ms: POLL_MS, tracks, distance: Default::default() };
    let out = distance::build(&raw, &replay, &enc);

    let mut worst_com: f64 = 0.0;
    let mut worst_stack: f64 = 0.0;
    for (i, row) in scalars_json.iter().enumerate() {
        let ours = out[&(1000 + i as u64)];
        let ei_com = row["distToCom"].as_f64().expect("distToCom");
        let ei_stack = row["stackDist"].as_f64().expect("stackDist");
        worst_com = worst_com.max((ours.dist_to_com - ei_com).abs());
        worst_stack = worst_stack.max((ours.stack_dist - ei_stack).abs());
    }
    println!(
        "distance_golden (semantics, EI's own positions): {} actors, worst abs \
         distToCom={worst_com:.6} stackDist={worst_stack:.6} inches",
        scalars_json.len()
    );
    assert!(
        worst_com <= PIXEL_ROUNDING_FLOOR,
        "distToCom off by {worst_com} inches, above the {PIXEL_ROUNDING_FLOOR} pixel-rounding floor \
         -- a semantic has drifted, do not widen this"
    );
    assert!(
        worst_stack <= PIXEL_ROUNDING_FLOOR,
        "stackDist off by {worst_stack} inches, above the {PIXEL_ROUNDING_FLOOR} pixel-rounding floor \
         -- a semantic has drifted, do not widen this"
    );

    // The commander's own distance to the commander is the one value in
    // this statistic that no position pipeline can perturb, so it is
    // asserted exactly rather than against a floor.
    let commander = scalars_json
        .iter()
        .position(|r| r["hasCommanderTag"].as_bool() == Some(true))
        .expect("this fixture has exactly one commander");
    assert_eq!(out[&(1000 + commander as u64)].dist_to_com, 0.0);
}

/// End-to-end from the committed `.zevtc`. Pins the join and the
/// improvement over the side-channel derivation this task replaces; the
/// arithmetic itself is certified by the test above.
#[test]
fn distance_from_a_decoded_log_beats_the_side_channel_derivation() {
    let bytes = std::fs::read(ANON_FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {ANON_FIXTURE_PATH}: {e}"));
    let golden = read_golden();
    let players_json = golden["players"].as_array().expect("players array");
    let scalars_json = golden["distanceScalars"].as_array().expect("distanceScalars array");

    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = axilog_core::model::resolve(&raw);
    let replay = build_replay(&raw, &enc, POLL_MS);

    // Same join every other golden test in this crate uses: raw agent-table
    // index -> `Anon<N>.<4 digits>` -> golden row.
    let addr_to_index: std::collections::HashMap<u64, usize> =
        raw.agents.iter().enumerate().map(|(i, a)| (a.addr, i)).collect();

    let (mut sum_com, mut sum_stack, mut matched) = (0.0f64, 0.0f64, 0usize);
    let mut commander_rows = 0usize;
    for track in &replay.tracks {
        let Some(&idx) = addr_to_index.get(&track.agent_addr) else { continue };
        let account = anon_account(idx);
        let key = account.trim_start_matches(':');
        let Some(row) = players_json.iter().position(|p| p["account"].as_str() == Some(key)) else {
            continue;
        };
        let ours: DistanceScalars = replay.distance[&track.agent_addr];
        let ei_com = scalars_json[row]["distToCom"].as_f64().expect("distToCom");
        let ei_stack = scalars_json[row]["stackDist"].as_f64().expect("stackDist");
        if scalars_json[row]["hasCommanderTag"].as_bool() == Some(true) {
            commander_rows += 1;
            // Exact, and independent of the position pipeline: the
            // commander is their own reference at every poll they are
            // active for. `-1` here would mean the commander segments
            // failed to resolve at all.
            assert_eq!(ours.dist_to_com, 0.0, "the commander's own distToCom");
        }
        if ei_com > 0.0 {
            sum_com += (ours.dist_to_com - ei_com).abs() / ei_com;
            sum_stack += (ours.stack_dist - ei_stack).abs() / ei_stack;
            matched += 1;
        }
    }

    assert!(matched >= 30, "expected at least 30 joined players, got {matched}");
    assert_eq!(commander_rows, 1, "this fixture has exactly one commander");
    let mean_com = 100.0 * sum_com / matched as f64;
    let mean_stack = 100.0 * sum_stack / matched as f64;
    println!(
        "distance_golden (end to end): {matched} players, mean rel error \
         distToCom={mean_com:.4}% stackDist={mean_stack:.4}% \
         (axibridge's deriveDistanceScalars: 3.7% / 4.3%)"
    );
    // Bounds, not targets: the residual is `analysis::replay`'s, so an
    // improvement there should tighten these rather than be absorbed.
    assert!(mean_com < 2.5, "distToCom mean error {mean_com}% regressed");
    assert!(mean_stack < 3.0, "stackDist mean error {mean_stack}% regressed");
}
