//! Unit tests for the damage-modifier framework + engine (M16, Task 1).
//!
//! Two layers:
//!
//! 1. **Gain computers, in isolation** -- every variant against the exact
//!    formula in its GW2EI source file, so a future refactor can't silently
//!    swap `g/100` for `g/(100+g)` (the single most consequential thing to
//!    get wrong here).
//! 2. **The engine, over synthetic logs** -- one fake definition per
//!    trigger/gain-computer shape, checking the four output fields against
//!    hand-computed values, plus the eligibility rules (`src_type` vs
//!    `compare_type`, `dmg_src` minion split, connected-hits-only) and the
//!    era/mode gating.
//!
//! The real-log calibration (`Moving Bonus` vs a reference EI export) lives
//! in `crates/axilog-core/tests/damage_mods_golden.rs`, which skips when
//! the gitignored local capture is absent.

use super::*;
use crate::evtc::{RawAgent, RawHeader, RawSkill};
use crate::model::{Enemy, Player};

// ---------------------------------------------------------------- fixtures

fn header(build: &str) -> RawHeader {
    RawHeader { build: build.into(), revision: 1, boss_id: 1 }
}

fn raw(events: Vec<RawEvent>, build: &str) -> RawLog {
    RawLog {
        header: header(build),
        agents: Vec::<RawAgent>::new(),
        skills: Vec::<RawSkill>::new(),
        events,
        guid_map: Vec::new(),
    }
}

fn player(addr: u64) -> Player {
    Player {
        agent_addr: addr,
        account: format!(":P{addr}.0001"),
        character: format!("P{addr}"),
        profession: "Thief".into(),
        elite_spec: String::new(),
        team: "red".into(),
        subgroup: 1,
        in_squad: true,
        commander: false,
        marker: None,
        commander_tag: None, guild_id: None,
        agent_addrs: vec![addr],
    }
}

fn enemy(addr: u64) -> Enemy {
    Enemy {
        id: addr,
        instid: addr as u16,
        name: format!("E{addr}"),
        team: "blue".into(),
        is_player: true,
        marker: None,
        profession: Some("Necromancer".into()),
        elite_spec: Some("Reaper".into()),
        agent_addrs: vec![addr],
    }
}

fn encounter(players: Vec<Player>, enemies: Vec<Enemy>) -> Encounter {
    Encounter {
        kind: "wvw".into(), pve: None,
        map: String::new(),
        duration_ms: 10_000,
        build: "20260114".into(),
        revision: 1,
        recorded_by: None,
        teams: Vec::new(),
        players,
        enemies,
        markers: Vec::new(), ground_markers: Vec::new(),
        tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
    }
}

fn blank(time: u64, src: u64, dst: u64) -> RawEvent {
    RawEvent {
        time,
        src_agent: src,
        dst_agent: dst,
        value: 0,
        buff_dmg: 0,
        overstack: 0,
        skillid: 1,
        src_instid: src as u16,
        dst_instid: dst as u16,
        src_master_instid: 0,
        dst_master_instid: 0,
        iff: 1,
        buff: 0,
        result: result::NORMAL,
        is_activation: 0,
        is_buffremove: 0,
        is_ninety: 0, is_fifty: 0,
        is_moving: 0,
        is_statechange: 0,
        is_flanking: 0,
        is_shields: 0,
        is_offcycle: 0,
        pad: 0,
    }
}

/// A connected strike hit.
fn strike(time: u64, src: u64, dst: u64, dmg: i32) -> RawEvent {
    RawEvent { value: dmg, ..blank(time, src, dst) }
}

/// A connected post-rework condition tick (`buff == 1`, `BUFF_CYCLE`) on a
/// skill id the condition catalog recognises (736 == Bleeding).
fn condi_tick(time: u64, src: u64, dst: u64, dmg: i32) -> RawEvent {
    RawEvent {
        buff: 1,
        buff_dmg: dmg,
        skillid: 736,
        result: result::BUFF_CYCLE,
        ..blank(time, src, dst)
    }
}

/// A buff APPLY row in the pre-rework wire shape
/// (`buff == 1`, `value` = duration ms, `buff_dmg == 0`) -- what
/// `analysis::buffs::events::extract_buff_events` reads.
fn buff_apply(time: u64, applier: u64, owner: u64, buff_id: u32, duration_ms: i32) -> RawEvent {
    RawEvent {
        buff: 1,
        value: duration_ms,
        skillid: buff_id,
        ..blank(time, applier, owner)
    }
}

const POST_ERA_BUILD: &str = "20260501";
const PRE_ERA_BUILD: &str = "20260114";

/// A definition template the individual tests mutate.
fn def_template() -> DamageModifierDef {
    DamageModifierDef {
        id: 9001,
        name: "Test Mod",
        icon: "",
        description: "",
        source: model::ModSource::Common,
        spec_specific_shared: false,
        gain_per_stack: 10.0,
        gain: GainComputer::ByPresence,
        trigger: Trigger::Hit,
        src_type: DamageType::All,
        compare_type: DamageType::All,
        dmg_src: DamageSource::NoPets,
        checks: &[],
        mode: model::ModifierMode::All,
        approximate: false,
        is_counter: false,
        actor_always_master: false,
        foe_always_master: false,
        with_absorbed_damage_events: false,
        min_gw2_build: model::START_OF_LIFE,
        max_gw2_build: model::END_OF_LIFE,
        min_evtc_build: model::EVTC_START_OF_LIFE,
        max_evtc_build: model::EVTC_END_OF_LIFE,
    }
}

fn run(events: Vec<RawEvent>, build: &str, def: &DamageModifierDef) -> BTreeMap<(u64, i32), DamageModifierStat> {
    let log = raw(events, build);
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1)], vec![enemy(9)]);
    evaluate(&log, &registry, &enc, &[def])
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// --------------------------------------------------------- gain computers

/// `GainComputerByPresence.cs:10-13`.
#[test]
fn gain_by_presence_is_g_over_100_plus_g() {
    let c = GainComputer::ByPresence;
    assert_eq!(c.compute_gain(5.0, 0), 0.0);
    assert!(approx(c.compute_gain(5.0, 1), 5.0 / 105.0));
    // Stack count above 1 does not scale a presence modifier.
    assert!(approx(c.compute_gain(5.0, 25), 5.0 / 105.0));
}

