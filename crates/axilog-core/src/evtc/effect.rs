//! arcdps effect events -- the three generations of `CBTS_EFFECT*`.
//!
//! An "effect" here is a visual: the ring under a well, the pulse on a
//! shout, the marker a trap leaves behind. arcdps has emitted them under
//! three different encodings, and GW2EI folds all three into one
//! `EffectEvent` type (`ParsedData/CombatEvents/StatusEvents/EffectEvents/`).
//! This module is that fold.
//!
//! | generation | statechange | shape |
//! |---|---|---|
//! | 1st | [`sc::EFFECT_45`] | one row, create only |
//! | 2nd | [`sc::EFFECT_51`] | one row; `skillid == 0` is an END row |
//! | 3rd | [`sc::EFFECT_GROUND_CREATE`] / [`sc::EFFECT_AGENT_CREATE`] (+ their REMOVE twins) | create and end are separate rows, ground- and agent-anchored effects are separate statechanges |
//!
//! # Why this project decodes them
//!
//! Purely visual events look skippable, and this project skipped them for
//! a long time. They are not: 175 of GW2EI's `InstantCastFinder`
//! constructions are keyed on an effect GUID
//! (`EIData/InstantCastFinders/EffectInstantCastFinder/`), because for a
//! great many traits and sigils the ONLY trace in the log that the thing
//! fired is the visual it spawned. Without this decode,
//! `analysis::instant_cast` -- and so `skillMap`'s `isInstantCast` and the
//! three proc flags -- silently undercounts on every log recorded with
//! effect data.
//!
//! # Identity: effect ids are session-local
//!
//! An effect row names its effect by a session-local id in `skillid`, NOT
//! by anything stable. The mapping to a stable 16-byte GUID arrives
//! separately, as [`sc::ID_TO_GUID`] rows of content type
//! [`ContentType::Effect`] -- which `evtc::guid` already decodes. So a
//! consumer that wants "did THIS effect happen" resolves its GUID through
//! [`EffectIndex::id_for_guid`] first; matching on the raw effect id across
//! logs is meaningless.
//!
//! # Deliberate divergence: non-static-platform effects are DROPPED
//!
//! GW2EI discards any effect flagged `OnNonStaticPlatform` from
//! `CombatData.EffectEvents` in every non-DEBUG build
//! (`CombatEventFactory.cs:348-353,496-501,511-516`). An effect riding a
//! moving platform has coordinates in the platform's frame, not the map's,
//! so its position is nonsense to anything that mixes it with agent
//! positions. Reproduced here rather than "fixed", because the drop is
//! observable through the finders: `HasEffectData` and every effect-keyed
//! finder see the filtered list in EI, and this port has to agree.

use std::collections::BTreeMap;

use super::event::{sc, RawEvent};
use super::guid::ContentType;
use super::RawLog;

/// GW2EI `ArcDPSBuilds.ExtraDataInGUIDEvents` (`ArcDPSEnums.cs:32`) -- the
/// arcdps build from which a `CBTS_IDTOGUID` row for an effect carries a
/// default duration and an effect type alongside the GUID itself.
///
/// GW2EI's `EffectGUIDEvent` gates on `Build > ExtraDataInGUIDEvents`
/// (strictly greater -- `EffectGUIDEvent.cs:11`), unlike its
/// `MarkerGUIDEvent` sibling which uses `>=` on the same constant. That
/// inconsistency is upstream's; it is reproduced here rather than
/// harmonized, because the whole point of this module is to agree with EI
/// on which finders fire.
pub const EXTRA_DATA_IN_GUID_EVENTS: i64 = 20241030;

/// GW2EI `ArcDPSBuilds.FunctionalIDToGUIDEvents` (`ArcDPSEnums.cs:17`) --
/// the build from which `CBTS_IDTOGUID` rows are usable at all.
pub const FUNCTIONAL_ID_TO_GUID_EVENTS: i64 = 20220709;

