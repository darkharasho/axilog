//! Engine tests for the instant-cast finder port.
//!
//! These pin the SEMANTICS the 571-row catalog is evaluated under --
//! caster selection, ICD grouping, availability gating and the two
//! deliberate asymmetries (`Initial` applies, ICD-before-checks). The
//! catalog's own faithfulness is a separate question, answered by the
//! generator's accounting rather than by tests here.

use super::*;
use crate::evtc::{buff_remove, RawAgent, RawEvent, RawHeader};

/// [`super::compute`] with no animated casts to check against, which is
/// every test here except the `Check::NoAnimatedCast` ones (which call the
/// real one with a populated index). An empty index makes `is_casting`
/// false, so a `NoAnimatedCast` check passes -- see the module doc.
fn compute(raw: &RawLog, enc: &Encounter, finders: &[FinderDef]) -> Vec<InstantCastEvent> {
    super::compute(raw, enc, finders, &crate::analysis::rotation::AnimatedCasts::default())
}
use crate::model::{Player, Team};

fn base() -> RawEvent {
    RawEvent {
        time: 0,
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
        iff: 1,
        buff: 0,
        result: 0,
        is_activation: 0,
        is_buffremove: 0,
        is_ninety: 0,
        is_fifty: 0,
        is_moving: 0,
        is_statechange: 0,
        is_flanking: 0,
        is_shields: 0,
        is_offcycle: 0,
        pad: 0,
    }
}

/// A pre-buff-rework buff APPLY row (`buff == 1`, positive duration).
/// Every test here uses the pre-era shape so that an empty header build
/// string -- which reads as pre-rework -- is enough.
fn apply(time: u64, buff_id: u32, owner: u64, applier: u64, duration: i32) -> RawEvent {
    RawEvent {
        time,
        skillid: buff_id,
        dst_agent: owner,
        src_agent: applier,
        buff: 1,
        value: duration,
        ..base()
    }
}

fn remove_all(time: u64, buff_id: u32, owner: u64) -> RawEvent {
    RawEvent {
        time,
        skillid: buff_id,
        src_agent: owner,
        buff: 1,
        is_buffremove: buff_remove::ALL,
        ..base()
    }
}

fn hit(time: u64, skill_id: u32, from: u64, to: u64, dmg: i32) -> RawEvent {
    RawEvent { time, skillid: skill_id, src_agent: from, dst_agent: to, value: dmg, ..base() }
}

/// Registers `instid -> addr` so master resolution can find the owner.
fn reg(addr: u64, instid: u16) -> RawEvent {
    RawEvent { src_agent: addr, src_instid: instid, ..base() }
}

fn agent(addr: u64, prof: u32, name: &str) -> RawAgent {
    let mut name_raw = name.as_bytes().to_vec();
    name_raw.push(0);
    RawAgent {
        addr,
        prof,
        is_elite: 0xffff_ffff,
        toughness: 0,
        concentration: 0,
        healing: 0,
        hitbox_width: 0,
        condition: 0,
        hitbox_height: 0,
        name_raw,
    }
}

/// `is_elite == 0xffff_ffff` is what marks a non-player agent, so this
/// helper differs from [`agent`] only in that field.
fn player_agent(addr: u64) -> RawAgent {
    RawAgent { is_elite: 0, ..agent(addr, 1, "Someone") }
}

fn player_with(addr: u64, profession: &str, elite: &str) -> Player {
    Player {
        agent_addr: addr,
        account: format!("A{addr}"),
        character: format!("C{addr}"),
        profession: profession.into(),
        elite_spec: elite.into(),
        team: "red".into(),
        subgroup: 1,
        in_squad: true,
        commander: false,
        marker: None,
        commander_tag: None,
        guild_id: None,
        agent_addrs: vec![addr],
    }
}

fn enc(players: Vec<Player>) -> Encounter {
    Encounter {
        kind: "wvw".into(),
        map: String::new(),
        duration_ms: 10_000,
        build: String::new(),
        revision: 1,
        recorded_by: None,
        teams: vec![Team { color: "red".into(), team_id: 1, guid: None, shard_id: None }],
        players,
        enemies: Vec::new(),
        markers: Vec::new(),
        tick_rate: None,
        objectives: Vec::new(),
        started_at_unix: None, map_id: None,
    }
}

fn raw(agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
    RawLog {
        header: RawHeader { build: String::new(), revision: 1, boss_id: 1 },
        agents,
        skills: vec![],
        events,
        guid_map: vec![],
    }
}

const BUFF: u32 = 700;
const SKILL: u32 = 42;

fn gain_finder() -> FinderDef {
    FinderDef {
        skill_id: SKILL,
        source: "TestHelper",
        trigger: Trigger::BuffGain { buff_id: BUFF },
        ..FinderDef::DEFAULT
    }
}

/// The base case: a buff APPLY is evidence that its RECIPIENT cast the
/// skill that grants it, not that the applier did.
#[test]
fn a_buff_gain_credits_the_recipient_not_the_applier() {
    let log = raw(
        vec![player_agent(1), player_agent(2)],
        vec![apply(100, BUFF, 1, 2, 5000)],
    );
    let out = compute(&log, &enc(vec![player_with(1, "Guardian", "Firebrand")]), &[gain_finder()]);
    assert_eq!(out, vec![InstantCastEvent { time: 100, skill_id: SKILL, caster: 1 }]);
}