/// `GainComputerByStack.cs:10-13`.
#[test]
fn gain_by_stack_scales_additively_then_normalizes() {
    let c = GainComputer::ByStack;
    assert_eq!(c.compute_gain(2.0, 0), 0.0);
    assert!(approx(c.compute_gain(2.0, 1), 2.0 / 102.0));
    assert!(approx(c.compute_gain(2.0, 10), 20.0 / 120.0));
}

/// `GainComputerByMultiPresence.cs:3-8` -- literally inherits `ByStack`.
#[test]
fn gain_by_multi_presence_matches_by_stack() {
    for stack in 0..5 {
        assert_eq!(
            GainComputer::ByMultiPresence.compute_gain(7.0, stack),
            GainComputer::ByStack.compute_gain(7.0, stack)
        );
    }
}

/// `GainComputerByAbsence.cs:10-13` -- the inverse of `ByPresence`.
#[test]
fn gain_by_absence_only_pays_at_zero_stacks() {
    let c = GainComputer::ByAbsence;
    assert!(approx(c.compute_gain(10.0, 0), 10.0 / 110.0));
    assert_eq!(c.compute_gain(10.0, 1), 0.0);
}

/// `GainComputerByStackPlusConstant.cs:11-14` -- non-zero even at stack 0.
#[test]
fn gain_by_stack_plus_constant_pays_the_constant_at_zero_stacks() {
    let c = GainComputer::ByStackPlusConstant(15.0);
    assert!(approx(c.compute_gain(2.0, 0), 15.0 / 115.0));
    assert!(approx(c.compute_gain(2.0, 5), 25.0 / 125.0));
}

/// `GainComputerByMultiplyingStack.cs:10-14` -- compounding, not additive.
#[test]
fn gain_by_multiplying_stack_compounds() {
    let c = GainComputer::ByMultiplyingStack;
    assert_eq!(c.compute_gain(10.0, 0), 0.0);
    assert!(approx(c.compute_gain(10.0, 1), 10.0 / 110.0));
    // 100 * 1.1^2 - 100 = 21
    assert!(approx(c.compute_gain(10.0, 2), 21.0 / 121.0));
    // Strictly greater than the additive `ByStack` equivalent.
    assert!(c.compute_gain(10.0, 3) > GainComputer::ByStack.compute_gain(10.0, 3));
}

/// `AtLeastN.cs:12-15`, `AtMostN.cs:12-15`, `ExactNumber.cs:12-15`.
#[test]
fn gain_threshold_computers_are_presence_gated_by_stack_count() {
    let at_least = GainComputer::AtLeastNStacks(5);
    assert_eq!(at_least.compute_gain(10.0, 4), 0.0);
    assert!(approx(at_least.compute_gain(10.0, 5), 10.0 / 110.0));

    let at_most = GainComputer::AtMostNStacks(5);
    assert!(approx(at_most.compute_gain(10.0, 0), 10.0 / 110.0));
    assert!(approx(at_most.compute_gain(10.0, 5), 10.0 / 110.0));
    assert_eq!(at_most.compute_gain(10.0, 6), 0.0);

    let exact = GainComputer::ExactNStacks(3);
    assert_eq!(exact.compute_gain(10.0, 2), 0.0);
    assert!(approx(exact.compute_gain(10.0, 3), 10.0 / 110.0));
    assert_eq!(exact.compute_gain(10.0, 4), 0.0);
}

/// A negative `gainPerStack` (damage REDUCTION, e.g.
/// `ItemDamageModifiers.cs:32`) runs through the same formula.
#[test]
fn negative_gain_per_stack_yields_a_negative_gain() {
    assert!(approx(GainComputer::ByPresence.compute_gain(-10.0, 1), -10.0 / 90.0));
}

/// `DamageModifierDescriptor.cs:21-22` + `JsonLogBuilder.cs:108-122`: the
/// only non-multiplier is the skill-based computer, and `IsCounter` forces
/// `Multiplier` back to true.
#[test]
fn only_skill_based_is_non_multiplier_and_counters_are_always_multiplier() {
    let mut d = def_template();
    assert!(d.is_multiplier() && !d.non_multiplier() && !d.skill_based());

    d.gain = GainComputer::BySkill;
    d.trigger = Trigger::Skill(42);
    assert!(d.non_multiplier() && d.skill_based());

    d.is_counter = true;
    assert!(d.is_multiplier(), "IsCounter forces Multiplier (descriptor :21)");
}

/// `DamageModifier.cs:26` -- incoming modifiers use the NEGATED id.
#[test]
fn json_id_is_negated_for_incoming_modifiers() {
    let mut d = def_template();
    assert_eq!(d.json_id(), 9001);
    d.dmg_src = DamageSource::Incoming;
    assert!(d.incoming());
    assert_eq!(d.json_id(), -9001);
}

// ------------------------------------------------------------- the engine

/// The `Trigger::Hit` shape (GW2EI's `DamageLogDamageModifier`), which is
/// exactly what `Moving Bonus` is: a constant gain over the hits whose
/// checker passes.
#[test]
fn hit_trigger_counts_only_checker_passing_hits_but_all_eligible_ones() {
    let def = DamageModifierDef {
        gain_per_stack: 5.0,
        checks: &[HitCheck::SrcMoving],
        src_type: DamageType::Strike,
        compare_type: DamageType::Strike,
        ..def_template()
    };
    let moving = RawEvent { is_moving: 1, ..strike(100, 1, 9, 1000) };
    let still = strike(200, 1, 9, 500);
    // `is_moving` bit 1 is the TARGET moving -- must NOT satisfy SrcMoving.
    let target_moving = RawEvent { is_moving: 2, ..strike(300, 1, 9, 700) };

    let out = run(vec![moving, still, target_moving], PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 3);
    assert_eq!(stat.total_damage, 2200);
    assert!(approx(stat.damage_gain, super::round_to_3(1000.0 * 5.0 / 105.0)));
}

