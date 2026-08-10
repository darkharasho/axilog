//! Orphaned-instid attribution repair (MATTRIB Task 1) -- a decode post-pass
//! that rewrites combat rows whose `src_agent`/`dst_agent` is `0` while the
//! matching instid field is live.
//!
//! # Why
//!
//! arcdps occasionally emits combat rows with a zeroed agent address but a
//! populated instid (observed on the reference capture: an enemy ranger pet's
//! rows, found during M16 Task 1). Every addr-keyed pass in this crate
//! (`damage`, `hit_stats`, `defenses`, `skill_damage`, `contribution`, ...)
//! tests membership by address, so those rows were silently dropped. GW2EI
//! does NOT drop them: it repairs the address from the instid inside the
//! parser, BEFORE any analysis runs.
//!
//! # The GW2EI algorithm, transcribed
//!
//! Everything below is `GW2EIEvtcParser/EvtcParser.cs`'s `CompleteAgents`
//! (`:1125-1245`, clone `7a6fe03`), which runs from `ParseCombatItems`
//! (`:389`) -- i.e. after the agent table is read and before any
//! `EIData`/statistics pass. Rule by rule:
//!
//! 1. **The agent universe** (`:1127-1141`). `allAgentValues` = every
//!    `SrcAgent` from rows where `SrcIsAgent()` plus every `DstAgent` from
//!    rows where `DstIsAgent()`, minus the addresses already in the evtc
//!    agent table, minus `0` (`:1136` -- `allAgentValues.Remove(0)`, the rule
//!    that makes addr-`0` rows orphans in the first place). Each remaining
//!    value becomes a synthesized `UNKNOWN <addr>` agent (`:1139-1141`). So
//!    after this step EVERY non-zero address has an agent and `0` never does.
//! 2. **`SrcIsAgent()` / `DstIsAgent()`** (`CombatItem.cs:240-291` and
//!    `:302-317`) decide which rows have a real agent in the src/dst slot at
//!    all -- transcribed verbatim in [`src_is_agent`] / [`dst_is_agent`]
//!    below. This gating is load-bearing twice over: it selects which rows
//!    may be repaired, and it keeps the pass from touching statechange rows
//!    whose `dst_agent` is a payload rather than an agent (e.g.
//!    `MaxHealthUpdate`'s max health, `HealthUpdate`'s percent).
//! 3. **The main pass** (`:1154-1211`) walks the combat items in stream
//!    order. For an addr already in the agent lookup it calls
//!    `UpdateAgentData` (`:980-1003`), which (a) assigns the agent's `InstID`
//!    the first time a non-zero instid is seen for it and (b) grows the
//!    agent's `[FirstAware, LastAware]` window to include the row's time
//!    (the first observation sets both, `:994-1001`). For an addr NOT in the
//!    lookup -- which, per rule 1, means addr `0` -- the row is pushed onto
//!    `orphanedSrcInstidCombatItems` (`:1179-1182`) / `orphanedDstInstid-
//!    CombatItems` (`:1207-1210`), but ONLY when the corresponding instid is
//!    `> 0`.
//! 4. **The repair** (`:1213-1245`), run only if at least one orphan exists.
//!    An `instid -> agents` lookup is built from the POST-pass agent state
//!    (`:1215-1220`), each bucket sorted by `FirstAware` (stable,
//!    `AgentItem.cs:667-670`). Then, src orphans first (`:1221-1232`) and dst
//!    orphans second (`:1233-1244`), each in stream order:
//!    - candidate = the FIRST agent in the instid's bucket (i.e. lowest
//!      `FirstAware`) satisfying `InAwareTimes(t - 300) || InAwareTimes(t +
//!      300)` (`:1225` / `:1237`), where `InAwareTimes(x)` is
//!      `FirstAware <= x && LastAware >= x` (`AgentItem.cs:310-313`).
//!    - on a hit: `OverrideSrcAgent`/`OverrideDstAgent` rewrites the row's
//!      address IN PLACE, then `UpdateAgentData(candidate, c.Time, 0, false)`
//!      grows that candidate's aware window to cover the repaired row --
//!      which can make a later orphan match a candidate it otherwise would
//!      not, so the two loops are order-dependent and this port keeps that
//!      order.
//!    - on a miss (unknown instid, or no candidate in window): the row is
//!      left with its zero address and is dropped downstream exactly as
//!      before.
//!
//! Note the ±300ms rule is NOT "the aware window widened by 300ms": it tests
//! the two probe points `t-300` and `t+300` for containment. An agent whose
//! entire aware window sits strictly inside `(t-300, t+300)` -- i.e. one
//! alive for under 600ms around the orphaned row -- fails BOTH probes and is
//! rejected. That is GW2EI's literal behaviour and [`repair_orphaned_agents`]
//! reproduces it (see `rejects_candidate_whose_window_is_inside_the_probes`).
//!
//! # Deliberate divergences
//!
//! - **One agent per address.** GW2EI's `agentsLookup` groups the agent table
//!   by address and can hold several `AgentItem`s per address, in which case
//!   `UpdateAgentData`'s `checkInstid` arm (`:988-991`) picks the one whose
//!   instid matches and rows matching none end up in `invalid*CombatItems`
//!   and may be deleted (`:1246-1256`). This port keeps one slot per address
//!   (the evtc agent table is addr-unique on every capture in the calibration
//!   set), so `checkInstid` is always `false` and no row is ever deleted --
//!   matching GW2EI exactly for addr-unique tables, and never removing rows
//!   in the (unobserved) duplicate case.
//! - **Deterministic tie-break.** GW2EI's bucket sort is stable over
//!   `_allAgentsList`, whose synthesized-agent tail comes from `HashSet`
//!   iteration order (i.e. is not reproducible). This port orders buckets by
//!   `(first_aware, discovery order)`, with discovery order = evtc table
//!   order followed by first-appearance order in the event stream. This can
//!   only differ from GW2EI when two agents share an instid AND a
//!   `FirstAware`.
//! - **`ArcDPSAgentRedirection`** (`:1158-1161` / `:1186-1189`) is an
//!   encounter-specific (raid/strike) address remap, empty for WvW, and is
//!   not ported.

