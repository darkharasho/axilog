//! arcdps-meter-parity cleanse/strip counting (`cleanses_arcdps` /
//! `strips_arcdps`).
//!
//! **Provenance: this module is a transcription of the arcdps meter's own
//! counting code**, pasted verbatim into Discord by deltaconnected (arcdps'
//! author) on 2026-08-26 in response to a direct question about why
//! AxiBridge's cleanse totals read a few percent under the in-game meter.
//! It is deliberately a SEPARATE pass from [`crate::analysis::support`],
//! which stays bit-identical to GW2EI (`SupportStatistics.cs`) and is
//! calibrated exact against the golden fixture. The two disagree on purpose;
//! see "Where this differs from EI" below.
//!
//! The rules, in the order the reference code evaluates them:
//!
//! 1. Only `CBTS_BUFFREMOVE_ALL` rows count. (Same as EI --
//!    `BuffRemoveSingle`/`Manual` feed neither meter.)
//! 2. Two skill-specific exclusions, applied before anything else:
//!    - **stability** is skipped unless `result > 1`. On a removal row
//!      `result` carries the stacks-removed count, so a single-stack loss
//!      is stability being consumed by a CC, not a strip. (The same rule
//!      already governs `analysis::contribution`'s strip credit.)
//!    - **blind** is skipped when `src_agent == dst_agent`, i.e. blind you
//!      burned off yourself with your own next attack.
//! 3. Roles are inverted relative to an apply: `src_agent` is the buff's
//!    HOLDER, `dst_agent` is the REMOVER. Both must be nonzero (the
//!    reference's `if (dst_iid && src_iid)`); the game emits removal rows
//!    with `dst_agent == 0` for uncredited removals and those count for
//!    nobody.
//! 4. If the remover's master resolves to the holder -- self-self, or
//!    pet-self -- a condition removal is a **cleanse credited to the
//!    holder**. There is no `iff` test on this branch.
//! 5. Otherwise the remover is credited: condition + `iff == FRIEND` is a
//!    cleanse, boon + `iff == FOE` is a strip.
//! 6. **Down-undo:** when an agent goes down the game dumps every condition
//!    off them as self-removals, and arcdps subtracts those back out. See
//!    [`DOWN_UNDO_WINDOW_MS`] for how this pass reproduces that from a log,
//!    and why it cannot copy the reference implementation literally.
//! 7. **No squad/non-squad discrimination**, for players or for pets. The
//!    in-game meter has no `PlayerList` concept at all. (deltaconnected,
//!    same thread: "afaik there shouldnt be any discrimination between
//!    squad and non-squad players (or squad and non-squad pets)".)
//!
//! # Why this is three counters and not one
//!
//! There is no single "the arcdps number". Asked what the meter actually
//! displays, deltaconnected's answer was that it "depends on what the
//! exclusion on that particular window is set to. some players might only
//! have vs npcs turned off (thatll exclude squad players cleansing minions
//! etc), some might have vs npcs and from npcs turned off, which in addition
//! would also exclude pets/npcs cleansing from squad."
//!
//! So the displayed total is a function of two per-window toggles, and this
//! pass refuses to guess which the reader has set. It emits the toggle-
//! independent base plus the two adjustments, and the consumer sums the ones
//! their meter includes:
//!
//! | reader's window | sum |
//! |---|---|
//! | vs npcs OFF, from npcs OFF | `cleanses_arcdps` |
//! | vs npcs ON, from npcs OFF | `+ cleanses_arcdps_on_minion` |
//! | vs npcs OFF, from npcs ON | `+ cleanses_arcdps_by_minion` |
//! | both ON | all three |
//!
//! This matters: on `fixtures/wvw-small.anon.zevtc` the three buckets are
//! 878 / 82 / 145, i.e. the choice of toggles moves the total by 26%. A
//! single hardcoded number would have been wrong for most readers.
//!
//! Rows where BOTH sides are minions are counted under `by_minion` alone
//! (they are excluded whenever "from npcs" is off, which is the common
//! case); a reader running "from npcs ON, vs npcs OFF" therefore over-counts
//! by exactly that population, which is 0 rows on the calibration fixture.
//!
//! **Where this differs from EI**, and therefore from
//! [`crate::analysis::support`]'s calibrated counters:
//!
//! | | `support` (EI) | this module (arcdps) |
//! |---|---|---|
//! | stability single-stack | counts as a strip | excluded |
//! | self-consumed blind | counts as a self-cleanse | excluded |
//! | conditions dumped on down | count as self-cleanses | subtracted back out |
//! | cleanse off a non-squad friendly | uncounted | counted |
//! | cleanse off any pet/minion | uncounted (`cleanses_minions` only, and only for squad-owned pets) | `_on_minion` bucket |
//! | a pet's own removal | credited to the pet (dropped) | folded into its master, `_by_minion` bucket |
//! | "foe" for strips | `enemies` set membership | the row's own `iff` byte |
//!
//! Because rule 4 credits the HOLDER (a pet cleansing its master credits the
//! master) and rule 5 folds a pet remover into its master, these counters
//! are NOT `cleanses + cleanses_self + cleanses_minions` plus a correction
//! -- they are an independent count. Consumers wanting the in-game number
//! read the `_arcdps` family; consumers wanting EI parity read the existing
//! fields. Neither is derived from the other.