/// `SingleActorDamageModifierHelper.cs:13-45`: an entry only exists when the
/// modifier produced at least one event, even if eligible hits existed.
#[test]
fn no_entry_when_no_hit_qualifies() {
    let def = DamageModifierDef { checks: &[HitCheck::SrcMoving], ..def_template() };
    let out = run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def);
    assert!(out.is_empty());
}

/// `Actor.cs:190-202` -- only CONNECTED hits are eligible at all, so a
/// blocked/evaded row does not even inflate `totalHitCount`.
#[test]
fn blocked_and_evaded_rows_are_not_eligible_hits() {
    let def = def_template();
    let hit = strike(100, 1, 9, 1000);
    let blocked = RawEvent { result: result::BLOCK, ..strike(200, 1, 9, 0) };
    let evaded = RawEvent { result: result::EVADE, ..strike(300, 1, 9, 0) };
    let cc = RawEvent { result: result::CROWD_CONTROL, ..strike(400, 1, 9, 3000) };

    let out = run(vec![hit, blocked, evaded, cc], PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 1);
    assert_eq!(stat.total_damage, 1000);
}

/// `Actor.FilterDamageEvents` (`:446-479`): `src_type` picks the eligible
/// pool, `compare_type` picks the denominator, and the two are independent.
#[test]
fn src_type_filters_hits_while_compare_type_filters_the_denominator() {
    let def = DamageModifierDef {
        src_type: DamageType::Strike,
        compare_type: DamageType::All,
        ..def_template()
    };
    let events = vec![strike(100, 1, 9, 1000), condi_tick(200, 1, 9, 300)];
    let out = run(events, POST_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    // Only the strike is eligible ...
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 1);
    // ... but the denominator is All damage, condition tick included.
    assert_eq!(stat.total_damage, 1300);

    // Flip the two around: condition-only pool, strike-only denominator.
    let def = DamageModifierDef {
        src_type: DamageType::Condition,
        compare_type: DamageType::Strike,
        ..def_template()
    };
    let events = vec![strike(100, 1, 9, 1000), condi_tick(200, 1, 9, 300)];
    let out = run(events, POST_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 1);
    assert_eq!(stat.total_damage, 1000);
}

/// `SingleActor.cs:781-845` -- the `dmg_src` minion split, plus the
/// deliberate GW2EI quirk that `PetsOnly`'s denominator is actor+minions.
#[test]
fn dmg_src_splits_actor_and_minion_hits() {
    // The minion registers instid 50 with master instid 1 (the player).
    let minion_hit =
        RawEvent { src_master_instid: 1, ..strike(200, 77, 9, 400) };
    let events = vec![strike(100, 1, 9, 1000), minion_hit];

    let no_pets = run(events.clone(), PRE_ERA_BUILD, &def_template());
    let stat = no_pets[&(1, 9001)];
    assert_eq!((stat.hit_count, stat.total_hit_count, stat.total_damage), (1, 1, 1000));

    let all = run(
        events.clone(),
        PRE_ERA_BUILD,
        &DamageModifierDef { dmg_src: DamageSource::All, ..def_template() },
    );
    let stat = all[&(1, 9001)];
    assert_eq!((stat.hit_count, stat.total_hit_count, stat.total_damage), (2, 2, 1400));

    let pets = run(
        events,
        PRE_ERA_BUILD,
        &DamageModifierDef { dmg_src: DamageSource::PetsOnly, ..def_template() },
    );
    let stat = pets[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 1);
    // The quirk: actor+minion, NOT minion-only.
    assert_eq!(stat.total_damage, 1400);
}

/// `IncomingDamageModifier.cs` -- direction flips the actor/foe roles and
/// the id sign.
#[test]
fn incoming_modifier_measures_damage_taken_under_the_negated_id() {
    let def = DamageModifierDef { dmg_src: DamageSource::Incoming, ..def_template() };
    let events = vec![strike(100, 9, 1, 800), strike(200, 1, 9, 5000)];
    let out = run(events, PRE_ERA_BUILD, &def);
    let stat = out[&(1, -9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 1);
    assert_eq!(stat.total_damage, 800);
    assert!(approx(stat.damage_gain, super::round_to_3(800.0 * 10.0 / 110.0)));
}

/// The incoming pool is source-agnostic (MATTRIB Task 2). GW2EI builds
/// incoming modifiers from `GetDamageTakenEvents` with no source filter, so
/// damage a squad member takes from ANOTHER SQUAD MEMBER -- or from
/// THEMSELVES -- is in the denominator. This was M16's quarantined
/// "incoming denominator deficit": one account's 7 self-inflicted Bleeding
/// ticks (239 damage) were dropped because `classify_hit` required
/// `!src_in_squad`. See `tests/damage_mods_golden.rs`'s tracked-cause-2
/// note.
#[test]
fn incoming_pool_includes_self_and_squad_sourced_damage() {
    let def = DamageModifierDef { dmg_src: DamageSource::Incoming, ..def_template() };
    let events = vec![
        // From a foe: always counted.
        strike(100, 9, 1, 800),
        // Self-inflicted Bleeding on player 1 (src == dst == 1).
        condi_tick(200, 1, 1, 35),
        // From another squad member.
        strike(300, 2, 1, 60),
    ];
    let log = raw(events, POST_ERA_BUILD);
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1), player(2)], vec![enemy(9)]);
    let out = evaluate(&log, &registry, &enc, &[&def]);

    let stat = out[&(1, -9001)];
    assert_eq!(stat.total_hit_count, 3, "all three rows are damage player 1 TOOK");
    assert_eq!(stat.total_damage, 895);
    // Player 2 took nothing, so it has no incoming row at all.
    assert!(!out.contains_key(&(2, -9001)));
}

/// ... and the outgoing side is NOT symmetric: a squad-on-squad (or self)
/// hit is not an outgoing modifier hit, because its destination is not a
/// foe. Only the incoming branch is source-agnostic.
#[test]
fn outgoing_pool_still_requires_a_foe_destination() {
    let def = def_template(); // outgoing, `DamageSource::NoPets`
    let events = vec![strike(100, 1, 9, 800), strike(200, 1, 2, 500), condi_tick(300, 1, 1, 35)];
    let out = run(events, POST_ERA_BUILD, &def);
    assert_eq!(out[&(1, 9001)].total_hit_count, 1, "only the hit on foe 9 counts");
    assert_eq!(out[&(1, 9001)].total_damage, 800);
}