/// `BuffGiveCastFinder` reads the same stream and takes the OTHER party.
/// This is the pairing that made a first pass of the engine wrong: both
/// finders share one apply stream, so the caster cannot be baked into the
/// stream.
#[test]
fn a_buff_give_credits_the_applier_on_the_very_same_row() {
    let log = raw(vec![player_agent(1), player_agent(2)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let give = FinderDef {
        trigger: Trigger::BuffGive { buff_id: BUFF },
        ..gain_finder()
    };
    let out = compute(&log, &enc(vec![player_with(1, "Guardian", "Firebrand")]), &[give]);
    assert_eq!(
        out,
        vec![InstantCastEvent { time: 100, skill_id: SKILL, caster: 2 }],
        "the applier casts, the recipient receives"
    );
}

/// The ICD is per CASTER. Two players gaining the same buff 10ms apart
/// are two casts; one player gaining it twice 10ms apart is one.
#[test]
fn the_icd_groups_by_caster_rather_than_globally() {
    let together = raw(
        vec![player_agent(1), player_agent(2)],
        vec![apply(100, BUFF, 1, 9, 5000), apply(110, BUFF, 2, 9, 5000)],
    );
    let out = compute(&together, &enc(vec![]), &[gain_finder()]);
    assert_eq!(out.len(), 2, "different casters do not suppress each other");

    let alone = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 9, 5000), apply(110, BUFF, 1, 9, 5000)]);
    let out = compute(&alone, &enc(vec![]), &[gain_finder()]);
    assert_eq!(out.len(), 1, "same caster inside the 50ms default ICD is one cast");

    let apart = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 9, 5000), apply(200, BUFF, 1, 9, 5000)]);
    let out = compute(&apart, &enc(vec![]), &[gain_finder()]);
    assert_eq!(out.len(), 2, "outside the ICD is two casts");
}

/// A `BUFF_INITIAL` row is a stack that predates the log, so it is not
/// evidence of a cast inside it (`BuffGainCastFinder.cs:11`).
#[test]
fn a_pre_log_initial_stack_is_not_a_cast() {
    let mut initial = apply(100, BUFF, 1, 2, 5000);
    initial.is_statechange = sc::BUFF_INITIAL;
    let log = raw(vec![player_agent(1)], vec![initial]);
    assert!(compute(&log, &enc(vec![]), &[gain_finder()]).is_empty());
}

/// Build gates are half-open `[min, max)`: two finders covering adjacent
/// eras must not both fire at the seam.
#[test]
fn build_gates_are_half_open_so_adjacent_eras_do_not_overlap() {
    let mut build = base();
    build.is_statechange = sc::GW2_BUILD;
    build.src_agent = 2000;
    let log = raw(vec![player_agent(1)], vec![build, apply(100, BUFF, 1, 2, 5000)]);

    let old = FinderDef { skill_id: 1, max_gw2_build: 2000, ..gain_finder() };
    let new = FinderDef { skill_id: 2, min_gw2_build: 2000, ..gain_finder() };
    let out = compute(&log, &enc(vec![]), &[old, new]);
    assert_eq!(
        out.iter().map(|e| e.skill_id).collect::<Vec<_>>(),
        vec![2],
        "build 2000 belongs to the new era only"
    );
}

/// `.UsingDisableWithEffectData()` is the mechanism that makes the flag
/// set log-specific rather than build-specific -- the same finder is
/// available on one log and not on another recorded at the same build.
#[test]
fn effect_data_presence_flips_availability_on_an_identical_build() {
    let f = FinderDef { enable: &[Enable::NoEffectData], ..gain_finder() };
    let without = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    assert_eq!(compute(&without, &enc(vec![]), &[f]).len(), 1);

    let mut effect = base();
    effect.is_statechange = 60; // EffectGroundCreate
    let with = raw(vec![player_agent(1)], vec![effect, apply(100, BUFF, 1, 2, 5000)]);
    assert!(
        compute(&with, &enc(vec![]), &[f]).is_empty(),
        "a log carrying effect events disables the buff-side fallback finder"
    );
}

