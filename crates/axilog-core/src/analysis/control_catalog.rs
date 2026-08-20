//! The duration-stacking CONTROL-effect catalog -- Stun and Daze.
//!
//! ## Why these two ids need a table of their own
//!
//! Elite Insights classifies both as `BuffClassification.Other`, not
//! `Condition`, so neither appears in `CommonBuffs.Conditions` and neither
//! is in this project's [`crate::analysis::condition_catalog::CONDITION_BUFFS`].
//! They are also not boons. Before this table there was no id table in this
//! repo that carried them at all, which is exactly why the squad-side CC
//! lanes downstream were permanently empty.
//!
//! The remaining control effects -- Knockdown, Launch, Pull, Knockback,
//! Float, Sink -- are deliberately ABSENT. They are instantaneous, not
//! duration buffs: they produce no apply/remove pair and so no stack
//! timeline exists to build. `analysis::cc` already counts them, which is
//! the correct shape for an instantaneous effect.
//!
//! ## The two values, measured rather than guessed
//!
//! `is_intensity = false` and `ctor_capacity = 1` for both, read off
//! `sc::BUFF_INFO` in `fixtures/wvw-small.anon.zevtc` (build 20260114) and
//! calibrated against ids whose classification is already known: every
//! known intensity id reports arcdps stack type 4 or 0, every known
//! duration id reports 1, and these two report 5 --
//! [`crate::analysis::buffs::BuffStackType::Force`], whose
//! `is_intensity()` is false. arcdps reports a max-stacks of 1 for both,
//! so the table and the log agree and the fallback can never contradict
//! the log. Elite Insights' own `buffMap` agrees independently: `b872` and
//! `b833` are `"stacking": false` with `"Max Stack(s) 1"`.

pub const STUN: u32 = 872;
pub const DAZE: u32 = 833;

/// `(skill id, display name, is_intensity, ctor capacity)` -- the same
/// four-tuple shape [`crate::analysis::condition_catalog::CONDITION_BUFFS`]
/// carries, so one lookup can scan both tables.
pub const CONTROL_EFFECTS: [(u32, &str, bool, u32); 2] =
    [(DAZE, "Daze", false, 1), (STUN, "Stun", false, 1)];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::buffs::BOON_IDS;
    use crate::analysis::condition_catalog::CONDITION_BUFFS;

    #[test]
    fn the_table_is_sorted_deduplicated_and_well_formed() {
        let ids: Vec<u32> = CONTROL_EFFECTS.iter().map(|&(id, _, _, _)| id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ids, "CONTROL_EFFECTS must be ascending");
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no duplicate ids");
        for &(id, name, _, cap) in CONTROL_EFFECTS.iter() {
            assert!(!name.is_empty(), "{id} needs a display name");
            assert!(cap > 0, "{id} needs a positive capacity");
        }
    }

    /// Both entries are duration-stacking with capacity 1 -- the measured
    /// values this table exists to record. A silent flip to intensity would
    /// make every Stun timeline report a raw stack count where Elite
    /// Insights reports 0/1.
    #[test]
    fn stun_and_daze_are_duration_stacking_with_capacity_one() {
        for &(id, _, is_intensity, cap) in CONTROL_EFFECTS.iter() {
            assert!(!is_intensity, "{id} is duration-stacking (BuffStackType::Force)");
            assert_eq!(cap, 1, "{id} has ctor capacity 1");
        }
    }

    /// The three id tables must stay pairwise disjoint. A duplicate would
    /// make a composed lookup's answer depend on scan order.
    #[test]
    fn the_control_table_is_disjoint_from_the_boon_and_condition_tables() {
        for &(id, _, _, _) in CONTROL_EFFECTS.iter() {
            assert!(
                !CONDITION_BUFFS.iter().any(|&(cid, _, _, _)| cid == id),
                "control effect {id} must not be in the condition catalog"
            );
            assert!(
                !BOON_IDS.iter().any(|&(bid, _, _)| bid == id),
                "control effect {id} must not be in the boon table"
            );
        }
    }
}