/// `SkillDamageModifier.cs:32-57`: skill-gated, gain hardcoded to 1, so
/// `damageGain` is RAW damage.
#[test]
fn skill_trigger_accumulates_raw_damage_at_gain_one() {
    let def = DamageModifierDef {
        gain: GainComputer::BySkill,
        trigger: Trigger::Skill(1234),
        ..def_template()
    };
    let matching = RawEvent { skillid: 1234, ..strike(100, 1, 9, 900) };
    let other = RawEvent { skillid: 5, ..strike(200, 1, 9, 100) };
    let out = run(vec![matching, other], PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 2);
    assert_eq!(stat.damage_gain, 900.0);
    assert_eq!(stat.total_damage, 1000);
}

/// `CounterOnActorDamageModifier.cs:21-25`: the gain decides whether the hit
/// counts, then is overwritten with 1.
#[test]
fn counter_modifier_accumulates_raw_damage_and_reports_a_hit_rate() {
    let def = DamageModifierDef {
        is_counter: true,
        gain_per_stack: 100.0,
        checks: &[HitCheck::OverNinety],
        ..def_template()
    };
    let over = RawEvent { is_ninety: 1, ..strike(100, 1, 9, 600) };
    let under = strike(200, 1, 9, 400);
    let out = run(vec![over, under], PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 2);
    assert_eq!(stat.damage_gain, 600.0, "gain forced to 1 -> raw damage");
}

/// `BuffOnActorDamageModifier.cs:57-61` with a `ByStack` computer: the
/// per-hit stack count comes from the M3 simulator's timeline for that
/// agent, so a hit before the apply gets nothing and a hit after gets the
/// stacked gain.
#[test]
fn buff_on_actor_by_stack_reads_the_stack_count_at_hit_time() {
    static MIGHT: [u32; 1] = [crate::analysis::buffs::MIGHT];
    let def = DamageModifierDef {
        gain_per_stack: 3.0,
        gain: GainComputer::ByStack,
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &MIGHT, multi: false },
            from_foe: false,
        },
        ..def_template()
    };
    let events = vec![
        strike(100, 1, 9, 1000),
        buff_apply(200, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        buff_apply(210, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        strike(300, 1, 9, 1000),
    ];
    let out = run(events, PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1, "only the post-apply hit has stacks");
    assert_eq!(stat.total_hit_count, 2);
    // 2 stacks x 3% => 6/106 of the observed damage.
    assert!(approx(stat.damage_gain, super::round_to_3(1000.0 * 6.0 / 106.0)));
}

/// `GainComputerByAbsence` inverts it: the PRE-apply hit is the one that
/// pays.
#[test]
fn buff_on_actor_by_absence_pays_before_the_buff_lands() {
    static STAB: [u32; 1] = [crate::analysis::buffs::STABILITY];
    let def = DamageModifierDef {
        gain_per_stack: 10.0,
        gain: GainComputer::ByAbsence,
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &STAB, multi: false },
            from_foe: false,
        },
        ..def_template()
    };
    let events = vec![
        strike(100, 1, 9, 1000),
        buff_apply(200, 1, 1, crate::analysis::buffs::STABILITY, 8_000),
        strike(300, 1, 9, 1000),
    ];
    let out = run(events, PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 2);
    assert!(approx(stat.damage_gain, super::round_to_3(1000.0 * 10.0 / 110.0)));
}

/// `BuffsTrackerMulti.cs:7-15` + `ByMultiPresence`: the "stack" is the
/// number of DISTINCT watched buffs present, never a sum of stacks.
#[test]
fn multi_tracker_counts_distinct_present_buffs_not_stacks() {
    static IDS: [u32; 2] = [crate::analysis::buffs::MIGHT, crate::analysis::buffs::FURY];
    let def = DamageModifierDef {
        gain_per_stack: 5.0,
        gain: GainComputer::ByMultiPresence,
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &IDS, multi: true },
            from_foe: false,
        },
        ..def_template()
    };
    // Three Might stacks but no Fury => "stack" is 1, not 3.
    let events = vec![
        buff_apply(100, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        buff_apply(110, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        buff_apply(120, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        strike(200, 1, 9, 1000),
        buff_apply(300, 1, 1, crate::analysis::buffs::FURY, 8_000),
        strike(400, 1, 9, 1000),
    ];
    let out = run(events, PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.hit_count, 2);
    let one_buff = 1000.0 * 5.0 / 105.0;
    let two_buffs = 1000.0 * 10.0 / 110.0;
    assert!(approx(stat.damage_gain, super::round_to_3(one_buff + two_buffs)));
}

/// `BuffOnFoeDamageModifier.Keep` (`:83-91`): foe-buff modifiers are
/// dropped outright in WvW, which is this project's own mode.
#[test]
fn buff_on_foe_modifiers_are_dropped_in_wvw() {
    static VULN: [u32; 1] = [738];
    let def = DamageModifierDef {
        trigger: Trigger::BuffOnFoe {
            tracker: model::BuffTracker { ids: &VULN, multi: false },
            actor_check: None,
            from_actor: false,
        },
        ..def_template()
    };
    assert!(!def.keep(ParseMode::WvW, SkillMode::WvW));
    assert!(!def.keep(ParseMode::SPvP, SkillMode::SPvP));
    assert!(def.keep(ParseMode::Instanced, SkillMode::PvE));
    // And the engine honours it on a WvW encounter.
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
}

/// A `WithBuffOnActorFromFoe` definition is skipped rather than
/// mis-evaluated (see the module doc's gap list).
#[test]
fn from_foe_buff_definitions_are_skipped_as_unsupported() {
    static IDS: [u32; 1] = [crate::analysis::buffs::MIGHT];
    let def = DamageModifierDef {
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &IDS, multi: false },
            from_foe: true,
        },
        ..def_template()
    };
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
}