use crate::analysis::buffs::{BOON_IDS, STABILITY};
use crate::analysis::condition_catalog::{BLIND, CONDITION_SKILL_IDS};
use crate::analysis::damage::InstidRegistry;
use crate::analysis::PlayerMetrics;
use crate::evtc::{buff_remove, iff, sc, RawEvent, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// How far back before a `CBTS_CHANGEDOWN` row this pass looks for the
/// self-removal burst that going down produces (rule 6).
///
/// **This is a deliberate deviation from the reference implementation, and
/// it is forced.** arcdps walks its live per-actor event chain backwards
/// from the downed apply, breaking at the first entry that is neither a
/// buff-removal nor the "determined" apply, and dedupes by zeroing the
/// entry's `dst_agent` so a later readback cannot double-subtract it.
/// deltaconnected flagged in the same thread that this mutation is invisible
/// to us: *"`cur_readback->dst_agent = 0;` wouldnt apply to anything written
/// to logs - by the time the client gets the downed signal, the buff removal
/// events are already committed into the writing buffer with non-zero dst"*.
/// So a log-side reimplementation needs its own once-only guard (this pass
/// keeps a consumed-row set) and its own run boundary.
///
/// A literal chain-walk is also not reproducible from a log, because
/// "determined" is not one id. The catalog in [`crate::analysis::buff_icons`]
/// carries **three** — 762, 785 and 788 — all sharing the one wiki icon, and
/// which one a down emits depends on the context. deltaconnected confirmed
/// the reference's `SKILL_DETERMINED_PLAYER = 788` from the training golem:
/// *"i promise 788 is the determined you get in the training golem lol i
/// didnt check wvw"*. WvW is the case that differs: dumping the rows
/// preceding all 25 downs in `fixtures/wvw-small.anon.zevtc` shows the
/// determined apply landing under **762**, with 788 appearing five times in
/// the entire log. So the constant is correct for the encounter it was read
/// off, and simply does not generalise to our capture population. The
/// sequence also interleaves non-removal applies (`851`, "Downed Pet") that a
/// literal break-on-first-non-removal walk would stop at, cutting the burst
/// short.
///
/// A time window is robust to both. The burst and its down are emitted in
/// the same server tick: in that fixture every self-removal in a down burst
/// lands within 1ms of the `CHANGE_DOWN` row. 100ms is two orders of
/// magnitude of headroom while staying far below the ~4s minimum between a
/// player's successive downs, so a burst can never be attributed to the
/// wrong down.
pub const DOWN_UNDO_WINDOW_MS: u64 = 100;

/// Every "determined" buff id, which the down-undo chain walks through rather
/// than breaking on. There is no single one: the catalog carries three, all
/// sharing one wiki icon, and which is emitted depends on the context and the
/// agent. The reference's `SKILL_DETERMINED_PLAYER = 788` was read off the
/// training golem; player downs in the WvW fixture carry 762, and 788 shows up
/// there on a non-player agent.
const DETERMINED_IDS: [u32; 3] = [762, 785, 788];

/// True for the era-appropriate `BUFFREMOVE_ALL` wire shape. Transcribed
/// from [`crate::analysis::support`]'s two era loops, which carry the full
/// GW2EI provenance for both predicates.
fn is_remove_all(e: &RawEvent, post_era: bool) -> bool {
    if post_era {
        e.is_statechange == sc::BUFF_REMOVE_ALL
    } else {
        e.is_statechange == 0 && e.is_activation == 0 && e.is_buffremove == buff_remove::ALL
    }
}

/// Fold a possibly-pet agent into the actor arcdps would credit: its master
/// if it has one at this time, else itself. Mirrors
/// `buffs::events::extract_buff_events_with_registry`'s `resolve_agent`.
fn fold_to_master(addr: u64, master_instid: u16, time: u64, registry: &InstidRegistry) -> u64 {
    if master_instid != 0 {
        registry.resolve_at(master_instid, time).unwrap_or(addr)
    } else {
        addr
    }
}

/// Fill `cleanses_arcdps`/`strips_arcdps` on every tracked player. Additive
/// only: touches no other [`crate::analysis::support::SupportMetrics`]
/// field, so every EI-calibrated golden is unaffected.
pub fn apply(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    addr_to_rep: &BTreeMap<u64, u64>,
    registry: &InstidRegistry,
) {
    let idx: BTreeMap<u64, usize> = players
        .iter()
        .enumerate()
        .map(|(i, p)| (p.agent_addr, i))
        .collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    let post_era = raw.header.is_post_buff_rework();
    let boon_id_set: BTreeSet<u32> = BOON_IDS.iter().map(|&(id, _, _)| id).collect();
    let condition_id_set: BTreeSet<u32> = CONDITION_SKILL_IDS.iter().copied().collect();

    // Rule 4's credited rows, per holder, in log order -- the only rows the
    // down-undo pass is allowed to take back. `(time, event index, player
    // index, by_minion)`; the last field says which bucket the credit went
    // into, so the take-back decrements the same one.
    // Keyed by raw event index: `(holder, player index, by_minion)`. The
    // down-undo walk looks rows up by the index it is standing on, so it needs
    // no separate ordering.
    let mut self_row_at: BTreeMap<usize, (u64, usize, bool)> = BTreeMap::new();

    for (k, e) in raw.events.iter().enumerate() {
        if !is_remove_all(e, post_era) {
            continue;
        }
        let is_condition = condition_id_set.contains(&e.skillid);
        let is_boon = boon_id_set.contains(&e.skillid);
        if !is_condition && !is_boon {
            continue;
        }
        // Rule 2.
        if e.skillid == STABILITY && (e.result as u32) <= 1 {
            continue;
        }
        if e.skillid == BLIND && e.src_agent == e.dst_agent {
            continue;
        }
        // Rule 3.
        let holder = e.src_agent;
        let remover = e.dst_agent;
        if holder == 0 || remover == 0 {
            continue;
        }
        let remover_actor = fold_to_master(remover, e.dst_master_instid, e.time, registry);
        // Which of the reader's two window toggles this row answers to.
        // `by_minion` wins when both apply -- see the module doc.
        let by_minion = e.dst_master_instid != 0;
        let on_minion = e.src_master_instid != 0;
        // Rule 4: self-self or pet-self. Credit lands on the HOLDER (which
        // IS the remover's master here), and carries no `iff` test.
        if rep(remover_actor) == rep(holder) {
            if is_condition {
                if let Some(&i) = idx.get(&rep(holder)) {
                    let s = &mut players[i].support;
                    if by_minion {
                        s.cleanses_arcdps_by_minion += 1;
                    } else {
                        s.cleanses_arcdps += 1;
                    }
                    self_row_at.insert(k, (rep(holder), i, by_minion));
                }
            }
            continue;
        }
        // Rule 5: the remover (pet folded into master) is credited.
        let Some(&i) = idx.get(&rep(remover_actor)) else {
            continue;
        };
        let s = &mut players[i].support;
        if is_condition && e.iff == iff::FRIEND {
            if by_minion {
                s.cleanses_arcdps_by_minion += 1;
            } else if on_minion {
                s.cleanses_arcdps_on_minion += 1;
            } else {
                s.cleanses_arcdps += 1;
            }
        } else if is_boon && e.iff == iff::FOE {
            if by_minion {
                s.strips_arcdps_by_minion += 1;
            } else if on_minion {
                s.strips_arcdps_on_minion += 1;
            } else {
                s.strips_arcdps += 1;
            }
        }
    }

    // Rule 6: take back the self-removal burst each down produces. A row may
    // only be taken back once (the reference's `dst_agent = 0` dedupe, which
    // never reaches the log -- see `DOWN_UNDO_WINDOW_MS`).
    //
    // Walked as a chain, the way the reference does it: backwards from the
    // down over the rows between the downed agent and itself, stopping at the
    // first entry that is not a buff removal. Two adjustments are forced by
    // the fact that we read a log rather than arcdps' live buffer, and both
    // were verified against every down in `fixtures/wvw-small.anon.zevtc`:
    //
    // - **Statechange rows are skipped, not treated as chain entries.** The
    //   reference walks arcdps' internal buff-event chain, which contains no
    //   statechange rows; the log stream interleaves them. Breaking on them
    //   truncated 4 of the 25 bursts to nothing, every one of them on a
    //   `sc == 62` row that is not a buff event at all.
    // - **The walk is bounded by `DOWN_UNDO_WINDOW_MS`.** arcdps' chain is a
    //   bounded live ring buffer; a log has no such horizon, so an agent whose
    //   previous buff event was minutes earlier accumulates removals all the
    //   way back. Unbounded, one burst reached 23 rows against a true 10.
    //
    // Bounded and skipping statechanges, the chain walk agrees with a plain
    // time window on all 25 downs, so this refinement costs nothing against
    // the calibration and only guards logs where a non-removal buff event
    // lands inside the window.
    let mut consumed: BTreeSet<usize> = BTreeSet::new();
    let downs: Vec<usize> = raw
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_statechange == sc::CHANGE_DOWN)
        .map(|(i, _)| i)
        .collect();
    for di in downs {
        let d = &raw.events[di];
        let holder = rep(d.src_agent);
        let floor = d.time.saturating_sub(DOWN_UNDO_WINDOW_MS);
        for j in (0..di).rev() {
            let e2 = &raw.events[j];
            if e2.time < floor {
                break;
            }
            let involves_self = rep(e2.src_agent) == holder && rep(e2.dst_agent) == holder;
            let server_row = e2.src_agent == 0 && rep(e2.dst_agent) == holder;
            if !involves_self && !server_row {
                continue;
            }
            let any_buffremove = is_remove_all(e2, post_era) || e2.is_buffremove != 0;
            if !any_buffremove {
                // Not a removal. Three kinds of row pass through without
                // breaking the chain; everything else ends it.
                //
                // deltaconnected: *"the chain i loop over is going to
                // lock/unlock around the condi sim loop, evtc logs dont. so
                // its possible youll see buff damage inbetween the server
                // buffremove messages - you should do `skillid_readback ==
                // SKILL_DETERMINED_PLAYER || (cur->statechange == CBTS_COMBAT
                // && cur->buff)` to skip over buff damage without breaking
                // out"*. So any buff row in a combat entry is skipped, which
                // covers buff damage ticks and buff applies alike.
                let buff_combat_row =
                    e2.is_statechange == 0 && e2.is_activation == 0 && e2.buff != 0;
                let determined = DETERMINED_IDS.contains(&e2.skillid);
                // Anything the server applies with a zero source: flagged as
                // almost guaranteed to sit between the down and its removals.
                let server_row = e2.src_agent == 0;
                if buff_combat_row || determined || server_row {
                    continue;
                }
                // Statechange rows are not chain entries at all -- arcdps'
                // chain holds buff events only, the log stream interleaves
                // these. Skipping them is what keeps the burst intact.
                if e2.is_statechange != 0 {
                    continue;
                }
                break;
            }
            // Any buffremove keeps the chain alive; only REMOVE_ALL scored.
            let Some(&(row_holder, i, by_minion)) = self_row_at.get(&j) else {
                continue;
            };
            if row_holder != holder || !consumed.insert(j) {
                continue;
            }
            let s = &mut players[i].support;
            if by_minion {
                s.cleanses_arcdps_by_minion = s.cleanses_arcdps_by_minion.saturating_sub(1);
            } else {
                s.cleanses_arcdps = s.cleanses_arcdps.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::condition_catalog::{BLEEDING, WEAKNESS};
    use crate::evtc::{RawHeader, RawLog};

    const PLAYER_A: u64 = 100;
    const PLAYER_B: u64 = 200;
    const ENEMY: u64 = 900;
    /// A pet of `PLAYER_A`. `instid` and `master_instid` wiring below make
    /// the registry resolve it back to `PLAYER_A`.
    const PET_A: u64 = 101;
    const PET_A_INSTID: u16 = 11;
    const PLAYER_A_INSTID: u16 = 10;

    fn ev() -> RawEvent {
        RawEvent {
            time: 1_000,
            src_agent: 0,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: iff::FRIEND,
            buff: 1,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0,
            is_fifty: 0,
            is_moving: 0,
            is_statechange: 0,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    /// A `BUFFREMOVE_ALL` row: `holder` lost `skillid`, `remover` took it off.
    fn removal(skillid: u32, holder: u64, remover: u64) -> RawEvent {
        RawEvent {
            skillid,
            src_agent: holder,
            dst_agent: remover,
            is_buffremove: buff_remove::ALL,
            result: 1,
            ..ev()
        }
    }

    /// Registers `PLAYER_A_INSTID -> PLAYER_A` so a pet row carrying
    /// `master_instid = PLAYER_A_INSTID` folds onto A.
    fn registration() -> RawEvent {
        RawEvent {
            src_agent: PLAYER_A,
            src_instid: PLAYER_A_INSTID,
            time: 0,
            ..ev()
        }
    }

    fn run(mut events: Vec<RawEvent>) -> Vec<PlayerMetrics> {
        events.insert(0, registration());
        let raw = RawLog {
            header: RawHeader {
                build: "20260114".into(),
                revision: 1,
                boss_id: 1,
            },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        };
        let mut players = vec![
            PlayerMetrics {
                agent_addr: PLAYER_A,
                ..Default::default()
            },
            PlayerMetrics {
                agent_addr: PLAYER_B,
                ..Default::default()
            },
        ];
        let addr_to_rep: BTreeMap<u64, u64> = [
            (PLAYER_A, PLAYER_A),
            (PLAYER_B, PLAYER_B),
            (ENEMY, ENEMY),
            (PET_A, PET_A),
        ]
        .into_iter()
        .collect();
        let registry = InstidRegistry::build(&raw);
        apply(&mut players, &raw, &addr_to_rep, &registry);
        players
    }

    fn a(players: &[PlayerMetrics]) -> crate::analysis::support::SupportMetrics {
        players
            .iter()
            .find(|p| p.agent_addr == PLAYER_A)
            .unwrap()
            .support
    }

    /// Rule 5: B strips a condition off A -- a cleanse credited to B.
    #[test]
    fn cross_cleanse_credits_the_remover() {
        let p = run(vec![removal(BLEEDING, PLAYER_A, PLAYER_B)]);
        let b = p.iter().find(|p| p.agent_addr == PLAYER_B).unwrap().support;
        assert_eq!(b.cleanses_arcdps, 1);
        assert_eq!(a(&p).cleanses_arcdps, 0, "the holder is not credited");
    }

    /// Rule 4: a self-removal credits the holder, with no `iff` test.
    #[test]
    fn self_cleanse_credits_the_holder() {
        let p = run(vec![removal(BLEEDING, PLAYER_A, PLAYER_A)]);
        assert_eq!(a(&p).cleanses_arcdps, 1);
    }

    /// Rule 2: stability only counts as a strip when MORE THAN ONE stack
    /// came off -- a single-stack loss is stability being eaten by a CC.
    #[test]
    fn stability_single_stack_is_not_a_strip() {
        let mut one = removal(STABILITY, ENEMY, PLAYER_A);
        one.iff = iff::FOE;
        one.result = 1;
        assert_eq!(a(&run(vec![one])).strips_arcdps, 0);

        let mut two = removal(STABILITY, ENEMY, PLAYER_A);
        two.iff = iff::FOE;
        two.result = 2;
        assert_eq!(a(&run(vec![two])).strips_arcdps, 1);
    }

    /// Rule 2: blind you burned off yourself with your own next attack is
    /// not a cleanse. Blind removed by ANYONE else still is.
    #[test]
    fn self_consumed_blind_is_not_a_cleanse() {
        assert_eq!(
            a(&run(vec![removal(BLIND, PLAYER_A, PLAYER_A)])).cleanses_arcdps,
            0,
            "self-consumed blind must not count"
        );
        let p = run(vec![removal(BLIND, PLAYER_A, PLAYER_B)]);
        let b = p.iter().find(|p| p.agent_addr == PLAYER_B).unwrap().support;
        assert_eq!(
            b.cleanses_arcdps, 1,
            "blind cleansed by someone else does count"
        );
    }

    /// Rule 3: the reference's `if (dst_iid && src_iid)` -- a removal row
    /// with no credited remover counts for nobody.
    #[test]
    fn removal_with_zero_remover_counts_for_nobody() {
        let p = run(vec![removal(BLEEDING, PLAYER_A, 0)]);
        assert_eq!(a(&p).cleanses_arcdps, 0);
        assert_eq!(a(&p).cleanses_arcdps_by_minion, 0);
    }

    /// Rule 5 + bucketing: a cleanse landed ON a pet is credited to the
    /// remover, but into the "vs npcs" bucket rather than the base.
    #[test]
    fn cleanse_on_a_pet_goes_to_the_vs_npcs_bucket() {
        let mut e = removal(BLEEDING, PET_A, PLAYER_B);
        e.src_master_instid = PLAYER_A_INSTID;
        let p = run(vec![e]);
        let b = p.iter().find(|p| p.agent_addr == PLAYER_B).unwrap().support;
        assert_eq!(b.cleanses_arcdps, 0);
        assert_eq!(b.cleanses_arcdps_on_minion, 1);
    }

    /// Rule 5 + bucketing: a cleanse performed BY a pet folds onto its
    /// master (EI drops it entirely) and lands in the "from npcs" bucket.
    #[test]
    fn cleanse_by_a_pet_folds_onto_its_master() {
        let mut e = removal(BLEEDING, PLAYER_B, PET_A);
        e.dst_instid = PET_A_INSTID;
        e.dst_master_instid = PLAYER_A_INSTID;
        let p = run(vec![e]);
        assert_eq!(a(&p).cleanses_arcdps, 0);
        assert_eq!(
            a(&p).cleanses_arcdps_by_minion,
            1,
            "credited to the master, not the pet"
        );
    }

    /// Rule 5: the recipient side is decided by the row's own `iff` byte --
    /// a condition removed off a FOE is not a cleanse.
    #[test]
    fn condition_removed_off_a_foe_is_not_a_cleanse() {
        let mut e = removal(BLEEDING, ENEMY, PLAYER_A);
        e.iff = iff::FOE;
        assert_eq!(a(&run(vec![e])).cleanses_arcdps, 0);
    }

    /// Rule 6: the condition dump a player takes on going down is taken
    /// back out of their self-cleanse credit.
    #[test]
    fn down_undoes_the_self_removal_burst() {
        let mut burst_a = removal(BLEEDING, PLAYER_A, PLAYER_A);
        burst_a.time = 5_000;
        let mut burst_b = removal(WEAKNESS, PLAYER_A, PLAYER_A);
        burst_b.time = 5_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 5_001,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        let p = run(vec![burst_a, burst_b, down]);
        assert_eq!(a(&p).cleanses_arcdps, 0, "both burst rows taken back");
    }

    /// Rule 6's once-only guard: the reference dedupes by zeroing the row's
    /// `dst_agent`, which never reaches the log -- two downs in quick
    /// succession must not take the same burst back twice.
    #[test]
    fn two_downs_do_not_take_the_same_burst_back_twice() {
        let mut burst = removal(BLEEDING, PLAYER_A, PLAYER_A);
        burst.time = 5_000;
        let down = |t: u64| RawEvent {
            src_agent: PLAYER_A,
            time: t,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        // A second self-cleanse well clear of the window, so a double
        // take-back would be visible as 0 rather than clamped by saturation.
        let mut later = removal(WEAKNESS, PLAYER_A, PLAYER_A);
        later.time = 50_000;
        let p = run(vec![burst, down(5_001), down(5_002), later]);
        assert_eq!(
            a(&p).cleanses_arcdps,
            1,
            "burst taken back once; the later cleanse survives"
        );
    }

    /// Rule 6 is window-bounded: a self-cleanse well before the down is
    /// ordinary play, not part of the down's dump.
    #[test]
    fn down_does_not_reach_back_past_the_window() {
        let mut earlier = removal(BLEEDING, PLAYER_A, PLAYER_A);
        earlier.time = 1_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 1_000 + DOWN_UNDO_WINDOW_MS + 1,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        assert_eq!(a(&run(vec![earlier, down])).cleanses_arcdps, 1);
    }

    /// The chain walk must step over buff damage. deltaconnected: arcdps'
    /// chain locks around the condi sim loop and a log does not, so buff
    /// damage ticks land between the server's buffremove rows. Breaking on
    /// one would truncate the burst.
    #[test]
    fn buff_damage_between_removals_does_not_break_the_chain() {
        let mut first = removal(BLEEDING, PLAYER_A, PLAYER_A);
        first.time = 5_000;
        // A bleed tick on A: a combat row carrying `buff`, not a removal.
        let tick = RawEvent {
            skillid: BLEEDING,
            src_agent: PLAYER_A,
            dst_agent: PLAYER_A,
            buff_dmg: 300,
            time: 5_000,
            ..ev()
        };
        let mut second = removal(WEAKNESS, PLAYER_A, PLAYER_A);
        second.time = 5_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 5_001,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        let p = run(vec![first, tick, second, down]);
        assert_eq!(
            a(&p).cleanses_arcdps,
            0,
            "both removals taken back across the tick"
        );
    }

    /// Statechange rows are not chain entries -- arcdps walks a buff-event
    /// chain that never contains them, while the log stream interleaves them
    /// freely. On the real fixture, breaking on `sc == 62` rows truncated 4 of
    /// 25 bursts to nothing.
    #[test]
    fn statechange_rows_do_not_break_the_chain() {
        let mut first = removal(BLEEDING, PLAYER_A, PLAYER_A);
        first.time = 5_000;
        let noise = RawEvent {
            src_agent: PLAYER_A,
            dst_agent: PLAYER_A,
            time: 5_000,
            is_statechange: 62,
            buff: 0,
            ..ev()
        };
        let mut second = removal(WEAKNESS, PLAYER_A, PLAYER_A);
        second.time = 5_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 5_001,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        let p = run(vec![first, noise, second, down]);
        assert_eq!(
            a(&p).cleanses_arcdps,
            0,
            "both removals taken back across the statechange"
        );
    }

    /// The chain still ends somewhere: a direct-damage row (a combat entry
    /// with no `buff`) is what the reference breaks on.
    #[test]
    fn direct_damage_ends_the_chain() {
        let mut earlier = removal(BLEEDING, PLAYER_A, PLAYER_A);
        earlier.time = 5_000;
        let hit = RawEvent {
            src_agent: PLAYER_A,
            dst_agent: PLAYER_A,
            value: 500,
            buff: 0,
            time: 5_000,
            ..ev()
        };
        let mut recent = removal(WEAKNESS, PLAYER_A, PLAYER_A);
        recent.time = 5_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 5_001,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        // Walking back: `recent` is taken back, then the hit ends the chain
        // and `earlier` survives as an ordinary self-cleanse.
        let p = run(vec![earlier, hit, recent, down]);
        assert_eq!(a(&p).cleanses_arcdps, 1);
    }

    /// Every "determined" id is walked through, not broken on -- 788 is the
    /// golem's, 762 is what WvW player downs carry.
    #[test]
    fn any_determined_id_is_walked_through() {
        for determined in DETERMINED_IDS {
            let mut burst = removal(BLEEDING, PLAYER_A, PLAYER_A);
            burst.time = 5_000;
            let marker = RawEvent {
                skillid: determined,
                src_agent: PLAYER_A,
                dst_agent: PLAYER_A,
                time: 5_000,
                ..ev()
            };
            let down = RawEvent {
                src_agent: PLAYER_A,
                time: 5_001,
                is_statechange: sc::CHANGE_DOWN,
                ..ev()
            };
            let p = run(vec![burst, marker, down]);
            assert_eq!(a(&p).cleanses_arcdps, 0, "id {determined} broke the chain");
        }
    }

    /// A down only takes back the DOWNED agent's own burst.
    #[test]
    fn down_does_not_touch_another_players_cleanses() {
        let mut theirs = removal(BLEEDING, PLAYER_B, PLAYER_B);
        theirs.time = 5_000;
        let down = RawEvent {
            src_agent: PLAYER_A,
            time: 5_001,
            is_statechange: sc::CHANGE_DOWN,
            ..ev()
        };
        let p = run(vec![theirs, down]);
        let b = p.iter().find(|p| p.agent_addr == PLAYER_B).unwrap().support;
        assert_eq!(b.cleanses_arcdps, 1);
    }

    /// `BUFF_REMOVE_SINGLE`/`MANUAL` rows feed neither meter (rule 1).
    #[test]
    fn only_remove_all_rows_count() {
        let mut single = removal(BLEEDING, PLAYER_A, PLAYER_B);
        single.is_buffremove = buff_remove::SINGLE;
        let mut manual = removal(BLEEDING, PLAYER_A, PLAYER_B);
        manual.is_buffremove = buff_remove::MANUAL;
        let p = run(vec![single, manual]);
        let b = p.iter().find(|p| p.agent_addr == PLAYER_B).unwrap().support;
        assert_eq!(b.cleanses_arcdps, 0);
    }
}
