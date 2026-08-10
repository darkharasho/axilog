use crate::analysis::damage::InstidRegistry;
use crate::analysis::{PlayerMetrics, Timeline};
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

pub fn timeline(
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> Timeline {
    timeline_with_registry(enc, raw, &InstidRegistry::build(raw), squad, enemies)
}

/// [`timeline`] against a caller-supplied, already-built [`InstidRegistry`]
/// (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for standalone/test callers.
pub fn timeline_with_registry(
    enc: &Encounter,
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> Timeline {
    let res = 1000u64;
    let buckets = ((enc.duration_ms / res) + 1) as usize;
    let mut squad_damage = vec![0u64; buckets];
    let mut cc_applied = vec![0u32; buckets];
    let mut downs = vec![0u32; buckets];
    let t0 = raw.events.first().map(|e| e.time).unwrap_or(0);
    // M4 Task 2: era-gate `is_cc` (see its doc comment) -- post-rework logs
    // route genuine CC through `buff == 1` rows too, via the shared
    // `DamageResult` enum.
    let post_era = raw.header.is_post_buff_rework();

    // WvW: fold friendly pet/minion damage into the same per-second buckets
    // as direct squad damage, matching how `analysis::analyze` credits it
    // to owning players — otherwise sum(squad_damage) undercounts
    // sum(player.damage_total) by the pet-credit share (Finding #4).
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (time, _owner, _dst, dmg) in crate::analysis::damage::pet_credit_events_with_registry(
        raw,
        registry,
        squad,
        friendly_team,
        &agent_team,
    ) {
        let rel = time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b < buckets {
            squad_damage[b] += dmg;
        }
    }

    for e in &raw.events {
        let rel = e.time.saturating_sub(t0);
        let b = (rel / res) as usize;
        if b >= buckets {
            continue;
        }
        if e.is_statechange == 0
            && e.is_activation == 0
            && e.is_buffremove == 0
            // Crowd-control application events reuse value/buff_dmg to
            // carry CC duration ms, not damage — exclude them so the
            // timeline matches `damage::accumulate`'s predicate exactly
            // (Finding #4). Unlike `is_cc` below, this exclusion is NOT
            // conditioned on `buff` at all -- it already drops every
            // `result == CROWD_CONTROL` row regardless of the buff flag
            // (M4 Task 2, verified against GW2EI's `AddDirectDamageEvent`/
            // `AddBuffDamageDamageEvent`: both branches route
            // `DamageResult.CrowdControl` to a `CrowdControlEvent`, never a
            // `HealthDamageEvent`, on EVERY era -- pre-rework, a buff==1 row
            // with raw result byte 12 decodes as the OLD `ConditionResult`
            // enum instead, which has no value 12 at all and maps to
            // `Unknown` -- i.e. "not health damage" either, so this
            // unconditional exclusion was already correct pre-era too, just
            // for a different underlying reason). So this predicate needs
            // NO era dispatch, unlike `is_cc`/`pet_credit_cc_events` below.
            && crate::analysis::damage::is_health_damage_result(e.result)
            && squad.contains(&e.src_agent)
            && enemies.contains(&e.dst_agent)
        {
            let d = if e.buff == 1 {
                e.buff_dmg.max(0) as u64
            } else {
                e.value.max(0) as u64
            };
            squad_damage[b] += d;
        }
        if e.is_statechange == 0
            && e.result == crate::evtc::result::DOWNED
            && enemies.contains(&e.dst_agent)
        {
            downs[b] += 1;
        }
        if is_cc(e, post_era) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            cc_applied[b] += 1;
        }
    }
    Timeline { resolution_ms: res, squad_damage, cc_applied, downs }
}

/// CC application predicate (Task 3, M2; era-gated M4 Task 2): a
/// non-statechange combat event whose `result` is `CROWD_CONTROL` —
/// `value` carries the CC duration in ms (see `result::CROWD_CONTROL`
/// docs, and `CrowdControlEvent.cs`: `Duration = evtcItem.Value`,
/// unaffected by the buff flag or era). Replaces the earlier
/// overstack-based heuristic (Task 11).
///
/// **Pre-era (`post_era == false`), the `buff == 0` check matters**: on
/// pre-`ResultEnumRework` arcdps builds (< 20260501), `CombatItem.
/// IsBuffDamageEvent`'s dispatch (`GW2EIEvtcParser/CombatItem.cs`) routes
/// `buff == 1` rows through `CombatEventFactory.AddBuffDamageDamageEvent`'s
/// PRE-`ResultEnumRework` branch, which decodes the result byte as the
/// separate, now-retired `ConditionResult` enum (`ArcDPSEnums.cs`:
/// `ExpectedToHit=0..InvulByPlayerSkill3=4, Unknown=5+`) — this enum has NO
/// value 12 at all, so byte value 12 there means "`Unknown`" (i.e. "not
/// health damage", but also NOT a `CrowdControlEvent` — GW2EI creates
/// nothing for it), spuriously colliding with `DamageResult.CrowdControl`
/// (`= 12`) if this predicate didn't guard on `buff == 0`. Debugged against
/// the golden WvW fixture (build 20260114, i.e. pre-rework): without
/// `buff == 0`, this collision surfaces on ordinary boon-stack application
/// events (Might/Stability/Fury/Vulnerability/Resolution), producing
/// nonsensical multi-minute "CC durations" — those are a different field
/// entirely, not CC.
///
/// **Post-era (`post_era == true`, arcdps `>= 20260501`), genuine CC CAN
/// arrive as `buff == 1`**: verified against GW2EI's post-`ResultEnumRework`
/// branch of `AddBuffDamageDamageEvent` (`CombatEventFactory.cs:857-881`),
/// which — unlike the pre-era branch above — decodes the result byte
/// through the SAME shared `DamageResult` enum `AddDirectDamageEvent`
/// (`buff == 0`) already uses, and routes `DamageResult.CrowdControl` to
/// `AddNonDamageDamageEvent` → a genuine `CrowdControlEvent`, exactly like a
/// `buff == 0` CC row (`CombatItem.IsBuffDamageEvent`'s own post-era branch,
/// `CombatItem.cs:226-238`, also drops the pre-era `Value == 0` guard, so a
/// buff-shaped row's `result` byte alone decides its fate post-era). This
/// resolves the M3 TODO this doc comment previously carried. So post-era,
/// this predicate must NOT require `buff == 0` — any `is_statechange == 0`
/// row with `result == CROWD_CONTROL` is real CC, regardless of the buff
/// flag.
///
/// **Damage-leak safety, verified**: post-era buff==1 CC rows must NOT also
/// leak into condi-damage accounting. Every damage/timeline/down-contribution
/// predicate in this codebase (`damage::accumulate`, `damage::
/// pet_credit_events`/`accumulate_pet_credit`, `damage::
/// accumulate_damage_taken`, `cc::timeline`'s own `squad_damage` loop above,
/// `downs::apply`'s down-contribution window) already excludes
/// `result == CROWD_CONTROL` UNCONDITIONALLY (never gated on `buff` at all)
/// — so they were already era-safe before this task with no code changes
/// needed; see `timeline`'s `squad_damage` loop doc comment above for the
/// full citation trail. Only THIS predicate (CC recognition, not damage
/// exclusion) needed era-gating.
///
/// Calibrated against the golden (pre-era) fixture: this predicate (combined
/// with pet-credit in `apply_cc`) reproduces EI's `appliedCrowdControl`/
/// `appliedCrowdControlDuration` squad totals within tolerance (34 /
/// 50460ms). `post_era` comes from `RawHeader::is_post_buff_rework` at each
/// call site.
pub(crate) fn is_cc(e: &crate::evtc::RawEvent, post_era: bool) -> bool {
    e.is_statechange == 0
        && (post_era || e.buff == 0)
        && e.result == crate::evtc::result::CROWD_CONTROL
}

/// Pet/minion-sourced CC application events credited to the owning squad
/// player: `(owner, dst, duration_ms)`. Mirrors `damage::pet_credit_events`'s
/// owner resolution (via `src_master_instid` looked up against the shared,
/// time-aware `damage::InstidRegistry` -- Task 4, M2) but selects `is_cc`
/// rows instead of excluding them, and reads the CC duration from `value`
/// instead of damage from `value`/`buff_dmg`.
///
/// Required to match EI: GW2EI's `SingleActor.InitOutgoingCrowdControlEvents`
/// folds each player's minions' `GetOutgoingCrowdControlEvents` into the
/// owning player's own outgoing-CC list (same as it does for damage) —
/// confirmed by reading GW2EI's `SingleActor.cs` on GitHub. Calibrated
/// against the golden WvW fixture (Task 3, M2): squad
/// `cc_applied`/`cc_duration_ms` sums only land within 2% of EI's golden
/// 34 / 50460ms when pet-sourced CC is included; excluding it undercounts.
///
/// Unlike `damage::pet_credit_events`, this DOES restrict `dst_agent` to
/// the `enemies` set (rather than relying solely on `iff`). Debugged
/// against the golden fixture: some pet-sourced CC rows carry `dst_agent ==
/// 0` (periodic/self ground-effect ticks with no real target — e.g. a
/// ranger pet's repeating ~280ms-interval skill), which pass the `iff`
/// check but were never "CC applied to an enemy" and aren't in EI's count.
/// Restricting to `enemies` drops those and reproduces the golden fixture
/// exactly (34 / 50460ms, not just within tolerance).
fn pet_credit_cc_events(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &std::collections::BTreeMap<u64, u32>,
) -> Vec<(u64, u64, u64)> {
    let post_era = raw.header.is_post_buff_rework();
    let mut out = Vec::new();
    for e in &raw.events {
        if !is_cc(e, post_era) { continue; }
        if !enemies.contains(&e.dst_agent) { continue; } // must be a real, known enemy
        if e.iff == 0 { continue; } // FRIEND: not CC applied to an enemy
        if squad.contains(&e.src_agent) { continue; } // real players: handled directly below
        if agent_team.get(&e.src_agent).copied() != friendly_team { continue; } // not our pet
        let owner = match registry.resolve_at(e.src_master_instid, e.time) {
            Some(addr) if squad.contains(&addr) => addr,
            _ => continue,
        };
        out.push((owner, e.dst_agent, e.value.max(0) as u64));
    }
    out
}

pub fn apply_cc(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &std::collections::BTreeMap<u64, u64>,
) {
    apply_cc_with_registry(
        players,
        raw,
        &InstidRegistry::build(raw),
        squad,
        enemies,
        addr_to_rep,
    );
}

/// [`apply_cc`] against a caller-supplied, already-built [`InstidRegistry`]
/// (MPERF Task 2) -- see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]'s doc
/// comment for why the registry is threaded rather than rebuilt per
/// consumer. The `raw`-only wrapper above stays for standalone/test callers.
pub fn apply_cc_with_registry(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &std::collections::BTreeMap<u64, u64>,
) {
    use std::collections::BTreeMap;
    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);
    let post_era = raw.header.is_post_buff_rework();

    // Direct (player-sourced) CC applied to an enemy.
    for e in &raw.events {
        if is_cc(e, post_era) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].cc_applied += 1;
                players[i].cc_duration_ms += e.value.max(0) as u64;
            }
        }
    }

    // Pet/minion-sourced CC, credited to the owning squad player (see
    // `pet_credit_cc_events` docs — required to match EI).
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, _dst, duration_ms) in
        pet_credit_cc_events(raw, registry, squad, enemies, friendly_team, &agent_team)
    {
        if let Some(&i) = idx.get(&rep(owner)) {
            players[i].cc_applied += 1;
            players[i].cc_duration_ms += duration_ms;
        }
    }

    // CBTS_STUNBREAK: keyed by `src_agent`, the agent whose stun broke
    // early (see `sc::STUN_BREAK` docs). `value` is the remaining stun
    // duration (ms) that was cancelled.
    for e in &raw.events {
        if e.is_statechange == crate::evtc::sc::STUN_BREAK && squad.contains(&e.src_agent) {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                players[i].stun_breaks += 1;
                players[i].removed_stun_duration_ms += e.value.max(0) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::Encounter;
    use std::collections::BTreeSet;
    fn dmg(time: u64, src: u64, dst: u64, v: i32) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: dst,
            value: v,
            buff_dmg: 0,
            overstack: 0,
            skillid: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 1,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: 0,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }
    #[test]
    fn buckets_squad_damage_per_second() {
        let enc = Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 2500,
            build: "".into(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![],
            enemies: vec![],
            markers: vec![],
            tick_rate: None,
        };
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![dmg(100, 1, 9, 50), dmg(1200, 1, 9, 70), dmg(2400, 1, 9, 30)],
            guid_map: vec![],
        };
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let tl = timeline(&enc, &raw, &squad, &enemies);
        assert_eq!(tl.resolution_ms, 1000);
        assert_eq!(tl.squad_damage.len(), 3); // seconds 0,1,2
        assert_eq!(tl.squad_damage[0], 50);
        assert_eq!(tl.squad_damage[1], 70);
        assert_eq!(tl.squad_damage[2], 30);
    }

    fn cc_ev(time: u64, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        let mut e = dmg(time, src, dst, duration_ms);
        e.result = crate::evtc::result::CROWD_CONTROL;
        e
    }
    fn team_change(agent: u64, team_id: i32) -> RawEvent {
        let mut e = dmg(0, agent, 0, team_id);
        e.is_statechange = crate::evtc::sc::TEAM_CHANGE;
        e
    }
    fn pov(agent: u64) -> RawEvent {
        let mut e = dmg(0, agent, 0, 0);
        e.is_statechange = crate::evtc::sc::POINT_OF_VIEW;
        e
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

    #[test]
    fn apply_cc_credits_direct_events_and_sums_duration() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let raw = raw_from(vec![cc_ev(100, 1, 9, 1500), cc_ev(200, 1, 9, 500)]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 2);
        assert_eq!(players[0].cc_duration_ms, 2000);
    }

    #[test]
    fn apply_cc_ignores_non_cc_and_non_enemy_events() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        // Plain damage (not CC) and a CC event against a non-enemy dst.
        let raw = raw_from(vec![dmg(50, 1, 9, 999), cc_ev(100, 1, 2, 1500)]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 0);
        assert_eq!(players[0].cc_duration_ms, 0);
    }

    /// Pet/minion-sourced CC must be credited to the owning squad player
    /// (see `pet_credit_cc_events` docs — required to match EI's
    /// `appliedCrowdControl`, which folds minion CC into the owner).
    #[test]
    fn apply_cc_credits_pet_sourced_cc_to_owner() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut seed = dmg(50, 1, 9, 10); // registers agent 1's instid in the log-wide table
        seed.src_instid = 11;
        let mut pet_cc = cc_ev(100, 2, 9, 800); // pet agent 2, owned by agent 1
        pet_cc.src_instid = 22;
        pet_cc.src_master_instid = 11; // points back to owner's instid
        let raw = raw_from(vec![
            team_change(1, 10), // owner's WvW team
            team_change(2, 10), // pet: same team as owner
            team_change(9, 20), // enemy: different team
            pov(1),             // recorder is the owner, so friendly_team = 10
            seed,
            pet_cc,
        ]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 1);
        assert_eq!(players[0].cc_duration_ms, 800);
    }

    #[test]
    fn apply_cc_tracks_stun_breaks() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut sb1 = dmg(100, 1, 0, 750);
        sb1.is_statechange = crate::evtc::sc::STUN_BREAK;
        let mut sb2 = dmg(200, 1, 0, 250);
        sb2.is_statechange = crate::evtc::sc::STUN_BREAK;
        let raw = raw_from(vec![sb1, sb2]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].stun_breaks, 2);
        assert_eq!(players[0].removed_stun_duration_ms, 1000);
    }

    fn raw_post(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    /// M4 Task 2: a `buff == 1` CC-shaped row (result == CROWD_CONTROL) must
    /// NOT be counted under a pre-era header -- see `is_cc`'s doc comment
    /// (pre-era, `buff == 1` rows decode through the retired `ConditionResult`
    /// enum, where byte 12 is meaningless, not genuine CC).
    #[test]
    fn buff_flagged_cc_row_not_counted_pre_era() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut e = cc_ev(100, 1, 9, 1500);
        e.buff = 1;
        let raw = raw_from(vec![e]); // pre-era header (empty build)
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 0, "pre-era buff==1 CC-shaped row must not be counted");
        assert_eq!(players[0].cc_duration_ms, 0);
    }

    /// M4 Task 2 core deliverable: the SAME `buff == 1` CC-shaped row IS
    /// counted under a post-era (>= 20260501) header -- verified against
    /// GW2EI's post-`ResultEnumRework` `AddBuffDamageDamageEvent` branch
    /// (`CombatEventFactory.cs:857-881`), which routes `DamageResult.
    /// CrowdControl` to a genuine `CrowdControlEvent` regardless of the buff
    /// flag. This is the era-equivalence pairing with the pre-era test
    /// above: pre-era rejects it, post-era accepts it, same event otherwise.
    #[test]
    fn buff_flagged_cc_row_counted_post_era() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut e = cc_ev(100, 1, 9, 1500);
        e.buff = 1;
        let raw = raw_post(vec![e]);
        let mut players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep);
        assert_eq!(players[0].cc_applied, 1, "post-era buff==1 CC-shaped row must be counted");
        assert_eq!(players[0].cc_duration_ms, 1500);
    }

    /// Post-era, an ordinary `buff == 0` CC row must still count exactly the
    /// same as pre-era (era-equivalence for the already-working case, not
    /// just the new buff==1 case).
    #[test]
    fn buff_zero_cc_row_is_era_equivalent() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let pre = raw_from(vec![cc_ev(100, 1, 9, 1500)]);
        let post = raw_post(vec![cc_ev(100, 1, 9, 1500)]);
        let mut pre_players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let mut post_players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut pre_players, &pre, &squad, &enemies, &addr_to_rep);
        apply_cc(&mut post_players, &post, &squad, &enemies, &addr_to_rep);
        assert_eq!(pre_players[0].cc_applied, post_players[0].cc_applied);
        assert_eq!(pre_players[0].cc_duration_ms, post_players[0].cc_duration_ms);
        assert_eq!(post_players[0].cc_applied, 1);
        assert_eq!(post_players[0].cc_duration_ms, 1500);
    }

    /// A post-era `buff == 1` CC row must not alter `timeline`'s
    /// `squad_damage` bucket (it must be excluded from damage regardless of
    /// buff, per the `squad_damage` loop's doc comment) while still landing
    /// in the `cc_applied` bucket.
    #[test]
    fn timeline_post_era_buff_cc_excluded_from_damage_but_counted_as_cc() {
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![], enemies: vec![],
            markers: vec![], tick_rate: None,
        };
        let mut e = cc_ev(100, 1, 9, 5000); // would look like 5000 damage if not excluded
        e.buff = 1;
        e.buff_dmg = 5000; // buff-damage field also populated -- must still be ignored
        let raw = raw_post(vec![e]);
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let tl = timeline(&enc, &raw, &squad, &enemies);
        assert_eq!(tl.squad_damage[0], 0, "CC row must not leak into squad_damage even when buff==1 post-era");
        assert_eq!(tl.cc_applied[0], 1, "post-era buff==1 CC row must still be counted as CC");
    }

    /// M4 Task 2: pet/minion-sourced CC via a `buff == 1` row must also be
    /// credited to the owner post-era (era-gating applies uniformly through
    /// the shared `is_cc` predicate `pet_credit_cc_events` calls).
    #[test]
    fn pet_credit_cc_counts_buff_flagged_row_post_era_only() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut seed = dmg(50, 1, 9, 10);
        seed.src_instid = 11;
        let mut pet_cc = cc_ev(100, 2, 9, 800);
        pet_cc.src_instid = 22;
        pet_cc.src_master_instid = 11;
        pet_cc.buff = 1;
        let events = vec![
            team_change(1, 10),
            team_change(2, 10),
            team_change(9, 20),
            pov(1),
            seed,
            pet_cc,
        ];

        let mut pre_players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        let addr_to_rep: std::collections::BTreeMap<u64, u64> = [(1u64, 1u64)].into_iter().collect();
        apply_cc(&mut pre_players, &raw_from(events.clone()), &squad, &enemies, &addr_to_rep);
        assert_eq!(pre_players[0].cc_applied, 0, "pre-era buff==1 pet CC must not be credited");

        let mut post_players = vec![PlayerMetrics { agent_addr: 1, ..Default::default() }];
        apply_cc(&mut post_players, &raw_post(events), &squad, &enemies, &addr_to_rep);
        assert_eq!(post_players[0].cc_applied, 1, "post-era buff==1 pet CC must be credited to owner");
        assert_eq!(post_players[0].cc_duration_ms, 800);
    }
}
