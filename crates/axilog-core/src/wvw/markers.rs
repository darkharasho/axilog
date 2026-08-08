//! `CBTS_MARKER` squad-marker decode (incl. commander-tag variant) and
//! `CBTS_TICK` tick-rate telemetry (Task 7, M2 -- arcdps-dev guidance
//! items 3/4/5/7). Both are native-schema-only differentiators: EI's JSON
//! has no comparable field for either, so the EI adapter is intentionally
//! left untouched by this module.

use crate::evtc::{sc, ContentType, GuidMapping, RawEvent, RawLog};
use crate::model::{CommanderTag, MarkerAssignment, TickRate};
use std::collections::BTreeMap;

/// GUID (lowercase hex, no dashes, matching `GuidMapping::guid_hex()`) ->
/// squad overhead marker name. Sourced from GW2EI's `MarkerGUIDs`
/// ("Overhead Squad Markers" section):
/// raw.githubusercontent.com/baaron4/GW2-Elite-Insights-Parser/master/
/// GW2EIEvtcParser/ParserHelpers/GUIDs/MarkerGUIDs.cs (fetched and
/// programmatically extracted, not hand-transcribed, to avoid hex-string
/// transposition errors).
///
/// Byte order matches our own `guid_hex()`: GW2EI's `GUID(ReadOnlySpan<char>
/// hex)` constructor parses each hex-string byte pair directly into
/// `first8 ++ last8` with no swapping, and `IDToGUIDEvent` builds `GUID =
/// new(evtcItem.SrcAgent, evtcItem.DstAgent)` -- the exact same
/// `src_agent_le_bytes ++ dst_agent_le_bytes` construction
/// `decode_guid_mappings` uses. So GW2EI's hex literals can be lowercased
/// and compared directly against `guid_hex()` with no reordering.
///
/// Verified against the real WvW fixture: `CBTS_IDTOGUID` (content type
/// Marker) mapping local id 3201 -> GUID `1993fadb6fb70e4383a223a54d311f7d`,
/// which is exactly `PurpleCommanderTag` below -- and the fixture's
/// `CBTS_MARKER` events assigning local id 3201 do carry `buff == 1` (see
/// `sc::MARKER` docs). The fixture also has a marker local id (1090) whose
/// GUID (`3cd1c64a5000774488009d4d69455c5c`) is NOT in either table below
/// -- real, not just synthetic, coverage of the "unknown GUID -> hex
/// fallback" path.
const SQUAD_MARKER_NAMES: &[(&str, &str)] = &[
    ("c3a56f1e045e3848b07cbac5bbdd2c32", "arrow"),
    ("73c880ae431c9f4d8a5972acf7066f4e", "circle"),
    ("185008e2437b184d8fdad647dd972d9f", "heart"),
    ("6e5997457b3f6a45b984c613806fa72a", "square"),
    ("5140125657c6084d94226c8ec0216649", "star"),
    ("ebbe113ae2e53f4e96f3e92fb1353ece", "swirl"),
    ("46ebc4397f8a3740b900333b591f6183", "triangle"),
    ("8bdcf5c47f8a8340a251f102af3b5905", "x"),
];

