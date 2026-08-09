//! Support stats: condition cleanses, boon strips, resurrects (M3, Task 3).
//!
//! Verified against GW2EI source (`baaron4/GW2-Elite-Insights-Parser`,
//! `master` as of 2026-08-08):
//! - `GW2EIEvtcParser/EIData/Statistics/SupportStatistics.cs` (the
//!   `condiCleanse`/`condiCleanseSelf`/`boonStrips`/`resurrects` fields'
//!   arbiter -- this is what feeds `support[0]` in dps.report/EI JSON, the
//!   golden fixture's source).
//! - `GW2EIEvtcParser/EIData/Statistics/SupportPerAllyStatistics.cs` (the
//!   per-target removal bucketing `SupportStatistics` sums over).
//! - `GW2EIEvtcParser/EIData/Actors/ActorsHelper/SingleActorBuffsHelper.cs`
//!   (`GetBuffRemoveAllDataBySrc` -- confirms only `BuffRemoveAllEvent`s,
//!   i.e. `is_buffremove == ALL`, feed these stats).
//! - `GW2EIEvtcParser/ParsedData/CombatEvents/CombatEventFactory.cs`
//!   (`CreateCastEvents` -- the resurrect-cast pairing state machine).

use crate::analysis::buffs::BOON_IDS;
use crate::analysis::PlayerMetrics;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// The 14 condition/debuff skill ids GW2EI classifies under
/// `Buff.BuffClassification.Condition`.
///
/// **MCONDCAT Task 1: this is no longer a local table.** The ids now live in
/// exactly one place, `analysis::condition_catalog`, which is ALSO the
/// arbiter for `hit_stats`'/`defenses`' condition-vs-power damage bucketing
/// (`SkillEvent.ConditionDamageBased`). Both consumers ask GW2EI the same
/// question -- "is this id a `BuffClassification.Condition` buff?" -- so
/// they must not be able to drift apart. Re-exported here (rather than
/// deleted) to keep `support::CONDITION_IDS`/`support::BLEEDING`/... working
/// unchanged for existing callers.
///
/// The previous doc comment on this block cited the wrong list
/// (`CommonBuffs.Boons`); the correct site is `CommonBuffs.Conditions`
/// (`GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:34-52`) -- corrected, with
/// the full exhaustive-scan provenance, in `condition_catalog`'s module doc.
pub use crate::analysis::condition_catalog::{
    BLEEDING, BLIND, BURNING, CHILLED, CONFUSION, CRIPPLED, FEAR, IMMOBILE, POISON, SLOW, TAUNT,
    TORMENT, VULNERABILITY, WEAKNESS,
};

/// Alias of `condition_catalog::CONDITION_SKILL_IDS` -- see above.
pub use crate::analysis::condition_catalog::CONDITION_SKILL_IDS as CONDITION_IDS;

/// `SkillIDs.Resurrect` (`GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs`) --
/// the "Res" skill every profession has on downed/defeated allies. Distinct
/// from `SkillIDs.Resurrection = 848` (a different, unused-here skill) and
/// `SkillIDs.Resurrect2 = 1006` (verified NOT read by `GetReses` in
/// `SupportAllStatistics.cs` -- only `SkillIDs.Resurrect` is).
pub const RESURRECT_SKILL_ID: u32 = 1066;

/// `is_activation` byte values that start a skill-cast animation (arcdps
/// `enum cbtanimation`: `ACTV_START_DEFUNC = 1`, `ACTV_QUICKNESS_DEFUNC =
/// 2`), verified against GW2EI's `ArcDPSEnums.Activation` (`Normal = 1,
/// Quickness = 2`) and `CombatItem.IsStartCastEvent()`.
const ACTIVATION_START: [u8; 2] = [1, 2];

/// One squad player's support-stat counts (M3, Task 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupportMetrics {
    /// Condition removed from ANOTHER squad player (friendly, not self).
    /// Verified against `SupportStatistics.cs`: `ConditionCleanseCount` sums
    /// `FriendlyRemovals` for every OTHER player in `log.PlayerList`
    /// (excludes the actor's own self-removals, tracked separately below).
    pub cleanses: u32,
    /// Condition removed from the SAME account that removed it (a genuine
    /// self-cleanse). Verified against `SupportStatistics.cs`:
    /// `ConditionCleanseSelfCount` sums `actor.GetSupportStats(actor, ...)
    /// .FriendlyRemovals` -- i.e. the removal's `To` (owner) is the actor
    /// itself. Folded by account representative (a relogged account's
    /// self-cleanse must still count as self, not cross-account), per the
    /// Task 3 brief.
    pub cleanses_self: u32,
    /// Boon removed from an enemy. Verified against `SupportStatistics.cs`:
    /// `BoonStripCount` sums `FoeRemovals` (recipient IFF == Foe) +
    /// `UnknownRemovals` (recipient IFF == Unknown) per boon --
    /// `BoonStripDownContribution` (a subset of `FoeRemovals`, further
    /// restricted to enemies about to go from 90%+ HP to downed) is a
    /// SEPARATE stat this task doesn't need and is NOT double-added into
    /// `BoonStripCount`. This project classifies "foe" by encounter
    /// `enemies` set membership (per the Task 3 brief) rather than the raw
    /// event's own `iff` byte (which is what GW2EI's `ToFoe`/`ToUnknown`
    /// read) -- calibrated exact against the golden fixture, so the
    /// `Unknown`-bucket recipients (if any exist in this fixture) are
    /// already-known `enemies` members here, not a separate case to model.
    pub strips: u32,
    /// Resurrect-skill (id 1066) cast attempts. Verified against
    /// `SupportAllStatistics.GetReses`: counts every `CastEvent` whose
    /// `SkillID == SkillIDs.Resurrect`, with NO filter on cast completion
    /// status (an interrupted/cancelled resurrect attempt still counts --
    /// `AnimatedCastEvent`'s `SetAcceleration` explicitly skips setting
    /// `Status` at all for this skill id: `if (SkillID != SkillIDs.
    /// Resurrect) { switch (endItem.IsActivation) { ... sets Status ... } }`
    /// -- so `GetReses` never had a completion status to filter on even if
    /// it wanted to). The only exclusion `CombatEventFactory.CreateCastEvents`
    /// applies is `ActualDuration <= 1`ms (near-instant animations, e.g.
    /// double-fired activation bytes) -- not modeled here as a separate
    /// filter since every real resurrect-cast start observed in the golden
    /// fixture has `ExpectedDuration` (`value`/`buff_dmg`) of 700-800ms,
    /// nowhere near that threshold. See `apply`'s doc comment for the
    /// simplification from full start/end `CastEvent` pairing down to
    /// counting start-cast events directly (verified count-equivalent on
    /// this fixture: 6 starts, 6 `CastEvent`s -- 4 properly closed by a
    /// matching end, 2 left "dangling" at cast-attempt boundaries that
    /// GW2EI's pairing state machine still credits as one `CastEvent` each).
    pub resurrects: u32,
}

