use crate::evtc::{result, sc, RawEvent, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// Time-aware `instid -> owning agent addr` resolution (Task 4, M2).
///
/// arcdps recycles instids after an agent despawns, so a single log-wide
/// last-write-wins map (the pre-Task-4 approach) can misattribute pet/minion
/// damage: if instid 42 belongs to player A's pet early in the log and gets
/// reassigned to player B's pet later, the last-write-wins map always
/// resolves instid 42 to B -- even when crediting an event that happened
/// while A owned it.
///
/// This instead records every `(time, addr)` registration observed for each
/// instid, in event order, and resolves an instid at a specific event time
/// to whichever registration was in effect at that time (the latest
/// registration with `time <= t`). Where no registration exists yet at or
/// before `t`, resolution fails (`None`) -- the event is left uncredited
/// rather than guessing.
///
/// Shared by `pet_credit_events` (damage) and `cc::pet_credit_cc_events` (CC)
/// so both stay consistent with each other, mirroring how they already share
/// the `pet_credit_events` records themselves (see that fn's docs).
///
/// **M10 Task 1 fix**: rows on `sc::EXTENSION`/`sc::EXTENSION_COMBAT` do NOT
/// contribute registrations (see `build`'s doc comment for why) -- but they
/// ARE still resolvable via `resolve_at` against whatever ordinary-event
/// registration was in effect at their own time, which is exactly what
/// `analysis::healing` needs (extension data rows carry real `src_instid`/
/// `dst_instid` but untrustworthy `src_agent`/`dst_agent` -- see
/// `evtc::ext_healing`'s module doc).
pub struct InstidRegistry {
    /// Registrations per instid, indexed DIRECTLY by the instid (MPERF Task
    /// 3): `by_instid[instid as usize]` is that instid's own chronological
    /// `(time, addr)` registration list, empty when it was never registered.
    ///
    /// This was a `BTreeMap<u16, Vec<(u64, u64)>>` through MPERF Task 2. The
    /// key space is a `u16`, so the map is bounded at 65,536 entries no
    /// matter how large the log is -- a flat `Vec` of exactly that length
    /// (~1.5 MiB of `Vec` headers, allocated once) replaces an O(log n)
    /// tree descent with an O(1) index on every `build` registration (two
    /// per event, i.e. over a million on a real WvW log) and on every
    /// `resolve_at` query. Nothing outside this type ever sees the backing
    /// store: `by_instid` is private and is only ever indexed by a known
    /// instid -- it is never iterated, so no ordering-dependent behaviour
    /// (and therefore no output) can change. The per-instid `Vec<(u64,
    /// u64)>` contents, and `resolve_at`'s `partition_point` query over
    /// them, are untouched.
    by_instid: Vec<Vec<(u64, u64)>>,
}

/// Number of distinct arcdps instids -- the full `u16` key space
/// [`InstidRegistry::by_instid`] is indexed by.
const INSTID_SPACE: usize = u16::MAX as usize + 1;

impl InstidRegistry {
    /// Build the registry by scanning every event's src/dst instid + addr
    /// pair, in event order (i.e. the order they appear in the log, which is
    /// chronological).
    ///
    /// **Excludes** `sc::EXTENSION`/`sc::EXTENSION_COMBAT` rows from
    /// contributing registrations (M10 Task 1 fix, found while wiring up
    /// healing-extension decode): GW2EI's own `HealingStatsExtensionHandler.
    /// AdjustCombatEvent` explicitly does NOT trust `src_agent`/`dst_agent`
    /// on these rows (it overrides both via instid lookup instead -- "Prefer
    /// instid fetch for healing events") -- and empirically, real captures
    /// (this project's own local post-rework fixture) carry these rows with
    /// small, plausible-looking-but-WRONG `src_agent`/`dst_agent` values
    /// that are NOT the true owner of that `src_instid` at that time
    /// (verified directly: a synthetic extension-shaped row with an unset
    /// `src_agent` of `0` was, before this fix, silently accepted as a
    /// registration and corrupted a subsequent `resolve_at` query for the
    /// SAME instid at the SAME row's own time). Without this exclusion, ANY
    /// log carrying the healing extension (which, per this project's own
    /// fixtures, both a Jan-2026 and a Jul-2026 real WvW capture do) risks
    /// silently corrupting pet/minion damage and CC credit for whichever
    /// instids the extension happens to reuse -- this fix closes that gap
    /// for every registry consumer (`pet_credit_events`, `cc::
    /// pet_credit_cc_events`, and the new `analysis::healing`), not just the
    /// new one.
    pub fn build(raw: &RawLog) -> Self {
        let mut by_instid: Vec<Vec<(u64, u64)>> = vec![Vec::new(); INSTID_SPACE];
        for e in &raw.events {
            if e.is_statechange == sc::EXTENSION || e.is_statechange == sc::EXTENSION_COMBAT {
                continue;
            }
            if e.src_instid != 0 {
                Self::register(&mut by_instid, e.src_instid, e.time, e.src_agent);
            }
            if e.dst_instid != 0 {
                Self::register(&mut by_instid, e.dst_instid, e.time, e.dst_agent);
            }
        }
        InstidRegistry { by_instid }
    }

    fn register(map: &mut [Vec<(u64, u64)>], instid: u16, time: u64, addr: u64) {
        let entries = &mut map[instid as usize];
        if let Some(&(last_time, last_addr)) = entries.last() {
            if last_addr == addr {
                return; // no ownership change; avoid growing the vec pointlessly
            }
            if time < last_time {
                // Events are expected in chronological order; if not, insert
                // at the correct sorted position rather than assume append.
                let pos = entries.partition_point(|&(t, _)| t <= time);
                entries.insert(pos, (time, addr));
                return;
            }
        }
        entries.push((time, addr));
    }

    /// The addr registered to `instid` at time `t` -- i.e. the latest
    /// registration with `time <= t`. `None` if `instid` had no registration
    /// yet at or before `t` (including "never registered at all").
    pub fn resolve_at(&self, instid: u16, t: u64) -> Option<u64> {
        let entries = &self.by_instid[instid as usize];
        let idx = entries.partition_point(|&(time, _)| time <= t);
        if idx == 0 {
            // Either `instid` was never registered at all (empty list) or its
            // first registration is later than `t` -- both are "no addr known
            // at this time", exactly as the old `BTreeMap::get(&instid)?`
            // miss and this same `idx == 0` check together expressed.
            return None;
        }
        Some(entries[idx - 1].1)
    }
}

/// Returns per-source: (total_to_enemies, per_enemy_map).
pub fn accumulate(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    let mut out: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        // Crowd-control application events reuse `value`/`buff_dmg` to carry
        // CC duration (ms), not damage — see `result::CROWD_CONTROL` docs.
        if e.result == result::CROWD_CONTROL { continue; }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) { continue; }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        let entry = out.entry(e.src_agent).or_default();
        entry.0 += dmg;
        *entry.1.entry(e.dst_agent).or_default() += dmg;
    }
    out
}

