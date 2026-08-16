//! WvW objective ownership timelines (MOBJ) -- `CBTS_WVWOBJECTIVESTATUS`
//! (sc=75), the last whole GW2EI feature surface axilog did not emit.
//!
//! ## What the event carries
//!
//! One event per objective per status update: which map, which objective,
//! and who owned it at that moment. Repeated updates for the same objective
//! are MERGED into one record with an appended owner timeline -- that
//! append-don't-replace shape is where the "ownership timeline" comes from.
//!
//! Field mapping, transcribed from GW2EI's `WvWObjectiveStatusEvent`
//! (`GW2EIEvtcParser/ParsedData/CombatEvents/StatusEvents/
//! WvWObjectiveStatusEvent.cs:24-31`):
//!
//! ```text
//! MapID               = evtcItem.Value
//! ObjectiveID         = (int)evtcItem.SkillID
//! AutoUpgradeProgress = evtcItem.Pad
//! Owners.Add(((uint)evtcItem.BuffDmg, evtcItem.Time))
//! ```
//!
//! `AutoUpgradeProgress` is parsed but does NOT reach the wire, in either
//! format: `JsonWvWMapDataBuilder` writes only `MapID`/`ObjectiveID`/
//! `ObjectiveType`/`Owners`, and EI's only other consumer
//! (`WvWHelper.GetObjectiveTier`) feeds the HTML tier badge, which axilog
//! has no analogue for. It is carried on [`ObjectiveStatus`] anyway because
//! [`RawEvent::pad`] is already decoded (M10 Task 1, for the healing-
//! extension signature), so keeping the parsed event whole is free and a
//! tier badge would otherwise need this module reopened.
//!
//! ## The catalog, and why unknown objectives vanish
//!
//! An objective's TYPE is not in the log. It comes from a static table
//! keyed by `(map_id, objective_id)` -- [`OBJECTIVES`], transcribed from
//! `GW2EIEvtcParser/ParserHelpers/WvWHelper.cs:161-268`. GW2EI drops any
//! status event whose `(map, objective)` pair is missing from that table
//! (`CombatEventFactory.cs:557-559`, `if (wvwObjectiveStatus.IsUnknown)
//! continue;`), so an incomplete catalog silently shortens the output
//! rather than emitting `"Unknown"` rows. [`objectives`] reproduces that
//! filter exactly, which is why the table is transcribed whole rather than
//! for the maps this project happens to have fixtures for.
//!
//! Verified against the reference EI export of a Blue Alpine capture: 13
//! objective records, `{mapID, objectiveID, objectiveType, owners}`, with
//! `mapID: 96` and objective ids 34/52/37 typed Camp/Camp/Keep -- exactly
//! this table's Blue Alpine rows for Demesne, Godslore and Blue Garrison.

use crate::evtc::{RawEvent, RawLog, sc};
use std::collections::BTreeMap;

/// GW2EI's `WvWHelper.ObjectiveType`. The `Unknown` variant is intentionally
/// absent: an unknown `(map, objective)` pair is not a typed objective in
/// this project's model, it is a dropped event -- see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectiveType {
    Camp,
    Ruins,
    Tower,
    Keep,
    Castle,
}

impl ObjectiveType {
    /// The wire name, from `WvWHelper.GetObjectiveTypeName`
    /// (`WvWHelper.cs:24-38`). EI's `Unknown` arm returns `"None"`; it is
    /// unreachable through this project's parse, since untyped objectives
    /// never become records.
    pub fn name(self) -> &'static str {
        match self {
            ObjectiveType::Camp => "Camp",
            ObjectiveType::Ruins => "Ruins",
            ObjectiveType::Tower => "Tower",
            ObjectiveType::Keep => "Keep",
            ObjectiveType::Castle => "Castle",
        }
    }
}

/// One row of the static objective catalog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectiveDef {
    /// `CBTS_MAPID` value -- see [`crate::wvw::maps`] for the id table.
    pub map_id: i32,
    /// Objective id, log-local to the map. Ids REPEAT across maps (both
    /// alpine borderlands number their garrison 37), which is why the
    /// catalog key is the pair and never the objective id alone.
    pub objective_id: i32,
    pub kind: ObjectiveType,
    /// GW2EI's `WvWObjectiveData.ContinentPosition`, `(x, y, z)`. Carried
    /// because it is the same source lines as the type and a second
    /// transcription pass is the expensive part; nothing reads it yet. It
    /// is the input to EI's objective combat-replay decorations
    /// (`GetPosition` -> `CombatReplayMap.ContinentCoordToMapCoord`), the
    /// natural consumer if replay objective markers ever land.
    pub continent_pos: (f32, f32, f32),
}