/// Computes `SupportMetrics` for every squad player in `players` (matched by
/// `PlayerMetrics::agent_addr`) and adds the result into each entry's
/// `support` field. Mirrors `cc::apply_cc`'s shape (mutates `players` in
/// place via an `agent_addr -> index` map) rather than returning a parallel
/// `BTreeMap` (unlike `buffs::simulate_boons`/`boon_uptime`, which need a
/// `(agent, buff id)` compound key) -- `support` is a single per-player
/// value, same shape as `cc_applied`/`stun_breaks` etc. already on
/// `PlayerMetrics`.
///
/// **Cleanse/strip event selection (verified against `SupportPerAllyStatistics.cs`
/// / `SingleActorBuffsHelper.GetBuffRemoveAllDataBySrc`):** ONLY
/// `is_buffremove == ALL` removal events feed these stats -- `BuffRemoveAllEvent`
/// is the sole type `GetBuffRemoveAllDataBySrc`/`GetBuffRemoveAllEventsByByID`
/// read from. `SINGLE` removals (which the M3 Task 1/2 boon-uptime stack
/// simulator DOES consume, for its own different purpose of tracking
/// individual-stack expiry) are correctly excluded here -- confirmed by the
/// class name and every accessor in that source file operating exclusively
/// on `BuffRemoveAllEvent`, never `BuffRemoveSingleEvent`. `events::
/// BuffEventKind::RemoveAll` (this project's own equivalent, from Task 1) is
/// the exact analogue.
///
/// **Field-role reuse, WITH ONE DELIBERATE DEVIATION:** removal events invert
/// apply's src/dst roles (`owner = src_agent`, `remover = dst_agent`, per
/// Task 1's verified `AbstractBuffRemoveEvent` semantics -- same as
/// `events::extract_buff_events`'s `RemoveAll` extraction). This function does
/// **NOT**, however, reuse `extract_buff_events`'s `agent` field (which
/// pet-master-resolves the remover through the shared `damage::
/// InstidRegistry`, exactly like `damage`/`cc`'s pet-credit) -- it reads
/// `dst_agent` RAW instead. This is a load-bearing, ground-truth-verified
/// deviation from every OTHER pet-crediting pass in this codebase (and from
/// the Task 3 brief's suggestion to reuse it here): GW2EI's own
/// `SingleActorBuffsHelper.InitBuffRemoveAllByByIDDict` groups
/// `BuffRemoveAllEvent`s by their raw, UN-resolved `.By` AgentItem
/// (`_buffDataBySrc = buffEvents.GroupBy(x => x.By)`, `AbstractBuffRemoveEvent`
/// ctor: `By = agentData.GetAgent(evtcItem.DstAgent, ...)` -- no
/// `.GetFinalMaster()`/englobing-owner resolution at all), NOT by
/// `CreditedBy` (`By.GetFinalMaster()`, the property `damage`/`cc`'s own
/// pet-credit *does* use for buff-APPLY events' accelerator). Verified
/// empirically, not just by reading source: decompiled the exact deployed
/// GW2EIEvtcParser.dll (via `ilspycmd`) and cross-checked against a live
/// C# harness that re-parses this project's own local `wvw-small.zevtc`
/// fixture and calls the real `SingleActor.GetSupportStats`/
/// `CombatData.GetBuffRemoveAllDataBySrc` directly -- for the golden
/// fixture's Druid (raw addr 4568), the real algorithm's per-target
/// `FriendlyRemovals` sum is exactly 56 (matching golden's `condiCleanse:
/// 56`), while this project's raw condition-`ALL`-removal events with
/// `dst_agent == 4568` number exactly 69 (matching too) -- but pet-crediting
/// 4568's Ranger pet's OWN 28 separate condition-`ALL`-removals (a real,
/// distinct in-game mechanic on this profession) up to the owner inflated
/// this project's original implementation to 97, an over-count the harness
/// definitively traced to exactly this cause (28 = 97 - 69). A player's pet
/// performing a cleanse/strip is consequently NOT credited to anyone in this
/// project's `SupportMetrics` (mirrors GW2EI: pets have no `players[]` JSON
/// row of their own to receive that credit either). This is DIFFERENT from
/// `damage`/`cc`, which correctly pet-credit their own stats -- confirmed
/// this isn't a blanket "never pet-credit" rule but a per-statistic one,
/// specific to how GW2EI's own accelerator happens to be keyed for this one.
///
/// **A second, distinct ground-truth finding: `Encounter::players` contains
/// real "Non Squad Player" friendlies, and GW2EI credits them ASYMMETRICALLY.**
/// Investigating a residual over-count on a different squad member
/// ("R O O M B A", raw addr 4587, golden `condiCleanse: 50`) surfaced FOUR
/// raw addrs (9512/9513/9515/9517, `Character` == "Evoker"/"Specter"/
/// "Berserker"/"Troubadour" -- literally their own profession/spec name)
/// that `model::resolve` folds into `Encounter.players` (hence `squad`)
/// alongside the 37 recorded squad members. These are NOT clones/pets:
/// cross-checked directly against the golden fixture and found FOUR
/// matching rows with `account: "Non Squad Player <N>"` and the SAME FOUR
/// profession names -- GW2EI's own WvW JSON export label for "a real,
/// friendly human character present in the encounter but outside the
/// recorded squad" (this project's fixture `_note` already documented this
/// GW2EI concept for `boons`/M2 -- Task 3 is the first pass to actually
/// interact with it). Crucially, TWO of these four have real nonzero
/// `condiCleanse` in golden (`"Non Squad Player 3"`: 15, `"Non Squad Player
/// 11"`: 10) -- these agents DO perform and get credited for real cleanses,
/// just not as a cross-cleanse RECIPIENT. Verified via the same C# harness:
/// `GetToAllySupportStats` (which produces each exported `players[]` row,
/// including these four) is keyed by the raw `AgentItem` and never checks
/// `log.PlayerList` membership -- so a non-squad friendly's OWN removals
/// (as remover, or self-cleansing themselves) DO get credited to them. Only
/// `ConditionCleanseCount`'s CROSS-player loop (`foreach (Player p in log.
/// PlayerList)`) is `PlayerList`-restricted -- and `log.PlayerList` (37
/// entries, confirmed via the harness: none of the four "Non Squad Player"
/// signatures match any of the 37) excludes non-squad friendlies as
/// RECIPIENTS. So: remover eligibility is deliberately left BROAD (the
/// ordinary `idx` lookup, same as every other `squad` member); only the
/// CROSS-cleanse recipient check below is restricted to a STRICTER
/// `real_players` set (self-cleanses need no such restriction either, since
/// `ConditionCleanseSelfCount` comes from `to = actor` directly, likewise
/// never consulting `PlayerList`). The signal used to build `real_players`
/// without touching `Encounter`/`squad` themselves (every other
/// already-calibrated metric depends on them as-is): `Player.subgroup == 0`
/// -- arcdps never assigns a squad subgroup slot (1-8) to a non-squad
/// friendly, and unlike `account` (initially tried, see below) this survives
/// anonymization. **`account`-emptiness does NOT work as this signal**: the
/// committed `wvw-small.anon.zevtc`'s anonymization script synthesizes a
/// non-empty `"Anon<N>.NNNN"` account for EVERY player-shaped agent
/// uniformly, INCLUDING these four (verified directly: decoded both fixtures
/// at the same raw agent-table index and found `account: ""` on the real
/// log but `account: ":Anon145.6365"` etc. on the anonymized one) -- an
/// `account`-based `real_players` filter passed against the gitignored
/// local raw fixture but silently regressed the squad-wide `cleanses` sum to
/// 832 (not 801) against the COMMITTED anonymized fixture CI actually runs.
/// `subgroup` has no such problem: confirmed identical (`0` for these four,
/// `1-8` for every other squad member) on both fixtures.
///
/// **Result: exact.** With both the pet-master exclusion and this
/// asymmetric `real_players` rule in place, every joinable account's
/// `cleanses`/`cleanses_self` matches golden exactly (including the two
/// nonzero "Non Squad Player" rows themselves, credited correctly as
/// REMOVER), and the squad-wide sums match golden's 801/97/437/6 exactly --
/// see `tests/support_golden.rs`.
/// Era dispatch (M4 Task 2): on arcdps builds `>= 20260501`
/// (`raw.header.is_post_buff_rework()`), routes both the cleanse/strip
/// removal scan AND the resurrect-cast scan to their post-era siblings
/// below -- mirrors `events::extract_buff_events`'s dispatch (M4 Task 1).
/// The PRE-ERA BODY BELOW IS BYTE-IDENTICAL TO BEFORE M4 (calibrated; do not
/// edit its logic) -- `apply_post_era` is a fully separate function that
/// reproduces the identical `SupportMetrics` credit for a post-era log
/// carrying the same logical removal/resurrect-cast sequence (see the
/// `era_equivalence` tests at the bottom of this module).
pub fn apply(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    enc: &Encounter,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) {
    if raw.header.is_post_buff_rework() {
        return apply_post_era(players, raw, enc, enemies, addr_to_rep);
    }

    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    let boon_id_set: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let condition_id_set: BTreeSet<u32> = CONDITION_IDS.iter().copied().collect();

    // `real_players`: squad addrs with a genuine squad subgroup (`1`-`8`) --
    // excludes the "Non Squad Player" friendlies `Encounter.players` also
    // contains (`subgroup == 0`, see the big doc comment above for the full
    // ground-truth trace, including why `account`-emptiness was tried first
    // and rejected). Used ONLY to restrict cross-cleanse RECIPIENT
    // eligibility below -- remover eligibility deliberately stays the plain
    // `idx` lookup (broader), and this set is never used for strips or
    // resurrects. Scoped narrowly to this function -- `squad`/`enc.players`
    // themselves are untouched (every other already-calibrated metric
    // depends on them as-is).
    let real_players: BTreeSet<u64> = enc
        .players
        .iter()
        .filter(|p| p.subgroup != 0)
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();

    for e in &raw.events {
        // Removal predicate (verified `CombatItem.IsBuffRemoveEvent`, Task 1):
        // an ordinary `is_statechange == 0` combat event, no activation flag,
        // `is_buffremove != NONE`. Only `ALL` feeds support stats (see doc
        // comment) -- `SINGLE` is intentionally excluded here.
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != crate::evtc::buff_remove::ALL {
            continue;
        }
        let is_condition = condition_id_set.contains(&e.skillid);
        let is_boon = boon_id_set.contains(&e.skillid);
        if !is_condition && !is_boon {
            continue;
        }
        // Role inversion (Task 1): owner = src_agent, remover = dst_agent --
        // RAW, deliberately NOT pet-master-resolved (see doc comment above).
        let owner = e.src_agent;
        let remover = e.dst_agent;
        // Remover eligibility is intentionally BROAD (any `enc.players`
        // entry, including the "Non Squad Player" agents `real_players`
        // excludes) -- verified: GW2EI computes a `support[0]` row (as
        // REMOVER) for every exported `players[]` entry via `GetToAllySupportStats`,
        // which is keyed by the raw `AgentItem` and never checks
        // `log.PlayerList` membership at all. Only the CROSS-player cleanse
        // recipient check below is `log.PlayerList`-restricted (see doc
        // comment's "Non Squad Player" finding).
        let Some(&i) = idx.get(&rep(remover)) else { continue };
        if is_condition {
            if rep(owner) == rep(remover) {
                // Self-cleanse: verified against `SupportAllStatistics`'s
                // `to=actor` self-check, which never consults `log.
                // PlayerList` either (the target IS the actor) -- no
                // additional recipient-identity restriction needed.
                players[i].support.cleanses_self += 1;
            } else {
                // Cross-cleanse: recipient (owner) must be a GENUINE squad
                // player (`real_players`, not just `squad`) -- verified:
                // `ConditionCleanseCount`'s cross-player loop is `foreach
                // (Player p in log.PlayerList)`, which excludes the "Non
                // Squad Player" friendlies (see doc comment).
                if !real_players.contains(&owner) {
                    continue;
                }
                players[i].support.cleanses += 1;
            }
        } else {
            // Strip: recipient (owner) must be an enemy.
            if !enemies.contains(&owner) {
                continue;
            }
            players[i].support.strips += 1;
        }
    }

    // Resurrects: start-cast events (`is_activation` == Normal(1) or
    // Quickness(2)) of the Resurrect skill, by a squad player. See
    // `SupportMetrics::resurrects` doc comment for the verified GW2EI
    // `CastEvent` semantics this reproduces (count-equivalent for this
    // fixture, without needing full start/end animation pairing).
    for e in &raw.events {
        if e.is_statechange == 0
            && e.skillid == RESURRECT_SKILL_ID
            && ACTIVATION_START.contains(&e.is_activation)
        {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].support.resurrects += 1;
            }
        }
    }
}