use super::{RawAgent, RawEvent};
use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, Hasher};

/// A trivial multiply-shift hasher for the addr -> slot map. arcdps agent
/// addresses are pointer-derived and already well spread; SipHash's cost
/// shows up directly in [`repair_orphaned_agents`]'s per-row lookups (two
/// per event over the whole stream). Only ever used for lookup -- the map is
/// never iterated -- so this cannot affect output ordering/determinism.
#[derive(Default)]
struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_u64(&mut self, n: u64) {
        // fibonacci hashing: spread the low bits into the high ones.
        self.0 = (self.0 ^ n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 ^= self.0 >> 29;
    }
}

type AddrMap = HashMap<u64, usize, BuildHasherDefault<AddrHasher>>;

/// GW2EI's ±300ms probe offset (`EvtcParser.cs:1225`, `:1237`).
const AWARE_SLACK_MS: u64 = 300;

/// `CombatItem.SrcIsAgent()` (`GW2EIEvtcParser/CombatItem.cs:240-291`) --
/// true when the row's `src_agent` slot holds a real agent address.
/// Transcribed against `ArcDPSEnums.StateChange` (`:260-344`) by ordinal, so
/// codes this crate has no named constant for are still handled. `Extension`
/// (40) / `ExtensionCombat` (49) are deliberately absent: GW2EI's
/// parameterless overload (the one `CompleteAgents` calls) excludes them.
pub fn src_is_agent(statechange: u8) -> bool {
    matches!(
        statechange,
        0    // Combat
        | 1  // EnterCombat
        | 2  // ExitCombat
        | 3  // ChangeUp
        | 4  // ChangeDead
        | 5  // ChangeDown
        | 6  // Spawn
        | 7  // Despawn
        | 8  // HealthUpdate
        | 11 // WeaponSwap
        | 12 // MaxHealthUpdate
        | 13 // PointOfView
        | 18 // BuffInitial
        | 19 // Position       \
        | 20 // Velocity        > IsGeographical (CombatItem.cs:44-46)
        | 21 // Rotation       /
        | 22 // TeamChange
        | 23 // AttackTarget
        | 24 // Targetable
        | 27 // StackActive
        | 28 // StackDeactive
        | 29 // Guild
        | 34 // BreakbarState
        | 35 // BreakbarPercent
        | 37 // Marker
        | 38 // BarrierUpdate
        | 44 // Last90BeforeDown
        | 45 // Effect_45
        | 51 // Effect_51
        | 55 // Glider
        | 56 // StunBreak
        | 57 // MissileCreate
        | 58 // MissileLaunch
        | 59 // MissileRemove
        | 60 // EffectGroundCreate
        | 62 // EffectAgentCreate
        | 67 // AnimationStart
        | 68 // AnimationStop
        | 69 // BuffApply
        | 71 // BuffRemoveSingle
        | 72 // BuffRemoveAll
        | 73 // Transformation
        | 76 // StealthChange
        | 77 // GadgetAnimation
        | 78 // GadgetNameVisible
        | 79 // EffectMissileCreate
        | 80 // GadgetCaptureOutlineShow
        | 81 // GadgetCaptureSplitPercent
        | 82 // GadgetCaptureOutlineHide
        | 83 // GadgetCaptureOutlinePoint
    )
}