/// A spec checker reads the party it names, not the caster. Here the
/// caster is the recipient and the check is on the APPLIER.
#[test]
fn a_spec_check_reads_the_party_it_names() {
    let log = raw(vec![player_agent(1), player_agent(2)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let e = enc(vec![
        player_with(1, "Guardian", "Firebrand"),
        player_with(2, "Ranger", "Druid"),
    ]);

    let on_applier = FinderDef {
        checks: &[Check::Spec { party: Party::Other, specs: &["Druid"], base: false, negated: false }],
        ..gain_finder()
    };
    assert_eq!(compute(&log, &e, &[on_applier]).len(), 1);

    let on_recipient = FinderDef {
        checks: &[Check::Spec { party: Party::Key, specs: &["Druid"], base: false, negated: false }],
        ..gain_finder()
    };
    assert!(
        compute(&log, &e, &[on_recipient]).is_empty(),
        "the recipient is a Firebrand; naming the wrong party must not silently pass"
    );
}

/// `base` selects the core profession over the elite spec.
#[test]
fn a_base_spec_check_reads_the_profession_not_the_elite_spec() {
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let e = enc(vec![player_with(1, "Guardian", "Firebrand")]);

    let by_base = FinderDef {
        checks: &[Check::Spec { party: Party::Key, specs: &["Guardian"], base: true, negated: false }],
        ..gain_finder()
    };
    assert_eq!(compute(&log, &e, &[by_base]).len(), 1);

    let by_elite = FinderDef {
        checks: &[Check::Spec { party: Party::Key, specs: &["Guardian"], base: false, negated: false }],
        ..gain_finder()
    };
    assert!(compute(&log, &e, &[by_elite]).is_empty());
}

/// `BuffLossCastFinder` reads removals, where `src_agent` is the OWNER --
/// the opposite role assignment to an apply row. Getting this backwards
/// would credit the remover.
#[test]
fn a_buff_loss_credits_the_owner_from_the_inverted_removal_roles() {
    let mut ev = remove_all(100, BUFF, 1);
    ev.dst_agent = 2;
    let log = raw(vec![player_agent(1)], vec![ev]);
    let f = FinderDef { trigger: Trigger::BuffLoss { buff_id: BUFF }, ..gain_finder() };
    assert_eq!(
        compute(&log, &enc(vec![]), &[f]),
        vec![InstantCastEvent { time: 100, skill_id: SKILL, caster: 1 }]
    );
}

/// `MinionCommandCastFinder` narrows the shared command buff by the
/// commanded minion's SPECIES and credits the owning player.
#[test]
fn a_minion_command_credits_the_master_and_narrows_by_species() {
    let log = raw(
        vec![player_agent(1), agent(50, 1234, "Pet"), agent(60, 9999, "OtherPet")],
        vec![
            reg(1, 11),
            // A positive duration is part of the pre-era apply SHAPE
            // (`is_pre_era_apply_shaped`), not incidental -- a zero-value
            // row is not an apply at all.
            RawEvent { dst_master_instid: 11, ..apply(100, MINION_COMMAND_BUFF, 50, 1, 1000) },
            RawEvent { dst_master_instid: 11, ..apply(400, MINION_COMMAND_BUFF, 60, 1, 1000) },
        ],
    );
    let f = FinderDef {
        trigger: Trigger::MinionCommand { species_id: 1234 },
        ..gain_finder()
    };
    assert_eq!(
        compute(&log, &enc(vec![]), &[f]),
        vec![InstantCastEvent { time: 100, skill_id: SKILL, caster: 1 }],
        "only the named species counts, and the cast belongs to the commander"
    );
}

/// A masterless agent has no owner to credit, so the minion subclasses
/// drop the hit rather than crediting the minion itself.
#[test]
fn a_masterless_minion_yields_no_command_cast() {
    let log = raw(
        vec![agent(50, 1234, "Pet")],
        vec![apply(100, MINION_COMMAND_BUFF, 50, 1, 1000)],
    );
    let f = FinderDef { trigger: Trigger::MinionCommand { species_id: 1234 }, ..gain_finder() };
    assert!(compute(&log, &enc(vec![]), &[f]).is_empty());
}

/// `DamageCastFinder` recovers the cast from a hit, and its ctor forces
/// `UsingNotAccurate()` -- damage lands after the cast, so the time is an
/// upper bound. The flag must survive into the availability sets even
/// though no builder call sets it.
#[test]
fn a_damage_finder_is_not_accurate_without_saying_so() {
    let f = FinderDef { trigger: Trigger::Damage { skill_id: 900 }, ..gain_finder() };
    assert!(!f.not_accurate, "the builder never set it");
    assert!(f.is_not_accurate(), "but the subclass ctor forces it");

    let log = raw(vec![player_agent(1)], vec![hit(100, 900, 1, 2, 500)]);
    assert_eq!(
        compute(&log, &enc(vec![]), &[f]),
        vec![InstantCastEvent { time: 100, skill_id: SKILL, caster: 1 }]
    );

    let (_, _, _, not_accurate) = available_flags(&log, &[f]);
    assert!(not_accurate.contains(&SKILL));
}

/// Breakbar rows are damage-SHAPED but are not health damage; the two
/// streams must not leak into each other.
#[test]
fn breakbar_rows_do_not_feed_the_health_damage_stream() {
    let mut bb = hit(100, 900, 1, 2, 500);
    bb.result = result::BREAKBAR_DAMAGE;
    let log = raw(vec![player_agent(1)], vec![bb]);

    let health = FinderDef { trigger: Trigger::Damage { skill_id: 900 }, ..gain_finder() };
    assert!(compute(&log, &enc(vec![]), &[health]).is_empty());

    let breakbar =
        FinderDef { trigger: Trigger::BreakbarDamage { skill_id: 900 }, ..gain_finder() };
    assert_eq!(compute(&log, &enc(vec![]), &[breakbar]).len(), 1);
}

/// The proc-flag sets come from AVAILABILITY alone -- a finder that never
/// fires still flags its skill. This is the distinction that makes
/// `isInstantCast` strictly stronger than the four proc flags.
#[test]
fn proc_flags_come_from_availability_but_is_instant_cast_needs_a_firing() {
    // A log with no events of the finder's buff at all.
    let log = raw(vec![player_agent(1)], vec![hit(100, 1, 1, 2, 5)]);
    let f = FinderDef { origin: CastOrigin::Gear, ..gain_finder() };

    let (traits, gear, uncond, _) = available_flags(&log, &[f]);
    assert!(gear.contains(&SKILL), "available => flagged");
    assert!(traits.is_empty() && uncond.is_empty());

    assert!(
        compute(&log, &enc(vec![]), &[f]).is_empty(),
        "but it never fired, so isInstantCast stays false"
    );
}

/// The default origin sets no flag at all (`CombatData.cs:238-243` has no
/// `Skill` case), which is why 510 of the 658 finders contribute nothing
/// to the three proc booleans.
#[test]
fn the_default_origin_contributes_to_no_proc_set() {
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let (traits, gear, uncond, _) = available_flags(&log, &[gain_finder()]);
    assert!(traits.is_empty() && gear.is_empty() && uncond.is_empty());
}

/// `UsingTimeOffset` shifts the emitted cast off the triggering event.
#[test]
fn a_time_offset_moves_the_emitted_cast() {
    let log = raw(vec![player_agent(1)], vec![apply(1000, BUFF, 1, 2, 5000)]);
    let f = FinderDef { time_offset: -300, ..gain_finder() };
    assert_eq!(compute(&log, &enc(vec![]), &[f])[0].time, 700);
}

/// A duration checker matches within `epsilon`, not exactly.
#[test]
fn a_duration_check_is_a_band_not_an_equality() {
    let f = FinderDef {
        checks: &[Check::Duration { duration: 5000, epsilon: SERVER_DELAY }],
        ..gain_finder()
    };
    for (applied, want) in [(5000, 1), (5008, 1), (5020, 0), (3000, 0)] {
        let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, applied)]);
        assert_eq!(compute(&log, &enc(vec![]), &[f]).len(), want, "applied {applied}");
    }
}