/// GUID -> commander-tag variant name (colour + standard/"catmander" cat
/// variant), from the same GW2EI `MarkerGUIDs` source
/// (`CommanderTagMarkersHexGUIDs`). Variant names are lowercase
/// `<color>-commander` / `<color>-catmander`, matching this codebase's
/// existing lowercase-kebab convention for team colors (`"red"`, `"blue"`,
/// `"green"` in `wvw::team_color`).
const COMMANDER_TAG_VARIANTS: &[(&str, &str)] = &[
    ("4242f370667ce54eb3bf22be8d06f986", "red-commander"),
    ("e57aae9ee7fc5d458b0cf16be4b096bf", "orange-commander"),
    ("af9442a290c6214596e0b339eb3bde92", "yellow-commander"),
    ("74ad480e531f4740a407879976c8ca91", "green-commander"),
    ("96f4ab5cdec5294388375c7a03ab7614", "cyan-commander"),
    ("ae714fc5e4ea464c8961cd78e86f9291", "blue-commander"),
    ("1993fadb6fb70e4383a223a54d311f7d", "purple-commander"),
    ("e911d8c0ef2fdf4d8d252e5fb1283c62", "pink-commander"),
    ("a59678cdfb5732439d7fcbf58d8bcec3", "white-commander"),
    ("ca76ab023593b0448f692fe29df03d17", "red-catmander"),
    ("9fdf03e9ba09a2458c1edda4d81bc34d", "orange-catmander"),
    ("6bce90e99016b448969eb317784a8334", "yellow-catmander"),
    ("2ca226e07262c743ba193acf6f9d0af6", "green-catmander"),
    ("a8072d65ce35924babbac831b12019d7", "cyan-catmander"),
    ("9b94f0fd616e7f4aa58efdc8c59fb689", "blue-catmander"),
    ("7224a4af710e4243bfe032629e17ca6e", "purple-catmander"),
    ("4387be6146d43246aa7b333168ea58ea", "pink-catmander"),
    ("a0b0ec076bc83b40a293c1cdec4a7de7", "white-catmander"),
];

fn squad_marker_name(guid_hex: &str) -> Option<&'static str> {
    SQUAD_MARKER_NAMES.iter().find(|(g, _)| *g == guid_hex).map(|(_, n)| *n)
}

fn commander_tag_variant(guid_hex: &str) -> Option<&'static str> {
    COMMANDER_TAG_VARIANTS.iter().find(|(g, _)| *g == guid_hex).map(|(_, n)| *n)
}

/// Resolve a `CBTS_MARKER` `value` (content-local marker id, never 0 --
/// callers must handle the `value == 0` "remove" case themselves) to a
/// display name: the known squad-marker name if the id's `CBTS_IDTOGUID`
/// GUID is one of the 8 overhead markers, else the raw GUID hex (unknown
/// marker, but we know *which* GUID it is), else -- when no `CBTS_IDTOGUID`
/// mapping exists at all for this local id -- the decimal id as a string
/// (last-resort fallback so a marker with no resolvable identity still
/// produces *some* stable, non-empty value rather than being silently
/// dropped).
fn marker_display_name(local_id: u32, guid_map: &[GuidMapping]) -> (String, Option<String>) {
    let guid_hex = guid_map
        .iter()
        .find(|g| g.content_type == ContentType::Marker && g.local_id == local_id)
        .map(|g| g.guid_hex());
    match &guid_hex {
        Some(hex) => {
            let name = squad_marker_name(hex).map(str::to_string).unwrap_or_else(|| hex.clone());
            (name, guid_hex)
        }
        None => (local_id.to_string(), None),
    }
}

/// Live commander-tag state for one agent: which variant (or hex fallback,
/// per the brief) it currently is.
#[derive(Debug, Clone)]
pub(crate) struct LiveCommanderTag {
    variant: String,
    guid: String,
}

/// One agent's currently-assigned marker plus, if it happens to be a
/// commander tag, the resolved variant.
#[derive(Debug, Clone, Default)]
pub(crate) struct CurrentMarker {
    name: Option<String>,
    commander_tag: Option<LiveCommanderTag>,
    /// `time` of the assignment that produced this state -- used to pick
    /// the freshest state across a deduped account's several raw addrs
    /// (relogs/build-swaps).
    time_ms: u64,
}

/// Decoded `CBTS_MARKER` state: each agent's final (last-assigned, not yet
/// removed) marker/commander-tag, plus the full chronological assignment
/// list for `Encounter.markers`.
pub(crate) struct MarkerResolution {
    pub current: BTreeMap<u64, CurrentMarker>,
    pub assignments: Vec<MarkerAssignment>,
}