/// `CombatItem.DstIsAgent()` (`GW2EIEvtcParser/CombatItem.cs:302-317`) --
/// true when the row's `dst_agent` slot holds a real agent address (rather
/// than a statechange payload). Far narrower than [`src_is_agent`].
pub fn dst_is_agent(statechange: u8) -> bool {
    matches!(
        statechange,
        0    // Combat
        | 18 // BuffInitial
        | 23 // AttackTarget
        | 45 // Effect_45
        | 47 // LogNPCUpdate
        | 51 // Effect_51
        | 58 // MissileLaunch
        | 62 // EffectAgentCreate
        | 67 // AnimationStart
        | 69 // BuffApply
        | 70 // BuffChange
        | 71 // BuffRemoveSingle
        | 72 // BuffRemoveAll
    )
}

/// What [`repair_orphaned_agents`] did, for diagnostics/tests (the decode
/// path itself discards it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepairStats {
    /// Rows with `src_agent == 0` and `src_instid > 0` on an agent-bearing
    /// statechange (GW2EI's `orphanedSrcInstidCombatItems`).
    pub src_orphans: usize,
    /// Ditto for the dst side (`orphanedDstInstidCombatItems`).
    pub dst_orphans: usize,
    /// Orphans whose address was rewritten from a qualifying candidate.
    pub src_repaired: usize,
    /// Ditto for the dst side.
    pub dst_repaired: usize,
}

impl RepairStats {
    /// Total rows rewritten (a single row can be repaired on both sides, and
    /// then counts twice).
    pub fn repaired(&self) -> usize {
        self.src_repaired + self.dst_repaired
    }
}

/// One GW2EI `AgentItem`, reduced to the three fields `CompleteAgents` uses.
#[derive(Debug, Clone, Copy)]
struct Slot {
    addr: u64,
    /// The first non-zero instid observed for this address
    /// (`UpdateAgentData`, `EvtcParser.cs:982-987`). `0` = never observed.
    instid: u16,
    first_aware: u64,
    /// `u64::MAX` is GW2EI's `long.MaxValue` "never observed" sentinel
    /// (`AgentItem.cs:59`); the first observation overwrites both bounds.
    last_aware: u64,
}