/// The healing-extension finders test the ICD BEFORE their checkers,
/// unlike every other subclass. The difference is observable: a
/// check-failing event still advances `lastTime` and so can suppress a
/// later passing one.
#[test]
fn the_ext_healing_subclass_applies_its_icd_before_its_checks() {
    // Two events 10ms apart (inside the default ICD). The FIRST fails the
    // spec check, the second would pass it.
    let checks: &[Check] =
        &[Check::Spec { party: Party::Key, specs: &["Druid"], base: false, negated: false }];

    let ordinary = FinderDef {
        trigger: Trigger::BuffGain { buff_id: BUFF },
        checks,
        ..gain_finder()
    };
    let log = raw(
        vec![player_agent(1), player_agent(2)],
        // Same caster: agent 1, first row fails the check because the
        // encounter maps it to a Firebrand.
        vec![apply(100, BUFF, 1, 9, 5000), apply(110, BUFF, 1, 9, 5000)],
    );
    let e = enc(vec![player_with(1, "Ranger", "Firebrand")]);
    assert!(
        compute(&log, &e, &[ordinary]).is_empty(),
        "both rows fail the check, so nothing is emitted either way"
    );

    // The ordering itself is asserted structurally: only the extension
    // subclasses claim ICD-first.
    assert!(Trigger::ExtHealing { skill_id: 1 }.icd_before_checks());
    assert!(!Trigger::BuffGain { buff_id: 1 }.icd_before_checks());
    assert!(!Trigger::Damage { skill_id: 1 }.icd_before_checks());
}

/// A log with no `CBTS_GWBUILD` row reads as `StartOfLife`, matching
/// GW2EI's synthesised build event -- so an unbounded finder still runs.
#[test]
fn a_log_without_a_build_row_still_runs_unbounded_finders() {
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    assert_eq!(capabilities(&log).gw2_build, None);
    assert_eq!(compute(&log, &enc(vec![]), &[gain_finder()]).len(), 1);

    let gated = FinderDef { min_gw2_build: 1, ..gain_finder() };
    assert!(compute(&log, &enc(vec![]), &[gated]).is_empty());
}

/// Leaving a shroud or stowing a tome forces a weapon swap, and the two
/// rows arrive in no guaranteed order. `SwapSnap::Before` is
/// `min(swap - 1, time)` (`InstantCastFinder.cs:125`), so it is a
/// one-directional CLAMP, not a magnet: it only moves a cast that landed
/// on the wrong side of the swap, and never pushes one later.
#[test]
fn a_before_swap_snap_clamps_a_late_cast_and_leaves_an_early_one() {
    let swap = |time: u64, who: u64| RawEvent {
        time,
        src_agent: who,
        is_statechange: sc::WEAPON_SWAP,
        ..base()
    };
    let f = FinderDef {
        trigger: Trigger::BuffLoss { buff_id: BUFF },
        swap_snap: SwapSnap::Before,
        ..gain_finder()
    };

    // The loss landed AFTER the swap: clamped back to just before it.
    // 4ms apart -- the window is `ServerDelayConstant / 2` = 5ms, so these
    // offsets are deliberately tight.
    let late = raw(vec![player_agent(1)], vec![swap(996, 1), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&late, &enc(vec![]), &[f])[0].time, 995);

    // The loss already precedes the swap: left alone. `min` is the whole
    // reason -- a symmetric "snap to the swap" would move this to 1005.
    let early = raw(vec![player_agent(1)], vec![remove_all(1000, BUFF, 1), swap(1004, 1)]);
    assert_eq!(compute(&early, &enc(vec![]), &[f])[0].time, 1000);

    // A swap 200ms earlier is far outside the +-5ms window.
    let far = raw(vec![player_agent(1)], vec![swap(800, 1), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&far, &enc(vec![]), &[f])[0].time, 1000);

    // A swap by a DIFFERENT agent must not move this player's cast.
    let other = raw(vec![player_agent(1)], vec![swap(996, 2), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&other, &enc(vec![]), &[f])[0].time, 1000);

    // Without the flag the swap is ignored entirely.
    let plain = FinderDef { swap_snap: SwapSnap::None, ..f };
    assert_eq!(compute(&late, &enc(vec![]), &[plain])[0].time, 1000);
}

// ----------------------------------------------------------------------
// Effect finders
// ----------------------------------------------------------------------

const EGUID: [u8; 16] = [
    0xE7, 0xC5, 0x0E, 0x0E, 0x14, 0x8C, 0xBE, 0x44, 0xBB, 0x27, 0x70, 0xAF, 0x2D, 0x67, 0x50, 0xA4,
];
const EGUID2: [u8; 16] = [
    0x10, 0x87, 0x3B, 0xDE, 0x22, 0xD8, 0x78, 0x45, 0xAA, 0xF0, 0x04, 0xB0, 0xA6, 0x0F, 0xA5, 0x46,
];
const EFFECT_ID: u32 = 900;
const EFFECT_ID2: u32 = 901;

fn id_to_guid(local_id: u32, guid: [u8; 16]) -> RawEvent {
    RawEvent {
        is_statechange: sc::ID_TO_GUID,
        src_agent: u64::from_le_bytes(guid[0..8].try_into().unwrap()),
        dst_agent: u64::from_le_bytes(guid[8..16].try_into().unwrap()),
        overstack: 0, // ContentLocal::Effect
        skillid: local_id,
        ..base()
    }
}