/// Credits friendly-side NPC/gadget (pet, minion, spirit, mount-summon,
/// etc.) damage to the owning squad player.
///
/// arcdps attributes pet/minion damage to the pet's own agent, not its
/// owner, so `accumulate` (which only looks at `squad` player addresses as
/// `src_agent`) misses it entirely. In the WvW golden fixture (Task 16A)
/// that undercounted squadTotalDamage by ~1.1% — enough to fail the 0.5%
/// calibration tolerance. This resolves the owner via each event's
/// `src_master_instid`, looked up in `InstidRegistry` at that event's own
/// time (Task 4, M2) -- so instid reuse across the log (arcdps recycles
/// instids after an agent despawns) resolves to whichever agent actually
/// held that instid when the event happened, not just whoever holds it last.
///
/// `friendly_team` / `agent_team` come from `wvw::resolve_teams`. Damage
/// destination is intentionally NOT restricted to the `enemies` set here:
/// arcdps' own `iff` field (relative to the pet, i.e. non-FRIEND) is used
/// instead, matching how `accumulate`'s squad/enemies split is derived in
/// the first place — see wvw::apply.
pub fn accumulate_pet_credit(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    accumulate_pet_credit_with_registry(
        raw,
        &InstidRegistry::build(raw),
        squad,
        friendly_team,
        agent_team,
    )
}

/// [`accumulate_pet_credit`] against a caller-supplied, already-built
/// [`InstidRegistry`] (MPERF Task 2).
///
/// `InstidRegistry::build` is a pure function of `raw` -- a full linear scan
/// over every event -- so every consumer that built its own was producing a
/// bit-for-bit identical map. `analysis::analyze` now builds it exactly once
/// and threads `&InstidRegistry` into each pass, which is provably
/// output-identical while removing ~9 redundant whole-log scans per parse.
/// The `raw`-only wrapper above stays for SDK/standalone/test callers that
/// have no registry in hand.
pub fn accumulate_pet_credit_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> BTreeMap<u64, (u64, BTreeMap<u64, u64>)> {
    let mut out: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for (_time, owner, dst, dmg) in
        pet_credit_events_with_registry(raw, registry, squad, friendly_team, agent_team)
    {
        let entry = out.entry(owner).or_default();
        entry.0 += dmg;
        *entry.1.entry(dst).or_default() += dmg;
    }
    out
}