/// Post-era (arcdps `>= 20260501`) twin of `apply`'s pre-era body above --
/// dispatches the cleanse/strip removal scan on the dedicated
/// `sc::BUFF_REMOVE_ALL` statechange instead of the `is_statechange == 0
/// && is_buffremove == ALL` combat-event shape, and the resurrect-cast scan
/// on `sc::ANIMATION_START` instead of `is_activation`. Every OTHER
/// decision -- role assignment, pet-exclusion (by construction: still reads
/// RAW, unresolved agents), self-cleanse/cross-cleanse/strip eligibility,
/// `real_players` recipient restriction -- is IDENTICAL to the pre-era body
/// (see the `era_equivalence` tests below for the same-output assertion).
///
/// **Removal sourcing (verified against `sc::BUFF_REMOVE_ALL`'s doc
/// comment, `crate::evtc::event`)**: post-era, `BuffRemoveAllEvent`
/// construction is UNCONDITIONAL on this statechange (no `is_buffremove`
/// check -- unlike `BUFF_REMOVE_SINGLE`, which still disambiguates Single
/// vs Manual by that byte post-era) and reuses the SAME
/// `AbstractBuffRemoveEvent(CombatItem evtcItem, ...)` ctor as the pre-era
/// shape (`By = agentData.GetAgent(evtcItem.DstAgent, ...)`, `To =
/// agentData.GetAgent(evtcItem.SrcAgent, ...)`, no `.GetFinalMaster()`
/// resolution at all) -- so `owner = src_agent`/`remover = dst_agent`, BOTH
/// READ RAW (un-resolved), is exactly the same deliberate deviation this
/// module's own doc comment above already establishes for the pre-era
/// shape (GW2EI groups `BuffRemoveAllEvent`s by the raw, unresolved `.By`
/// AgentItem, not `CreditedBy`/pet-master-resolved) -- confirmed by
/// `events::extract_buff_events_post_era`'s OWN post-era `RemoveAll` branch
/// using the identical `owner: e.src_agent` / raw `e.dst_agent` field
/// reads (M4 Task 1), which this function's removal scan mirrors minus
/// that function's `agent` field's master-resolution (this module never
/// wanted that -- see the big pet-exclusion doc comment above `apply`).
///
/// **Resurrect sourcing (verified against `sc::ANIMATION_START`'s doc
/// comment)**: post-era cast-start detection moves to the DEDICATED
/// `AnimationStart` statechange (GW2EI's `CombatItem.IsStartCastEvent()`,
/// gated on the EARLIER `ArcDPSBuilds.AnimationAsStateChanges = 20260430`
/// threshold -- subsumed by this project's single `is_post_buff_rework`
/// `20260501` gate, see `sc::ANIMATION_START`'s doc comment for the full
/// version-split explanation and its one known gap). `src_agent` stays the
/// caster (GW2EI groups cast events by the raw `combatItem.SrcAgent`,
/// unchanged from the pre-era shape), so the credit logic (raw `src_agent`,
/// no master resolution -- a squad player's own resurrect-cast, never
/// pet-credited, matching GW2EI: `AnimatedCastEvent`s are never folded into
/// an owner the way damage/CC are) is identical, only the SELECTION
/// predicate changes.
fn apply_post_era(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    enc: &Encounter,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) {
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    let boon_id_set: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let condition_id_set: BTreeSet<u32> = CONDITION_IDS.iter().copied().collect();

    let real_players: BTreeSet<u64> = enc
        .players
        .iter()
        .filter(|p| p.subgroup != 0)
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();

    for e in &raw.events {
        // Post-era removal predicate: the dedicated `sc::BUFF_REMOVE_ALL`
        // statechange, unconditional (no `is_buffremove`/`is_activation`
        // gate -- see this fn's doc comment).
        if e.is_statechange != crate::evtc::sc::BUFF_REMOVE_ALL {
            continue;
        }
        let is_condition = condition_id_set.contains(&e.skillid);
        let is_boon = boon_id_set.contains(&e.skillid);
        if !is_condition && !is_boon {
            continue;
        }
        // Role inversion (identical to pre-era, see this fn's doc comment):
        // owner = src_agent, remover = dst_agent, BOTH RAW.
        let owner = e.src_agent;
        let remover = e.dst_agent;
        let Some(&i) = idx.get(&rep(remover)) else { continue };
        if is_condition {
            if rep(owner) == rep(remover) {
                players[i].support.cleanses_self += 1;
            } else {
                if !real_players.contains(&owner) {
                    continue;
                }
                players[i].support.cleanses += 1;
            }
        } else {
            if !enemies.contains(&owner) {
                continue;
            }
            players[i].support.strips += 1;
        }
    }

    // Resurrects (post-era): `sc::ANIMATION_START` cast-start rows for the
    // Resurrect skill, credited by raw `src_agent` (see this fn's doc
    // comment).
    for e in &raw.events {
        if e.is_statechange == crate::evtc::sc::ANIMATION_START && e.skillid == RESURRECT_SKILL_ID {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].support.resurrects += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{buff_remove, RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn player(addr: u64) -> PlayerMetrics {
        PlayerMetrics { agent_addr: addr, ..Default::default() }
    }

    /// A genuine `Encounter::players` entry for `addr`, with a real squad
    /// subgroup (1-8) -- the shape `real_players` in `apply` requires.
    fn enc_player(addr: u64) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup: 1,
            in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![addr],
        }
    }

    /// A "Non Squad Player" friendly `Encounter::players` entry (`subgroup:
    /// 0` -- arcdps never assigns one) -- reproduces the exact real-fixture
    /// finding `apply`'s doc comment cites (ground-truth-verified via the
    /// real GW2EIEvtcParser.dll: absent from `log.PlayerList`, so NOT a
    /// valid cross-cleanse RECIPIENT, but still eligible as a REMOVER/
    /// self-cleanser -- see `apply`'s doc comment for the asymmetry).
    fn enc_fake_pet_player(addr: u64) -> Player {
        Player {
            agent_addr: addr, account: "".into(), character: "Evoker".into(),
            profession: "Elementalist".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 0, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![addr],
        }
    }

    fn enc_from(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "20260114".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], tick_rate: None,
        }
    }

    /// A condition removed by squad player 1 from squad player 2 (a
    /// different account) is a plain cleanse, not a self-cleanse.
    #[test]
    fn condition_remove_all_to_other_squad_player_is_cleanse() {
        let mut e = base_event();
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 2; // owner (had Bleeding removed)
        e.dst_agent = 1; // remover
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 1);
        assert_eq!(players[0].support.cleanses_self, 0);
        assert_eq!(players[1].support.cleanses, 0);
    }

    /// A condition removed by squad player 1 from THEMSELVES is a
    /// self-cleanse, not counted in the plain `cleanses` total.
    #[test]
    fn condition_remove_all_from_self_is_self_cleanse() {
        let mut e = base_event();
        e.skillid = POISON;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 1; // owner
        e.dst_agent = 1; // remover -- same agent
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 0);
        assert_eq!(players[0].support.cleanses_self, 1);
    }

    /// Self-cleanse detection folds by account representative: a relogged
    /// account's post-relog addr must still be recognized as "self" against
    /// its pre-relog addr.
    #[test]
    fn self_cleanse_folds_across_relog_addrs() {
        let mut e = base_event();
        e.skillid = POISON;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 1; // pre-relog addr (owner)
        e.dst_agent = 2; // post-relog addr (remover) -- same account
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)]; // representative addr 1
        let mut rep_player = enc_player(1);
        rep_player.agent_addrs = vec![1, 2]; // relog: both addrs, one account
        let enc = enc_from(vec![rep_player]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses_self, 1);
        assert_eq!(players[0].support.cleanses, 0);
    }

    /// A condition removed from an enemy is neither a cleanse nor a strip
    /// (conditions aren't stripped -- only boons are).
    #[test]
    fn condition_remove_all_to_enemy_not_counted() {
        let mut e = base_event();
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 9; // enemy owner
        e.dst_agent = 1; // squad remover
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = [9].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 0);
        assert_eq!(players[0].support.cleanses_self, 0);
        assert_eq!(players[0].support.strips, 0);
    }

    /// A boon removed by a squad player from an enemy is a strip.
    #[test]
    fn boon_remove_all_from_enemy_is_strip() {
        let mut e = base_event();
        e.skillid = crate::analysis::buffs::MIGHT;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 9; // enemy owner
        e.dst_agent = 1; // squad remover
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = [9].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.strips, 1);
    }

    /// A boon removed from a friendly (squad) recipient is not a strip --
    /// GW2EI's `BoonStripCount` only sums `FoeRemovals`/`UnknownRemovals`,
    /// never `FriendlyRemovals`.
    #[test]
    fn boon_remove_all_from_squad_not_a_strip() {
        let mut e = base_event();
        e.skillid = crate::analysis::buffs::MIGHT;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 2; // squad owner
        e.dst_agent = 1; // squad remover
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.strips, 0);
    }

    /// `SINGLE` removals must NOT feed cleanse/strip credit -- only `ALL`
    /// does (verified against `GetBuffRemoveAllDataBySrc`, see module docs).
    #[test]
    fn single_removal_not_counted_as_cleanse_or_strip() {
        let mut cleanse = base_event();
        cleanse.skillid = BLEEDING;
        cleanse.is_buffremove = buff_remove::SINGLE;
        cleanse.value = 500;
        cleanse.src_agent = 2;
        cleanse.dst_agent = 1;
        let mut strip = base_event();
        strip.skillid = crate::analysis::buffs::MIGHT;
        strip.is_buffremove = buff_remove::SINGLE;
        strip.value = 500;
        strip.src_agent = 9;
        strip.dst_agent = 1;
        let raw = raw_from(vec![cleanse, strip]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = [9].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 0);
        assert_eq!(players[0].support.strips, 0);
    }

    /// A pet-sourced condition-cleanse (remover's raw `dst_agent` is a pet
    /// whose `dst_master_instid` points back to a squad player) must NOT be
    /// credited to the owner -- ground-truth-verified deviation from
    /// `damage::pet_credit_events`'s owner resolution, see `apply`'s doc
    /// comment (GW2EI's own `GetBuffRemoveAllDataBySrc` groups by the raw,
    /// un-resolved remover, not `CreditedBy`). Since the pet's own raw addr
    /// is never a squad player, this event must simply be dropped -- not
    /// credited to the pet's owner OR anyone else.
    #[test]
    fn pet_sourced_cleanse_not_credited_to_owner() {
        let mut seed = base_event();
        seed.src_agent = 1; // owner
        seed.src_instid = 11;
        seed.dst_agent = 9;

        let mut e = base_event();
        e.time = 100;
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 2; // owner of the removed Bleeding (a different squad player)
        e.dst_agent = 300; // pet's own addr (remover)
        e.dst_instid = 77;
        e.dst_master_instid = 11; // points back to owning player's instid

        let raw = raw_from(vec![seed, e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 0, "pet-sourced cleanse must NOT credit the owner");
        assert_eq!(players[1].support.cleanses, 0);
    }

    /// A condition removed FROM a "Non Squad Player" friendly (empty
    /// account -- see `apply`'s doc comment) by a genuine squad player must
    /// NOT be counted as a cleanse, even though the friendly is technically
    /// a member of the `squad`/`enc.players` addr set. Reproduces the exact
    /// real-fixture over-count this task's calibration traced and fixed
    /// (GW2EI's `ConditionCleanseCount` cross-player recipient loop is
    /// `log.PlayerList`-restricted, which never includes non-squad
    /// friendlies).
    #[test]
    fn condition_removed_from_non_squad_friendly_not_a_cleanse() {
        let mut e = base_event();
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 2; // the non-squad-friendly "player" (owner)
        e.dst_agent = 1; // genuine squad remover
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_fake_pet_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.cleanses, 0, "recipient is not a genuine (in-`PlayerList`) player");
    }

    /// A "Non Squad Player" friendly (empty account) acting as REMOVER,
    /// targeting a GENUINE squad player, MUST be credited -- verified
    /// against the golden fixture directly: two such agents ("Non Squad
    /// Player 3"/"11") have real nonzero `condiCleanse` (15/10) in golden,
    /// because GW2EI computes a `support[0]` row for every exported
    /// `players[]` entry via `GetToAllySupportStats`, which is keyed by the
    /// raw `AgentItem` and never checks `log.PlayerList` membership for the
    /// REMOVER side (only the cross-player RECIPIENT loop is so
    /// restricted -- see the previous test).
    #[test]
    fn non_squad_friendly_remover_targeting_real_player_is_credited() {
        let mut e = base_event();
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 1; // genuine squad owner
        e.dst_agent = 2; // the non-squad-friendly "player" (remover)
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_fake_pet_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[1].support.cleanses, 1, "remover credit doesn't require PlayerList membership");
    }

    /// A "Non Squad Player" friendly self-cleansing (remover == owner, same
    /// account) must still count as a self-cleanse -- verified: `Condition
    /// CleanseSelfCount` comes from `actor.GetSupportStats(actor, ...)`
    /// (`to = actor` directly), which never consults `log.PlayerList`
    /// either.
    #[test]
    fn non_squad_friendly_self_cleanse_is_credited() {
        let mut e = base_event();
        e.skillid = BLEEDING;
        e.is_buffremove = buff_remove::ALL;
        e.src_agent = 2; // owner == remover (self)
        e.dst_agent = 2;
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1), player(2)];
        let enc = enc_from(vec![enc_player(1), enc_fake_pet_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[1].support.cleanses_self, 1);
    }

    /// Resurrect: a start-cast event (`is_activation == 1`) of skill 1066 by
    /// a squad player counts once.
    #[test]
    fn resurrect_start_cast_counted() {
        let mut e = base_event();
        e.skillid = RESURRECT_SKILL_ID;
        e.is_activation = 1; // Normal
        e.src_agent = 1;
        e.value = 700;
        e.buff_dmg = 800;
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.resurrects, 1);
    }

    /// Only the start-cast activation bytes (1, 2) count -- the end-cast
    /// bytes (3-6) must not double the count when both a start and end
    /// event exist for the same cast.
    #[test]
    fn resurrect_end_cast_not_double_counted() {
        let mut start = base_event();
        start.time = 0;
        start.skillid = RESURRECT_SKILL_ID;
        start.is_activation = 1;
        start.src_agent = 1;
        start.value = 700;
        start.buff_dmg = 800;
        let mut end = base_event();
        end.time = 1000;
        end.skillid = RESURRECT_SKILL_ID;
        end.is_activation = 3; // Minimum (end)
        end.src_agent = 1;
        end.value = 1000;
        let raw = raw_from(vec![start, end]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.resurrects, 1);
    }

    /// A different skill id's activation events must not be counted as
    /// resurrects.
    #[test]
    fn non_resurrect_skill_not_counted() {
        let mut e = base_event();
        e.skillid = 999_999;
        e.is_activation = 1;
        e.src_agent = 1;
        let raw = raw_from(vec![e]);
        let mut players = vec![player(1)];
        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.resurrects, 0);
    }
}

