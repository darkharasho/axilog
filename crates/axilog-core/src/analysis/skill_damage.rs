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
use crate::evtc::{result, RawAgent, RawEvent, RawLog};
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
    /// Of `total`, the portion dealt by a PLAYER or a player's minion --
    /// see [`IncomingSource`]. Populated by `accumulate_taken` only; left
    /// `0` on every outgoing/per-target grouping, where it would be a
    /// tautology (those rows are squad-player-sourced by construction).
    /// `to_sorted_vec`'s `include_player_split` flag is what decides
    /// whether it reaches [`SkillEntry`] as `Some`, so a reader never sees
    /// a `0` that means "not measured".
    pub player_total: u64,
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
        self.player_total += other.player_total;
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
        if !crate::analysis::damage::is_health_damage_result(e.result) {
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
        if !crate::analysis::damage::is_health_damage_result(e.result) {
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

/// What dealt an incoming damage row, for the player-vs-environment split
/// on `taken`.
///
/// The distinction users actually want is "did another PLAYER do this to
/// me", which is what the arcdps in-game filters express -- siege, guards,
/// and NPCs are a different kind of number and averaging them together
/// makes a fight's incoming damage unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncomingSource {
    /// A player agent (`RawAgent::is_player`), friend or foe.
    Player,
    /// An agent whose resolved master is a player -- pets, clones, phantasms,
    /// minions, turrets, spirits.
    PlayerMinion,
    /// Siege, guards, NPCs, environmental damage -- and rows whose source
    /// could not be named at all (see [`IncomingSourceClassifier::classify`]).
    Other,
}

impl IncomingSource {
    /// Whether this source counts toward `SkillStats::player_total`.
    fn is_player_sourced(self) -> bool {
        matches!(self, IncomingSource::Player | IncomingSource::PlayerMinion)
    }
}

/// Resolves the SOURCE of an incoming damage row to a [`IncomingSource`].
///
/// Two things make this more than an `agents` lookup on `src_agent`:
///
/// 1. **Out-of-bubble sources.** arcdps emits damage rows with a real
///    `src_instid` but `src_agent == 0` when the attacker is outside the POV
///    client's update bubble and the target is a squad member near its edge:
///    the client knows which instid hit, but holds no agent record to name
///    it. Measured over 400 real WvW captures, that is 5.80% of incoming
///    direct rows and 3.60% of incoming direct damage -- and the top skills
///    on those rows (Soul Spiral, Gravedigger, Overload Air) are
///    unmistakably player skills, so discarding them would systematically
///    under-count exactly the number this split exists to show.
///
///    They are recoverable here, because a log we hold complete lets us look
///    up a binding arcdps could only have produced by buffering the orphaned
///    rows and retro-patching them live (confirmed with arcdps upstream; no
///    arcdps-side change is coming or needed). Resolving `src_instid`
///    through the [`InstidRegistry`] recovers 94.2% of that damage to a
///    player and 3.0% to a non-player, leaving 2.8% unresolved -- i.e. the
///    residual unknown falls from 3.60% to 0.10% of incoming direct damage.
///
///    This depends on [`InstidRegistry::build`]'s addr-`0` exclusion: before
///    it, the very rows being recovered here had poisoned the registry that
///    would have named them.
///
/// 2. **Instid recycling.** Resolution MUST be time-windowed. 8,283 of those
///    rows (~16%) carry an instid bound to more than one real agent within
///    the same log, so a global `instid -> agent` map would misattribute
///    them. Soundness control on rows that DO carry `src_agent`: resolving
///    their `src_instid` at their own time returns the same agent 1,533,195
///    times against 7 mismatches.
struct IncomingSourceClassifier<'a> {
    registry: &'a InstidRegistry,
    /// Every agent addr the log declares to be a player.
    players: BTreeSet<u64>,
    /// `agent addr -> master addr`, for ANY agent, resolved once per agent
    /// rather than per row.
    ///
    /// Ownership is an AGENT-level fact, not a per-row one: arcdps emits
    /// many of a minion's rows with `*_master_instid == 0`, so deriving the
    /// link per row drops them (`analysis::minions` measured 49 of 586
    /// (minion, skill) rows lost that way, which is why it resolves the link
    /// from ANY row that carries it and then applies it to all of them).
    /// Same rule here, with one difference: `minions` only needs SQUAD
    /// masters, while the sources classified here are overwhelmingly
    /// ENEMIES, so this map is built over every player rather than the
    /// squad.
    master_of: BTreeMap<u64, u64>,
}