/// Decode every `CBTS_MARKER` (sc=37) event in `raw.events` into
/// `MarkerResolution`. Events are processed in stream order (arcdps emits
/// them chronologically; nothing here re-sorts), tracking each agent's
/// currently-assigned marker: `value == 0` clears it (per the arcdps
/// reference: "if value is 0, remove all markers presently on agent"),
/// any other `value` sets it to the resolved name (see
/// `marker_display_name`) and, when `buff == 1`, also records the resolved
/// commander-tag variant (falling back to the raw hex GUID for an
/// unrecognized tag colour, per the Task 7 brief -- `buff == 1` is the
/// authoritative "this is a commander tag" signal per the arcdps
/// reference, independent of whether we recognize the specific GUID).
///
/// Only real assignments (`value != 0`) are pushed into `assignments` --
/// removals aren't "an assignment", they just clear `current` for that
/// agent so a later `marker`/`commander_tag` lookup (Task 7's per-agent
/// final-state fields) correctly reports "no marker" rather than a stale
/// one.
pub(crate) fn resolve_markers(raw: &RawLog) -> MarkerResolution {
    let mut current: BTreeMap<u64, CurrentMarker> = BTreeMap::new();
    let mut assignments = Vec::new();
    for e in &raw.events {
        if e.is_statechange != sc::MARKER {
            continue;
        }
        let agent = e.src_agent;
        if e.value == 0 {
            current.remove(&agent);
            continue;
        }
        let local_id = e.value as u32;
        let (name, guid_hex) = marker_display_name(local_id, &raw.guid_map);
        let commander_tag = if e.buff == 1 {
            let guid = guid_hex.clone().unwrap_or_else(|| name.clone());
            let variant = commander_tag_variant(&guid).map(str::to_string).unwrap_or_else(|| guid.clone());
            Some(LiveCommanderTag { variant, guid })
        } else {
            None
        };
        current.insert(
            agent,
            CurrentMarker { name: Some(name.clone()), commander_tag, time_ms: e.time },
        );
        assignments.push(MarkerAssignment { agent_addr: agent, marker: name, time_ms: e.time });
    }
    MarkerResolution { current, assignments }
}

/// Pick the freshest (highest `time_ms`) marker state across every raw
/// addr an account/enemy owns (relog/build-swap dedupe folds several raw
/// addrs into one `Player`/`Enemy` with `agent_addrs` covering all of
/// them -- Task 4, M2). Returns `None` if none of the addrs ever got a
/// marker, or if the freshest state was a since-cleared removal.
fn freshest<'a>(
    current: &'a BTreeMap<u64, CurrentMarker>,
    agent_addrs: &[u64],
) -> Option<&'a CurrentMarker> {
    agent_addrs.iter().filter_map(|a| current.get(a)).max_by_key(|m| m.time_ms)
}

/// Look up the final marker name for a deduped account's addrs, or `None`.
pub(crate) fn final_marker(current: &BTreeMap<u64, CurrentMarker>, agent_addrs: &[u64]) -> Option<String> {
    freshest(current, agent_addrs).and_then(|m| m.name.clone())
}

/// Look up the final commander-tag state for a deduped account's addrs, or
/// `None`.
pub(crate) fn final_commander_tag(
    current: &BTreeMap<u64, CurrentMarker>,
    agent_addrs: &[u64],
) -> Option<CommanderTag> {
    freshest(current, agent_addrs)
        .and_then(|m| m.commander_tag.as_ref())
        .map(|t| CommanderTag { variant: t.variant.clone(), guid: t.guid.clone() })
}

