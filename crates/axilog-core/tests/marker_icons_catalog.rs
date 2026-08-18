//! Behaviour tests for the generated `analysis::marker_icons` table.
//!
//! These live OUT of the catalog file on purpose. `scripts/gen_marker_
//! catalog.py` rewrites that file whole, so a `mod tests` appended there by
//! hand is silently deleted the next time anyone regenerates -- which is
//! exactly what happened once, taking five tests with it and leaving a
//! green suite. An integration test cannot be clobbered that way.

use axilog_core::analysis::marker_icons::{lookup, MARKERS};

#[test]
fn the_table_is_sorted_so_binary_search_is_valid() {
    assert!(MARKERS.windows(2).all(|w| w[0].guid < w[1].guid), "MARKERS must be sorted by guid");
}

/// The one marker GUID the committed WvW fixture actually carries.
/// arcdps reports it as a bare hex string; without this table there is
/// no way to know it means "purple commander tag".
#[test]
fn resolves_the_purple_commander_tag_the_fixture_carries() {
    let m = lookup("1993fadb6fb70e4383a223a54d311f7d").expect("purple commander tag");
    assert_eq!(m.kind, "commander_tag");
    assert_eq!(m.label, "Purple");
    assert!(m.icon.is_some_and(|i| i.contains("Commander_tag")), "icon: {:?}", m.icon);
}

/// All eight overhead squad markers resolve and carry art. The fixture
/// has none of these, so this table is the only proof that path works.
#[test]
fn every_overhead_squad_marker_resolves_with_art() {
    let squad: Vec<_> = MARKERS.iter().filter(|m| m.kind == "squad_marker").collect();
    assert_eq!(squad.len(), 8, "GW2 has exactly eight overhead squad markers");
    for m in &squad {
        assert!(m.icon.is_some(), "{} has no art", m.label);
    }
    let mut labels: Vec<_> = squad.iter().map(|m| m.label).collect();
    labels.sort_unstable();
    assert_eq!(labels, ["Arrow", "Circle", "Heart", "Square", "Star", "Swirl", "Triangle", "X"]);
}

/// Lookups are case-sensitive by design: GW2EI writes GUIDs uppercase,
/// the log spells them lowercase, and the caller lowercases. Pin that
/// contract so nobody "fixes" it into a silent miss.
#[test]
fn lookup_is_lowercase_only() {
    assert!(lookup("1993FADB6FB70E4383A223A54D311F7D").is_none());
    assert!(lookup("1993fadb6fb70e4383a223a54d311f7d").is_some());
}

#[test]
fn an_unknown_guid_resolves_to_nothing_rather_than_a_guess() {
    // Present in the committed fixture, absent from GW2EI's tables.
    assert!(lookup("3cd1c64a5000774488009d4d69455c5c").is_none());
}
