use serde::Serialize;
use axilog_core::model::Encounter;
use axilog_core::analysis::{buffs, Metrics};
use axilog_core::analysis::replay::Replay;

#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub axilog_version: String,
    pub encounter: EncounterOut,
    pub players: Vec<PlayerOut>,
    pub enemies: Vec<EnemyOut>,
    pub timeline: TimelineOut,
    /// Structured, user-facing analysis warnings (final-review fix wave) --
    /// see `axilog_core::analysis::Metrics::warnings`'s doc comment. Omitted
    /// entirely from the JSON (not serialized as `[]`) when there are none,
    /// matching this schema's existing omit-when-absent convention for
    /// other optional/empty-by-default fields (e.g. `TickRateOut`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Combat-replay position tracks (M9, Task 2), opt-in -- only present
    /// when the caller (CLI `--replay` / SDK `replay: true`) requested it
    /// by passing `Some(&Replay)` to [`build_report`]. Omitted entirely
    /// from the JSON (not serialized as `null`) rather than emitted as
    /// `None`, matching this schema's existing omit-when-absent convention
    /// for other opt-in/optional fields (e.g. `TickRateOut`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayOut>,
}
/// Native-only combat-replay block (M9, Task 2) -- see
/// `axilog_core::analysis::replay` for how `poll_ms`/`tracks`/intervals are
/// computed. `bounds` is the min/max `x`/`y` observed across every track's
/// samples, letting an HTML/JS consumer size a viewBox without a second
/// pass over `tracks`.
#[derive(Serialize)]
pub struct ReplayOut {
    pub poll_ms: u64,
    pub bounds: ReplayBoundsOut,
    pub tracks: Vec<ReplayTrackOut>,
}
#[derive(Serialize)]
pub struct ReplayBoundsOut {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}
/// One tracked agent's replay data. `name`/`team`/`commander`/`is_squad`
/// mirror `axilog_core::analysis::replay::Track`'s own fields verbatim
/// (already the right display-field precedence -- `Player::character` for
/// squad players, `Enemy::name` for enemy-player representatives -- per
/// that module's doc comment). `samples` are `[t_ms, x, y]` triples, `x`/`y`
/// rounded to 1 decimal place to keep the embedded JSON small; `t_ms` is
/// left exact (already an integer grid position, not a lossy float).
/// `down_intervals`/`dead_intervals` are `[start_ms, end_ms]` pairs.
#[derive(Serialize)]
pub struct ReplayTrackOut {
    pub name: String,
    pub team: String,
    pub commander: bool,
    pub is_squad: bool,
    pub samples: Vec<(u64, f64, f64)>,
    pub down_intervals: Vec<(u64, u64)>,
    pub dead_intervals: Vec<(u64, u64)>,
}

/// Round to 1 decimal place -- keeps `ReplayTrackOut.samples`' JSON
/// representation compact (per the M9 Task 2 brief) without materially
/// affecting on-screen replay accuracy (sub-pixel at any sane map zoom).
fn round1(v: f32) -> f64 {
    (v as f64 * 10.0).round() / 10.0
}