/// Derive `encounter.tick_rate` from `CBTS_TICK` (sc=84) events: `None`
/// when the log has fewer than 2 such events (nothing to compute a rate
/// from -- Task 7 brief: "skip gracefully ... when the log has no tick
/// events").
///
/// The exact meaning of `dst_agent` ("ticks since last real tick update")
/// isn't independently corroborated anywhere in GW2EI's source, so rather
/// than build on an unverified interpretation of it (or on the "every 25
/// ticks" cadence claim, which is a *comment*, not a guaranteed invariant),
/// the rate is derived from the one payload field whose meaning is
/// unambiguous either way: `src_agent`, "current extrapolated tick". The
/// delta in that counter between two consecutive `CBTS_TICK` events,
/// divided by the real wall-clock time (`event.time`) between them, is
/// exactly ticks-per-second over that interval -- regardless of how often
/// arcdps happens to emit the event. Intervals where the counter goes
/// backwards ("ticks may go backwards if real update is lower than
/// extrapolation", per the arcdps reference) are skipped rather than
/// producing a nonsensical negative rate.
///
/// `avg` is the overall rate (total tick delta / total elapsed time across
/// all valid intervals, not a plain mean of per-interval rates, so long
/// intervals aren't overweighted). `min` is the lowest instantaneous
/// per-interval rate -- the "tick rate dip" signal the arcdps-dev guidance
/// calls out as the objective skill-lag indicator. `per_second` buckets the
/// per-interval rates by wall-clock second (same 1000ms resolution as
/// `analysis::cc::timeline`), averaging when more than one interval lands
/// in the same bucket; empty buckets default to `0.0`, mirroring how the
/// rest of the per-second timeline already treats gaps (see
/// `cc::timeline`).
pub(crate) fn resolve_tick_rate(raw: &RawLog, duration_ms: u64) -> Option<TickRate> {
    let ticks: Vec<&RawEvent> = raw.events.iter().filter(|e| e.is_statechange == sc::TICK).collect();
    if ticks.len() < 2 {
        return None;
    }
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);
    let res = 1000u64;
    let buckets = ((duration_ms / res) + 1) as usize;
    let mut per_second = vec![0.0f64; buckets];
    let mut bucket_hits = vec![0u32; buckets];
    let mut rates: Vec<f64> = Vec::new();
    let mut total_ticks: i64 = 0;
    let mut total_ms: i64 = 0;

    for w in ticks.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dt_ms = b.time.saturating_sub(a.time);
        if dt_ms == 0 {
            continue;
        }
        let dtick = (b.src_agent as i64) - (a.src_agent as i64);
        if dtick <= 0 {
            continue; // backwards/stalled extrapolation -- not a usable sample
        }
        let rate = dtick as f64 / (dt_ms as f64 / 1000.0);
        rates.push(rate);
        total_ticks += dtick;
        total_ms += dt_ms as i64;
        let rel = b.time.saturating_sub(t0);
        let bi = (rel / res) as usize;
        if bi < buckets {
            per_second[bi] += rate;
            bucket_hits[bi] += 1;
        }
    }
    if rates.is_empty() {
        return None;
    }
    for (i, hits) in bucket_hits.iter().enumerate() {
        if *hits > 0 {
            per_second[i] /= *hits as f64;
        }
    }
    let avg = total_ticks as f64 / (total_ms as f64 / 1000.0);
    let min = rates.iter().cloned().fold(f64::INFINITY, f64::min);
    Some(TickRate { avg, min, per_second })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{ContentType, GuidMapping, RawHeader};

    fn marker_ev(time: u64, agent: u64, value: i32, buff: u8) -> RawEvent {
        RawEvent {
            time,
            src_agent: agent,
            dst_agent: 0,
            value,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_statechange: sc::MARKER,
        }
    }

    fn tick_ev(time: u64, extrapolated_tick: u64, ticks_since_real: u64) -> RawEvent {
        RawEvent {
            time,
            src_agent: extrapolated_tick,
            dst_agent: ticks_since_real,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_statechange: sc::TICK,
        }
    }

    fn guid_from_hex(hex: &str) -> [u8; 16] {
        let mut out = [0u8; 16];
        for i in 0..16 {
            out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    fn marker_guid_mapping(local_id: u32, guid_hex: &str) -> GuidMapping {
        GuidMapping { content_type: ContentType::Marker, local_id, guid: guid_from_hex(guid_hex) }
    }

    fn raw_from(events: Vec<RawEvent>, guid_map: Vec<GuidMapping>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map,
        }
    }

    /// A recognized squad-marker GUID resolves to its human name, not the
    /// raw hex -- catches a wrong field extraction (e.g. reading `skillid`
    /// instead of `value` for the marker id) immediately, since a wrong id
    /// would fail to find the GUID mapping and fall back to hex/decimal.
    #[test]
    fn resolves_known_squad_marker_name() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 42, 0)],
            vec![marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32")], // Arrow
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.current, &[1]).as_deref(), Some("arrow"));
        assert_eq!(res.assignments.len(), 1);
        assert_eq!(res.assignments[0], MarkerAssignment { agent_addr: 1, marker: "arrow".into(), time_ms: 100 });
    }

    /// An unrecognized marker GUID (present in `guid_map` but not in our
    /// name table) falls back to the raw hex string -- not silently
    /// dropped, not confused with a known name.
    #[test]
    fn unknown_marker_guid_falls_back_to_hex() {
        // Real GUID from the WvW fixture (local id 1090), not in either
        // table -- proves the fallback path against real, not just made-up,
        // data.
        let raw = raw_from(
            vec![marker_ev(100, 1, 1090, 0)],
            vec![marker_guid_mapping(1090, "3cd1c64a5000774488009d4d69455c5c")],
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.current, &[1]).as_deref(), Some("3cd1c64a5000774488009d4d69455c5c"));
    }

    /// No `CBTS_IDTOGUID` mapping at all for the local id: last-resort
    /// fallback to the decimal id (still non-empty, still stable).
    #[test]
    fn marker_with_no_guid_mapping_falls_back_to_decimal_id() {
        let raw = raw_from(vec![marker_ev(100, 1, 999, 0)], vec![]);
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.current, &[1]).as_deref(), Some("999"));
    }

    /// `value == 0` clears the agent's current marker.
    #[test]
    fn removal_event_clears_marker() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 42, 0), marker_ev(200, 1, 0, 0)],
            vec![marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32")],
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.current, &[1]), None);
        // The removal itself is not recorded as an "assignment".
        assert_eq!(res.assignments.len(), 1);
    }

    /// `buff == 1` marks the marker as a commander tag; the GUID resolves
    /// to a known colour variant.
    #[test]
    fn resolves_known_commander_tag_variant() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 3201, 1)],
            vec![marker_guid_mapping(3201, "1993fadb6fb70e4383a223a54d311f7d")], // PurpleCommanderTag
        );
        let res = resolve_markers(&raw);
        let tag = final_commander_tag(&res.current, &[1]).expect("commander tag present");
        assert_eq!(tag.variant, "purple-commander");
        assert_eq!(tag.guid, "1993fadb6fb70e4383a223a54d311f7d");
    }

    /// A catmander-tag GUID resolves to the distinct "-catmander" variant,
    /// not conflated with the standard commander tag of the same colour.
    #[test]
    fn resolves_catmander_variant_distinctly() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 7, 1)],
            vec![marker_guid_mapping(7, "ca76ab023593b0448f692fe29df03d17")], // RedCatmanderTag
        );
        let res = resolve_markers(&raw);
        let tag = final_commander_tag(&res.current, &[1]).expect("commander tag present");
        assert_eq!(tag.variant, "red-catmander");
    }

    /// `buff == 1` on a GUID we don't recognize still counts as a
    /// commander tag (per the arcdps reference, `buff` is the
    /// authoritative signal) -- falls back to the hex GUID as the variant,
    /// per the Task 7 brief ("unknown GUIDs fall back to the hex string").
    #[test]
    fn unknown_commander_tag_guid_falls_back_to_hex_variant() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 55, 1)],
            vec![marker_guid_mapping(55, "00112233445566778899aabbccddeeff")],
        );
        let res = resolve_markers(&raw);
        let tag = final_commander_tag(&res.current, &[1]).expect("commander tag present");
        assert_eq!(tag.variant, "00112233445566778899aabbccddeeff");
    }

    /// A non-commander marker (`buff == 0`) never produces a
    /// `commander_tag`, even though the agent does have a marker.
    #[test]
    fn non_commander_marker_has_no_commander_tag() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 42, 0)],
            vec![marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32")],
        );
        let res = resolve_markers(&raw);
        assert!(final_commander_tag(&res.current, &[1]).is_none());
    }

    /// Relog/build-swap dedupe (Task 4, M2): the freshest (highest
    /// `time_ms`) marker across every raw addr an account owns wins, even
    /// when it's on a later addr than the account's representative.
    #[test]
    fn final_marker_picks_freshest_across_deduped_addrs() {
        let raw = raw_from(
            vec![
                marker_ev(100, 1, 42, 0),  // pre-relog addr: arrow
                marker_ev(500, 2, 999, 0), // post-relog addr: no guid mapping -> "999", but later in time
            ],
            vec![marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32")],
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.current, &[1, 2]).as_deref(), Some("999"));
    }

    /// Fewer than 2 `CBTS_TICK` events: `tick_rate` is `None` (skip
    /// gracefully, per the Task 7 brief), not a bogus zero/NaN rate.
    #[test]
    fn tick_rate_none_with_fewer_than_two_events() {
        let raw = raw_from(vec![tick_ev(0, 1000, 25)], vec![]);
        assert!(resolve_tick_rate(&raw, 1000).is_none());
        let raw_empty = raw_from(vec![], vec![]);
        assert!(resolve_tick_rate(&raw_empty, 1000).is_none());
    }

    /// A steady, non-lagging tick stream: the extrapolated-tick-counter
    /// delta (25) over 1000ms real elapsed = 25 ticks/sec, constant across
    /// intervals, so avg == min == 25.0.
    #[test]
    fn tick_rate_steady_stream_reports_constant_rate() {
        let raw = raw_from(
            vec![
                tick_ev(0, 1000, 0),
                tick_ev(1000, 1025, 0),
                tick_ev(2000, 1050, 0),
                tick_ev(3000, 1075, 0),
            ],
            vec![],
        );
        let tr = resolve_tick_rate(&raw, 3000).expect("tick rate present");
        assert!((tr.avg - 25.0).abs() < 1e-9, "avg={}", tr.avg);
        assert!((tr.min - 25.0).abs() < 1e-9, "min={}", tr.min);
    }

    /// A tick-rate dip (server skill-lag signature): one interval where
    /// the same tick delta takes twice as long in real time halves the
    /// instantaneous rate, and that's what `min` must catch -- proving the
    /// per-interval computation actually drives `min` down, not just
    /// reporting the steady-state average.
    #[test]
    fn tick_rate_min_catches_a_dip() {
        let raw = raw_from(
            vec![
                tick_ev(0, 1000, 0),
                tick_ev(1000, 1025, 0), // 25 ticks / 1000ms = 25.0/s
                tick_ev(3000, 1050, 0), // 25 ticks / 2000ms = 12.5/s (lag dip)
                tick_ev(4000, 1075, 0), // 25 ticks / 1000ms = 25.0/s
            ],
            vec![],
        );
        let tr = resolve_tick_rate(&raw, 4000).expect("tick rate present");
        assert!((tr.min - 12.5).abs() < 1e-9, "min={}", tr.min);
        assert!(tr.avg > tr.min, "avg ({}) should be pulled up by the two 25.0/s intervals", tr.avg);
    }

    /// A backwards tick-counter interval ("ticks may go backwards if real
    /// update is lower than extrapolation", per the arcdps reference) is
    /// skipped rather than producing a negative/nonsensical rate.
    #[test]
    fn tick_rate_skips_backwards_intervals() {
        let raw = raw_from(
            vec![
                tick_ev(0, 1000, 0),
                tick_ev(1000, 1025, 0),
                tick_ev(2000, 1010, 0), // counter went backwards -- skip this interval
                tick_ev(3000, 1035, 0),
            ],
            vec![],
        );
        let tr = resolve_tick_rate(&raw, 3000).expect("tick rate present");
        // Only the two forward 25-tick/1000ms intervals contribute.
        assert!((tr.avg - 25.0).abs() < 1e-9, "avg={}", tr.avg);
        assert!((tr.min - 25.0).abs() < 1e-9, "min={}", tr.min);
    }
}
