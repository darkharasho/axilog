use serde::Serialize;
use axilog_core::model::Encounter;
use axilog_core::analysis::{buffs, Metrics};

#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub axilog_version: String,
    pub encounter: EncounterOut,
    pub players: Vec<PlayerOut>,
    pub enemies: Vec<EnemyOut>,
    pub timeline: TimelineOut,
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
    pub support: SupportOut }
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

pub fn build_report(enc: &Encounter, metrics: &Metrics, axilog_version: &str) -> Report {
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
            boon_generation: Default::default() };
        let report = build_report(&enc, &m, "0.1.0");
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.1");
        assert_eq!(v["axilog_version"], "0.1.0");
        assert_eq!(v["players"][0]["damage"]["total"], 500);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
    }
}