impl<'a> IncomingSourceClassifier<'a> {
    fn build(raw: &RawLog, registry: &'a InstidRegistry) -> Self {
        let players: BTreeSet<u64> =
            raw.agents.iter().filter(|a| a.is_player()).map(|a: &RawAgent| a.addr).collect();

        let mut master_of: BTreeMap<u64, u64> = BTreeMap::new();
        for e in &raw.events {
            for (master_instid, agent_addr) in
                [(e.src_master_instid, e.src_agent), (e.dst_master_instid, e.dst_agent)]
            {
                if master_instid == 0 || agent_addr == 0 || players.contains(&agent_addr) {
                    continue; // a player is never their own minion
                }
                if master_of.contains_key(&agent_addr) {
                    continue; // first link observed wins, as in `minions`
                }
                let Some(owner) = registry.resolve_at(master_instid, e.time) else { continue };
                master_of.insert(agent_addr, owner);
            }
        }
        IncomingSourceClassifier { registry, players, master_of }
    }

    fn classify(&self, e: &RawEvent) -> IncomingSource {
        // `src_agent` first: it is the authoritative naming of the source
        // whenever arcdps had one, and on condition rows (`buff == 1`) it is
        // specifically the APPLIER -- measured over the same 400 captures,
        // 94.48% of condition damage is player-sourced with only 0.16% null,
        // so the condi path needs no instid recovery at all.
        let src = if e.src_agent != 0 {
            e.src_agent
        } else {
            match self.registry.resolve_at(e.src_instid, e.time) {
                Some(addr) => addr,
                // Neither an agent nor a resolvable instid: 0.10% of incoming
                // direct damage. Counted as `Other` rather than folded into
                // the player bucket -- an agent we could not name is not
                // evidence of a player, and under-reading by a tenth of a
                // percent beats inflating the filtered number with guesses.
                None => return IncomingSource::Other,
            }
        };
        if self.players.contains(&src) {
            return IncomingSource::Player;
        }
        match self.master_of.get(&src) {
            Some(master) if self.players.contains(master) => IncomingSource::PlayerMinion,
            _ => IncomingSource::Other,
        }
    }
}

