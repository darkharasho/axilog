//! The condition-skill-id catalog (MCONDCAT Task 1) -- this project's
//! reproduction of GW2EI's `SkillEvent.ConditionDamageBased(log)` predicate.
//!
//! Verified directly against GW2EI source (`baaron4/
//! GW2-Elite-Insights-Parser`, `master`, merge `7a6fe03`, read 2026-08-09).
//!
//! ## What GW2EI actually asks
//!
//! ```text
//! // GW2EIEvtcParser/ParsedData/CombatEvents/SkillEvent.cs:43-50
//! public bool ConditionDamageBased(ParsedEvtcLog log)
//! {
//!     if (_isCondi == -1 && log.Buffs.BuffsByIDs.TryGetValue(SkillID, out var b))
//!     {
//!         _isCondi = b.Classification == Buff.BuffClassification.Condition ? 1 : 0;
//!     }
//!     return _isCondi == 1;
//! }
//! ```
//!
//! i.e. **"is this damage row's SKILL ID registered as a
//! `BuffClassification.Condition` buff?"** -- a pure per-skill-id lookup,
//! with no reference to the row's `buff` byte, `result` byte, era, actor, or
//! anything else. (The `_isCondi == -1` guard is a per-event memoisation
//! cache, not a semantic condition: a miss in `BuffsByIDs` leaves `_isCondi`
//! at `-1` and the method returns `false`, re-probing on the next call.
//! There is no observable behaviour difference from a plain set-membership
//! test, which is what this module implements.)
//!
//! ## The complete static id set, with per-group citations
//!
//! An exhaustive scan of the GW2EI tree for `BuffClassification.Condition`
//! at a `Buff` CONSTRUCTION site (as opposed to a `Classification ==`
//! comparison site) finds **exactly one group**, all fourteen entries
//! contiguous in a single list:
//!
//! - **`GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:34-52`,
//!   `CommonBuffs.Conditions`** -- 14 `Buff` ctor calls tagged
//!   `BuffClassification.Condition` (lines 36-49). The 15th entry in that
//!   same list, `"Number of Conditions"` (`NumberOfConditions`, line 51), is
//!   tagged `BuffClassification.Other` DESPITE living in a list called
//!   `Conditions` -- deliberately EXCLUDED here, exactly as GW2EI excludes
//!   it. Each id cross-checked against `GW2EIEvtcParser/ParserHelpers/IDs/
//!   SkillIDs.cs` (line numbers in each constant's own doc below).
//!
//! There is NO second group. Every other `BuffClassification.Condition`
//! occurrence in the tree is a READ (`x.Classification ==
//! BuffClassification.Condition` / `BuffsByClassification[
//! BuffClassification.Condition]`), never a registration:
//! `EIData/Statistics/SupportStatistics.cs:53`,
//! `EIData/Statistics/DefensePerTargetStatistics.cs:150`,
//! `EIData/Statistics/StatisticsHelper.cs:26`,
//! `EIData/Statistics/GameplayStatistics.cs:131`,
//! `EIData/Actors/ActorsHelper/SingleActorBuffsHelper.cs:499,969`,
//! `GW2EIBuilders/JsonModels/JsonLogBuilder.cs:47`. In particular **no
//! profession/elite-spec helper (`EIData/ProfHelpers/*.cs`) registers a
//! single Condition-classified buff** -- every one of their `Buffs` lists
//! uses `Offensive`/`Defensive`/`Support`/`Debuff`/`Other`.
//!
//! ## Runtime membership (`BuffsByIDs`) vs the static set -- IDENTICAL here
//!
//! `BuffsByIDs` is NOT the raw static concatenation; it is built in
//! `GW2EIEvtcParser/EIData/Buffs/BuffsContainer.cs:21-149` as
//!
//! ```text
//! foreach (IReadOnlyList<Buff> buffs in AllBuffs)          // :96-99
//!     currentBuffs.AddRange(buffs.Where(x => x.Available(combatData)));
//! ... // :110-140  unknown-consumable synthesis, see below
//! BuffsByIDs = currentBuffs.GroupBy(x => x.ID).ToDictionary(...);  // :142-149
//! ```
//!
//! so membership is genuinely LOG-DEPENDENT in general, via two mechanisms:
//!
//! 1. **`Buff.Available(combatData)`** (`EIData/Buffs/Buff.cs:259-270`):
//!    `gw2Build ∈ [_minBuild, _maxBuild) && evtcBuild ∈ [_minEvtcBuild,
//!    _maxEvtcBuild)`. The bounds default to `StartOfLife`/`EndOfLife`
//!    (`Buff.cs:58-61`) and are only narrowed by an explicit
//!    `.WithBuilds(...)`/`.WithEvtcBuilds(...)` chained onto the ctor
//!    (`Buff.cs:125-137`). **None of the fourteen Condition entries carries
//!    either call** (verified line-by-line at `CommonBuffs.cs:36-49`;
//!    contrast the sibling Boon list, where e.g. `"Retaliation"` at
//!    `CommonBuffs.cs:27` DOES carry `.WithBuilds(GW2Builds.StartOfLife,
//!    GW2Builds.May2021Balance)`), so all fourteen are UNCONDITIONALLY
//!    available on every build, every era, every log.
//! 2. **Synthesised unknown consumables** (`BuffsContainer.cs:110-140`):
//!    ids seen in the log's own buff-info table but absent from the static
//!    lists get a `CreateCustomBuff(...)` entry -- but ONLY with
//!    `BuffClassification.Nourishment` (`:121`) or
//!    `BuffClassification.Enhancement` (`:137`). Neither can ever be
//!    `Condition`, so this path cannot ADD to the Condition set either.
//!
//! Two further guarantees make the set exact rather than approximate: the
//! `GroupBy(x => x.ID)` at `BuffsContainer.cs:142-149` THROWS
//! (`InvalidDataException`) on a duplicate id, so no other list can silently
//! shadow a Condition id with a differently-classified buff of the same id;
//! and the `#if DEBUG` reclassification at `Buff.cs:81-86` only rewrites
//! `Hidden` -> `Other`, never touching `Condition`.
//!
//! **Conclusion: runtime membership == the static set, unconditionally.** A
//! plain hardcoded 14-id table is a faithful reproduction, not a
//! simplification -- no era gating, no log-dependent probing, nothing to
//! recompute per log. This is the same "threshold/table far outside this
//! project's operating range, so hardcode it and SAY so" reasoning
//! `hit_stats::NON_CRITABLE_SKILLS` and `defenses::DODGE_SKILL_ID` already
//! establish, except here it is stronger: the set is build-independent by
//! construction, not merely build-independent in practice.
//!
//! ## Why this module exists (the bucket it unlocks)
//!
//! Before MCONDCAT, `hit_stats::classify`/`defenses::classify` approximated
//! `ConditionDamageBased` as "`buff == 1` and not life-leech", which merges
//! two genuinely distinct GW2EI buckets and, on a real post-rework capture,
//! diverged by up to ~35% relative on `condition_count`/`power_count`. See
//! those modules' own doc comments for the full ctor transcriptions and the
//! FOURTH BUCKET (`buff == 1`, outside this catalog, not life-leech ->
//! power-only) this catalog makes representable.

