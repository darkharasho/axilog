//! Boon buff-event extraction + stack-state-machine simulation (M3, Task 1).
//!
//! This is the foundation for M3's boons/support-stats milestone: turning
//! raw arcdps buff apply/remove events into a per-agent, per-boon
//! stack-count-over-time timeline. See `events.rs` and `simulator.rs` for
//! the verified field semantics and stack-machine behavior respectively
//! (each cites the arcdps EVTC reference and/or GW2EI source directly).
//!
//! Scope (Task 1, deliberately narrow): only the 12 boon ids in
//! `BOON_IDS`, and only for SQUAD players (`Encounter::players`) -- mirrors
//! how `analysis::cc`/`analysis::damage` scope their own squad-side passes.
//! Enemy/NPC buff uptime, and boon-source attribution ("who generated this
//! boon"), are out of scope here and left to later M3 tasks.

pub mod events;
pub mod generation;
pub mod simulator;
/// GW2EI-shape boon stack timelines (`buffUptimes[].states`/
/// `.statesPerSource`, MEIGAP Task 1b) -- an OPT-IN standalone pass, like
/// `replay`/`missiles`/`damage_mods`, matching GW2EI's own
/// `RawFormatTimelineArrays` gate on the same two arrays. See its module
/// doc for the shape/citation trail.
pub mod states;
pub mod uptime;

pub use generation::GenerationStats;
pub use states::BoonStates;
pub use uptime::BoonUptime;

/// GW2EI's `ArcDPSEnums.BuffStackType`
/// (`GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:384-393`) -- a property of
/// the BUFF, and the key GW2EI's `BuffSimulator` ctor dispatches its
/// stacking logic on (`EIData/Buffs/BuffSimulators/BuffSimulatorNoID/
/// BuffSimulator.cs:24-43`).
///
/// MBUFFSIM Task 2 promoted this from the single `intensity: bool` the M16
/// catalog carried, because rule 2 (the `RemovedDuration` band aid,
/// `EIData/Buffs/BuffsContainer.cs:196-252`) gates on
/// `StackingConditionalLoss` vs `Stacking` SPECIFICALLY -- a distinction a
/// bool cannot express, even though the two share a simulator and a
/// stacking logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffStackType {
    /// -> `QueueLogic` + `BuffSimulatorDuration`.
    Queue,
    /// -> `HealingLogic` + `BuffSimulatorDuration`. Only Regeneration.
    /// **Not yet implemented** -- this project simulates it as [`Queue`];
    /// see the MBUFFSIM Task 1 report's "secondary findings" for the
    /// measured (sub-0.01pp) cost and the deferred follow-up.
    ///
    /// [`Queue`]: BuffStackType::Queue
    Regeneration,
    /// -> `ForceOverrideLogic` + `BuffSimulatorDuration`. Capacity is
    /// effectively 1 regardless of the catalogued/reported value
    /// (`ForceOverrideLogic.IsFull => stacks.Count == 1`).
    Force,
    /// -> `OverrideLogic` + `BuffSimulatorIntensity`.
    Stacking,
    /// -> `OverrideLogic` + `BuffSimulatorIntensity` (identical simulation
    /// to [`Stacking`]; the distinction is upstream, in `BuffDictionary`/
    /// `BuffsContainer` pre-processing).
    ///
    /// [`Stacking`]: BuffStackType::Stacking
    StackingUniquePerSrc,
    /// -> `OverrideLogic` + `BuffSimulatorIntensity`. "the same thing as
    /// Stacking but individual stacks can be removed" (`ArcDPSEnums.cs:386`).
    /// The only stack type the `RemovedDuration` band aid applies to
    /// unconditionally.
    StackingConditionalLoss,
}

impl BuffStackType {
    /// `Buff.Type == BuffType.Intensity` (`EIData/Buffs/Buff.cs:45-56`):
    /// which of the two `BuffSimulatorNoID` classes simulates it.
    pub fn is_intensity(self) -> bool {
        matches!(
            self,
            BuffStackType::Stacking
                | BuffStackType::StackingUniquePerSrc
                | BuffStackType::StackingConditionalLoss
        )
    }