/// M4 Task 2 core deliverable: for the simulator-relevant scenarios the
/// pre-era tests above cover, build a POST-era synthetic twin (same logical
/// event, decoded from `sc::BUFF_REMOVE_ALL`/`sc::ANIMATION_START` instead
/// of the `is_statechange == 0` combat-event shape) and assert `apply`
/// produces IDENTICAL `SupportMetrics` regardless of era. Mirrors
/// `buffs::events::era_equivalence`'s structure (M4 Task 1).
#[cfg(test)]
mod era_equivalence {
    use super::*;
    use crate::evtc::{buff_remove, sc, RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn raw_pre(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn raw_post(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![],
        }
    }

    fn player(addr: u64) -> PlayerMetrics {
        PlayerMetrics { agent_addr: addr, ..Default::default() }
    }

    fn enc_player(addr: u64) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup: 1,
            in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![addr],
        }
    }

    fn enc_from(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 20_000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], tick_rate: None,
        }
    }

    /// Scenario: cross-cleanse (condition removed by one squad player from
    /// another). Pre-era: `is_buffremove == ALL` combat event. Post-era:
    /// dedicated `sc::BUFF_REMOVE_ALL` statechange (unconditional -- see
    /// `apply_post_era`'s doc comment). Same owner=src_agent/remover=dst_agent
    /// roles (both RAW, not master-resolved) in both eras.
    #[test]
    fn cross_cleanse_is_era_equivalent() {
        let mut pre = base_event();
        pre.skillid = BLEEDING;
        pre.is_buffremove = buff_remove::ALL;
        pre.src_agent = 2; // owner
        pre.dst_agent = 1; // remover

        let mut post = base_event();
        post.skillid = BLEEDING;
        post.is_statechange = sc::BUFF_REMOVE_ALL;
        post.src_agent = 2;
        post.dst_agent = 1;

        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();

        let mut pre_players = vec![player(1), player(2)];
        apply(&mut pre_players, &raw_pre(vec![pre]), &enc, &enemies, &addr_to_rep);
        let mut post_players = vec![player(1), player(2)];
        apply(&mut post_players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);

        assert_eq!(pre_players[0].support, post_players[0].support);
        assert_eq!(pre_players[0].support.cleanses, 1);
        assert_eq!(pre_players[0].support.cleanses_self, 0);
    }

    /// Scenario: self-cleanse (owner == remover, same account). Same era
    /// pairing as above.
    #[test]
    fn self_cleanse_is_era_equivalent() {
        let mut pre = base_event();
        pre.skillid = POISON;
        pre.is_buffremove = buff_remove::ALL;
        pre.src_agent = 1;
        pre.dst_agent = 1;

        let mut post = base_event();
        post.skillid = POISON;
        post.is_statechange = sc::BUFF_REMOVE_ALL;
        post.src_agent = 1;
        post.dst_agent = 1;

        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();

        let mut pre_players = vec![player(1)];
        apply(&mut pre_players, &raw_pre(vec![pre]), &enc, &enemies, &addr_to_rep);
        let mut post_players = vec![player(1)];
        apply(&mut post_players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);

        assert_eq!(pre_players[0].support, post_players[0].support);
        assert_eq!(post_players[0].support.cleanses_self, 1);
    }

    /// Scenario: boon strip from an enemy. Same era pairing.
    #[test]
    fn strip_is_era_equivalent() {
        let mut pre = base_event();
        pre.skillid = crate::analysis::buffs::MIGHT;
        pre.is_buffremove = buff_remove::ALL;
        pre.src_agent = 9; // enemy owner
        pre.dst_agent = 1; // squad remover

        let mut post = base_event();
        post.skillid = crate::analysis::buffs::MIGHT;
        post.is_statechange = sc::BUFF_REMOVE_ALL;
        post.src_agent = 9;
        post.dst_agent = 1;

        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = [9].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();

        let mut pre_players = vec![player(1)];
        apply(&mut pre_players, &raw_pre(vec![pre]), &enc, &enemies, &addr_to_rep);
        let mut post_players = vec![player(1)];
        apply(&mut post_players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);

        assert_eq!(pre_players[0].support, post_players[0].support);
        assert_eq!(post_players[0].support.strips, 1);
    }

    /// Scenario: a SINGLE removal must NOT feed cleanse/strip credit in
    /// either era -- pre-era: `is_buffremove == SINGLE` on an ordinary
    /// combat event; post-era: the SAME `sc::BUFF_REMOVE_SINGLE`
    /// statechange (with `is_buffremove == SINGLE`) that `sc::
    /// BUFF_REMOVE_ALL`'s sibling ordinal carries -- `apply_post_era` only
    /// ever matches `sc::BUFF_REMOVE_ALL`, so this must produce zero credit
    /// regardless of the `is_buffremove` byte's value on that different
    /// statechange.
    #[test]
    fn single_removal_not_era_equivalent_counted() {
        let mut pre = base_event();
        pre.skillid = BLEEDING;
        pre.is_buffremove = buff_remove::SINGLE;
        pre.value = 500;
        pre.src_agent = 2;
        pre.dst_agent = 1;

        let mut post = base_event();
        post.skillid = BLEEDING;
        post.is_statechange = sc::BUFF_REMOVE_SINGLE;
        post.is_buffremove = buff_remove::SINGLE;
        post.value = 500;
        post.src_agent = 2;
        post.dst_agent = 1;

        let enc = enc_from(vec![enc_player(1), enc_player(2)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1), (2, 2)].into_iter().collect();

        let mut pre_players = vec![player(1), player(2)];
        apply(&mut pre_players, &raw_pre(vec![pre]), &enc, &enemies, &addr_to_rep);
        let mut post_players = vec![player(1), player(2)];
        apply(&mut post_players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);

        assert_eq!(pre_players[0].support, post_players[0].support);
        assert_eq!(post_players[0].support.cleanses, 0);
        assert_eq!(post_players[0].support.strips, 0);
    }

    /// Scenario: resurrect start-cast. Pre-era: `is_activation == Normal`
    /// on an ordinary combat event. Post-era: dedicated
    /// `sc::ANIMATION_START` statechange (see `sc::ANIMATION_START`'s doc
    /// comment for the version-threshold finding this reproduces -- this
    /// project's single `is_post_buff_rework` gate is what routes here).
    /// Same `src_agent` (raw, no master resolution) role in both eras.
    #[test]
    fn resurrect_start_cast_is_era_equivalent() {
        let mut pre = base_event();
        pre.skillid = RESURRECT_SKILL_ID;
        pre.is_activation = 1; // Normal
        pre.src_agent = 1;
        pre.value = 700;
        pre.buff_dmg = 800;

        let mut post = base_event();
        post.skillid = RESURRECT_SKILL_ID;
        post.is_statechange = sc::ANIMATION_START;
        post.src_agent = 1;
        post.value = 700;
        post.buff_dmg = 800;

        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();

        let mut pre_players = vec![player(1)];
        apply(&mut pre_players, &raw_pre(vec![pre]), &enc, &enemies, &addr_to_rep);
        let mut post_players = vec![player(1)];
        apply(&mut post_players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);

        assert_eq!(pre_players[0].support, post_players[0].support);
        assert_eq!(post_players[0].support.resurrects, 1);
    }

    /// Regression check for the version-threshold finding documented on
    /// `sc::ANIMATION_START`: a post-era log's resurrect-start row must NOT
    /// be missed by (incorrectly) still scanning for the pre-era
    /// `is_activation`-on-`is_statechange==0` shape -- i.e. a post-era log
    /// containing ONLY the dedicated-statechange row (no legacy-shaped row
    /// at all, which is what a real `>= 20260501` capture would emit) must
    /// still count the resurrect. This is the exact bug this task fixes:
    /// before this change, `apply` only ever recognized the pre-era shape,
    /// so this scenario silently produced zero resurrects.
    #[test]
    fn post_era_resurrect_not_silently_dropped() {
        let mut post = base_event();
        post.skillid = RESURRECT_SKILL_ID;
        post.is_statechange = sc::ANIMATION_START;
        post.src_agent = 1;
        post.value = 700;
        post.buff_dmg = 800;
        // No `is_activation` set at all (post-era rows don't carry it for
        // cast-start purposes) -- if `apply` still gated on `is_activation`,
        // this would wrongly produce zero.

        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        let mut players = vec![player(1)];
        apply(&mut players, &raw_post(vec![post]), &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.resurrects, 1, "post-era resurrect-cast row must not be silently dropped");
    }

    /// A post-era `sc::ANIMATION_STOP` (68) row for the Resurrect skill id
    /// -- the paired end-cast statechange, one ordinal after
    /// `ANIMATION_START` -- must NOT be double-counted (mirrors the pre-era
    /// `resurrect_end_cast_not_double_counted` test, using the post-era
    /// wire shape instead).
    #[test]
    fn post_era_resurrect_end_cast_not_double_counted() {
        let mut start = base_event();
        start.time = 0;
        start.skillid = RESURRECT_SKILL_ID;
        start.is_statechange = sc::ANIMATION_START;
        start.src_agent = 1;
        start.value = 700;
        start.buff_dmg = 800;
        let mut end = base_event();
        end.time = 1000;
        end.skillid = RESURRECT_SKILL_ID;
        end.is_statechange = 68; // CBTS_ANIMATIONSTOP -- one ordinal after ANIMATION_START
        end.src_agent = 1;
        end.value = 1000;

        let enc = enc_from(vec![enc_player(1)]);
        let enemies: BTreeSet<u64> = BTreeSet::new();
        let addr_to_rep: BTreeMap<u64, u64> = [(1, 1)].into_iter().collect();
        let mut players = vec![player(1)];
        apply(&mut players, &raw_post(vec![start, end]), &enc, &enemies, &addr_to_rep);
        assert_eq!(players[0].support.resurrects, 1);
    }
}