/// `SkillIDs.Bleeding` (`GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs:130`).
pub const BLEEDING: u32 = 736;
/// `SkillIDs.Burning` (`SkillIDs.cs:131`).
pub const BURNING: u32 = 737;
/// `SkillIDs.Confusion` (`SkillIDs.cs:151`).
pub const CONFUSION: u32 = 861;
/// `SkillIDs.Poison` (`SkillIDs.cs:126`).
pub const POISON: u32 = 723;
/// `SkillIDs.Torment` (`SkillIDs.cs:1328`).
pub const TORMENT: u32 = 19426;
/// `SkillIDs.Blind` (`SkillIDs.cs:123`).
pub const BLIND: u32 = 720;
/// `SkillIDs.Chilled` (`SkillIDs.cs:125`).
pub const CHILLED: u32 = 722;
/// `SkillIDs.Crippled` (`SkillIDs.cs:124`).
pub const CRIPPLED: u32 = 721;
/// `SkillIDs.Fear` (`SkillIDs.cs:146`).
pub const FEAR: u32 = 791;
/// `SkillIDs.Immobile` (`SkillIDs.cs:129`).
pub const IMMOBILE: u32 = 727;
/// `SkillIDs.Slow` (`SkillIDs.cs:1587`).
pub const SLOW: u32 = 26766;
/// `SkillIDs.Taunt` (`SkillIDs.cs:1609`).
pub const TAUNT: u32 = 27705;
/// `SkillIDs.Weakness` (`SkillIDs.cs:135`).
pub const WEAKNESS: u32 = 742;
/// `SkillIDs.Vulnerability` (`SkillIDs.cs:132`).
pub const VULNERABILITY: u32 = 738;

