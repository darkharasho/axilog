//! PvE encounter identification, pinned against thirteen committed logs.
//!
//! These are the first PvE fixtures in the repo. Every golden before them is
//! WvW, which is exactly how `model::resolve` came to hardcode
//! `kind: "wvw"` / `map: "World vs World"` for *every* log and how
//! `axilog_ei` came to render `fightName` as `"Detailed WvW - {map}"`
//! unconditionally: a full green suite could not tell the difference. The
//! bug reached users -- axibridge listed a night of raids as "World vs
//! World" fights.
//!
//! The set is chosen to exercise every branch of identification rather than
//! to be a pile of raid logs:
//!
//! | dimension                        | fixture |
//! |----------------------------------|---------|
//! | four `LogCategory` kinds         | `raid_wing`, `raid_encounter`, `fractal`, `golem` |
//! | six sub-categories               | Spirit Vale, Salvation Pass, Stronghold, Bastion, Bjora, Cantha, Silent Surf, Kinfall |
//! | the DEFAULT name rule            | every fixture but Harvest Temple -- named from the boss's own agent |
//! | the catalog's FIXED name         | `strike-harvest-temple-wipe` -- the trigger agent is called "The Dragonvoid" |
//! | the GADGET trigger fallback      | same fixture: id 43488 is a gadget, not an NPC |
//! | a CONDITIONAL `DetectLogic` case | `raid-w3-xera-wipe` -- a Xera id resolves to Twisted Castle when statues precede it, and these logs carry none |
//! | success AND failure, same boss   | `raid-w3-keep-construct-{kill,wipe}` |
//! | success on a non-death rule      | the golems -- see `golem_success_agrees_with_gw2ei_2_percent_rule` |
//!
//! Fixtures are anonymized (`axilog anonymize`), which rewrites player
//! character/account names and leaves every other byte -- the header trigger
//! id, the boss's agent record, every combat event -- untouched. Nothing
//! asserted here is affected by anonymization.

use axilog_core::evtc::{decode_raw, sc, RawLog};
use axilog_core::model::resolve;

/// One fixture and everything identification is expected to derive.
struct Case {
    file: &'static str,
    trigger_id: u32,
    /// The fight name. For all but Harvest Temple this is GW2EI's DEFAULT
    /// rule -- the boss agent's own name, read from the log -- so these are
    /// the game's strings, not GW2EI shorthand ("Gorseval the
    /// Multifarious", not "Gorseval").
    name: &'static str,
    kind: &'static str,
    sub_category: &'static str,
    /// The GW2 instance map id, from the log's MAP_ID event. Asserted
    /// because it is the one piece of map information that stays true for a
    /// PvE log once `map` is (correctly) blanked.
    map_id: u32,
    success: bool,
}