/// One effect create: agent-anchored when `dst` is `Some`, ground-anchored
/// (no anchor agent, so `IsAroundDst` is false) when it is `None`.
fn effect(time: u64, effect_id: u32, src: u64, dst: Option<u64>) -> RawEvent {
    let mut e = base();
    e.time = time;
    e.skillid = effect_id;
    e.src_agent = src;
    match dst {
        Some(d) => {
            e.is_statechange = sc::EFFECT_AGENT_CREATE;
            e.dst_agent = d;
        }
        None => e.is_statechange = sc::EFFECT_GROUND_CREATE,
    }
    e
}

/// Writes the split generations' duration field (wire bytes 48..52).
fn with_duration(mut e: RawEvent, ms: u32) -> RawEvent {
    let b = ms.to_le_bytes();
    e.iff = b[0];
    e.buff = b[1];
    e.result = b[2];
    e.is_activation = b[3];
    e
}

/// A log at an arcdps build new enough for `CBTS_IDTOGUID` to be trusted,
/// with the GUID map decoded the way `decode_raw` would.
fn raw_fx(agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
    let guid_map = crate::evtc::guid::decode_guid_mappings(&events);
    RawLog {
        header: RawHeader { build: "20250101".into(), revision: 1, boss_id: 1 },
        agents,
        skills: vec![],
        events,
        guid_map,
    }
}

fn effect_finder(by_dst: bool) -> FinderDef {
    FinderDef {
        skill_id: SKILL,
        source: "TestHelper",
        trigger: Trigger::Effect { guid: &EGUID, by_dst },
        ..FinderDef::DEFAULT
    }
}

#[test]
fn an_effect_credits_its_spawner_and_the_by_dst_subclass_its_anchor() {
    let log = raw_fx(
        vec![player_agent(1), player_agent(2)],
        vec![id_to_guid(EFFECT_ID, EGUID), effect(100, EFFECT_ID, 1, Some(2))],
    );
    let e = enc(vec![]);
    assert_eq!(compute(&log, &e, &[effect_finder(false)])[0].caster, 1);
    assert_eq!(compute(&log, &e, &[effect_finder(true)])[0].caster, 2);
}

#[test]
fn a_by_dst_finder_ignores_a_ground_anchored_effect() {
    // A ground effect has no anchor agent at all, so GW2EI's group key is
    // the unknown agent and the whole group is skipped. The plain
    // subclass still fires on the same row.
    let log = raw_fx(
        vec![player_agent(1)],
        vec![id_to_guid(EFFECT_ID, EGUID), effect(100, EFFECT_ID, 1, None)],
    );
    let e = enc(vec![]);
    assert!(compute(&log, &e, &[effect_finder(true)]).is_empty());
    assert_eq!(compute(&log, &e, &[effect_finder(false)]).len(), 1);
}

#[test]
fn an_effect_finder_needs_the_log_to_carry_effect_data() {
    // The `HasEffectData` condition comes from the trigger, not from the
    // row's `enable` list -- so a catalog row cannot forget it.
    let f = effect_finder(false);
    assert!(f.enable.is_empty());
    let no_effects = raw_fx(vec![], vec![id_to_guid(EFFECT_ID, EGUID)]);
    assert!(!f.available(&capabilities(&no_effects)));
    let with_effects =
        raw_fx(vec![], vec![id_to_guid(EFFECT_ID, EGUID), effect(1, EFFECT_ID, 1, None)]);
    assert!(f.available(&capabilities(&with_effects)));
}

#[test]
fn a_guid_this_log_never_mapped_recovers_nothing() {
    // The effect happened, but nothing ties its session-local id to the
    // stable GUID the finder names -- so the finder cannot know it fired.
    let log = raw_fx(vec![player_agent(1)], vec![effect(100, EFFECT_ID, 1, Some(2))]);
    assert!(compute(&log, &enc(vec![]), &[effect_finder(false)]).is_empty());
}

#[test]
fn around_dst_distinguishes_ground_from_agent_anchored_effects() {
    let log = raw_fx(
        vec![player_agent(1)],
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            effect(100, EFFECT_ID, 1, Some(2)),
            effect(2000, EFFECT_ID, 1, None),
        ],
    );
    let e = enc(vec![]);
    let anchored = FinderDef {
        checks: &[Check::AroundDst { negated: false }],
        ..effect_finder(false)
    };
    let ground = FinderDef {
        checks: &[Check::AroundDst { negated: true }],
        ..effect_finder(false)
    };
    assert_eq!(compute(&log, &e, &[anchored])[0].time, 100);
    assert_eq!(compute(&log, &e, &[ground])[0].time, 2000);
}

#[test]
fn effect_duration_is_an_inclusive_range_not_an_epsilon_band() {
    // Deliberately distinct from `Check::Duration`: the buff form is
    // `|applied - d| < epsilon`, which would accept 999 and 1001 here.
    let log = raw_fx(
        vec![player_agent(1)],
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            with_duration(effect(100, EFFECT_ID, 1, Some(2)), 999),
            with_duration(effect(2000, EFFECT_ID, 1, Some(2)), 1000),
            with_duration(effect(4000, EFFECT_ID, 1, Some(2)), 2000),
            with_duration(effect(6000, EFFECT_ID, 1, Some(2)), 2001),
        ],
    );
    let f = FinderDef {
        checks: &[Check::EffectDuration { min: 1000, max: 2000 }],
        ..effect_finder(false)
    };
    let times: Vec<u64> = compute(&log, &enc(vec![]), &[f]).iter().map(|c| c.time).collect();
    assert_eq!(times, vec![2000, 4000]);
}

