//! Structural guards on the generated `skill_name_overrides` table.
//!
//! Deliberately NOT a transcription of the table's contents: the generator
//! and `git diff` are what verify it against GW2EI's source. These check
//! the two properties the CONSUMER depends on and a generator bug could
//! silently break.

use axilog_core::analysis::skill_name_overrides::{name, SKILL_NAME_OVERRIDES};

#[test]
fn table_is_sorted_by_id_so_the_binary_search_is_valid() {
    let ids: Vec<u32> = SKILL_NAME_OVERRIDES.iter().map(|&(id, _)| id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted, "entries must be sorted by id and unique");
}

#[test]
fn no_entry_is_empty_or_numeric() {
    // A numeric "name" is exactly the arcdps placeholder `resolve_name`
    // rejects -- an override that reinstated one would be worse than none.
    for &(id, n) in SKILL_NAME_OVERRIDES {
        assert!(!n.trim().is_empty(), "skill {id} has an empty override name");
        assert!(
            !n.chars().all(|c| c.is_ascii_digit()),
            "skill {id}'s override name {n:?} is numeric"
        );
    }
}

#[test]
fn resurrect_1066_is_the_reported_id_and_resolves() {
    // The MNAME report's headline offender: arcdps writes the literal
    // string "1066" for this id and /v2/skills has never listed it, so
    // this table is its only name source.
    assert_eq!(name(1066), Some("Resurrect"));
}

#[test]
fn only_positive_ids_are_transcribed() {
    // The negative pseudo ids live in `skill_map::PSEUDO_SKILL_NAMES`.
    for &(id, n) in SKILL_NAME_OVERRIDES {
        assert!((id as i32) > 0, "synthetic id {} ({n}) must not be here", id as i32);
    }
}
