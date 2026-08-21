//! Squad-side uptime for every OTHER buff -- `blocks.squad_buffs`.
//!
//! ## The gap this closes
//!
//! Elite Insights carries every buff a player HELD in one `buffUptimes`
//! array: boons, conditions, and the long tail of sigils, relics, food,
//! trait buffs, auras and signets. This project splits that array by
//! family, and until now the tail had no home at all:
//!
//! | | which ids | timelines |
//! |---|---|---|
//! | `buffs` (`blocks.boons`) | the 12 `BOON_IDS` | opt-in |
//! | `self_effects` | conditions + Stun + Daze (16) | yes |
//! | here | **everything else the log carries** | no |
//!
//! Measured on one real WvW log: Elite Insights emitted a 234-entry
//! `buffMap` and 36 `buffUptimes` rows for its first player; axilog
//! emitted 26 and 12. The 24 missing rows are this block. Downstream that
//! showed up as two entirely empty sections in axibridge (Special Buffs,
//! Sigil/Relic Uptime), both of which read exactly this population.
//!
//! ## A partition, not an overlap
//!
//! The three passes above cover disjoint id sets, and
//! [`squad_buff_ids`] enforces it by subtracting the other two. That
//! matters because the EI adapter concatenates `blocks.boons` and this
//! block into one `buffUptimes` array: an id in both would appear twice,
//! and Elite Insights' array has one entry per id.
//!
//! ## Mechanics: the boon machinery, re-pointed a fourth time
//!
//! `target_conditions` and `self_effects` each describe this pipeline as
//! the same code called with different inputs. This is the fourth
//! instantiation, and the only one whose id table is not static:
//!
//! | | boons | self_effects | here |
//! |---|---|---|---|
//! | id table | `BOON_IDS` (12) | conditions + control (16) | **discovered from the log** |
//! | owner scope | squad | squad | squad |
//! | output | uptime + opt-in states | uptime + states | **uptime only** |
//!
//! Discovery is what makes the id table dynamic, and it is deliberately
//! not a scan of the whole GW2EI catalog: simulating 2,267 buffs for every
//! player would be absurd when a log carries a few dozen. The buff-event
//! extractor is asked for EVERY id in the log and answers with only the
//! ids that produced a real buff event -- so the era-specific
//! apply/remove classification stays in one place rather than being
//! duplicated by a pre-scan here.
//!
//! ## Why no timelines
//!
//! `self_effects` emits them because "when was I stunned" is a graph
//! question. Nothing plots a sigil's stack count over time, and a timeline
//! per player per sigil would multiply this block's payload by an order of
//! magnitude for a graph no consumer draws. Additive to add later; a
//! breaking removal if it ships unused. Uptime alone is also why this pass
//! is ALWAYS-ON rather than `--timeseries`-gated like the other two: the
//! cost it carries is the cost `blocks.boons`' uptime half already carries.
//!
//! ## Which ids are eligible
//!
//! An id must resolve in [`crate::analysis::buffs::stacking`] -- some
//! catalog has to state its stack type -- or it is dropped. A buff whose
//! stack type is unknown cannot be simulated without guessing between the
//! duration and intensity machines, which produce different numbers, and
//! Elite Insights likewise tracks only the buffs its own container
//! defines.

use crate::analysis::buffs::events::BuffEvent;
use crate::analysis::buffs::{events, simulator, uptime, BoonTimeline, BoonUptime};
use crate::analysis::damage::InstidRegistry;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One log's squad-side uptime for every non-boon, non-self-effect buff,
/// keyed by `(player representative addr, buff id)`.
#[derive(Debug, Clone, Default)]
pub struct SquadBuffs {
    pub uptime: BTreeMap<(u64, u32), BoonUptime>,
}

