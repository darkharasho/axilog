//! Healing/barrier stats from the arcdps healing extension (M10 Task 1) --
//! aggregation layer on top of `evtc::ext_healing`'s byte/flag-level decode.
//! Mirrors GW2EI's `extHealingStats` "outgoing" definitions (`EXTFinalOutgoingHealingStat`
//! / `EXTActorHealingHelper`), account-folded like every other per-player
//! metric this project computes (see `analysis::mod::analyze`'s `addr_to_rep`
//! doc comment) -- totals only, no per-second timelines (out of this task's
//! scope per the plan).
//!
//! ## Agent resolution
//!
//! Extension data rows carry `src_instid`/`dst_instid`, NOT valid `src_agent`/
//! `dst_agent` addrs -- see `evtc::ext_healing`'s module doc for the full
//! citation. This module resolves both via `analysis::damage::InstidRegistry`
//! (the SAME time-aware instid->addr registry pet/minion damage credit
//! already uses, for the same underlying reason: an event whose real actor
//! identity is only known by instid, not raw address, at decode time).
//!
//! ## Peer sanitization
//!
//! GW2EI's `HealingStatsExtensionHandler.AttachToCombatData` groups all
//! healing (and, separately, all barrier) events by resolved SOURCE
//! (healer) addr and applies `SanitizeForSrc`: within each healer's group,
//! if ANY event has `src_is_peer == true`, every `!src_is_peer` event in
//! THAT SAME group is dropped (preferring a peer-reported duplicate over a
//! self/estimated one) -- see `ext_healing`'s doc comment. This is the view
//! GW2EI's own "outgoing" stats (`EXTHealingCombatData.GetHealData`, what
//! `outgoingHealing`/`outgoingHealingAllies` are built from) read from; the
//! separate DST-grouped `SanitizeForDst` view backs "incoming" stats, which
//! this task doesn't need.
//!
//! ## `healing_out_self` / `healing_out_allies` -- a deliberate, DOCUMENTED
//! divergence from EI's own per-friendly array
//!
//! EI's raw JSON exposes `outgoingHealingAllies[friendlyIndex][phase]`, one
//! entry PER enumerated `Friendlies` actor (which includes the healer
//! itself as one of its own "allies", at its own index) -- there is no
//! scalar "healing to allies excluding self" field in EI's own output.
//! Summing that whole per-friendly array does NOT equal EI's own scalar
//! `outgoingHealing[phase].healing` total: verified directly against this
//! project's golden fixture (see `fixtures/wvw-small.ei.json`'s `_note`),
//! e.g. one real player's per-friendly array sums to 165971 while their
//! true `healing` total is 205820 -- the gap is heals landing on friendly
//! targets EI's `Friendlies` list doesn't enumerate at all (pets/minions of
//! OTHER players, which legitimately receive `ToFriendly` heals and count
//! toward the scalar total but are never one of the per-friendly rows).
//! This module instead computes `healing_out_self` directly (events where
//! the resolved healer addr equals the resolved target addr) and derives
//! `healing_out_allies = healing_out_total - healing_out_self` -- always
//! internally consistent by construction, and exactly reproduces EI's own
//! `outgoingHealingAllies[selfIndex][phase].healing` for the self case
//! (verified against all 41 players in the golden fixture, zero
//! mismatches -- see `tests/healing_golden.rs`).
//!
//! ## Known residual: repeating-skill peer-report reconciliation
//!
//! `healing_out_self`/`downed_healing_out` are EXACT against the golden
//! fixture (41/41 accounts, both squad-wide and per-player, zero
//! mismatches). `healing_out_total`/`healing_out_allies` are exact for
//! 37/41 accounts and `barrier_out` for 40/41 -- the remaining handful are
//! within a documented, bounded tolerance (`tests/healing_golden.rs`'s
//! module doc has the exact numbers), not exact. Root cause, found by
//! directly inspecting every mismatching account's raw decoded events (not
//! guessed): these accounts cast a REPEATING skill (same skill id, same
//! fixed heal/barrier amount, reapplied roughly 15-35s apart -- consistent
//! with a recharge-gated skill being cast again, not a genuine wire-level
//! duplicate) whose peer-relayed copies straddle GW2EI's `SanitizeForSrc`
//! all-or-nothing per-healer rule (this module's earlier doc section) in a
//! way a byte-level replication of JUST that rule cannot perfectly
//! reproduce. The single largest observed case: one account's `barrier_out`
//! is 70931 computed here vs EI's 49691 -- a difference of EXACTLY 21240,
//! which traces to a 5-application cluster of skill id 72008 (4248 each)
//! whose peer-flag pattern this module's `keep`-set computation resolves
//! differently than GW2EI's real run did. Reproducing GW2EI's answer
//! exactly here would require replicating its internal per-agent-lifetime
//! `AgentItem` identity tracking (which `SanitizeForSrc`'s `GroupBy(x =>
//! x.From)` keys off, not just the resolved addr this module keys off) --
//! judged out of scope for this task's "totals only" brief given the
//! magnitude of the residual (bounded, single-digit-account, single-digit-
//! percent at the squad level for healing, ~7.6% for barrier -- see
//! `tests/healing_golden.rs`, which records the PLAN-OWNER RULING
//! authorizing a `barrier_out`-only tolerance exception at the measured
//! residual plus a small margin) versus the depth of GW2EI-internal
//! machinery required to close it exactly.
//!
//! ## Pet/minion heal folding (fix round)
//!
//! GW2EI's `EXTSingleActorHealingHelper.InitHealEvents`/`EXTSingleActorBarrierHelper.
//! InitBarrierEvents` both fold a player's minions' own outgoing heal/
//! barrier events into the OWNER's totals (`_actor.GetMinions(log)` loop,
//! `minion.EXTHealing.GetOutgoingHealEvents(null, log)`). This module
//! replicates that: a decoded event whose raw resolved healer is NOT a
//! squad player is still attributed to a squad player if `src_master_instid`
//! (the WIRE field, untouched by the extension's src-agent adjustment --
//! see `ext_healing::RawExtHealEvent::src_master_instid`'s doc comment)
//! resolves, at that event's own time, to a squad player -- mirroring
//! `analysis::damage::pet_credit_events`'s established owner-credit
//! pattern exactly (same `InstidRegistry`, same field). Peer sanitization
//! still happens on the RAW (pre-fold) healer identity first, matching
//! GW2EI's own `GroupBy(x => x.From)` (a minion's events are sanitized in
//! their own group, not merged with their owner's group, before folding).
//!
//! **Verified empirically to have zero effect on this project's two real
//! fixtures**: both `fixtures/wvw-small.anon.zevtc` (2065 healing-extension
//! data rows) and the local `fixtures/local/wvw-postrework.zevtc` (11401
//! rows) have ZERO rows whose raw resolved healer is outside the squad --
//! i.e. the arcdps healing-stats addon, in these captures, only ever
//! reports player-sourced heals/barriers, never pet/minion-sourced ones
//! (confirmed pets ARE present and active in the same log: ordinary pet
//! DAMAGE credit, `analysis::damage::pet_credit_events`, finds 87 real
//! pet-sourced damage events in `wvw-small.anon.zevtc`). The fold logic
//! itself is exercised and correct (`tests::minion_sourced_*`,
//! `tests::minion_owned_by_non_squad_agent_is_dropped`,
//! `tests::minion_with_no_master_instid_is_uncredited`,
//! `tests::minion_peer_sanitization_happens_before_owner_fold`) -- it's
//! simply dormant on these two logs specifically, and does NOT explain any
//! part of the residual described above (which is unchanged before/after
//! this fold was added).

