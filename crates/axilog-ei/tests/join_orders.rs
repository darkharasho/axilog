//! `EiJoin` must hand back exactly the orders the legacy report iterates,
//! because the ei-json goldens are positional.

mod common;
use common::fixture_legacy_and_v1;

#[test]
fn player_iteration_matches_legacy_order() {
    let (legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);

    let seen: Vec<String> = join
        .players()
        .map(|(_, _, e)| e.account.clone().unwrap_or_default())
        .collect();
    let want: Vec<String> = legacy.players.iter().map(|p| p.account.clone()).collect();
    assert_eq!(seen, want);
}

#[test]
fn target_iteration_matches_legacy_order() {
    let (legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);

    let seen: Vec<String> = join
        .targets()
        .map(|(_, id, _)| join.display_name(id).to_string())
        .collect();
    let want: Vec<String> = legacy.ei_targets.iter().map(|t| t.name.clone()).collect();
    assert_eq!(seen, want);
}

#[test]
fn target_slot_is_the_inverse_of_target_iteration() {
    let (_legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);
    for (slot, id, _) in join.targets() {
        assert_eq!(join.target_slot(id), Some(slot));
    }
}

#[test]
fn display_name_prefers_character_then_name() {
    let (_legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);
    for e in &v1.entities {
        let got = join.display_name(e.id);
        let want = e.character.as_deref().or(e.name.as_deref()).unwrap_or("");
        assert_eq!(got, want, "entity {} resolved the wrong label", e.id);
    }
}