const fn obj(
    map_id: i32,
    objective_id: i32,
    kind: ObjectiveType,
    continent_pos: (f32, f32, f32),
) -> ObjectiveDef {
    ObjectiveDef { map_id, objective_id, kind, continent_pos }
}

use ObjectiveType::{Camp, Castle, Keep, Ruins, Tower};

/// Every WvW objective GW2EI knows, in its source order (grouped by map,
/// then by EI's own within-map grouping). Transcribed from
/// `WvWHelper.cs:161-268`; the trailing comment on each row is EI's own
/// objective name, kept so a future re-check against the source is a
/// line-by-line diff rather than a re-derivation.
pub const OBJECTIVES: &[ObjectiveDef] = &[
    // --- Blue Alpine Borderland (MapIDs.BlueAlpineBorderland = 96) ---
    obj(96, 39, Camp, (14082.8, 11228.4, -3676.89)),   // Spiritholm
    obj(96, 38, Tower, (13444.8, 12078.2, -3758.65)),  // Woodhaven
    obj(96, 37, Keep, (14056.6, 12430.9, -2800.76)),   // Blue Garrison
    obj(96, 40, Tower, (14683.4, 12030.3, -4839.9)),   // Dawn's Eyrie
    obj(96, 52, Camp, (13211.9, 12195.7, -46.1562)),   // Godslore
    obj(96, 51, Camp, (15025.5, 12168.3, -1533.33)),   // Stargrove
    obj(96, 64, Ruins, (13859.2, 12703.1, -393.0)),    // Baeur
    obj(96, 65, Ruins, (14327.1, 12757.0, -711.0)),    // Orchard
    obj(96, 63, Ruins, (13761.0, 13074.7, -0.649902)), // Battle
    obj(96, 66, Ruins, (14362.3, 13112.1, -2742.0)),   // Carver
    obj(96, 62, Ruins, (14065.1, 13339.5, -1168.0)),   // Lost Prayers
    obj(96, 33, Keep, (13035.1, 12956.6, -300.694)),   // Ascension
    obj(96, 35, Tower, (13688.9, 13339.0, -1892.9)),   // Redbriar
    obj(96, 53, Camp, (13262.3, 13457.1, -687.909)),   // Redvale
    obj(96, 32, Keep, (15252.2, 12880.7, -3107.91)),   // Askalion
    obj(96, 36, Tower, (14581.0, 13409.9, -1821.91)),  // Greenlake
    obj(96, 50, Camp, (15015.7, 13502.9, -10.3619)),   // Greenwater
    obj(96, 34, Camp, (14083.2, 14033.2, -307.1)),     // Demesne
    // --- Green Alpine Borderland (MapIDs.GreenAlpineBorderland = 95) ---
    obj(95, 39, Camp, (6914.78, 11868.4, -3676.89)),  // Titanpaw
    obj(95, 38, Tower, (6276.77, 12718.2, -3758.65)), // Sunnyhill
    obj(95, 37, Keep, (6888.59, 13070.9, -2800.76)),  // Green Garrison
    obj(95, 40, Tower, (7515.42, 12670.3, -4839.9)),  // Cragtop
    obj(95, 52, Camp, (6043.87, 12835.7, -46.1562)),  // Faithleap
    obj(95, 51, Camp, (7857.45, 12808.3, -1533.33)),  // Foghaven
    obj(95, 64, Ruins, (6691.21, 13343.1, -393.0)),   // Gzertzz
    obj(95, 65, Ruins, (7159.09, 13397.0, -711.0)),   // Cohen
    obj(95, 63, Ruins, (6593.0, 13714.7, -0.649902)), // Norfolk
    obj(95, 66, Ruins, (7194.27, 13752.1, -2742.0)),  // Patrick
    obj(95, 62, Ruins, (6897.13, 13979.5, -1168.0)),  // Fallen
    obj(95, 33, Keep, (5867.06, 13596.6, -300.694)),  // Dradfall Bay
    obj(95, 35, Tower, (6520.89, 13979.0, -1892.9)),  // Bluebriar
    obj(95, 53, Camp, (6094.29, 14097.1, -687.909)),  // Bluevale
    obj(95, 32, Keep, (8084.25, 13520.7, -3107.91)),  // Shadaran Hills
    obj(95, 36, Tower, (7413.05, 14049.9, -1821.91)), // Redlake
    obj(95, 50, Camp, (7847.75, 14142.9, -10.3619)),  // Redwater
    obj(95, 34, Camp, (6915.25, 14673.2, -307.1)),    // Hero's Lodge
    // --- Red Desert Borderland (MapIDs.RedDesertBorderland = 1099) ---
    obj(1099, 99, Camp, (10743.8, 9492.51, -2955.0)),     // Hamm
    obj(1099, 102, Tower, (9831.82, 9507.67, -2897.5)),   // O'del
    obj(1099, 113, Keep, (10776.6, 10120.4, -4120.01)),   // Stoic Rampart
    obj(1099, 104, Tower, (11739.2, 9654.33, -4452.81)),  // Eternal Necropolis
    obj(1099, 115, Camp, (9310.12, 10008.0, -1283.35)),   // Boettiger
    obj(1099, 109, Camp, (12097.5, 10018.3, -1025.05)),   // Roy's
    obj(1099, 122, Ruins, (10725.3, 10453.5, -235.954)),  // Tilly
    obj(1099, 119, Ruins, (10446.7, 10761.6, -620.871)),  // Bearce
    obj(1099, 120, Ruins, (10989.5, 10778.3, -941.543)),  // Zak
    obj(1099, 121, Ruins, (10399.4, 11059.5, -1255.37)),  // Darra
    obj(1099, 118, Ruins, (10913.3, 11198.2, -992.897)),  // Higgins'
    obj(1099, 106, Keep, (9327.72, 10634.1, -3714.37)),   // Blistering
    obj(1099, 110, Tower, (10243.9, 11331.3, -5557.72)),  // Parched
    obj(1099, 101, Camp, (9584.13, 11316.1, -3877.82)),   // Mclain
    obj(1099, 114, Keep, (12203.0, 10706.2, -4254.64)),   // Osprey
    obj(1099, 105, Tower, (11256.9, 11551.1, -5219.09)),  // Crankshaft
    obj(1099, 100, Camp, (11891.4, 11286.6, -4736.73)),   // Bauer
    obj(1099, 116, Camp, (10754.8, 11854.4, -2801.74)),   // Dustwhisper
    // --- Eternal Battlegrounds (MapIDs.EternalBattleground = 38) ---
    obj(38, 6, Camp, (9841.05, 13545.8, -508.295)),    // Speldan
    obj(38, 17, Tower, (10256.6, 13514.4, -2015.34)),  // Mendon
    obj(38, 18, Tower, (10188.8, 14082.3, -1657.95)),  // Anzalias
    obj(38, 1, Keep, (10763.6, 13655.8, -2464.89)),    // Red Overlook
    obj(38, 20, Tower, (11090.4, 13488.2, -2569.23)),  // Veloka
    obj(38, 19, Tower, (10965.2, 14054.6, -1847.47)),  // Ogrewatch
    obj(38, 5, Camp, (11279.8, 13736.8, -835.691)),    // Pangloss
    obj(38, 8, Camp, (11565.5, 14444.8, -302.91)),     // Umberglade
    obj(38, 22, Tower, (11766.3, 14793.5, -2133.39)),  // Bravost
    obj(38, 21, Tower, (11156.4, 14527.8, -1622.95)),  // Durios
    obj(38, 2, Keep, (11496.5, 15120.6, -1786.97)),    // Blue Valley
    obj(38, 15, Tower, (11452.7, 15490.7, -2246.3)),   // Langor
    obj(38, 16, Tower, (10850.1, 15224.4, -1052.29)),  // Quentin
    obj(38, 7, Camp, (11037.9, 15556.2, -483.931)),    // Danelon
    obj(38, 4, Camp, (10202.6, 15437.1, -79.961)),     // Golanta
    obj(38, 13, Tower, (9805.96, 15406.4, -1659.98)),  // Jerrifer
    obj(38, 14, Tower, (10171.8, 15081.8, -495.673)),  // Klovan
    obj(38, 3, Keep, (9604.47, 15129.9, -906.09)),     // Green Lowlands
    obj(38, 11, Tower, (9413.84, 14792.8, -1313.37)),  // Aldon
    obj(38, 12, Tower, (9906.21, 14624.6, -1014.99)),  // Wildcreek
    obj(38, 10, Camp, (9570.97, 14423.2, -700.0)),     // Rogue
    obj(38, 9, Castle, (10606.3, 14580.3, -1536.93)),  // Stonemist Castle
];