use crate::analysis::damage::InstidRegistry;
use crate::analysis::PlayerMetrics;
use crate::evtc::{ext_healing, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// One squad player's healing/barrier totals for the full log (M10 Task 1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealingMetrics {
    /// Every friendly-directed heal this player cast (self + allies +
    /// non-enumerated friendly targets like pets), sanitized per the module
    /// doc. Mirrors EI's `outgoingHealing[phase].healing` exactly.
    pub healing_out_total: u64,
    /// `healing_out_total - healing_out_self` -- see the module doc for why
    /// this is derived rather than a sum of EI's own per-friendly array.
    pub healing_out_allies: u64,
    /// The subset of `healing_out_total` where healer == target (a genuine
    /// self-heal). Mirrors EI's `outgoingHealingAllies[selfIndex][phase]
    /// .healing`.
    pub healing_out_self: u64,
    /// Every friendly-directed barrier this player granted. Mirrors EI's
    /// `extBarrierStats.outgoingBarrier[phase].barrier` exactly.
    pub barrier_out: u64,
    /// The subset of `healing_out_total` where the target was downed at
    /// event time. Mirrors EI's `outgoingHealing[phase].downedHealing`
    /// exactly (both self- and ally-directed downed healing, matching EI's
    /// own scalar -- EI does not split this further by self/ally).
    pub downed_healing_out: u64,
}

