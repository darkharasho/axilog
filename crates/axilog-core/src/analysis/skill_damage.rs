//! Per-skill damage distribution (M12, Task 1): outgoing damage-to-enemies
//! grouped by skill id (plus a per-target breakdown), and incoming
//! damage-taken grouped by skill id.
//!
//! This is aggregation-only over already-decoded events -- no new wire
//! shapes are read. Every predicate mirrors `analysis::damage` EXACTLY (see
//! that module's `accumulate`/`accumulate_pet_credit`/`pet_credit_events`/
//! `accumulate_damage_taken`), just with an extra `skillid` grouping key
//! folded in, so this module's totals are an internal REFINEMENT of
//! `damage::accumulate`'s totals, not an independent computation --
//! `sum(outgoing[*].total) == PlayerMetrics::damage_total` and
//! `sum(taken[*].total) == PlayerMetrics::damage_taken` hold EXACTLY by
//! construction for every player (asserted directly in this module's unit
//! tests and in `analysis::mod`'s existing relog/pet-credit tests via the
//! golden calibration test, `tests/skill_damage_golden.rs`).
//!
//! `crit_hits`/`flank_hits` are included (not gated behind "cleanly
//! derivable" uncertainty) because both source bytes are unconditionally
//! decoded and already documented elsewhere in this codebase as reliable on
//! an ordinary (non-missile) `CBTS_COMBAT` strike/condi-tick row:
//! `result::CRIT` (`evtc::event::result`, verified against the arcdps
//! `cbtresult` enum order) and `is_flanking` (`RawEvent::is_flanking`'s doc
//! comment: "On an ordinary `CBTS_COMBAT` strike event this is the 'src is
//! flanking dst' flag"). Both are counted as a HIT COUNT (number of events
//! with that flag set), matching how GW2EI's own `totalDamageDist[].crit`/
//! `.flank` are hit counts, not damage sums -- see this module's doc
//! addendum in `tests/skill_damage_golden.rs` for the calibration writeup
//! (including the one documented systematic gap: EI's `totalDamageDist`
//! tracks the PLAYER ACTOR only, excluding pet/minion damage entirely --
//! that's tracked under a separate per-minion `totalDamageDist`, never
//! folded into the owning player's own dist -- while this module's
//! `outgoing`/`per_target` DO fold pet/minion credit onto the owner, same
//! as `damage::accumulate_pet_credit` already does for `PlayerMetrics::
//! damage_total`/`per_enemy`).
//!
//! `hits`/`min`/`max` here count only CONTRIBUTING events (`dmg > 0`),
//! mirroring `damage::accumulate`'s own `if dmg == 0 { continue; }` skip --
//! this is a deliberate, documented divergence from GW2EI's own `hits`
//! (which also counts 0-damage attempts: missed/blocked/invulned/evaded),
//! since this project's established damage predicate has never tracked
//! those non-damage outcomes at all (no missed/blocked/etc. field exists
//! anywhere else in this schema either).

use super::damage::InstidRegistry;
use crate::evtc::{result, RawEvent, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// One skill's aggregated stats within some grouping (whole-player outgoing,
/// one enemy's outgoing, or whole-player incoming).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillStats {
    pub total: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    pub crit_hits: u32,
    pub flank_hits: u32,
}

impl SkillStats {
    fn record(&mut self, dmg: u64, is_crit: bool, is_flank: bool) {
        self.total += dmg;
        self.min = if self.hits == 0 { dmg } else { self.min.min(dmg) };
        self.max = self.max.max(dmg);
        self.hits += 1;
        if is_crit {
            self.crit_hits += 1;
        }
        if is_flank {
            self.flank_hits += 1;
        }
    }

    fn merge(&mut self, other: &SkillStats) {
        self.min = if self.hits == 0 {
            other.min
        } else if other.hits == 0 {
            self.min
        } else {
            self.min.min(other.min)
        };
        self.max = self.max.max(other.max);
        self.total += other.total;
        self.hits += other.hits;
        self.crit_hits += other.crit_hits;
        self.flank_hits += other.flank_hits;
    }
}

