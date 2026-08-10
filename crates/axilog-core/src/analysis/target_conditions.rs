//! Incoming-condition attribution on ENEMY agents -- GW2EI's
//! `targets[].buffs[].id` / `.statesPerSource` (MEIGAP Task 2d).
//!
//! ## What GW2EI emits, and on which side
//!
//! `targets[]` entries carry a `buffs` array built by
//! `JsonNPCBuilder.GetNPCJsonBuffsUptime`
//! (`GW2EIBuilders/JsonModels/JsonActors/JsonNPCBuilder.cs:87-118`) from
//! `npc.GetBuffs(ParserHelper.BuffEnum.Self, log, ...)` -- buffs **held BY
//! the enemy**, i.e. what the squad put ON it. Each entry then goes through
//! the very same `JsonBuffsUptimeBuilder.BuildJsonBuffsUptime` a player's
//! `buffUptimes` entry does (`:117`), so `states`/`statesPerSource` mean
//! exactly what MEIGAP Task 1b already established for players
//! (`buffs::states`'s module doc has the full transcription):
//! `statesPerSource` is `{source character name -> [[time, stacks], ...]}`,
//! transition points only, from the per-source segment overlap.
//!
//! Direction confirmed independently at the consumer:
//! `axibridge/packages/bridge-metrics/src/conditionsMetrics.ts:305-325`
//! walks `targets[].buffs[]`, keeps entries whose `buffMap` classification
//! is `Condition`, and reads `statesPerSource` keyed by SOURCE character
//! name -- resolving each name back to a SQUAD player key
//! (`nameToKey`). That is "which squad player applied which conditions to
//! this enemy", which is what the audit's gap row calls
//! incoming-conditions attribution.
//!
//! ## Scope: CONDITIONS only, `statesPerSource` only
//!
//! GW2EI emits every non-`Hidden` buff on the NPC (boons included) with the
//! full `JsonBuffsUptime` shape (`buffData`, `states`, `buffVolumes`, ...).
//! This pass deliberately emits a strict subset -- the fourteen conditions
//! of `condition_catalog::CONDITION_BUFFS`, with `id` and `statesPerSource`
//! -- for two reasons, both checked rather than assumed:
//!
//! 1. **That is exactly what axibridge reads.** Its loop drops any entry
//!    whose resolved `buffMap` meta is missing or not classified
//!    `Condition` (`conditionsMetrics.ts:311-314`) and then touches only
//!    `buff.statesPerSource`. `states`, `buffData` and `buffVolumes` on a
//!    target are read nowhere.
//! 2. **Payload.** This is the single largest family in MEIGAP: a per-source
//!    step timeline per (enemy, condition). Emitting boons and the total
//!    `states` alongside would multiply it for no consumer. The audit's
//!    gap row itself names only `buffs[].id` + `.statesPerSource`.
//!
//! The `buffMap` entries for those fourteen conditions are emitted
//! alongside (see the adapter), since without them axibridge's
//! `resolveBuffMetaById` returns nothing and the whole array is skipped.
//!
//! ## Mechanics: the boon machinery, re-pointed
//!
//! Everything here is the Task-1b pipeline with two substitutions and
//! nothing else:
//!
//! | | boons (Task 1b) | conditions (here) |
//! |---|---|---|
//! | id table | `buffs::BOON_IDS` | `condition_catalog::CONDITION_BUFFS` |
//! | owner scope | squad `Player::agent_addr` | enemy `Enemy::id` |
//!
//! The event extraction (`buffs::events::extract_buff_events_with_registry`,
//! era-dispatched), the capacity source (`extract_buff_capacities`, with the
//! ctor table as fallback), the two segment simulators
//! (`buffs::generation::run_segments`) and the per-source overlap reduction
//! (`buffs::states::overlap_steps`/`to_ei_states`) are the SAME code, called
//! with different inputs. So a condition timeline on an enemy and a boon
//! timeline on a player can never disagree about simulator semantics.
//!
//! **Standalone, NOT wired into `analyze()`** -- opt-in like
//! `buffs::states`, and gated by the adapter on `--timeseries`, GW2EI's own
//! `RawFormatTimelineArrays` gate on `statesPerSource`
//! (`JsonBuffsUptimeBuilder.cs:52`).

use crate::analysis::buffs::events::BuffEvent;
use crate::analysis::buffs::states::{self, StateTimeline};
use crate::analysis::buffs::{events, generation};
use crate::analysis::condition_catalog::CONDITION_BUFFS;
use crate::analysis::damage::InstidRegistry;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// Per-(enemy representative id, condition id) source-split step timelines.
/// The inner key is the SOURCE's character name, exactly as GW2EI keys
/// `statesPerSource` (GW2EI would use its `UNKNOWN` placeholder for a source that is not a
/// recorded squad player).
pub type TargetConditionStates = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;

/// `(arcdps-reported-or-ctor-table capacity, is_intensity)` for one
/// condition id. Same preference order `generation::capacity_and_kind` uses
/// for boons -- arcdps's own `sc::BUFF_INFO` row wins, since MBUFFSIM
/// measured several real capacities far above the static table's values --
/// falling back to the capacity transcribed from GW2EI's own `Buff` ctor
/// (`CommonBuffs.cs:36-49`).
fn capacity_and_kind(capacities: &BTreeMap<u32, u32>, id: u32) -> (u32, bool) {
    let (_, _, is_intensity, ctor_capacity) = CONDITION_BUFFS
        .iter()
        .copied()
        .find(|&(cid, _, _, _)| cid == id)
        .expect("capacity_and_kind is only ever called with a catalogued condition id");
    (capacities.get(&id).copied().unwrap_or(ctor_capacity), is_intensity)
}

