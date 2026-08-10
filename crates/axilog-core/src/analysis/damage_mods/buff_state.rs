//! "How many stacks of buff B did agent A have at time t" (M16, Task 1).
//!
//! ## What GW2EI does
//!
//! Every buff-gated damage modifier resolves its stack count through the
//! actor's buff GRAPHS, not through a status query on the actor:
//! `BuffsTrackerSingle.GetStack` is
//! `bgms.TryGetValue(_id, out var bgm) ? bgm.GetStackCount(time) : 0`
//! (`GW2EIEvtcParser/EIData/DamageModifiers/BuffsTrackers/
//! BuffsTrackerSingle.cs:12-15`), where `bgms` is
//! `SingleActor.GetBuffGraphs(log)` and `BuffGraph.GetStackCount(time)` is
//! `(int)GetBuffStatus(time).Value`, a binary search over the graph's
//! segments (`GW2EIEvtcParser/EIData/Buffs/BuffGraph.cs:61-64`, returning a
//! zero segment when the graph is empty, `:38-41`).
//!
//! `BuffsTrackerMulti.GetStack` counts PRESENT BUFFS, not stacks:
//! `foreach (long key in bgms.Keys.Intersect(_ids)) stack += bgms[key].
//! GetStackCount(time) > 0 ? 1 : 0` (`BuffsTrackerMulti.cs:7-15`).
//!
//! ## What this does, and why
//!
//! This project already owns the equivalent of GW2EI's buff graph: M3's
//! `analysis::buffs::simulator::run(events, capacity, is_intensity,
//! log_end_ms)` turns extracted apply/remove events into exactly the same
//! `(time, stack_count)` step timeline (`analysis::buffs::BoonTimeline`),
//! and MPERF's `events::extract_buff_events_with_registry(raw, registry,
//! ids)` is already generic over WHICH buff ids to extract -- the boon
//! pipeline just happens to pass `BOON_IDS`.
//!
//! So this module reuses both, unchanged, on a different id set. That is
//! the design choice the Task 1 brief asked to document:
//!
//! - It does NOT touch `simulate_boons`/`simulate_boon_generation_ms` or
//!   their shared `BoonInputs`, so no boon/support output can move. It is a
//!   separate extraction over a separate id set, run only when a definition
//!   actually needs buff state (with the one shipped definition it never
//!   runs at all -- `Moving Bonus` is buff-free).
//! - It does NOT hand-roll a second stack state machine. Era gating of the
//!   apply/remove wire (the pre/post-`ResultEnumRework` split, `BUFF_INITIAL`
//!   seeding, owner-vs-applier field resolution) all lives inside
//!   `extract_buff_events_with_registry` and is inherited for free.
//! - The one thing it must supply that the boon path gets from `BOON_IDS` is
//!   whether a buff is intensity-stacking (and its capacity). That is a
//!   property of the BUFF, so M16 Task 2 transcribes GW2EI's own `Buff`
//!   table for the ids the catalog watches -- see
//!   [`super::catalog::buff_stack`].
//!
//! ## Keying
//!
//! GW2EI asks the MINION's own graphs for a minion hit and the PLAYER's for
//! a player hit (`BuffOnActorDamageModifier.cs:79-99`, `log.FindActor(
//! damageModifier.GetActor(evt))`), and the FOE's for a foe-buff modifier.
//! Timelines are therefore keyed by an "actor key" that is the squad
//! player's REPRESENTATIVE addr when the agent is a known squad player
//! (folding relogs together exactly as `simulate_boons` does) and the raw
//! agent addr otherwise (minions, enemies) -- see [`BuffStates::actor_key`].

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::buffs::{events, simulator};
use crate::analysis::damage::InstidRegistry;
use crate::evtc::RawLog;

/// Per-`(actor key, buff id)` stack-count step timelines, queryable at an
/// arbitrary time.
#[derive(Debug, Clone, Default)]
pub struct BuffStates {
    timelines: BTreeMap<(u64, u32), Vec<(u64, u32)>>,
    addr_to_rep: BTreeMap<u64, u64>,
}

impl BuffStates {
    /// Build timelines for exactly the `(buff id -> is_intensity)` set the
    /// active definitions need. An empty `wanted` short-circuits to an
    /// empty, allocation-free instance -- so a catalog with no buff-gated
    /// definitions costs nothing.
    pub fn build(
        raw: &RawLog,
        registry: &InstidRegistry,
        addr_to_rep: &BTreeMap<u64, u64>,
        wanted: &BTreeMap<u32, bool>,
    ) -> Self {
        let mut out =
            BuffStates { timelines: BTreeMap::new(), addr_to_rep: addr_to_rep.clone() };
        if wanted.is_empty() {
            return out;
        }
        let ids: BTreeSet<u32> = wanted.keys().copied().collect();
        let evs = events::extract_buff_events_with_registry(raw, registry, &ids);
        let capacities = events::extract_buff_capacities(raw, &ids);
        let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);

        let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
        for &e in &evs {
            let key = addr_to_rep.get(&e.owner).copied().unwrap_or(e.owner);
            grouped.entry((key, e.buff_id)).or_default().push(e);
        }
        for ((key, buff_id), group) in grouped {
            // Same capacity preference order `simulate_boons_with_inputs`
            // uses: arcdps's own `sc::BUFF_INFO`-reported capacity when the
            // log carries one, else the hardcoded table.
            // Capacity preference order: arcdps's own `sc::BUFF_INFO`
            // capacity when the log carries one, then GW2EI's catalogued
            // capacity (`super::catalog::buff_stack`), then M3's boon
            // fallback table.
            let capacity = capacities.get(&buff_id).copied().unwrap_or_else(|| {
                super::catalog::buff_stack::stack_info(buff_id)
                    .map(|b| b.capacity)
                    .unwrap_or_else(|| simulator::capacity_for(buff_id))
            });
            let intensity = wanted.get(&buff_id).copied().unwrap_or(false);
            out.timelines
                .insert((key, buff_id), simulator::run(group, capacity, intensity, log_end_ms));
        }
        out
    }

    /// The timeline key for a raw agent addr: the squad representative when
    /// known (relog folding, as `simulate_boons` does), else the raw addr.
    pub fn actor_key(&self, addr: u64) -> u64 {
        self.addr_to_rep.get(&addr).copied().unwrap_or(addr)
    }

    /// `BuffGraph.GetStackCount(time)`: the value of the last transition at
    /// or before `t` (0 before the first one, or when the buff was never
    /// seen on this agent).
    pub fn stack_at(&self, actor_key: u64, buff_id: u32, t: u64) -> u32 {
        let Some(states) = self.timelines.get(&(actor_key, buff_id)) else { return 0 };
        let idx = states.partition_point(|&(time, _)| time <= t);
        if idx == 0 {
            0
        } else {
            states[idx - 1].1
        }
    }

    /// `BuffsTracker.GetStack` -- dispatches on
    /// [`super::model::BuffTracker::multi`].
    pub fn tracker_stack(
        &self,
        tracker: &super::model::BuffTracker,
        actor_key: u64,
        t: u64,
    ) -> u32 {
        if tracker.multi {
            // `BuffsTrackerMulti.cs:7-15` -- number of DISTINCT watched
            // buffs present, never a sum of stacks.
            tracker.ids.iter().filter(|&&id| self.stack_at(actor_key, id, t) > 0).count() as u32
        } else {
            // `BuffsTrackerSingle.cs:12-15`.
            tracker.ids.first().map_or(0, |&id| self.stack_at(actor_key, id, t))
        }
    }
}
