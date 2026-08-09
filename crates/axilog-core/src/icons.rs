//! Profession / elite-specialization icon URLs, matching Elite Insights'
//! own table (M15 Task 2).
//!
//! # What EI does
//!
//! `SingleActorCombatReplayDescription`'s constructor
//! (`GW2EIEvtcParser/EIData/CombatReplay/CombatReplayDescription/Actors/
//! SingleActorCombatReplayDescription.cs:54`) sets the exported
//! `combatReplayData.iconURL` from
//!
//! ```csharp
//! Img = actor.GetIcon(true);
//! ```
//!
//! and `PlayerActor.GetIcon` (`EIData/Actors/PlayerActor.cs:53-56`) is
//!
//! ```csharp
//! public override string GetIcon(bool forceLowResolutionIfApplicable = false)
//! {
//!     return !IsFriendlyPlayer && !forceLowResolutionIfApplicable
//!         ? GetHighResolutionProfIcon(Spec)
//!         : GetProfIcon(Spec);
//! }
//! ```
//!
//! The `true` argument matters: combat replay ALWAYS takes the
//! base-resolution branch, for enemy players as well as squad players. So
//! the only table the replay needs is `ParserIcons.BaseResProfIcons` -- the
//! parallel `HighResProfIcons` table is used for the HTML report's player
//! list, not for `iconURL`, and is deliberately not mirrored here.
//!
//! `ParserHelper.GetProfIcon` (`ParserHelpers/ParserHelper.cs:468-471`):
//!
//! ```csharp
//! internal static string GetProfIcon(Spec spec)
//! {
//!     return ParserIcons.BaseResProfIcons.TryGetValue(spec, out var icon)
//!         ? icon : ParserIcons.UnknownProfessionIcon;
//! }
//! ```
//!
//! -- i.e. an unknown spec falls back to
//! [`UNKNOWN_PROFESSION_ICON`], never to the base profession's icon.
//! [`prof_icon_url`] reproduces that exactly.
//!
//! # Keying
//!
//! EI's `Spec` enum is flat: a player's `Spec` is their elite
//! specialization if they have one, otherwise their core profession (and
//! EI's exported `players[].profession` string is that same `Spec` name).
//! axilog splits the two into `Player::profession` + `Player::elite_spec`,
//! so [`prof_icon_url`] recombines them the same way EI does.
//!
//! # Source
//!
//! Every URL below is transcribed verbatim from
//! `GW2EIEvtcParser/ParserHelpers/Images/ParserIcons.cs` -- the
//! `BaseResProfIcons` dictionary (`:824-880`) and the `BaseRes*` constants
//! it references (`:106-160`), at GW2EI revision `7a6fe03` (2026-08-09).
//! All 9 core professions and all 36 elite specs EI knows about (HoT / PoF
//! / EoD / SotO / post-SotO) are covered -- 45 entries, matching
//! `BaseResProfIcons.Count`.
//!
//! # Calibration
//!
//! `icons_match_ei_reference` in `crates/axilog-core/tests/
//! ei_replay_golden.rs` joins the local post-rework EI export's
//! `players[].profession` to its `players[].combatReplayData.iconURL` and
//! asserts an EXACT string match against this table for every one of the
//! 16 distinct specs present in that log (Amalgam, Berserker, Bladesworn,
//! Chronomancer, Dragonhunter, Druid, Firebrand, Luminary, Necromancer,
//! Paragon, Reaper, Soulbeast, Spellbreaker, Tempest, Troubadour, Untamed).
//! The remaining 29 entries are covered by this module's own unit tests
//! against the transcription.

/// `ParserIcons.UnknownProfessionIcon` (`ParserIcons.cs:48`) -- what EI
/// falls back to for a `Spec` it has no icon for.
pub const UNKNOWN_PROFESSION_ICON: &str = "https://i.imgur.com/UbvyFSt.png";