/// The four Weaver dual-attunement buff ids (Fire/Water/Air/Earth).
///
/// A NON-Weaver elementalist's log carries real apply/remove events for
/// these alongside the plain attunement ids -- the game applies both -- and
/// Elite Insights deletes them outright for such a player
/// (`ElementalistHelper.RemoveDualBuffs`, called from
/// `ParsedData/CombatData.cs:125-128` under
/// `p.BaseSpec == Spec.Elementalist && p.Spec != Spec.Weaver`). Without
/// this rule the pass reports four ids per Tempest/Catalyst/core
/// elementalist that EI's own `buffUptimes` does not contain, at plausible
/// ~90% uptimes -- so a consumer would render a duplicate of the plain
/// attunement row rather than an obvious zero.
///
/// The WEAVER half of GW2EI's handling is deliberately NOT ported.
/// `WeaverHelper.TransformWeaverAttunements` is a different operation: it
/// groups a Weaver's attunement events by timestamp and synthesizes one
/// composite id per group (intersecting the major and minor translation
/// sets), invalidating every original. That is a rewrite of the event
/// stream, not a filter, and no capture available here contains a Weaver
/// -- porting it would mean shipping ~80 lines calibrated against nothing.
/// A Weaver therefore still reports the raw ids the log carries, which is
/// wrong the way it was already wrong, rather than newly wrong in an
/// unmeasured way.
const DUAL_ATTUNEMENT_IDS: [u32; 4] = [41166, 42264, 43470, 44857];

/// Whether this player is the `BaseSpec == Elementalist && Spec != Weaver`
/// case [`DUAL_ATTUNEMENT_IDS`] describes.
fn drops_dual_attunements(player: &crate::model::Player) -> bool {
    player.profession == "Elementalist" && player.elite_spec != "Weaver"
}

/// Whether `id` belongs to this pass rather than to `blocks.boons` or
/// `blocks.self_effects` -- and whether any catalog can state its stack
/// type, without which it cannot be simulated.
///
/// `pub` so the schema reprojection and its partition test can ask the
/// same question rather than re-deriving the answer from a third place.
pub fn is_squad_buff(id: u32) -> bool {
    let is_boon = crate::analysis::buffs::BOON_IDS.iter().any(|&(bid, _, _)| bid == id);
    let is_self_effect = crate::analysis::self_effects::effect_ids().contains(&id);
    if is_boon || is_self_effect {
        return false;
    }
    // GW2EI drops a `BuffClassification.Hidden` buff from `buffUptimes`
    // outright (`JsonModels/JsonActors/JsonPlayerBuilder.cs:266-269`).
    // These are internal state markers a player really does hold -- "Tome
    // of Justice Open", "Dual Fire Attunement" -- and reporting them would
    // put seven ids in this block that Elite Insights' own array does not
    // carry, which is what `the_pass_reports_no_buff_the_ei_export_lacks`
    // measured before this rule existed.
    if crate::analysis::buff_icons::meta(id).is_some_and(|m| m.is_hidden()) {
        return false;
    }
    crate::analysis::buffs::stacking(id).1.is_some()
}

