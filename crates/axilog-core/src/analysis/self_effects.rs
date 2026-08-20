//! Squad-side condition and control-effect uptime + stack timelines --
//! `blocks.self_effects`.
//!
//! ## The gap this closes
//!
//! Elite Insights carries every buff a player HELD, boons and conditions
//! alike, in one `buffUptimes` array per player. This project splits them by
//! direction and by family, and until now the split left a hole:
//!
//! | | who holds it | which ids | timelines |
//! |---|---|---|---|
//! | `buffs::states` | squad players | the 12 `BOON_IDS` | yes |
//! | `target_conditions` | ENEMIES | the 14 `CONDITION_BUFFS` | `per_source` only |
//! | here | **squad players** | **conditions + Stun + Daze** | **yes** |
//!
//! Nothing answered "what was on ME". `analysis::cc` is not a substitute:
//! it tests `result == CROWD_CONTROL` and yields event COUNTS and summed
//! durations, a genuinely different measurement that cannot be turned into
//! a stack timeline.
//!
//! ## Mechanics: the boon machinery, re-pointed again
//!
//! `target_conditions`'s module doc describes this pipeline with two
//! substitutions; this is the third instantiation, with the same two:
//!
//! | | boons | target_conditions | here |
//! |---|---|---|---|
//! | id table | `buffs::BOON_IDS` (12) | `CONDITION_BUFFS` (14) | conditions + control (16) |
//! | owner scope | squad `Player::agent_addr` | enemy `Enemy::id` | squad `Player::agent_addr` |
//!
//! Event extraction (`buffs::events::extract_buff_events_with_registry`),
//! capacity extraction (`events::extract_buff_capacities`), the stack
//! machine (`buffs::simulator::run`), the uptime integral
//! (`buffs::uptime::compute`) and the EI reshaping
//! (`buffs::states::to_ei_states`) are all the SAME code, called with
//! different inputs -- so a condition timeline on a player and a boon
//! timeline on the same player can never disagree about simulator
//! semantics.
//!
//! This is the [`buffs::simulate_boons`] path, NOT the
//! `generation::run_segments` path `target_conditions` uses: this block
//! emits a FUSED total and no `per_source`, and the fused total is exactly
//! what the stack machine produces. `buffs::states::build` itself cannot be
//! reused, because it consumes `metrics.boons` -- already-simulated
//! timelines that exist only for the 12 boons.
//!
//! **Capacity 1 is new here.** Stun and Daze are the first buffs this
//! project simulates whose capacity is 1, which reaches an arm of
//! `simulator::run_duration` no boon and no condition ever did (the boons'
//! minimum real capacity is 5, the conditions' 3). That arm already
//! implements the right semantics -- see its comment.
//!
//! **Standalone, NOT wired into `analyze()`** -- opt-in like
//! `buffs::states` and `target_conditions`, and gated by every caller on
//! `--timeseries`.

use crate::analysis::buffs::events::BuffEvent;
use crate::analysis::buffs::states::{self, StateTimeline};
use crate::analysis::buffs::{events, simulator, uptime, BoonTimeline, BoonUptime};
use crate::analysis::condition_catalog::CONDITION_BUFFS;
use crate::analysis::control_catalog::CONTROL_EFFECTS;
use crate::analysis::damage::InstidRegistry;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One log's squad-side effect uptime and stack timelines, both keyed by
/// `(player representative addr, buff id)`.
///
/// Two reductions of ONE simulation, deliberately: the summary numbers and
/// the graph must describe the same held stacks, and the only way to
/// guarantee that is to derive both from the same timeline rather than run
/// the machine twice.
#[derive(Debug, Clone, Default)]
pub struct SelfEffects {
    /// The fight-long uptime summary, `buffUptimes[].buffData[0]`'s two
    /// numbers.
    pub uptime: BTreeMap<(u64, u32), BoonUptime>,
    /// The fused stack timeline, `buffUptimes[].states`. Duration effects
    /// are clamped to 0/1 (see [`build_with_registry`]).
    pub states: BTreeMap<(u64, u32), StateTimeline>,
}

/// `(is_intensity, ctor capacity)` for a tracked effect id, from whichever
/// of the two tables carries it, or `None` for an id this pass does not
/// track.
///
/// This is the ONE lookup spanning both tables, and it is `pub` so the
/// schema reprojection can ask the same question rather than re-deriving
/// the answer from a third place. It deliberately does NOT reuse
/// `target_conditions::capacity_and_kind`, whose `.expect` panics on any id
/// outside `CONDITION_BUFFS`, nor `simulator::capacity_for`, whose `_ => 5`
/// arm is documented as unreachable for the ids it knows and would silently
/// give Stun a capacity of 5.
pub fn effect_kind(id: u32) -> Option<(bool, u32)> {
    CONDITION_BUFFS
        .iter()
        .chain(CONTROL_EFFECTS.iter())
        .find(|&&(i, _, _, _)| i == id)
        .map(|&(_, _, is_intensity, capacity)| (is_intensity, capacity))
}

