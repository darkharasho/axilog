//! Calibration pins for `analysis::arcdps_parity` against the committed WvW
//! fixture.
//!
//! The unit tests in the module itself prove each transcribed rule in
//! isolation on synthetic rows. This file pins what those rules add up to on
//! a real capture, because the squad-wide totals are the actual deliverable
//! -- the whole point of the module is that AxiBridge's cleanse column reads
//! a few percent under the in-game meter, and these numbers are what say
//! whether it still does.
//!
//! The load-bearing assertion is `base_is_within_a_hair_of_ei`: with no
//! minion rows on either side, arcdps' methodology and GW2EI's should agree
//! almost exactly, because everything they disagree about (non-squad
//! recipients, self-consumed blind, the down dump, stability single-stacks)
//! is small and partly self-cancelling. They land 2 apart on 898. That is
//! the single best evidence the transcription is right: it is a prediction
//! the implementation could easily have failed, made against a number nobody
//! tuned it to.

use axilog_core::analysis::analyze;
use axilog_core::analysis::support::SupportMetrics;
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");

fn totals() -> SupportMetrics {
    let bytes = std::fs::read(FIXTURE).expect("read committed fixture");
    let raw = decode_raw(&bytes).expect("decode fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);
    metrics.players.iter().fold(SupportMetrics::default(), |mut acc, p| {
        acc.cleanses += p.support.cleanses;
        acc.cleanses_self += p.support.cleanses_self;
        acc.cleanses_minions += p.support.cleanses_minions;
        acc.cleanses_arcdps += p.support.cleanses_arcdps;
        acc.cleanses_arcdps_by_minion += p.support.cleanses_arcdps_by_minion;
        acc.cleanses_arcdps_on_minion += p.support.cleanses_arcdps_on_minion;
        acc.strips += p.support.strips;
        acc.strips_arcdps += p.support.strips_arcdps;
        acc.strips_arcdps_by_minion += p.support.strips_arcdps_by_minion;
        acc.strips_arcdps_on_minion += p.support.strips_arcdps_on_minion;
        acc
    })
}

/// Player-to-player only, both meter toggles off: arcdps' methodology and
/// EI's `condiCleanse + condiCleanseSelf` describe the same population, so
/// they must land within a point of each other.
#[test]
fn base_is_within_a_hair_of_ei() {
    let t = totals();
    let ei = t.cleanses + t.cleanses_self;
    assert_eq!(ei, 898, "EI baseline moved -- recalibrate before touching parity");
    assert_eq!(t.cleanses_arcdps, 900);
    let drift = (t.cleanses_arcdps as f64 / ei as f64 - 1.0).abs();
    assert!(drift < 0.01, "base drifted {drift:.4} from EI; expected well under 1%");
}

/// The "vs npcs" bucket -- squad players cleansing their own pets -- is what
/// the field reports of a +3.3%/+4.1% arcdps-over-AxiBridge gap were seeing.
/// It reproduces here at +5.4%, the right size and the right sign.
#[test]
fn vs_npcs_bucket_reproduces_the_reported_field_gap() {
    let t = totals();
    let ei = t.cleanses + t.cleanses_self;
    assert_eq!(t.cleanses_arcdps_on_minion, 46);
    let with_pets = t.cleanses_arcdps + t.cleanses_arcdps_on_minion;
    let gap = (with_pets as f64 / ei as f64 - 1.0) * 100.0;
    assert!((3.0..7.0).contains(&gap), "gap {gap:.2}% outside the reported 3-4% neighbourhood");
}

/// Cross-check against the independently-derived
/// [`SupportMetrics::cleanses_minions`], which counts the same population
/// (conditions cleansed off a squad member's pet) by an entirely different
/// route -- EI's `real_players` set plus a master lookup, rather than the
/// row's `src_master_instid`. Two implementations, one number.
#[test]
fn on_minion_bucket_agrees_with_the_ei_side_minion_counter() {
    let t = totals();
    assert_eq!(t.cleanses_arcdps_on_minion, t.cleanses_minions);
}

/// The "from npcs" bucket -- cleanses performed BY pets, which EI drops
/// entirely rather than folding into the master. The largest single
/// divergence, and the reason a hardcoded "the arcdps number" would have
/// been wrong for most readers.
#[test]
fn from_npcs_bucket_is_pinned() {
    assert_eq!(totals().cleanses_arcdps_by_minion, 157);
}

/// Strips: the base excludes both the stability single-stack removals EI
/// counts and the boons taken off enemy pets, so it sits BELOW EI; adding
/// the "vs npcs" bucket back lands 14 under, which is exactly the stability
/// population.
#[test]
fn strips_buckets_are_pinned() {
    let t = totals();
    assert_eq!(t.strips, 437, "EI strips baseline moved");
    assert_eq!(t.strips_arcdps, 322);
    assert_eq!(t.strips_arcdps_by_minion, 0);
    assert_eq!(t.strips_arcdps_on_minion, 101);
    assert_eq!(
        t.strips - (t.strips_arcdps + t.strips_arcdps_on_minion),
        14,
        "the EI-minus-arcdps strip residue is the stability single-stack population"
    );
}
