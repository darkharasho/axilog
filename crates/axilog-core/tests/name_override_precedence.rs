//! How much does the override table MOVE, and how much does it FIX?
//!
//! `skill_name_overrides` is unlike `skill_icon_overrides` in one way that
//! decides where it belongs in the chain: an icon override can only change
//! art nobody was reading, but a NAME override can rename an id the log
//! already named perfectly well, because GW2EI's table carries its own
//! disambiguations ("Flame Blast (Superior Sigil of Fire)" where the log
//! says "Flame Blast").
//!
//! Ranked FIRST it matches GW2EI's own `SkillItem.cs`. Ranked LAST it can
//! only ever displace the `"Skill <id>"` placeholder, which is the same
//! justification `skill_map`'s doc comment gives for ranking the API
//! catalog third.
//!
//! MEASURED on `fixtures/local/wvw-postrework.zevtc`: ranking the override
//! table first would rename 17 ids the log table already resolved (would
//! move a golden-visible name for a readability gain) against only 2
//! placeholder ids it actually fixes. 17 exceeds the 5-id budget
//! `skill_map::resolve_name`'s doc comment sets for this, so the table is
//! shipped LAST-resort instead: below `skill_icons::name`, where it can
//! only ever displace `"Skill <id>"`.
//!
//! This file has TWO tests, covering two different questions:
//!
//! - [`override_table_renames_few_and_fixes_many`] measures the
//!   log-table-vs-override relationship on one real capture (gated on the
//!   gitignored local fixture). It prints the historical "would rename if
//!   ranked first" count (`would_rename`, informational only, not
//!   asserted -- it is a property of the two data tables, not of this
//!   module's precedence choice, so it does not change when the ranking
//!   does) and asserts the "actually renamed by the shipped chain" count
//!   (`actually_renamed`) stays at `MAX_RENAMES`. It also prints the
//!   double-covered cohort size (see below) for visibility alongside
//!   those two numbers, but does not assert on it -- that assertion lives
//!   in the second test, which does not depend on the fixture.
//!
//! - [`double_covered_ids_prefer_skill_icons_over_the_override_table`]
//!   is the test that actually pins THIS task's ordering decision:
//!   `skill_icons::name` before `skill_name_overrides::name`. It needs no
//!   log and no fixture -- it walks the two generated catalogs directly,
//!   finds every id BOTH cover, and for the ones where the two tables
//!   actually disagree on the name, asserts `resolve_name` returns the
//!   `skill_icons` name. It is NOT gated and always runs in CI.
//!
//! # Why two tests, and what each does and does not catch
//!
//! The first test's `would_rename`/`actually_renamed`/`fixed` buckets
//! only classify ids by whether the LOG name was usable (`fixed` further
//! requires `skill_icons::name(id).is_none()`, i.e. override-ONLY ids).
//! An id covered by BOTH `skill_icons` and `skill_name_overrides`, with
//! an unusable log name, falls into neither the "usable branch" nor the
//! override-only `fixed` branch -- it is silently skipped, and that
//! skip is unavoidable given what that test measures (the log-vs-override
//! relationship, not the icon-vs-override one). Consequence: on its own,
//! that test does NOT exercise the `skill_icons` vs `skill_name_overrides`
//! sub-ordering that this task actually decided. If someone later
//! reordered `resolve_name`'s final rungs to
//! `skill_name_overrides::name(id).or_else(|| skill_icons::name(id))`,
//! `actually_renamed` would stay 0 (both rungs are still last-resort
//! relative to the log table and `pseudo_name`) and the first test alone
//! would still pass.
//!
//! `double_covered_ids_prefer_skill_icons_over_the_override_table` closes
//! that gap: it is the one that would catch exactly that reordering,
//! because it asserts the icon name wins for every double-covered id
//! where the two tables disagree.
//!
//! Neither test catches every possible regression (e.g. `pseudo_name`
//! being reordered relative to either table is out of scope for both --
//! pseudo ids are negative-as-`u32` and cannot collide with positive
//! skill ids, per `skill_map::resolve_name`'s doc comment). What IS
//! caught, precisely: (1) the override table renaming an id the log
//! table or `pseudo_name` already resolved (test 1), and (2) the override
//! table winning over `skill_icons` for a double-covered id (test 2).

mod common;

use axilog_core::analysis::skill_map::resolve_name;
use axilog_core::analysis::{analyze, skill_icons, skill_name_overrides};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;

/// The shipped (last-resort) chain may actually rename at most this many
/// ids that some other rung already resolved, on one log. Last-resort
/// ranking guarantees this is 0 by construction -- see this file's module
/// doc and `skill_map::resolve_name`'s doc comment.
const MAX_RENAMES: usize = 0;