#[test]
fn a_secondary_effect_check_requires_a_companion_effect_from_the_same_caster() {
    let base_events = vec![id_to_guid(EFFECT_ID, EGUID), id_to_guid(EFFECT_ID2, EGUID2)];
    let f = FinderDef {
        checks: &[Check::SecondaryEffect {
            guid: &EGUID2,
            inverted_src: false,
            type_rel: model::TypeRel::Any,
            time_offset: 0,
            epsilon: SERVER_DELAY,
            negated: false,
        }],
        ..effect_finder(false)
    };

    // Companion present, same caster, inside the window.
    let mut evs = base_events.clone();
    evs.push(effect(100, EFFECT_ID, 1, Some(2)));
    evs.push(effect(105, EFFECT_ID2, 1, Some(2)));
    assert_eq!(compute(&raw_fx(vec![], evs), &enc(vec![]), &[f]).len(), 1);

    // Companion belongs to a DIFFERENT caster.
    let mut evs = base_events.clone();
    evs.push(effect(100, EFFECT_ID, 1, Some(2)));
    evs.push(effect(105, EFFECT_ID2, 9, Some(2)));
    assert!(compute(&raw_fx(vec![], evs), &enc(vec![]), &[f]).is_empty());

    // Companion too far away in time.
    let mut evs = base_events.clone();
    evs.push(effect(100, EFFECT_ID, 1, Some(2)));
    evs.push(effect(400, EFFECT_ID2, 1, Some(2)));
    assert!(compute(&raw_fx(vec![], evs), &enc(vec![]), &[f]).is_empty());
}

#[test]
fn a_negated_secondary_effect_check_passes_when_the_companion_is_absent() {
    // GW2EI's `UsingNoSecondaryEffect*` family returns TRUE outright when
    // the log has no effects under the GUID at all (`else return true`),
    // which the empty-set case has to reproduce.
    let f = FinderDef {
        checks: &[Check::SecondaryEffect {
            guid: &EGUID2,
            inverted_src: false,
            type_rel: model::TypeRel::Any,
            time_offset: 0,
            epsilon: SERVER_DELAY,
            negated: true,
        }],
        ..effect_finder(false)
    };
    let alone = raw_fx(vec![], vec![id_to_guid(EFFECT_ID, EGUID), effect(100, EFFECT_ID, 1, Some(2))]);
    assert_eq!(compute(&alone, &enc(vec![]), &[f]).len(), 1);

    let accompanied = raw_fx(
        vec![],
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            id_to_guid(EFFECT_ID2, EGUID2),
            effect(100, EFFECT_ID, 1, Some(2)),
            effect(105, EFFECT_ID2, 1, Some(2)),
        ],
    );
    assert!(compute(&accompanied, &enc(vec![]), &[f]).is_empty());
}

#[test]
fn secondary_effect_type_rel_compares_how_the_two_are_anchored() {
    let evs = |companion_anchored: bool| {
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            id_to_guid(EFFECT_ID2, EGUID2),
            // The triggering effect is always agent-anchored.
            effect(100, EFFECT_ID, 1, Some(2)),
            effect(105, EFFECT_ID2, 1, companion_anchored.then_some(2)),
        ]
    };
    // `FinderDef::checks` is `&'static`, so the three variants have to be
    // spelled out rather than built in a closure.
    const fn secondary(rel: model::TypeRel) -> Check {
        Check::SecondaryEffect {
            guid: &EGUID2,
            inverted_src: false,
            type_rel: rel,
            time_offset: 0,
            epsilon: SERVER_DELAY,
            negated: false,
        }
    }
    const ANY: &[Check] = &[secondary(model::TypeRel::Any)];
    const SAME: &[Check] = &[secondary(model::TypeRel::Same)];
    const INVERTED: &[Check] = &[secondary(model::TypeRel::Inverted)];

    let e = enc(vec![]);
    for (checks, label, same_ok, inverted_ok) in [
        (ANY, "Any", true, true),
        (SAME, "Same", true, false),
        (INVERTED, "Inverted", false, true),
    ] {
        let f = FinderDef { checks, ..effect_finder(false) };
        assert_eq!(
            !compute(&raw_fx(vec![], evs(true)), &e, &[f]).is_empty(),
            same_ok,
            "{label} against an equally-anchored companion"
        );
        assert_eq!(
            !compute(&raw_fx(vec![], evs(false)), &e, &[f]).is_empty(),
            inverted_ok,
            "{label} against an oppositely-anchored companion"
        );
    }
}

#[test]
fn a_spec_check_accepts_a_set_of_specs() {
    // `UsingSrcSpecsChecker([Spec.Mirage, Spec.Mesmer])` -- the plural
    // form, which the singular one is a one-element case of.
    let log = raw_fx(
        vec![player_agent(1), player_agent(3)],
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            effect(100, EFFECT_ID, 1, Some(2)),
            effect(2000, EFFECT_ID, 3, Some(2)),
        ],
    );
    let e = enc(vec![
        player_with(1, "Mesmer", "Mirage"),
        player_with(3, "Guardian", "Firebrand"),
    ]);
    let f = FinderDef {
        checks: &[Check::Spec {
            party: Party::Key,
            specs: &["Mirage", "Mesmer"],
            base: false,
            negated: false,
        }],
        ..effect_finder(false)
    };
    let out = compute(&log, &e, &[f]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].caster, 1);
}

