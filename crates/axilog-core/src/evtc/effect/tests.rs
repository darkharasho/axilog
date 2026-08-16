//! Wire-level tests for the effect decode.
//!
//! Every one of these builds the raw bytes by hand rather than reusing a
//! fixture, because the whole risk in this module is byte-offset drift:
//! three arcdps generations pack duration, tracking id, position and
//! orientation into overlapping reuses of the same twelve trailing bytes,
//! and a fixture-level assertion ("we decoded 5056 effects") cannot tell a
//! correct unpack from one that swapped two fields.

use super::*;
use crate::evtc::header::RawHeader;

fn blank(time: u64, sc_val: u8) -> RawEvent {
    RawEvent {
        time,
        src_agent: 0,
        dst_agent: 0,
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
        is_ninety: 0,
        is_fifty: 0,
        is_moving: 0,
        is_statechange: sc_val,
        is_flanking: 0,
        is_shields: 0,
        is_offcycle: 0,
        pad: 0,
    }
}

/// A log at an arcdps build new enough for both GUID gates.
fn log(events: Vec<RawEvent>) -> RawLog {
    log_at_build(events, "20250101")
}

fn log_at_build(events: Vec<RawEvent>, build: &str) -> RawLog {
    let guid_map = crate::evtc::guid::decode_guid_mappings(&events);
    RawLog {
        header: RawHeader { build: build.to_string(), revision: 1, boss_id: 1 },
        agents: vec![],
        skills: vec![],
        events,
        guid_map,
    }
}

/// `CBTS_IDTOGUID` for content type EFFECT, optionally with a default
/// duration in `buff_dmg` (f32 bits).
fn effect_guid_row(local_id: u32, guid: [u8; 16], default_duration: f32) -> RawEvent {
    let mut e = blank(0, sc::ID_TO_GUID);
    e.src_agent = u64::from_le_bytes(guid[0..8].try_into().unwrap());
    e.dst_agent = u64::from_le_bytes(guid[8..16].try_into().unwrap());
    e.overstack = 0; // ContentLocal::Effect
    e.skillid = local_id;
    e.buff_dmg = default_duration.to_bits() as i32;
    e
}

/// Sets the four bytes at wire offsets 48..52, which every generation past
/// the first reads as one little-endian `u32` duration.
fn set_head4(e: &mut RawEvent, v: u32) {
    let b = v.to_le_bytes();
    e.iff = b[0];
    e.buff = b[1];
    e.result = b[2];
    e.is_activation = b[3];
}

/// Sets wire offsets 52..56 -- the CBTS51 tracking id.
fn set_mid4(e: &mut RawEvent, v: u32) {
    let b = v.to_le_bytes();
    e.is_buffremove = b[0];
    e.is_ninety = b[1];
    e.is_fifty = b[2];
    e.is_moving = b[3];
}

const GUID_A: [u8; 16] = [
    0xE7, 0xC5, 0x0E, 0x0E, 0x14, 0x8C, 0xBE, 0x44, 0xBB, 0x27, 0x70, 0xAF, 0x2D, 0x67, 0x50, 0xA4,
];

// ---------------------------------------------------------------------
// generation 1: CBTS_EFFECT_45
// ---------------------------------------------------------------------

#[test]
fn cbts45_reads_position_when_unanchored_and_dst_when_anchored() {
    let mut ground = blank(100, sc::EFFECT_45);
    ground.skillid = 7;
    ground.src_agent = 0xAA;
    ground.value = 1.5f32.to_bits() as i32;
    ground.buff_dmg = (-2.5f32).to_bits() as i32;
    ground.overstack = 3.5f32.to_bits();

    let mut anchored = blank(200, sc::EFFECT_45);
    anchored.skillid = 7;
    anchored.src_agent = 0xAA;
    anchored.dst_agent = 0xBB;
    // Same bytes as above -- they must be IGNORED once `dst_agent` is set.
    anchored.value = 1.5f32.to_bits() as i32;

    let idx = decode(&log(vec![ground, anchored]));
    assert_eq!(idx.events.len(), 2);
    assert_eq!(idx.events[0].dst, None);
    assert_eq!(idx.events[0].position, [1.5, -2.5, 3.5]);
    assert_eq!(idx.events[1].dst, Some(0xBB));
    assert_eq!(idx.events[1].position, [0.0; 3]);
}