/// Every id present in BOTH `skill_icons::name` and
/// `skill_name_overrides::name`, split into ids where the two tables
/// happen to AGREE on the name (which prove nothing about ordering -- one
/// or the other winning is unobservable) and ids where they DISAGREE
/// (where ordering is directly observable, and is exactly what this
/// task's precedence decision controls).
fn double_covered_cohort() -> (usize, Vec<(u32, &'static str, &'static str)>) {
    let mut agreements = 0usize;
    let mut disagreements = Vec::new();
    for &(id, over_name) in skill_name_overrides::SKILL_NAME_OVERRIDES {
        let Some(icon_name) = skill_icons::name(id) else { continue };
        if icon_name == over_name {
            agreements += 1;
        } else {
            disagreements.push((id, icon_name, over_name));
        }
    }
    (agreements, disagreements)
}

#[test]
fn override_table_renames_few_and_fixes_many() {
    let Some(bytes) = common::read_bytes_or_skip(
        &common::local_fixture("wvw-postrework.zevtc"),
        "name-override precedence measurement",
    ) else {
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut would_rename = Vec::new();
    let mut actually_renamed = Vec::new();
    let mut fixed = Vec::new();
    for (&id, log_name) in &metrics.log_skill_names {
        let Some(over) = skill_name_overrides::name(id) else { continue };
        let trimmed = log_name.trim();
        let usable = !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit());
        let resolved = resolve_name(id, Some(log_name));
        if usable {
            if trimmed != over {
                would_rename.push((id, trimmed.to_string(), over));
            }
            if resolved != trimmed {
                actually_renamed.push((id, trimmed.to_string(), resolved.clone()));
            }
        } else if skill_icons::name(id).is_none() {
            fixed.push((id, over));
            assert_eq!(
                resolved, over,
                "id {id}: the last-resort override rung should still fix this placeholder"
            );
        }
    }

    let (agreements, disagreements) = double_covered_cohort();

    println!(
        "name-override precedence on wvw-postrework: {} would-rename-if-first, {} actually-renamed-as-shipped, {} placeholder-fixes, {} double-covered ({} agree, {} disagree -- see double_covered_ids_prefer_skill_icons_over_the_override_table for the assertion)",
        would_rename.len(),
        actually_renamed.len(),
        fixed.len(),
        agreements + disagreements.len(),
        agreements,
        disagreements.len()
    );
    for (id, was, now) in would_rename.iter().take(20) {
        println!("  WOULD-RENAME {id}: {was:?} -> {now:?}");
    }
    for (id, was, now) in actually_renamed.iter().take(20) {
        println!("  ACTUALLY-RENAMED {id}: {was:?} -> {now:?}");
    }
    for (id, now) in fixed.iter().take(20) {
        println!("  FIX    {id}: \"Skill {id}\" -> {now:?}");
    }

    assert!(
        actually_renamed.len() <= MAX_RENAMES,
        "the shipped chain actually renamed {} ids that already resolved (cap {MAX_RENAMES}). \
         Last-resort ranking should make this impossible by construction -- see this module's \
         doc comment before changing the cap.",
        actually_renamed.len()
    );
}

/// Pins THIS task's actual precedence decision: for every id both
/// `skill_icons` and `skill_name_overrides` cover, and where the two
/// tables disagree on the name (agreements are skipped -- they cannot
/// distinguish the two orderings), `resolve_name` must return the
/// `skill_icons` name, never the override name. Needs no log and no
/// fixture: it is a static property of the two generated catalogs, so it
/// always runs in CI, unlike `override_table_renames_few_and_fixes_many`.
///
/// Asserts the disagreeing cohort is NON-EMPTY, so this test cannot pass
/// vacuously (an empty cohort would mean every double-covered id happens
/// to agree, in which case the ordering decision would be unobservable on
/// the current catalogs -- that would itself be worth reporting, not
/// silently passing).
#[test]
fn double_covered_ids_prefer_skill_icons_over_the_override_table() {
    let (agreements, disagreements) = double_covered_cohort();
    let cohort_size = agreements + disagreements.len();

    println!(
        "double-covered ids (present in both skill_icons and skill_name_overrides): {cohort_size} total, {agreements} agree, {} disagree",
        disagreements.len()
    );
    for (id, icon_name, over_name) in disagreements.iter().take(20) {
        println!("  DOUBLE-COVERED {id}: skill_icons={icon_name:?} overrides={over_name:?}");
    }

    assert!(
        !disagreements.is_empty(),
        "no double-covered id disagrees between skill_icons and skill_name_overrides ({agreements} \
         agree, 0 disagree) -- this task's icon-over-override ordering would be UNOBSERVABLE on the \
         current catalogs. That is a real finding, not a pass: report it rather than letting this \
         assert be vacuously satisfied."
    );

    for &(id, icon_name, over_name) in &disagreements {
        let resolved = resolve_name(id, None);
        assert_eq!(
            resolved, icon_name,
            "id {id}: double-covered by both skill_icons ({icon_name:?}) and skill_name_overrides \
             ({over_name:?}) with no usable log name -- the demoted chain must prefer skill_icons, \
             but resolve_name returned {resolved:?}. This is the exact regression this test exists \
             to catch: skill_name_overrides ranked ahead of skill_icons."
        );
    }
}