/// The COMPLETE set of skill ids GW2EI registers with
/// `Buff.BuffClassification.Condition` -- see this module's doc comment for
/// the exhaustive-scan provenance and the proof that runtime `BuffsByIDs`
/// membership can never differ from it.
///
/// Kept in **ascending id order** (NOT `CommonBuffs.Conditions`' own
/// declaration order) so `is_condition_damage_based`'s scan is
/// cache-friendly and the table is trivially eyeball-diffable against a
/// sorted extraction of the C# source.
pub const CONDITION_SKILL_IDS: [u32; 14] = [
    BLIND,         // 720
    CRIPPLED,      // 721
    CHILLED,       // 722
    POISON,        // 723
    IMMOBILE,      // 727
    BLEEDING,      // 736
    BURNING,       // 737
    VULNERABILITY, // 738
    WEAKNESS,      // 742
    FEAR,          // 791
    CONFUSION,     // 861
    TORMENT,       // 19426
    SLOW,          // 26766
    TAUNT,         // 27705
];

/// `SkillEvent.ConditionDamageBased(log)` (`SkillEvent.cs:43-50`) -- true
/// iff `skill_id` is registered as a `BuffClassification.Condition` buff.
///
/// A 14-element linear scan over a `const` array beats any hashed structure
/// at this size (it stays in one cache line's worth of L1 and needs no
/// pointer chase or hash), and is called once per already-filtered damage
/// row.
///
/// **Deliberately NOT gated on the row's `buff` byte.** GW2EI evaluates this
/// predicate FIRST, ahead of its `is NonDirectHealthDamageEvent` /
/// `is DirectHealthDamageEvent` type test, in BOTH damage ctors
/// (`OffensiveStatistics.cs:109`, `DefensePerTargetStatistics.cs`) -- so a
/// hypothetical `buff == 0` strike row carrying a catalogued skill id would
/// be counted as a CONDITION hit by GW2EI (never reaching the
/// strike/crit/flank/glance branch at all). This function reproduces that
/// ordering faithfully rather than second-guessing it; callers must apply it
/// before their own direct-vs-non-direct split.
#[inline]
pub fn is_condition_damage_based(skill_id: u32) -> bool {
    CONDITION_SKILL_IDS.contains(&skill_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catalog membership spot-check (MCONDCAT Task 1): one id from each end
    /// of the numeric range, one mid-range, plus the most important
    /// NON-members: a Boon (`Might`, 740, `SkillIDs.cs:134`) which sits
    /// BETWEEN two catalogued ids and would be swept up by any range-based
    /// shortcut, another Boon (`Regeneration`, 718, `SkillIDs.cs:121`) just
    /// below the catalog's low end, and an ordinary damaging strike skill
    /// id. (`NumberOfConditions`, the `Other`-classified 15th entry of
    /// `CommonBuffs.Conditions`, is `SkillIDs.cs:21`'s synthetic NEGATIVE
    /// pseudo-id `-4` -- unrepresentable in the `u32` skill ids this
    /// project decodes from the wire, so excluded structurally rather than
    /// by table membership.)
    #[test]
    fn catalog_membership_spot_check() {
        // Members, all fourteen.
        for id in CONDITION_SKILL_IDS {
            assert!(is_condition_damage_based(id), "{id} must be catalogued");
        }
        assert!(is_condition_damage_based(736), "Bleeding");
        assert!(is_condition_damage_based(720), "Blind (lowest id)");
        assert!(is_condition_damage_based(27705), "Taunt (highest id)");
        assert!(is_condition_damage_based(19426), "Torment");

        // Non-members.
        assert!(!is_condition_damage_based(740), "Might is a Boon, and sits between Vulnerability(738) and Weakness(742)");
        assert!(!is_condition_damage_based(718), "Regeneration is a Boon");
        assert!(!is_condition_damage_based(0), "id 0 is not a buff");
        assert!(!is_condition_damage_based(u32::MAX));
        assert!(!is_condition_damage_based(30770), "PulmonaryImpact is an ordinary strike skill");
    }

    /// The table must stay sorted + duplicate-free: `CommonBuffs.cs`'s own
    /// `GroupBy(x => x.ID)` throws on a duplicate id, and the sort order is
    /// what makes a machine diff against the C# source meaningful.
    #[test]
    fn catalog_is_sorted_and_deduplicated() {
        let mut sorted = CONDITION_SKILL_IDS;
        sorted.sort_unstable();
        assert_eq!(sorted, CONDITION_SKILL_IDS, "CONDITION_SKILL_IDS must be ascending");
        let unique: std::collections::BTreeSet<u32> = CONDITION_SKILL_IDS.iter().copied().collect();
        assert_eq!(unique.len(), CONDITION_SKILL_IDS.len(), "no duplicate ids");
    }
}