impl Slot {
    /// `AgentItem.InAwareTimes(long)` (`AgentItem.cs:310-313`).
    fn in_aware_times(&self, t: u64) -> bool {
        self.first_aware <= t && self.last_aware >= t
    }

    /// `UpdateAgentData(ag, logTime, instid, false)` (`EvtcParser.cs:980-1003`),
    /// minus the `checkInstid` arm (see the module doc's divergence note).
    fn update(&mut self, time: u64, instid: u16) {
        if instid != 0 && self.instid == 0 {
            self.instid = instid;
        }
        if self.last_aware == u64::MAX {
            self.first_aware = time;
            self.last_aware = time;
        } else {
            self.first_aware = self.first_aware.min(time);
            self.last_aware = self.last_aware.max(time);
        }
    }
}

/// GW2EI's `EvtcParser.CompleteAgents` orphaned-instid repair, in place over
/// the decoded event stream. See the module doc for the rule-by-rule
/// transcription and the citations.
///
/// Called by [`super::decode_raw`] so that EVERY consumer -- `analysis`,
/// the standalone `replay`/`missiles`/`health` builders, the SDKs and the
/// `--format ei-json` exporter alike -- sees the repaired stream, matching
/// GW2EI's placement (repair inside the parser, before anything reads the
/// events).
pub fn repair_orphaned_agents(agents: &[RawAgent], events: &mut [RawEvent]) -> RepairStats {
    let mut stats = RepairStats::default();

    // The agent universe (rule 1): evtc-table addresses first, in table
    // order, then every other non-zero address seen in an agent-bearing
    // slot, in first-appearance order. `slot_of` is the addr -> index map;
    // `Vec` order is the deterministic tie-break for equal `first_aware`.
    let mut slots: Vec<Slot> = Vec::with_capacity(agents.len());
    let mut slot_of = AddrMap::default();
    for a in agents {
        if a.addr == 0 {
            continue;
        }
        slot_of.entry(a.addr).or_insert_with(|| {
            slots.push(Slot { addr: a.addr, instid: 0, first_aware: 0, last_aware: u64::MAX });
            slots.len() - 1
        });
    }

    // The main pass (rule 3): assign instids, grow aware windows, and note
    // the orphans. One scan, src side before dst side per row, exactly as
    // GW2EI walks `_combatItems`.
    let mut src_orphans: Vec<usize> = Vec::new();
    let mut dst_orphans: Vec<usize> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if src_is_agent(e.is_statechange) {
            if e.src_agent != 0 {
                let idx = *slot_of.entry(e.src_agent).or_insert_with(|| {
                    slots.push(Slot {
                        addr: e.src_agent, instid: 0, first_aware: 0, last_aware: u64::MAX,
                    });
                    slots.len() - 1
                });
                slots[idx].update(e.time, e.src_instid);
            } else if e.src_instid > 0 {
                src_orphans.push(i);
            }
        }
        if dst_is_agent(e.is_statechange) {
            if e.dst_agent != 0 {
                let idx = *slot_of.entry(e.dst_agent).or_insert_with(|| {
                    slots.push(Slot {
                        addr: e.dst_agent, instid: 0, first_aware: 0, last_aware: u64::MAX,
                    });
                    slots.len() - 1
                });
                slots[idx].update(e.time, e.dst_instid);
            } else if e.dst_instid > 0 {
                dst_orphans.push(i);
            }
        }
    }
    stats.src_orphans = src_orphans.len();
    stats.dst_orphans = dst_orphans.len();
    if src_orphans.is_empty() && dst_orphans.is_empty() {
        return stats;
    }

    // The instid -> candidates lookup, built from the POST-pass agent state
    // and sorted by `first_aware` (`EvtcParser.cs:1215-1220`). Agents that
    // were never observed keep `instid == 0` and so can never be candidates
    // (orphan repair requires `instid > 0`) -- the same outcome as GW2EI,
    // where such agents are dropped a few lines later (`:1258`).
    let mut by_instid: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for (i, s) in slots.iter().enumerate() {
        if s.instid != 0 {
            by_instid.entry(s.instid).or_default().push(i);
        }
    }
    for cands in by_instid.values_mut() {
        // Stable: preserves discovery order for equal `first_aware`.
        cands.sort_by_key(|&i| slots[i].first_aware);
    }

    // The repair (rule 4): src orphans in stream order, then dst orphans.
    for &i in &src_orphans {
        let (t, instid) = (events[i].time, events[i].src_instid);
        if let Some(idx) = pick_candidate(&slots, &by_instid, instid, t) {
            events[i].src_agent = slots[idx].addr;
            slots[idx].update(t, 0);
            stats.src_repaired += 1;
        }
    }
    for &i in &dst_orphans {
        let (t, instid) = (events[i].time, events[i].dst_instid);
        if let Some(idx) = pick_candidate(&slots, &by_instid, instid, t) {
            events[i].dst_agent = slots[idx].addr;
            slots[idx].update(t, 0);
            stats.dst_repaired += 1;
        }
    }
    stats
}