/// Build `targets[].buffs[].statesPerSource` for every tracked enemy.
///
/// `enc.enemies` is the FULL roster (matching `Report::all_enemies`, which
/// is what the adapter's `targets[]` is built from), and every one of an
/// enemy's `agent_addrs` folds onto its representative `Enemy::id` -- the
/// same relog fold the squad side applies to `Player::agent_addr`.
pub fn build(raw: &RawLog, enc: &Encounter) -> TargetConditionStates {
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> TargetConditionStates {
    let ids: BTreeSet<u32> = CONDITION_BUFFS.iter().map(|&(id, _, _, _)| id).collect();
    let all = events::extract_buff_events_with_registry(raw, registry, &ids);
    let capacities = events::extract_buff_capacities(raw, &ids);

    // Enemy addr -> representative id, and squad addr -> character name.
    // The source side folds onto the squad ACCOUNT representative first
    // (so a relogged applier stays one `statesPerSource` key), then onto
    // that account's character name -- the same two-step
    // `buffs::states::build` applies for boons.
    let enemy_rep: BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    let player_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let name_of: BTreeMap<u64, &str> =
        enc.players.iter().map(|p| (p.agent_addr, p.character.as_str())).collect();

    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for &(mut e) in &all {
        let Some(&rep) = enemy_rep.get(&e.owner) else { continue };
        e.agent = player_rep.get(&e.agent).copied().unwrap_or(e.agent);
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_start = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end = raw.events.last().map(|e| e.time).unwrap_or(0);

    let mut out: TargetConditionStates = BTreeMap::new();
    for (key, evs) in grouped {
        let (capacity, is_intensity) = capacity_and_kind(&capacities, key.1);
        let segments = generation::run_segments(evs, capacity, is_intensity, log_end);
        let mut by_source: BTreeMap<u64, Vec<generation::HeldSegment>> = BTreeMap::new();
        for s in segments {
            by_source.entry(s.source).or_default().push(s);
        }
        let mut named: BTreeMap<String, StateTimeline> = BTreeMap::new();
        for (source, segs) in by_source {
            // Only SQUAD-sourced conditions are emitted. axibridge resolves
            // every `statesPerSource` key through `nameToKey`, a map built
            // from the payload's own squad `players[]`, and silently drops
            // anything else (`conditionsMetrics.ts:322-323`) -- so an
            // enemy-applied or unresolved condition is pure payload with no
            // consumer. GW2EI would emit it (under the applier's character
            // name, or `UNKNOWN`); this is a deliberate, documented
            // narrowing, not a semantic difference in what the emitted rows
            // MEAN.
            let Some(name) = name_of.get(&source) else { continue };
            let steps = states::overlap_steps(&segs);
            if steps.is_empty() {
                continue;
            }
            let timeline = states::to_ei_states(steps.into_iter(), log_start);
            // A timeline that never leaves 0 is the mandatory leading pair
            // and nothing else -- no information, so it is dropped rather
            // than emitted (GW2EI cannot produce one: a source with no held
            // segment at all is simply absent from its dictionary).
            if timeline.len() < 2 {
                continue;
            }
            named.insert((*name).to_string(), timeline);
        }
        if !named.is_empty() {
            out.insert(key, named);
        }
    }
    out
}

/// The condition ids this pass can ever emit, in ascending order -- what the
/// adapter needs to know to populate `buffMap` (without which axibridge's
/// `resolveBuffMetaById` fails and the whole `targets[].buffs` array is
/// skipped). `(id, display name)`; all fourteen are `BuffStackType`s that
/// GW2EI's own `buffMap` reports as `"Stacking"`/`"Queue"`, mapped by the
/// adapter the same way the boon `buffMap` entries already are.
pub fn condition_buff_map() -> Vec<(u32, &'static str, bool)> {
    CONDITION_BUFFS.iter().map(|&(id, name, intensity, _)| (id, name, intensity)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::condition_catalog::BLEEDING;

    #[test]
    fn capacity_prefers_the_arcdps_reported_value_over_the_ctor_table() {
        let none: BTreeMap<u32, u32> = BTreeMap::new();
        // Bleeding: `BuffStackType.Stacking`, ctor capacity 1500
        // (`CommonBuffs.cs:36`).
        assert_eq!(capacity_and_kind(&none, BLEEDING), (1500, true));
        let reported: BTreeMap<u32, u32> = [(BLEEDING, 99u32)].into_iter().collect();
        assert_eq!(capacity_and_kind(&reported, BLEEDING), (99, true));
    }

    /// Every catalogued id must be resolvable, and the BOON table must stay
    /// disjoint from it (Might, 740, sits numerically between Vulnerability
    /// and Weakness -- the exact id any range-based shortcut would sweep up).
    #[test]
    fn every_catalogued_condition_resolves_and_boons_stay_out() {
        let none: BTreeMap<u32, u32> = BTreeMap::new();
        for &(id, _, is_intensity, cap) in CONDITION_BUFFS.iter() {
            assert_eq!(capacity_and_kind(&none, id), (cap, is_intensity));
        }
        for &(boon_id, _, _) in crate::analysis::buffs::BOON_IDS.iter() {
            assert!(
                !CONDITION_BUFFS.iter().any(|&(id, _, _, _)| id == boon_id),
                "boon {boon_id} must not be in the condition catalog"
            );
        }
    }
}
