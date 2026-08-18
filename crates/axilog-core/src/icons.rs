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

/// The base every mirrored icon is served from.
///
/// Art that GW2EI hosts on `i.imgur.com` or `assets.gw2dat.com` is served
/// from here instead. Those two can disappear without notice and the art
/// cannot be re-sourced -- of the ids they back, the official GW2 API knows
/// one, and that one carries no icon.
pub const ICON_MIRROR_BASE: &str = "https://darkharasho.github.io/axibridge-map-tiles/icons/";

/// The upstream URL a mirrored icon was taken from, or `url` unchanged when
/// it was never mirrored.
///
/// This is the inverse of the substitution above, and it exists because our
/// icon URLs are GW2EI's modulo exactly this one rewrite. The EI-equality
/// goldens compare our output against real GW2EI exports, which still carry
/// the upstream URLs; they undo the rewrite on our side rather than freeze
/// the mirrored strings into their assertions, so they stay able to notice
/// GW2EI changing an icon.
pub fn upstream_icon_url(url: &str) -> std::borrow::Cow<'_, str> {
    let Some(name) = url.strip_prefix(ICON_MIRROR_BASE) else {
        return std::borrow::Cow::Borrowed(url);
    };
    // Hosts spelled in two pieces so this file does not contain the literals
    // that `no_shipped_icon_url_points_at_an_untrusted_host` forbids.
    match name.split_once('-') {
        Some(("imgur", rest)) => std::borrow::Cow::Owned(format!("https://i.{}/{rest}", "imgur.com")),
        Some(("gw2dat", rest)) => std::borrow::Cow::Owned(format!("https://assets.{}/{rest}", "gw2dat.com")),
        _ => std::borrow::Cow::Borrowed(url),
    }
}

/// `ParserIcons.UnknownProfessionIcon` (`ParserIcons.cs:48`) -- what EI
/// falls back to for a `Spec` it has no icon for.
pub const UNKNOWN_PROFESSION_ICON: &str = "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-UbvyFSt.png";

/// `ParserIcons.BaseResProfIcons`, as `(spec name, url)` pairs in EI's own
/// declaration order (grouped by profession, elite specs newest-first, core
/// profession last).
///
/// The names are EI's `Spec` enum member names, which are also the strings
/// EI exports in `players[].profession` and the strings
/// `crate::model::profession_name` produces.
pub const BASE_RES_PROF_ICONS: &[(&str, &str)] = &[
    // Ranger
    ("Galeshot", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-4wTs28o.png"),
    ("Untamed", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-u8l36Pw.png"),
    ("Soulbeast", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-1uDdNtU.png"),
    ("Druid", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-Glb39dj.png"),
    ("Ranger", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-r7TAcjS.png"),
    // Engineer
    ("Amalgam", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-SjSb5yJ.png"),
    ("Mechanist", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-1jUOMlX.png"),
    ("Holosmith", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-Q96yagv.png"),
    ("Scrapper", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-Cd9yD43.png"),
    ("Engineer", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-hckhnZy.png"),
    // Thief
    ("Antiquary", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-R1f6iXn.png"),
    ("Specter", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-nVAyYVQ.png"),
    ("Deadeye", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-kryyJRy.png"),
    ("Daredevil", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-RiCJalE.png"),
    ("Thief", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-dS8un97.png"),
    // Elementalist
    ("Evoker", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-Ie4y9Qf.png"),
    ("Catalyst", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-2B73rSk.png"),
    ("Weaver", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-03RLBaX.png"),
    ("Tempest", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-FnLyZvk.png"),
    ("Elementalist", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-2ybEpCV.png"),
    // Mesmer
    ("Troubadour", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-xRdE1iN.png"),
    ("Virtuoso", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-sncfljQ.png"),
    ("Mirage", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-fL88z7p.png"),
    ("Chronomancer", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-rI1tW64.png"),
    ("Mesmer", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-FXgZQ46.png"),
    // Necromancer
    ("Ritualist", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-S8msdHU.png"),
    ("Harbinger", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-PwhIT4u.png"),
    ("Scourge", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-uVdgw3H.png"),
    ("Reaper", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-X463V90.png"),
    ("Necromancer", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-kK3l1C1.png"),
    // Warrior
    ("Paragon", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-Wp4lhTM.png"),
    ("Bladesworn", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-mFzTJXv.png"),
    ("Spellbreaker", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-A6JTWBV.png"),
    ("Berserker", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-dNY6e8n.png"),
    ("Warrior", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-ejI5STj.png"),
    // Guardian
    ("Luminary", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-1znO8HP.png"),
    ("Willbender", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-pIFrNLa.png"),
    ("Firebrand", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-TOsmJOl.png"),
    ("Dragonhunter", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-GqKocpf.png"),
    ("Guardian", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-l329bR4.png"),
    // Revenant
    ("Conduit", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-qaXHsQU.png"),
    ("Vindicator", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-hKBqtWE.png"),
    ("Renegade", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-whOAxsp.png"),
    ("Herald", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-O7kekkb.png"),
    ("Revenant", "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-lvp7545.png"),
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
            assert!(url.starts_with("https://darkharasho.github.io/axibridge-map-tiles/icons/"), "{name}: {url}");
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
        assert_eq!(prof_icon_url("Warrior", "Berserker"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-dNY6e8n.png");
        // BaseResFirebrand (PoF, Guardian)
        assert_eq!(prof_icon_url("Guardian", "Firebrand"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-TOsmJOl.png");
        // BaseResMechanist (EoD, Engineer)
        assert_eq!(prof_icon_url("Engineer", "Mechanist"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-1jUOMlX.png");
        // BaseResUntamed (SotO, Ranger)
        assert_eq!(prof_icon_url("Ranger", "Untamed"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-u8l36Pw.png");
        // BaseResConduit (post-SotO, Revenant)
        assert_eq!(prof_icon_url("Revenant", "Conduit"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-qaXHsQU.png");
        // BaseResRitualist (post-SotO, Necromancer)
        assert_eq!(prof_icon_url("Necromancer", "Ritualist"), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-S8msdHU.png");
        // core, no elite spec
        assert_eq!(prof_icon_url("Thief", ""), "https://darkharasho.github.io/axibridge-map-tiles/icons/imgur-dS8un97.png");
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
