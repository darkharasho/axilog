//! Side-channel absorption Task 11: the activity pass -> the ALWAYS-ON half
//! of `blocks.replay`.
//!
//! Every absorption before this one moved a pass that rides a flag, so
//! "present" and "the flag was on" meant the same thing and one presence
//! check settled both. This block is the first where they come apart: the
//! intervals half is computed on every parse and the position half is not,
//! so `blocks.replay` can exist while being half-populated, and
//! `coverage.replay == "present"` is deliberately NOT a statement about
//! positions.
//!
//! These tests pin that seam from both sides -- the half that must survive
//! with the gate off, and the half that must not appear without it -- plus
//! the two roster facts that make the shape what it is: `active_ms`'s
//! subtraction rule, and the track roster being wider than the intervals
//! roster.

mod common;

use axilog_schema::v1::envelope::{BlockName, CoverageState};

/// The task's headline claim. `--replay` buys positions; it does not buy
/// down/dead history, which is there either way.
#[test]
fn the_intervals_survive_without_the_position_gate() {
    let (_enc, _metrics, legacy, v1) = common::fixture_report_no_gates();
    let replay = v1.blocks.replay.as_ref().expect("replay block present with no gates on");
    assert_eq!(
        replay.by_entity.0.len(),
        legacy.players.len(),
        "every player has an intervals row on a document parsed with no optional flags at all"
    );
    assert!(replay.tracks.is_none(), "positions ride --replay and nothing else does");
}

/// The other side of the same seam: the gate really does still gate.
#[test]
fn the_positions_appear_only_under_the_gate() {
    let (_enc, _metrics, legacy, v1) = common::fixture_report_all_gates();
    let replay = v1.blocks.replay.as_ref().expect("replay block present");
    let tracks = replay.tracks.as_ref().expect("positions present under --replay");
    assert!(!tracks.by_entity.0.is_empty(), "the gate is on, so there are tracks");
    assert_eq!(
        replay.by_entity.0.len(),
        legacy.players.len(),
        "turning the position gate ON must not change the intervals roster"
    );
}

/// `coverage.replay` answers the question this block can always answer.
/// Reporting `not_computed` on a no-flags parse would be a lie about data
/// that is right there; reporting `present` is true, and the block's doc
/// comment is where the "present does not mean positions" caveat lives.
#[test]
fn coverage_reports_present_on_a_document_with_no_positions() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_no_gates();
    assert_eq!(
        v1.coverage.get(BlockName::Replay.as_str()),
        Some(CoverageState::Present),
        "intervals exist, so the block is present -- the absent half is `tracks`, not the block"
    );
}

/// The reason `active_ms` is carried rather than left to the reader.
///
/// GW2EI's own active duration subtracts dead time and NOT down time (the
/// citation and the real-export verification are on
/// `ActivityIntervals::active_ms`). A consumer deriving it from the other
/// four fields would have to know that; one who assumed "active means
/// neither downed nor dead" would silently under-report every player who
/// went down. So the field is emitted, and this asserts it means what the
/// source says rather than what the name suggests.
#[test]
fn active_ms_subtracts_dead_time_but_not_down_time() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_no_gates();
    let replay = v1.blocks.replay.as_ref().expect("replay block present");

    let mut checked_with_downs = 0usize;
    for row in replay.by_entity.0.values() {
        let dead_ms: u64 = row.dead.iter().map(|&(s, e)| e.saturating_sub(s)).sum();
        assert_eq!(
            row.active_ms,
            (row.end_ms - row.start_ms).saturating_sub(dead_ms),
            "active_ms is (end - start) - dead, with down time NOT deducted"
        );
        if !row.down.is_empty() {
            let down_ms: u64 = row.down.iter().map(|&(s, e)| e.saturating_sub(s)).sum();
            assert!(down_ms > 0, "a recorded down interval has positive length");
            assert!(
                row.active_ms > (row.end_ms - row.start_ms).saturating_sub(dead_ms + down_ms),
                "this player went down, and their active time did not shrink by it"
            );
            checked_with_downs += 1;
        }
    }
    assert!(
        checked_with_downs > 0,
        "the fixture must contain at least one player who went down, or the assertion above \
         never runs and this test proves nothing"
    );
}