const CASES: &[Case] = &[
    // ---- raid wings -------------------------------------------------
    Case {
        file: "raid-w1-gorseval-kill.anon.zevtc",
        trigger_id: 15429,
        name: "Gorseval the Multifarious",
        kind: "raid_wing",
        sub_category: "SpiritVale",
        map_id: 1062,
        success: true,
    },
    Case {
        file: "raid-w2-slothasor-kill.anon.zevtc",
        trigger_id: 16123,
        name: "Slothasor",
        kind: "raid_wing",
        sub_category: "SalvationPass",
        map_id: 1149,
        success: true,
    },
    Case {
        file: "raid-w3-keep-construct-kill.anon.zevtc",
        trigger_id: 16235,
        name: "Keep Construct",
        kind: "raid_wing",
        sub_category: "StrongholdOfTheFaithful",
        map_id: 1156,
        success: true,
    },
    // The control for the one above: same boss, same map, same everything
    // except the outcome. Without a matched pair, a `success` field that
    // was hardwired to `true` would pass every other case here.
    Case {
        file: "raid-w3-keep-construct-wipe.anon.zevtc",
        trigger_id: 16235,
        name: "Keep Construct",
        kind: "raid_wing",
        sub_category: "StrongholdOfTheFaithful",
        map_id: 1156,
        success: false,
    },
    // A CONDITIONAL `DetectLogic` case: GW2EI redirects a Xera trigger id
    // to Twisted Castle when haunting statues precede the Xera agents, and
    // the catalog records only the fall-through logic. Neither of the
    // captured logs carries a statue (asserted below), so GW2EI would land
    // on `Xera` here too -- which is what makes this fixture a check on the
    // fall-through rather than a coincidence.
    Case {
        file: "raid-w3-xera-wipe.anon.zevtc",
        trigger_id: 16246,
        name: "Xera",
        kind: "raid_wing",
        sub_category: "StrongholdOfTheFaithful",
        map_id: 1156,
        success: false,
    },
    Case {
        file: "raid-w4-samarog-kill.anon.zevtc",
        trigger_id: 17188,
        name: "Samarog",
        kind: "raid_wing",
        sub_category: "BastionOfThePenitent",
        map_id: 1188,
        success: true,
    },
    // ---- strikes (GW2EI files these under `RaidEncounter`) -----------
    Case {
        file: "strike-boneskinner-kill.anon.zevtc",
        trigger_id: 22521,
        name: "Boneskinner",
        kind: "raid_encounter",
        sub_category: "Bjora",
        map_id: 1339,
        success: true,
    },
    // The only fixture named from the CATALOG rather than from the log:
    // the trigger agent is called "The Dragonvoid", the fight is called
    // "Harvest Temple". It is also the only one whose trigger species is a
    // GADGET, so it exercises `trigger_agents`' NPC-then-gadget fallback on
    // a real capture instead of only in a unit test.
    Case {
        file: "strike-harvest-temple-wipe.anon.zevtc",
        trigger_id: 43488,
        name: "Harvest Temple",
        kind: "raid_encounter",
        sub_category: "Cantha",
        map_id: 1437,
        success: false,
    },
    // ---- fractals ---------------------------------------------------
    // Kanaxai's trigger id IS the challenge-mode species. GW2EI still
    // renders no " CM" suffix for it -- its `GetLogMode` returns
    // `Mode.CMNoName`, and `LogData.CompleteLogName` only appends for `CM`
    // and `LegendaryCM` -- so this name is EI-exact despite this project
    // having no challenge-mote detection at all.
    Case {
        file: "fractal-kanaxai-wipe.anon.zevtc",
        trigger_id: 25577,
        name: "Kanaxai, Scythe of House Aurkus",
        kind: "fractal",
        sub_category: "SilentSurf",
        map_id: 1500,
        success: false,
    },
    Case {
        file: "fractal-whispering-shadow-kill.anon.zevtc",
        trigger_id: 27010,
        name: "Whispering Shadow",
        kind: "fractal",
        sub_category: "Kinfall",
        map_id: 1584,
        success: true,
    },
    // ---- golem benchmarks -------------------------------------------
    Case {
        file: "golem-kitty-kill.anon.zevtc",
        trigger_id: 16199,
        name: "Standard Kitty Golem",
        kind: "golem",
        sub_category: "Golem",
        map_id: 1154,
        success: true,
    },
    Case {
        file: "golem-kitty-abort-1.anon.zevtc",
        trigger_id: 16199,
        name: "Standard Kitty Golem",
        kind: "golem",
        sub_category: "Golem",
        map_id: 1154,
        success: false,
    },
    Case {
        file: "golem-kitty-abort-2.anon.zevtc",
        trigger_id: 16199,
        name: "Standard Kitty Golem",
        kind: "golem",
        sub_category: "Golem",
        map_id: 1154,
        success: false,
    },
];

fn fixture_bytes(file: &str) -> Vec<u8> {
    let path = format!("{}/../../fixtures/pve/{file}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read committed fixture {path}: {e}"))
}

fn raw(file: &str) -> RawLog {
    decode_raw(&fixture_bytes(file)).unwrap_or_else(|e| panic!("decode {file}: {e:?}"))
}

#[test]
fn every_pve_fixture_is_identified() {
    for case in CASES {
        let log = raw(case.file);
        assert_eq!(log.header.boss_id as u32, case.trigger_id, "{}", case.file);

        let enc = resolve(&log);
        let pve = enc
            .pve
            .as_ref()
            .unwrap_or_else(|| panic!("{}: identified as WvW, not PvE", case.file));

        assert_eq!(pve.name, case.name, "{}: fight name", case.file);
        assert_eq!(enc.kind, case.kind, "{}: encounter kind", case.file);
        assert_eq!(pve.kind, case.kind, "{}: kind disagrees with enc.kind", case.file);
        assert_eq!(pve.sub_category, Some(case.sub_category), "{}: grouping", case.file);
        assert_eq!(pve.trigger_id, case.trigger_id, "{}", case.file);
        assert!(pve.catalogued, "{}: not in the GW2EI catalog", case.file);
        assert_eq!(pve.success, case.success, "{}: outcome", case.file);
    }
}

