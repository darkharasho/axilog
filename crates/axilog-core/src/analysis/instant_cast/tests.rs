//! Engine tests for the instant-cast finder port.
//!
//! These pin the SEMANTICS the 658-row catalog will be evaluated under --
//! caster selection, ICD grouping, availability gating and the two
//! deliberate asymmetries (`Initial` applies, ICD-before-checks). The
//! catalog's own faithfulness is a separate question, answered by the
//! generator's accounting rather than by tests here.

use super::*;
use crate::evtc::{buff_remove, RawAgent, RawEvent, RawHeader};
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
        started_at_unix: None,
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
        checks: &[Check::Spec { party: Party::Other, spec: "Druid", base: false, negated: false }],
        ..gain_finder()
    };
    assert_eq!(compute(&log, &e, &[on_applier]).len(), 1);

    let on_recipient = FinderDef {
        checks: &[Check::Spec { party: Party::Key, spec: "Druid", base: false, negated: false }],
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
        checks: &[Check::Spec { party: Party::Key, spec: "Guardian", base: true, negated: false }],
        ..gain_finder()
    };
    assert_eq!(compute(&log, &e, &[by_base]).len(), 1);

    let by_elite = FinderDef {
        checks: &[Check::Spec { party: Party::Key, spec: "Guardian", base: false, negated: false }],
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
    for (applied, want) in [(5000, 1), (5100, 1), (5200, 0), (3000, 0)] {
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
        &[Check::Spec { party: Party::Key, spec: "Druid", base: false, negated: false }];

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
    let late = raw(vec![player_agent(1)], vec![swap(990, 1), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&late, &enc(vec![]), &[f])[0].time, 989);

    // The loss already precedes the swap: left alone. `min` is the whole
    // reason -- a symmetric "snap to the swap" would move this to 1019.
    let early = raw(vec![player_agent(1)], vec![remove_all(1000, BUFF, 1), swap(1020, 1)]);
    assert_eq!(compute(&early, &enc(vec![]), &[f])[0].time, 1000);

    // A swap 200ms earlier is outside the +-75ms window.
    let far = raw(vec![player_agent(1)], vec![swap(800, 1), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&far, &enc(vec![]), &[f])[0].time, 1000);

    // A swap by a DIFFERENT agent must not move this player's cast.
    let other = raw(vec![player_agent(1)], vec![swap(990, 2), remove_all(1000, BUFF, 1)]);
    assert_eq!(compute(&other, &enc(vec![]), &[f])[0].time, 1000);

    // Without the flag the swap is ignored entirely.
    let plain = FinderDef { swap_snap: SwapSnap::None, ..f };
    assert_eq!(compute(&late, &enc(vec![]), &[plain])[0].time, 1000);
}

// ----------------------------------------------------------------------
// The generated catalog
// ----------------------------------------------------------------------

/// The extraction accounting, pinned in code so a regenerate that changes
/// coverage has to change this number deliberately.
///
/// 429 of GW2EI's 649 finder constructions. The 220 skips are all
/// categorical and all named in `catalog/mod.rs`: 172 effect finders
/// (this project decodes no effect events), 40 arbitrary
/// `.UsingChecker(lambda)` predicates, 4 barrier-extension finders and 4
/// `BandTogetherCastFinder`s.
#[test]
fn the_catalog_carries_every_finder_the_generator_could_transcribe() {
    assert_eq!(catalog::all().len(), 429);
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
            if let Check::Spec { spec, .. } = c {
                assert!(known.contains(spec), "{} names unknown spec `{spec}`", f.source);
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

/// The engine cannot evaluate effect triggers, so no transcribed finder
/// may claim to need effect data. If one ever does, the generator let an
/// effect subclass through.
#[test]
fn no_transcribed_finder_requires_effect_data() {
    for f in catalog::all() {
        assert!(
            !f.enable.contains(&Enable::HasEffectData),
            "{} needs effect data the engine cannot supply",
            f.source
        );
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