/// Squad intervals appear in two places, and this is why that is safe.
///
/// `ReplayTrack` keeps its own `down_intervals`/`dead_intervals` because
/// the track roster covers enemy players the always-on pass never walks
/// (see the next test). For a SQUAD entity the two are the same
/// `build_intervals` call over the same folded addr set, so they cannot
/// disagree -- and if some future refactor made them able to, this is what
/// would catch it.
#[test]
fn a_squad_intervals_row_agrees_with_that_entitys_own_track() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_all_gates();
    let replay = v1.blocks.replay.as_ref().expect("replay block present");
    let tracks = replay.tracks.as_ref().expect("positions present under --replay");

    let mut compared = 0usize;
    for (id, row) in &replay.by_entity.0 {
        let Some(track) = tracks.by_entity.get(*id) else { continue };
        assert_eq!(row.down, track.down_intervals, "entity {id}: down intervals disagree");
        assert_eq!(row.dead, track.dead_intervals, "entity {id}: dead intervals disagree");
        compared += 1;
    }
    assert!(compared > 0, "the two rosters must overlap on the squad, or nothing was compared");
}

/// The roster asymmetry that shapes the block.
///
/// `replay::build_replay` walks squad players AND enemy-player
/// representatives; `build_activity_intervals` walks squad players only.
/// That is why the intervals stay on `ReplayTrack` too -- dropping them
/// would take every enemy player's down/dead history with them, and it is
/// also why `by_entity` is documented as a squad-only map rather than
/// simply "the replay roster".
#[test]
fn the_track_roster_is_wider_than_the_intervals_roster() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_all_gates();
    let replay = v1.blocks.replay.as_ref().expect("replay block present");
    let tracks = replay.tracks.as_ref().expect("positions present under --replay");

    let only_in_tracks: Vec<u32> = tracks
        .by_entity
        .0
        .keys()
        .copied()
        .filter(|id| !replay.by_entity.0.contains_key(id))
        .collect();
    assert!(
        !only_in_tracks.is_empty(),
        "the WvW fixture has enemy players, so some tracked entity must have no intervals row"
    );
    for id in &only_in_tracks {
        let entity = v1.entities.iter().find(|e| e.id == *id).expect("track keys are entity ids");
        assert!(
            !matches!(
                entity.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            ),
            "entity {id} is on the squad and still has no intervals row -- the always-on pass \
             missed a player"
        );
    }
}

/// The positional guard, which is the only way this pass can corrupt the
/// document rather than merely omit from it.
///
/// `activity` is joined to `report.players` by INDEX. A caller handing over
/// a slice of the wrong length would otherwise attribute one player's downs
/// to whoever happens to sit at that index -- a wrong answer that looks
/// exactly like a right one. A length mismatch drops the whole slice
/// instead, the same trade `blocks.healing` makes on its own positional
/// pass.
#[test]
fn a_mismatched_activity_slice_is_dropped_rather_than_misjoined() {
    let (enc, metrics, legacy, _v1) = common::fixture_report_no_gates();
    let short: Vec<axilog_core::analysis::replay::ActivityIntervals> = Vec::new();
    assert_ne!(short.len(), legacy.players.len(), "the slice must actually be the wrong length");

    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes { activity: Some(&short), ..Default::default() },
    );
    assert!(
        v1.blocks.replay.as_ref().map_or(true, |r| r.by_entity.0.is_empty()),
        "a mis-sized slice must produce no rows at all, never a shifted join"
    );
}

/// With neither half computed the block is absent, not an empty object --
/// `not_computed` and "we looked and there was nothing" stay distinct, the
/// distinction `coverage` exists for.
#[test]
fn the_block_is_absent_when_neither_half_ran() {
    let (enc, metrics, legacy, _v1) = common::fixture_report_no_gates();
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &Default::default(),
    );
    assert!(v1.blocks.replay.is_none(), "no activity pass and no --replay means no block");
    assert_eq!(v1.coverage.get(BlockName::Replay.as_str()), Some(CoverageState::NotComputed));
}