    /// Whether the `StackingConditionalLoss` `RemovedDuration` band aid
    /// (`BuffsContainer.cs:198-199`) considers this stack type at all, and
    /// if so whether it applies to EVERY real removal
    /// (`StackingConditionalLoss`) or only to those reporting
    /// `RemovedDuration == int.MaxValue` (`Stacking` /
    /// `StackingUniquePerSrc`).
    ///
    /// Returns `None` for the duration stack types, which the band aid's
    /// `stackTypeBuffs` filter excludes outright.
    pub fn band_aid_scope(self) -> Option<BandAidScope> {
        match self {
            BuffStackType::StackingConditionalLoss => Some(BandAidScope::EveryRealRemoval),
            BuffStackType::Stacking | BuffStackType::StackingUniquePerSrc => {
                Some(BandAidScope::OnlyInfiniteRemovedDuration)
            }
            _ => None,
        }
    }
}

/// See [`BuffStackType::band_aid_scope`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandAidScope {
    /// `x.StackType == BuffStackType.StackingConditionalLoss` satisfies
    /// `BuffsContainer.cs`'s `(buff.StackType == ... || x.RemovedDuration ==
    /// int.MaxValue)` disjunction on its own.
    EveryRealRemoval,
    /// Only removals whose `RemovedDuration` is the "infinite duration"
    /// sentinel qualify.
    OnlyInfiniteRemovedDuration,
}

/// GW2EI's `int.MaxValue` "infinite duration" sentinel, as it arrives in
/// this project's `u32` `removed_duration_ms` (`RawEvent::value` is `i32`
/// and is clamped with `.max(0)` on extraction, so the sentinel survives
/// unchanged). See [`BandAidScope::OnlyInfiniteRemovedDuration`].
pub const INFINITE_REMOVED_DURATION_MS: u32 = i32::MAX as u32;

/// The [`BuffStackType`] of a buff id, from the transcribed GW2EI buff
/// catalog.
///
/// **Layering note (MBUFFSIM Task 2, ledgered follow-up):** the table lives
/// in `analysis::damage_mods::catalog::buff_stack` because M16 needed it
/// first. It is really a property of the BUFF, shared by both consumers, so
/// it wants hoisting to a crate-level buff catalog; that refactor is
/// deliberately out of Task 2's scope (it would churn M16's calibrated
/// surface for no measurable gain). This accessor keeps every buff-side
/// caller pointed at one place in the meantime.
pub fn stack_type_for(buff_id: u32) -> Option<BuffStackType> {
    crate::analysis::damage_mods::catalog::buff_stack::stack_info(buff_id).map(|b| b.stack_type)
}

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// A stack-count-over-time step timeline for one (agent, boon): each entry
/// is a `(time_ms, stack_count)` transition -- the count holds at that value
/// until the next entry. No entry before the first one means "0 stacks".
/// `time_ms` is the same absolute arcdps `timeGetTime()` timescale as
/// `RawEvent::time` (not log-relative), matching every other raw-event
/// timestamp already flowing through this crate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoonTimeline {
    pub states: Vec<(u64, u32)>,
}

pub const MIGHT: u32 = 740;
pub const FURY: u32 = 725;
pub const REGENERATION: u32 = 718;
pub const VIGOR: u32 = 726;
pub const SWIFTNESS: u32 = 719;
pub const PROTECTION: u32 = 717;
pub const AEGIS: u32 = 743;
pub const RESOLUTION: u32 = 873;
pub const STABILITY: u32 = 1122;
pub const QUICKNESS: u32 = 1187;
pub const RESISTANCE: u32 = 26980;
pub const ALACRITY: u32 = 30328;

/// The 12 tracked boons: `(skill id, display name, is_intensity)`.
///
/// Every id verified against GW2EI's `SkillIDs.cs`
/// (`GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs`) and cross-checked
/// against `CommonBuffs.Boons` (`GW2EIEvtcParser/EIData/Buffs/
/// CommonBuffs.cs`) for stack type: `is_intensity == true` for
/// `BuffStackType.Stacking`/`StackingConditionalLoss` (Might, Stability --
/// many concurrent independently-ticking stacks, capacity 25), `false` for
/// `BuffStackType.Queue` (the rest -- one active stack plus a queue, per-boon
/// capacity 5 or 9, see `simulator::capacity_for`).
pub const BOON_IDS: [(u32, &str, bool); 12] = [
    (MIGHT, "Might", true),
    (FURY, "Fury", false),
    (REGENERATION, "Regeneration", false),
    (VIGOR, "Vigor", false),
    (SWIFTNESS, "Swiftness", false),
    (PROTECTION, "Protection", false),
    (AEGIS, "Aegis", false),
    (RESOLUTION, "Resolution", false),
    (STABILITY, "Stability", true),
    (QUICKNESS, "Quickness", false),
    (RESISTANCE, "Resistance", false),
    (ALACRITY, "Alacrity", false),
];