#[test]
fn with_minions_credits_the_owner_of_an_effect_spawned_by_a_pet() {
    let log = raw_fx(
        vec![player_agent(1), agent(5, 2000, "Pet")],
        vec![
            reg(1, 11),
            RawEvent { src_agent: 5, src_instid: 55, src_master_instid: 11, ..base() },
            id_to_guid(EFFECT_ID, EGUID),
            effect(100, EFFECT_ID, 5, Some(2)),
        ],
    );
    let folded = FinderDef { minions: true, ..effect_finder(false) };
    assert_eq!(compute(&log, &enc(vec![]), &[folded])[0].caster, 1);
    // Without `.WithMinions()` the pet keeps the credit -- effect finders
    // do no folding by default.
    assert_eq!(compute(&log, &enc(vec![]), &[effect_finder(false)])[0].caster, 5);
}

/// One animation boundary row for agent 1. [`raw_fx`]'s build is PRE the
/// `ANIMATION_START`/`STOP` era, so these are the older overloaded combat
/// rows -- `is_activation` 1 to start, 3 (reset) to end.
fn anim(time: u64, activation: u8, skill: u32, dur: i32) -> RawEvent {
    let mut e = base();
    e.time = time;
    e.src_agent = 1;
    e.skillid = skill;
    e.is_activation = activation;
    e.value = dur;
    e.buff_dmg = dur;
    e
}

/// A log in which agent 1 animates `CAST_SKILL` over `[0, 1000]` and an
/// effect fires at `effect_time`.
fn casting_log(effect_time: u64) -> RawLog {
    raw_fx(
        vec![player_agent(1)],
        vec![
            id_to_guid(EFFECT_ID, EGUID),
            anim(0, 1, CAST_SKILL, 1000),
            anim(1000, 3, CAST_SKILL, 1000),
            effect(effect_time, EFFECT_ID, 1, None),
        ],
    )
}

/// The animated skill the `NoAnimatedCast` tests below watch -- GW2EI's
/// `SymbolOfWrath_SymbolOfResolution`, one of the three real ones.
const CAST_SKILL: u32 = 9146;

/// `EffectCastFinder(..).UsingNoAnimatedCastChecker(SymbolOfWrath_SymbolOfResolution)`,
/// at the C#'s default `epsilon = ServerDelayConstant`.
fn no_animated_cast_finder() -> FinderDef {
    FinderDef {
        checks: &[Check::NoAnimatedCast {
            skill_id: CAST_SKILL,
            time_offset: 0,
            epsilon: NAC_EPSILON,
        }],
        ..effect_finder(false)
    }
}

const NAC_EPSILON: i64 = 10;

/// `analysis::rotation::animated` over `casting_log`, with agent 1 folded
/// onto itself -- what `analyze` hands `compute`.
fn windows(log: &RawLog) -> crate::analysis::rotation::AnimatedCasts {
    crate::analysis::rotation::animated(log, &[(1u64, 1u64)].into_iter().collect())
}

/// The point of the checker: the same effect GUID is spawned both by the
/// real skill and by the trait that copies it, so "was the caster
/// mid-animation on the real one?" is the only thing separating them.
#[test]
fn a_no_animated_cast_check_suppresses_an_effect_inside_the_cast_window() {
    let log = casting_log(500);
    assert!(
        super::compute(&log, &enc(vec![]), &[no_animated_cast_finder()], &windows(&log)).is_empty()
    );
    // Without the check the same effect is a cast -- i.e. the check is
    // what is doing the work here, not some other gate.
    assert_eq!(compute(&log, &enc(vec![]), &[effect_finder(false)]).len(), 1);
}

#[test]
fn a_no_animated_cast_check_passes_when_the_caster_was_not_casting() {
    let log = casting_log(5000);
    assert_eq!(
        super::compute(&log, &enc(vec![]), &[no_animated_cast_finder()], &windows(&log)).len(),
        1
    );
}

/// `IntersectsActualCastWindow` is `time >= Time - threshold && EndTime +
/// threshold >= time` -- INCLUSIVE at both epsilon-widened ends.
#[test]
fn the_cast_window_is_inclusive_at_both_epsilon_widened_ends() {
    let w = windows(&casting_log(0));
    for (time, want) in [
        (-NAC_EPSILON - 1, false),
        (-NAC_EPSILON, true),
        (0, true),
        (1000, true),
        (1000 + NAC_EPSILON, true),
        (1000 + NAC_EPSILON + 1, false),
    ] {
        assert_eq!(
            w.is_casting(CAST_SKILL, 1, time, NAC_EPSILON),
            want,
            "at {time}ms"
        );
    }
    // A different skill id, and a different agent, are both misses.
    assert!(!w.is_casting(CAST_SKILL + 1, 1, 500, NAC_EPSILON));
    assert!(!w.is_casting(CAST_SKILL, 2, 500, NAC_EPSILON));
}

#[test]
fn an_empty_cast_index_makes_every_no_animated_cast_check_pass() {
    // The documented default for a caller with no animated casts to give.
    let log = casting_log(500);
    let out = super::compute(
        &log,
        &enc(vec![]),
        &[no_animated_cast_finder()],
        &crate::analysis::rotation::AnimatedCasts::default(),
    );
    assert_eq!(out.len(), 1);
}

#[test]
fn every_effect_finder_is_not_accurate() {
    // Forced by the ctor (`EffectCastFinder.cs:40`): the visual appears
    // some frames after the cast that spawned it.
    assert!(effect_finder(false).is_not_accurate());
    assert!(!effect_finder(false).not_accurate);
}

// ----------------------------------------------------------------------
// The generated catalog
// ----------------------------------------------------------------------

/// The extraction accounting, pinned in code so a regenerate that changes
/// coverage has to change this number deliberately.
///
/// 571 of GW2EI's 649 finder constructions. The 78 skips are all
/// categorical and all named in `catalog/mod.rs`: 70 arbitrary
/// `.UsingChecker(lambda)` predicates, 4 barrier-extension finders and 4
/// `BandTogetherCastFinder`s.
#[test]
fn the_catalog_carries_every_finder_the_generator_could_transcribe() {
    assert_eq!(catalog::all().len(), 571);
}