/// `skillid -> stats`.
type BySkill = BTreeMap<u32, SkillStats>;
/// `enemy addr -> skillid -> stats`.
type ByTargetSkill = BTreeMap<u64, BySkill>;

fn merge_by_skill(dst: &mut BySkill, src: &BySkill) {
    for (id, stats) in src {
        dst.entry(*id).or_default().merge(stats);
    }
}

fn merge_by_target(dst: &mut ByTargetSkill, src: &ByTargetSkill) {
    for (target, by_skill) in src {
        merge_by_skill(dst.entry(*target).or_default(), by_skill);
    }
}

/// Direct squad -> enemy damage, per source addr, grouped by skill id --
/// mirrors `damage::accumulate`'s predicate exactly, just also keyed by
/// `e.skillid` (and split per-target).
fn accumulate_outgoing_direct(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> BTreeMap<u64, (BySkill, ByTargetSkill)> {
    let mut out: BTreeMap<u64, (BySkill, ByTargetSkill)> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) {
            continue;
        }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 {
            continue;
        }
        let is_crit = e.result == result::CRIT;
        let is_flank = e.is_flanking != 0;
        let entry = out.entry(e.src_agent).or_default();
        entry.0.entry(e.skillid).or_default().record(dmg, is_crit, is_flank);
        entry
            .1
            .entry(e.dst_agent)
            .or_default()
            .entry(e.skillid)
            .or_default()
            .record(dmg, is_crit, is_flank);
    }
    out
}

/// Friendly pet/minion damage, credited to the owning squad player, grouped
/// by skill id -- mirrors `damage::pet_credit_events`'s predicate exactly
/// (same `InstidRegistry` time-aware owner resolution), just also keyed by
/// `e.skillid` (and split per-target). Destination is NOT restricted to
/// `enemies` here, matching `damage::accumulate_pet_credit`'s own note (the
/// pet's own `iff` already excludes friend-directed damage).
fn accumulate_outgoing_pet_credit(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, (BySkill, ByTargetSkill)> {
    let mut out: BTreeMap<u64, (BySkill, ByTargetSkill)> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        if e.iff == 0 {
            continue; // FRIEND: never damage
        }
        if squad.contains(&e.src_agent) {
            continue; // real players: handled by accumulate_outgoing_direct
        }
        if agent_team.get(&e.src_agent).copied() != friendly_team {
            continue; // not our pet
        }
        let owner = match registry.resolve_at(e.src_master_instid, e.time) {
            Some(addr) if squad.contains(&addr) => addr,
            _ => continue,
        };
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 {
            continue;
        }
        let is_crit = e.result == result::CRIT;
        let is_flank = e.is_flanking != 0;
        let entry = out.entry(owner).or_default();
        entry.0.entry(e.skillid).or_default().record(dmg, is_crit, is_flank);
        entry
            .1
            .entry(e.dst_agent)
            .or_default()
            .entry(e.skillid)
            .or_default()
            .record(dmg, is_crit, is_flank);
    }
    out
}

/// Incoming damage per squad member (any source), grouped by skill id --
/// mirrors `damage::accumulate_damage_taken`'s predicate exactly.
fn accumulate_taken(events: &[RawEvent], squad: &BTreeSet<u64>) -> BTreeMap<u64, BySkill> {
    let mut out: BTreeMap<u64, BySkill> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if e.result == result::CROWD_CONTROL {
            continue;
        }
        if !squad.contains(&e.dst_agent) {
            continue;
        }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 {
            continue;
        }
        let is_crit = e.result == result::CRIT;
        let is_flank = e.is_flanking != 0;
        out.entry(e.dst_agent).or_default().entry(e.skillid).or_default().record(
            dmg, is_crit, is_flank,
        );
    }
    out
}

/// One skill id's totals, ready for the native schema / calibration --
/// sorted by skill id ascending (see `to_sorted_vec`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub skill_id: u32,
    pub total: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    pub crit_hits: u32,
    pub flank_hits: u32,
}

/// One enemy's per-skill outgoing breakdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerTargetSkills {
    pub enemy_id: u64,
    pub skills: Vec<SkillEntry>,
}