/// One decoded effect, with the generation-specific encodings already
/// resolved into common fields.
///
/// Fields GW2EI computes lazily from the parsed log (its
/// `ComputeLifespan` family, which consults buff removals and secondary
/// effects) are NOT here: those are analysis, and they need a log this
/// decoder does not have.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectEvent {
    /// Raw arcdps time, the same clock as [`RawEvent::time`].
    pub time: u64,
    /// The agent the effect is attributed to -- its caster/owner.
    pub src: u64,
    /// The agent the effect is ANCHORED to, when it follows an agent
    /// rather than sitting at a fixed point. `None` is GW2EI's
    /// `!IsAroundDst`, in which case [`EffectEvent::position`] is
    /// meaningful instead.
    pub dst: Option<u64>,
    /// Session-local effect id (`skillid`). Resolve through
    /// [`EffectIndex::id_for_guid`] before comparing across logs.
    pub effect_id: u32,
    /// Effect duration in ms, `0` when the row carries none and the
    /// GUID row supplied no default either.
    pub duration: i64,
    /// Links a create row to its END row. `0` means untracked, which
    /// GW2EI treats as "never gets a dynamic end".
    pub tracking_id: u32,
    /// Map position, only meaningful when [`EffectEvent::dst`] is `None`.
    pub position: [f32; 3],
    /// Rotation around each axis, in radians.
    pub orientation: [f32; 3],
    /// Ground-effect scale; `1.0` when the row carries none.
    pub scale: f32,
    /// Ground-effect flags byte (`EffectEventGroundCreate`'s `Flags`);
    /// arcdps documents no meaning for the individual bits, so this is
    /// carried verbatim and interpreted nowhere.
    pub flags: u8,
    /// End time supplied by a matching END row, if one arrived. `None`
    /// leaves the lifespan to be computed from [`EffectEvent::duration`].
    pub dynamic_end: Option<u64>,
}

/// Which statechange an [`EffectEvent`] came from -- kept only so the
/// tracking-id namespaces stay separate, exactly as GW2EI keeps three
/// separate `*EffectEventsByTrackingID` dictionaries
/// (`StatusEventsContainer`). A ground REMOVE must not close an agent
/// create that happens to share a tracking id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Track {
    Combined,
    Ground,
    Agent,
}

/// Effect events plus the GUID mapping needed to name them.
#[derive(Debug, Clone, Default)]
pub struct EffectIndex {
    /// Every surviving effect, in log order.
    pub events: Vec<EffectEvent>,
    /// Stable GUID -> session-local effect id.
    by_guid: BTreeMap<[u8; 16], u32>,
}

impl EffectIndex {
    /// The session-local effect id a stable GUID was mapped to, if this log
    /// mapped it at all.
    ///
    /// Last mapping wins on a duplicate GUID, matching GW2EI's
    /// `EffectGUIDEventsByGUID[guid] = event` assignment
    /// (`CombatEventFactory.cs:371`).
    pub fn id_for_guid(&self, guid: &[u8; 16]) -> Option<u32> {
        self.by_guid.get(guid).copied()
    }

    /// GW2EI's `CombatData.HasEffectData` (`CombatData.cs:614`) -- whether
    /// any effect survived decoding.
    pub fn has_effect_data(&self) -> bool {
        !self.events.is_empty()
    }
}

/// Cheap `HasEffectData` probe that does not build the whole index.
///
/// Exactly the predicate [`decode`] applies before pushing a row -- an END
/// row (`skillid == 0`, first two generations) is not an effect, and a
/// non-static-platform effect is dropped -- so the two cannot disagree.
pub fn has_effect_data(raw: &RawLog) -> bool {
    raw.events.iter().any(|e| match e.is_statechange {
        sc::EFFECT_45 => e.skillid != 0,
        sc::EFFECT_51 => e.skillid != 0 && e.is_flanking == 0,
        sc::EFFECT_GROUND_CREATE | sc::EFFECT_AGENT_CREATE => e.is_flanking == 0,
        _ => false,
    })
}

fn f32_at(bytes: [u8; 4]) -> f32 {
    f32::from_le_bytes(bytes)
}

fn u32_at(bytes: [u8; 4]) -> u32 {
    u32::from_le_bytes(bytes)
}

/// Bytes 48..52 of the raw event -- `iff`, `buff`, `result`,
/// `is_activation` -- which every generation past the first reuses as one
/// little-endian `u32` duration.
fn head4(e: &RawEvent) -> [u8; 4] {
    [e.iff, e.buff, e.result, e.is_activation]
}

/// Bytes 52..56 -- `is_buffremove`, `is_ninety`, `is_fifty`, `is_moving`.
fn mid4(e: &RawEvent) -> [u8; 4] {
    [e.is_buffremove, e.is_ninety, e.is_fifty, e.is_moving]
}

