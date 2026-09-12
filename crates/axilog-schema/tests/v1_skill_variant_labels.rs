//! Skill variant disambiguation: fixture-level regression.
//!
//! `no_two_entries_render_identically` (in `axilog-schema/src/v1/catalogs.rs`)
//! is a unit test that builds a two-entry `BTreeMap` by hand and calls
//! `label_name_collisions` directly. It would still pass even if
//! `CatalogBuilder::finish` never called `label_name_collisions` at all --
//! it never touches the real parse pipeline. This file closes that gap: it
//! parses the COMMITTED fixture through the full pipeline (the same
//! `build_report_v1` path every real caller uses) and checks the rendered-row
//! invariant on the catalog that pipeline actually produced.

mod common;

/// Across `catalogs.skills`, the pair `(name, variant_label)` must be
/// unique -- two ids that share a name must not ALSO share a label, or a
/// consumer rendering `name` plus `variant_label` (the Discord report shape
/// this feature exists for) would print two identical rows for two
/// different skills.
///
/// This states the rule rather than hardcoding the fixture's current
/// numbers, so it keeps holding if the fixture or the curated table grows.
/// It is still consistent with today's fixture, which has 512 skills, 35
/// colliding name groups, and 77 labelled entries -- rendering zero
/// duplicate rows.
#[test]
fn rendered_skill_rows_are_unique_across_the_committed_fixture() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();

    let skills = &v1.catalogs.skills;
    assert!(
        skills.len() > 1,
        "fixture must carry more than one skill for this invariant to mean anything"
    );

    let mut rendered: Vec<String> = skills
        .values()
        .map(|entry| match &entry.variant_label {
            Some(label) => format!("{} ({label})", entry.name),
            None => entry.name.clone(),
        })
        .collect();
    let total = rendered.len();

    rendered.sort();
    rendered.dedup();

    assert_eq!(
        rendered.len(),
        total,
        "two or more skill entries render identically in catalogs.skills -- \
         label_name_collisions failed to disambiguate a real collision"
    );

    // Sanity floor: the fixture is known to contain at least one colliding
    // name group, so a scan that finds zero labels at all is not proving
    // the invariant -- it's just not exercising `label_name_collisions`.
    let labelled = skills.values().filter(|e| e.variant_label.is_some()).count();
    assert!(
        labelled > 0,
        "expected at least one labelled skill entry on the committed fixture; \
         found none -- label_name_collisions may not be running at all"
    );
}