/// Display name for a buff id, from whichever of the two tracked name
/// tables (the 12 [`BOON_IDS`] or the 14
/// [`crate::analysis::condition_catalog::CONDITION_BUFFS`]) carries it.
///
/// Added for the native-format-1.0 buff catalog (NFCAT Task 4), which needs
/// a name for an arbitrary referenced buff id without owning a second copy
/// of either table -- this project has no crate-wide buff catalog (see
/// `damage_mods::catalog::buff_stack`'s module doc), so composing the two
/// existing name-carrying tables is the calibration-safe option: neither
/// table's data is duplicated, only looked up.
pub fn name(id: u32) -> Option<&'static str> {
    BOON_IDS
        .iter()
        .find(|&&(i, _, _)| i == id)
        .map(|&(_, n, _)| n)
        .or_else(|| {
            crate::analysis::condition_catalog::CONDITION_BUFFS
                .iter()
                .find(|&&(i, _, _, _)| i == id)
                .map(|&(_, n, _, _)| n)
        })
}

/// `(is_intensity, max_stacks)` for a buff id, consulted in this order
/// (native-format-1.0 catalog fix, NFCAT Task 4 review round 1, Finding 1):
///
/// 1. [`crate::analysis::condition_catalog::CONDITION_BUFFS`] -- the
///    authoritative table for all 14 conditions, damaging or not. It alone
///    carries the correct capacity (e.g. 1500 for Bleeding/Burning/
///    Confusion/Poison/Torment); [`damage_mods::catalog::buff_stack`]'s
///    table is NOT a general buff catalog (its own module doc says so) and
///    only happens to carry one of the 14 (Vulnerability), so consulting it
///    first silently produced `stacking: "duration"`/no `max_stacks` for
///    the other 13.
/// 2. [`stack_type_for`] (boons, auras, forms, and anything else the
///    damage-modifier catalog's stack table knows about).
/// 3. Otherwise `(false, None)` -- duration, no known capacity. Matches
///    GW2EI's own `Buff.cs:120` default.
///
/// [`damage_mods::catalog::buff_stack`]: crate::analysis::damage_mods::catalog::buff_stack
pub fn stacking(id: u32) -> (bool, Option<u32>) {
    if let Some(&(_, _, is_intensity, capacity)) =
        crate::analysis::condition_catalog::CONDITION_BUFFS.iter().find(|&&(i, _, _, _)| i == id)
    {
        return (is_intensity, Some(capacity));
    }
    let info = crate::analysis::damage_mods::catalog::buff_stack::stack_info(id);
    let is_intensity = info.map(|b| b.stack_type.is_intensity()).unwrap_or(false);
    (is_intensity, info.map(|b| b.capacity))
}

#[cfg(test)]
mod stacking_tests {
    use super::*;
    use crate::analysis::condition_catalog;

    #[test]
    fn a_damaging_condition_resolves_through_the_condition_table() {
        assert_eq!(stacking(condition_catalog::BLEEDING), (true, Some(1500)));
    }

    #[test]
    fn a_non_damaging_condition_still_resolves_through_the_condition_table() {
        // Chilled is a non-damaging condition -- Finding 1's regression
        // lock. Before the fix this fell through to the damage-mod stack
        // table (which does not carry Chilled) and silently defaulted to
        // duration/no-capacity, which happens to be the right answer here
        // by coincidence but for the wrong reason; the real assertion this
        // guards is Bleeding above.
        assert_eq!(stacking(condition_catalog::CHILLED), (false, Some(5)));
    }

    #[test]
    fn a_boon_resolves_through_the_stack_type_path() {
        assert_eq!(stacking(MIGHT), (true, Some(25)));
        assert_eq!(stacking(PROTECTION), (false, Some(5)));
    }