/// Decode every effect event in the log, resolving END rows onto their
/// creates.
///
/// One pass; the END resolution is a second pass over the collected
/// tracking ids, which is what lets an END row close the LAST create at or
/// before its own time (GW2EI's
/// `effectEvents.LastOrDefault(x => x.Time <= Time)`).
pub fn decode(raw: &RawLog) -> EffectIndex {
    let build = raw.header.build.parse::<i64>().unwrap_or(i64::MIN);
    let extra_data = build > EXTRA_DATA_IN_GUID_EVENTS;
    // Before this build arcdps emitted `CBTS_IDTOGUID` rows that GW2EI
    // does not trust, and it skips the whole case
    // (`CombatEventFactory.cs:362-363`). With no GUID table, no
    // effect-keyed finder can resolve its effect id -- which is the
    // correct outcome, not a bug to route around.
    let functional_guids = build >= FUNCTIONAL_ID_TO_GUID_EVENTS;

    // Effect-typed `CBTS_IDTOGUID` rows carry the GUID and, on new enough
    // arcdps, a default duration in `buff_dmg` (as f32 bits).
    let mut by_guid: BTreeMap<[u8; 16], u32> = BTreeMap::new();
    let mut default_duration: BTreeMap<u32, f32> = BTreeMap::new();
    if functional_guids {
        for m in raw.guid_map.iter().filter(|m| m.content_type == ContentType::Effect) {
            by_guid.insert(m.guid, m.local_id);
        }
    }
    if functional_guids && extra_data {
        for e in raw.events.iter().filter(|e| e.is_statechange == sc::ID_TO_GUID) {
            if ContentType::from_u32(e.overstack) == ContentType::Effect {
                default_duration.insert(e.skillid, f32::from_bits(e.buff_dmg as u32));
            }
        }
    }

    // `(track, tracking_id) -> indices into `events``, in log order.
    let mut tracked: BTreeMap<(Track, u32), Vec<usize>> = BTreeMap::new();
    let mut ends: Vec<(Track, u32, u64)> = Vec::new();
    let mut events: Vec<EffectEvent> = Vec::new();

    for e in &raw.events {
        let (ev, track) = match e.is_statechange {
            sc::EFFECT_45 => {
                // No END form ever reached this generation.
                if e.skillid == 0 {
                    continue;
                }
                (decode_combined_45(e), Track::Combined)
            }
            sc::EFFECT_51 => {
                if e.skillid == 0 {
                    ends.push((Track::Combined, u32_at(mid4(e)), e.time));
                    continue;
                }
                (decode_combined_51(e, &default_duration), Track::Combined)
            }
            sc::EFFECT_GROUND_CREATE => {
                (decode_ground_create(e, &default_duration), Track::Ground)
            }
            sc::EFFECT_GROUND_REMOVE => {
                ends.push((Track::Ground, e.pad, e.time));
                continue;
            }
            sc::EFFECT_AGENT_CREATE => (decode_agent_create(e, &default_duration), Track::Agent),
            sc::EFFECT_AGENT_REMOVE => {
                ends.push((Track::Agent, e.pad, e.time));
                continue;
            }
            _ => continue,
        };
        // See the module doc: GW2EI drops these outside DEBUG, and the
        // finders observe the filtered list.
        let Some(ev) = ev else { continue };
        if ev.tracking_id != 0 {
            tracked.entry((track, ev.tracking_id)).or_default().push(events.len());
        }
        events.push(ev);
    }

    for (track, tracking_id, time) in ends {
        if tracking_id == 0 {
            continue;
        }
        let Some(idx) = tracked.get(&(track, tracking_id)) else { continue };
        // The LAST create at or before the end row, and only if that
        // create has no end yet ("We can only set the EndEventOnce",
        // `EffectEvent.SetDynamicEndTime`).
        let Some(&i) = idx.iter().rev().find(|&&i| events[i].time <= time) else { continue };
        if events[i].dynamic_end.is_none() {
            events[i].dynamic_end = Some(time);
        }
    }

    EffectIndex { events, by_guid }
}

/// `dst_agent != 0` anchors the effect to an agent; otherwise the row's
/// `value`/`buff_dmg`/`overstack` are the three position floats
/// (`NonSplitEffectEvent`).
fn combined_anchor(e: &RawEvent) -> (Option<u64>, [f32; 3]) {
    if e.dst_agent != 0 {
        (Some(e.dst_agent), [0.0; 3])
    } else {
        (
            None,
            [
                f32::from_bits(e.value as u32),
                f32::from_bits(e.buff_dmg as u32),
                f32::from_bits(e.overstack),
            ],
        )
    }
}