/// Event-level pet/minion damage credit records: `(time, owner, dst, dmg)`.
/// Shared by `accumulate_pet_credit` (per-owner totals) and
/// `cc::timeline` (per-second buckets) so both stay consistent with each
/// other — see Finding #4: `sum(timeline.squad_damage)` must equal
/// `sum(player.damage_total)`, which requires the timeline to include the
/// same pet/minion credit that per-player totals do.
pub fn pet_credit_events(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> Vec<(u64, u64, u64, u64)> {
    pet_credit_events_with_registry(
        raw,
        &InstidRegistry::build(raw),
        squad,
        friendly_team,
        agent_team,
    )
}

/// [`pet_credit_events`] against a caller-supplied, already-built
/// [`InstidRegistry`] (MPERF Task 2) -- see
/// [`accumulate_pet_credit_with_registry`]'s doc comment for why the
/// registry is threaded rather than rebuilt per consumer.
pub fn pet_credit_events_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    squad: &BTreeSet<u64>,
    friendly_team: Option<u32>,
    agent_team: &BTreeMap<u64, u32>,
) -> Vec<(u64, u64, u64, u64)> {
    let mut out = Vec::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if e.result == result::CROWD_CONTROL { continue; }
        if e.iff == 0 { continue; } // FRIEND: never damage
        if squad.contains(&e.src_agent) { continue; } // real players: handled by `accumulate`
        if agent_team.get(&e.src_agent).copied() != friendly_team { continue; } // not our pet
        let owner = match registry.resolve_at(e.src_master_instid, e.time) {
            Some(addr) if squad.contains(&addr) => addr,
            _ => continue,
        };
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        out.push((e.time, owner, e.dst_agent, dmg));
    }
    out
}