/// Incoming damage per squad member (any source), grouped by skill id --
/// mirrors `damage::accumulate_damage_taken`'s predicate exactly.
///
/// Additionally splits out the player-sourced portion into
/// `SkillStats::player_total`. That is a REFINEMENT of `total`, never a
/// filter on it: every row still lands in `total`, so
/// `sum(taken[*].total) == PlayerMetrics::damage_taken` holds exactly as
/// before, and `player_total <= total` holds per skill by construction.
fn accumulate_taken(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
) -> BTreeMap<u64, BySkill> {
    let sources = IncomingSourceClassifier::build(raw, registry);
    let mut out: BTreeMap<u64, BySkill> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if !crate::analysis::damage::is_health_damage_result(e.result) {
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
        let stats = out.entry(e.dst_agent).or_default().entry(e.skillid).or_default();
        stats.record(dmg, is_crit, is_flank);
        if sources.classify(e).is_player_sourced() {
            stats.player_total += dmg;
        }
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
    /// Of `total`, the portion dealt by a player or a player's minion (see
    /// [`IncomingSource`]).
    ///
    /// `Some` on `taken` rows only. `None` everywhere else means "this split
    /// was not measured for this grouping", never "no player damage" --
    /// outgoing and per-target rows are squad-player-sourced by
    /// construction, so publishing a `0` there would invite exactly the
    /// wrong reading. Same presence convention as `SkillRow::hits`/
    /// `connected_hits` in the schema.
    pub player_total: Option<u64>,
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

/// `include_player_split` publishes `SkillStats::player_total` as
/// `Some`. Only `accumulate_taken` measures it, so only `taken` passes
/// `true` -- see [`SkillEntry::player_total`] for why the other groupings
/// omit the field rather than reporting `0`.
fn to_sorted_vec(by_skill: BySkill, include_player_split: bool) -> Vec<SkillEntry> {
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
            player_total: include_player_split.then_some(s.player_total),
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
    for (addr, by_skill) in accumulate_taken(raw, registry, squad) {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        merge_by_skill(taken_by_rep.entry(rep).or_default(), &by_skill);
    }

    let mut result: BTreeMap<u64, SkillDamageMetrics> = BTreeMap::new();
    for (rep, (by_skill, by_target)) in outgoing_by_rep {
        let entry = result.entry(rep).or_default();
        entry.outgoing = to_sorted_vec(by_skill, false);
        entry.per_target = by_target
            .into_iter()
            .map(|(enemy_id, skills)| PerTargetSkills { enemy_id, skills: to_sorted_vec(skills, false) })
            .collect();
    }
    for (rep, by_skill) in taken_by_rep {
        result.entry(rep).or_default().taken = to_sorted_vec(by_skill, true);
    }
    result
}

/// Per-ENEMY outgoing per-skill damage distribution -- GW2EI's
/// `targets[].totalDamageDist[0]` (MEIGAP Task 2c).
///
/// ## Direction and scope (GW2EI citation)
///
/// `targets[]` entries are built by `JsonNPCBuilder.BuildJsonNPC`, whose
/// first statement is `JsonActorBuilder.FillJsonActor(jsonNPC, npc, ...)`
/// -- so an NPC's `totalDamageDist` is the SAME field a player's is, just
/// over an NPC actor: `JsonActorBuilder.BuildDamageDistData`
/// (`GW2EIBuilders/JsonModels/JsonActors/JsonActorBuilder.cs:109-122`)
/// feeds it `actor.GetJustActorDamageEvents(null, log, ...)`. That is the
/// enemy's **OUTGOING** damage, to anyone (`null` target), grouped by skill
/// id. It is what axibridge's `precomputeGlobalEnemySkillStats`
/// (`packages/bridge-metrics/src/computePlayerAggregation.ts:490-509`)
/// folds into a global `skillId -> {totalDamage, connectedHits, min}` table
/// and then divides into the per-avoided-hit averages behind the
/// damage-mitigation columns (`recomputeMitigationTotals`, `:1517-1560`:
/// `avoided = glanced * avg/2 + (blocked+evaded+missed+invulned+
/// interrupted) * avg`, with `avg = totalDamage / connectedHits`).
///
/// **Actor-ONLY, no minion fold** -- and that is a real difference from
/// this module's squad-side `outgoing`. `GetJustActorDamageEvents`
/// (`EIData/Actors/SingleActor.cs:752-761`) is explicitly
/// `GetDamageEvents(...).Where(x => x.From.Is(AgentItem))`, filtering back
/// out the `mins.GetDamageEvents` fold `InitDamageEvents` applied
/// (`:735-740`). An NPC's minions get their own `minions[]` dist, never the
/// owner's. So this pass credits `src_agent` and nothing else -- which also
/// makes it the mirror image of the documented squad-side divergence in
/// this module's own doc comment (there, EI is actor-only and axilog folds
/// pets; here BOTH are actor-only, so that gap does not arise).
///
/// ## `hits` vs `connectedHits` -- the M12 divergence, RESOLVED for this field
///
/// `JsonDamageDistBuilder.BuildJsonDamageDist`
/// (`GW2EIBuilders/JsonModels/JsonActorUtilities/JsonDamageDistBuilder.cs:48-74`)
/// writes several different counts off one event list:
///
/// - `hits` = `dmgEvt.IsNotADamageEvent ? 0 : 1` -- every ATTEMPT,
///   including missed/blocked/evaded/invulned;
/// - `connectedHits` = `dmgEvt.HasHit ? 1 : 0`;
/// - `min`/`max`/`crit`/`flank` are guarded by `HasHit` too, and
///   `crit`/`flank` additionally by `!IndirectDamage`.
///
/// This module's squad-side [`SkillStats`] counts CONTRIBUTING events
/// (`dmg > 0`) -- the documented M12 divergence from EI's `hits`. **This
/// pass does NOT inherit that divergence**, because the consumer's
/// arithmetic depends on the difference: axibridge's mitigation math
/// divides by `connectedHits` (`computePlayerAggregation.ts:277-286`,
/// `avg = totalDamage / connectedHits`) and averages `min`
/// (`minMitigation`). Measured against the reference export, `dmg > 0` and
/// `HasHit` genuinely disagree: a connecting hit that dealt zero health
/// damage is common enough that it moved `connectedHits` on many
/// (target, skill) rows and collapsed `min` from a real value to `0` on
/// most of them.
///
/// So this pass reproduces `HasHit` directly, from the result byte, with
/// the same era dispatch `defenses::classify` and `hit_stats::classify`
/// already use:
///
/// - `buff == 0`: `DirectNormal`/`DirectCrit`/`DirectGlance`
///   (`DirectHealthDamageEvent.cs:18`). Every other direct result --
///   block/evade/blind/absorb/invert, and the `KillingBlow`/`Downed`
///   marker rows GW2EI routes to `NoDamageHealthDamageEvent` -- is not a
///   hit.
/// - `buff == 1`, post-rework era: `BuffCycle`/`BuffNotCycle`/
///   `BuffNotCycle_DamageToSourceOnHit`, plus the two life-leech results
///   (`NonDirectHealthDamageEvent.cs:32-33`: `HasHit = ... || IsLifeLeech`).
/// - `buff == 1`, pre-rework era: `ConditionResult.ExpectedToHit`
///   (`:17`), which decodes as result `0` on a `value == 0` row -- the
///   same apply-vs-tick disambiguator `hit_stats`/`defenses` established.
///
/// `crit`/`flank` are additionally zeroed for any skill whose row list
/// contains a `NonDirectHealthDamageEvent` -- GW2EI's per-skill
/// `IndirectDamage` flag (`:17`), which gates the whole
/// `flank`/`glance`/`crit`/`missed`/`evaded`/`blocked`/`interrupted` block
/// (`:59-70`). A condition skill therefore reports `crit: 0, flank: 0` on
/// both sides, rather than this project's raw byte counts.
///
/// EI's `hits` (the attempt count) is still NOT emitted: it needs the full
/// missed/blocked/evaded/invulned outcome set, which the adapter has no
/// key for on this shape. The count that IS emitted goes out under
/// `connectedHits`, the key whose EI meaning it reproduces.
///
/// **Standalone, NOT wired into `analyze()`** -- opt-in like
/// [`crate::analysis::timeseries::build_enemy_series`], gated by the
/// adapter on `--skill-damage` (the flag that already gates every other
/// per-skill block).
pub fn build_enemy_dist(
    raw: &RawLog,
    enemies: &BTreeSet<u64>,
    enemy_addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, Vec<SkillEntry>> {
    let post_era = raw.header.is_post_buff_rework();
    let mut by_rep: BTreeMap<u64, BTreeMap<u32, EnemySkillStats>> = BTreeMap::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if !crate::analysis::damage::is_health_damage_result(e.result) {
            continue;
        }
        if !enemies.contains(&e.src_agent) {
            continue;
        }
        // `SingleActor.InitDamageEvents` (`EIData/Actors/SingleActor.cs:734`)
        // filters `.Where(x => !x.ToFriendly)`, and `ToFriendly` is
        // `_iff == IFF.Friend` (`ParsedData/CombatEvents/SkillEvent.cs:17`),
        // i.e. this project's `iff == 0`. Same filter
        // `damage::pet_credit_events` already applies on the friendly side.
        // Measured: it removes ZERO rows on either fixture, so it is
        // faithfulness insurance against a log where an enemy does damage a
        // friendly of its own, not a change with an observed effect.
        if e.iff == 0 {
            continue;
        }
        // The row must actually become a `HealthDamageEvent` before it may
        // create a dist entry -- see `creates_health_damage_event`. Without
        // this gate the entry was created for ANY non-statechange row,
        // including pre-rework-era buff APPLICATION rows, which GW2EI
        // consumes in an earlier dispatch branch entirely. Measured
        // (MEIGAP Task 2 review round 1): on the committed PRE-rework
        // fixture this gate removes **143 of 488** emitted rows -- phantom
        // skill entries GW2EI never emits, all-zero by construction since
        // no row behind them was a damage event (e.g. Taunt 27705 on a
        // Juvenile Brown Bear), worth 19 skill ids in the enemy-player
        // aggregate (199 -> 180). On the POST-rework local capture it
        // removes **0 of 546**: there, buff applies are statechanges the
        // caller already drops, and no row's result byte falls outside the
        // accepted set. So the defect was pre-era-only in practice, and the
        // gate is what keeps it that way rather than by luck.
        if !creates_health_damage_event(e, post_era) {
            continue;
        }
        let rep = enemy_addr_to_rep.get(&e.src_agent).copied().unwrap_or(e.src_agent);
        let entry = by_rep.entry(rep).or_default().entry(e.skillid).or_default();
        // GW2EI's per-skill `IndirectDamage` flag is `dmList.Exists(x => x is
        // NonDirectHealthDamageEvent)` -- set from the WHOLE row list, hits
        // and non-hits alike, so it is recorded after the event-creation gate
        // but before the `has_hit` one.
        //
        // Known narrowness, deliberately not fixed: post-rework a `buff == 1`
        // row carrying a MARKER result (`Interrupt`/`KillingBlow`/`Downed`)
        // becomes a `NoDamageHealthDamageEvent`, NOT a
        // `NonDirectHealthDamageEvent`, so GW2EI would not let it set
        // `IndirectDamage` -- this `e.buff == 1` test would. Zero such rows
        // exist on either fixture (the whole enemy-side calibration is exact
        // with the test as written), so it is documented rather than split
        // into a second result-set match that nothing could exercise.
        if e.buff == 1 {
            entry.indirect = true;
        }
        if !has_hit(e, post_era) {
            continue;
        }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        entry.record(dmg, e.result == result::CRIT, e.is_flanking != 0);
    }
    by_rep
        .into_iter()
        .map(|(rep, by_skill)| {
            (
                rep,
                by_skill
                    .into_iter()
                    .map(|(skill_id, s)| SkillEntry {
                        skill_id,
                        total: s.total,
                        hits: s.hits,
                        min: s.min,
                        max: s.max,
                        // `crit`/`flank` live inside GW2EI's
                        // `if (!jsonDamageDist.IndirectDamage)` block.
                        crit_hits: if s.indirect { 0 } else { s.crit_hits },
                        flank_hits: if s.indirect { 0 } else { s.flank_hits },
                        // Enemy OUTGOING rows; this pass does not classify
                        // sources, so the split is unmeasured, not zero.
                        player_total: None,
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Does this row become a `HealthDamageEvent` at all -- i.e. may it create
/// a `totalDamageDist` entry?
///
/// GW2EI decides this in two steps, and reproducing only the second one is
/// what produced the phantom rows described in [`build_enemy_dist`].
///
/// **Step 1, the dispatch order** (`ParsedData/CombatData.cs:558-584`):
///
/// ```text
/// if      (combatItem.IsCastEvent())            { ... }
/// else if (combatItem.IsBuffApplyOrRemoveEvent()){ ... }
/// else if (combatItem.IsDamageEvent())          { AddDirect... / AddBuffDamage... }
/// ```
///
/// Buff APPLY/REMOVE rows are consumed BEFORE the damage branch is ever
/// reached. Post-rework they are statechanges (`CombatItem.cs:336-338`) and
/// this pass's caller already drops them, but **pre-rework a buff apply is
/// an ordinary combat row** -- `IsBuff != 0 && BuffDmg == 0 && Value > 0`
/// (`CombatItem.cs:340-342`) -- while `IsBuffDamageEvent` pre-rework
/// additionally requires `Value == 0` (`CombatItem.cs:234-238`). That is the
/// bulk of the phantoms on the committed (pre-rework) fixture.
///
/// **Step 2, the result switch.** `AddDirectDamageEvent`
/// (`CombatEventFactory.cs:822-850`) creates a `DirectHealthDamageEvent` for
/// `DirectNormal`/`DirectCrit`/`DirectGlance`/`DirectBlock`/`DirectEvade`/
/// `DirectOrBuffAbsorb`/`DirectBlind`/`DirectOrBuffInvert`, and routes
/// `Interrupt`/`KillingBlow`/`Downed` to a `NoDamageHealthDamageEvent`
/// (`:814-818`) -- which IS in the list, so it still creates a dist entry
/// (with `hits` 0, since `IsNotADamageEvent`). `BreakbarDamage` and
/// `CrowdControl` go to other lists entirely, and everything else hits
/// `default: break` and becomes nothing. `AddBuffDamageDamageEvent`
/// (`:852-894`) is the same shape over the buff results post-rework, and
/// pre-rework accepts any `ConditionResult` other than `Unknown` (values
/// 0-4; `>= 5` decodes as `Unknown` and is dropped).
///
/// **Zero-damage rows are deliberately still allowed through.** A fully
/// blocked or evaded strike is a real `HealthDamageEvent` with
/// `hits > 0, connectedHits = 0`, and the reference export carries 53 such
/// all-zero rows. Dropping them would trade one row-set error for another.
pub(crate) fn creates_health_damage_event(e: &RawEvent, post_era: bool) -> bool {
    // Cast events (`IsCastEvent`) and buff removals are taken by earlier
    // dispatch branches on both eras.
    if e.is_activation != 0 || e.is_buffremove != 0 {
        return false;
    }
    if e.buff == 0 {
        return matches!(
            e.result,
            result::NORMAL
                | result::CRIT
                | result::GLANCE
                | result::BLOCK
                | result::EVADE
                | result::INTERRUPT
                | result::ABSORB
                | result::BLIND
                | result::KILLING_BLOW
                | result::DOWNED
                | result::INVERT
        );
    }
    if post_era {
        return matches!(
            e.result,
            result::INTERRUPT
                | result::KILLING_BLOW
                | result::DOWNED
                | result::ABSORB
                | result::INVERT
                | result::BUFF_CYCLE
                | result::BUFF_NOT_CYCLE
                | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT
                | result::BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT
                | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE
        );
    }
    // Pre-rework buff row: an APPLY (`Value > 0`) never reaches the damage
    // branch, and only `ConditionResult` 0-4 construct an event.
    e.value == 0 && e.result <= 4
}

/// `HealthDamageEvent.HasHit` (see [`build_enemy_dist`]'s doc comment for
/// the three per-era ctor citations). Independent of the row's damage
/// VALUE: a connecting hit that dealt zero health damage is still a hit.
fn has_hit(e: &RawEvent, post_era: bool) -> bool {
    if e.buff == 0 {
        return matches!(e.result, result::NORMAL | result::CRIT | result::GLANCE);
    }
    if post_era {
        return matches!(
            e.result,
            result::BUFF_CYCLE
                | result::BUFF_NOT_CYCLE
                | result::BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT
                | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT
                | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE
        );
    }
    // Pre-era `buff == 1`: `ConditionResult.ExpectedToHit` is result 0, on a
    // damage TICK (`value == 0`) rather than an ordinary buff APPLY row --
    // the same disambiguator `hit_stats`/`defenses` use.
    e.value == 0 && e.result == 0
}

/// [`build_enemy_dist`]'s accumulator. Separate from [`SkillStats`] on
/// purpose: this one counts `HasHit` rows (0-damage ones included) and
/// carries GW2EI's per-skill `IndirectDamage` flag, where `SkillStats`
/// counts CONTRIBUTING rows -- the two must not drift into each other, or
/// the squad-side M12 calibration and this one would both move.
#[derive(Debug, Clone, Default)]
struct EnemySkillStats {
    total: u64,
    hits: u32,
    min: u64,
    max: u64,
    crit_hits: u32,
    flank_hits: u32,
    indirect: bool,
}

impl EnemySkillStats {
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
            is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    /// A player agent (`is_elite != 0xffff_ffff`) vs. an NPC/siege one.
    fn agent(addr: u64, is_player: bool) -> RawAgent {
        RawAgent {
            addr, prof: 1, is_elite: if is_player { 0 } else { 0xffff_ffff },
            toughness: 0, concentration: 0, healing: 0, hitbox_width: 0,
            condition: 0, hitbox_height: 0, name_raw: Vec::new(),
        }
    }

    fn raw_with_agents(agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
        RawLog { agents, ..raw_from(events) }
    }

    /// Every `taken` row's `player_total` for squad member 1, by skill id.
    fn taken_split(raw: &RawLog, squad: &BTreeSet<u64>) -> BTreeMap<u32, (u64, u64)> {
        let result =
            build(raw, squad, &BTreeSet::new(), &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        result
            .get(&1)
            .map(|m| {
                m.taken
                    .iter()
                    .map(|e| {
                        (e.skill_id, (e.total, e.player_total.expect("taken rows carry the split")))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The core of the split: an enemy PLAYER, that player's MINION, and a
    /// piece of siege all hit the same squad member. Only the first two
    /// count as player-sourced, and `total` is untouched in every case --
    /// the split is a refinement of `total`, not a filter on it.
    ///
    /// The minion's damage row deliberately carries `src_master_instid == 0`
    /// (arcdps emits many of them that way); the link is established by a
    /// DIFFERENT row, exactly as `analysis::minions` does it. Deriving
    /// ownership per row would score this skill 0 player-sourced.
    #[test]
    fn taken_splits_player_minion_and_environment_sources() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let mut linking_row = strike(500, 1, 999, 0); // 0 damage: a link, not a hit
        linking_row.src_agent = 500;
        linking_row.src_master_instid = 77;

        let mut minion_hit = strike(500, 1, 300, 40);
        minion_hit.src_master_instid = 0; // the case per-row derivation drops

        let mut owner_row = strike(600, 1, 100, 10);
        owner_row.src_instid = 77; // instid 77 belongs to enemy player 600

        let raw = raw_with_agents(
            vec![agent(1, true), agent(600, true), agent(500, false), agent(900, false)],
            vec![
                owner_row,
                linking_row,
                minion_hit,
                strike(900, 1, 400, 70), // siege: non-player, no master
            ],
        );
        let split = taken_split(&raw, &squad);

        assert_eq!(split.get(&100), Some(&(10, 10)), "enemy player damage is player-sourced");
        assert_eq!(split.get(&300), Some(&(40, 40)), "that player's minion counts too");
        assert_eq!(split.get(&400), Some(&(70, 0)), "siege is not player-sourced");
        // `total` is unchanged by the classification.
        assert_eq!(split.values().map(|(t, _)| t).sum::<u64>(), 120);
        for (skill, (total, player)) in &split {
            assert!(player <= total, "skill {skill}: player_total must not exceed total");
        }
    }

    /// Out-of-bubble sources: arcdps knows which instid hit but has no agent
    /// record for it, so `src_agent` is 0 while `src_instid` is real. Over
    /// 400 real WvW captures these are 3.60% of incoming direct damage and
    /// their top skills are unmistakably player skills, so dropping them
    /// would under-count exactly what the split is for. Resolving the instid
    /// recovers them.
    ///
    /// Also pins the negative: an instid that names nothing stays `Other`
    /// rather than being optimistically credited to a player.
    #[test]
    fn taken_recovers_out_of_bubble_sources_via_instid() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let mut known = strike(600, 1, 100, 10);
        known.src_instid = 42;
        known.time = 100;

        let mut orphan = strike(0, 1, 200, 500); // src_agent unset, instid real
        orphan.src_instid = 42;
        orphan.time = 200;

        let mut unknowable = strike(0, 1, 300, 25); // instid never names an agent
        unknowable.src_instid = 8;
        unknowable.time = 300;

        let raw = raw_with_agents(
            vec![agent(1, true), agent(600, true)],
            vec![known, orphan, unknowable],
        );
        let split = taken_split(&raw, &squad);

        assert_eq!(split.get(&200), Some(&(500, 500)), "orphaned row resolves to the player");
        assert_eq!(
            split.get(&300),
            Some(&(25, 0)),
            "an unnameable source stays non-player: absence of evidence is not a player"
        );
    }

    /// Instid recycling is real -- ~16% of the recovered rows use an instid
    /// bound to more than one agent in the same log -- so resolution must be
    /// time-windowed. A global `instid -> agent` map would credit BOTH eras
    /// below to whichever owner registered last, scoring the siege era's
    /// damage as player-sourced.
    #[test]
    fn taken_resolves_recycled_instids_by_era() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let mut siege_era = strike(900, 1, 100, 10); // instid 42 is siege first
        siege_era.src_instid = 42;
        siege_era.time = 100;
        let mut siege_orphan = strike(0, 1, 400, 60);
        siege_orphan.src_instid = 42;
        siege_orphan.time = 150;

        let mut player_era = strike(600, 1, 100, 10); // then recycled to a player
        player_era.src_instid = 42;
        player_era.time = 500;
        let mut player_orphan = strike(0, 1, 500, 80);
        player_orphan.src_instid = 42;
        player_orphan.time = 550;

        let raw = raw_with_agents(
            vec![agent(1, true), agent(600, true), agent(900, false)],
            vec![siege_era, siege_orphan, player_era, player_orphan],
        );
        let split = taken_split(&raw, &squad);

        assert_eq!(split.get(&400), Some(&(60, 0)), "the siege era must stay non-player");
        assert_eq!(split.get(&500), Some(&(80, 80)), "the player era must be credited");
    }

    /// Condition damage (`buff == 1`) needs no instid recovery: `src_agent`
    /// on those rows IS the applier, measured at 94.48% of condition damage
    /// player-sourced with only 0.16% null over the same 400 captures. Pinned
    /// because the damage field differs (`buff_dmg`, not `value`), so a
    /// classification bug here would be invisible to the direct-damage tests.
    #[test]
    fn taken_splits_condition_damage_by_its_applier() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let mut condi = strike(600, 1, 736, 0);
        condi.buff = 1;
        condi.buff_dmg = 250;
        let mut env_condi = strike(900, 1, 737, 0);
        env_condi.buff = 1;
        env_condi.buff_dmg = 90;

        let raw = raw_with_agents(
            vec![agent(1, true), agent(600, true), agent(900, false)],
            vec![condi, env_condi],
        );
        let split = taken_split(&raw, &squad);

        assert_eq!(split.get(&736), Some(&(250, 250)));
        assert_eq!(split.get(&737), Some(&(90, 0)));
    }

    /// The split is published on `taken` ONLY. Outgoing and per-target rows
    /// omit it rather than reporting a tautological `total`, so `None` there
    /// always means "not measured" and never "no player damage".
    #[test]
    fn only_taken_rows_carry_the_player_split() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let raw = raw_with_agents(
            vec![agent(1, true), agent(9, true)],
            vec![strike(1, 9, 100, 50), strike(9, 1, 200, 30)],
        );
        let result =
            build(&raw, &squad, &enemies, &BTreeMap::new(), &BTreeMap::new(), None, &BTreeMap::new());
        let m = result.get(&1).expect("player 1");

        assert!(m.outgoing.iter().all(|e| e.player_total.is_none()), "outgoing omits the split");
        assert!(
            m.per_target.iter().flat_map(|t| &t.skills).all(|e| e.player_total.is_none()),
            "per-target omits the split"
        );
        assert!(m.taken.iter().all(|e| e.player_total.is_some()), "every taken row carries it");
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
                is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0,
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

    /// MEIGAP Task 2c, review fix 1: `build_enemy_dist` must create a row
    /// only for rows GW2EI turns into a `HealthDamageEvent`.
    ///
    /// The pre-rework buff APPLY row is the case behind 143 of the 488 rows
    /// the committed fixture used to emit: `buff == 1, buff_dmg == 0,
    /// value > 0` is `IsBuffApplyEvent` (`CombatItem.cs:340-342`), consumed
    /// by an earlier dispatch branch, never a damage event.
    #[test]
    fn enemy_dist_skips_pre_era_buff_apply_rows() {
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut apply = strike(9, 1, 27705, 0); // Taunt application
        apply.buff = 1;
        apply.value = 3000; // apply duration, NOT damage
        apply.buff_dmg = 0;
        let raw = raw_from(vec![apply]);
        let out = build_enemy_dist(&raw, &enemies, &BTreeMap::new());
        assert!(out.is_empty(), "a buff APPLY row must not create a dist entry at all");

        // ... while a pre-era buff DAMAGE tick (`value == 0`) does.
        let mut tick = strike(9, 1, 736, 0);
        tick.buff = 1;
        tick.value = 0;
        tick.buff_dmg = 120;
        let raw = raw_from(vec![tick]);
        let out = build_enemy_dist(&raw, &enemies, &BTreeMap::new());
        let rows = out.get(&9).expect("enemy present");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].skill_id, 736);
        assert_eq!(rows[0].total, 120);
        assert_eq!(rows[0].hits, 1, "connectedHits");
    }

    /// A row whose result byte GW2EI's switch drops (`default: break`) must
    /// not create an entry either -- here a `buff == 0` breakbar-damage row
    /// (`BreakbarDamage` goes to a different list, `CombatEventFactory.cs:
    /// 799-809`).
    #[test]
    fn enemy_dist_skips_rows_whose_result_creates_no_health_damage_event() {
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut brk = strike(9, 1, 500, 400);
        brk.result = result::BREAKBAR_DAMAGE;
        let raw = raw_from(vec![brk]);
        assert!(build_enemy_dist(&raw, &enemies, &BTreeMap::new()).is_empty());
    }

    /// A fully BLOCKED strike IS a real `HealthDamageEvent`
    /// (`DirectHealthDamageEvent`, `HasHit == false`), so it must still
    /// create an all-zero row -- the reference export carries 53 of them.
    /// Dropping zero rows would trade one row-set error for another.
    #[test]
    fn enemy_dist_keeps_legitimate_all_zero_rows() {
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut blocked = strike(9, 1, 600, 0);
        blocked.result = result::BLOCK;
        let raw = raw_from(vec![blocked]);
        let rows = build_enemy_dist(&raw, &enemies, &BTreeMap::new()).remove(&9).expect("enemy");
        assert_eq!(rows.len(), 1, "a blocked strike still creates a dist row");
        assert_eq!(rows[0].skill_id, 600);
        assert_eq!((rows[0].total, rows[0].hits, rows[0].min, rows[0].max), (0, 0, 0, 0));
    }

    /// `!ToFriendly` (`SingleActor.cs:734`, `SkillEvent.cs:17`): an enemy's
    /// friend-directed row never enters its own outgoing dist.
    #[test]
    fn enemy_dist_excludes_friend_directed_rows() {
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut friendly = strike(9, 10, 700, 250);
        friendly.iff = 0;
        let raw = raw_from(vec![friendly, strike(9, 1, 700, 50)]);
        let rows = build_enemy_dist(&raw, &enemies, &BTreeMap::new()).remove(&9).expect("enemy");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total, 50, "only the foe-directed row counts");
    }

    /// A CONDITION skill reports `crit: 0, flank: 0` because GW2EI guards
    /// both behind `!IndirectDamage` (`JsonDamageDistBuilder.cs:59-70`), and
    /// the flag is per-SKILL over the whole row list.
    #[test]
    fn enemy_dist_zeroes_crit_and_flank_on_indirect_skills() {
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut tick = strike(9, 1, 736, 0);
        tick.buff = 1;
        tick.value = 0;
        tick.buff_dmg = 90;
        // Pre-era `ConditionResult.ExpectedToHit`; the `is_flanking` byte is
        // set anyway, which EI would refuse to report on an indirect skill.
        tick.result = 0;
        tick.is_flanking = 1;
        let raw = raw_from(vec![tick]);
        let rows = build_enemy_dist(&raw, &enemies, &BTreeMap::new()).remove(&9).expect("enemy");
        assert_eq!(rows[0].total, 90);
        assert_eq!(rows[0].hits, 1);
        assert_eq!((rows[0].crit_hits, rows[0].flank_hits), (0, 0), "IndirectDamage zeroes both");
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
