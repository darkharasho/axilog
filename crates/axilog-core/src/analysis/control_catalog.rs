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

/// What KIND of control a CC row applied.
///
/// ## Where this comes from, and why it is not a curated skill table
///
/// A CC row's `skill_id` is almost never the skill that was cast. arcdps
/// substitutes one of its own generic control ids (the `23294..=23307`
/// band in [`crate::analysis::skill_symbol_names`]), so the log names the
/// EFFECT and discards the cause. Measured on real WvW logs: every one of
/// the 674 and 908 incoming-CC rows in two 40-player fights carried a
/// generic id and not one carried a real skill.
///
/// That is a loss and a gift at once. The cause is gone -- there is no
/// "pulled by Spectral Grasp" to be had from these rows. But the kind is
/// free, and it is the thing [`crate::analysis::self_effects`] cannot
/// give: the instantaneous effects produce no buff apply/remove pair, so
/// this band is the ONLY evidence in the log that a knockdown or a launch
/// happened at all. See this module's header for why they are absent from
/// [`CONTROL_EFFECTS`].
///
/// The table is arcdps' own, not a guess. Ids outside it -- including the
/// non-control generics that sit either side of the band -- return `None`
/// rather than being forced into a bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Knockdown,
    /// Knockback and pull share ONE arcdps id (23295), so they cannot be
    /// told apart here. The distinction is real and visible -- a pull
    /// moves the victim toward the caster, a knockback away -- but it
    /// lives in position data, not in this row. A consumer that needs it
    /// must measure displacement against the caster and treat this
    /// variant as the confirmation that a displacement happened at all.
    KnockbackOrPull,
    Launch,
    Float,
    Sink,
    /// arcdps' `Generic Water Float Sink` (23298) names both directions at
    /// once, exactly as `KnockbackOrPull` does. Kept fused for the same
    /// reason: the log does not say which.
    FloatOrSink,
    Fear,
    /// arcdps' `Generic Stagger` (23300).
    Stagger,
    /// The duration half: [`STUN`] and [`DAZE`]. arcdps reports these
    /// through two generics (`Generic CC Buff` and `Generic Lock Out`),
    /// neither of which says which of the two it was -- unlike the
    /// instantaneous kinds above, though, these DO leave a buff trail, so
    /// `self_effects` can answer that question where this cannot.
    StunOrDaze,
}

impl ControlKind {
    /// The stable wire spelling, as `catalogs.skills[].control_kind`
    /// carries it. Snake case to match every other key in the format; the
    /// fused variants say so in the name rather than picking a side.
    pub fn as_str(self) -> &'static str {
        match self {
            ControlKind::Knockdown => "knockdown",
            ControlKind::KnockbackOrPull => "knockback_or_pull",
            ControlKind::Launch => "launch",
            ControlKind::Float => "float",
            ControlKind::Sink => "sink",
            ControlKind::FloatOrSink => "float_or_sink",
            ControlKind::Fear => "fear",
            ControlKind::Stagger => "stagger",
            ControlKind::StunOrDaze => "stun_or_daze",
        }
    }
}

/// arcdps' generic control ids -> the effect they stand for.
///
/// Ordered by id. Deliberately a table and not a range test: the
/// `23294..=23307` band interleaves control effects with `Generic Kill`,
/// `Generic Evade`, `Generic Emote` and friends, which a range would
/// wrongly classify as crowd control.
const GENERIC_CONTROL_IDS: [(u32, ControlKind); 11] = [
    (23294, ControlKind::Knockdown),
    (23295, ControlKind::KnockbackOrPull),
    (23296, ControlKind::Float),
    (23297, ControlKind::Launch),
    (23298, ControlKind::FloatOrSink),
    (23299, ControlKind::StunOrDaze),
    (23300, ControlKind::Stagger),
    (23304, ControlKind::Float),
    (23305, ControlKind::Sink),
    (23306, ControlKind::StunOrDaze),
    (23307, ControlKind::Fear),
];

/// Classify a CC row's `skill_id`. `None` for anything that is not one of
/// arcdps' generic control ids -- including a genuine skill id, which
/// carries no control-kind information of its own.
pub fn control_kind(skill_id: u32) -> Option<ControlKind> {
    GENERIC_CONTROL_IDS
        .iter()
        .find(|(id, _)| *id == skill_id)
        .map(|(_, kind)| *kind)
}

#[cfg(test)]
mod control_kind_tests {
    use super::*;

    #[test]
    fn names_the_instantaneous_effects_arcdps_reports_generically() {
        assert_eq!(control_kind(23294), Some(ControlKind::Knockdown));
        assert_eq!(control_kind(23297), Some(ControlKind::Launch));
        assert_eq!(control_kind(23307), Some(ControlKind::Fear));
        assert_eq!(control_kind(23305), Some(ControlKind::Sink));
    }

    #[test]
    fn keeps_knockback_and_pull_fused_because_arcdps_does_not_separate_them() {
        // 23295 is arcdps' `Generic Knockback Pull` -- ONE id for both.
        // Splitting it here would be inventing a distinction the log does
        // not carry; a consumer that needs it must look at displacement.
        assert_eq!(control_kind(23295), Some(ControlKind::KnockbackOrPull));
    }

    #[test]
    fn maps_the_duration_effects_onto_the_buffs_this_module_already_knows() {
        // The lock-out / cc-buff generics are the Stun and Daze family --
        // the two ids `CONTROL_EFFECTS` carries.
        assert_eq!(control_kind(23299), Some(ControlKind::StunOrDaze));
        assert_eq!(control_kind(23306), Some(ControlKind::StunOrDaze));
    }

    #[test]
    fn returns_none_for_a_real_skill_id() {
        // A CC row carrying a genuine skill id (rather than a generic)
        // classifies to nothing rather than being forced into a bucket.
        assert_eq!(control_kind(5491), None);
    }

    #[test]
    fn returns_none_for_the_neighbouring_non_control_generics() {
        // The generic range is contiguous and mostly NOT control: a
        // range check instead of a table would sweep these in.
        for id in [
            23288, 23289, 23290, 23291, 23292, 23293, 23301, 23302, 23303, 23308,
        ] {
            assert_eq!(control_kind(id), None, "id {id} is not a control effect");
        }
    }
}

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