/// The catalog lookup -- GW2EI's `WvWHelper.GetObjectiveData`. Linear over
/// ~80 rows, called once per distinct objective in a log (at most a few
/// dozen), so a map would cost more to build than the scan saves.
pub fn objective_def(map_id: i32, objective_id: i32) -> Option<&'static ObjectiveDef> {
    OBJECTIVES.iter().find(|o| o.map_id == map_id && o.objective_id == objective_id)
}

/// One objective's ownership timeline over the log.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveStatus {
    pub map_id: i32,
    pub objective_id: i32,
    pub kind: ObjectiveType,
    /// EI's `AutoUpgradeProgress`, from the FIRST status event for this
    /// objective -- EI's is a `readonly` field set in the constructor, and
    /// `AddOwners` merges only owners, so later events' progress is
    /// discarded there too. Not serialized in either output format; see the
    /// module doc.
    pub auto_upgrade_progress: u32,
    /// `(team_id, time_ms)` in event order, one entry per status event seen
    /// for this objective. Log-relative ms, the project-wide convention
    /// ([`RawLog::log_start_ms`] is the anchor).
    ///
    /// NOT deduplicated, deliberately: GW2EI concatenates
    /// (`WvWObjectiveStatusEvent.AddOwners` is a bare `AddRange`) and the
    /// reference export shows the consequence -- a Blue Garrison row
    /// carrying `[433, 315057]` twice in a row. Collapsing repeats here
    /// would be a nicer timeline and a parity break.
    pub owners: Vec<(u32, u64)>,
}

