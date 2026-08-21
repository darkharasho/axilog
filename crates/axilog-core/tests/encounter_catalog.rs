//! Structural invariants for the generated `pve::encounters` catalog.
//!
//! These live outside `src/` on purpose: `scripts/gen_encounter_catalog.py`
//! rewrites its target file WHOLE, so a `mod tests` next to the table would
//! be deleted on the next regeneration -- silently, and with a green suite.
//! Same split, and the same reason, as `marker_icons_catalog.rs`.
//!
//! Nothing here re-asserts the *contents* of individual rows; the generator
//! transcribes those from GW2EI and `git diff` is the check that it did.
//! What is checked is everything a transcription bug would break without
//! changing any single row's plausibility: ordering, uniqueness, the value
//! domains the rest of the crate switches on, and the handful of rows other
//! code names directly.

use axilog_core::pve::encounters::{lookup, ENCOUNTERS};

/// Every category slug the catalog is allowed to carry.
///
/// This is a closed set on purpose. `damage_mods::ModeContext::
/// from_encounter` matches on these strings to pick a GW2EI `ParseMode`,
/// and its fallback arm is `ParseMode::Unknown` -- so a new slug appearing
/// in the catalog (GW2EI adding a `LogCategory`, or the generator's
/// `CATEGORY_SLUGS` map drifting) would not fail anything there. It would
/// just quietly analyse those logs in the wrong mode. Fail here instead.
const CATEGORIES: &[&str] = &[
    "fractal",
    "raid_encounter",
    "raid_wing",
    "wvw",
    "golem",
    "story",
    "open_world",
    "convergence",
    "unknown_encounter",
    "unknown",
];

#[test]
fn is_sorted_and_unique_by_trigger_id() {
    // `lookup` binary-searches, so unsorted or duplicated ids do not error
    // -- they miss, and a real boss reads as an unknown encounter.
    for pair in ENCOUNTERS.windows(2) {
        assert!(
            pair[0].trigger_id < pair[1].trigger_id,
            "not strictly ascending: {} ({}) then {} ({})",
            pair[0].trigger_id, pair[0].member,
            pair[1].trigger_id, pair[1].member,
        );
    }
}

#[test]
fn every_row_is_reachable_through_lookup() {
    for e in ENCOUNTERS {
        assert_eq!(lookup(e.trigger_id), Some(e), "{}", e.member);
    }
}

#[test]
fn every_category_is_one_the_rest_of_the_crate_handles() {
    for e in ENCOUNTERS {
        assert!(
            CATEGORIES.contains(&e.category),
            "{} carries category {:?}, which nothing switches on",
            e.member, e.category,
        );
    }
}

#[test]
fn names_and_sub_categories_are_never_blank() {
    // `Some("")` is worse than `None` everywhere downstream: it suppresses
    // the agent-name fallback and renders as an empty fight name.
    for e in ENCOUNTERS {
        assert!(!e.member.is_empty() && !e.logic.is_empty(), "{}", e.trigger_id);
        assert_ne!(e.name, Some(""), "{}: blank name override", e.member);
        assert_ne!(e.sub_category, Some(""), "{}: blank sub-category", e.member);
    }
}

#[test]
fn rows_sharing_a_logic_agree_on_category_and_name() {
    // Two trigger ids handled by the same GW2EI `LogLogic` ARE the same
    // fight -- Nikare and Kenut are both "Twin Largos", the six Old Lion's
    // Court prototypes are one encounter. If the generator's base-class
    // walk went wrong for one of them, they would disagree here.
    for a in ENCOUNTERS {
        for b in ENCOUNTERS {
            if a.logic != b.logic {
                continue;
            }
            assert_eq!(a.category, b.category, "{} vs {}", a.member, b.member);
            assert_eq!(a.name, b.name, "{} vs {}", a.member, b.member);
            assert_eq!(a.sub_category, b.sub_category, "{} vs {}", a.member, b.member);
        }
    }
}

#[test]
fn the_wvw_trigger_id_is_catalogued_as_wvw() {
    // `pve::identify` short-circuits on this id before it ever reaches the
    // table, so this row is never read in anger -- but if the generator
    // ever mapped id 1 to a PvE logic (it did once, picking a conditional
    // `River` return out of the WvW case arm), that would be the signal
    // that the switch parser regressed.
    assert_eq!(lookup(axilog_core::pve::WVW_TRIGGER_ID).map(|e| e.category), Some("wvw"));
}

#[test]
fn the_fixture_bosses_are_present_and_agree_with_their_grouping() {
    // Every trigger id the committed PvE fixtures carry, pinned here as
    // well as in `pve_encounters_golden.rs`, so a catalog regression is
    // attributed to the catalog rather than showing up only as a fixture
    // diff.
    //
    // `name: None` on all but Harvest Temple is the assertion that matters:
    // these fights are named after their own agent, and a constant in the
    // catalog would silently override the log.
    for (id, member, category, sub, name) in [
        (15429u32, "Gorseval", "raid_wing", "SpiritVale", None),
        (16123, "Slothasor", "raid_wing", "SalvationPass", None),
        (16235, "KeepConstruct", "raid_wing", "StrongholdOfTheFaithful", None),
        (16246, "Xera", "raid_wing", "StrongholdOfTheFaithful", None),
        (17188, "Samarog", "raid_wing", "BastionOfThePenitent", None),
        (22521, "Boneskinner", "raid_encounter", "Bjora", None),
        (25577, "KanaxaiScytheOfHouseAurkusCM", "fractal", "SilentSurf", None),
        (27010, "WhisperingShadow", "fractal", "Kinfall", None),
        (16199, "StdGolem", "golem", "Golem", None),
        // The exception, and the reason the `name` column exists: the
        // trigger agent is "The Dragonvoid", the fight is "Harvest Temple".
        (43488, "GadgetTheDragonVoid1", "raid_encounter", "Cantha", Some("Harvest Temple")),
    ] {
        let e = lookup(id).unwrap_or_else(|| panic!("{member} ({id}) missing from the catalog"));
        assert_eq!(e.member, member);
        assert_eq!(e.category, category, "{member}");
        assert_eq!(e.sub_category, Some(sub), "{member}");
        assert_eq!(e.name, name, "{member}");
    }
}

#[test]
fn multi_boss_encounters_carry_a_fixed_name() {
    // The other half of the naming rule: these fights have no single agent
    // to be named after, so the catalog MUST supply the name. A `None`
    // here would silently rename "Twin Largos" to whichever largo the
    // trigger id happened to be.
    for (id, expected) in [
        (21089u32, "Twin Largos"),   // Kenut
        (21105, "Twin Largos"),      // Nikare
        (16088, "Bandit Trio"),      // Berg
        (24375, "Harvest Temple"),   // Void Amalgamate
    ] {
        assert_eq!(lookup(id).and_then(|e| e.name), Some(expected), "trigger id {id}");
    }
}
