//! Squad-player minion damage-TAKEN rollups (MEIGAP Task 3b) -- GW2EI's
//! `players[].minions[].totalDamageTakenDist`.
//!
//! ## Scope: strictly what axibridge reads
//!
//! GW2EI's `JsonMinionsBuilder.BuildJsonMinions`
//! (`GW2EIBuilders/JsonModels/JsonActors/JsonMinionsBuilder.cs`) emits
//! twenty-odd arrays per minion group: `totalDamage`,
//! `totalTargetDamage[target][phase]`, `totalBreakbarDamage*`,
//! `totalShieldDamage*`, `totalDamageDist`, `targetDamageDist`,
//! `rotation`, `extHealingStats`, `extBarrierStats`, `combatReplayData`.
//! **None of them is read anywhere in `packages/bridge-metrics`.** Two
//! readers touch `minions[]`, and both touch the same one field:
//!
//! - `computePlayerAggregation.ts:871-887` (`getMinionDamageTaken`) folds
//!   `minion.totalDamageTakenDist[0][].totalDamage ?? .damageTaken` into
//!   the `minionDamageTaken` defense metric, bucketed by
//!   `normalizeMinionName(minion.name)`.
//! - `computePlayerAggregation.ts:1462-1500` builds the damage-mitigation
//!   MINION rows from the same `totalDamageTakenDist[0]`, reading
//!   `id`, `blocked`, `evaded`, `glance ?? glanced`, `missed`, `invulned`,
//!   `interrupted` and `hits ?? connectedHits` per entry.
//!
//! So this pass emits exactly `{id, name, totalDamageTakenDist: [[...]]}`
//! and each row carries exactly the fields those two readers consume. That
//! is the whole family; nothing here is a placeholder for a larger one.
//!
//! ## The outcome columns, and why they exist here and nowhere else
//!
//! `skill_damage`'s module doc records that this schema has no
//! missed/blocked/evaded/invulned key anywhere -- true of the DAMAGE-DIST
//! surfaces. It is not true of the classification: `defenses::classify`
//! has resolved every one of those outcomes from the result byte, with
//! full era dispatch, since M2, and `defenses[0].blockedCount` /
//! `.evadedCount` / `.missedCount` / `.interruptedCount` /
//! `.invulnedCount` are already emitted per player. This pass is the same
//! classification, split by skill id, on the minion's incoming rows -- so
//! the numbers a consumer sums here are the same ones the player-side
//! defense counters are built from, one classification, not two.
//!
//! GW2EI's own per-row rules (`JsonDamageDistBuilder.BuildJsonDamageDist`,
//! `GW2EIBuilders/JsonModels/JsonActorUtilities/JsonDamageDistBuilder.cs:48-74`),
//! reproduced:
//!
//! - `hits` = `IsNotADamageEvent ? 0 : 1` -- every ATTEMPT that became a
//!   `HealthDamageEvent`, minus the `NoDamageHealthDamageEvent` marker
//!   rows (`Interrupt`/`KillingBlow`/`Downed`), which are exactly the ones
//!   `IsNotADamageEvent` is true for.
//! - `connectedHits` = `HasHit ? 1 : 0`.
//! - `min`/`max` are guarded by `HasHit`.
//! - `glance`/`crit`/`flank` are guarded by `HasHit` AND
//!   `!IndirectDamage`; `missed` (= `IsBlind`), `evaded`, `blocked`,
//!   `interrupted` by `!IndirectDamage` alone; `invulned` (= `IsAbsorbed`)
//!   by nothing.
//! - `IndirectDamage` = the row list contains a
//!   `NonDirectHealthDamageEvent`.
//!
//! Row EXISTENCE goes through the same two-step
//! `skill_damage::creates_health_damage_event` gate MEIGAP Task 2 added
//! for the enemy dist (the pre-rework buff-apply dispatch order plus the
//! result switch), so this family cannot grow the phantom rows that class
//! of bug produced there.
//!
//! ## Which agents are minions, and whose
//!
//! The wire's `dst_master_instid` -- the field GW2EI's "Linking minions to
//! their masters" pass reads (`EvtcParser.cs`'s
//! `FindAgentMaster(c.Time, c.SrcMasterInstid, c.SrcAgent)`, with the
//! roles inverted on an incoming row) -- resolved at that event's own time
//! through the shared [`InstidRegistry`], then folded onto the owning
//! account's representative addr. Identical machinery to
//! `damage::pet_credit_events` and to `defenses::incoming_boon_strips`'s
//! `CreditedBy` fold; nothing new.
//!
//! ## Grouping: by name, not by agent
//!
//! GW2EI groups a master's minions by species (`Minions.ID`), emitting one
//! `minions[]` entry per species with `Name = minions.Character` -- so
//! seven Juvenile Brown Bears summoned across a fight are ONE row. This
//! pass groups by the minion agent's NAME, which for an NPC agent is that
//! species name, and reports the species id (`RawAgent::prof`) alongside.
//! Grouping by name rather than by id is deliberate: it is what the
//! consumer keys on (`normalizeMinionName` strips `Juvenile ` and collapses
//! anything containing `UNKNOWN` to `Unknown`), and on the reference
//! capture EI's own rows are already one-per-name (31 groups over 11
//! distinct names, no name appearing under two ids).
//!
//! ## Gating
//!
//! **Standalone, NOT wired into `analyze()`** -- opt-in like the other
//! MEIGAP passes, and gated by the adapter on `--skill-damage`: it is a
//! per-skill distribution, the same shape and the same flag every other
//! `*Dist` block in this schema rides, and a flag axibridge hardcodes to
//! `true`. GW2EI itself emits `minions[]` unconditionally, so this is a
//! payload gate, not a semantic one.

