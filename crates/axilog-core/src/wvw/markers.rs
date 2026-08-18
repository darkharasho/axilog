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
// NOTE: these two tables and `analysis::marker_icons::MARKERS` are extracted
// from the SAME GW2EI source and must not drift apart. They are kept separate
// on purpose -- these carry the legacy schema's lowercase-kebab names
// (`"arrow"`, `"purple-commander"`), which are part of `PlayerOut.marker`'s
// published output, while `marker_icons` carries display labels and art for
// the 1.0 document. `marker_tables_agree_with_the_icon_catalog` below pins the
// GUID SETS together so a regenerated icon catalog cannot silently diverge.
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

/// One currently-open marker instance on an agent. arcdps (build >=
/// `NewMarkerEventBehavior`, 20240418 -- see `MarkerInstance` docs on
/// `resolve_markers`) tracks *multiple concurrent* markers per agent, not
/// one: a player can simultaneously have a commander tag AND a squad
/// overhead marker (e.g. the squad lieutenant target-calls the commander
/// with Arrow -- routine in WvW). `is_commander` (from `buff == 1`)
/// distinguishes which "slot" this instance occupies for the purposes of
/// `final_marker`/`final_commander_tag` below.
#[derive(Debug, Clone)]
pub(crate) struct MarkerInstance {
    /// Content-local marker id (`value`) -- used only to detect a repeat
    /// assignment of the *same* marker id, which replaces (does not
    /// duplicate) the earlier open instance, mirroring GW2EI's "We can't
    /// have the same markers active at the same time on one Src".
    marker_id: i32,
    name: String,
    guid_hex: Option<String>,
    is_commander: bool,
    /// Resolved commander-tag variant (or hex/decimal fallback), only
    /// meaningful when `is_commander`.
    commander_variant: Option<String>,
    /// Assignment time -- used to pick the freshest instance across a
    /// deduped account's several raw addrs (relogs/build-swaps), and
    /// (among same-slot instances on one addr) the most recently assigned.
    start_ms: u64,
}