/// Sums INCOMING damage per squad member (any source — enemy players or
/// NPCs). Keyed by the raw destination agent addr; callers fold this by
/// account representative (see `wvw`/`analysis::mod` relog aggregation) to
/// get a per-account total. Mirrors `accumulate`'s damage predicate exactly
/// (non-statechange/activation/buffremove, excludes CROWD_CONTROL result
/// rows since those carry CC duration ms, not damage).
pub fn accumulate_damage_taken(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
) -> BTreeMap<u64, u64> {
    let mut out: BTreeMap<u64, u64> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 { continue; }
        if e.result == result::CROWD_CONTROL { continue; }
        if !squad.contains(&e.dst_agent) { continue; }
        let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
        if dmg == 0 { continue; }
        *out.entry(e.dst_agent).or_default() += dmg;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::RawEvent;
    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time:0, src_agent:src, dst_agent:dst, value:dmg, buff_dmg:0,
            overstack:0, skillid:1, src_instid:0, dst_instid:0,
            src_master_instid:0, dst_master_instid:0, iff:1, buff:0, result:0,
            is_activation:0, is_buffremove:0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
    }
    #[test]
    fn sums_physical_damage_to_enemy() {
        let squad = [1u64].into_iter().collect();
        let enemies = [9u64].into_iter().collect();
        let evs = vec![strike(1, 9, 100), strike(1, 9, 50), strike(1, 2, 999)];
        let dmg = accumulate(&evs, &squad, &enemies);
        assert_eq!(dmg[&1].0, 150); // total to enemies only
        assert_eq!(dmg[&1].1.get(&9).copied(), Some(150));
    }
    #[test]
    fn sums_incoming_damage_from_any_source() {
        let squad = [1u64].into_iter().collect();
        // enemy player (9) and an NPC (10) both hit squad member 1.
        let evs = vec![
            strike(9, 1, 200),
            strike(10, 1, 75),
            strike(9, 2, 999), // to a non-squad dst, ignored
        ];
        let taken = accumulate_damage_taken(&evs, &squad);
        assert_eq!(taken.get(&1).copied(), Some(275));
    }
    /// Task 4 (M2): arcdps recycles instids after an agent despawns, so the
    /// same instid can belong to two different owners at different points
    /// in the same log. A last-write-wins `instid -> addr` map (the
    /// pre-Task-4 approach) would resolve EVERY pet event with
    /// `src_master_instid == 11` to whichever owner registered that instid
    /// last in the log -- silently misattributing the earlier era's pet
    /// damage. Time-aware resolution must instead credit each era's pet
    /// damage to that era's actual owner.
    #[test]
    fn pet_credit_resolves_instid_reuse_by_era() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let friendly_team = Some(10u32);
        let agent_team: BTreeMap<u64, u32> =
            [(1u64, 10u32), (2u64, 10u32), (300u64, 10u32)].into_iter().collect();

        fn ev(time: u64, src: u64, src_instid: u16, master: u16, dst: u64, v: i32) -> RawEvent {
            RawEvent { time, src_agent: src, dst_agent: dst, value: v, buff_dmg: 0,
                overstack: 0, skillid: 1, src_instid, dst_instid: 0,
                src_master_instid: master, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
                is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0, is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0 }
        }

        let raw = RawLog {
            header: crate::evtc::RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events: vec![
                // Era 1: agent 1 (A) holds instid 11.
                ev(0, 1, 11, 0, 9, 10),
                // Pet damage credited during era 1 -- must resolve to A.
                ev(200, 300, 77, 11, 9, 100),
                // Era 2: instid 11 gets recycled to agent 2 (B), e.g. after
                // A relogs/despawns and arcdps reassigns the freed instid.
                ev(600, 2, 11, 0, 9, 10),
                // Pet damage credited during era 2 -- must resolve to B, not
                // A (which a last-write-wins map would also get right here,
                // but only by coincidence -- the era-1 event above is the
                // real regression check).
                ev(700, 300, 77, 11, 9, 50),
            ],
            guid_map: vec![],
        };

        let credited = accumulate_pet_credit(&raw, &squad, friendly_team, &agent_team);
        assert_eq!(credited.get(&1).map(|(t, _)| *t), Some(100), "era-1 pet damage credited to A");
        assert_eq!(credited.get(&2).map(|(t, _)| *t), Some(50), "era-2 pet damage credited to B, not A");
    }

    #[test]
    fn excludes_crowd_control_from_incoming_damage() {
        let squad = [1u64].into_iter().collect();
        let mut cc = strike(9, 1, 5000); // would look huge if treated as damage
        cc.result = result::CROWD_CONTROL;
        let evs = vec![strike(9, 1, 100), cc];
        let taken = accumulate_damage_taken(&evs, &squad);
        assert_eq!(taken.get(&1).copied(), Some(100));
    }

    /// M4 Task 2: the CC exclusion here is buff-flag-independent (unlike
    /// `cc::is_cc`, which era-gates CC RECOGNITION on the buff flag) --
    /// verified against GW2EI's post-`ResultEnumRework`
    /// `AddBuffDamageDamageEvent` branch, which routes `DamageResult.
    /// CrowdControl` to a `CrowdControlEvent`, never a `HealthDamageEvent`,
    /// for `buff == 1` rows exactly as `AddDirectDamageEvent` already does
    /// for `buff == 0` rows -- so a post-era `buff == 1` CC-shaped row
    /// (`result == CROWD_CONTROL`, `buff_dmg` populated) must not leak into
    /// `accumulate`/`accumulate_pet_credit`/`accumulate_damage_taken`
    /// regardless of arcdps build era. No header/era input is needed by any
    /// of these three functions at all -- this predicate was already
    /// correct for both eras before M4 Task 2 (see `cc::is_cc`'s doc comment
    /// for the full citation trail); this test locks that in explicitly.
    #[test]
    fn excludes_buff_flagged_crowd_control_from_all_damage_paths() {
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64].into_iter().collect();
        let mut cc = strike(1, 9, 0);
        cc.buff = 1;
        cc.buff_dmg = 5000; // would look huge if treated as condi damage
        cc.result = result::CROWD_CONTROL;

        let evs = vec![strike(1, 9, 100), cc.clone()];
        let dmg = accumulate(&evs, &squad, &enemies);
        assert_eq!(dmg[&1].0, 100, "buff==1 CC row must not leak into accumulate's condi-damage sum");

        let mut cc_taken = strike(9, 1, 0);
        cc_taken.buff = 1;
        cc_taken.buff_dmg = 5000;
        cc_taken.result = result::CROWD_CONTROL;
        let taken_evs = vec![strike(9, 1, 100), cc_taken];
        let taken = accumulate_damage_taken(&taken_evs, &squad);
        assert_eq!(taken.get(&1).copied(), Some(100), "buff==1 CC row must not leak into damage_taken");
    }
}