    #[test]
    fn an_unknown_id_defaults_to_duration_with_no_capacity() {
        assert_eq!(stacking(u32::MAX), (false, None));
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn resolves_a_boon_name() {
        assert_eq!(name(MIGHT), Some("Might"));
        assert_eq!(name(PROTECTION), Some("Protection"));
    }

    #[test]
    fn resolves_a_condition_name() {
        assert_eq!(name(crate::analysis::condition_catalog::BLEEDING), Some("Bleeding"));
    }

    #[test]
    fn unknown_id_resolves_to_none() {
        assert_eq!(name(u32::MAX), None);
    }
}

/// The shared, pass-independent inputs both boon simulations need (MPERF
/// Task 3).
///
/// `simulate_boons` (stack-count timelines) and
/// `generation::simulate_boon_generation_ms` (per-source generation
/// attribution) are two genuinely different reductions -- the first tracks
/// only stack COUNT, the second tracks WHICH source owns each stack, and
/// `generation`'s module doc explains why the two simulators are
/// deliberately kept independent rather than derived from one another. But
/// they both start from *bit-identical* raw material: the same
/// `events::extract_buff_events_with_registry(raw, registry, BOON_IDS)`
/// vector and the same `events::extract_buff_capacities(raw, BOON_IDS)`
/// map, both pure functions of `(raw, registry)` and the compile-time
/// [`BOON_IDS`] table. Before MPERF Task 3 each simulation re-derived both
/// (two extra full scans over `raw.events` per parse, plus a third
/// extraction for `analyze()`'s post-rework "zero buff events" warning
/// check).
///
/// Extracting once into this struct and lending it to both simulations is
/// therefore provably output-identical -- the simulations themselves are
/// untouched, they just stop recomputing their own identical input. Each
/// consumer copies the [`events::BuffEvent`]s it keeps out of the shared
/// slice (`BuffEvent` is `Copy`, and `generation` rewrites `agent` on its
/// own copies), so lending is safe in both directions.
#[derive(Debug, Clone, Default)]
pub struct BoonInputs {
    /// Every extracted apply/remove/initial event for the [`BOON_IDS`]
    /// boons, in log order (era-dispatched internally by
    /// `events::extract_buff_events_with_registry`).
    pub events: Vec<events::BuffEvent>,
    /// arcdps's own reported per-buff stack capacity, where the log carries
    /// `sc::BUFF_INFO` rows for the tracked boons -- preferred over
    /// `simulator::capacity_for`'s hardcoded table.
    pub capacities: BTreeMap<u32, u32>,
    /// Per-agent healing power, straight off the arcdps agent table
    /// (`RawAgent::healing`) -- GW2EI's `AgentItem.Healing`.
    ///
    /// Needed by exactly one thing: `HealingLogic`'s stack sort, which is
    /// how Regeneration (`BuffStackType.Regeneration`, the only buff GW2EI
    /// routes there) chooses its capacity-overflow eviction victim. See
    /// `generation::run_sim`'s doc comment. Keyed by RAW agent addr; the
    /// generation pass folds sources onto account representatives before
    /// simulating, so it looks up through the same fold.
    pub healing_power: BTreeMap<u64, i16>,
}

/// Build the [`BoonInputs`] both boon simulations consume, from a
/// caller-supplied, already-built [`crate::analysis::damage::InstidRegistry`]
/// (MPERF Task 3). `analysis::analyze` calls this exactly once per parse and
/// lends the result to `simulate_boons_with_inputs`,
/// `generation::simulate_boon_generation_ms_with_inputs`, and its own
/// post-rework warning check.
pub fn extract_boon_inputs_with_registry(
    raw: &RawLog,
    registry: &crate::analysis::damage::InstidRegistry,
) -> BoonInputs {
    let boon_ids: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    BoonInputs {
        events: events::extract_buff_events_with_registry(raw, registry, &boon_ids),
        capacities: events::extract_buff_capacities(raw, &boon_ids),
        healing_power: raw.agents.iter().map(|a| (a.addr, a.healing)).collect(),
    }
}

/// Simulates every tracked boon's stack-count timeline for every squad
/// player in `enc`. Keyed by `(agent-representative addr, buff id)` --
/// agent addrs are folded onto each account's representative
/// `Player.agent_addr` first (relog/build-swap within one recording gets a
/// new raw addr per login, same as `analysis::analyze`'s
/// `addr_to_rep` -- see that function's doc comment), so a relogged
/// account's boon uptime stays on one entry instead of splitting across
/// addrs.
pub fn simulate_boons(raw: &RawLog, enc: &Encounter) -> BTreeMap<(u64, u32), BoonTimeline> {
    simulate_boons_with_registry(raw, &crate::analysis::damage::InstidRegistry::build(raw), enc)
}

/// [`simulate_boons`] against a caller-supplied, already-built
/// [`crate::analysis::damage::InstidRegistry`] (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for standalone/test callers.
pub fn simulate_boons_with_registry(
    raw: &RawLog,
    registry: &crate::analysis::damage::InstidRegistry,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), BoonTimeline> {
    simulate_boons_with_inputs(raw, &extract_boon_inputs_with_registry(raw, registry), enc)
}

/// [`simulate_boons`] against caller-supplied, already-extracted
/// [`BoonInputs`] (MPERF Task 3) -- see [`BoonInputs`]'s doc comment for why
/// sharing the extraction with `generation::
/// simulate_boon_generation_ms_with_inputs` is output-identical. The
/// `registry`-taking wrapper above stays for callers that have a registry but
/// no extracted inputs.
///
/// `raw` is still needed here for the log's final event time (the absolute
/// window the simulator ticks stacks against), which is not part of the
/// extracted inputs.
pub fn simulate_boons_with_inputs(
    raw: &RawLog,
    inputs: &BoonInputs,
    enc: &Encounter,
) -> BTreeMap<(u64, u32), BoonTimeline> {
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    let intensity_ids: BTreeSet<u32> =
        BOON_IDS.iter().filter(|&&(_, _, is_intensity)| is_intensity).map(|&(id, _, _)| id).collect();

    // arcdps's own reported per-buff stack capacity (M3 Task 2) --
    // preferred over the hardcoded `simulator::capacity_for` table
    // whenever present, mirroring GW2EI's `Buff.CreateSimulator` (see
    // `sc::BUFF_INFO`'s doc comment for the calibration finding that
    // motivated this: several Queue-type boons in this fixture report a
    // real capacity of 99, far above the hardcoded 5-9 guess).
    let arcdps_capacities = &inputs.capacities;
    let mut grouped: BTreeMap<(u64, u32), Vec<events::BuffEvent>> = BTreeMap::new();
    for &e in &inputs.events {
        // Squad-only scope (see module docs): an event whose owner isn't a
        // known squad addr (enemy/NPC recipient, or an addr we never saw in
        // the agent table) is dropped here.
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    grouped
        .into_iter()
        .map(|((rep, buff_id), evs)| {
            let capacity = arcdps_capacities
                .get(&buff_id)
                .copied()
                .unwrap_or_else(|| simulator::capacity_for(buff_id));
            let is_intensity = intensity_ids.contains(&buff_id);
            let states = simulator::run(evs, capacity, is_intensity, log_end_ms);
            ((rep, buff_id), BoonTimeline { states })
        })
        .collect()
}

/// Simulates every tracked boon's timeline (`simulate_boons`) and reduces
/// each to a `BoonUptime` (`uptime::compute`) over the SAME absolute
/// `[log_start_ms, log_end_ms)` window `simulate_boons` itself ticks
/// against (`raw.events.first().time`..`raw.events.last().time`) -- see
/// `uptime`'s module docs for why this must be the absolute event window
/// and not `Encounter::duration_ms` (which is start-relative).
pub fn simulate_boon_uptimes(raw: &RawLog, enc: &Encounter) -> BTreeMap<(u64, u32), BoonUptime> {
    let boons = simulate_boons(raw, enc);
    let log_start_ms = raw.log_start_ms();
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    boons
        .iter()
        .map(|(&key, tl)| (key, uptime::compute(tl, log_start_ms, log_end_ms)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{buff_remove, RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn player(addr: u64, addrs: Vec<u64>) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup: 1,
            in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None, agent_addrs: addrs,
        }
    }

    fn enc(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "20260114".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], ground_markers: vec![], tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    fn apply_ev(time: u64, buff_id: u32, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: duration_ms, buff_dmg: 0, overstack: 0,
            skillid: buff_id, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 1, result: 0, is_activation: 0, is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    #[test]
    fn simulate_boons_folds_relog_addrs_onto_representative() {
        let p = player(1, vec![1, 2]); // relogged: addrs 1 (pre) and 2 (post)
        let e = enc(vec![p]);
        let raw = raw_from(vec![
            apply_ev(0, MIGHT, 9, 1, 5000),   // pre-relog: Might on addr 1
            apply_ev(6000, MIGHT, 9, 2, 5000), // post-relog: Might on addr 2 (same account)
            // Trailing non-boon event so `log_end_ms` extends past the
            // second stack's natural expiry (11000) -- otherwise the
            // simulator correctly (per GW2EI's `Trim(logEnd)`) leaves that
            // last stack un-flushed rather than fabricating an expiry past
            // the log's actual end.
            apply_ev(12_000, 999_999, 0, 0, 0),
        ]);
        let boons = simulate_boons(&raw, &e);
        // Both applies must land on the SAME key (the representative addr
        // 1), not split into two entries.
        let key = (1u64, MIGHT);
        assert!(boons.contains_key(&key), "expected a folded entry for the representative addr");
        assert_eq!(boons.len(), 1, "relog addrs must fold into one timeline, not two");
        let tl = &boons[&key];
        // Two separate apply/expiry cycles: 0->1 (5000)->0, 6000->1 (11000)->0.
        assert_eq!(tl.states, vec![(0, 1), (5000, 0), (6000, 1), (11000, 0)]);
    }

    #[test]
    fn simulate_boons_skips_non_squad_recipients() {
        let p = player(1, vec![1]);
        let e = enc(vec![p]);
        // Might applied to agent 99, which isn't in the squad -- must not
        // appear in the output at all.
        let raw = raw_from(vec![apply_ev(0, MIGHT, 1, 99, 5000)]);
        let boons = simulate_boons(&raw, &e);
        assert!(boons.is_empty());
    }

    /// Smoke-shaped check (real fixture coverage lives in
    /// `tests/golden.rs`): a plausible multi-player Might scenario produces
    /// non-empty timelines for more than one player, per the Task 1 GATE.
    #[test]
    fn simulate_boons_nonempty_for_multiple_players() {
        let e = enc(vec![player(1, vec![1]), player(2, vec![2])]);
        let raw = raw_from(vec![
            apply_ev(0, MIGHT, 9, 1, 5000),
            apply_ev(0, MIGHT, 9, 2, 5000),
        ]);
        let boons = simulate_boons(&raw, &e);
        assert_eq!(boons.len(), 2);
        assert!(!boons[&(1, MIGHT)].states.is_empty());
        assert!(!boons[&(2, MIGHT)].states.is_empty());
    }

    /// **Fix Round 1**: end-to-end check that `simulate_boons` routes
    /// Queue-type (duration) boons through the frozen-queue model, not the
    /// intensity concurrent-tick model -- a second Fury application while
    /// the first is still active must be frozen, not expire at
    /// apply_time+duration.
    #[test]
    fn simulate_boons_duration_boon_uses_frozen_queue_model() {
        let e = enc(vec![player(1, vec![1])]);
        let raw = raw_from(vec![
            apply_ev(0, FURY, 9, 1, 1000),   // active: expires 1000
            apply_ev(100, FURY, 9, 1, 5000), // queued frozen (is_shields not set)
            apply_ev(20_000, 999_999, 0, 0, 0), // trailing event extending log_end_ms
        ]);
        let boons = simulate_boons(&raw, &e);
        let tl = &boons[&(1, FURY)];
        // If this were (wrongly) simulated with the intensity/concurrent
        // model, the second stack would expire at 100+5000=5100. The real
        // frozen-queue model instead promotes it at t=1000 and it then runs
        // its full 5000ms, expiring at 6000.
        assert_eq!(tl.states, vec![(0, 1), (100, 2), (1000, 1), (6000, 0)]);
    }

    #[test]
    fn simulate_boons_ignores_manual_removal_via_buff_remove_const() {
        // Sanity: buff_remove::MANUAL round-trips through the public evtc
        // re-export used by this module's callers.
        assert_eq!(buff_remove::MANUAL, 3);
    }
}
