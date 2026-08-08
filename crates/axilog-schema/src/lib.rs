use serde::Serialize;
use axilog_core::model::Encounter;
use axilog_core::analysis::Metrics;

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
    pub teams: Vec<TeamOut> }
#[derive(Serialize)]
pub struct TeamOut { pub color: String, pub team_id: u16 }
#[derive(Serialize)]
pub struct DamageOut { pub total: u64, pub dps: f64, pub per_enemy: Vec<PerEnemyOut> }
#[derive(Serialize)]
pub struct PerEnemyOut { pub enemy_id: u64, pub total: u64 }
#[derive(Serialize)]
pub struct CcOut { pub applied_total: u32, pub applied_duration_ms: u64 }
#[derive(Serialize)]
pub struct PlayerOut { pub account: String, pub character: String, pub profession: String,
    pub elite_spec: String, pub team: String, pub subgroup: u8, pub in_squad: bool,
    pub commander: bool, pub damage: DamageOut, pub downs_dealt: u32, pub kills_dealt: u32,
    pub down_contribution: u64, pub downs_taken: u32, pub deaths: u32, pub damage_taken: u64,
    pub cc: CcOut }
#[derive(Serialize)]
pub struct EnemyOut { pub id: u64, pub name: String, pub team: String, pub is_player: bool }
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
                        applied_duration_ms: m.map(|m| m.cc_duration_ms).unwrap_or(0) },
        }
    }).collect();
    Report {
        schema_version: "0.1", axilog_version: axilog_version.to_string(),
        encounter: EncounterOut { kind: enc.kind.clone(), map: enc.map.clone(),
            duration_ms: enc.duration_ms, build: enc.build.clone(), revision: enc.revision,
            recorded_by: enc.recorded_by.clone(),
            teams: enc.teams.iter().map(|t| TeamOut{color:t.color.clone(),team_id:t.team_id}).collect() },
        players,
        enemies: enc.enemies.iter().map(|e| EnemyOut{id:e.id,name:e.name.clone(),
            team:e.team.clone(),is_player:e.is_player}).collect(),
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
            team:"red".into(),subgroup:1,in_squad:true,commander:false,agent_addrs:vec![1]}],
            enemies:vec![] };
        let m = Metrics { players: vec![PlayerMetrics{agent_addr:1,damage_total:500,
            dps:500.0,..Default::default()}],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![500],
            cc_applied:vec![0],downs:vec![0]} };
        let report = build_report(&enc, &m, "0.1.0");
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.1");
        assert_eq!(v["axilog_version"], "0.1.0");
        assert_eq!(v["players"][0]["damage"]["total"], 500);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
    }
}