/// `ParserIcons.BaseResProfIcons`, as `(spec name, url)` pairs in EI's own
/// declaration order (grouped by profession, elite specs newest-first, core
/// profession last).
///
/// The names are EI's `Spec` enum member names, which are also the strings
/// EI exports in `players[].profession` and the strings
/// `crate::model::profession_name` produces.
pub const BASE_RES_PROF_ICONS: &[(&str, &str)] = &[
    // Ranger
    ("Galeshot", "https://i.imgur.com/4wTs28o.png"),
    ("Untamed", "https://i.imgur.com/u8l36Pw.png"),
    ("Soulbeast", "https://i.imgur.com/1uDdNtU.png"),
    ("Druid", "https://i.imgur.com/Glb39dj.png"),
    ("Ranger", "https://i.imgur.com/r7TAcjS.png"),
    // Engineer
    ("Amalgam", "https://i.imgur.com/SjSb5yJ.png"),
    ("Mechanist", "https://i.imgur.com/1jUOMlX.png"),
    ("Holosmith", "https://i.imgur.com/Q96yagv.png"),
    ("Scrapper", "https://i.imgur.com/Cd9yD43.png"),
    ("Engineer", "https://i.imgur.com/hckhnZy.png"),
    // Thief
    ("Antiquary", "https://i.imgur.com/R1f6iXn.png"),
    ("Specter", "https://i.imgur.com/nVAyYVQ.png"),
    ("Deadeye", "https://i.imgur.com/kryyJRy.png"),
    ("Daredevil", "https://i.imgur.com/RiCJalE.png"),
    ("Thief", "https://i.imgur.com/dS8un97.png"),
    // Elementalist
    ("Evoker", "https://i.imgur.com/Ie4y9Qf.png"),
    ("Catalyst", "https://i.imgur.com/2B73rSk.png"),
    ("Weaver", "https://i.imgur.com/03RLBaX.png"),
    ("Tempest", "https://i.imgur.com/FnLyZvk.png"),
    ("Elementalist", "https://i.imgur.com/2ybEpCV.png"),
    // Mesmer
    ("Troubadour", "https://i.imgur.com/xRdE1iN.png"),
    ("Virtuoso", "https://i.imgur.com/sncfljQ.png"),
    ("Mirage", "https://i.imgur.com/fL88z7p.png"),
    ("Chronomancer", "https://i.imgur.com/rI1tW64.png"),
    ("Mesmer", "https://i.imgur.com/FXgZQ46.png"),
    // Necromancer
    ("Ritualist", "https://i.imgur.com/S8msdHU.png"),
    ("Harbinger", "https://i.imgur.com/PwhIT4u.png"),
    ("Scourge", "https://i.imgur.com/uVdgw3H.png"),
    ("Reaper", "https://i.imgur.com/X463V90.png"),
    ("Necromancer", "https://i.imgur.com/kK3l1C1.png"),
    // Warrior
    ("Paragon", "https://i.imgur.com/Wp4lhTM.png"),
    ("Bladesworn", "https://i.imgur.com/mFzTJXv.png"),
    ("Spellbreaker", "https://i.imgur.com/A6JTWBV.png"),
    ("Berserker", "https://i.imgur.com/dNY6e8n.png"),
    ("Warrior", "https://i.imgur.com/ejI5STj.png"),
    // Guardian
    ("Luminary", "https://i.imgur.com/1znO8HP.png"),
    ("Willbender", "https://i.imgur.com/pIFrNLa.png"),
    ("Firebrand", "https://i.imgur.com/TOsmJOl.png"),
    ("Dragonhunter", "https://i.imgur.com/GqKocpf.png"),
    ("Guardian", "https://i.imgur.com/l329bR4.png"),
    // Revenant
    ("Conduit", "https://i.imgur.com/qaXHsQU.png"),
    ("Vindicator", "https://i.imgur.com/hKBqtWE.png"),
    ("Renegade", "https://i.imgur.com/whOAxsp.png"),
    ("Herald", "https://i.imgur.com/O7kekkb.png"),
    ("Revenant", "https://i.imgur.com/lvp7545.png"),
];

/// EI's `Spec` for an axilog player: the elite specialization when there is
/// one, otherwise the core profession.
///
/// `elite_spec` may be a bare number (`crate::model::profession_name`
/// stringifies specialization ids it doesn't know); such a value simply
/// misses the table and falls back like any other unknown `Spec`.
pub fn spec_name<'a>(profession: &'a str, elite_spec: &'a str) -> &'a str {
    if elite_spec.is_empty() { profession } else { elite_spec }
}

/// `ParserHelper.GetProfIcon(Spec)`: the base-resolution icon URL for a
/// spec name, or [`UNKNOWN_PROFESSION_ICON`] if the name is not one of EI's
/// 45 specs.
pub fn spec_icon_url(spec: &str) -> &'static str {
    BASE_RES_PROF_ICONS
        .iter()
        .find(|(name, _)| *name == spec)
        .map_or(UNKNOWN_PROFESSION_ICON, |(_, url)| *url)
}

