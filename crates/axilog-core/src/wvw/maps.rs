//! The WvW map table -- axilog's SINGLE source of truth for per-map
//! combat-replay geometry, display names and arena image URLs (M15 Task 2).
//!
//! # Why one table
//!
//! Before M15 Task 2 the same five map ids were spelled out in three
//! places: [`crate::wvw::map_name`]'s display-name match, the
//! `MapTransform::for_map_id` constants added by M15 Task 1, and the
//! hand-specialized Alpine Borderland rect in
//! `crates/axilog-core/tests/replay_golden.rs`. Those three could drift.
//! They now all read [`map_def`].
//!
//! # Source
//!
//! Every value below is transcribed from the GW2EI clone at the revision
//! this module was written against (`7a6fe03`, 2026-08-09):
//!
//! ## Map ids -- `GW2EIEvtcParser/ParserHelpers/IDs/MapIDs.cs:6-12`
//!
//! ```csharp
//! public const int EternalBattleground   = 38;
//! public const int GreenAlpineBorderland = 95;
//! public const int BlueAlpineBorderland  = 96;
//! public const int RedDesertBorderland   = 1099;
//! public const int ObsidianSanctum       = 899;
//! public const int EdgeOfTheMists        = 968;
//! public const int ArmisticeBastion      = 1315;
//! ```
//!
//! ## Geometry + arena image -- `GW2EIEvtcParser/LogLogic/WvW/WvWLogic.cs`, `GetCombatMapInternal`
//!
//! ```csharp
//! var lifespan = (log.LogData.LogStart, log.LogData.LogEnd);
//! switch (mapID.MapID)
//! {
//!     case EternalBattleground:
//!         crMap = new CombatReplayMap((954, 1000),
//!             (-36864 + 950, -36864 + 2250, 36864 + 950, 36864 + 2250),
//!             (-36864, -36864, 36864, 36864),
//!             (8958, 12798, 12030, 15870));
//!         arenaDecorations.Add(new ArenaDecoration(lifespan, CombatReplayEternalBattlegrounds, crMap));
//!         break;
//!     case GreenAlpineBorderland:
//!         crMap = new CombatReplayMap((697, 1000),
//!             (-30720, -43008, 30720, 43008),
//!             (-30720, -43008, 30720, 43008),
//!             (5630, 11518, 8190, 15102));
//!         arenaDecorations.Add(new ArenaDecoration(lifespan, CombatReplayAlpineBorderlands, crMap));
//!         break;
//!     case BlueAlpineBorderland:
//!         crMap = new CombatReplayMap((697, 1000),
//!             (-30720, -43008, 30720, 43008),
//!             (-30720, -43008, 30720, 43008),
//!             (12798, 10878, 15358, 14462));
//!         arenaDecorations.Add(new ArenaDecoration(lifespan, CombatReplayAlpineBorderlands, crMap));
//!         break;
//!     case RedDesertBorderland:
//!         crMap = new CombatReplayMap((1000, 1000),
//!             (-36864, -36864, 36864, 36864),
//!             (-36864, -36864, 36864, 36864),
//!             (9214, 8958, 12286, 12030));
//!         arenaDecorations.Add(new ArenaDecoration(lifespan, CombatReplayDesertBorderlands, crMap));
//!         break;
//!     case EdgeOfTheMists:
//!         crMap = new CombatReplayMap((3556, 3646), (-36864, -36864, 36864, 36864));
//!         arenaDecorations.Add(new ArenaDecoration(lifespan, CombatReplayEdgeOfTheMists, crMap));
//!         break;
//!     default:
//!         crMap = base.GetCombatMapInternal(log, arenaDecorations);
//!         break;
//! }
//! ```
//!
//! Only the first two `CombatReplayMap` constructor arguments matter here:
//! `_pixelSize` (the arena image's native pixel size) and `_rectInMap` (the
//! world/game-inch rect that image covers). The 3rd and 4th arguments are
//! the "full map" rect and the default viewpoint rect, used only by the
//! HTML replay's camera; axilog does not export a camera.
//!
//! **`ObsidianSanctum` (899) and `ArmisticeBastion` (1315) are deliberately
//! absent.** GW2EI names those map ids but has no `GetCombatMapInternal`
//! case for them, and their `LogImages` combat-replay constants are the
//! empty string (`CombatReplayObsidianSanctum = ""`,
//! `CombatReplayArmisticeBastion = ""`, `LogImages.cs:243,245`) -- they
//! fall through to the `default` branch, i.e. the computed bounding box.
//! Ditto any other map id: [`map_def`] returns `None`, and the caller falls
//! back to `ei_replay::bounding_box_transform`.
//!
//! ## Arena image URLs -- `GW2EIEvtcParser/ParserHelpers/Images/LogImages.cs:239-245`
//!
//! ```csharp
//! // Combat Replay Maps
//! internal const string CombatReplayEternalBattlegrounds = "https://i.imgur.com/t0khtQd.png";
//! internal const string CombatReplayAlpineBorderlands    = "https://i.imgur.com/nVu2ivF.png";
//! internal const string CombatReplayDesertBorderlands    = "https://i.imgur.com/R5p9fqw.png";
//! internal const string CombatReplayObsidianSanctum      = "";
//! internal const string CombatReplayEdgeOfTheMists       = "https://i.imgur.com/iEpKYL0.jpg";
//! internal const string CombatReplayArmisticeBastion     = "";
//! ```
//!
//! (Note Edge of the Mists' `.jpg`, and that Green and Blue Alpine share
//! one image -- their `_pixelSize`/`_rectInMap` are identical too; only the
//! default-viewpoint rect differs.)
//!
//! ## Display names
//!
//! Not a GW2EI constant table: GW2EI builds them by string concatenation in
//! `WvWLogic.GetLogicName` (`"World vs World" + " - Eternal
//! Battlegrounds"`, ...). The short forms below are what axilog has emitted
//! as `encounter.map` since M2 Task 2 and are preserved verbatim; unknown
//! ids keep falling back to the generic `"World vs World"` (see
//! [`crate::wvw::map_name`]).