/// Decoded `CBTS_MARKER` state: each agent's currently-open marker
/// instances (Task 7 fix round 1: a `Vec`, not a single slot -- see
/// `resolve_markers`), plus the full chronological assignment list for
/// `Encounter.markers`.
pub(crate) struct MarkerResolution {
    pub(crate) open: BTreeMap<u64, Vec<MarkerInstance>>,
    /// M4 post-rework real-log calibration finding: the most recent
    /// commander-tag assignment ever observed for each agent, kept
    /// regardless of a later removal event with no reassignment.
    ///
    /// `open` alone (only currently-active instances) matches GW2EI's own
    /// per-instance bookkeeping, but NOT its actual `hasCommanderTag`
    /// semantics: `StatisticsHelper.CalculateCommanderStates` accumulates
    /// EVERY commander-tag-GUID marker segment a player ever held over the
    /// whole log -- open or since-closed -- into `Player.GetCommanderStates`,
    /// and `Player.IsCommander` is simply "at least one such segment exists
    /// at all" (`GetCommanderStates(log).Count > 0`), not "one is still
    /// open at the log's last event". Verified against a real post-rework
    /// capture (`fixtures/local/wvw-postrework.zevtc`): the recorded
    /// commander's only commander-tag marker activity is a burst of
    /// assign/remove events within the first ~350ms of the ~5m48s log
    /// (arcdps replaying current marker state at recording start), ending
    /// in an unreciprocated removal with no marker activity for the
    /// remaining ~99.9% of the fight -- so `open` alone has nothing for
    /// them, and axilog reported zero commanders even though the EI golden
    /// JSON for this same log has `hasCommanderTag: true` for that exact
    /// player (cross-checked by account). `final_commander_tag` below
    /// consults `open` first (still-active instance, sharper "who's
    /// commander right now" info when available) and falls back to this
    /// map only when nothing is currently open -- so a commander who later
    /// genuinely un-tagged and a NEW commander tagged up afterward is still
    /// reported correctly; this only changes the "silently detected zero
    /// commanders" case.
    pub(crate) ever_commander: BTreeMap<u64, MarkerInstance>,
    /// Closed `[tag-on, tag-off)` windows per agent, in ms, for
    /// commander-tag instances only. `open` deliberately drops closed
    /// instances -- it only ever needed final state -- so this is a
    /// parallel collection rather than something derivable from it.
    ///
    /// **Literal, per-instance segments -- confirmed against GW2EI source**
    /// (`StatisticsHelper.CalculateCommanderStates`,
    /// `/var/tmp/gw2ei/GW2EIEvtcParser/EIData/Statistics/StatisticsHelper.cs:308-383`;
    /// full citations in
    /// `.superpowers/sdd/2026-08-15-phase-b-native-gap-closure/open-questions-findings.md`,
    /// Question 1). Three rules, all mirrored below with no invented
    /// smoothing:
    ///
    /// - A segment still open when the raw stream ends is closed at the raw
    ///   stream's last event time (GW2EI: `Math.Min(markerEvent.EndTime,
    ///   log.LogData.EvtcLogEnd)` -- an unclosed assign is clamped to the
    ///   log's real end, because that's the boundary of what we actually
    ///   observed, not a manufactured extension past it).
    /// - An unreciprocated REMOVAL -- `value == 0` with nothing open for
    ///   that agent -- is a silent no-op: GW2EI's own `CombatEventFactory`
    ///   never even constructs a `MarkerEvent` for it (the `break` at
    ///   `CombatEventFactory.cs:239` skips the `Add` calls entirely), so it
    ///   cannot extend, retract, or otherwise touch any segment, closed or
    ///   open. A commander whose only activity is a sub-second burst of
    ///   assign/remove pairs (the real-log finding on `ever_commander`
    ///   above) gets exactly that sub-second segment -- thin coverage, not
    ///   a bug, and NOT papered over with a threshold or a log-end
    ///   extension GW2EI itself does not have.
    /// - At most ONE still-open window per agent, and nothing after it: EI
    ///   `break`s out of its per-player loop at the first commander marker
    ///   with `EndNotSet` (`StatisticsHelper.cs:322-325`). Added 2026-08-16;
    ///   see [`truncate_at_first_unclosed`], which also explains the
    ///   tag-colour-swap overlap this closes.
    /// - No minimum-coverage fallback exists anywhere in this collection or
    ///   in GW2EI's `distToCom` chain (`GameplayStatistics.cs:64`: zero
    ///   samples -> sentinel `-1`, never a whole-track fallback). Thin
    ///   coverage is simply an average over few samples downstream.
    ///
    /// Multiple simultaneous commanders are normal in WvW; this map holds
    /// every one of them and does NOT pick a reference. That choice belongs
    /// to the consumer (see the distance scalars), which GW2EI resolves by
    /// pooling every player's segments and sorting by start time --
    /// **earliest tag-start wins**, truncating a later-starting overlapping
    /// segment to where the earlier one ends (`StatisticsHelper.cs:351,
    /// 360-378`) -- not by squad membership or tag duration. This
    /// collection does not perform that pooling; it is Task 7's job once it
    /// has the squad roster.
    pub(crate) commander_segments: BTreeMap<u64, Vec<(u64, u64)>>,
    pub assignments: Vec<MarkerAssignment>,
}