/// A player's full per-skill damage distribution (M12, Task 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillDamageMetrics {
    /// Outgoing damage to any enemy, grouped by skill id (sorted by skill
    /// id ascending). `sum(outgoing[*].total) == PlayerMetrics::
    /// damage_total` exactly.
    pub outgoing: Vec<SkillEntry>,
    /// Incoming damage from any source, grouped by skill id (sorted by
    /// skill id ascending). `sum(taken[*].total) == PlayerMetrics::
    /// damage_taken` exactly.
    pub taken: Vec<SkillEntry>,
    /// Outgoing damage broken down per enemy (representative id), each with
    /// its own per-skill list (sorted by skill id ascending); the outer
    /// list is sorted by `enemy_id` ascending. `sum` over every entry here
    /// equals `sum(outgoing[*].total)` exactly (same events, just also
    /// split by destination).
    pub per_target: Vec<PerTargetSkills>,
}

fn to_sorted_vec(by_skill: BySkill) -> Vec<SkillEntry> {
    by_skill
        .into_iter()
        .map(|(skill_id, s)| SkillEntry {
            skill_id,
            total: s.total,
            hits: s.hits,
            min: s.min,
            max: s.max,
            crit_hits: s.crit_hits,
            flank_hits: s.flank_hits,
        })
        .collect()
}