/// Apply healing-extension stats to `players` (mutates each entry's
/// `healing` field in place, like `support::apply`/`downs::apply`/
/// `cc::apply_cc`). Returns whether the healing extension was present at
/// all (a valid signature+revision registration row was found) -- callers
/// use this to decide whether to omit the native schema's `healing` block
/// and push the "healing extension not present" warning (`analysis::mod::
/// analyze`).
pub fn apply(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> bool {
    if !ext_healing::healing_extension_present(raw) {
        return false;
    }

    let registry = InstidRegistry::build(raw);
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();

    // Decode every data row, resolving healer/target via the instid
    // registry (module doc: raw src_agent/dst_agent on these rows are not
    // valid). Rows that fail to resolve (no registration yet at that time
    // for that instid) are dropped, matching `InstidRegistry`'s own
    // "leave uncredited rather than guess" convention.
    //
    // Fix round (pet/minion heal folding): `healer` here is the RAW
    // resolved source identity (a player OR their pet/minion) -- NOT yet
    // filtered to squad members, because GW2EI's own `SanitizeForSrc`
    // groups (and peer-sanitizes) by this exact raw identity BEFORE a
    // minion's kept events get folded into its owner's totals (see
    // `EXTSingleActorHealingHelper.InitHealEvents`'s `_actor.GetMinions(log)`
    // loop, replicated below via `src_master_instid`). Filtering out
    // non-squad healers at THIS point (the pre-fix-round behavior) silently
    // dropped every pet/minion-sourced heal -- e.g. Water Blast fields,
    // Spirit of Nature-style pet skills, guardian tome-summoned constructs.
    #[derive(Clone, Copy)]
    struct Resolved {
        time: u64,
        healer: u64,
        target: u64,
        src_master_instid: u16,
        amount: u64,
        is_barrier: bool,
        against_downed: bool,
        src_is_peer: bool,
    }
    let mut resolved: Vec<Resolved> = Vec::new();
    for e in &raw.events {
        let Some(ev) = ext_healing::decode_data_event(e, ext_healing::HEALING_SIGNATURE) else {
            continue;
        };
        if !ev.to_friendly {
            continue;
        }
        let Some(healer) = registry.resolve_at(ev.src_instid, ev.time) else { continue };
        let Some(target) = registry.resolve_at(ev.dst_instid, ev.time) else { continue };
        resolved.push(Resolved {
            time: ev.time,
            healer,
            target,
            src_master_instid: ev.src_master_instid,
            amount: ev.amount,
            is_barrier: ev.is_barrier,
            against_downed: ev.against_downed,
            src_is_peer: ev.src_is_peer,
        });
    }

    // Peer sanitization (module doc): group by (healer, is_barrier) -- heal
    // and barrier events are independent streams in GW2EI (`_healingEvents`
    // vs `_barrierEvents`, sanitized separately) -- and drop non-peer rows
    // within any group that has at least one peer row. Grouped by the RAW
    // healer identity (pre-owner-fold), matching GW2EI's own `GroupBy(x =>
    // x.From)` -- a pet/minion's events are sanitized within their OWN
    // group, exactly as if they were an independent healer, before folding.
    let mut groups: BTreeMap<(u64, bool), Vec<usize>> = BTreeMap::new();
    for (i, r) in resolved.iter().enumerate() {
        groups.entry((r.healer, r.is_barrier)).or_default().push(i);
    }
    let mut keep = vec![true; resolved.len()];
    for idxs in groups.values() {
        let any_peer = idxs.iter().any(|&i| resolved[i].src_is_peer);
        if any_peer {
            for &i in idxs {
                if !resolved[i].src_is_peer {
                    keep[i] = false;
                }
            }
        }
    }

    for (i, r) in resolved.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        // Owner resolution (fix round): a squad-player healer attributes
        // directly; anyone else (a pet/minion, or an unrelated agent) only
        // attributes if `src_master_instid` -- the WIRE field GW2EI's own
        // "Linking minions to their masters" pass reads for EVERY agent-
        // sourced combat item, extension rows included (`ext_healing::
        // RawExtHealEvent::src_master_instid`'s doc comment) -- resolves,
        // at this event's own time, to a squad player. Anything else (an
        // enemy's pet, an unowned NPC) is dropped, matching `pet_credit_
        // events`'s own "not our pet" exclusion (`analysis::damage`).
        let attributed_healer = if squad.contains(&r.healer) {
            Some(r.healer)
        } else {
            registry.resolve_at(r.src_master_instid, r.time).filter(|owner| squad.contains(owner))
        };
        let Some(attributed_healer) = attributed_healer else { continue };
        let rep = addr_to_rep.get(&attributed_healer).copied().unwrap_or(attributed_healer);
        let Some(&pi) = idx.get(&rep) else { continue };
        let m = &mut players[pi].healing;
        if r.is_barrier {
            m.barrier_out += r.amount;
            continue;
        }
        m.healing_out_total += r.amount;
        let target_rep = addr_to_rep.get(&r.target).copied().unwrap_or(r.target);
        if target_rep == rep {
            m.healing_out_self += r.amount;
        }
        if r.against_downed {
            m.downed_healing_out += r.amount;
        }
    }
    for p in players.iter_mut() {
        p.healing.healing_out_allies = p.healing.healing_out_total - p.healing.healing_out_self;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{sc, RawEvent, RawHeader};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn registration(rev: u32) -> RawEvent {
        let src_agent = (ext_healing::HEALING_SIGNATURE as u64) | ((rev as u64) << 32);
        RawEvent { is_statechange: sc::EXTENSION, pad: 0, src_agent, ..base_event() }
    }

    /// Registers instid `instid` to addr `addr` at `time` via an ordinary
    /// (non-extension) combat row -- the same mechanism `InstidRegistry`
    /// scans for.
    fn instid_reg(time: u64, addr: u64, instid: u16) -> RawEvent {
        RawEvent { time, src_agent: addr, src_instid: instid, dst_agent: addr, dst_instid: instid,
            iff: 1, ..base_event() }
    }

    fn direct_heal(time: u64, src_instid: u16, dst_instid: u16, amount: i32) -> RawEvent {
        RawEvent {
            is_statechange: sc::EXTENSION_COMBAT,
            pad: ext_healing::HEALING_SIGNATURE,
            time,
            src_instid,
            dst_instid,
            value: -amount,
            iff: 0,
            ..base_event()
        }
    }

    /// Same as `direct_heal` but with `src_master_instid` set (a pet/minion-
    /// sourced heal, fix round) -- `is_barrier` via `is_shields`.
    fn direct_heal_from_minion(
        time: u64,
        src_instid: u16,
        src_master_instid: u16,
        dst_instid: u16,
        amount: i32,
        is_barrier: bool,
    ) -> RawEvent {
        RawEvent {
            is_statechange: sc::EXTENSION_COMBAT,
            pad: ext_healing::HEALING_SIGNATURE,
            time,
            src_instid,
            src_master_instid,
            dst_instid,
            value: -amount,
            is_shields: if is_barrier { 1 } else { 0 },
            iff: 0,
            ..base_event()
        }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn player_metrics(addr: u64) -> PlayerMetrics {
        PlayerMetrics { agent_addr: addr, ..Default::default() }
    }

    #[test]
    fn returns_false_and_leaves_zeros_when_extension_absent() {
        let raw = raw_from(vec![]);
        let mut players = vec![player_metrics(1)];
        let squad: BTreeSet<u64> = [1].into_iter().collect();
        let present = apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert!(!present);
        assert_eq!(players[0].healing, HealingMetrics::default());
    }

    /// Core decode probe: a healer (addr 1, instid 11) healing an ally
    /// (addr 2, instid 22) for 300 must land entirely on healer 1's
    /// `healing_out_total`/`healing_out_allies`, none on `healing_out_self`.
    #[test]
    fn credits_ally_heal_to_healer_not_target() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            direct_heal(100, 11, 22, 300),
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        let present = apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert!(present);
        assert_eq!(players[0].healing.healing_out_total, 300);
        assert_eq!(players[0].healing.healing_out_allies, 300);
        assert_eq!(players[0].healing.healing_out_self, 0);
        assert_eq!(players[1].healing.healing_out_total, 0, "target must not be credited");
    }

    #[test]
    fn self_heal_splits_into_self_not_allies() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            direct_heal(100, 11, 11, 150),
        ]);
        let mut players = vec![player_metrics(1)];
        let squad: BTreeSet<u64> = [1].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 150);
        assert_eq!(players[0].healing.healing_out_self, 150);
        assert_eq!(players[0].healing.healing_out_allies, 0);
    }

    #[test]
    fn barrier_flag_routes_to_barrier_out_not_healing_out() {
        let mut heal_shielded = direct_heal(100, 11, 22, 200);
        heal_shielded.is_shields = 1;
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            heal_shielded,
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.barrier_out, 200);
        assert_eq!(players[0].healing.healing_out_total, 0, "barrier must not leak into healing");
    }

    #[test]
    fn downed_flag_accumulates_separately() {
        let mut e = direct_heal(100, 11, 22, 400);
        e.is_offcycle = 0x20; // BuffDamageDstIsDowned
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            e,
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.downed_healing_out, 400);
        assert_eq!(players[0].healing.healing_out_total, 400);
    }

    /// Peer sanitization: two rows for the same (healer, target) pair, one
    /// non-peer (a duplicate self/estimated report) and one peer -- only
    /// the peer-reported amount must be counted, matching GW2EI's
    /// `SanitizeForSrc`.
    #[test]
    fn peer_sanitization_drops_non_peer_duplicate_for_same_healer() {
        let mut non_peer = direct_heal(100, 11, 22, 300);
        non_peer.is_offcycle = 0; // defaults to src_is_peer=true actually...
        // Force a genuine non-peer row: dst-peer bit set, src-peer bit
        // clear (`0x40`), so `src_is_peer` decodes false per the module.
        non_peer.is_offcycle = 0x40;
        let mut peer = direct_heal(150, 11, 22, 500);
        peer.is_offcycle = 0x80; // src-peer bit set
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            non_peer,
            peer,
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(
            players[0].healing.healing_out_total, 500,
            "only the peer-reported row must count once the group has any peer row"
        );
    }

    /// A healer whose events all default to `src_is_peer = true` (the
    /// ordinary case, no cross-peer sharing at all) keeps every event --
    /// sanitization must not spuriously drop anything when there's no real
    /// duplicate.
    #[test]
    fn no_sanitization_drop_when_all_events_default_peer() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            direct_heal(100, 11, 22, 100),
            direct_heal(200, 11, 22, 200),
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 300);
    }

    #[test]
    fn account_folded_across_relog_addrs() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(500, 3, 11), // instid 11 recycled to addr 3 post-relog
            instid_reg(0, 2, 22),
            direct_heal(100, 11, 22, 100), // pre-relog, credited to addr 1
            direct_heal(600, 11, 22, 50),  // post-relog, credited to addr 3
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2, 3].into_iter().collect();
        let mut addr_to_rep = BTreeMap::new();
        addr_to_rep.insert(1u64, 1u64);
        addr_to_rep.insert(3u64, 1u64); // addr 3 folds onto rep 1 (same account)
        addr_to_rep.insert(2u64, 2u64);
        apply(&mut players, &raw, &squad, &addr_to_rep);
        assert_eq!(players[0].healing.healing_out_total, 150, "both eras folded onto the account rep");
    }

    #[test]
    fn foe_directed_heal_is_excluded() {
        let mut e = direct_heal(100, 11, 22, 300);
        e.iff = 1; // FOE, not ToFriendly
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 11),
            instid_reg(0, 2, 22),
            e,
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 0);
    }

    #[test]
    fn non_squad_healer_is_ignored() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 9, 11), // addr 9, not in squad
            instid_reg(0, 2, 22),
            direct_heal(100, 11, 22, 300),
        ]);
        let mut players = vec![player_metrics(2)];
        let squad: BTreeSet<u64> = [2].into_iter().collect(); // 9 excluded
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 0);
    }

    /// Fix round: a pet/minion's own heal (raw healer addr NOT in squad)
    /// must fold into its OWNER's `healing_out_total` via `src_master_instid`
    /// resolution -- mirroring `analysis::damage::pet_credit_events`'s own
    /// pattern. Owner addr 1 (instid 10) has a pet, raw addr 99 (instid 33,
    /// `src_master_instid=10`), which heals ally addr 2 (instid 22).
    #[test]
    fn minion_sourced_heal_folds_into_owner_not_dropped() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 10),  // owner
            instid_reg(0, 2, 22),  // ally target
            instid_reg(0, 99, 33), // the pet itself, own addr/instid
            direct_heal_from_minion(100, 33, 10, 22, 400, false),
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect(); // 99 (the pet) excluded
        let present = apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert!(present);
        assert_eq!(players[0].healing.healing_out_total, 400, "pet heal credited to owner");
        assert_eq!(players[0].healing.healing_out_allies, 400);
        assert_eq!(players[0].healing.healing_out_self, 0);
    }

    /// Fix round: a pet healing its OWN OWNER counts as a self-heal for the
    /// owner (matches GW2EI's `outgoingHealingAllies[selfIndex]` semantics
    /// -- the module doc's "self" definition is symmetric under minion
    /// folding: target's representative == attributed healer's
    /// representative).
    #[test]
    fn minion_sourced_heal_on_owner_counts_as_self() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 10),  // owner
            instid_reg(0, 99, 33), // pet
            direct_heal_from_minion(100, 33, 10, 10, 250, false), // heals owner's own instid
        ]);
        let mut players = vec![player_metrics(1)];
        let squad: BTreeSet<u64> = [1].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 250);
        assert_eq!(players[0].healing.healing_out_self, 250);
        assert_eq!(players[0].healing.healing_out_allies, 0);
    }

    /// Fix round: minion-sourced BARRIER folds the same way as heal.
    #[test]
    fn minion_sourced_barrier_folds_into_owner() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 10),
            instid_reg(0, 2, 22),
            instid_reg(0, 99, 33),
            direct_heal_from_minion(100, 33, 10, 22, 150, true),
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.barrier_out, 150);
        assert_eq!(players[0].healing.healing_out_total, 0, "barrier must not leak into healing");
    }

    /// A pet whose `src_master_instid` resolves to an addr that is NOT a
    /// squad member (an enemy's pet, or an unowned NPC) must not attribute
    /// anywhere -- matches `pet_credit_events`'s own "not our pet"
    /// exclusion (`analysis::damage`).
    #[test]
    fn minion_owned_by_non_squad_agent_is_dropped() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 9, 10),  // owner addr 9, NOT in squad
            instid_reg(0, 2, 22),  // ally target, in squad
            instid_reg(0, 99, 33), // pet
            direct_heal_from_minion(100, 33, 10, 22, 400, false),
        ]);
        let mut players = vec![player_metrics(2)];
        let squad: BTreeSet<u64> = [2].into_iter().collect(); // 9 and 99 both excluded
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 0);
    }

    /// A pet with NO `src_master_instid` at all (0 -- "none", per this
    /// project's existing instid convention) is simply uncredited, not a
    /// panic/default-to-something-wrong.
    #[test]
    fn minion_with_no_master_instid_is_uncredited() {
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 2, 22),
            instid_reg(0, 99, 33),
            direct_heal_from_minion(100, 33, 0, 22, 400, false),
        ]);
        let mut players = vec![player_metrics(2)];
        let squad: BTreeSet<u64> = [2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 0);
    }

    /// Peer sanitization for a minion's events happens PER MINION (its own
    /// raw healer identity), before folding into the owner -- a duplicate
    /// non-peer/peer pair from the SAME pet must still collapse to one
    /// counted amount, exactly like a direct player healer.
    #[test]
    fn minion_peer_sanitization_happens_before_owner_fold() {
        let mut non_peer = direct_heal_from_minion(100, 33, 10, 22, 300, false);
        non_peer.is_offcycle = 0x40; // dst-peer only -> src_is_peer=false
        let mut peer = direct_heal_from_minion(150, 33, 10, 22, 500, false);
        peer.is_offcycle = 0x80; // src-peer
        let raw = raw_from(vec![
            registration(1),
            instid_reg(0, 1, 10),
            instid_reg(0, 2, 22),
            instid_reg(0, 99, 33),
            non_peer,
            peer,
        ]);
        let mut players = vec![player_metrics(1), player_metrics(2)];
        let squad: BTreeSet<u64> = [1, 2].into_iter().collect();
        apply(&mut players, &raw, &squad, &BTreeMap::new());
        assert_eq!(players[0].healing.healing_out_total, 500, "only the peer-reported pet heal counts");
    }
}