/// The effect finders are the largest single bucket, and they are the
/// reason `evtc::effect` exists -- so pin that they actually made it into
/// the catalog rather than silently reverting to a skip reason.
#[test]
fn the_catalog_carries_the_effect_finders() {
    let effect = catalog::all()
        .iter()
        .filter(|f| matches!(f.trigger, Trigger::Effect { .. }))
        .count();
    assert_eq!(effect, 142);
    // Both subclasses, not just the common one.
    assert!(catalog::all()
        .iter()
        .any(|f| matches!(f.trigger, Trigger::Effect { by_dst: true, .. })));
}

/// Every spec a checker names must be a real one. A typo here is
/// invisible at runtime -- the check simply never matches, and the finder
/// silently stops firing -- so it is worth a compile-time-adjacent guard.
#[test]
fn every_spec_named_by_a_check_is_a_real_spec() {
    let known: std::collections::BTreeSet<&str> =
        crate::icons::BASE_RES_PROF_ICONS.iter().map(|(n, _)| *n).collect();
    for f in catalog::all() {
        for c in f.checks {
            if let Check::Spec { specs, .. } = c {
                for spec in *specs {
                    assert!(known.contains(spec), "{} names unknown spec `{spec}`", f.source);
                }
            }
        }
    }
}

/// A build range with `min >= max` can never be satisfied, so a finder
/// carrying one is dead code -- and the most likely cause is a swapped
/// argument pair in extraction.
#[test]
fn no_transcribed_finder_has_an_unsatisfiable_build_range() {
    for f in catalog::all() {
        assert!(
            f.min_gw2_build < f.max_gw2_build,
            "{} has an empty GW2 build range",
            f.source
        );
        assert!(
            f.min_evtc_build < f.max_evtc_build,
            "{} has an empty evtc build range",
            f.source
        );
    }
}

/// `HasEffectData` is forced by the TRIGGER, never carried in a row's
/// `enable` list -- see `Trigger::forces_has_effect_data`. A row that
/// spells it out would still work, but it would mean the generator had
/// started emitting a condition the model already guarantees, and the two
/// could then drift apart.
#[test]
fn effect_data_is_a_trigger_property_not_an_enable_entry() {
    for f in catalog::all() {
        assert!(
            !f.enable.contains(&Enable::HasEffectData),
            "{} spells out an enable condition its trigger already forces",
            f.source
        );
        assert_eq!(
            f.trigger.forces_has_effect_data(),
            matches!(f.trigger, Trigger::Effect { .. }),
            "{} disagrees about needing effect data",
            f.source
        );
    }
}

/// A `Check::SecondaryEffect` whose GUID no finder can ever resolve is
/// dead weight, and the likeliest cause is a mis-emitted const. Every
/// secondary GUID must be 16 non-zero-ish bytes -- an all-zero array is
/// what a failed hex decode would leave behind.
#[test]
fn every_effect_guid_in_the_catalog_is_non_empty() {
    for f in catalog::all() {
        if let Trigger::Effect { guid, .. } = f.trigger {
            assert_ne!(guid, &[0u8; 16], "{} has an empty effect GUID", f.source);
        }
        for c in f.checks {
            if let Check::SecondaryEffect { guid, .. } = c {
                assert_ne!(*guid, &[0u8; 16], "{} has an empty secondary GUID", f.source);
            }
        }
    }
}

/// The whole catalog runs over a log without panicking, and an
/// event-less log recovers nothing -- the cheapest guard against an
/// indexing or grouping mistake in a 429-finder sweep.
#[test]
fn the_whole_catalog_runs_and_an_empty_log_recovers_nothing() {
    let finders: Vec<FinderDef> = catalog::all().into_iter().copied().collect();
    let empty = raw(vec![], vec![]);
    assert!(compute(&empty, &enc(vec![]), &finders).is_empty());

    // One buff apply, against every finder at once.
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let out = compute(&log, &enc(vec![]), &finders);
    assert!(out.iter().all(|e| e.caster == 1 || e.caster == 2), "{out:?}");
}

/// The proc-flag sets over the real catalog are non-empty and disjoint
/// from each other -- GW2EI files each finder under exactly one origin.
#[test]
fn the_catalog_yields_the_three_disjoint_proc_sets() {
    let finders: Vec<FinderDef> = catalog::all().into_iter().copied().collect();
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let (traits, gear, uncond, not_acc) = available_flags(&log, &finders);

    assert!(!traits.is_empty(), "trait procs");
    assert!(!gear.is_empty(), "gear procs");
    assert!(!not_acc.is_empty(), "not-accurate skills");
    // `uncond` is the smallest bucket (17 `.UsingOrigin(Unconditional)`
    // call sites in GW2EI), and some of those are effect finders that
    // this catalog skips -- so it is allowed to be empty, but if it is
    // not, it must not overlap the other two.
    for (a, b, name) in
        [(&traits, &gear, "trait/gear"), (&traits, &uncond, "trait/uncond"), (&gear, &uncond, "gear/uncond")]
    {
        assert!(a.is_disjoint(b), "{name} proc sets overlap");
    }
}

/// Two finders for the same skill on the same event must not double-count
/// it -- GW2EI's `SkillID` is a set key, and the ei-json `rotation` this
/// feeds would otherwise show a phantom second cast.
#[test]
fn two_finders_agreeing_on_one_cast_collapse_to_one_event() {
    let log = raw(vec![player_agent(1)], vec![apply(100, BUFF, 1, 2, 5000)]);
    let a = gain_finder();
    let b = FinderDef { source: "OtherHelper", ..gain_finder() };
    assert_eq!(compute(&log, &enc(vec![]), &[a, b]).len(), 1);
}
