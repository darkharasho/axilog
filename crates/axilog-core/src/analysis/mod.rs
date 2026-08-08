pub mod damage;
pub mod downs;
pub mod cc;
pub mod buffs;
pub mod support;

use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct PlayerMetrics { pub agent_addr: u64, pub damage_total: u64, pub dps: f64,
    pub per_enemy: Vec<(u64,u64)>, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32,
    pub damage_taken: u64, pub cc_applied: u32, pub cc_duration_ms: u64,
    pub stun_breaks: u32, pub removed_stun_duration_ms: u64,
    /// Condition cleanses / boon strips / resurrects (M3, Task 3) -- see
    /// `support::SupportMetrics`.
    pub support: support::SupportMetrics }
#[derive(Debug, Clone)]
pub struct Timeline { pub resolution_ms: u64, pub squad_damage: Vec<u64>,
    pub cc_applied: Vec<u32>, pub downs: Vec<u32> }
#[derive(Debug, Clone)]
pub struct Metrics { pub players: Vec<PlayerMetrics>, pub timeline: Timeline,
    /// Per-(agent representative, boon id) stack-count timelines (M3, Task
    /// 1) -- see `buffs::simulate_boons`. Computed after all other passes
    /// below; does not alter any pre-existing metric.
    pub boons: BTreeMap<(u64, u32), buffs::BoonTimeline>,
    /// Per-(agent representative, boon id) uptime summaries (M3, Task 2) --
    /// see `buffs::simulate_boon_uptimes`/`buffs::uptime`. Reduces `boons`
    /// above over the same absolute fight window; does not alter any
    /// pre-existing metric.
    pub boon_uptime: BTreeMap<(u64, u32), buffs::BoonUptime>,
    /// Per-(source representative addr, boon id) self/group/squad
    /// generation-attribution rollups (M3, Task 4) -- see
    /// `buffs::generation`. Reduces the same per-target event streams
    /// `boons`/`boon_uptime` are built from (re-simulated with source
    /// tracking, not derived from `boons` directly -- see
    /// `buffs::generation`'s module doc for why the two simulations must
    /// stay independently verified against each other); does not alter any
    /// pre-existing metric.
    pub boon_generation: BTreeMap<(u64, u32), buffs::GenerationStats>,
    /// Structured, user-facing warnings about this analysis run (final-review
    /// fix wave). Currently populated with exactly one case: the log's
    /// arcdps build is on/after the post-2026-05-01 buff-statechange rework
    /// (`crate::evtc::RawHeader::is_post_buff_rework`) AND zero buff apply/
    /// remove/initial events were extracted for the tracked boons -- see
    /// `events::extract_buff_events`'s module doc for why this project's
    /// current extractor only understands the OLDER (pre-rework)
    /// `is_statechange == 0` combat-event shape, so a genuinely post-rework
    /// log silently reads all-zero boon/support metrics without this
    /// warning. Empty on every other log (including a post-rework build
    /// that still happens to carry zero tracked-boon events for a
    /// legitimate reason, e.g. no boons cast at all -- indistinguishable
    /// from the silent-zero case from data alone, so the build-era check is
    /// the only reliable signal; a false positive here is a harmless
    /// heads-up, not a wrong metric). Native schema surfaces this as a
    /// top-level `warnings: [...]` array (omitted when empty); the CLI
    /// table view prints it to stderr; `ei-json` has no comparable field so
    /// it's simply not carried over.
    pub warnings: Vec<String> }