/// `DamageModifierDescriptor.Available` (`:138-150`) -- both build windows
/// are HALF-OPEN.
#[test]
fn build_gating_is_half_open_on_both_windows() {
    let def = DamageModifierDef {
        min_gw2_build: 100,
        max_gw2_build: 200,
        ..def_template()
    };
    assert!(!def.available(Some(99), None));
    assert!(def.available(Some(100), None));
    assert!(def.available(Some(199), None));
    assert!(!def.available(Some(200), None), "max is exclusive");

    let def = DamageModifierDef {
        min_evtc_build: 20_260_501,
        max_evtc_build: model::EVTC_END_OF_LIFE,
        ..def_template()
    };
    assert!(!def.available(None, Some(20_260_114)));
    assert!(def.available(None, Some(20_260_501)));
}

/// End to end: an era-gated definition is dropped by the engine when the
/// log's own arcdps build falls outside its window.
#[test]
fn engine_drops_definitions_outside_the_logs_evtc_build_window() {
    let def = DamageModifierDef {
        min_evtc_build: 20_260_501,
        max_evtc_build: model::EVTC_END_OF_LIFE,
        ..def_template()
    };
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
    assert!(!run(vec![strike(100, 1, 9, 1000)], POST_ERA_BUILD, &def).is_empty());
}

/// `Keep` (`:156`): an approximate modifier is dropped in WvW/sPvP even when
/// its mode is `All`.
#[test]
fn approximate_modifiers_are_dropped_in_wvw_even_with_mode_all() {
    let def = DamageModifierDef { approximate: true, ..def_template() };
    assert_eq!(def.mode, model::ModifierMode::All);
    assert!(!def.keep(ParseMode::WvW, SkillMode::WvW));
    assert!(def.keep(ParseMode::Instanced, SkillMode::PvE));
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
}

/// `Keep` (`:169-173`): `PvEInstanceOnly` survives an instanced PvE parse
/// but not an open-world one.
#[test]
fn pve_instance_only_mode_requires_an_instanced_parse() {
    let def = DamageModifierDef { mode: model::ModifierMode::PvEInstanceOnly, ..def_template() };
    assert!(def.keep(ParseMode::Instanced, SkillMode::PvE));
    assert!(!def.keep(ParseMode::OpenWorld, SkillMode::PvE));
    assert!(!def.keep(ParseMode::WvW, SkillMode::WvW));
}

/// `GW2BuildEvent.cs:12-15` -- the build lives in `src_agent` of a
/// `statechange == 15` row, and a zero one counts as absent.
#[test]
fn gw2_build_is_read_from_the_gwbuild_statechange_row() {
    let mut zero = blank(0, 0, 0);
    zero.is_statechange = sc::GW2_BUILD;
    let mut real = blank(1, 0, 0);
    real.is_statechange = sc::GW2_BUILD;
    real.src_agent = 178_000;

    assert_eq!(gw2_build(&raw(vec![], PRE_ERA_BUILD)), None);
    assert_eq!(gw2_build(&raw(vec![zero.clone()], PRE_ERA_BUILD)), None);
    assert_eq!(gw2_build(&raw(vec![zero, real], PRE_ERA_BUILD)), Some(178_000));
    assert_eq!(evtc_build(&raw(vec![], PRE_ERA_BUILD)), Some(20_260_114));
}

/// `DamageModifierStat.cs:14` -- `Math.Round(x, 3)`, .NET's banker's
/// rounding.
#[test]
fn damage_gain_rounds_to_three_decimals_half_to_even() {
    assert_eq!(super::round_to_3(1.23456), 1.235);
    assert_eq!(super::round_to_3(1.23444), 1.234);
    // Exact halves go to the even neighbour, not away from zero.
    assert_eq!(super::round_to_3(0.0625), 0.062);
    assert_eq!(super::round_to_3(0.0635), 0.064);
}

/// Two squad players never cross-contaminate, and the map is keyed by the
/// account REPRESENTATIVE addr (relog folding).
#[test]
fn stats_are_per_player_and_folded_onto_the_relog_representative() {
    let def = def_template();
    let log = raw(
        vec![strike(100, 1, 9, 1000), strike(200, 2, 9, 500), strike(300, 3, 9, 250)],
        PRE_ERA_BUILD,
    );
    let registry = InstidRegistry::build(&log);
    // Player 1 relogged as addr 3.
    let mut p1 = player(1);
    p1.agent_addrs = vec![1, 3];
    let enc = encounter(vec![p1, player(2)], vec![enemy(9)]);
    let out = evaluate(&log, &registry, &enc, &[&def]);

    assert_eq!(out[&(1, 9001)].total_damage, 1250, "both of p1's addrs fold onto rep 1");
    assert_eq!(out[&(1, 9001)].total_hit_count, 2);
    assert_eq!(out[&(2, 9001)].total_damage, 500);
}

/// The shipped catalog is evaluable end-to-end and produces `Moving Bonus`
/// (`d10`) numbers with the documented constant gain.
#[test]
fn catalog_moving_bonus_evaluates_end_to_end() {
    let log = raw(
        vec![
            RawEvent { is_moving: 1, ..strike(100, 1, 9, 1000) },
            strike(200, 1, 9, 400),
            // Condition ticks are NOT eligible: `Moving Bonus` is Strike-only.
            RawEvent { is_moving: 1, ..condi_tick(300, 1, 9, 900) },
        ],
        POST_ERA_BUILD,
    );
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1)], vec![enemy(9)]);
    let out = evaluate_catalog(&log, &registry, &enc);

    let stat = out[&(1, 10)];
    assert_eq!(stat.hit_count, 1);
    assert_eq!(stat.total_hit_count, 2);
    assert_eq!(stat.total_damage, 1400, "Strike-only denominator");
    assert!(approx(stat.damage_gain, super::round_to_3(1000.0 * 5.0 / 105.0)));
    assert_eq!(catalog::MOVING_BONUS.json_id(), 10);
}