/// Every objective status timeline in the log, in first-seen order.
///
/// Reproduces `CombatEventFactory.cs:555-570`: key on
/// `(map_id << 16) + objective_id`, drop pairs the catalog does not know,
/// and append owners onto the first record seen for a key rather than
/// emitting a second record.
pub fn objectives(raw: &RawLog) -> Vec<ObjectiveStatus> {
    // `usize` index into `out`, so first-seen ORDER survives -- EI's
    // `WvWObjectiveStatusEvents` is a List appended in stream order and
    // `WvWObjectiveStatusEventsByKey` is only the dedupe index. A
    // `BTreeMap<key, ObjectiveStatus>` would have quietly re-sorted the
    // output by `(map, objective)`.
    let mut index: BTreeMap<i64, usize> = BTreeMap::new();
    let mut out: Vec<ObjectiveStatus> = Vec::new();

    for e in raw.events.iter().filter(|e| e.is_statechange == sc::WVW_OBJECTIVE_STATUS) {
        let ev = parse_objective_status_event(e);
        let Some(def) = objective_def(ev.map_id, ev.objective_id) else { continue };
        let key = ((ev.map_id as i64) << 16) + ev.objective_id as i64;
        match index.get(&key) {
            Some(&i) => out[i].owners.push((ev.owner, e.time)),
            None => {
                index.insert(key, out.len());
                out.push(ObjectiveStatus {
                    map_id: ev.map_id,
                    objective_id: ev.objective_id,
                    kind: def.kind,
                    auto_upgrade_progress: ev.auto_upgrade_progress,
                    owners: vec![(ev.owner, e.time)],
                });
            }
        }
    }
    out
}

/// One sc=75 event's payload. See the module doc for the field-mapping
/// citation.
struct RawObjectiveStatus {
    map_id: i32,
    objective_id: i32,
    owner: u32,
    auto_upgrade_progress: u32,
}