/// Decode every `CBTS_MARKER` (sc=37) event in `raw.events` into
/// `MarkerResolution`. Events are processed in stream order (arcdps emits
/// them chronologically; nothing here re-sorts).
///
/// **Fix round 1:** the first version of this function kept a single
/// "current marker" slot per agent, unconditionally overwritten by every
/// `CBTS_MARKER` event -- so a commander later getting a routine squad
/// target-call (an overhead marker) silently wiped out their
/// `commander_tag`, with no removal event involved at all. That doesn't
/// match arcdps's real model: per GW2EI's `CombatEventFactory` (the
/// `evtcVersion.Build >= ArcDPSBuilds.NewMarkerEventBehavior` branch, i.e.
/// arcdps builds from 2024-04-18 onward -- our fixture's build `20260114`
/// is well past this, so this is the behavior that actually applies),
/// arcdps tracks a *list* of concurrently-open markers per agent
/// (`MarkerEventsBySrc`), with two rules:
///
/// - A non-removal assignment (`value != 0`) only closes a previous *open
///   instance of the same marker id* on that agent (`"We can't have the
///   same markers active at the same time on one Src"`) -- it leaves any
///   other concurrently-open marker of a *different* id (e.g. the
///   commander tag) untouched.
/// - A removal (`value == 0`, "if value is 0, remove all markers presently
///   on agent" per the arcdps reference) ends **every** currently-open
///   marker on that agent unconditionally -- GW2EI's own comment: `"An end
///   event ends all previous markers"`. No per-type distinction: it closes
///   the commander tag too, not just the most recent overhead marker.
///
/// This function mirrors both rules with `open: BTreeMap<u64,
/// Vec<MarkerInstance>>` (only *open* instances are retained -- closed
/// ones are simply dropped, since nothing downstream needs point-in-time
/// history, only final state).
///
/// `buff == 1` marks an instance as a commander tag (the authoritative
/// signal per the arcdps reference, independent of whether the GUID is one
/// we recognize -- see `marker_display_name`/`commander_tag_variant`).
///
/// Only real assignments (`value != 0`) are pushed into `assignments` --
/// removals aren't "an assignment".
#[cfg(test)]
pub(crate) fn resolve_markers(raw: &RawLog) -> MarkerResolution {
    let mut guilds: BTreeMap<u64, String> = BTreeMap::new();
    resolve_markers_and_guilds(raw, &mut guilds)
}

