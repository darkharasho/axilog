//! Per-skill hit-OUTCOME columns for the player-side damage distributions
//! (MEIGAP2 row 1) -- GW2EI's `players[].totalDamageDist[][]` /
//! `totalDamageTaken[][]` `hits`/`connectedHits`/`glance`/`missed`/
//! `evaded`/`blocked`/`invulned`/`interrupted`/`indirectDamage`.
//!
//! ## Why a separate module from `skill_damage`
//!
//! `skill_damage`'s [`SkillStats`](crate::analysis::skill_damage::SkillStats)
//! deliberately counts only CONTRIBUTING (`dmg > 0`) rows -- the documented
//! M12 divergence from GW2EI's own `hits`. These columns are the opposite
//! kind of number: they exist precisely to count the rows that dealt no
//! damage (a blocked, evaded, missed or invulned attempt). Merging them
//! into that accumulator would mean either changing its established
//! meaning or carrying two counting rules in one struct, so they live in
//! their own pass, keyed the same way (`skillid`) and joined by the
//! adapter.
//!
//! It is also what keeps this OFF the default path. `analyze()` runs every
//! pass unconditionally, and this one classifies rows the existing damage
//! passes reject early; the distributions it annotates are themselves
//! gated behind `--skill-damage`, so this pass is a standalone opt-in
//! builder (the same shape as
//! [`skill_damage::build_enemy_dist`](crate::analysis::skill_damage::build_enemy_dist)
//! and [`timeseries::build_enemy_series`](crate::analysis::timeseries::build_enemy_series))
//! and the flagless run pays nothing for it.
//!
//! ## GW2EI's counting rules, transcribed
//!
//! `JsonDamageDistBuilder.BuildJsonDamageDist`
//! (`GW2EIBuilders/JsonModels/JsonActorUtilities/JsonDamageDistBuilder.cs:48-77`):
//!
//! ```text
//! jsonDamageDist.Hits += dmgEvt.IsNotADamageEvent ? 0 : 1;      // :52
//! if (!jsonDamageDist.IndirectDamage) {
//!     if (dmgEvt.HasHit) { ... Glance += dmgEvt.HasGlanced ? 1 : 0; ... }   // :65
//!     Missed += dmgEvt.IsBlind ? 1 : 0;                          // :69
//!     Evaded += ...; Blocked += ...; Interrupted += ...;         // :70-72
//! }
//! jsonDamageDist.ConnectedHits += dmgEvt.HasHit ? 1 : 0;         // :74
//! jsonDamageDist.Invulned += dmgEvt.IsAbsorbed ? 1 : 0;          // :75
//! ```
//!
//! and `IndirectDamage = dmList.Exists(x => x is NonDirectHealthDamageEvent)`
//! (`:17`) -- a per-SKILL flag set from the whole row list, hits and
//! non-hits alike, which is why it is recorded before any outcome gate
//! below and why the five direct-only counters are zeroed afterwards for an
//! indirect skill. `connectedHits`/`invulned`/`hits` are outside that
//! guard and are counted for condition skills too.
//!
//! This is the same rule set `minions::build` already reproduces for
//! `minions[].totalDamageTakenDist` (MEIGAP Task 3b) -- the row-existence
//! gate (`skill_damage::creates_health_damage_event`), the marker-row
//! exclusion from `hits`, the `defenses::classify_outcome` dispatch and the
//! glance re-test off the raw result byte are all literally the same
//! sequence, applied to the player's own two distributions instead of a
//! minion's.
//!
//! ## Scope: exactly what axibridge reads
//!
//! - **Taken** (`totalDamageTaken`) is the player half of the damage
//!   mitigation table (`packages/bridge-metrics/src/
//!   computePlayerAggregation.ts:1434-1459`), whose `avoided = glanced *
//!   avg/2 + (blocked + evaded + missed + invulned + interrupted) * avg`
//!   needs every one of those five counters plus `glance`. The minion half
//!   of that same table landed in MEIGAP Task 3b; this is its other half.
//! - **Outgoing** (`totalDamageDist`) is read for `connectedHits`
//!   (`computeSpikeDamageData.ts:415`, with a `hits` fallback) and, more
//!   importantly, for `indirectDamage`, which every strike-damage surface
//!   uses as a skip filter (`:21`, `:152`, `:411`) -- without it condition
//!   ticks are silently counted as strike damage. The remaining outcome
//!   counters come along for free from the same classification and are
//!   emitted because real EI emits them on this dist too.
//!
//! ## Row sets
//!
//! Both directions here can produce a skill id `skill_damage` has no row
//! for at all -- a skill every one of whose attempts was blocked deals no
//! damage and so never reaches that module's `dmg > 0` accumulator, while
//! GW2EI emits it with `totalDamage: 0, hits: n`. Those rows matter to the
//! consumer (they are pure mitigation), so the adapter emits the UNION and
//! this pass does not filter itself down to the damage dist's ids.
//!
//! ## Pet/minion fold
//!
//! The outgoing side mirrors `skill_damage`'s own two-part accumulation --
//! direct rows plus friendly pet/minion rows credited to the owner through
//! the same `InstidRegistry` resolution -- so the outcome columns describe
//! the same event set as the `totalDamage`/`hits` they annotate. (That
//! fold is itself a documented divergence from GW2EI, whose player dist is
//! actor-only; it is inherited here on purpose rather than introducing a
//! second, differently-scoped row set inside one JSON object.) The taken
//! side needs no fold: a minion's own damage taken is a separate
//! `minions[]` dist on both sides.