fn parse_objective_status_event(e: &RawEvent) -> RawObjectiveStatus {
    RawObjectiveStatus {
        map_id: e.value,
        objective_id: e.skillid as i32,
        owner: e.buff_dmg as u32,
        auto_upgrade_progress: e.pad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};

    fn status(time: u64, map_id: i32, objective_id: u32, team: i32) -> RawEvent {
        RawEvent {
            time,
            src_agent: 0,
            dst_agent: 0,
            value: map_id,
            buff_dmg: team,
            overstack: 0,
            skillid: objective_id,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::WVW_OBJECTIVE_STATUS,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn log(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 0 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    #[test]
    fn repeated_status_events_merge_into_one_owner_timeline() {
        // Blue Garrison (96/37) seen three times: one record, three owners.
        let got = objectives(&log(vec![
            status(0, 96, 37, 433),
            status(44684, 96, 37, 433),
            status(315057, 96, 37, 2767),
        ]));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, ObjectiveType::Keep);
        assert_eq!(got[0].owners, vec![(433, 0), (433, 44684), (2767, 315057)]);
    }

    #[test]
    fn a_repeated_owner_at_a_repeated_time_is_kept_not_collapsed() {
        // The reference export carries `[433, 315057]` twice on one row.
        // EI's `AddOwners` is a bare `AddRange`; dedupe here would be a
        // parity break, so this pins the duplicate.
        let got = objectives(&log(vec![status(315057, 96, 37, 433), status(315057, 96, 37, 433)]));
        assert_eq!(got[0].owners, vec![(433, 315057), (433, 315057)]);
    }

    #[test]
    fn an_objective_the_catalog_does_not_know_is_dropped_entirely() {
        // Objective 9999 on a known map, and objective 37 on a map that is
        // not a WvW map at all. EI emits neither, and emits no "Unknown"
        // placeholder for them.
        let got = objectives(&log(vec![
            status(0, 96, 9999, 433),
            status(0, 1, 37, 433),
            status(0, 96, 37, 433),
        ]));
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].map_id, got[0].objective_id), (96, 37));
    }

    #[test]
    fn records_come_back_in_first_seen_order_not_sorted_by_id() {
        let got = objectives(&log(vec![
            status(0, 96, 52, 433),
            status(0, 96, 34, 433),
            status(0, 96, 37, 433),
        ]));
        let ids: Vec<i32> = got.iter().map(|o| o.objective_id).collect();
        assert_eq!(ids, vec![52, 34, 37], "EI appends to a List in stream order");
    }

    #[test]
    fn objective_ids_repeat_across_maps_so_the_key_is_the_pair() {
        // 37 is a Keep on both alpine borderlands; 34 is a Camp on both.
        // Two maps in one log is not a real capture, but it is exactly what
        // a key of `objective_id` alone would collapse.
        let got = objectives(&log(vec![status(0, 96, 37, 433), status(0, 95, 37, 433)]));
        assert_eq!(got.len(), 2, "(map, objective) is the key, not objective alone");
    }

    #[test]
    fn the_catalog_has_no_duplicate_map_objective_pairs() {
        // Transcription guard: EI's source is a nested Dictionary, so a
        // repeated key there would not compile. Here it is a flat slice,
        // where a duplicated row would silently shadow.
        let mut seen: Vec<(i32, i32)> = OBJECTIVES.iter().map(|o| (o.map_id, o.objective_id)).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "duplicate (map_id, objective_id) in OBJECTIVES");
    }

    #[test]
    fn the_catalog_covers_ei_s_four_maps_with_ei_s_counts() {
        // Per-map row counts. A dropped row during transcription is
        // otherwise invisible -- it just makes real objectives disappear
        // from the output.
        //
        // These came from MACHINE-counting `WvWHelper.cs:161-268`, not from
        // reading. The first version of this test carried a hand count
        // (19/19/18/22) and was wrong on both alpine maps; the whole table
        // was then re-diffed against the source by script -- ids, types and
        // all 76 continent positions -- and matches row for row in order.
        let count = |m: i32| OBJECTIVES.iter().filter(|o| o.map_id == m).count();
        assert_eq!(count(96), 18, "Blue Alpine Borderland");
        assert_eq!(count(95), 18, "Green Alpine Borderland");
        assert_eq!(count(1099), 18, "Red Desert Borderland");
        assert_eq!(count(38), 22, "Eternal Battlegrounds");
        assert_eq!(OBJECTIVES.len(), 18 + 18 + 18 + 22);
    }

    #[test]
    fn stonemist_is_the_only_castle() {
        let castles: Vec<_> =
            OBJECTIVES.iter().filter(|o| o.kind == ObjectiveType::Castle).collect();
        assert_eq!(castles.len(), 1);
        assert_eq!((castles[0].map_id, castles[0].objective_id), (38, 9));
    }

    #[test]
    fn type_names_match_ei_s_wire_spelling() {
        assert_eq!(ObjectiveType::Camp.name(), "Camp");
        assert_eq!(ObjectiveType::Ruins.name(), "Ruins");
        assert_eq!(ObjectiveType::Tower.name(), "Tower");
        assert_eq!(ObjectiveType::Keep.name(), "Keep");
        assert_eq!(ObjectiveType::Castle.name(), "Castle");
    }

    #[test]
    fn a_log_with_no_status_events_yields_no_records() {
        assert!(objectives(&log(vec![])).is_empty());
    }
}