#[test]
fn cbts45_orientation_is_two_raw_floats_and_a_negated_third() {
    let mut e = blank(1, sc::EFFECT_45);
    e.skillid = 7;
    let x = 0.25f32.to_bits().to_le_bytes();
    let y = 0.75f32.to_bits().to_le_bytes();
    e.iff = x[0];
    e.buff = x[1];
    e.result = x[2];
    e.is_activation = x[3];
    e.is_buffremove = y[0];
    e.is_ninety = y[1];
    e.is_fifty = y[2];
    e.is_moving = y[3];
    e.pad = 0.5f32.to_bits();

    let idx = decode(&log(vec![e]));
    // The z negation is not cosmetic: GW2EI flips it on every generation
    // because arcdps's z axis points the other way.
    assert_eq!(idx.events[0].orientation, [0.25, 0.75, -0.5]);
}

#[test]
fn cbts45_ignores_the_guid_default_duration() {
    // `EffectEventCBTS45` overrides `ComputeEndTime` to skip the duration
    // fallback entirely, so a default on the GUID row must not leak in.
    let guid = effect_guid_row(7, GUID_A, 5000.0);
    let mut e = blank(1, sc::EFFECT_45);
    e.skillid = 7;
    let idx = decode(&log(vec![guid, e]));
    assert_eq!(idx.events[0].duration, 0);
}

#[test]
fn cbts45_has_no_end_form() {
    // A `skillid == 0` row in this generation is dropped outright, not
    // treated as an end marker.
    let mut end = blank(1, sc::EFFECT_45);
    end.skillid = 0;
    assert!(decode(&log(vec![end])).events.is_empty());
}

// ---------------------------------------------------------------------
// generation 2: CBTS_EFFECT_51
// ---------------------------------------------------------------------

#[test]
fn cbts51_unpacks_duration_tracking_id_and_orientation_from_distinct_bytes() {
    let mut e = blank(10, sc::EFFECT_51);
    e.skillid = 9;
    set_head4(&mut e, 4200);
    set_mid4(&mut e, 0xDEAD_BEEF);
    // Orientation: three i16 milliradians at offsets 58..64.
    e.is_shields = 100i16.to_le_bytes()[0];
    e.is_offcycle = 100i16.to_le_bytes()[1];
    let mut p = [0u8; 4];
    p[0..2].copy_from_slice(&(-250i16).to_le_bytes());
    p[2..4].copy_from_slice(&500i16.to_le_bytes());
    e.pad = u32::from_le_bytes(p);

    let idx = decode(&log(vec![e]));
    let ev = idx.events[0];
    assert_eq!(ev.duration, 4200);
    assert_eq!(ev.tracking_id, 0xDEAD_BEEF);
    assert_eq!(ev.orientation, [0.1, -0.25, -0.5]);
}

#[test]
fn cbts51_zero_duration_falls_back_to_the_guid_default() {
    let guid = effect_guid_row(9, GUID_A, 3000.0);
    let mut e = blank(10, sc::EFFECT_51);
    e.skillid = 9;
    // head4 left at 0 -- the row carries no duration of its own.
    let idx = decode(&log(vec![guid, e]));
    assert_eq!(idx.events[0].duration, 3000);
    assert_eq!(idx.id_for_guid(&GUID_A), Some(9));
}