/// Build the native [`ReplayOut`] schema block from a computed [`Replay`]
/// (`axilog_core::analysis::replay::build_replay`). Standalone from
/// [`build_report`] rather than folded into it directly, so a caller that
/// only wants the replay block (or wants to build it once and reuse it)
/// doesn't have to re-thread it through the whole report-building path;
/// [`build_report`] itself takes an already-built `Option<&Replay>` and
/// calls this internally when `Some`.
pub fn build_replay_out(replay: &Replay) -> ReplayOut {
    let tracks: Vec<ReplayTrackOut> = replay
        .tracks
        .iter()
        .map(|t| ReplayTrackOut {
            name: t.name.clone(),
            team: t.team.clone(),
            commander: t.commander,
            is_squad: t.is_squad,
            samples: t.samples.iter().map(|s| (s.t_ms, round1(s.x), round1(s.y))).collect(),
            down_intervals: t.down_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
            dead_intervals: t.dead_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
        })
        .collect();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for t in &tracks {
        for &(_, x, y) in &t.samples {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    // No samples anywhere (e.g. a log with no position telemetry at all) --
    // fall back to a degenerate zero-sized bounds rather than emit
    // infinities.
    if !min_x.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    }

    ReplayOut {
        poll_ms: replay.poll_ms,
        bounds: ReplayBoundsOut { min_x, min_y, max_x, max_y },
        tracks,
    }
}
#[derive(Serialize)]
pub struct EncounterOut { pub kind: String, pub map: String, pub duration_ms: u64,
    pub build: String, pub revision: u8, pub recorded_by: Option<String>,
    pub teams: Vec<TeamOut>,
    /// Every `CBTS_MARKER` assignment observed in the log (Task 7, M2),
    /// across all agents -- not just squad/enemy players. Native-only: EI's
    /// JSON has no comparable field. Empty (not omitted) when the log has
    /// no marker events, consistent with `teams`/`players` always being
    /// present arrays.
    pub markers: Vec<MarkerAssignmentOut>,
    /// Tick-rate telemetry from `CBTS_TICK` (Task 7, M2). Native-only.
    /// Omitted entirely (not `null`) when the log has fewer than two
    /// `CBTS_TICK` events -- mirrors `TeamOut.guid`'s omit-when-absent
    /// convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_rate: Option<TickRateOut> }
#[derive(Serialize)]
pub struct MarkerAssignmentOut { pub agent_addr: u64, pub marker: String, pub time_ms: u64 }
#[derive(Serialize)]
pub struct TickRateOut { pub avg: f64, pub min: f64, pub per_second: Vec<f64> }
#[derive(Serialize)]
pub struct CommanderTagOut { pub variant: String, pub guid: String }
#[derive(Serialize)]
pub struct TeamOut {
    pub color: String,
    pub team_id: u16,
    /// Stable content GUID for this team (Task 2b), when known. Omitted
    /// entirely from the JSON when absent, rather than serialized as null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
}
#[derive(Serialize)]
pub struct DamageOut { pub total: u64, pub dps: f64, pub per_enemy: Vec<PerEnemyOut> }
#[derive(Serialize)]
pub struct PerEnemyOut { pub enemy_id: u64, pub total: u64 }
#[derive(Serialize)]
pub struct CcOut { pub applied_total: u32, pub applied_duration_ms: u64,
    pub stun_breaks: u32, pub removed_stun_duration_ms: u64 }
/// Self/group/squad boon-generation attribution (M3, Task 4) -- see
/// `axilog_core::analysis::buffs::GenerationStats`'s doc comment for the
/// exact scope of each field (mirrors `BuffStatistics.GetBuffsForSelf`/
/// `GetBuffsForPlayers` 1:1). Same 0-100 (duration boons) / raw
/// average-concurrent-stack-count (intensity boons, no `*100`) scale as
/// `BoonOut.presence_pct`/`avg_stacks`.
#[derive(Serialize)]
pub struct GenerationOut { pub self_pct: f64, pub group_pct: f64, pub squad_pct: f64 }
/// One tracked boon's whole-fight summary for one player (M3, Tasks 1-4).
/// `presence_pct` is EI's "% of the fight with >=1 held stack" for every
/// boon (0-100). `avg_stacks` (time-weighted mean held-stack count) is only
/// meaningful -- and only serialized -- for the two INTENSITY-type boons
/// (Might, Stability); it's always 0 for the other 10 (duration-type)
/// boons, so it's omitted there rather than serialized as a meaningless
/// zero (see `buffs::uptime`'s module doc for the EI field-meaning source).
#[derive(Serialize)]
pub struct BoonOut {
    pub id: u32,
    pub name: String,
    pub presence_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    pub generation: GenerationOut,
}
/// Condition-cleanse/boon-strip/resurrect counts (M3, Task 3). Stun-break
/// counts stay on `CcOut` (already there since M1) rather than duplicated
/// here -- see the Task 5 brief.
#[derive(Serialize)]
pub struct SupportOut { pub cleanses: u32, pub cleanses_self: u32, pub strips: u32, pub resurrects: u32 }
/// arcdps healing-extension totals (M10, Task 1) -- see
/// `axilog_core::analysis::healing::HealingMetrics`'s doc comment for the
/// exact field definitions (mirrors EI's `extHealingStats`/
/// `extBarrierStats` "outgoing" scalars, `healing_out_allies` derived as
/// `healing_out_total - healing_out_self` -- see that module's doc for why).
#[derive(Serialize)]
pub struct HealingOut {
    pub healing_out_total: u64,
    pub healing_out_allies: u64,
    pub healing_out_self: u64,
    pub barrier_out: u64,
    pub downed_healing_out: u64,
}
#[derive(Serialize)]
pub struct PlayerOut { pub account: String, pub character: String, pub profession: String,
    pub elite_spec: String, pub team: String, pub subgroup: u8, pub in_squad: bool,
    pub commander: bool,
    /// The player's current squad marker (Task 7, M2), name or hex GUID
    /// fallback. Omitted when no marker is currently assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Commander-tag colour/variant (Task 7, M2), when `commander` is
    /// true. Native-only richer form alongside the plain `commander` bool
    /// (kept for compatibility). Omitted when the player has no commander
    /// tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander_tag: Option<CommanderTagOut>,
    pub damage: DamageOut, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32, pub damage_taken: u64,
    pub cc: CcOut,
    /// Per-tracked-boon uptime/generation summary (M3, Tasks 1-4), one
    /// entry per `buffs::BOON_IDS` id, in that table's order.
    pub boons: Vec<BoonOut>,
    /// Support-stat counts (M3, Task 3).
    pub support: SupportOut,
    /// arcdps healing-extension totals (M10, Task 1). `None` (omitted from
    /// the JSON entirely, not serialized as `null`) when the log carries no
    /// healing-extension data at all (`Metrics::has_healing_extension ==
    /// false`) -- see `HealingOut`'s doc comment and `Metrics::
    /// has_healing_extension`'s doc comment for why this is a real
    /// "no data" signal, not "genuinely all zero".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing: Option<HealingOut> }
#[derive(Serialize)]
pub struct EnemyOut { pub id: u64, pub name: String, pub team: String, pub is_player: bool,
    /// The enemy's current squad marker, mirroring `PlayerOut.marker`
    /// (Task 7, M2). Omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String> }
#[derive(Serialize)]
pub struct TimelineOut { pub resolution_ms: u64, pub per_second: PerSecondOut }
#[derive(Serialize)]
pub struct PerSecondOut { pub squad_damage: Vec<u64>, pub cc_applied: Vec<u32>, pub downs: Vec<u32> }

/// `replay` (M9, Task 2): pass `Some(&Replay)` (from
/// `axilog_core::analysis::replay::build_replay`, called separately by the
/// caller when `--replay`/SDK `replay: true` was requested) to embed the
/// native replay block; `None` (the default for every existing call site)
/// omits it entirely, matching `ReplayOut`'s skip-when-absent serde
/// attribute.
pub fn build_report(enc: &Encounter, metrics: &Metrics, axilog_version: &str, replay: Option<&Replay>) -> Report {
    let pm: std::collections::BTreeMap<u64, &axilog_core::analysis::PlayerMetrics> =
        metrics.players.iter().map(|p| (p.agent_addr, p)).collect();
    let players = enc.players.iter().map(|p| {
        let m = pm.get(&p.agent_addr);
        PlayerOut {
            account: p.account.clone(), character: p.character.clone(),
            profession: p.profession.clone(), elite_spec: p.elite_spec.clone(),
            team: p.team.clone(), subgroup: p.subgroup, in_squad: p.in_squad,
            commander: p.commander,
            marker: p.marker.clone(),
            commander_tag: p.commander_tag.as_ref().map(|t| CommanderTagOut { variant: t.variant.clone(), guid: t.guid.clone() }),
            damage: DamageOut {
                total: m.map(|m| m.damage_total).unwrap_or(0),
                dps: m.map(|m| m.dps).unwrap_or(0.0),
                per_enemy: m.map(|m| m.per_enemy.iter()
                    .map(|(id,t)| PerEnemyOut{enemy_id:*id,total:*t}).collect())
                    .unwrap_or_default(),
            },
            downs_dealt: m.map(|m| m.downs_dealt).unwrap_or(0),
            kills_dealt: m.map(|m| m.kills_dealt).unwrap_or(0),
            down_contribution: m.map(|m| m.down_contribution).unwrap_or(0),
            downs_taken: m.map(|m| m.downs_taken).unwrap_or(0),
            deaths: m.map(|m| m.deaths).unwrap_or(0),
            damage_taken: m.map(|m| m.damage_taken).unwrap_or(0),
            cc: CcOut { applied_total: m.map(|m| m.cc_applied).unwrap_or(0),
                        applied_duration_ms: m.map(|m| m.cc_duration_ms).unwrap_or(0),
                        stun_breaks: m.map(|m| m.stun_breaks).unwrap_or(0),
                        removed_stun_duration_ms: m.map(|m| m.removed_stun_duration_ms).unwrap_or(0) },
            boons: buffs::BOON_IDS.iter().map(|&(id, name, is_intensity)| {
                let u = metrics.boon_uptime.get(&(p.agent_addr, id)).copied()
                    .unwrap_or(buffs::BoonUptime { presence_pct: 0.0, avg_stacks: 0.0 });
                let g = metrics.boon_generation.get(&(p.agent_addr, id)).copied().unwrap_or_default();
                BoonOut {
                    id, name: name.to_string(), presence_pct: u.presence_pct,
                    avg_stacks: if is_intensity { Some(u.avg_stacks) } else { None },
                    generation: GenerationOut { self_pct: g.self_pct, group_pct: g.group_pct, squad_pct: g.squad_pct },
                }
            }).collect(),
            support: m.map(|m| SupportOut {
                cleanses: m.support.cleanses, cleanses_self: m.support.cleanses_self,
                strips: m.support.strips, resurrects: m.support.resurrects,
            }).unwrap_or(SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, resurrects: 0 }),
            healing: if metrics.has_healing_extension {
                Some(m.map(|m| HealingOut {
                    healing_out_total: m.healing.healing_out_total,
                    healing_out_allies: m.healing.healing_out_allies,
                    healing_out_self: m.healing.healing_out_self,
                    barrier_out: m.healing.barrier_out,
                    downed_healing_out: m.healing.downed_healing_out,
                }).unwrap_or(HealingOut { healing_out_total: 0, healing_out_allies: 0,
                    healing_out_self: 0, barrier_out: 0, downed_healing_out: 0 }))
            } else {
                None
            },
        }
    }).collect();
    Report {
        schema_version: "0.1", axilog_version: axilog_version.to_string(),
        encounter: EncounterOut { kind: enc.kind.clone(), map: enc.map.clone(),
            duration_ms: enc.duration_ms, build: enc.build.clone(), revision: enc.revision,
            recorded_by: enc.recorded_by.clone(),
            teams: enc.teams.iter().map(|t| TeamOut{color:t.color.clone(),team_id:t.team_id,guid:t.guid.clone()}).collect(),
            markers: enc.markers.iter().map(|m| MarkerAssignmentOut{agent_addr:m.agent_addr,marker:m.marker.clone(),time_ms:m.time_ms}).collect(),
            tick_rate: enc.tick_rate.as_ref().map(|t| TickRateOut{avg:t.avg,min:t.min,per_second:t.per_second.clone()}) },
        players,
        enemies: enc.enemies.iter().map(|e| EnemyOut{id:e.id,name:e.name.clone(),
            team:e.team.clone(),is_player:e.is_player,marker:e.marker.clone()}).collect(),
        timeline: TimelineOut { resolution_ms: metrics.timeline.resolution_ms,
            per_second: PerSecondOut { squad_damage: metrics.timeline.squad_damage.clone(),
                cc_applied: metrics.timeline.cc_applied.clone(),
                downs: metrics.timeline.downs.clone() } },
        warnings: metrics.warnings.clone(),
        replay: replay.map(build_replay_out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{Encounter, Player};
    use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
    #[test]
    fn serializes_report_with_versions() {
        let enc = Encounter { kind:"wvw".into(), map:"Eternal Battlegrounds".into(),
            duration_ms:1000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]}],
            enemies:vec![], markers:vec![], tick_rate:None };
        let m = Metrics { players: vec![PlayerMetrics{agent_addr:1,damage_total:500,
            dps:500.0,..Default::default()}],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![500],
            cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None);
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.1");
        assert_eq!(v["axilog_version"], "0.1.0");
        assert_eq!(v["players"][0]["damage"]["total"], 500);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
        assert!(v.get("replay").is_none(), "replay must be omitted when not requested");
        assert!(
            v["players"][0].get("healing").is_none(),
            "healing must be omitted when has_healing_extension is false"
        );
    }

    /// M10 Task 1: `healing` is present (not omitted) once `Metrics::
    /// has_healing_extension` is true, even for a player whose own
    /// `HealingMetrics` is all-zero (a real extension log where this
    /// specific player never healed) -- `has_healing_extension` gates the
    /// WHOLE block's presence, not each player's own totals.
    #[test]
    fn healing_block_present_when_extension_detected() {
        use axilog_core::analysis::healing::HealingMetrics;
        let enc = Encounter { kind:"wvw".into(), map:"".into(),
            duration_ms:1000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![
                Player{agent_addr:1,account:":A.1".into(),character:"A".into(),
                    profession:"Thief".into(),elite_spec:"".into(),team:"red".into(),
                    subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]},
                Player{agent_addr:2,account:":B.1".into(),character:"B".into(),
                    profession:"Guardian".into(),elite_spec:"".into(),team:"red".into(),
                    subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![2]},
            ],
            enemies:vec![], markers:vec![], tick_rate:None };
        let m = Metrics { players: vec![
            PlayerMetrics{agent_addr:1,
                healing: HealingMetrics { healing_out_total: 500, healing_out_allies: 300,
                    healing_out_self: 200, barrier_out: 50, downed_healing_out: 10 },
                ..Default::default()},
            PlayerMetrics{agent_addr:2, ..Default::default()}, // never healed
        ],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: true };
        let report = build_report(&enc, &m, "0.1.0", None);
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["players"][0]["healing"]["healing_out_total"], 500);
        assert_eq!(v["players"][0]["healing"]["healing_out_allies"], 300);
        assert_eq!(v["players"][0]["healing"]["healing_out_self"], 200);
        assert_eq!(v["players"][0]["healing"]["barrier_out"], 50);
        assert_eq!(v["players"][0]["healing"]["downed_healing_out"], 10);
        // Second player never healed but the extension IS present overall
        // -- their `healing` block must still be present, all-zero.
        assert_eq!(v["players"][1]["healing"]["healing_out_total"], 0);
        assert!(v["players"][1]["healing"].is_object(), "healing block present even when all-zero, since the extension is present overall");
    }

    #[test]
    fn replay_block_present_and_rounded_when_requested() {
        use axilog_core::analysis::replay::{build_replay, DEFAULT_POLL_MS};
        use axilog_core::evtc::{sc, RawEvent, RawHeader, RawLog};
        use axilog_core::model::Player;

        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "Alice".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: true, marker: None, commander_tag: None,
            agent_addrs: vec![1],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![player],
            enemies: vec![], markers: vec![], tick_rate: None,
        };
        let mut dst = [0u8; 8];
        dst[0..4].copy_from_slice(&123.456f32.to_le_bytes());
        dst[4..8].copy_from_slice(&(-9.87f32).to_le_bytes());
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![],
            events: vec![RawEvent {
                time: 0, src_agent: 1, dst_agent: u64::from_le_bytes(dst), value: 0,
                buff_dmg: 0, overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
                src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0, result: 0,
                is_activation: 0, is_buffremove: 0, is_statechange: sc::POSITION,
                is_shields: 0, is_offcycle: 0, pad: 0,
            }],
            guid_map: vec![],
        };
        let m = Metrics { players: vec![], timeline: Timeline { resolution_ms: 1000, squad_damage: vec![], cc_applied: vec![], downs: vec![] },
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default() };
        let replay = build_replay(&raw, &enc, DEFAULT_POLL_MS);
        let report = build_report(&enc, &m, "0.1.0", Some(&replay));
        assert!(report.replay.is_some());
        let r = report.replay.unwrap();
        assert_eq!(r.poll_ms, DEFAULT_POLL_MS);
        assert_eq!(r.tracks.len(), 1);
        assert_eq!(r.tracks[0].name, "Alice");
        assert_eq!(r.tracks[0].samples[0], (0, 123.5, -9.9), "x/y rounded to 1dp");
    }
}
