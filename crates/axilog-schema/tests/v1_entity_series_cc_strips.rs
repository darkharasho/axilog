//! CC-strip-timelines Task 4: the per-entity CC and boon-strip 1s lanes on
//! `blocks.series.by_entity`.
//!
//! Three claims, and each one is a different way the join can go wrong:
//!
//! 1. The lanes DECOMPOSE the scalars they sit beside -- every lane sums to
//!    the block scalar built by a completely different pass. A lane that
//!    counted a different event set, or that folded a relogged account onto
//!    the wrong row, breaks this.
//! 2. The lanes sit on the SAME BUCKET GRID as the squad lane. A sum-only
//!    assertion cannot see a uniform shift, and a uniform shift is exactly
//!    what a missing `log_start_ms` subtraction produces (the strip and CC
//!    tuples carry RAW `e.time`, not log-relative ms). So the per-bucket
//!    equality against `SquadSeries::strips` -- built by
//!    `cc::timeline_with_registry`, which does its own `t0` subtraction --
//!    is the load-bearing assertion here, not the sums.
//! 3. Absent means "the gate was off", never "measured zero", and the
//!    positional length guard makes that true even when the pass and the
//!    roster disagree.

mod common;

use axilog_schema::v1::entities::Role;

/// Decode the committed fixture into everything the block builders need.
/// `common::fixture_report` cannot serve here: these tests need the raw log
/// and the roster-truncation control, and they need to call `build_series`
/// with a hand-chosen `entity_series` argument rather than the all-on /
/// all-off pair that harness offers.
struct Direct {
    raw: axilog_core::evtc::RawLog,
    enc: axilog_core::model::Encounter,
    metrics: axilog_core::analysis::Metrics,
    legacy: axilog_schema::Report,
    index: axilog_schema::v1::entities::EntityIndex,
}

fn direct() -> Direct {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let (_entities, index, _order) = axilog_schema::v1::entities::build_entities(&enc, &metrics);
    // Every gate ON, so `per_second` is populated and `build_series`
    // actually emits per-entity rows -- otherwise a "the lanes are absent"
    // assertion would pass over an empty map.
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, true, true, None);
    Direct { raw, enc, metrics, legacy, index }
}

fn detail_for(d: &Direct, players: &[axilog_core::analysis::PlayerMetrics])
    -> axilog_core::analysis::entity_series::EntitySeriesDetail
{
    let squad: std::collections::BTreeSet<u64> =
        d.enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let addr_to_rep: std::collections::BTreeMap<u64, u64> = d
        .enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let enemy_addrs: std::collections::BTreeSet<u64> =
        d.enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    axilog_core::analysis::entity_series::build(
        &d.enc,
        &d.raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&d.raw),
        players,
        &squad,
        &enemy_addrs,
        &addr_to_rep,
    )
}

fn is_friendly(role: Role) -> bool {
    matches!(role, Role::Squad | Role::FriendlyPlayer)
}