/// `candidates.FirstOrDefault(x => x.InAwareTimes(c.Time - 300) ||
/// x.InAwareTimes(c.Time + 300))` (`EvtcParser.cs:1225` / `:1237`).
/// `saturating_sub` stands in for C#'s signed `c.Time - 300`: both make the
/// low probe fall before every aware window when the row is in the log's
/// first 300ms.
fn pick_candidate(
    slots: &[Slot],
    by_instid: &BTreeMap<u16, Vec<usize>>,
    instid: u16,
    t: u64,
) -> Option<usize> {
    let cands = by_instid.get(&instid)?;
    cands
        .iter()
        .copied()
        .find(|&i| {
            slots[i].in_aware_times(t.saturating_sub(AWARE_SLACK_MS))
                || slots[i].in_aware_times(t + AWARE_SLACK_MS)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::sc;

    fn agent(addr: u64) -> RawAgent {
        RawAgent {
            addr, prof: 4, is_elite: 0xffff_ffff, toughness: 0, concentration: 0,
            healing: 0, hitbox_width: 0, condition: 0, hitbox_height: 0,
            name_raw: b"Pet\0".to_vec(),
        }
    }

    /// A plain `CBTS_COMBAT` (statechange 0) strike row.
    fn hit(time: u64, src: u64, src_instid: u16, dst: u64, dst_instid: u16) -> RawEvent {
        RawEvent {
            time, src_agent: src, dst_agent: dst, value: 100, buff_dmg: 0, overstack: 0,
            skillid: 1, src_instid, dst_instid, src_master_instid: 0, dst_master_instid: 0,
            iff: 0, buff: 0, result: 0, is_activation: 0, is_buffremove: 0, is_ninety: 0,
            is_fifty: 0, is_moving: 0, is_statechange: sc::NONE, is_flanking: 0,
            is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    #[test]
    fn repairs_an_eligible_src_orphan() {
        // Agent 0xAA is aware over [1000, 2000] with instid 7; the orphan at
        // t=1500 carries instid 7 and a zeroed src.
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(1500, 0, 7, 0xBB, 9),
            hit(2000, 0xAA, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st, RepairStats { src_orphans: 1, dst_orphans: 0, src_repaired: 1, dst_repaired: 0 });
        assert_eq!(ev[1].src_agent, 0xAA);
    }

    #[test]
    fn repairs_an_eligible_dst_orphan() {
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(1500, 0xAA, 7, 0, 9),
            hit(2000, 0xAA, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st, RepairStats { src_orphans: 0, dst_orphans: 1, src_repaired: 0, dst_repaired: 1 });
        assert_eq!(ev[1].dst_agent, 0xBB);
    }

    #[test]
    fn rejects_an_orphan_outside_the_aware_window_plus_slack() {
        // 0xAA is aware only at [1000, 1000]. An orphan at t = 1400 probes
        // 1100 and 1700 -- neither is inside [1000, 1000], so no repair.
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(1400, 0, 7, 0xBB, 9)];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!((st.src_orphans, st.src_repaired), (1, 0));
        assert_eq!(ev[1].src_agent, 0, "unrepairable orphan keeps its zero addr");
    }

    #[test]
    fn accepts_an_orphan_exactly_at_the_slack_boundary() {
        // Same shape, at t = 1300: the low probe lands exactly on 1000, and
        // `InAwareTimes` is inclusive on both bounds.
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(1300, 0, 7, 0xBB, 9)];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[1].src_agent, 0xAA);
    }

    #[test]
    fn repairs_an_orphan_that_precedes_the_agents_first_row() {
        // The high probe (t + 300 = 1100) covers an agent that only becomes
        // aware at 1000 -- GW2EI's rule is symmetric, unlike M16's
        // latest-before-t `NonZeroAddrIndex` fallback.
        let mut ev = vec![
            hit(800, 0, 7, 0xBB, 9),
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(2000, 0xAA, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[0].src_agent, 0xAA);
    }

    #[test]
    fn rejects_candidate_whose_window_is_inside_the_probes() {
        // 0xAA is aware over [1450, 1550]; an orphan at 1500 probes 1200 and
        // 1800, BOTH outside that window. GW2EI's literal `InAwareTimes(t-300)
        // || InAwareTimes(t+300)` rejects it -- a widened-window reading would
        // not. This is a fidelity test, not a preference.
        let mut ev = vec![
            hit(1450, 0xAA, 7, 0xBB, 9),
            hit(1500, 0, 7, 0xBB, 9),
            hit(1550, 0xAA, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!((st.src_orphans, st.src_repaired), (1, 0));
        assert_eq!(ev[1].src_agent, 0);
    }

    #[test]
    fn picks_the_right_candidate_when_an_instid_is_reused() {
        // instid 7 belongs to 0xAA over [1000, 2000] and, after that agent
        // despawns, to 0xCC over [9000, 10000]. Two orphans, one in each era.
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(2000, 0xAA, 7, 0xBB, 9),
            hit(1100, 0, 7, 0xBB, 9),
            hit(9000, 0xCC, 7, 0xBB, 9),
            hit(10000, 0xCC, 7, 0xBB, 9),
            hit(9100, 0, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB), agent(0xCC)], &mut ev);
        assert_eq!(st.src_repaired, 2);
        assert_eq!(ev[2].src_agent, 0xAA);
        assert_eq!(ev[5].src_agent, 0xCC, "the later-era orphan must not take the earlier agent");
    }

    #[test]
    fn prefers_the_earliest_first_aware_candidate_when_windows_overlap() {
        // Both 0xAA ([1000, 5000]) and 0xCC ([2000, 5000]) end up carrying
        // instid 7 and both cover t = 3000. GW2EI takes `FirstOrDefault` over
        // a `FirstAware`-sorted bucket, i.e. 0xAA.
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(2000, 0xCC, 7, 0xBB, 9),
            hit(5000, 0xAA, 7, 0xBB, 9),
            hit(5000, 0xCC, 7, 0xBB, 9),
            hit(3000, 0, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB), agent(0xCC)], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[4].src_agent, 0xAA);
    }

    #[test]
    fn leaves_rows_alone_when_the_instid_is_unknown() {
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(1100, 0, 42, 0xBB, 9)];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!((st.src_orphans, st.src_repaired), (1, 0));
        assert_eq!(ev[1].src_agent, 0);
    }

    #[test]
    fn ignores_zero_instid_rows() {
        // addr 0 + instid 0 is not an orphan: there is nothing to key on.
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(1100, 0, 0, 0, 0)];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st, RepairStats::default());
        assert_eq!(ev[1].src_agent, 0);
    }

    #[test]
    fn does_not_touch_statechange_payload_slots() {
        // `MaxHealthUpdate` (12) is src-agent-bearing but NOT dst-agent-
        // bearing: its `dst_agent` is the max-health payload. A zero there
        // with a live `dst_instid` must never be rewritten.
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(2000, 0xAA, 7, 0xBB, 9)];
        let mut mh = hit(1100, 0xAA, 7, 0, 9);
        mh.is_statechange = sc::MAX_HEALTH;
        ev.push(mh);
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st, RepairStats::default());
        assert_eq!(ev[2].dst_agent, 0);

        // ... and the same row's src side IS eligible.
        let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(2000, 0xAA, 7, 0xBB, 9)];
        let mut mh = hit(1100, 0, 7, 0, 0);
        mh.is_statechange = sc::MAX_HEALTH;
        ev.push(mh);
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[2].src_agent, 0xAA);
    }

    #[test]
    fn ignores_extension_rows_on_both_sides() {
        // `Extension` (40) / `ExtensionCombat` (49) are absent from GW2EI's
        // parameterless `SrcIsAgent`/`DstIsAgent`, so their addr fields (which
        // extensions define freely) are never rewritten.
        for code in [sc::EXTENSION, sc::EXTENSION_COMBAT] {
            let mut ev = vec![hit(1000, 0xAA, 7, 0xBB, 9)];
            let mut x = hit(1100, 0, 7, 0, 9);
            x.is_statechange = code;
            ev.push(x);
            let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
            assert_eq!(st, RepairStats::default(), "statechange {code}");
            assert_eq!((ev[1].src_agent, ev[1].dst_agent), (0, 0));
        }
    }

    #[test]
    fn a_repaired_row_widens_the_candidates_window_for_later_orphans() {
        // GW2EI's `UpdateAgentData(candidate, c.Time, 0, false)` after a
        // successful repair grows the window, so a chain of orphans 300ms
        // apart walks forward past the agent's last real row.
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(1300, 0, 7, 0xBB, 9),
            hit(1600, 0, 7, 0xBB, 9),
            hit(1900, 0, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st.src_repaired, 3);
        assert!(ev[1..].iter().all(|e| e.src_agent == 0xAA));
    }

    #[test]
    fn handles_orphans_in_the_logs_first_300ms_without_underflow() {
        let mut ev = vec![hit(0, 0xAA, 7, 0xBB, 9), hit(100, 0, 7, 0xBB, 9)];
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[1].src_agent, 0xAA);
    }

    #[test]
    fn synthesizes_agents_absent_from_the_evtc_table() {
        // GW2EI's `allAgentValues` step: an address that only ever appears in
        // combat rows still becomes a candidate agent.
        let mut ev = vec![
            hit(1000, 0xAA, 7, 0xBB, 9),
            hit(2000, 0xAA, 7, 0xBB, 9),
            hit(1100, 0, 7, 0xBB, 9),
        ];
        let st = repair_orphaned_agents(&[], &mut ev);
        assert_eq!(st.src_repaired, 1);
        assert_eq!(ev[2].src_agent, 0xAA);
    }

    #[test]
    fn is_a_no_op_on_a_stream_with_no_orphans() {
        let before = vec![hit(1000, 0xAA, 7, 0xBB, 9), hit(1100, 0xBB, 9, 0xAA, 7)];
        let mut after = before.clone();
        let st = repair_orphaned_agents(&[agent(0xAA), agent(0xBB)], &mut after);
        assert_eq!(st, RepairStats::default());
        for (a, b) in before.iter().zip(&after) {
            assert_eq!((a.src_agent, a.dst_agent), (b.src_agent, b.dst_agent));
        }
    }
}