/// [`resolve_markers`], additionally collecting `CBTS_GUILD` rows into
/// `guilds` (`agent addr -> guild GUID`, first row per agent) so the two
/// share one pass over the event stream. MEIGAP Task 3c.
pub(crate) fn resolve_markers_and_guilds(
    raw: &RawLog,
    guilds: &mut BTreeMap<u64, String>,
) -> MarkerResolution {
    let mut open: BTreeMap<u64, Vec<MarkerInstance>> = BTreeMap::new();
    let mut ever_commander: BTreeMap<u64, MarkerInstance> = BTreeMap::new();
    // `bool` = "explicitly closed", i.e. this window ended because a real
    // event ended it, not because the log ran out. Dropped again before
    // return; see `truncate_at_first_unclosed` for why it has to be carried
    // this far rather than inferred from the end time.
    let mut commander_segments: BTreeMap<u64, Vec<(u64, u64, bool)>> = BTreeMap::new();
    let mut assignments = Vec::new();
    for e in &raw.events {
        // MEIGAP Task 3c: `CBTS_GUILD` rides this scan rather than paying
        // for its own. Guild resolution is otherwise unrelated to markers,
        // but `wvw::apply` already runs several whole-event passes and a
        // separate one measured +12% on `model::resolve` for a single
        // `u8` compare per event -- see `docs/BENCHMARKS.md`.
        if super::guilds::collect_guild_event(e, guilds) {
            continue;
        }
        if e.is_statechange != sc::MARKER {
            continue;
        }
        let agent = e.src_agent;
        if e.value == 0 {
            // "An end event ends all previous markers" (GW2EI
            // `CombatEventFactory`) -- unconditionally clear every open
            // instance for this agent, commander tag included. `ever_commander`
            // is deliberately NOT touched here -- see its doc comment on
            // `MarkerResolution`.
            //
            // If there is nothing open for this agent, this removal is a
            // silent no-op (see `commander_segments`'s doc comment) -- the
            // `BTreeMap::remove` below simply returns `None` and no segment
            // is pushed, which is exactly GW2EI's behavior for a removal
            // with nothing to close.
            if let Some(instances) = open.remove(&agent) {
                for m in instances.into_iter().filter(|m| m.is_commander) {
                    commander_segments.entry(agent).or_default().push((m.start_ms, e.time, true));
                }
            }
            continue;
        }
        let local_id = e.value as u32;
        let (name, guid_hex) = marker_display_name(local_id, &raw.guid_map);
        let is_commander = e.buff == 1;
        let commander_variant = if is_commander {
            let guid = guid_hex.clone().unwrap_or_else(|| name.clone());
            Some(commander_tag_variant(&guid).map(str::to_string).unwrap_or(guid))
        } else {
            None
        };
        let instance = MarkerInstance {
            marker_id: e.value,
            name: name.clone(),
            guid_hex,
            is_commander,
            commander_variant,
            start_ms: e.time,
        };
        if is_commander {
            ever_commander.insert(agent, instance.clone());
        }
        let slot = open.entry(agent).or_default();
        // Replace (not stack) a still-open instance of the same marker id;
        // a different id (e.g. commander tag vs. overhead marker) stays
        // open alongside the new instance.
        for m in slot.iter().filter(|m| m.marker_id == instance.marker_id && m.is_commander) {
            commander_segments.entry(agent).or_default().push((m.start_ms, e.time, true));
        }
        slot.retain(|m| m.marker_id != instance.marker_id);
        slot.push(instance);
        assignments.push(MarkerAssignment { agent_addr: agent, marker: name, time_ms: e.time });
    }
    // Close every commander instance still open when the raw stream ends,
    // at the raw stream's own last event time -- these are absolute
    // (not t0-relative) timestamps; the caller in `wvw::apply` rebases if
    // needed. See `commander_segments`'s doc comment for why this is the
    // literal log boundary, not a manufactured extension.
    if let Some(log_end) = raw.events.last().map(|e| e.time) {
        for (&agent, instances) in &open {
            for m in instances.iter().filter(|m| m.is_commander) {
                commander_segments.entry(agent).or_default().push((m.start_ms, log_end, false));
            }
        }
    }
    let commander_segments = commander_segments
        .into_iter()
        .map(|(agent, segs)| (agent, truncate_at_first_unclosed(segs)))
        .filter(|(_, segs)| !segs.is_empty())
        .collect();
    MarkerResolution { open, ever_commander, commander_segments, assignments }
}

/// GW2EI's per-player commander-segment cutoff, ported.
///
/// `StatisticsHelper.CalculateCommanderStates` walks one player's marker
/// events IN EVENT ORDER, appends each commander-tag window, and `break`s
/// at the first one whose end was never set (`StatisticsHelper.cs:322-325`
/// -- the `if (markerEvent.EndNotSet) break;` immediately after the `Add`).
/// So a single player can contribute AT MOST ONE open-ended window, and
/// nothing after it.
///
/// Without that cutoff a commander-tag COLOUR SWAP produced two overlapping
/// segments: post-`NewMarkerEventBehavior` (`ArcDPSEnums.cs:25`, build
/// 20240418) a non-end marker only closes a previously-open marker with the
/// SAME marker id (`CombatEventFactory.cs:241-254`), and two tag colours are
/// two different ids -- so both stayed open and both got closed at log end,
/// yielding `(t_blue, log_end)` and `(t_red, log_end)` for one agent.
///
/// The truncation is INCLUSIVE: the first unclosed window is kept (EI adds
/// it before breaking), everything after it is dropped. Sorting by start
/// first reproduces EI's event order, since markers are recorded in event
/// order and a window's start IS its event's time.
///
/// Note this cannot be inferred from the end time alone -- a window
/// explicitly closed by a removal that happens to land on the log's last
/// event is closed, and must not truncate the rest. Hence the carried flag.
///
/// Overlap BETWEEN players is a different rule and is not applied here:
/// EI resolves that when pooling the whole squad ("previous tag has
/// priority", `StatisticsHelper.cs:363-367`), which this project does in
/// `analysis::distance::commander_positions`.
fn truncate_at_first_unclosed(mut segs: Vec<(u64, u64, bool)>) -> Vec<(u64, u64)> {
    segs.sort_unstable();
    let mut out = Vec::with_capacity(segs.len());
    for (start, end, closed) in segs {
        out.push((start, end));
        if !closed {
            break;
        }
    }
    out
}

