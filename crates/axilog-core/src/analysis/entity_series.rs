//! Per-entity 1s series for outgoing CC and boon strips (both directions).
//!
//! Deliberately a separate pass rather than an extension of `cc::apply` or
//! `timeline_with_registry`: `apply` has no `Encounter` (so it cannot size
//! buckets) and `timeline_with_registry` has no per-player index. Reusing the
//! same `is_cc` predicate and the same strip primitives is what makes the
//! sum-invariant tests hold; sharing a loop with them is not required for it.
//!
//! Indexing is POSITIONAL over `players`, matching `healing_detail`.

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::cc::{is_cc, pet_credit_cc_events_timed};
use crate::analysis::damage::InstidRegistry;
use crate::analysis::{defenses, support, PlayerMetrics};
use crate::evtc::RawLog;
use crate::model::Encounter;

#[derive(Debug, Clone, Default)]
pub struct PlayerSeries {
    pub cc_applied: Vec<u32>,
    pub strips: Vec<u32>,
    pub strips_taken: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct EntitySeriesDetail {
    per_player: Vec<PlayerSeries>,
}

impl EntitySeriesDetail {
    pub fn len(&self) -> usize { self.per_player.len() }
    pub fn is_empty(&self) -> bool { self.per_player.is_empty() }
    pub fn get(&self, i: usize) -> &PlayerSeries { &self.per_player[i] }
}

pub fn build(
    enc: &Encounter,
    raw: &RawLog,
    registry: &InstidRegistry,
    players: &[PlayerMetrics],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> EntitySeriesDetail {
    let res = 1000u64;
    let buckets = ((enc.duration_ms / res) + 1) as usize;
    let t0 = raw.log_start_ms();
    let post_era = raw.header.is_post_buff_rework();

    let idx: BTreeMap<u64, usize> =
        players.iter().enumerate().map(|(i, p)| (p.agent_addr, i)).collect();
    let rep = |addr: u64| addr_to_rep.get(&addr).copied().unwrap_or(addr);

    let mut per_player = vec![
        PlayerSeries {
            cc_applied: vec![0u32; buckets],
            strips: vec![0u32; buckets],
            strips_taken: vec![0u32; buckets],
        };
        players.len()
    ];

    // Bucket index for an absolute event time, or None if out of range.
    // The event's `time` is log-ABSOLUTE (not log-relative), same as every
    // other raw-event field -- must subtract `t0` before dividing, exactly
    // like `cc::timeline_with_registry` does with its own `t0`. Getting
    // this backwards (or omitting it) produces buckets that are silently
    // all zero or wildly out of range -- see this module's test module for
    // the distinct-bucket assertion that would catch it.
    let bucket = |time: u64| -> Option<usize> {
        let b = (time.saturating_sub(t0) / res) as usize;
        (b < buckets).then_some(b)
    };

    // Direct player-sourced CC — same predicate and same guards as
    // `cc::apply_cc`'s first loop, which is why the sums match.
    for e in &raw.events {
        if is_cc(e, post_era) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let (Some(&i), Some(b)) = (idx.get(&rep(e.src_agent)), bucket(e.time)) {
                per_player[i].cc_applied[b] += 1;
            }
        }
    }

    // Pet/minion-sourced CC credited to the owner — `cc::apply_cc`'s second
    // loop, timed variant.
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, _dst, _duration_ms, time) in
        pet_credit_cc_events_timed(raw, registry, squad, enemies, friendly_team, &agent_team)
    {
        if let (Some(&i), Some(b)) = (idx.get(&rep(owner)), bucket(time)) {
            per_player[i].cc_applied[b] += 1;
        }
    }

    for (rep_addr, strips) in support::outgoing_boon_strips(raw, enemies, addr_to_rep) {
        let Some(&i) = idx.get(&rep_addr) else { continue };
        for &(time, _skillid, _ms) in &strips {
            if let Some(b) = bucket(time) { per_player[i].strips[b] += 1; }
        }
    }

    for (rep_addr, strips) in
        defenses::incoming_boon_strips_with_registry(raw, registry, squad, addr_to_rep)
    {
        let Some(&i) = idx.get(&rep_addr) else { continue };
        for &(time, _skillid, _ms) in &strips {
            if let Some(b) = bucket(time) { per_player[i].strips_taken[b] += 1; }
        }
    }

    EntitySeriesDetail { per_player }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cc;
    use crate::analysis::damage::InstidRegistry;
    use crate::analysis::{defenses, support, PlayerMetrics};
    use crate::evtc::{RawAgent, RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};
    use std::collections::{BTreeMap, BTreeSet};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn cc_ev(time: u64, src: u64, dst: u64, duration_ms: i32) -> RawEvent {
        let mut e = base_event();
        e.time = time;
        e.src_agent = src;
        e.dst_agent = dst;
        e.value = duration_ms;
        e.result = crate::evtc::result::CROWD_CONTROL;
        e
    }