#[test]
fn the_matched_keep_construct_pair_differs_only_in_outcome() {
    // The strongest single assertion in this file. Everything a `success`
    // implementation could accidentally key on -- boss, map, category,
    // player count, fixture age -- is held constant across these two, so
    // the only thing that can explain a difference is the outcome itself.
    let kill = resolve(&raw("raid-w3-keep-construct-kill.anon.zevtc"));
    let wipe = resolve(&raw("raid-w3-keep-construct-wipe.anon.zevtc"));
    let (k, w) = (kill.pve.as_ref().unwrap(), wipe.pve.as_ref().unwrap());

    assert_eq!(k.name, w.name);
    assert_eq!(k.trigger_id, w.trigger_id);
    assert_eq!(k.kind, w.kind);
    assert_eq!(kill.map_id, wipe.map_id);
    assert_eq!(kill.players.len(), wipe.players.len());

    assert!(k.success, "the kill must read as a success");
    assert!(!w.success, "the wipe must read as a failure");
}

#[test]
fn the_wvw_map_name_is_not_reused_for_pve() {
    // The regression itself. `map` used to come back as "World vs World"
    // for these logs -- `wvw::map_name`'s fallback for an unrecognized map
    // id, which every PvE map is -- and `axilog_ei` pasted it straight into
    // `fightName`. An empty `map` alongside a real `map_id` is the honest
    // shape: this project has no PvE map table, and says so.
    for case in CASES {
        let enc = resolve(&raw(case.file));
        assert_eq!(enc.map, "", "{}: map name", case.file);
        assert_eq!(enc.map_id, Some(case.map_id), "{}: map id", case.file);
    }
}

#[test]
fn the_boss_is_the_only_target() {
    // `pve::Identity::target_addrs` is what `axilog_schema` narrows
    // `targets[]` to. A raid instance is full of ambient NPCs -- the
    // Gorseval log alone carries 265, including 46 "Spirit Energy" and 21
    // crows -- so "every enemy that is not a player" is not a usable target
    // list, and that is the rule PvE logs would otherwise have fallen into.
    for case in CASES {
        let log = raw(case.file);
        let enc = resolve(&log);
        let pve = enc.pve.as_ref().unwrap();
        assert_eq!(
            pve.target_addrs.len(),
            1,
            "{}: expected exactly one trigger-species agent, got {:?}",
            case.file,
            pve.target_addrs,
        );
        let addr = pve.target_addrs[0];
        let agent = log.agents.iter().find(|a| a.addr == addr).expect("target agent");
        // The AGENT's name, which is the fight name everywhere except
        // Harvest Temple -- where the catalog deliberately overrides "The
        // Dragonvoid".
        let agent_name = agent.name_parts().0;
        if case.file.starts_with("strike-harvest-temple") {
            assert_eq!(agent_name, "The Dragonvoid", "{}", case.file);
            assert_ne!(pve.name, agent_name, "{}: the catalog name must win", case.file);
        } else {
            assert_eq!(agent_name, case.name, "{}: target agent", case.file);
        }
    }
}

#[test]
fn ambient_npcs_are_not_promoted_to_targets() {
    // The other half of the above: the roster is still whole, so
    // `target_addrs` is NARROWING it rather than the log simply being
    // sparse.
    //
    // The floor is deliberately low, because the honest floor IS low --
    // `fractal-whispering-shadow-kill` carries 11 enemies, and a golem
    // benchmark's "instance" is a training area with a handful of props.
    // What every fixture must show is that narrowing happened at all.
    for case in CASES {
        let enc = resolve(&raw(case.file));
        let targets = enc.pve.as_ref().unwrap().target_addrs.len();
        assert!(
            enc.enemies.len() > targets,
            "{}: {} enemies for {} target(s) -- fixture no longer demonstrates narrowing",
            case.file,
            enc.enemies.len(),
            targets,
        );
    }

    // ...and the two worst cases, pinned, because they are the reason the
    // rule exists. These counts are the whole agent roster of a raid
    // instance; the old `kind != "wvw"` branch would have shipped every one
    // of them as a `targets[]` row.
    for (file, floor) in [
        ("raid-w4-samarog-kill.anon.zevtc", 500),
        ("raid-w3-keep-construct-kill.anon.zevtc", 400),
    ] {
        let enc = resolve(&raw(file));
        assert!(
            enc.enemies.len() > floor,
            "{file}: {} enemies, expected more than {floor}",
            enc.enemies.len(),
        );
    }
}