fn decode_combined_45(e: &RawEvent) -> Option<EffectEvent> {
    let (dst, position) = combined_anchor(e);
    Some(EffectEvent {
        time: e.time,
        src: e.src_agent,
        dst,
        effect_id: e.skillid,
        // This generation carries neither a duration nor a tracking id,
        // and `EffectEventCBTS45` overrides `ComputeEndTime` to ignore the
        // GUID default as well.
        duration: 0,
        tracking_id: 0,
        position,
        // Two raw f32s, then a NEGATED third out of the pad word.
        orientation: [f32_at(head4(e)), f32_at(mid4(e)), -f32_at(e.pad.to_le_bytes())],
        scale: 1.0,
        flags: 0,
        dynamic_end: None,
    })
}

fn decode_combined_51(e: &RawEvent, defaults: &BTreeMap<u32, f32>) -> Option<EffectEvent> {
    if e.is_flanking != 0 {
        return None;
    }
    let (dst, position) = combined_anchor(e);
    // Orientation is three i16 milliradians spanning `is_shields`,
    // `is_offcycle` and the four pad bytes -- offsets 58..64.
    let p = e.pad.to_le_bytes();
    let o = |a: u8, b: u8| i16::from_le_bytes([a, b]) as f32 / 1000.0;
    Some(EffectEvent {
        time: e.time,
        src: e.src_agent,
        dst,
        effect_id: e.skillid,
        duration: resolve_duration(u32_at(head4(e)), e.skillid, defaults),
        tracking_id: u32_at(mid4(e)),
        position,
        orientation: [o(e.is_shields, e.is_offcycle), o(p[0], p[1]), -o(p[2], p[3])],
        scale: 1.0,
        flags: 0,
        dynamic_end: None,
    })
}

fn decode_ground_create(e: &RawEvent, defaults: &BTreeMap<u32, f32>) -> Option<EffectEvent> {
    if e.is_flanking != 0 {
        return None;
    }
    // Six i16 packed across `dst_agent` (4 of them) and `value` (2):
    // position x/y/z at 1/10 scale, then orientation x/y/z in
    // milliradians with z negated.
    let mut v = [0u8; 12];
    v[0..8].copy_from_slice(&e.dst_agent.to_le_bytes());
    v[8..12].copy_from_slice(&e.value.to_le_bytes());
    let s = |i: usize| i16::from_le_bytes([v[i * 2], v[i * 2 + 1]]) as f32;
    // A `u16` of milli-units, defaulting to 1.0 when the row leaves it 0.
    let scale16 = |a: u8, b: u8| match u16::from_le_bytes([a, b]) {
        0 => 1.0,
        n => n as f32 / 1000.0,
    };
    Some(EffectEvent {
        time: e.time,
        src: e.src_agent,
        // Ground effects are never agent-anchored: GW2EI's
        // `EffectEventGroundCreate` never sets `_dst`, and
        // `CombatEventFactory` correspondingly never files one under
        // `EffectEventsByDst`.
        dst: None,
        effect_id: e.skillid,
        duration: resolve_duration(u32_at(head4(e)), e.skillid, defaults),
        tracking_id: e.pad,
        position: [s(0) * 10.0, s(1) * 10.0, s(2) * 10.0],
        orientation: [s(3) / 1000.0, s(4) / 1000.0, -s(5) / 1000.0],
        scale: scale16(e.is_shields, e.is_offcycle),
        flags: e.is_buffremove,
        dynamic_end: None,
    })
}

fn decode_agent_create(e: &RawEvent, defaults: &BTreeMap<u32, f32>) -> Option<EffectEvent> {
    if e.is_flanking != 0 {
        return None;
    }
    Some(EffectEvent {
        time: e.time,
        src: e.src_agent,
        // Always agent-anchored -- unlike the combined generations, an
        // agent-create row does not fall back to a position.
        dst: Some(e.dst_agent),
        effect_id: e.skillid,
        duration: resolve_duration(u32_at(head4(e)), e.skillid, defaults),
        tracking_id: e.pad,
        position: [0.0; 3],
        orientation: [0.0; 3],
        scale: 1.0,
        flags: 0,
        dynamic_end: None,
    })
}

/// A zero duration on the row falls back to the GUID row's default, capped
/// at `i32::MAX` -- GW2EI's own comment explains the cap ("13 days is more
/// than enough to cover a log's duration") and it exists so `start +
/// duration` cannot overflow downstream.
fn resolve_duration(raw: u32, effect_id: u32, defaults: &BTreeMap<u32, f32>) -> i64 {
    if raw != 0 {
        return i64::from(raw);
    }
    match defaults.get(&effect_id) {
        Some(&d) if d > 0.0 => (d as i64).min(i64::from(i32::MAX)),
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