#[test]
fn extra_guid_data_is_gated_on_the_arcdps_build() {
    // `EffectGUIDEvent` reads the default duration only above
    // ExtraDataInGUIDEvents, and the gate is STRICTLY greater -- a log
    // recorded exactly at that build gets no default.
    let guid = effect_guid_row(9, GUID_A, 3000.0);
    let mut e = blank(10, sc::EFFECT_51);
    e.skillid = 9;
    let at = decode(&log_at_build(vec![guid.clone(), e.clone()], "20241030"));
    assert_eq!(at.events[0].duration, 0);
    let above = decode(&log_at_build(vec![guid, e], "20241031"));
    assert_eq!(above.events[0].duration, 3000);
}

#[test]
fn guid_table_is_empty_below_the_functional_build() {
    let guid = effect_guid_row(9, GUID_A, 0.0);
    let mut e = blank(10, sc::EFFECT_51);
    e.skillid = 9;
    let idx = decode(&log_at_build(vec![guid, e], "20220708"));
    // The effect itself still decodes; it just cannot be NAMED, which is
    // exactly what stops an effect-keyed finder from firing.
    assert_eq!(idx.events.len(), 1);
    assert_eq!(idx.id_for_guid(&GUID_A), None);
}

#[test]
fn cbts51_end_row_closes_the_last_create_at_or_before_it() {
    let mut first = blank(10, sc::EFFECT_51);
    first.skillid = 9;
    set_mid4(&mut first, 77);
    let mut second = blank(20, sc::EFFECT_51);
    second.skillid = 9;
    set_mid4(&mut second, 77);
    let mut later = blank(90, sc::EFFECT_51);
    later.skillid = 9;
    set_mid4(&mut later, 77);
    let mut end = blank(50, sc::EFFECT_51);
    end.skillid = 0; // end form
    set_mid4(&mut end, 77);

    let idx = decode(&log(vec![first, second, later, end]));
    assert_eq!(idx.events.len(), 3);
    // Closes the create at t=20 -- the LAST one at or before t=50 -- and
    // leaves both the earlier and the later create open.
    assert_eq!(idx.events[0].dynamic_end, None);
    assert_eq!(idx.events[1].dynamic_end, Some(50));
    assert_eq!(idx.events[2].dynamic_end, None);
}

#[test]
fn an_untracked_create_is_never_closed() {
    let mut create = blank(10, sc::EFFECT_51);
    create.skillid = 9; // tracking id left 0
    let mut end = blank(50, sc::EFFECT_51);
    end.skillid = 0;
    let idx = decode(&log(vec![create, end]));
    assert_eq!(idx.events[0].dynamic_end, None);
}

// ---------------------------------------------------------------------
// generation 3: the split ground/agent events
// ---------------------------------------------------------------------

#[test]
fn ground_create_unpacks_six_shorts_across_dst_agent_and_value() {
    let mut e = blank(5, sc::EFFECT_GROUND_CREATE);
    e.skillid = 3;
    let shorts: [i16; 6] = [100, -200, 300, 250, -750, 1000];
    let mut v = [0u8; 12];
    for (i, s) in shorts.iter().enumerate() {
        v[i * 2..i * 2 + 2].copy_from_slice(&s.to_le_bytes());
    }
    e.dst_agent = u64::from_le_bytes(v[0..8].try_into().unwrap());
    e.value = i32::from_le_bytes(v[8..12].try_into().unwrap());
    e.pad = 42; // tracking id lives in the pad word for this generation

    let idx = decode(&log(vec![e]));
    let ev = idx.events[0];
    // Position is x10; orientation is milliradians with a negated z.
    assert_eq!(ev.position, [1000.0, -2000.0, 3000.0]);
    assert_eq!(ev.orientation, [0.25, -0.75, -1.0]);
    assert_eq!(ev.tracking_id, 42);
    // A ground effect is never agent-anchored, even though the bytes it
    // packs its position into are the same ones `dst_agent` normally uses.
    assert_eq!(ev.dst, None);
}