/// What EI puts in a player's `combatReplayData.iconURL`, from axilog's
/// split profession/elite-spec pair. See this module's doc comment for the
/// `GetIcon(true)` derivation.
pub fn prof_icon_url(profession: &str, elite_spec: &str) -> &'static str {
    spec_icon_url(spec_name(profession, elite_spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// EI's `BaseResProfIcons` has one entry per `Spec`; a duplicated name
    /// here would silently shadow, and a duplicated URL would mean a
    /// transcription copy/paste slip.
    #[test]
    fn table_is_a_bijection_of_the_expected_size() {
        let names: BTreeSet<&str> = BASE_RES_PROF_ICONS.iter().map(|(n, _)| *n).collect();
        let urls: BTreeSet<&str> = BASE_RES_PROF_ICONS.iter().map(|(_, u)| *u).collect();
        assert_eq!(BASE_RES_PROF_ICONS.len(), 45, "9 professions x (4 elites + core)");
        assert_eq!(names.len(), 45, "duplicate spec name");
        assert_eq!(urls.len(), 45, "duplicate icon URL");
        assert!(!urls.contains(UNKNOWN_PROFESSION_ICON), "fallback must be distinct");
        for (name, url) in BASE_RES_PROF_ICONS {
            assert!(url.starts_with("https://i.imgur.com/"), "{name}: {url}");
            assert!(url.ends_with(".png"), "{name}: {url}");
        }
    }

    /// All 9 core professions must be present under their bare name --
    /// that's the lookup used for a player with no elite spec.
    #[test]
    fn every_core_profession_resolves() {
        for prof in [
            "Guardian",
            "Warrior",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
            "Revenant",
        ] {
            assert_ne!(
                prof_icon_url(prof, ""),
                UNKNOWN_PROFESSION_ICON,
                "core profession {prof} missing from the table"
            );
        }
    }

    /// Spot-checks straight off `ParserIcons.cs`, one per profession,
    /// spanning every expansion generation.
    #[test]
    fn spot_checks_against_parser_icons() {
        // BaseResBerserker (HoT, Warrior)
        assert_eq!(prof_icon_url("Warrior", "Berserker"), "https://i.imgur.com/dNY6e8n.png");
        // BaseResFirebrand (PoF, Guardian)
        assert_eq!(prof_icon_url("Guardian", "Firebrand"), "https://i.imgur.com/TOsmJOl.png");
        // BaseResMechanist (EoD, Engineer)
        assert_eq!(prof_icon_url("Engineer", "Mechanist"), "https://i.imgur.com/1jUOMlX.png");
        // BaseResUntamed (SotO, Ranger)
        assert_eq!(prof_icon_url("Ranger", "Untamed"), "https://i.imgur.com/u8l36Pw.png");
        // BaseResConduit (post-SotO, Revenant)
        assert_eq!(prof_icon_url("Revenant", "Conduit"), "https://i.imgur.com/qaXHsQU.png");
        // BaseResRitualist (post-SotO, Necromancer)
        assert_eq!(prof_icon_url("Necromancer", "Ritualist"), "https://i.imgur.com/S8msdHU.png");
        // core, no elite spec
        assert_eq!(prof_icon_url("Thief", ""), "https://i.imgur.com/dS8un97.png");
    }

    /// The elite spec wins over the profession, and an elite spec EI does
    /// not know falls back to the UNKNOWN icon rather than to the core
    /// profession's -- matching `GetProfIcon`'s single dictionary lookup.
    #[test]
    fn unknown_spec_falls_back_without_degrading_to_the_core_profession() {
        assert_eq!(
            prof_icon_url("Warrior", "Berserker"),
            spec_icon_url("Berserker"),
            "elite spec takes precedence"
        );
        // `profession_name` stringifies specialization ids it lacks a name
        // for; such a player gets EI's unknown icon.
        assert_eq!(prof_icon_url("Warrior", "9999"), UNKNOWN_PROFESSION_ICON);
        assert_ne!(prof_icon_url("Warrior", "9999"), prof_icon_url("Warrior", ""));
        assert_eq!(prof_icon_url("", ""), UNKNOWN_PROFESSION_ICON);
        assert_eq!(prof_icon_url("Chef", ""), UNKNOWN_PROFESSION_ICON);
    }

    #[test]
    fn spec_name_recombines_axilogs_split_pair() {
        assert_eq!(spec_name("Warrior", "Berserker"), "Berserker");
        assert_eq!(spec_name("Warrior", ""), "Warrior");
    }
}