use super::damage::InstidRegistry;
use crate::analysis::defenses::{classify_outcome, Outcome};
use crate::analysis::skill_damage::creates_health_damage_event;
use crate::evtc::{RawLog, RawAgent};
use crate::model::Encounter;
use std::collections::BTreeMap;

/// One (minion group, skill id) row of `totalDamageTakenDist[0]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MinionSkillTaken {
    pub skill_id: u32,
    /// EI's `totalDamage`.
    pub total: u64,
    /// EI's `hits` -- attempts, marker rows excluded.
    pub hits: u32,
    /// EI's `connectedHits` -- `HasHit` rows.
    pub connected_hits: u32,
    pub min: u64,
    pub max: u64,
    pub blocked: u32,
    pub evaded: u32,
    pub glance: u32,
    /// EI's `missed`, i.e. `IsBlind`.
    pub missed: u32,
    /// EI's `invulned`, i.e. `IsAbsorbed`.
    pub invulned: u32,
    pub interrupted: u32,
    /// EI's `indirectDamage`.
    pub indirect: bool,
}

/// One minion species belonging to one squad player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinionGroup {
    /// Species id (`RawAgent::prof`) of the first agent folded into this
    /// group. EI's `minions[].id`; read by no axibridge reader, emitted
    /// for shape fidelity.
    pub species_id: u32,
    /// EI's `minions[].name` -- the minion agent's character name.
    pub name: String,
    /// `totalDamageTakenDist[0]`, sorted by skill id ascending.
    pub taken: Vec<MinionSkillTaken>,
}

/// Per-player minion groups, positionally joined to `enc.players`. A player
/// with no minions gets an empty vec (the adapter omits the key entirely).
pub type MinionRollups = Vec<Vec<MinionGroup>>;