#[test]
fn ground_scale_defaults_to_one_when_the_row_carries_zero() {
    let mut zero = blank(5, sc::EFFECT_GROUND_CREATE);
    zero.skillid = 3;
    let mut scaled = blank(6, sc::EFFECT_GROUND_CREATE);
    scaled.skillid = 3;
    let s = 2500u16.to_le_bytes();
    scaled.is_shields = s[0];
    scaled.is_offcycle = s[1];

    let idx = decode(&log(vec![zero, scaled]));
    assert_eq!(idx.events[0].scale, 1.0);
    assert_eq!(idx.events[1].scale, 2.5);
}

#[test]
fn agent_create_is_always_anchored_and_tracks_via_pad() {
    let mut e = blank(5, sc::EFFECT_AGENT_CREATE);
    e.skillid = 3;
    e.src_agent = 0xAA;
    e.dst_agent = 0xBB;
    e.pad = 9;
    set_head4(&mut e, 1234);

    let idx = decode(&log(vec![e]));
    assert_eq!(idx.events[0].dst, Some(0xBB));
    assert_eq!(idx.events[0].duration, 1234);
    assert_eq!(idx.events[0].tracking_id, 9);
}

#[test]
fn tracking_id_namespaces_do_not_cross_between_ground_and_agent() {
    // Both generations key their tracking ids off the same pad word, and
    // GW2EI keeps three separate dictionaries. A ground remove must not
    // close an agent create that happens to share the id.
    let mut ground = blank(10, sc::EFFECT_GROUND_CREATE);
    ground.skillid = 3;
    ground.pad = 5;
    let mut agent = blank(11, sc::EFFECT_AGENT_CREATE);
    agent.skillid = 3;
    agent.pad = 5;
    let mut ground_end = blank(20, sc::EFFECT_GROUND_REMOVE);
    ground_end.pad = 5;

    let idx = decode(&log(vec![ground, agent, ground_end]));
    assert_eq!(idx.events[0].dynamic_end, Some(20));
    assert_eq!(idx.events[1].dynamic_end, None);
}

// ---------------------------------------------------------------------
// cross-cutting
// ---------------------------------------------------------------------

#[test]
fn non_static_platform_effects_are_dropped() {
    // See the module doc -- this reproduces GW2EI's release-build filter.
    for sc_val in [sc::EFFECT_51, sc::EFFECT_GROUND_CREATE, sc::EFFECT_AGENT_CREATE] {
        let mut e = blank(1, sc_val);
        e.skillid = 3;
        e.is_flanking = 1;
        let idx = decode(&log(vec![e.clone()]));
        assert!(idx.events.is_empty(), "sc {sc_val} should have been dropped");
        assert!(!idx.has_effect_data());
        assert!(!has_effect_data(&log(vec![e])));
    }
}

#[test]
fn cheap_probe_agrees_with_the_full_decode() {
    // `has_effect_data` exists to answer the enable condition without
    // building the index; if the two ever disagree, finder availability
    // and finder firing disagree too.
    let mut end_only = blank(1, sc::EFFECT_51);
    end_only.skillid = 0;
    let mut real = blank(2, sc::EFFECT_AGENT_CREATE);
    real.skillid = 3;
    let mut moving = blank(3, sc::EFFECT_GROUND_CREATE);
    moving.skillid = 3;
    moving.is_flanking = 1;

    for evs in [
        vec![],
        vec![end_only.clone()],
        vec![moving.clone()],
        vec![end_only, moving],
        vec![real],
    ] {
        let l = log(evs.clone());
        assert_eq!(has_effect_data(&l), decode(&l).has_effect_data(), "{evs:?}");
    }
}

#[test]
fn last_guid_mapping_wins_for_a_duplicated_guid() {
    let idx = decode(&log(vec![
        effect_guid_row(1, GUID_A, 0.0),
        effect_guid_row(2, GUID_A, 0.0),
    ]));
    assert_eq!(idx.id_for_guid(&GUID_A), Some(2));
}