/// The 16 buff ids this pass tracks, ascending.
pub fn effect_ids() -> BTreeSet<u32> {
    CONDITION_BUFFS
        .iter()
        .chain(CONTROL_EFFECTS.iter())
        .map(|&(id, _, _, _)| id)
        .collect()
}

/// `(arcdps-reported-or-table capacity, is_intensity)` for one tracked id.
/// arcdps's own `sc::BUFF_INFO` row wins where the log carries one -- the
/// same preference order `simulate_boons_with_inputs` and
/// `target_conditions::capacity_and_kind` both use, for the reason
/// MBUFFSIM measured: several real capacities sit far above the static
/// tables' values.
fn capacity_and_kind(capacities: &BTreeMap<u32, u32>, id: u32) -> (u32, bool) {
    let (is_intensity, ctor_capacity) =
        effect_kind(id).expect("capacity_and_kind is only called with an id from `effect_ids`");
    (capacities.get(&id).copied().unwrap_or(ctor_capacity), is_intensity)
}

/// Build every squad player's effect uptime and stack timelines.
pub fn build(raw: &RawLog, enc: &Encounter) -> SelfEffects {
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> SelfEffects {
    let ids = effect_ids();
    let all = events::extract_buff_events_with_registry(raw, registry, &ids);
    let capacities = events::extract_buff_capacities(raw, &ids);

    // Every login's address folds onto the account's representative, so a
    // relogged player stays one row -- the same fold
    // `simulate_boons_with_inputs` applies, for the same reason.
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();

    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for &e in &all {
        // Squad-only scope: an event whose OWNER is not a known squad addr
        // (an enemy holding the same condition, an NPC, an addr absent from
        // the agent table) is dropped here. The enemy side is
        // `target_conditions`' job.
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_start = raw.log_start_ms();
    let log_end = raw.events.last().map(|e| e.time).unwrap_or(0);

    let mut out = SelfEffects::default();
    for (key, evs) in grouped {
        let (capacity, is_intensity) = capacity_and_kind(&capacities, key.1);
        let timeline = BoonTimeline { states: simulator::run(evs, capacity, is_intensity, log_end) };
        // Clamped for the GRAPH only, never for the uptime integral: the
        // clamp is a presentation rule GW2EI applies to duration buffs'
        // step function, while `presence_pct` is already a 0/1 measure and
        // `avg_stacks` is meaningless for a duration buff (the schema omits
        // it). `buffs::states::build` applies the same clamp at the same
        // point, driven by the same `is_intensity`.
        let clamp = !is_intensity;
        let steps = timeline.states.iter().map(move |&(t, v)| (t, if clamp { v.min(1) } else { v }));
        let ei_states = states::to_ei_states(steps, log_start);
        // A timeline that never leaves 0 carries no information, so it is
        // dropped rather than emitted -- the same rule `target_conditions`
        // reaches by a different route (its `overlap_steps` skips every
        // `end <= start` segment, so a zero-length hold never survives to
        // `to_ei_states` in the first place). Elite Insights likewise omits
        // such a buff from `buffUptimes` entirely.
        //
        // Testing the VALUES, not the length: a length test is insufficient
        // here. A zero-length application emits two simulator states at the
        // same instant -- `push_state` dedupes only CONSECUTIVE EQUAL
        // counts, so `[(t, 1), (t, 0)]` survives it -- and `to_ei_states`
        // fuses that pair at one timestamp into `[[0, 0], [t, 0]]`, which is
        // two entries and would sneak past `len() < 2`. Two such rows really
        // do occur on the committed fixture. Do not re-simplify this back to
        // a length check.
        //
        // Both maps are written together so their key sets cannot diverge.
        if !ei_states.iter().any(|&(_, v)| v > 0) {
            continue;
        }
        out.uptime.insert(key, uptime::compute(&timeline, log_start, log_end));
        out.states.insert(key, ei_states);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::condition_catalog::{BLEEDING, CHILLED};
    use crate::analysis::control_catalog::{DAZE, STUN};
    use crate::evtc::decode_raw;
    use crate::model::resolve;

    #[test]
    fn the_id_set_is_exactly_sixteen_and_spans_both_tables() {
        let ids = effect_ids();
        assert_eq!(ids.len(), 16, "14 conditions + Stun + Daze");
        assert!(ids.contains(&STUN));
        assert!(ids.contains(&DAZE));
        assert!(ids.contains(&BLEEDING));
        // A boon must never be swept in: Might (740) sits numerically
        // between Vulnerability (738) and Weakness (742), the exact id any
        // range-based shortcut would catch.
        assert!(!ids.contains(&740), "Might is not a self effect");
    }

    #[test]
    fn every_tracked_id_resolves_to_a_kind_and_capacity() {
        for id in effect_ids() {
            assert!(effect_kind(id).is_some(), "{id} must resolve in one of the two tables");
        }
        assert_eq!(effect_kind(STUN), Some((false, 1)));
        assert_eq!(effect_kind(DAZE), Some((false, 1)));
        assert_eq!(effect_kind(BLEEDING), Some((true, 1500)));
        assert_eq!(effect_kind(CHILLED), Some((false, 5)));
        assert_eq!(effect_kind(740), None, "a boon is not a self effect");
    }

    /// arcdps's own reported capacity wins over the table's ctor value,
    /// the same preference order `target_conditions::capacity_and_kind`
    /// and `simulate_boons_with_inputs` both use.
    #[test]
    fn capacity_prefers_the_arcdps_reported_value_over_the_table() {
        let none: BTreeMap<u32, u32> = BTreeMap::new();
        assert_eq!(capacity_and_kind(&none, BLEEDING), (1500, true));
        let reported: BTreeMap<u32, u32> = [(BLEEDING, 99u32)].into_iter().collect();
        assert_eq!(capacity_and_kind(&reported, BLEEDING), (99, true));
    }

    fn fixture() -> SelfEffects {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
                .expect("read committed fixture");
        let raw = decode_raw(&bytes).expect("decode fixture");
        let enc = resolve(&raw);
        build(&raw, &enc)
    }

    /// The committed fixture carries all 16 ids on squad players --
    /// measured with a throwaway probe before this plan was written: Stun
    /// on 3 players, Daze on 8, Taunt on 2 (the thinnest three). A pass
    /// that silently emitted nothing for the two control effects -- the
    /// whole reason this block exists -- cannot go green.
    #[test]
    fn the_committed_fixture_produces_rows_for_every_tracked_id_including_stun_and_daze() {
        let out = fixture();
        assert!(!out.states.is_empty(), "the fixture must produce timelines");
        let ids: BTreeSet<u32> = out.states.keys().map(|&(_, id)| id).collect();
        for id in effect_ids() {
            assert!(ids.contains(&id), "no row for tracked id {id}");
        }
    }

    /// `uptime` and `states` are two reductions of ONE simulation, so they
    /// must have exactly the same keys. A divergence would mean a consumer
    /// could read a timeline with no uptime, or an uptime with no graph.
    #[test]
    fn uptime_and_states_carry_identical_keys() {
        let out = fixture();
        let u: BTreeSet<(u64, u32)> = out.uptime.keys().copied().collect();
        let s: BTreeSet<(u64, u32)> = out.states.keys().copied().collect();
        assert_eq!(u, s);
    }

    /// Duration effects are clamped to 0/1 so the graph means what Elite
    /// Insights' means; intensity effects keep their real stack count.
    /// Most of these 16 ids are duration-stacking, so getting this wrong
    /// would be visible everywhere.
    #[test]
    fn duration_effects_are_clamped_to_zero_or_one_and_intensity_ones_are_not() {
        let out = fixture();
        let mut saw_intensity_above_one = false;
        for (&(_, id), timeline) in &out.states {
            let (is_intensity, _) = effect_kind(id).expect("tracked id resolves");
            for &(_, stacks) in timeline {
                if is_intensity {
                    saw_intensity_above_one |= stacks > 1;
                } else {
                    assert!(stacks <= 1, "duration effect {id} reported {stacks} stacks");
                }
            }
        }
        assert!(
            saw_intensity_above_one,
            "the fixture must exercise the unclamped intensity branch too"
        );
    }

    /// Every timeline opens with the mandatory leading `[0, 0]` pair and
    /// reaches a nonzero stack count somewhere -- a timeline that never
    /// leaves 0 carries no information and is dropped rather than emitted.
    ///
    /// The nonzero assertion is the point: a `len() >= 2` check passes over
    /// a fused zero-length application (`[[0, 0], [t, 0]]`), which is
    /// exactly the row the drop rule exists to remove.
    #[test]
    fn every_emitted_timeline_is_nontrivial_and_starts_at_zero() {
        let out = fixture();
        for (key, timeline) in &out.states {
            assert_eq!(timeline.first(), Some(&(0u64, 0u32)), "{key:?} must open with [0, 0]");
            assert!(
                timeline.iter().any(|&(_, stacks)| stacks > 0),
                "{key:?} never leaves 0 stacks, so it carries no information"
            );
        }
    }

    /// Timelines are keyed by the account's REPRESENTATIVE address, so a
    /// relogged player stays one row rather than splitting across the
    /// addresses each login produced.
    #[test]
    fn rows_are_keyed_by_representative_addresses_only() {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
                .expect("read committed fixture");
        let raw = decode_raw(&bytes).expect("decode fixture");
        let enc = resolve(&raw);
        let out = build(&raw, &enc);
        let reps: BTreeSet<u64> = enc.players.iter().map(|p| p.agent_addr).collect();
        for &(addr, _) in out.states.keys() {
            assert!(reps.contains(&addr), "{addr} is not a squad representative address");
        }
    }
}
