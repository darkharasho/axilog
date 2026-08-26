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
//! This file prints both counts every run -- the historical "would rename
//! if ranked first" count (`would_rename`, informational only, not
//! asserted -- it is a property of the two data tables, not of this
//! module's precedence choice, so it does not change when the ranking
//! does) and the "actually renamed by the shipped chain" count
//! (`actually_renamed`, asserted against `MAX_RENAMES`). Last-resort
//! ranking makes `actually_renamed` zero BY CONSTRUCTION: the override
//! rung only runs when `skill_icons::name` also missed, so it can never
//! displace a name the log table already supplied. If a future GW2EI sync
//! or a reordering of `resolve_name`'s chain ever makes `actually_renamed`
//! nonzero, this test fails instead of quietly renaming a squad's skills.

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

    println!(
        "name-override precedence on wvw-postrework: {} would-rename-if-first, {} actually-renamed-as-shipped, {} placeholder-fixes",
        would_rename.len(),
        actually_renamed.len(),
        fixed.len()
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