/// The zero-addr repair, seen from this module: a real capture contains
/// damage rows with `dst_agent == 0` but a live `dst_instid`. Since MATTRIB
/// Task 1 the repair is `crate::evtc::repair` (GW2EI's
/// `EvtcParser.CompleteAgents`), applied once at decode for every pass --
/// this module has no repair of its own any more. Found by the `Moving
/// Bonus` calibration: it was the difference between one account matching
/// exactly and being off by a hit.
#[test]
fn zero_dst_addr_is_repaired_upstream_by_the_decode_pass() {
    let def = def_template();
    // Enemy 9 is aware over [100, 600]; the anomalous row at t = 200 has no
    // dst addr but the same instid, and its high probe (500) lands inside
    // that window, so `CompleteAgents` adopts it.
    let events = vec![
        strike(100, 1, 9, 1000),
        RawEvent { dst_agent: 0, dst_instid: 9, ..strike(200, 1, 0, 430) },
        strike(600, 1, 9, 70),
    ];

    // Without the pre-pass (i.e. what a hand-built `RawLog` that never went
    // through `decode_raw` looks like) the zeroed row is dropped.
    let out = run(events.clone(), PRE_ERA_BUILD, &def);
    assert_eq!(out[&(1, 9001)].total_hit_count, 2);
    assert_eq!(out[&(1, 9001)].total_damage, 1070);

    // With it -- the shipping path -- the row lands on enemy 9.
    let mut repaired = events;
    let stats = crate::evtc::repair_orphaned_agents(&[], &mut repaired);
    assert_eq!(stats.dst_repaired, 1);
    let out = run(repaired, PRE_ERA_BUILD, &def);
    assert_eq!(out[&(1, 9001)].total_hit_count, 3, "the zeroed row must not be dropped");
    assert_eq!(out[&(1, 9001)].total_damage, 1500);
}

/// ... but an instid that never had a non-zero addr stays unresolved, and
/// the row is dropped exactly as before (no guessing) -- the repair leaves
/// the zero in place and the foe set never matches it.
#[test]
fn zero_dst_addr_with_no_known_instid_is_still_dropped() {
    let def = def_template();
    let mut events =
        vec![strike(100, 1, 9, 1000), RawEvent { dst_agent: 0, dst_instid: 4242, ..strike(200, 1, 0, 430) }];
    let stats = crate::evtc::repair_orphaned_agents(&[], &mut events);
    assert_eq!(stats, crate::evtc::RepairStats { dst_orphans: 1, ..Default::default() });
    let out = run(events, PRE_ERA_BUILD, &def);
    let stat = out[&(1, 9001)];
    assert_eq!(stat.total_hit_count, 1);
    assert_eq!(stat.total_damage, 1000);
}

/// `.UsingActorFetchIsAlwaysMaster()`
/// (`DamageModifierDescriptor.cs:101-105` -> `GetActor` returns
/// `evt.From.GetFinalMaster()`, `OutgoingDamageModifier.cs:176-179`): a
/// MINION's hit must read the MASTER's buff state, not the minion's own.
/// Real and WvW-relevant (`ChronomancerHelper.cs:53`, `Mod_DangerTime`, is
/// `SPvPWvW` mode).
#[test]
fn actor_always_master_reads_the_owners_buff_state_for_a_minion_hit() {
    static MIGHT: [u32; 1] = [crate::analysis::buffs::MIGHT];
    let base = DamageModifierDef {
        gain_per_stack: 10.0,
        gain: GainComputer::ByPresence,
        dmg_src: DamageSource::All,
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &MIGHT, multi: false },
            from_foe: false,
        },
        ..def_template()
    };
    // Might is on the PLAYER (addr 1); the minion (addr 77, master instid 1)
    // has none of its own.
    let events = vec![
        buff_apply(50, 1, 1, crate::analysis::buffs::MIGHT, 8_000),
        RawEvent { src_master_instid: 1, ..strike(200, 77, 9, 1000) },
    ];

    // Default: the minion's own (empty) timeline -> no qualifying hit.
    assert!(
        run(events.clone(), PRE_ERA_BUILD, &base).is_empty(),
        "without the flag the minion's own buff state is read, and it has none"
    );

    // With the flag: the master's timeline -> the hit qualifies.
    let master = DamageModifierDef { actor_always_master: true, ..base };
    let stat = run(events, PRE_ERA_BUILD, &master)[&(1, 9001)];
    assert_eq!(stat.hit_count, 1);
    assert!(approx(stat.damage_gain, super::round_to_3(1000.0 * 10.0 / 110.0)));
}

/// The `GetFoe` mirror (`.UsingFoeFetchIsAlwaysMaster()`,
/// `OutgoingDamageModifier.cs:171-174`) -- only reachable in a non-PvP mode,
/// since foe-buff modifiers are dropped in WvW, so this checks the key
/// selection directly.
#[test]
fn foe_always_master_selects_the_targets_owner_key() {
    let base = def_template();
    let hit = Hit {
        ev: &strike(0, 1, 9, 1),
        actor: 1,
        actor_buff_key: 77,
        actor_master_buff_key: 1,
        foe_buff_key: 88,
        foe_addr: 88,
        foe_master_buff_key: 9,
        dst_buff_key: 88,
        from_minion: true,
        incoming: false,
        dmg: 1,
        is_strike: true,
        is_condition: false,
        is_life_leech: false,
        is_crit: false,
        is_glance: false,
        is_src_moving: false,
        is_against_moving: false,
        is_over_ninety: false,
        is_against_under_fifty: false,
        is_against_downed: false,
        is_flanking: false,
        has_shield_damage: false,
    };
    assert_eq!(hit.actor_key(&base), 77);
    assert_eq!(hit.foe_key(&base), 88);
    let master = DamageModifierDef {
        actor_always_master: true,
        foe_always_master: true,
        ..base
    };
    assert_eq!(hit.actor_key(&master), 1);
    assert_eq!(hit.foe_key(&master), 9);
}

/// `UsingHitAndAbsorbedDamageEvents` is not modelled (no absorbed-hit
/// classification exists in this project), so such a definition is REJECTED
/// rather than evaluated over the wrong pool.
#[test]
fn absorbed_damage_event_definitions_are_skipped_as_unsupported() {
    let def = DamageModifierDef { with_absorbed_damage_events: true, ..def_template() };
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
}