/// Per-entity CC and strip lanes must be present when `timeseries` is on
/// and must sum to the block scalars they decompose.
#[test]
fn entity_cc_and_strip_lanes_sum_to_block_scalars() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let series = v1.blocks.series.as_ref().expect("series block present");
    let cc = v1.blocks.cc.as_ref().expect("cc block present");
    let support = v1.blocks.support.as_ref().expect("support block present");
    let defenses = v1.blocks.defenses.as_ref().expect("defenses block present");

    let mut checked = 0usize;
    // Running totals across every friendly row, so the non-vacuity guard
    // below can pin the CC and strips-taken lanes the same way the strips
    // lane is already pinned (against the squad total, in the next test):
    // `cc_sum == applied_total` and `taken_sum == boon_strips_taken` both
    // pass trivially at `0 == 0` on a fixture that never records friendly
    // CC or incoming strips, which proves only that the fields exist, not
    // that the decomposition is right.
    let mut cc_total = 0u64;
    let mut cc_taken_total = 0u64;
    let mut taken_total = 0u64;
    // Friendly rows only: `by_entity` also carries the enemy rows Task 8
    // added, and no pass measures these lanes for an enemy -- so `None`
    // there is the honest answer, not a regression. (The brief's sketch
    // iterated every row; on a real fixture that trips over the enemy
    // rows, which is why this filters.)
    for e in v1.entities.iter().filter(|e| is_friendly(e.role)) {
        let Some(entity) = series.by_entity.get(e.id) else { continue };

        let cc_sum: u64 =
            entity.cc_applied.as_ref().expect("cc lane present").decode_u64().iter().sum();
        assert_eq!(
            cc_sum,
            u64::from(cc.by_entity.get(e.id).expect("cc row").applied_total),
            "entity {}: the CC lane must decompose `blocks.cc[].applied_total`",
            e.id
        );
        cc_total += cc_sum;

        let cc_taken_sum: u64 =
            entity.cc_taken.as_ref().expect("cc-taken lane present").decode_u64().iter().sum();
        assert_eq!(
            cc_taken_sum,
            u64::from(defenses.by_entity.get(e.id).expect("defenses row").received_cc_count),
            "entity {}: the CC-taken lane must decompose `blocks.defenses[].received_cc_count`",
            e.id
        );
        cc_taken_total += cc_taken_sum;

        let strips_sum: u64 =
            entity.strips.as_ref().expect("strips lane present").decode_u64().iter().sum();
        assert_eq!(
            strips_sum,
            u64::from(support.by_entity.get(e.id).expect("support row").strips),
            "entity {}: the strips lane must decompose `blocks.support[].strips`",
            e.id
        );

        let taken_sum: u64 =
            entity.strips_taken.as_ref().expect("taken lane present").decode_u64().iter().sum();
        assert_eq!(
            taken_sum,
            u64::from(defenses.by_entity.get(e.id).expect("defenses row").boon_strips_taken),
            "entity {}: the strips-taken lane must decompose \
             `blocks.defenses[].boon_strips_taken`",
            e.id
        );
        taken_total += taken_sum;
        checked += 1;
    }
    assert!(checked > 0, "no friendly series rows to check -- the fixture proved nothing");
    assert!(
        cc_total > 0,
        "the fixture recorded no friendly CC at all -- the `cc_sum == applied_total` pin \
         above would otherwise hold vacuously at 0 == 0"
    );
    assert!(
        cc_taken_total > 0,
        "the fixture recorded no incoming CC at all -- the `cc_taken_sum == \
         received_cc_count` pin above would otherwise hold vacuously at 0 == 0"
    );
    assert!(
        taken_total > 0,
        "the fixture recorded no incoming strips at all -- the `taken_sum == \
         boon_strips_taken` pin above would otherwise hold vacuously at 0 == 0"
    );
}

/// The BUCKET-INDEX pin, and the reason this file exists rather than three
/// more lines in `v1_healing_detail.rs`.
///
/// `SquadSeries::strips` and the per-entity `strips` lanes fold the SAME
/// `support::outgoing_boon_strips` primitive on the SAME grid
/// (`duration_ms / 1000 + 1` buckets, both subtracting `raw.log_start_ms()`),
/// through two independent code paths. Column-by-column equality is
/// therefore exact -- and it is the only assertion here that can see a
/// uniform bucket shift, which every sum in the test above would happily
/// agree with.
#[test]
fn entity_strip_lanes_align_bucket_for_bucket_with_the_squad_lane() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let series = v1.blocks.series.as_ref().expect("series block present");

    let squad = series.squad.strips.decode_u64();
    let mut summed = vec![0u64; squad.len()];
    for e in v1.entities.iter().filter(|e| is_friendly(e.role)) {
        let Some(entity) = series.by_entity.get(e.id) else { continue };
        let lane = entity.strips.as_ref().expect("strips lane present").decode_u64();
        assert_eq!(
            lane.len(),
            squad.len(),
            "entity {}: the per-entity lane is on a different grid than the squad lane",
            e.id
        );
        for (b, v) in lane.iter().enumerate() {
            summed[b] += v;
        }
    }
    assert_eq!(
        summed, squad,
        "the per-entity strip lanes must add up to the squad lane BUCKET BY BUCKET -- a \
         mismatch that still sums correctly overall is a grid offset (a missing log-start \
         subtraction), not a counting bug"
    );

    // A fixture whose entire mass sits in bucket 0 (or in no bucket at all)
    // would make the column comparison above vacuous, so pin that it does
    // not.
    let total: u64 = squad.iter().sum();
    assert!(total > 0, "the fixture recorded no strips at all -- the alignment pin is vacuous");
    let nonzero = squad.iter().filter(|v| **v > 0).count();
    assert!(
        nonzero > 1 && squad.iter().skip(1).sum::<u64>() > 0,
        "every strip landed in one bucket -- the alignment pin cannot see a shift"
    );
}