use super::damage::InstidRegistry;
use super::defenses::{classify_outcome, Outcome};
use super::skill_damage::creates_health_damage_event;
use crate::evtc::{result, RawEvent, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One skill id's outcome counters within one distribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillOutcomes {
    pub skill_id: u32,
    /// GW2EI's `hits`: every row that is not one of its
    /// `NoDamageHealthDamageEvent` markers (`Interrupt`/`KillingBlow`/
    /// `Downed`).
    pub hits: u32,
    /// GW2EI's `connectedHits`: `HasHit` rows, condition skills included.
    pub connected_hits: u32,
    pub glance: u32,
    pub missed: u32,
    pub evaded: u32,
    pub blocked: u32,
    pub invulned: u32,
    pub interrupted: u32,
    /// GW2EI's per-skill `IndirectDamage` flag.
    pub indirect: bool,
}

/// One player's outcome columns for both player-side distributions, each
/// sorted by skill id ascending.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DistOutcomes {
    pub outgoing: Vec<SkillOutcomes>,
    pub taken: Vec<SkillOutcomes>,
}

type BySkill = BTreeMap<u32, SkillOutcomes>;

fn record(rows: &mut BySkill, e: &RawEvent, post_era: bool) {
    let row = rows.entry(e.skillid).or_insert(SkillOutcomes { skill_id: e.skillid, ..Default::default() });
    // Set from the WHOLE row list, before any outcome gate -- GW2EI's
    // `dmList.Exists(x => x is NonDirectHealthDamageEvent)`.
    if e.buff == 1 {
        row.indirect = true;
    }
    let is_marker = e.buff == 0
        && matches!(e.result, result::INTERRUPT | result::KILLING_BLOW | result::DOWNED);
    if !is_marker {
        row.hits += 1;
    }
    match classify_outcome(e, post_era) {
        Some(Outcome::Hit { .. }) => {
            row.connected_hits += 1;
            if e.result == result::GLANCE {
                row.glance += 1;
            }
        }
        Some(Outcome::Blocked) => row.blocked += 1,
        Some(Outcome::Evaded) => row.evaded += 1,
        Some(Outcome::Missed) => row.missed += 1,
        Some(Outcome::Invulned) => row.invulned += 1,
        Some(Outcome::Interrupted) => row.interrupted += 1,
        None => {}
    }
}

/// `glance`/`missed`/`evaded`/`blocked`/`interrupted` all sit inside
/// GW2EI's `if (!IndirectDamage)` guard (`JsonDamageDistBuilder.cs:62-73`),
/// so a condition skill reports zero for all five -- applied as a post-pass
/// because the flag is only final once every row has been seen.
fn finish(rows: BySkill) -> Vec<SkillOutcomes> {
    rows.into_values()
        .map(|mut row| {
            if row.indirect {
                row.glance = 0;
                row.missed = 0;
                row.evaded = 0;
                row.blocked = 0;
                row.interrupted = 0;
            }
            row
        })
        .collect()
}

/// [`build_with_registry`] for a standalone caller that has only the log
/// and its resolved [`Encounter`] -- derives the squad/enemy addr sets, the
/// relog folds, the WvW team map and a private [`InstidRegistry`] the same
/// way `analyze()` does. This is the entry point the CLI and both SDKs use
/// (matching `minions::build`'s own `(raw, enc)` shape).
pub fn build(raw: &RawLog, enc: &Encounter) -> BTreeMap<u64, DistOutcomes> {
    let squad: BTreeSet<u64> =
        enc.players.iter().flat_map(|p| p.agent_addrs.iter().copied()).collect();
    let enemies: BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    build_with_registry(
        raw,
        &InstidRegistry::build(raw),
        &squad,
        &enemies,
        &addr_to_rep,
        friendly_team,
        &agent_team,
    )
}

