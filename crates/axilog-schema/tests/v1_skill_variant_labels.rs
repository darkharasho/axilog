//! Skill variant labels: fixture-level regression.
//!
//! The unit tests in `axilog-schema/src/v1/catalogs.rs` pin the label of
//! individual ids. This file parses the COMMITTED fixture through the full
//! pipeline (the same `build_report_v1` path every real caller uses) and
//! checks the labels that pipeline actually produced.

mod common;

/// On the committed fixture: every label is a readable word, never a raw
/// id; the catalog carries at least one label (the fixture's Catalyst
/// casts Deploy Jade Sphere in three attunements); and those three render
/// as three distinct rows.
#[test]
fn fixture_labels_are_readable_and_split_the_jade_spheres() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let skills = &v1.catalogs.skills;

    for (id, entry) in skills {
        if let Some(label) = &entry.variant_label {
            assert!(
                !label.chars().all(|c| c.is_ascii_digit()),
                "skill {id} ({}) carries a numeric label {label:?}",
                entry.name
            );
        }
    }

    let spheres: Vec<Option<&str>> = [62723u32, 62813, 62940]
        .iter()
        .map(|id| {
            let entry = skills.get(id).unwrap_or_else(|| panic!("fixture lost Deploy Jade Sphere {id}"));
            assert_eq!(entry.name, "Deploy Jade Sphere");
            entry.variant_label.as_deref()
        })
        .collect();
    assert_eq!(spheres, [Some("Water"), Some("Fire"), Some("Air")]);
}