/// The `--timeseries` gate. Absent is a statement about the FLAG, so no
/// zero-filled lane may appear when it is off.
///
/// Driven through `build_series` directly rather than through
/// `common::fixture_report_no_gates`: with every gate off, `per_second` is
/// `None` for every player and `by_entity` is EMPTY, so the loop below
/// would have iterated nothing and the test would have passed vacuously.
/// Here the rest of the gates stay on -- the rows exist, and only the
/// entity-series pass is withheld.
#[test]
fn entity_cc_and_strip_lanes_absent_without_timeseries() {
    let d = direct();
    let block = axilog_schema::v1::blocks::activity::build_series(
        &d.legacy, &d.index, None, None, None, None,
    );
    let mut rows = 0usize;
    for (id, entity) in block.by_entity.iter() {
        assert!(entity.cc_applied.is_none(), "entity {id}: CC lane emitted with the gate off");
        assert!(entity.cc_taken.is_none(), "entity {id}: CC-taken lane emitted with the gate off");
        assert!(entity.strips.is_none(), "entity {id}: strip lane emitted with the gate off");
        assert!(
            entity.strips_taken.is_none(),
            "entity {id}: strips-taken lane emitted with the gate off"
        );
        rows += 1;
    }
    assert!(rows > 0, "no rows were built, so the gate assertion proved nothing");

    // And the whole-document form: the fully-ungated fixture must not carry
    // a lane either, whatever rows it happens to have.
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_no_gates();
    let series = v1.blocks.series.as_ref().expect("series block is always emitted");
    for (id, entity) in series.by_entity.iter() {
        assert!(
            entity.cc_applied.is_none()
                && entity.cc_taken.is_none()
                && entity.strips.is_none()
                && entity.strips_taken.is_none(),
            "entity {id}: lane emitted on the no-gates document"
        );
    }
}

/// The POSITIONAL-JOIN guard. `EntitySeriesDetail` is a `Vec` over
/// `enc.players` carrying no address, so a detail whose length disagrees
/// with the roster cannot be joined at all -- and silently joining it
/// anyway would attribute every player's lane to their neighbour, which no
/// sum invariant above would catch (the totals would still be a
/// permutation of the right numbers).
///
/// Dropping the pass wholesale is the only honest answer, and `None` then
/// means what it always means. Exercised by handing the builder a detail
/// built over a TRUNCATED player slice.
#[test]
fn a_length_mismatched_detail_is_dropped_rather_than_misattributed() {
    let d = direct();
    assert!(d.legacy.players.len() > 1, "need a roster to truncate");

    let short = detail_for(&d, &d.metrics.players[..d.metrics.players.len() - 1]);
    assert_ne!(short.len(), d.legacy.players.len(), "the truncated detail must actually mismatch");

    let block = axilog_schema::v1::blocks::activity::build_series(
        &d.legacy, &d.index, None, None, None, Some(&short),
    );
    let mut rows = 0usize;
    for (id, entity) in block.by_entity.iter() {
        assert!(
            entity.cc_applied.is_none()
                && entity.cc_taken.is_none()
                && entity.strips.is_none()
                && entity.strips_taken.is_none(),
            "entity {id}: a length-mismatched detail was joined anyway"
        );
        rows += 1;
    }
    assert!(rows > 0, "no rows built -- the guard was not exercised");

    // Control: the SAME builder with a correctly-sized detail does populate
    // the lanes, so the assertion above is about the guard and not about
    // the builder being inert.
    let full = detail_for(&d, &d.metrics.players);
    let block = axilog_schema::v1::blocks::activity::build_series(
        &d.legacy, &d.index, None, None, None, Some(&full),
    );
    assert!(
        block.by_entity.iter().any(|(_, e)| e.strips.is_some()),
        "the correctly-sized detail must populate at least one lane"
    );
}