/// `WithBuffOnFoeFromActor` -- the mirror of `from_foe`, equally
/// unsupported, and now expressible so it can be rejected.
#[test]
fn from_actor_foe_buff_definitions_are_skipped_as_unsupported() {
    static VULN: [u32; 1] = [738];
    let def = DamageModifierDef {
        trigger: Trigger::BuffOnFoe {
            tracker: model::BuffTracker { ids: &VULN, multi: false },
            actor_check: None,
            from_actor: true,
        },
        ..def_template()
    };
    // Rejected on its own merits, not merely by the WvW foe-buff drop.
    assert!(!super::is_supported(&def));
    assert!(run(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD, &def).is_empty());
}

/// `DamageStatistics.ComputeDamageFrom` (`:65-96`): the aggregate buckets
/// are an if/else chain, so a DIRECT hit whose skill is condition-catalogued
/// lands in `strike`/`power` -- never in `condition` -- even though
/// `FilterDamageEvents` would still treat it as eligible for a
/// `Condition`-typed modifier. The two predicates differ in GW2EI, and the
/// combined `compare_type` arms must not double-count.
#[test]
fn damage_buckets_are_mutually_exclusive_like_computedamagefrom() {
    // Skill 736 (Bleeding) delivered as a DIRECT hit -- condition-catalogued
    // but `buff == 0`.
    let direct_condi_skill = RawEvent { skillid: 736, ..strike(100, 1, 9, 1000) };
    let real_tick = condi_tick(200, 1, 9, 300);
    let events = vec![direct_condi_skill, real_tick];

    for (compare, expected) in [
        (DamageType::All, 1300),
        (DamageType::Strike, 1000),
        (DamageType::Condition, 300),
        (DamageType::Power, 1000),
        // Sum of two mutually exclusive buckets -- no double count.
        (DamageType::StrikeAndCondition, 1300),
    ] {
        let def = DamageModifierDef { compare_type: compare, ..def_template() };
        let stat = run(events.clone(), POST_ERA_BUILD, &def)[&(1, 9001)];
        assert_eq!(stat.total_damage, expected, "compare_type {compare:?}");
    }
}

/// `validate()` rejects the combinations GW2EI's own ctors throw on.
#[test]
fn validate_rejects_gw2ei_impossible_definitions() {
    assert!(def_template().validate().is_ok());

    // `DamageModifierDescriptor.cs:60-63`
    assert!(DamageModifierDef { gain_per_stack: 0.0, ..def_template() }.validate().is_err());
    // `:51`
    assert!(DamageModifierDef { id: 0, ..def_template() }.validate().is_err());
    // `DamageLogDamageModifier.cs:10` hardcodes ByPresence.
    assert!(DamageModifierDef { gain: GainComputer::ByStack, ..def_template() }.validate().is_err());
    // Skill trigger and skill computer must come as a pair.
    assert!(DamageModifierDef {
        trigger: Trigger::Skill(1),
        gain: GainComputer::ByPresence,
        ..def_template()
    }
    .validate()
    .is_err());
    assert!(DamageModifierDef {
        trigger: Trigger::Skill(1),
        gain: GainComputer::BySkill,
        ..def_template()
    }
    .validate()
    .is_ok());
    // `BuffsTrackerSingle` reads exactly one id.
    static TWO: [u32; 2] = [740, 725];
    assert!(DamageModifierDef {
        trigger: Trigger::BuffOnActor {
            tracker: model::BuffTracker { ids: &TWO, multi: false },
            from_foe: false,
        },
        ..def_template()
    }
    .validate()
    .is_err());
    // Empty build window.
    assert!(DamageModifierDef { min_gw2_build: 200, max_gw2_build: 100, ..def_template() }
        .validate()
        .is_err());
}

/// Determinism: the same input yields a bit-identical map, twice.
#[test]
fn evaluation_is_deterministic() {
    let events = vec![
        RawEvent { is_moving: 1, ..strike(100, 1, 9, 1000) },
        strike(200, 1, 9, 400),
    ];
    let log = raw(events, POST_ERA_BUILD);
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1)], vec![enemy(9)]);
    assert_eq!(
        evaluate_catalog(&log, &registry, &enc),
        evaluate_catalog(&log, &registry, &enc)
    );
}

// ------------------------------------------------- M16 Task 3: emission

/// `ParserHelper.DamageTypeToString` (`ParserHelper.cs:97-142`): `All`
/// short-circuits, everything else concatenates the set bits in
/// `Power, Strike, Condition, Life Leech` order with ONE trailing
/// `" Damage"` -- so a combined type is `"Strike, Condition Damage"`, not
/// `"Strike Damage, Condition Damage"`.
#[test]
fn damage_type_display_matches_parserhelper() {
    let cases = [
        (DamageType::All, "All Damage"),
        (DamageType::Power, "Power Damage"),
        (DamageType::Strike, "Strike Damage"),
        (DamageType::Condition, "Condition Damage"),
        (DamageType::LifeLeech, "Life Leech Damage"),
        (DamageType::StrikeAndCondition, "Strike, Condition Damage"),
        (DamageType::ConditionAndLifeLeech, "Condition, Life Leech Damage"),
        (DamageType::StrikeAndLifeLeech, "Strike, Life Leech Damage"),
        (DamageType::StrikeAndConditionAndLifeLeech, "Strike, Condition, Life Leech Damage"),
    ];
    for (t, expected) in cases {
        assert_eq!(t.to_display(), expected, "{t:?}");
    }
}