/// Build minion damage-taken rollups for every squad player.
pub fn build(raw: &RawLog, enc: &Encounter) -> MinionRollups {
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> MinionRollups {
    let n = enc.players.len();
    let mut out: MinionRollups = vec![Vec::new(); n];
    if n == 0 {
        return out;
    }

    let index_of: BTreeMap<u64, usize> = enc
        .players
        .iter()
        .enumerate()
        .flat_map(|(i, p)| p.agent_addrs.iter().map(move |&a| (a, i)))
        .collect();
    // Every squad addr is also a potential MASTER; a player is never their
    // own minion, so those rows are skipped below.
    let name_of: BTreeMap<u64, &RawAgent> = raw.agents.iter().map(|a| (a.addr, a)).collect();
    let post_era = raw.header.is_post_buff_rework();

    // Pass 1: the minion ROSTER, `minion addr -> owning player index`.
    //
    // Ownership is an AGENT-level fact in GW2EI, not a per-row one:
    // `EvtcParser.cs`'s "Linking minions to their masters" pass runs
    // `FindAgentMaster(c.Time, c.SrcMasterInstid, c.SrcAgent)` over the
    // event stream ONCE and stores the result on the `AgentItem`, after
    // which `master.GetMinions(log)` reaches every one of that minion's
    // events -- including the many rows arcdps emits with
    // `*_master_instid == 0`. Deriving ownership per row instead (the
    // first implementation) silently dropped every such row: measured on
    // the reference capture, 49 of 586 (minion, skill) rows went missing
    // that way. So the link is resolved from ANY row that carries it, in
    // either direction, and then applied to every row addressed to that
    // agent.
    let mut owner_of: BTreeMap<u64, usize> = BTreeMap::new();
    for e in &raw.events {
        for (master_instid, agent_addr) in
            [(e.src_master_instid, e.src_agent), (e.dst_master_instid, e.dst_agent)]
        {
            if master_instid == 0 || agent_addr == 0 || index_of.contains_key(&agent_addr) {
                continue;
            }
            if owner_of.contains_key(&agent_addr) {
                continue;
            }
            let Some(owner) = registry.resolve_at(master_instid, e.time) else { continue };
            let Some(&pi) = index_of.get(&owner) else { continue };
            owner_of.insert(agent_addr, pi);
        }
    }

    // (player index, minion name) -> (species id, skill -> row).
    let mut acc: BTreeMap<(usize, String), (u32, BTreeMap<u32, MinionSkillTaken>)> =
        BTreeMap::new();

    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        // Only rows GW2EI would turn into a `HealthDamageEvent` may create
        // a dist entry -- the same two-step gate the enemy dist uses.
        if !creates_health_damage_event(e, post_era) {
            continue;
        }
        let Some(&pi) = owner_of.get(&e.dst_agent) else { continue };
        let Some(agent) = name_of.get(&e.dst_agent) else { continue };
        let (name, _, _) = agent.name_parts();
        let entry = acc.entry((pi, name)).or_insert((agent.prof, BTreeMap::new()));
        let row =
            entry.1.entry(e.skillid).or_insert(MinionSkillTaken { skill_id: e.skillid, ..Default::default() });

        // `IndirectDamage` is set from the WHOLE row list, hits and
        // non-hits alike -- so before any outcome gate, exactly as the
        // enemy dist does it.
        if e.buff == 1 {
            row.indirect = true;
        }
        // `Hits` counts everything that is not a marker row
        // (`IsNotADamageEvent`); GW2EI routes `Interrupt`/`KillingBlow`/
        // `Downed` to `NoDamageHealthDamageEvent`, whose
        // `IsNotADamageEvent` is true.
        let is_marker = e.buff == 0
            && matches!(
                e.result,
                crate::evtc::result::INTERRUPT
                    | crate::evtc::result::KILLING_BLOW
                    | crate::evtc::result::DOWNED
            );
        if !is_marker {
            row.hits += 1;
        }
        match classify_outcome(e, post_era) {
            Some(Outcome::Hit { dmg, .. }) => {
                row.total += dmg;
                row.min = if row.connected_hits == 0 { dmg } else { row.min.min(dmg) };
                row.max = row.max.max(dmg);
                row.connected_hits += 1;
                if e.result == crate::evtc::result::GLANCE {
                    row.glance += 1;
                }
            }
            Some(Outcome::Blocked) => row.blocked += 1,
            Some(Outcome::Evaded) => row.evaded += 1,
            Some(Outcome::Missed) => row.missed += 1,
            Some(Outcome::Invulned) => row.invulned += 1,
            Some(Outcome::Interrupted) => row.interrupted += 1,
            _ => {}
        }
    }

    for ((pi, name), (species_id, by_skill)) in acc {
        let mut taken: Vec<MinionSkillTaken> = by_skill.into_values().collect();
        // GW2EI's `glance`/`crit`/`flank`/`missed`/`evaded`/`blocked`/
        // `interrupted` all sit inside `if (!IndirectDamage)`.
        for row in taken.iter_mut() {
            if row.indirect {
                row.glance = 0;
                row.missed = 0;
                row.evaded = 0;
                row.blocked = 0;
                row.interrupted = 0;
            }
        }
        out[pi].push(MinionGroup { species_id, name, taken });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{result, RawEvent, RawHeader};
    use crate::model::{Player, Team};

    fn base() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    /// Registers `instid -> addr` so `InstidRegistry` can resolve masters.
    fn reg(addr: u64, instid: u16) -> RawEvent {
        RawEvent { src_agent: addr, src_instid: instid, dst_agent: addr, dst_instid: instid, ..base() }
    }

    /// A hit ON the minion (`dst`), owned by master instid `master`.
    fn hit_on_minion(dst: u64, master: u16, skillid: u32, dmg: i32, res: u8) -> RawEvent {
        RawEvent {
            src_agent: 900, dst_agent: dst, dst_master_instid: master,
            skillid, value: dmg, result: res, ..base()
        }
    }

    fn agent(addr: u64, prof: u32, name: &str) -> RawAgent {
        let mut name_raw = name.as_bytes().to_vec();
        name_raw.push(0);
        RawAgent {
            addr, prof, is_elite: 0xffff_ffff, toughness: 0, concentration: 0,
            healing: 0, hitbox_width: 0, condition: 0, hitbox_height: 0, name_raw,
        }
    }

    fn player(addr: u64) -> Player {
        Player {
            agent_addr: addr, account: format!("A{addr}"), character: format!("C{addr}"),
            profession: "Ranger".into(), elite_spec: String::new(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None,
            commander_tag: None, guild_id: None, agent_addrs: vec![addr],
        }
    }

    fn enc_with(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: String::new(), duration_ms: 1000,
            build: String::new(), revision: 1, recorded_by: None,
            teams: vec![Team { color: "red".into(), team_id: 1, guid: None, shard_id: None }],
            players, enemies: Vec::new(), markers: Vec::new(), tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
        }
    }

    fn raw_from(agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
        raw_from_build("", agents, events)
    }

    fn raw_from_build(build: &str, agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: build.into(), revision: 1, boss_id: 1 },
            agents, skills: vec![], events, guid_map: vec![],
        }
    }

    /// Two same-species minions of one player fold into ONE group keyed by
    /// name, and the outcome columns come out of `defenses::classify`.
    #[test]
    fn same_species_minions_fold_into_one_named_group() {
        let raw = raw_from(
            vec![agent(50, 1234, "Juvenile Brown Bear"), agent(51, 1234, "Juvenile Brown Bear")],
            vec![
                reg(1, 10),
                reg(50, 50),
                reg(51, 51),
                hit_on_minion(50, 10, 700, 100, result::NORMAL),
                hit_on_minion(51, 10, 700, 300, result::NORMAL),
                hit_on_minion(50, 10, 700, 0, result::BLOCK),
                hit_on_minion(50, 10, 701, 0, result::EVADE),
            ],
        );
        let enc = enc_with(vec![player(1)]);
        let m = build(&raw, &enc);
        assert_eq!(m[0].len(), 1, "one group per species name");
        let g = &m[0][0];
        assert_eq!(g.name, "Juvenile Brown Bear");
        assert_eq!(g.species_id, 1234);
        let s700 = g.taken.iter().find(|r| r.skill_id == 700).unwrap();
        assert_eq!(s700.total, 400);
        assert_eq!(s700.connected_hits, 2);
        assert_eq!(s700.hits, 3, "the blocked attempt counts as a hit ATTEMPT");
        assert_eq!(s700.blocked, 1);
        assert_eq!(s700.min, 100);
        assert_eq!(s700.max, 300);
        let s701 = g.taken.iter().find(|r| r.skill_id == 701).unwrap();
        assert_eq!((s701.evaded, s701.connected_hits, s701.total), (1, 0, 0));
    }

    /// Damage taken by the PLAYER never lands in a minion group, and a
    /// minion of a non-squad agent is dropped entirely.
    #[test]
    fn player_rows_and_foreign_minions_are_excluded() {
        let raw = raw_from(
            vec![agent(50, 1, "Pet"), agent(60, 1, "EnemyPet")],
            vec![
                reg(1, 10),
                reg(50, 50),
                reg(900, 90),
                reg(60, 60),
                // A hit on the player itself, carrying a stale master instid.
                RawEvent { dst_agent: 1, dst_master_instid: 10, skillid: 5, value: 99, ..base() },
                // A hit on an ENEMY's pet: master resolves, but not to squad.
                hit_on_minion(60, 90, 6, 50, result::NORMAL),
                hit_on_minion(50, 10, 7, 25, result::NORMAL),
            ],
        );
        let enc = enc_with(vec![player(1)]);
        let m = build(&raw, &enc);
        assert_eq!(m[0].len(), 1);
        assert_eq!(m[0][0].name, "Pet");
        // Skill 6 (the enemy pet's row) and skill 5 (the row addressed to
        // the player itself) must both be absent; only skill 7 is ours.
        // Skill 0 is the `reg` helper's own synthetic rows -- see the note
        // in `indirect_skills_zero_the_outcome_block`.
        let skills: Vec<u32> = m[0][0].taken.iter().map(|r| r.skill_id).collect();
        assert!(skills.contains(&7), "the squad minion's own row must be present");
        assert!(!skills.contains(&5) && !skills.contains(&6), "got {skills:?}");
    }

    /// GW2EI zeroes the whole `glance`/`missed`/`evaded`/`blocked`/
    /// `interrupted` block on an indirect (condition) skill.
    #[test]
    fn indirect_skills_zero_the_outcome_block() {
        let raw = raw_from_build(
            "20260601",
            vec![agent(50, 1, "Pet")],
            vec![
                reg(1, 10),
                reg(50, 50),
                // Post-rework condition tick on the minion.
                RawEvent {
                    src_agent: 900, dst_agent: 50, dst_master_instid: 10, skillid: 736,
                    buff: 1, buff_dmg: 40, result: result::BUFF_CYCLE, ..base()
                },
                // ...and an absorbed condition row, which classifies as
                // Invulned -- a column GW2EI does NOT gate on IndirectDamage.
                RawEvent {
                    src_agent: 900, dst_agent: 50, dst_master_instid: 10, skillid: 736,
                    buff: 1, result: result::ABSORB, ..base()
                },
            ],
        );
        let enc = enc_with(vec![player(1)]);
        let m = build(&raw, &enc);
        // (The `reg` helper's own rows are ordinary result-0 combat rows
        // addressed to the minion, so they show up as a zero-damage skill-0
        // entry -- an artefact of the synthetic stream, not of the pass.)
        let row = m[0][0].taken.iter().find(|r| r.skill_id == 736).unwrap();
        assert!(row.indirect);
        assert_eq!(row.total, 40);
        assert_eq!(row.invulned, 1, "invulned survives the IndirectDamage gate");
        assert_eq!((row.blocked, row.evaded, row.missed, row.interrupted, row.glance), (0, 0, 0, 0, 0));
    }
}