/// Compute per-squad-player skill damage distributions (outgoing + taken +
/// per-target), account-folded via `addr_to_rep`/`enemy_addr_to_rep` exactly
/// like `analysis::mod::analyze`'s own `dmg_by_rep`/`taken_by_rep` folding.
/// `friendly_team`/`agent_team` are `wvw::resolve_teams`'s output, same as
/// `damage::accumulate_pet_credit`'s callers already thread through.
pub fn build(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, SkillDamageMetrics> {
    build_with_registry(
        raw,
        &InstidRegistry::build(raw),
        squad,
        enemies,
        addr_to_rep,
        enemy_addr_to_rep,
        friendly_team,
        agent_team,
    )
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`]
/// (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for standalone/test callers.
#[allow(clippy::too_many_arguments)]
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, SkillDamageMetrics> {
    // Outgoing: direct + pet-credit, combined by raw src addr first.
    let mut combined: BTreeMap<u64, (BySkill, ByTargetSkill)> = BTreeMap::new();
    for (src, (by_skill, by_target)) in accumulate_outgoing_direct(&raw.events, squad, enemies) {
        let entry = combined.entry(src).or_default();
        merge_by_skill(&mut entry.0, &by_skill);
        merge_by_target(&mut entry.1, &by_target);
    }
    for (src, (by_skill, by_target)) in
        accumulate_outgoing_pet_credit(raw, registry, squad, friendly_team, agent_team)
    {
        let entry = combined.entry(src).or_default();
        merge_by_skill(&mut entry.0, &by_skill);
        merge_by_target(&mut entry.1, &by_target);
    }

    // Fold every source addr onto its account representative, and every
    // destination addr onto its enemy representative -- same two-sided fold
    // `analyze()` already does for `dmg_by_rep`.
    let mut outgoing_by_rep: BTreeMap<u64, (BySkill, ByTargetSkill)> = BTreeMap::new();
    for (addr, (by_skill, by_target)) in combined {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        let entry = outgoing_by_rep.entry(rep).or_default();
        merge_by_skill(&mut entry.0, &by_skill);
        for (dst, skills) in by_target {
            let dst_rep = enemy_addr_to_rep.get(&dst).copied().unwrap_or(dst);
            merge_by_skill(entry.1.entry(dst_rep).or_default(), &skills);
        }
    }

    // Taken: fold destination addr onto its account representative.
    let mut taken_by_rep: BTreeMap<u64, BySkill> = BTreeMap::new();
    for (addr, by_skill) in accumulate_taken(&raw.events, squad) {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        merge_by_skill(taken_by_rep.entry(rep).or_default(), &by_skill);
    }

    let mut result: BTreeMap<u64, SkillDamageMetrics> = BTreeMap::new();
    for (rep, (by_skill, by_target)) in outgoing_by_rep {
        let entry = result.entry(rep).or_default();
        entry.outgoing = to_sorted_vec(by_skill);
        entry.per_target = by_target
            .into_iter()
            .map(|(enemy_id, skills)| PerTargetSkills { enemy_id, skills: to_sorted_vec(skills) })
            .collect();
    }
    for (rep, by_skill) in taken_by_rep {
        result.entry(rep).or_default().taken = to_sorted_vec(by_skill);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;

    fn strike(src: u64, dst: u64, skillid: u32, dmg: i32) -> RawEvent {
        RawEvent {
            time: 0, src_agent: src, dst_agent: dst, value: dmg, buff_dmg: 0,
            overstack: 0, skillid, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// Multi-skill, multi-target: two skills against two enemies from one
    /// squad player. `outgoing` must sum both skills across both targets;
    /// `per_target` must split correctly per enemy.
    #[test]
    fn multi_skill_multi_target_groups_correctly() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64, 10u64].into_iter().collect();
        let raw = raw_from(vec![
            strike(1, 9, 100, 50),
            strike(1, 9, 100, 30),
            strike(1, 9, 200, 20),
            strike(1, 10, 100, 40),
        ]);
        let result = build(&raw, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        let m = result.get(&1).expect("player 1 present");

        // outgoing: skill 100 total = 50+30+40=120, hits=3; skill 200 total=20, hits=1.
        let s100 = m.outgoing.iter().find(|e| e.skill_id == 100).unwrap();
        assert_eq!(s100.total, 120);
        assert_eq!(s100.hits, 3);
        assert_eq!(s100.min, 30);
        assert_eq!(s100.max, 50);
        let s200 = m.outgoing.iter().find(|e| e.skill_id == 200).unwrap();
        assert_eq!(s200.total, 20);
        assert_eq!(s200.hits, 1);

        // sum(outgoing) == total damage dealt (140).
        let sum: u64 = m.outgoing.iter().map(|e| e.total).sum();
        assert_eq!(sum, 140);

        // per_target: enemy 9 got skill 100 (80) + skill 200 (20); enemy 10 got skill 100 (40).
        let t9 = m.per_target.iter().find(|t| t.enemy_id == 9).unwrap();
        let t9_100 = t9.skills.iter().find(|e| e.skill_id == 100).unwrap();
        assert_eq!(t9_100.total, 80);
        let t9_200 = t9.skills.iter().find(|e| e.skill_id == 200).unwrap();
        assert_eq!(t9_200.total, 20);
        let t10 = m.per_target.iter().find(|t| t.enemy_id == 10).unwrap();
        let t10_100 = t10.skills.iter().find(|e| e.skill_id == 100).unwrap();
        assert_eq!(t10_100.total, 40);

        // sum(per_target) == sum(outgoing).
        let per_target_sum: u64 =
            m.per_target.iter().flat_map(|t| t.skills.iter()).map(|e| e.total).sum();
        assert_eq!(per_target_sum, sum);
    }

    /// Pet/minion damage must fold onto the owning squad player's skill
    /// distribution, using the PET'S skill id (not the owner's), mirroring
    /// `damage::accumulate_pet_credit`'s owner-crediting exactly.
    #[test]
    fn pet_damage_folds_onto_owner_by_pet_skill_id() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let friendly_team = Some(10u32);
        let agent_team: BTreeMap<u64, u32> =
            [(1u64, 10u32), (300u64, 10u32)].into_iter().collect();

        fn ev(src: u64, src_instid: u16, master: u16, dst: u64, skillid: u32, v: i32) -> RawEvent {
            RawEvent {
                time: 100, src_agent: src, dst_agent: dst, value: v, buff_dmg: 0,
                overstack: 0, skillid, src_instid, dst_instid: 0,
                src_master_instid: master, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
                is_activation: 0, is_buffremove: 0, is_ninety: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
                is_shields: 0, is_offcycle: 0, pad: 0,
            }
        }
        let raw = raw_from(vec![
            ev(1, 11, 0, 9, 500, 100),  // player's own attack (skill 500)
            ev(300, 77, 11, 9, 999, 40), // pet attack (skill 999), owner registers instid 11 first
        ]);
        // Register owner's instid before the pet event by time-ordering: add
        // a zero-damage registration row via a preceding strike-shaped event
        // isn't needed here since InstidRegistry also registers off ordinary
        // events' src/dst instid+addr, and `ev(1,11,...)` above already
        // registers instid 11 -> addr 1 at time 100 (same time, so pet event
        // resolves against it: partition_point uses `time <= t`).
        let result = build(&raw, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new(), friendly_team, &agent_team);
        let m = result.get(&1).expect("owner present");
        let own = m.outgoing.iter().find(|e| e.skill_id == 500).unwrap();
        assert_eq!(own.total, 100);
        let pet = m.outgoing.iter().find(|e| e.skill_id == 999).unwrap();
        assert_eq!(pet.total, 40, "pet damage credited to owner, keyed by the PET's own skill id");

        let sum: u64 = m.outgoing.iter().map(|e| e.total).sum();
        assert_eq!(sum, 140);
    }

    /// Incoming (`taken`) is grouped by skill id, any source, unaffected by
    /// squad/enemy membership on the source side (mirrors `damage::
    /// accumulate_damage_taken`).
    #[test]
    fn taken_groups_by_skill_id_any_source() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let raw = raw_from(vec![
            strike(9, 1, 700, 200),  // enemy player
            strike(10, 1, 700, 75),  // enemy NPC, same skill id
            strike(10, 1, 800, 30),  // different skill id
        ]);
        let result = build(&raw, &squad, &BTreeSet::new(), &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        let m = result.get(&1).expect("player present");
        let s700 = m.taken.iter().find(|e| e.skill_id == 700).unwrap();
        assert_eq!(s700.total, 275);
        assert_eq!(s700.hits, 2);
        let s800 = m.taken.iter().find(|e| e.skill_id == 800).unwrap();
        assert_eq!(s800.total, 30);

        let sum: u64 = m.taken.iter().map(|e| e.total).sum();
        assert_eq!(sum, 305);
    }

    /// Crowd-control application rows (`value`/`buff_dmg` carry CC duration
    /// ms, not damage) must be excluded from every grouping, matching
    /// `damage`'s own CC exclusion.
    #[test]
    fn excludes_crowd_control_rows() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut cc = strike(1, 9, 300, 5000);
        cc.result = result::CROWD_CONTROL;
        let raw = raw_from(vec![strike(1, 9, 300, 100), cc]);
        let result = build(&raw, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        let m = result.get(&1).unwrap();
        let s = m.outgoing.iter().find(|e| e.skill_id == 300).unwrap();
        assert_eq!(s.total, 100, "CC-shaped row must not leak into skill damage");
        assert_eq!(s.hits, 1);
    }

    /// `crit_hits`/`flank_hits` are hit COUNTS (not damage sums), derived
    /// from `result::CRIT` and `is_flanking != 0` respectively.
    #[test]
    fn crit_and_flank_are_hit_counts() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut crit = strike(1, 9, 400, 100);
        crit.result = result::CRIT;
        let mut flank = strike(1, 9, 400, 50);
        flank.is_flanking = 1;
        let normal = strike(1, 9, 400, 10);
        let raw = raw_from(vec![crit, flank, normal]);
        let result = build(&raw, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        let m = result.get(&1).unwrap();
        let s = m.outgoing.iter().find(|e| e.skill_id == 400).unwrap();
        assert_eq!(s.total, 160);
        assert_eq!(s.hits, 3);
        assert_eq!(s.crit_hits, 1);
        assert_eq!(s.flank_hits, 1);
    }

    /// Account-fold: a relogged squad member's outgoing damage (two raw
    /// addrs) must sum onto the single representative entry, per-skill.
    #[test]
    fn relog_folds_outgoing_across_account_addrs() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64), (2u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![strike(1, 9, 100, 50), strike(2, 9, 100, 30)]);
        let result = build(&raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new(), None, &BTreeMap::new());
        assert_eq!(result.len(), 1);
        let m = result.get(&1).unwrap();
        let s = m.outgoing.iter().find(|e| e.skill_id == 100).unwrap();
        assert_eq!(s.total, 80);
    }
}