/// Pick the freshest (highest `start_ms`) open marker instance matching
/// `is_commander` across every raw addr an account/enemy owns (relog/
/// build-swap dedupe folds several raw addrs into one `Player`/`Enemy`
/// with `agent_addrs` covering all of them -- Task 4, M2).
fn freshest_open<'a>(
    open: &'a BTreeMap<u64, Vec<MarkerInstance>>,
    agent_addrs: &[u64],
    is_commander: bool,
) -> Option<&'a MarkerInstance> {
    agent_addrs
        .iter()
        .filter_map(|a| open.get(a))
        .flatten()
        .filter(|m| m.is_commander == is_commander)
        .max_by_key(|m| m.start_ms)
}

/// Look up the current non-commander (overhead) marker name for a deduped
/// account's addrs, or `None`. Independent of `final_commander_tag` --
/// both can be `Some` at once (Task 7 fix round 1).
pub(crate) fn final_marker(open: &BTreeMap<u64, Vec<MarkerInstance>>, agent_addrs: &[u64]) -> Option<String> {
    freshest_open(open, agent_addrs, false).map(|m| m.name.clone())
}

/// Look up the commander-tag state for a deduped account's addrs, or
/// `None`. Independent of `final_marker` -- both can be `Some` at once
/// (Task 7 fix round 1).
///
/// Prefers a still-open instance (sharper "who's commander right now" info
/// when available); falls back to the most recent commander-tag ever
/// observed for this agent, even if since closed with no reassignment --
/// EI-parity fix, see `MarkerResolution::ever_commander`'s doc comment for
/// why the fallback is needed at all.
///
/// `segments` is filled by merging and sorting `commander_segments` across
/// every addr in `agent_addrs` -- independent of which addr `m` (the
/// presence/variant signal) came from, since a relogged account's several
/// addrs can each contribute segments.
pub(crate) fn final_commander_tag(
    open: &BTreeMap<u64, Vec<MarkerInstance>>,
    ever_commander: &BTreeMap<u64, MarkerInstance>,
    commander_segments: &BTreeMap<u64, Vec<(u64, u64)>>,
    agent_addrs: &[u64],
) -> Option<CommanderTag> {
    let m = freshest_open(open, agent_addrs, true).or_else(|| {
        agent_addrs.iter().filter_map(|a| ever_commander.get(a)).max_by_key(|m| m.start_ms)
    })?;
    let mut segments: Vec<(u64, u64)> = agent_addrs
        .iter()
        .filter_map(|a| commander_segments.get(a))
        .flatten()
        .copied()
        .collect();
    segments.sort_unstable();
    Some(CommanderTag {
        variant: m.commander_variant.clone().unwrap_or_default(),
        guid: m.guid_hex.clone().unwrap_or_else(|| m.name.clone()),
        segments,
    })
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
    let t0 = raw.log_start_ms();
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
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::MARKER,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
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
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: sc::TICK,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
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

    /// Content-local marker ids used only by the `commander_segments`
    /// tests below -- arbitrary but distinct, no GUID mapping needed since
    /// `is_commander` comes from `buff`, not from GUID resolution.
    const COMMANDER_LOCAL_ID: i32 = 3201;
    const OVERHEAD_LOCAL_ID: i32 = 42;

    /// `marker_event(time, src, local_id, buff)` -- same `CBTS_MARKER` row
    /// shape as `marker_ev`, named to match the commander-segment tests'
    /// intent (a marker *event* on an agent, not necessarily an
    /// assignment: `local_id == 0` is a removal).
    fn marker_event(time: u64, src: u64, local_id: i32, buff: u8) -> RawEvent {
        marker_ev(time, src, local_id, buff)
    }

    /// A commander tag opened and later closed by a removal produces one
    /// closed segment. Marker resolution discards closed instances today
    /// ("nothing downstream needs point-in-time history"), which is exactly
    /// what this changes.
    #[test]
    fn commander_segments_capture_a_closed_tag_window() {
        let raw = raw_from(
            vec![
                marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
                marker_event(5000, 1, 0, 0), // value == 0: removal
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 5000)]);
    }

    /// A tag still open at the end of the log runs to the log's end, not
    /// to its own start -- a commander who never un-tagged commanded the
    /// whole fight.
    #[test]
    fn commander_segments_close_an_open_tag_at_log_end() {
        let raw = raw_from(
            vec![
                marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
                marker_event(9000, 2, OVERHEAD_LOCAL_ID, 0),
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 9000)]);
    }

    /// An overhead marker on the same agent must not close the commander
    /// tag -- the arcdps rule mirrored at `resolve_markers_and_guilds`:
    /// a non-removal assignment only closes an open instance of the SAME
    /// marker id.
    #[test]
    fn overhead_marker_does_not_end_a_commander_segment() {
        let raw = raw_from(
            vec![
                marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
                marker_event(2000, 1, OVERHEAD_LOCAL_ID, 0),
                marker_event(6000, 1, 0, 0),
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 6000)]);
    }

    /// An unreciprocated removal -- `value == 0` with nothing open for that
    /// agent -- is a silent no-op (GW2EI: the removal never even
    /// constructs a `MarkerEvent`, so nothing is extended or retracted).
    /// This would fail under an "unreciprocated removal extends to log
    /// end" implementation (rejected option (a) in the task-6 open
    /// question): that reading would fabricate a `(500, 9000)` segment for
    /// agent 1 from nothing; the literal reading records no segment at all.
    #[test]
    fn unreciprocated_removal_is_a_silent_no_op() {
        let raw = raw_from(
            vec![
                marker_event(500, 1, 0, 0), // removal with nothing open
                marker_event(9000, 2, OVERHEAD_LOCAL_ID, 0),
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert!(
            !res.commander_segments.contains_key(&1),
            "an unreciprocated removal must not fabricate a segment"
        );
    }

    /// A second commander-tag COLOUR (a different marker id, still
    /// `buff == 1`) used to leave BOTH tags open, so both were closed at log
    /// end and the agent reported two overlapping windows,
    /// `(1000, 9000)` and `(4000, 9000)`. EI cannot produce that: it stops
    /// at the first commander marker whose end was never set
    /// (`StatisticsHelper.cs:322-325`), so only the earlier window survives.
    #[test]
    fn a_tag_colour_swap_does_not_produce_two_overlapping_segments() {
        const OTHER_COMMANDER_LOCAL_ID: i32 = 3202;
        let raw = raw_from(
            vec![
                marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
                marker_event(4000, 1, OTHER_COMMANDER_LOCAL_ID, 1),
                marker_event(9000, 2, OVERHEAD_LOCAL_ID, 0),
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert_eq!(
            res.commander_segments[&1],
            vec![(1000, 9000)],
            "a colour swap must yield ONE window, the earlier one -- not two overlapping ones"
        );
    }

    /// The cutoff must not fire on a window that a real removal happened to
    /// close ON the log's last event: that window IS explicitly closed, so
    /// everything after it still counts. Distinguishing this from the case
    /// above is why the "explicitly closed" flag is carried rather than
    /// inferred from `end == log_end`.
    #[test]
    fn a_removal_landing_on_the_last_event_does_not_truncate_later_windows() {
        let raw = raw_from(
            vec![
                marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
                marker_event(4000, 1, 0, 0), // removal closes the first window
                marker_event(4000, 1, COMMANDER_LOCAL_ID, 1), // re-tag, still open
                marker_event(4000, 2, OVERHEAD_LOCAL_ID, 0), // log ends here
            ],
            vec![],
        );
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 4000), (4000, 4000)]);
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
        assert_eq!(final_marker(&res.open, &[1]).as_deref(), Some("arrow"));
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
        assert_eq!(final_marker(&res.open, &[1]).as_deref(), Some("3cd1c64a5000774488009d4d69455c5c"));
    }

    /// No `CBTS_IDTOGUID` mapping at all for the local id: last-resort
    /// fallback to the decimal id (still non-empty, still stable).
    #[test]
    fn marker_with_no_guid_mapping_falls_back_to_decimal_id() {
        let raw = raw_from(vec![marker_ev(100, 1, 999, 0)], vec![]);
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.open, &[1]).as_deref(), Some("999"));
    }

    /// `value == 0` clears the agent's current marker.
    #[test]
    fn removal_event_clears_marker() {
        let raw = raw_from(
            vec![marker_ev(100, 1, 42, 0), marker_ev(200, 1, 0, 0)],
            vec![marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32")],
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.open, &[1]), None);
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
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag present");
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
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag present");
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
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag present");
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
        assert!(final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).is_none());
    }

    /// Fix round 1 (reviewer-reported bug): a commander gets tagged, then
    /// later gets a routine squad overhead marker assigned (e.g. the
    /// lieutenant target-calls the commander with Arrow -- common in WvW).
    /// Both must survive concurrently: `marker` becomes the overhead one,
    /// `commander_tag` must NOT be silently cleared just because a
    /// different marker id was assigned on the same agent. Then a removal
    /// event (`value == 0`) ends both `open` instances -- per GW2EI's
    /// `CombatEventFactory` ("An end event ends all previous markers", no
    /// per-type carve-out) -- but (M4 post-rework EI-parity fix)
    /// `final_commander_tag` still reports the commander tag via the
    /// `ever_commander` fallback, since GW2EI's OWN `hasCommanderTag`
    /// likewise doesn't clear on a removal with no reassignment (see
    /// `MarkerResolution::ever_commander`'s doc comment). Only the overhead
    /// marker (`final_marker`, unaffected by this fix) actually goes back
    /// to `None`.
    #[test]
    fn commander_tag_survives_later_overhead_marker_assignment() {
        let raw = raw_from(
            vec![
                marker_ev(100, 1, 3201, 1), // commander tag assigned first
                marker_ev(200, 1, 42, 0),   // later: squad target-calls them with Arrow
            ],
            vec![
                marker_guid_mapping(3201, "1993fadb6fb70e4383a223a54d311f7d"), // PurpleCommanderTag
                marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32"),   // Arrow
            ],
        );
        let res = resolve_markers(&raw);

        // Both concurrently open: the overhead marker is the "current
        // marker", but the commander tag is NOT wiped out by it.
        assert_eq!(final_marker(&res.open, &[1]).as_deref(), Some("arrow"));
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag must survive the overhead assignment");
        assert_eq!(tag.variant, "purple-commander");

        // A removal event ends both `open` instances -- the overhead
        // marker goes back to `None` -- but the commander tag is still
        // reported via the `ever_commander` fallback (EI parity).
        let mut events = raw.events.clone();
        events.push(marker_ev(300, 1, 0, 0));
        let raw2 = raw_from(events, raw.guid_map.clone());
        let res2 = resolve_markers(&raw2);
        assert_eq!(final_marker(&res2.open, &[1]), None, "removal must clear the overhead marker");
        let tag2 = final_commander_tag(&res2.open, &res2.ever_commander, &res2.commander_segments, &[1])
            .expect("commander tag must still be reported via the ever_commander fallback after removal");
        assert_eq!(tag2.variant, "purple-commander");
    }

    /// The reverse order also holds: an overhead marker assigned first,
    /// then a commander tag assigned afterward, keeps both -- proving the
    /// concurrent tracking isn't order-dependent.
    #[test]
    fn overhead_marker_survives_later_commander_tag_assignment() {
        let raw = raw_from(
            vec![
                marker_ev(100, 1, 42, 0),   // Arrow first
                marker_ev(200, 1, 3201, 1), // commander tag assigned later
            ],
            vec![
                marker_guid_mapping(42, "c3a56f1e045e3848b07cbac5bbdd2c32"),
                marker_guid_mapping(3201, "1993fadb6fb70e4383a223a54d311f7d"),
            ],
        );
        let res = resolve_markers(&raw);
        assert_eq!(final_marker(&res.open, &[1]).as_deref(), Some("arrow"));
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag present");
        assert_eq!(tag.variant, "purple-commander");
    }

    /// A repeat assignment of the SAME marker id replaces the earlier open
    /// instance rather than stacking a duplicate -- mirrors GW2EI's "We
    /// can't have the same markers active at the same time on one Src".
    /// Distinguishes this from the "different id -> concurrent" cases
    /// above: same id must NOT coexist with itself.
    #[test]
    fn reassigning_the_same_marker_id_replaces_not_stacks() {
        let raw = raw_from(
            vec![
                marker_ev(100, 1, 3201, 1),
                marker_ev(200, 1, 3201, 1), // same commander tag id, reassigned
            ],
            vec![marker_guid_mapping(3201, "1993fadb6fb70e4383a223a54d311f7d")],
        );
        let res = resolve_markers(&raw);
        assert_eq!(res.open.get(&1).map(|v| v.len()), Some(1), "must not stack two instances of the same marker id");
        let tag = final_commander_tag(&res.open, &res.ever_commander, &res.commander_segments, &[1]).expect("commander tag present");
        assert_eq!(tag.variant, "purple-commander");
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
        assert_eq!(final_marker(&res.open, &[1, 2]).as_deref(), Some("999"));
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

#[cfg(test)]
mod marker_table_drift_tests {
    use super::{COMMANDER_TAG_VARIANTS, SQUAD_MARKER_NAMES};
    use crate::analysis::marker_icons::MARKERS;

    /// The GUID sets here and in `analysis::marker_icons` come from one
    /// upstream file but are transcribed by two different mechanisms -- these
    /// tables by hand from a fetched copy, the icon catalog by
    /// `scripts/gen_marker_catalog.py`. Regenerating one and not the other
    /// would leave a marker named but art-less, or drawn but unnamed, with
    /// nothing failing. Pin the sets against each other.
    #[test]
    fn marker_tables_agree_with_the_icon_catalog() {
        for (guid, name) in SQUAD_MARKER_NAMES {
            let m = MARKERS.iter().find(|m| m.guid == *guid)
                .unwrap_or_else(|| panic!("squad marker {name} ({guid}) missing from marker_icons"));
            assert_eq!(m.kind, "squad_marker", "{name} ({guid}) has the wrong kind");
        }
        for (guid, name) in COMMANDER_TAG_VARIANTS {
            let m = MARKERS.iter().find(|m| m.guid == *guid)
                .unwrap_or_else(|| panic!("tag {name} ({guid}) missing from marker_icons"));
            assert!(
                m.kind == "commander_tag" || m.kind == "catmander_tag",
                "{name} ({guid}) has the wrong kind: {}", m.kind,
            );
        }
        // And the reverse, so a GUID added to the icon catalog cannot go
        // unnamed in the legacy tables.
        let named = SQUAD_MARKER_NAMES.len() + COMMANDER_TAG_VARIANTS.len();
        assert_eq!(named, MARKERS.len(), "one table gained a GUID the other did not");
    }
}