#[test]
fn the_xera_fixture_carries_no_statues_so_the_fall_through_logic_is_right() {
    // Guards the one assumption `raid-w3-xera-wipe`'s expected name rests
    // on. GW2EI redirects a Xera trigger id to `TwistedCastle` when
    // haunting statues (species 16247) are present and end before the Xera
    // agents begin; the catalog cannot express that redirect, so it records
    // the fall-through. If a future fixture swap put a statue-bearing log
    // here, the expected name would silently become wrong -- and this test
    // is what would say so.
    const HAUNTING_STATUE: u32 = 16247;
    let log = raw("raid-w3-xera-wipe.anon.zevtc");
    let statues = log
        .agents
        .iter()
        .filter(|a| a.is_elite == 0xffff_ffff && (a.prof & 0xffff) == HAUNTING_STATUE)
        .count();
    assert_eq!(statues, 0, "fixture carries haunting statues; GW2EI would call this Twisted Castle");
}

#[test]
fn golem_success_agrees_with_gw2ei_2_percent_rule() {
    // GW2EI's `Golem.CheckSuccess` is NOT the generic rule: a benchmark
    // counts as a success if the golem died OR its last health update is
    // below 2%. `pve::succeeded` implements only the death half, so these
    // three fixtures could in principle disagree with GW2EI.
    //
    // They do not, and this measures why rather than asserting it: the two
    // aborted runs end at 97% and 60% health, nowhere near the threshold.
    // A benchmark that ended at, say, 1.5% WOULD diverge -- that is a real
    // known gap, and this test is where it would surface if such a capture
    // were ever added here.
    for (file, expect_dead, max_health_pct) in [
        ("golem-kitty-kill.anon.zevtc", true, 2.0),
        ("golem-kitty-abort-1.anon.zevtc", false, 100.0),
        ("golem-kitty-abort-2.anon.zevtc", false, 100.0),
    ] {
        let log = raw(file);
        let enc = resolve(&log);
        let pve = enc.pve.as_ref().unwrap();
        let addr = pve.target_addrs[0];

        let died = log
            .events
            .iter()
            .any(|e| e.is_statechange == sc::CHANGE_DEAD && e.src_agent == addr);
        assert_eq!(died, expect_dead, "{file}: death event");
        assert_eq!(pve.success, expect_dead, "{file}: success tracks the death rule");

        // arcdps carries the percent in `dst_agent`, scaled by 100.
        let last_health = log
            .events
            .iter()
            .filter(|e| e.is_statechange == sc::HEALTH_UPDATE && e.src_agent == addr)
            .map(|e| e.dst_agent as f64 / 100.0)
            .next_back();
        if !expect_dead {
            let pct = last_health.expect("aborted golem run should carry health updates");
            assert!(
                pct >= 2.0 && pct <= max_health_pct,
                "{file}: ended at {pct}% -- GW2EI's 2% rule would call this a SUCCESS, \
                 so `pve::succeeded`'s death-only rule now diverges from it here",
            );
        }
    }
}

#[test]
fn the_committed_wvw_fixture_is_still_wvw() {
    // The other half of the change: nothing about WvW identification moved.
    // `pve` must stay `None` there, because that is what keeps `map`,
    // `fightName` and `success` on their original code paths -- the three
    // things every existing golden pins.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc");
    let log = decode_raw(&std::fs::read(path).expect("read wvw fixture")).expect("decode");
    let enc = resolve(&log);
    assert_eq!(enc.pve, None, "the WvW fixture must not be identified as PvE");
    assert_eq!(enc.kind, "wvw");
    assert_eq!(enc.map, "Green Alpine Borderlands");
}