/// One WvW map's static combat-replay definition.
///
/// Field-for-field: `map_id` is the `CBTS_MAPID` payload, `name` is
/// axilog's `encounter.map` label, and the rest are GW2EI's
/// `CombatReplayMap` constructor arguments plus the arena image the map's
/// `ArenaDecoration` is built with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WvwMapDef {
    /// `CBTS_MAPID` (`sc::MAP_ID`) `src_agent` value.
    pub map_id: u32,
    /// axilog's `encounter.map` display name.
    pub name: &'static str,
    /// GW2EI's `_pixelSize` -- the arena image's native `(width, height)`
    /// in pixels. NOT the exported `combatReplayMetaData.sizes`, which is
    /// this rescaled to a 750px max dimension by
    /// `CombatReplayMap.GetPixelMapSize()`.
    pub pixel_size: (u32, u32),
    /// GW2EI's `_rectInMap` as `(topX, topY, bottomX, bottomY)` in world
    /// (game-inch) units. `topY` is the image's BOTTOM edge in world terms:
    /// world y grows northward, image y grows downward.
    pub rect: (f64, f64, f64, f64),
    /// The `ArenaDecoration`'s image URL -- GW2EI's exported
    /// `combatReplayMetaData.maps[].url`.
    pub image_url: &'static str,
}

/// GW2EI's five WvW maps that have a hand-authored arena image, in map-id
/// order. See this module's doc comment for the per-value citations.
pub const WVW_MAPS: &[WvwMapDef] = &[
    WvwMapDef {
        map_id: 38,
        name: "Eternal Battlegrounds",
        pixel_size: (954, 1000),
        // GW2EI writes these as `-36864 + 950` etc.; kept as the evaluated
        // constants so the table stays a table.
        rect: (-35914.0, -34614.0, 37814.0, 39114.0),
        image_url: "https://i.imgur.com/t0khtQd.png",
    },
    WvwMapDef {
        map_id: 95,
        name: "Green Alpine Borderlands",
        pixel_size: (697, 1000),
        rect: (-30720.0, -43008.0, 30720.0, 43008.0),
        image_url: "https://i.imgur.com/nVu2ivF.png",
    },
    WvwMapDef {
        map_id: 96,
        name: "Blue Alpine Borderlands",
        pixel_size: (697, 1000),
        rect: (-30720.0, -43008.0, 30720.0, 43008.0),
        image_url: "https://i.imgur.com/nVu2ivF.png",
    },
    WvwMapDef {
        map_id: 968,
        name: "Edge of the Mists",
        pixel_size: (3556, 3646),
        rect: (-36864.0, -36864.0, 36864.0, 36864.0),
        image_url: "https://i.imgur.com/iEpKYL0.jpg",
    },
    WvwMapDef {
        map_id: 1099,
        name: "Red Desert Borderlands",
        pixel_size: (1000, 1000),
        rect: (-36864.0, -36864.0, 36864.0, 36864.0),
        image_url: "https://i.imgur.com/R5p9fqw.png",
    },
];

/// Look a map id up in [`WVW_MAPS`]. `None` for every id GW2EI routes to
/// `GetCombatMapInternal`'s `default` branch (including Obsidian Sanctum
/// and Armistice Bastion, which GW2EI names but has no image for) -- the
/// caller must then fall back to the computed bounding box.
///
/// Linear scan over five entries: cheaper than any map, and callable from a
/// `const` context's worth of hot paths without a lazy static.
pub fn map_def(map_id: u32) -> Option<&'static WvwMapDef> {
    WVW_MAPS.iter().find(|m| m.map_id == map_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_gw2ei_five_and_nothing_else() {
        let ids: Vec<u32> = WVW_MAPS.iter().map(|m| m.map_id).collect();
        assert_eq!(ids, vec![38, 95, 96, 968, 1099], "sorted by map id");
        // GW2EI names these two but has no arena image / no
        // `GetCombatMapInternal` case for them.
        assert!(map_def(899).is_none(), "Obsidian Sanctum -> bounding box");
        assert!(map_def(1315).is_none(), "Armistice Bastion -> bounding box");
        assert!(map_def(0).is_none());
        assert!(map_def(1).is_none());
    }

    #[test]
    fn ebg_rect_matches_gw2eis_offset_arithmetic() {
        let m = map_def(38).unwrap();
        assert_eq!(
            m.rect,
            (
                -36864.0 + 950.0,
                -36864.0 + 2250.0,
                36864.0 + 950.0,
                36864.0 + 2250.0
            ),
            "WvWLogic.cs EternalBattleground case"
        );
    }

    #[test]
    fn alpine_borderlands_share_geometry_and_image() {
        let green = map_def(95).unwrap();
        let blue = map_def(96).unwrap();
        assert_eq!(green.pixel_size, blue.pixel_size);
        assert_eq!(green.rect, blue.rect);
        assert_eq!(green.image_url, blue.image_url);
        assert_ne!(green.name, blue.name);
    }

    #[test]
    fn every_entry_has_a_nonempty_image_url() {
        for m in WVW_MAPS {
            assert!(
                m.image_url.starts_with("https://i.imgur.com/"),
                "{}: {}",
                m.map_id,
                m.image_url
            );
            assert!(m.pixel_size.0 > 0 && m.pixel_size.1 > 0);
            assert!(m.rect.2 > m.rect.0 && m.rect.3 > m.rect.1, "rect not degenerate");
        }
    }
}