/// Build both distributions' outcome columns for every squad player, keyed
/// by account representative addr.
///
/// Predicates mirror `skill_damage`'s own three accumulators exactly --
/// same roster tests, same pet-credit resolution, same `iff`/team gates --
/// with one deliberate difference: no `dmg > 0` skip, since counting
/// zero-damage rows is the entire point (see this module's doc comment).
#[allow(clippy::too_many_arguments)]
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, DistOutcomes> {
    let post_era = raw.header.is_post_buff_rework();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);
    let mut outgoing: BTreeMap<u64, BySkill> = BTreeMap::new();
    let mut taken: BTreeMap<u64, BySkill> = BTreeMap::new();

    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if !creates_health_damage_event(e, post_era) {
            continue;
        }
        // Outgoing, direct (`skill_damage::accumulate_outgoing_direct`).
        if squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            record(outgoing.entry(rep(e.src_agent)).or_default(), e, post_era);
        } else if e.iff != 0
            && !squad.contains(&e.src_agent)
            && agent_team.get(&e.src_agent).copied() == friendly_team
        {
            // Outgoing, friendly pet/minion credit
            // (`skill_damage::accumulate_outgoing_pet_credit`): destination
            // is deliberately NOT restricted to `enemies`, matching that
            // function's own note (the pet's `iff` already excludes
            // friend-directed rows).
            if let Some(owner) = registry.resolve_at(e.src_master_instid, e.time) {
                if squad.contains(&owner) {
                    record(outgoing.entry(rep(owner)).or_default(), e, post_era);
                }
            }
        }
        // Taken, any source (`skill_damage::accumulate_taken`).
        if squad.contains(&e.dst_agent) {
            record(taken.entry(rep(e.dst_agent)).or_default(), e, post_era);
        }
    }

    let mut out: BTreeMap<u64, DistOutcomes> = BTreeMap::new();
    for (addr, rows) in outgoing {
        out.entry(addr).or_default().outgoing = finish(rows);
    }
    for (addr, rows) in taken {
        out.entry(addr).or_default().taken = finish(rows);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader};

    fn base() -> RawEvent {
        RawEvent {
            time: 1_000, src_agent: 1, dst_agent: 2, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 7, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn log(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 1 },
            agents: Vec::new(),
            skills: Vec::new(),
            events,
            guid_map: Vec::new(),
        }
    }

    fn built(events: Vec<RawEvent>) -> BTreeMap<u64, DistOutcomes> {
        let raw = log(events);
        let registry = InstidRegistry::build(&raw);
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [2u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        build_with_registry(&raw, &registry, &squad, &enemies, &addr_to_rep, None, &BTreeMap::new())
    }

    #[test]
    fn zero_damage_outcomes_are_counted_where_the_damage_dist_drops_them() {
        let rows = built(vec![
            RawEvent { result: result::BLOCK, ..base() },
            RawEvent { result: result::EVADE, ..base() },
            RawEvent { result: result::BLIND, ..base() },
            RawEvent { result: result::ABSORB, ..base() },
            RawEvent { result: result::NORMAL, value: 500, ..base() },
            RawEvent { result: result::GLANCE, value: 100, ..base() },
        ]);
        let row = &rows[&1].outgoing[0];
        assert_eq!(row.skill_id, 7);
        assert_eq!(row.hits, 6);
        assert_eq!(row.connected_hits, 2);
        assert_eq!((row.blocked, row.evaded, row.missed, row.invulned), (1, 1, 1, 1));
        assert_eq!(row.glance, 1);
        assert!(!row.indirect);
    }

    #[test]
    fn marker_rows_do_not_count_as_hits_but_interrupts_still_count() {
        let rows = built(vec![
            RawEvent { result: result::INTERRUPT, ..base() },
            RawEvent { result: result::KILLING_BLOW, ..base() },
            RawEvent { result: result::DOWNED, ..base() },
        ]);
        let row = &rows[&1].outgoing[0];
        assert_eq!(row.hits, 0, "GW2EI's `IsNotADamageEvent` rows");
        assert_eq!(row.interrupted, 1);
    }

    #[test]
    fn an_indirect_skill_zeroes_the_direct_only_counters() {
        let rows = built(vec![
            RawEvent { buff: 1, result: 0, value: 0, buff_dmg: 300, ..base() },
            RawEvent { result: result::BLOCK, ..base() },
        ]);
        let row = &rows[&1].outgoing[0];
        assert!(row.indirect);
        assert_eq!(row.blocked, 0, "inside GW2EI's `if (!IndirectDamage)` guard");
        assert_eq!(row.hits, 2, "`hits` is outside that guard");
        assert_eq!(row.connected_hits, 1, "so is `connectedHits`");
    }

    #[test]
    fn incoming_rows_land_on_the_taken_distribution() {
        let rows = built(vec![RawEvent {
            src_agent: 2,
            dst_agent: 1,
            result: result::BLOCK,
            ..base()
        }]);
        assert!(rows[&1].outgoing.is_empty());
        assert_eq!(rows[&1].taken[0].blocked, 1);
    }
}
