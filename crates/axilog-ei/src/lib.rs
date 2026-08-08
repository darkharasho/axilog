use serde_json::{json, Value};
use axilog_schema::Report;

fn color_to_team_id(color: &str) -> u64 {
    // EI numeric team ids; refine against golden wvWMapData in Task 16
    match color { "red" => 883, "blue" => 882, "green" => 881, _ => 0 }
}

pub fn to_ei_json(report: &Report) -> Value {
    let players: Vec<Value> = report.players.iter().map(|p| json!({
        "account": p.account,
        "character_name": p.character,
        // EI convention: `profession` is the elite-spec name when the
        // player has one active, else the base profession. `elite_spec` is
        // kept alongside for consumers that want the native split.
        "profession": if p.elite_spec.is_empty() { &p.profession } else { &p.elite_spec },
        "elite_spec": p.elite_spec,
        "teamID": color_to_team_id(&p.team),
        "group": p.subgroup,
        "notInSquad": !p.in_squad,
        "hasCommanderTag": p.commander,
        "dpsAll": [ { "dps": p.damage.dps.round() as i64, "damage": p.damage.total } ],
        "statsTargets": [ [ {
            "downContribution": p.down_contribution,
            "killed": p.kills_dealt,
            "downed": p.downs_dealt
        } ] ],
        "defenses": [ {
            "downCount": p.downs_taken,
            "deadCount": p.deaths,
            "damageTaken": p.damage_taken
        } ]
    })).collect();
    let targets: Vec<Value> = report.enemies.iter().map(|e| json!({
        "id": e.id, "name": e.name, "enemyPlayer": true,
        "teamID": color_to_team_id(&e.team)
    })).collect();
    json!({
        "fightName": format!("Detailed WvW - {}", report.encounter.map),
        "durationMS": report.encounter.duration_ms,
        "recordedBy": report.encounter.recorded_by,
        "success": true,
        "eliteInsightsVersion": null,
        "players": players,
        "targets": targets,
        "wvWMapData": {
            "redTeamID": color_to_team_id("red"),
            "blueTeamID": color_to_team_id("blue"),
            "greenTeamID": color_to_team_id("green")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_report() -> axilog_schema::Report {
        // Construct via axilog_schema public API by round-tripping from core types.
        use axilog_core::model::{Encounter, Player};
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:Some(":A.1".into()),
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"Daredevil".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:true,agent_addrs:vec![1]},
            Player{agent_addr:2,account:":B.2".into(),
            character:"B".into(),profession:"Guardian".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:false,agent_addrs:vec![2]}],
            enemies:vec![]};
        let m = Metrics{players:vec![
            PlayerMetrics{agent_addr:1,damage_total:500,dps:500.0,
            downs_dealt:1,kills_dealt:1,down_contribution:400,deaths:0,..Default::default()},
            PlayerMetrics{agent_addr:2,damage_total:300,dps:300.0,
            downs_dealt:0,kills_dealt:0,down_contribution:0,deaths:1,..Default::default()}],
            timeline:Timeline{resolution_ms:1000,squad_damage:vec![800],cc_applied:vec![0],downs:vec![0]}};
        axilog_schema::build_report(&enc,&m,"0.1.0")
    }
    #[test]
    fn maps_core_ei_fields() {
        let v = to_ei_json(&sample_report());
        assert_eq!(v["durationMS"], 1000);
        assert_eq!(v["recordedBy"], ":A.1");
        assert_eq!(v["players"][0]["account"], ":A.1");
        assert_eq!(v["players"][0]["character_name"], "A");
        // EI-style: elite-spec name wins over base profession when present.
        assert_eq!(v["players"][0]["profession"], "Daredevil");
        assert_eq!(v["players"][0]["elite_spec"], "Daredevil");
        // ...but falls back to the base profession for a core (no elite spec) build.
        assert_eq!(v["players"][1]["profession"], "Guardian");
        assert_eq!(v["players"][1]["elite_spec"], "");
        assert_eq!(v["players"][0]["hasCommanderTag"], true);
        assert_eq!(v["players"][0]["dpsAll"][0]["damage"], 500);
        assert_eq!(v["players"][0]["statsTargets"][0][0]["downContribution"], 400);
        assert_eq!(v["players"][0]["defenses"][0]["deadCount"], 0);
    }
}