/// `DamageModifier`'s ctor (`DamageModifier.cs:34-68`) composes the
/// `damageModMap[].description` tooltip out of the descriptor's own
/// `InitialTooltip` plus six conditional `"<br>"` suffixes, in a fixed
/// order. The full table is verified against all 69 emitted ids of the
/// real reference export by
/// `axilog-ei/tests/damage_mods_ei_golden.rs`; these cases pin the
/// branches that test cannot reach in CI (it skips without the local
/// capture) and the one arm with no output at all.
#[test]
fn tooltip_composes_gw2eis_suffixes_in_order() {
    // `NoPets` + plain multiplier: two suffixes, nothing else.
    let plain = DamageModifierDef {
        description: "5% while moving",
        dmg_src: DamageSource::NoPets,
        src_type: DamageType::Strike,
        compare_type: DamageType::Strike,
        ..def_template()
    };
    assert_eq!(
        plain.tooltip(),
        "5% while moving<br>No Minions<br>Applied on Strike Damage\
         <br>Compared against Strike Damage"
    );

    // `All` and `PetsOnly` have their own labels ...
    assert!(DamageModifierDef { dmg_src: DamageSource::All, ..plain }
        .tooltip()
        .contains("<br>Actor + Minions"));
    assert!(DamageModifierDef { dmg_src: DamageSource::PetsOnly, ..plain }
        .tooltip()
        .contains("<br>Minions only"));

    // ... but `Incoming` falls through GW2EI's `default:` arm and adds
    // NOTHING -- the one branch that is silent, and the easiest to get
    // wrong by "helpfully" labelling it.
    let incoming = DamageModifierDef {
        dmg_src: DamageSource::Incoming,
        compare_type: DamageType::All,
        ..plain
    };
    assert_eq!(
        incoming.tooltip(),
        "5% while moving<br>Applied on Strike Damage<br>Compared against All Damage"
    );

    // Counter: `IsCounter` also forces `Multiplier`, so "Counter" appears
    // and "Non multiplier" must NOT (`DamageModifierDescriptor.cs:21`).
    let counter = DamageModifierDef { is_counter: true, ..plain };
    assert!(counter.tooltip().ends_with("<br>Counter"), "{}", counter.tooltip());
    assert!(!counter.tooltip().contains("Non multiplier"));

    // Approximate comes last.
    let approx_def = DamageModifierDef { approximate: true, ..plain };
    assert!(approx_def.tooltip().ends_with("<br>Approximate"));
}

/// The `damageModMap` flags, as `JsonLogBuilder.BuildDamageModDesc`
/// (`:308-322`) derives them: `nonMultiplier` is `!Multiplier`, and the
/// ONLY gain computer in all of GW2EI that is not a multiplier is the
/// skill-based one, so the two flags move together and nothing else can
/// set `nonMultiplier`.
#[test]
fn non_multiplier_is_only_ever_the_skill_based_computer() {
    let skill = DamageModifierDef {
        trigger: Trigger::Skill(1234),
        gain: GainComputer::BySkill,
        ..def_template()
    };
    assert!(skill.skill_based() && skill.non_multiplier());
    assert!(skill.tooltip().contains("<br>Non multiplier"));

    for gain in [
        GainComputer::ByPresence,
        GainComputer::ByStack,
        GainComputer::ByAbsence,
        GainComputer::ByMultiplyingStack,
        GainComputer::ByStackPlusConstant(10.0),
        GainComputer::AtLeastNStacks(3),
    ] {
        let d = DamageModifierDef { gain, ..def_template() };
        assert!(!d.non_multiplier(), "{gain:?} must be a multiplier");
        assert!(!d.skill_based(), "{gain:?} must not be skill-based");
    }
}

/// The per-target split (`evaluate_full(.., per_target = true)`): GW2EI
/// filters by the EXACT destination agent
/// (`DamageEventByDst[target.EnglobingAgentItem]`, `Actor.cs:128-136`), so
/// each target sees only its own hits while the whole-fight row still sees
/// every hit -- and each target's `totalDamage` denominator is likewise
/// restricted to that target.
#[test]
fn per_target_splits_hits_and_denominators_by_foe() {
    let events = vec![
        strike(100, 1, 9, 1000),
        strike(200, 1, 9, 500),
        strike(300, 1, 8, 400),
    ];
    let log = raw(events, PRE_ERA_BUILD);
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1)], vec![enemy(9), enemy(8)]);
    let def = def_template();

    let with_split = evaluate_full(&log, &registry, &enc, &[&def], true);
    let overall = with_split.overall[&(1, 9001)];
    assert_eq!((overall.hit_count, overall.total_hit_count, overall.total_damage), (3, 3, 1900));

    let t9 = with_split.per_target[&(1, 9, 9001)];
    assert_eq!((t9.hit_count, t9.total_hit_count, t9.total_damage), (2, 2, 1500));
    let t8 = with_split.per_target[&(1, 8, 9001)];
    assert_eq!((t8.hit_count, t8.total_hit_count, t8.total_damage), (1, 1, 400));
    assert_eq!(with_split.per_target.len(), 2, "no third target may appear");

    // Opting out costs nothing and changes nothing else.
    let without = evaluate_full(&log, &registry, &enc, &[&def], false);
    assert!(without.per_target.is_empty(), "per_target must stay empty when not requested");
    assert_eq!(without.overall, with_split.overall, "the whole-fight rows must not move");
}

/// `meta` is scoped to the ids actually emitted -- GW2EI fills its own
/// `damageModMap` lazily from inside the emission loop
/// (`JsonDamageModifierDataBuilder.cs:47-51`), so a definition that was
/// active but never triggered must NOT appear.
#[test]
fn meta_covers_exactly_the_emitted_ids() {
    let triggered = def_template();
    // Same shape, different id, gated to a skill nothing in the log casts.
    let never = DamageModifierDef {
        id: 4242,
        name: "Never Fires",
        trigger: Trigger::Skill(999_999),
        gain: GainComputer::BySkill,
        ..def_template()
    };
    let log = raw(vec![strike(100, 1, 9, 1000)], PRE_ERA_BUILD);
    let registry = InstidRegistry::build(&log);
    let enc = encounter(vec![player(1)], vec![enemy(9)]);

    let out = evaluate_full(&log, &registry, &enc, &[&triggered, &never], true);
    assert_eq!(out.meta.keys().copied().collect::<Vec<_>>(), vec![9001]);
    let m = &out.meta[&9001];
    assert_eq!(m.name, triggered.name);
    assert_eq!(m.description, triggered.tooltip());
    assert!(!m.incoming, "a positive id is outgoing");
}