pub fn analyze(enc: &Encounter, raw: &RawLog) -> Metrics {
    // A friendly account can own several raw agent addrs (relog / build
    // swap within the same recording — arcdps assigns a new addr per
    // login). `squad` is the union of ALL of an account's addrs so no
    // post-relog damage/downs/kills/CC/pets is dropped, while `addr_to_rep`
    // maps every one of those addrs back to the account's single
    // representative `Player.agent_addr` so per-account sums still land on
    // one `PlayerMetrics` entry (Finding #1).
    let squad: BTreeSet<u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc.players.iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();
    // Enemy players can also be deduped across relogs (Task 4, M2): `enemy`
    // is the union of ALL of an enemy's raw addrs (so combat events against
    // any of them still count), while `enemy_addr_to_rep` maps every one of
    // those addrs back to the representative `Enemy.id` so per-enemy damage
    // maps fold onto a single entry instead of splitting across the relog.
    let enemies: BTreeSet<u64> = enc.enemies.iter()
        .flat_map(|e| e.agent_addrs.iter().copied())
        .collect();
    let enemy_addr_to_rep: BTreeMap<u64, u64> = enc.enemies.iter()
        .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
        .collect();
    let mut dmg = damage::accumulate(&raw.events, &squad, &enemies);
    // WvW: credit friendly pet/minion damage (arcdps attributes it to the
    // pet's own agent) to the owning squad player — see Task 16A.
    let (agent_team, recorded_by) = crate::wvw::resolve_teams(raw);
    let friendly_team = recorded_by.and_then(|addr| agent_team.get(&addr).copied());
    for (owner, (total, per)) in damage::accumulate_pet_credit(raw, &squad, friendly_team, &agent_team) {
        let entry = dmg.entry(owner).or_default();
        entry.0 += total;
        for (dst, d) in per {
            *entry.1.entry(dst).or_default() += d;
        }
    }
    // Fold every source addr's damage onto its account representative so a
    // relogged/build-swapped account's damage is summed, not dropped. Also
    // fold each destination addr onto its enemy representative, so a
    // relogged enemy's damage lands on one `per_enemy` entry instead of
    // splitting across their addrs (Task 4, M2).
    let mut dmg_by_rep: BTreeMap<u64, (u64, BTreeMap<u64, u64>)> = BTreeMap::new();
    for (addr, (total, per)) in dmg.into_iter() {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        let entry = dmg_by_rep.entry(rep).or_default();
        entry.0 += total;
        for (dst, d) in per {
            let dst_rep = enemy_addr_to_rep.get(&dst).copied().unwrap_or(dst);
            *entry.1.entry(dst_rep).or_default() += d;
        }
    }
    let damage_taken = damage::accumulate_damage_taken(&raw.events, &squad);
    let mut taken_by_rep: BTreeMap<u64, u64> = BTreeMap::new();
    for (addr, total) in damage_taken {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        *taken_by_rep.entry(rep).or_default() += total;
    }
    let secs = (enc.duration_ms as f64 / 1000.0).max(1.0);
    let mut players: Vec<PlayerMetrics> = enc.players.iter().map(|p| {
        let (total, per) = dmg_by_rep.get(&p.agent_addr).cloned().unwrap_or_default();
        let taken = taken_by_rep.get(&p.agent_addr).copied().unwrap_or(0);
        PlayerMetrics { agent_addr: p.agent_addr, damage_total: total,
            dps: total as f64 / secs, damage_taken: taken,
            per_enemy: per.into_iter().collect(), ..Default::default() }
    }).collect();
    downs::apply(&mut players, enc, raw, &squad, &enemies, &addr_to_rep);
    cc::apply_cc(&mut players, raw, &squad, &enemies, &addr_to_rep);
    support::apply(&mut players, raw, enc, &enemies, &addr_to_rep);
    let timeline = cc::timeline(enc, raw, &squad, &enemies);
    // Computed last, after every other pass, per the Task 1 brief -- does
    // not read or alter `players`/`timeline` above.
    let boons = buffs::simulate_boons(raw, enc);
    // M3 Task 2: reduce each timeline to a `BoonUptime` over the same
    // absolute window `simulate_boons` itself ticks against (see
    // `buffs::uptime`'s module docs) -- computed directly from `boons`
    // rather than via `buffs::simulate_boon_uptimes` (which exists as a
    // standalone convenience for callers that only want uptimes) to avoid
    // re-running the simulator a second time.
    let log_start_ms = raw.events.first().map(|e| e.time).unwrap_or(0);
    let log_end_ms = raw.events.last().map(|e| e.time).unwrap_or(0);
    let boon_uptime = boons
        .iter()
        .map(|(&key, tl)| (key, buffs::uptime::compute(tl, log_start_ms, log_end_ms)))
        .collect();
    // M3 Task 4: self/group/squad generation attribution, over the same
    // absolute window as `boon_uptime` above (see `buffs::generation`'s
    // module docs) -- re-simulated with per-stack source tracking rather
    // than derived from `boons` (which only tracks stack COUNT, not WHICH
    // source's stack is held).
    let target_gen = buffs::generation::simulate_boon_generation_ms(raw, enc);
    let boon_generation = buffs::generation::rollup(&target_gen, enc, log_start_ms, log_end_ms);
    // Final-review fix wave: on a post-2026-05-01 (buff-statechange-rework)
    // build, this project's `events::extract_buff_events`/`support::apply`
    // only understand the OLDER pre-rework wire shape (see `Metrics::
    // warnings`'s doc comment) -- if that build era produced zero extracted
    // buff events, warn rather than silently emitting all-zero boon/support
    // metrics. Re-extracts (cheap: a single linear scan over the boon skill
    // ids) rather than threading a count out of `boons::simulate_boons`,
    // which already discards per-owner squad-membership before this point.
    let mut warnings = Vec::new();
    if raw.header.is_post_buff_rework() {
        let boon_id_set: BTreeSet<u32> = buffs::BOON_IDS.iter().map(|&(id, _, _)| id).collect();
        if buffs::events::extract_buff_events(raw, &boon_id_set).is_empty() {
            warnings.push(format!(
                "log build {} uses post-2026-05-01 buff statechange events; boon/support metrics are not yet supported for this era and will read zero",
                raw.header.build
            ));
        }
    }
    Metrics { players, timeline, boons, boon_uptime, boon_generation, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn strike(src: u64, dst: u64, dmg: i32) -> RawEvent {
        RawEvent { time: 0, src_agent: src, dst_agent: dst, value: dmg, buff_dmg: 0,
            overstack: 0, skillid: 1, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 1, buff: 0, result: 0,
            is_activation: 0, is_buffremove: 0, is_statechange: 0, is_shields: 0, is_offcycle: 0 }
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog { header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: vec![] }
    }

    /// Finding #1: a relogged/build-swapped account (two raw agent addrs,
    /// one `Player` after dedupe) must have damage from BOTH addrs summed
    /// into the single surviving `PlayerMetrics` entry, not dropped.
    #[test]
    fn relog_damage_aggregates_across_all_account_addrs() {
        let player = Player {
            agent_addr: 1, // representative (first-seen addr)
            account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![1, 2], // pre-relog addr 1, post-relog addr 2
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, agent_addrs: vec![9] }],
            markers: vec![], tick_rate: None,
        };
        let raw = raw_from(vec![
            strike(1, 9, 100), // pre-relog damage from addr 1
            strike(2, 9, 250), // post-relog damage from addr 2 (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players.len(), 1);
        assert_eq!(metrics.players[0].agent_addr, 1);
        assert_eq!(metrics.players[0].damage_total, 350); // 100 + 250, not just one addr
    }

    /// Finding #3: damage_taken sums incoming damage (from any source) for
    /// every one of an account's addrs, folded onto the representative.
    #[test]
    fn damage_taken_sums_incoming_damage_across_relog_addrs() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![1, 2],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, agent_addrs: vec![9] }],
            markers: vec![], tick_rate: None,
        };
        let raw = raw_from(vec![
            strike(9, 1, 80),  // enemy hits pre-relog addr
            strike(9, 2, 120), // enemy hits post-relog addr (same account)
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players[0].damage_taken, 200);
    }

    /// Task 4 (M2): a deduped enemy (e.g. a relogging enemy player, with
    /// `agent_addrs` covering both its raw addrs) must fold damage sent to
    /// EITHER addr into a single `per_enemy` entry keyed by the
    /// representative id -- not split into two rows, and not dropped for
    /// the non-representative addr.
    #[test]
    fn per_enemy_damage_folds_across_deduped_enemy_addrs() {
        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "A".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![1],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 2000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![],
            players: vec![player],
            // Enemy deduped from a relog: representative addr 9, but also
            // covers raw addr 10 (the post-relog addr).
            enemies: vec![Enemy { id: 9, instid: 0, name: "Foe".into(), team: "blue".into(),
                is_player: true, marker: None, agent_addrs: vec![9, 10] }],
            markers: vec![], tick_rate: None,
        };
        let raw = raw_from(vec![
            strike(1, 9, 100),  // damage to the enemy's pre-relog addr
            strike(1, 10, 250), // damage to the enemy's post-relog addr
        ]);
        let metrics = analyze(&enc, &raw);
        assert_eq!(metrics.players.len(), 1);
        assert_eq!(metrics.players[0].damage_total, 350, "both addrs' damage counted");
        assert_eq!(metrics.players[0].per_enemy.len(), 1, "folded into one per_enemy entry");
        assert_eq!(metrics.players[0].per_enemy[0], (9, 350), "keyed by the representative enemy id");
    }

    fn empty_enc() -> Encounter {
        Encounter { kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![], enemies: vec![],
            markers: vec![], tick_rate: None }
    }

    /// Final-review fix wave: a log whose arcdps build is on/after the
    /// post-2026-05-01 buff-statechange rework, with zero extracted buff
    /// events (this project's extractor only understands the older
    /// pre-rework wire shape -- see `Metrics::warnings`'s doc comment), must
    /// surface a non-empty `warnings` list naming the build.
    #[test]
    fn post_rework_build_with_no_buff_events_warns() {
        let raw = RawLog {
            header: RawHeader { build: "20260601".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events: vec![], guid_map: vec![],
        };
        let metrics = analyze(&empty_enc(), &raw);
        assert!(!metrics.warnings.is_empty(), "post-rework build with zero buff events must warn");
        assert!(metrics.warnings[0].contains("20260601"), "warning should name the offending build");
    }

    /// A pre-rework build produces no warnings (the ordinary case for every
    /// log this project currently supports), even with zero buff events.
    #[test]
    fn pre_rework_build_no_warnings() {
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events: vec![], guid_map: vec![],
        };
        let metrics = analyze(&empty_enc(), &raw);
        assert!(metrics.warnings.is_empty());
    }
}