    fn enc_from(duration_ms: u64, players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(), pve: None, map: "".into(), duration_ms, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players, enemies: vec![],
            markers: vec![], ground_markers: vec![], tick_rate: None, objectives: Vec::new(),
            started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    fn enc_player(addr: u64) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(), subgroup: 1,
            in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    /// A single squad player (addr 100) and one enemy (addr 200), 10s
    /// encounter, `log_start_ms() == 0` (a zero-time dummy event, involving
    /// neither addr, is listed first so `RawLog::log_start_ms` — which
    /// reads `events.first()` — resolves to 0), and two direct CC events at
    /// t=1500 and t=7500. `cc::apply_cc` is run so the scalar `cc_applied`
    /// this fixture pins against is populated the same way production
    /// builds it.
    fn two_cc_fixture() -> (
        Encounter,
        RawLog,
        InstidRegistry,
        Vec<PlayerMetrics>,
        BTreeSet<u64>,
        BTreeSet<u64>,
        BTreeMap<u64, u64>,
    ) {
        let squad: BTreeSet<u64> = [100u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [200u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(100u64, 100u64)].into_iter().collect();

        let mut dummy = base_event();
        dummy.time = 0;
        dummy.src_agent = 999;
        dummy.dst_agent = 999;

        let events = vec![dummy, cc_ev(1500, 100, 200, 500), cc_ev(7500, 100, 200, 700)];
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        };
        assert_eq!(raw.log_start_ms(), 0, "fixture must have log_start_ms == 0");

        let registry = InstidRegistry::build(&raw);
        let enc = enc_from(10_000, vec![enc_player(100)]);

        let mut players = vec![PlayerMetrics { agent_addr: 100, ..Default::default() }];
        cc::apply_cc(&mut players, &raw, &squad, &enemies, &addr_to_rep, &BTreeMap::new());

        (enc, raw, registry, players, squad, enemies, addr_to_rep)
    }

    /// The invariant the whole feature rests on: bucketed CC must sum to the
    /// scalar `cc_applied` that `cc::apply` already produces, because both
    /// walk the same events through the same `is_cc` predicate.
    #[test]
    fn per_player_cc_buckets_sum_to_scalar() {
        let (enc, raw, registry, players, squad, enemies, addr_to_rep) = two_cc_fixture();
        let detail = build(&enc, &raw, &registry, &players, &squad, &enemies, &addr_to_rep);
        for (i, p) in players.iter().enumerate() {
            let bucketed: u32 = detail.get(i).cc_applied.iter().sum();
            assert_eq!(bucketed, p.cc_applied, "player {i} CC buckets must sum to scalar");
        }
    }

    /// Two CC events 6s apart must land in different buckets, not be summed
    /// into one — this is what distinguishes a series from the scalar.
    #[test]
    fn cc_events_separate_into_distinct_buckets() {
        let (enc, raw, registry, players, squad, enemies, addr_to_rep) = two_cc_fixture();
        let detail = build(&enc, &raw, &registry, &players, &squad, &enemies, &addr_to_rep);
        let s = &detail.get(0).cc_applied;
        assert_eq!(s[1], 1, "first CC at t=1500ms lands in bucket 1");
        assert_eq!(s[7], 1, "second CC at t=7500ms lands in bucket 7");
        assert_eq!(s.iter().sum::<u32>(), 2);
    }

    fn raw_agent(addr: u64) -> RawAgent {
        RawAgent {
            addr, prof: 0, is_elite: 0, toughness: 0, concentration: 0, healing: 0,
            hitbox_width: 0, condition: 0, hitbox_height: 0, name_raw: Vec::new(),
        }
    }

    /// One squad player (addr 100), one enemy (addr 200), 10s encounter,
    /// with one outgoing strip (squad player strips the enemy's boon) and
    /// one incoming strip (the enemy strips the squad player's boon) at
    /// distinct times, run through the real `support::apply` and
    /// `defenses::build_with_registry` passes so the scalars this fixture
    /// pins against are populated exactly as production computes them.
    fn strip_fixture() -> (
        Encounter,
        RawLog,
        InstidRegistry,
        Vec<PlayerMetrics>,
        BTreeSet<u64>,
        BTreeSet<u64>,
        BTreeMap<u64, u64>,
    ) {
        let squad: BTreeSet<u64> = [100u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [200u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(100u64, 100u64)].into_iter().collect();

        let mut dummy = base_event();
        dummy.time = 0;
        dummy.src_agent = 999;
        dummy.dst_agent = 999;

        // Outgoing: squad player (100) strips a boon off the enemy (200).
        // Role inversion in `outgoing_boon_strips`: owner (src) = victim,
        // remover (dst) = the squad player getting credit.
        let mut outgoing = base_event();
        outgoing.time = 2000;
        outgoing.is_buffremove = crate::evtc::buff_remove::ALL;
        outgoing.skillid = crate::analysis::buffs::MIGHT;
        outgoing.src_agent = 200;
        outgoing.dst_agent = 100;
        outgoing.value = 3000;

        // Incoming: enemy (200) strips a boon off the squad player (100).
        let mut incoming = base_event();
        incoming.time = 6000;
        incoming.is_buffremove = crate::evtc::buff_remove::ALL;
        incoming.skillid = crate::analysis::buffs::MIGHT;
        incoming.src_agent = 100;
        incoming.dst_agent = 200;
        incoming.value = 2500;

        let events = vec![dummy, outgoing, incoming];
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![raw_agent(100), raw_agent(200)],
            skills: vec![],
            events,
            guid_map: vec![],
        };
        assert_eq!(raw.log_start_ms(), 0, "fixture must have log_start_ms == 0");

        let registry = InstidRegistry::build(&raw);
        let enc = enc_from(10_000, vec![enc_player(100)]);

        let mut players = vec![PlayerMetrics { agent_addr: 100, ..Default::default() }];
        support::apply(&mut players, &raw, &enc, &enemies, &addr_to_rep);
        let defenses_by_rep = defenses::build_with_registry(&raw, &registry, &squad, &addr_to_rep);
        for p in players.iter_mut() {
            if let Some(d) = defenses_by_rep.get(&p.agent_addr) {
                p.defenses = d.clone();
            }
        }

        // Guard against a vacuous pass: both scalars must be genuinely
        // nonzero, or the sum-invariant assertion below would trivially
        // hold at 0 == 0 without proving anything about the bucketing.
        assert_eq!(players[0].support.strips, 1, "fixture must produce exactly one outgoing strip");
        assert_eq!(players[0].defenses.boon_strips_taken, 1, "fixture must produce exactly one incoming strip");

        (enc, raw, registry, players, squad, enemies, addr_to_rep)
    }

    #[test]
    fn per_player_strip_buckets_sum_to_scalars() {
        let (enc, raw, registry, players, squad, enemies, addr_to_rep) = strip_fixture();
        let detail = build(&enc, &raw, &registry, &players, &squad, &enemies, &addr_to_rep);
        for (i, p) in players.iter().enumerate() {
            assert_eq!(
                detail.get(i).strips.iter().sum::<u32>(),
                p.support.strips,
                "player {i} outgoing strip buckets must sum to the scalar",
            );
            assert_eq!(
                detail.get(i).strips_taken.iter().sum::<u32>(),
                p.defenses.boon_strips_taken,
                "player {i} incoming strip buckets must sum to the scalar",
            );
        }
    }
    /// Two squad players (addr 100, addr 101) and one enemy (addr 200),
    /// 10s encounter, `log_start_ms() == 0`. Player 100's CC lands at
    /// t=1500 (bucket 1), player 101's CC lands at t=7500 (bucket 7) --
    /// deliberately DISTINCT buckets per player, so a transposed `idx` map
    /// (player 100's events credited to slot 1, and vice versa) cannot
    /// accidentally satisfy the assertions below the way a same-bucket or
    /// single-player fixture could.
    fn two_player_cc_fixture() -> (Encounter, RawLog, InstidRegistry, Vec<PlayerMetrics>, BTreeSet<u64>, BTreeSet<u64>, BTreeMap<u64, u64>) {
        let squad: BTreeSet<u64> = [100u64, 101u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [200u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(100u64, 100u64), (101u64, 101u64)].into_iter().collect();

        let mut dummy = base_event();
        dummy.time = 0;
        dummy.src_agent = 999;
        dummy.dst_agent = 999;

        let events = vec![
            dummy,
            cc_ev(1500, 100, 200, 500), // player 100's CC -> bucket 1
            cc_ev(7500, 101, 200, 700), // player 101's CC -> bucket 7
        ];
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        };
        assert_eq!(raw.log_start_ms(), 0, "fixture must have log_start_ms == 0");

        let registry = InstidRegistry::build(&raw);
        let enc = enc_from(10_000, vec![enc_player(100), enc_player(101)]);
        let players = vec![
            PlayerMetrics { agent_addr: 100, ..Default::default() },
            PlayerMetrics { agent_addr: 101, ..Default::default() },
        ];

        (enc, raw, registry, players, squad, enemies, addr_to_rep)
    }

    /// The positional contract `entity_series::build` is documented to
    /// hold (indexing matches `players`, like `healing_detail`) -- the next
    /// task (a downstream chart) joins on this being right. A wrong `idx`
    /// map (e.g. built off insertion order instead of `agent_addr`, or with
    /// the two players' slots swapped) would misattribute every lane while
    /// every single-player and sum-invariant test in this module still
    /// passes, since those only check TOTALS, never WHICH player's slot an
    /// event landed in.
    #[test]
    fn cc_applied_is_indexed_by_player_not_transposed() {
        let (enc, raw, registry, players, squad, enemies, addr_to_rep) = two_player_cc_fixture();
        let detail = build(&enc, &raw, &registry, &players, &squad, &enemies, &addr_to_rep);

        // Player 0 (addr 100): their own CC at bucket 1, nothing at bucket 7.
        assert_eq!(detail.get(0).cc_applied[1], 1, "player 0's own CC must land in player 0's slot at bucket 1");
        assert_eq!(detail.get(0).cc_applied[7], 0, "player 0's slot must stay zero at player 1's bucket");

        // Player 1 (addr 101): their own CC at bucket 7, nothing at bucket 1.
        assert_eq!(detail.get(1).cc_applied[7], 1, "player 1's own CC must land in player 1's slot at bucket 7");
        assert_eq!(detail.get(1).cc_applied[1], 0, "player 1's slot must stay zero at player 0's bucket");
    }

    /// Two squad players (addr 100, addr 101) and one enemy (addr 200),
    /// each landing one OUTGOING strip on the enemy at a distinct time --
    /// same transposition-proofing rationale as `two_player_cc_fixture`,
    /// for the strip primitives instead of the CC predicate.
    fn two_player_strip_fixture() -> (Encounter, RawLog, InstidRegistry, Vec<PlayerMetrics>, BTreeSet<u64>, BTreeSet<u64>, BTreeMap<u64, u64>) {
        let squad: BTreeSet<u64> = [100u64, 101u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [200u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(100u64, 100u64), (101u64, 101u64)].into_iter().collect();

        let mut dummy = base_event();
        dummy.time = 0;
        dummy.src_agent = 999;
        dummy.dst_agent = 999;

        // Role inversion in `outgoing_boon_strips`: owner (src) = victim
        // (the enemy), remover (dst) = the squad player getting credit.
        let mut strip_100 = base_event();
        strip_100.time = 2000; // bucket 2
        strip_100.is_buffremove = crate::evtc::buff_remove::ALL;
        strip_100.skillid = crate::analysis::buffs::MIGHT;
        strip_100.src_agent = 200;
        strip_100.dst_agent = 100;
        strip_100.value = 3000;

        let mut strip_101 = base_event();
        strip_101.time = 8000; // bucket 8
        strip_101.is_buffremove = crate::evtc::buff_remove::ALL;
        strip_101.skillid = crate::analysis::buffs::MIGHT;
        strip_101.src_agent = 200;
        strip_101.dst_agent = 101;
        strip_101.value = 2500;

        let events = vec![dummy, strip_100, strip_101];
        let raw = RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![raw_agent(100), raw_agent(101), raw_agent(200)],
            skills: vec![],
            events,
            guid_map: vec![],
        };
        assert_eq!(raw.log_start_ms(), 0, "fixture must have log_start_ms == 0");

        let registry = InstidRegistry::build(&raw);
        let enc = enc_from(10_000, vec![enc_player(100), enc_player(101)]);
        let players = vec![
            PlayerMetrics { agent_addr: 100, ..Default::default() },
            PlayerMetrics { agent_addr: 101, ..Default::default() },
        ];

        (enc, raw, registry, players, squad, enemies, addr_to_rep)
    }

    #[test]
    fn outgoing_strips_are_indexed_by_player_not_transposed() {
        let (enc, raw, registry, players, squad, enemies, addr_to_rep) = two_player_strip_fixture();
        let detail = build(&enc, &raw, &registry, &players, &squad, &enemies, &addr_to_rep);

        // Player 0 (addr 100): their own strip at bucket 2, nothing at bucket 8.
        assert_eq!(detail.get(0).strips[2], 1, "player 0's own strip must land in player 0's slot at bucket 2");
        assert_eq!(detail.get(0).strips[8], 0, "player 0's slot must stay zero at player 1's bucket");

        // Player 1 (addr 101): their own strip at bucket 8, nothing at bucket 2.
        assert_eq!(detail.get(1).strips[8], 1, "player 1's own strip must land in player 1's slot at bucket 8");
        assert_eq!(detail.get(1).strips[2], 0, "player 1's slot must stay zero at player 0's bucket");
    }
}