/// Build every squad player's uptime for every buff in this pass's scope.
pub fn build(raw: &RawLog, enc: &Encounter) -> SquadBuffs {
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> SquadBuffs {
    let ids = squad_buff_ids(raw);
    if ids.is_empty() {
        return SquadBuffs::default();
    }
    // The extractor is asked for the whole candidate set at once. It walks
    // the events exactly once either way -- the id filter is a set lookup
    // -- so this is not a scan per id, and it keeps the era-specific
    // apply/remove classification in `events` rather than duplicating a
    // cheaper-looking version of it here.
    let all = events::extract_buff_events_with_registry(raw, registry, &ids);
    let capacities = events::extract_buff_capacities(raw, &ids);

    // Every login's address folds onto the account's representative, so a
    // relogged player stays one row -- the same fold
    // `simulate_boons_with_inputs` and `self_effects` both apply.
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    // The representatives whose dual-attunement events Elite Insights
    // deletes -- see [`DUAL_ATTUNEMENT_IDS`]. Keyed by representative
    // rather than by login addr because the filter below runs after the
    // relog fold, and a player's spec cannot differ between logins.
    let drops_duals: BTreeSet<u64> = enc
        .players
        .iter()
        .filter(|p| drops_dual_attunements(p))
        .map(|p| p.agent_addr)
        .collect();

    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for &e in &all {
        // Squad-only scope: an event whose OWNER is not a known squad addr
        // (an enemy holding the same sigil buff, an NPC, an addr absent
        // from the agent table) is dropped here, exactly as `self_effects`
        // drops it.
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        if DUAL_ATTUNEMENT_IDS.contains(&e.buff_id) && drops_duals.contains(&rep) {
            continue;
        }
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_start = raw.log_start_ms();
    let log_end = raw.events.last().map(|e| e.time).unwrap_or(0);

    let mut out = SquadBuffs::default();
    for (key, evs) in grouped {
        let (is_intensity, ctor_capacity) = crate::analysis::buffs::stacking(key.1);
        // arcdps's own `BUFF_INFO` row wins over the catalogued capacity
        // wherever the log carries one -- the preference order
        // `simulate_boons_with_inputs` and
        // `target_conditions::capacity_and_kind` both use, for the reason
        // MBUFFSIM measured: several real capacities sit far above the
        // static tables' values.
        //
        // The `expect` cannot fire: `squad_buff_ids` admits an id only
        // when `stacking` returned a capacity for it.
        let capacity = capacities
            .get(&key.1)
            .copied()
            .or(ctor_capacity)
            .expect("squad_buff_ids admits only ids `stacking` gives a capacity");
        let timeline = BoonTimeline { states: simulator::run(evs, capacity, is_intensity, log_end) };
        // A timeline that never leaves 0 carries no information, so it is
        // dropped rather than emitted -- the same rule `self_effects`
        // applies, reached the same way (test the VALUES: a zero-length
        // application survives the simulator's consecutive-equal dedup as
        // `[(t, 1), (t, 0)]`). Elite Insights likewise omits such a buff
        // from `buffUptimes` entirely, which is what
        // `the_pass_reports_no_buff_the_ei_export_lacks` pins.
        if !timeline.states.iter().any(|&(_, v)| v > 0) {
            continue;
        }
        out.uptime.insert(key, uptime::compute(&timeline, log_start, log_end));
    }
    out
}

/// Every buff id this pass considers for `raw`, ascending: the ids the log
/// mentions at all, narrowed by [`is_squad_buff`].
///
/// A superset of what survives -- an id here that produced no real buff
/// event simply yields no group below -- which is the point: the
/// classification of what IS a buff event belongs to
/// [`events::extract_buff_events_with_registry`], not to this filter.
pub fn squad_buff_ids(raw: &RawLog) -> BTreeSet<u32> {
    raw.events.iter().map(|e| e.skillid).filter(|&id| is_squad_buff(id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::condition_catalog::CONDITION_BUFFS;
    use crate::analysis::buffs::BOON_IDS;

    /// A sigil is this pass's business.
    #[test]
    fn a_sigil_is_a_squad_buff() {
        assert!(is_squad_buff(9286), "Superior Sigil of Bloodlust");
    }

    /// A boon is not: `blocks.boons` already owns it, and a duplicate row
    /// would appear twice in the EI adapter's concatenated `buffUptimes`.
    #[test]
    fn no_boon_is_a_squad_buff() {
        for &(id, name, _) in &BOON_IDS {
            assert!(!is_squad_buff(id), "{name} ({id}) belongs to blocks.boons");
        }
    }

    /// Nor is a condition or control effect: `blocks.self_effects` owns
    /// those.
    #[test]
    fn no_self_effect_is_a_squad_buff() {
        for &(id, name, _, _) in &CONDITION_BUFFS {
            assert!(!is_squad_buff(id), "{name} ({id}) belongs to blocks.self_effects");
        }
        assert!(!is_squad_buff(872), "Stun belongs to blocks.self_effects");
        assert!(!is_squad_buff(833), "Daze belongs to blocks.self_effects");
    }

    /// An id no catalog defines is dropped rather than simulated with a
    /// guessed stack type.
    #[test]
    fn an_uncatalogued_id_is_not_a_squad_buff() {
        assert!(!is_squad_buff(4_000_000));
    }
}
