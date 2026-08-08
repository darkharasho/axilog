use std::collections::BTreeMap;
use serde_json::{json, Value};
use axilog_schema::Report;

// Representative real WvW team ids per color (Task 2, M2) — one id drawn
// from each of `axilog_core::wvw`'s RED/GREEN/BLUE_TEAM_IDS fixed tables
// (sourced from axibridge's `src/shared/wvwTeams.ts`, itself reconciled
// from Drevarr/EVTC_parser/gw2_data.py and
// Drevarr/GW2_EI_log_combiner/config.py). Used only as a fallback when a
// color wasn't actually observed in the log's own TEAM_CHANGE events (see
// `detected_team_ids` below) — e.g. a color with no roster presence in this
// particular fight.
fn representative_team_id(color: &str) -> u64 {
    match color {
        "red" => 697,
        "blue" => 432,
        "green" => 39,
        _ => 0,
    }
}

/// color -> the first real team id `wvw::apply` actually observed for it in
/// this log (`report.encounter.teams`, built from TEAM_CHANGE events).
fn detected_team_ids(report: &Report) -> BTreeMap<&str, u64> {
    let mut m = BTreeMap::new();
    for t in &report.encounter.teams {
        m.entry(t.color.as_str()).or_insert(t.team_id as u64);
    }
    m
}

pub fn to_ei_json(report: &Report) -> Value {
    let detected = detected_team_ids(report);
    let team_id_for = |color: &str| -> u64 {
        detected.get(color).copied().unwrap_or_else(|| representative_team_id(color))
    };

    let players: Vec<Value> = report.players.iter().map(|p| {
        // Real EI shape (verified against a real dps.report export,
        // axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].statsAll[0]`): whole-fight/whole-phase aggregates —
        // downContribution, killed, downed, appliedCrowdControl,
        // appliedCrowdControlDuration — live under `statsAll[phase]`, not
        // `statsTargets`. We don't model phases, so this is a single-element
        // array standing in for "phase 0 == the whole fight", matching how
        // EI itself collapses to one phase for logs with no phase splits.
        // `appliedCrowdControlDuration` is real EI's own ms convention (no
        // /1000 — cross-checked: our computed `cc_duration_ms=50460` equals
        // the golden fixture's `squadAppliedCrowdControlDuration=50460`
        // exactly, unlike the stun-break duration below which EI reports in
        // seconds). Every field here is backed by a real computed metric —
        // `appliedCrowdControlDownContribution` /
        // `appliedCrowdControlDurationDownContribution` (CC's own down-contribution
        // split) exist in real EI but we don't compute that finer breakdown,
        // so they're intentionally omitted rather than faked.
        let stats_all = json!([ {
            "downContribution": p.down_contribution,
            "killed": p.kills_dealt,
            "downed": p.downs_dealt,
            "appliedCrowdControl": p.cc.applied_total,
            "appliedCrowdControlDuration": p.cc.applied_duration_ms
        } ]);
        // Real EI's `statsTargets[targetIndex][phaseIndex]` carries a large
        // per-target breakdown (including its own per-target
        // downContribution/appliedCrowdControl split — see statsAll comment
        // above). We only compute one real per-target metric,
        // `damage.per_enemy` (damage dealt to that specific enemy), so that's
        // the only field we emit here — one entry per real `targets[]` enemy,
        // in the same order, `0` when this player dealt no damage to that
        // target. Everything else in real EI's per-target stats block would
        // have to be invented, so it's left out rather than faked.
        let stats_targets: Vec<Value> = report.enemies.iter().map(|e| {
            let dmg = p.damage.per_enemy.iter().find(|pe| pe.enemy_id == e.id)
                .map(|pe| pe.total).unwrap_or(0);
            json!([ { "totalDmg": dmg } ])
        }).collect();
        json!({
            "account": p.account,
            "character_name": p.character,
            // EI convention: `profession` is the elite-spec name when the
            // player has one active, else the base profession. `elite_spec` is
            // kept alongside for consumers that want the native split.
            "profession": if p.elite_spec.is_empty() { &p.profession } else { &p.elite_spec },
            "elite_spec": p.elite_spec,
            "teamID": team_id_for(&p.team),
            "group": p.subgroup,
            "notInSquad": !p.in_squad,
            "hasCommanderTag": p.commander,
            "dpsAll": [ { "dps": p.damage.dps.round() as i64, "damage": p.damage.total } ],
            "statsAll": stats_all,
            "statsTargets": stats_targets,
            "defenses": [ {
                "downCount": p.downs_taken,
                "deadCount": p.deaths,
                "damageTaken": p.damage_taken
            } ],
            // EI places stun-break stats under `support`, not `defenses` — verified
            // against GW2EI's `SupportAllStatistics` (StunBreakCount /
            // RemovedStunDuration) and the real dps.report EI JSON for the golden
            // WvW fixture (`support[0].stunBreak` / `support[0].removedStunDuration`,
            // not `defenses[0]`). `removedStunDuration` is EI's convention of
            // seconds (our native schema tracks whole ms).
            "support": [ {
                "stunBreak": p.cc.stun_breaks,
                "removedStunDuration": p.cc.removed_stun_duration_ms as f64 / 1000.0
            } ]
        })
    }).collect();
    let targets: Vec<Value> = report.enemies.iter().map(|e| json!({
        "id": e.id, "name": e.name, "enemyPlayer": true,
        "teamID": team_id_for(&e.team)
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
            "redTeamID": team_id_for("red"),
            "blueTeamID": team_id_for("blue"),
            "greenTeamID": team_id_for("green")
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
        use axilog_core::model::Enemy;
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:Some(":A.1".into()),
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"Daredevil".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:true,agent_addrs:vec![1]},
            Player{agent_addr:2,account:":B.2".into(),
            character:"B".into(),profession:"Guardian".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:false,agent_addrs:vec![2]}],
            enemies:vec![Enemy{id:9,instid:9,name:"Foe".into(),team:"blue".into(),
            is_player:true,agent_addrs:vec![9]}]};
        let m = Metrics{players:vec![
            PlayerMetrics{agent_addr:1,damage_total:500,dps:500.0,per_enemy:vec![(9,500)],
            downs_dealt:1,kills_dealt:1,down_contribution:400,deaths:0,
            cc_applied:3,cc_duration_ms:1200,..Default::default()},
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
        // Whole-fight aggregates (downContribution/killed/downed/CC) live
        // under statsAll[0], matching real EI's `statsAll[phase]` shape.
        assert_eq!(v["players"][0]["statsAll"][0]["downContribution"], 400);
        assert_eq!(v["players"][0]["statsAll"][0]["killed"], 1);
        assert_eq!(v["players"][0]["statsAll"][0]["downed"], 1);
        assert_eq!(v["players"][0]["statsAll"][0]["appliedCrowdControl"], 3);
        assert_eq!(v["players"][0]["statsAll"][0]["appliedCrowdControlDuration"], 1200);
        // Player 2 dealt no CC/downs — statsAll still present, all zero, not faked.
        assert_eq!(v["players"][1]["statsAll"][0]["appliedCrowdControl"], 0);
        // statsTargets has one entry per real target (one enemy here), only
        // carrying the one per-target metric we actually compute (damage).
        assert_eq!(v["players"][0]["statsTargets"].as_array().unwrap().len(), 1);
        assert_eq!(v["players"][0]["statsTargets"][0][0]["totalDmg"], 500);
        assert_eq!(v["players"][1]["statsTargets"][0][0]["totalDmg"], 0);
        assert_eq!(v["players"][0]["defenses"][0]["deadCount"], 0);
        assert_eq!(v["targets"][0]["id"], 9);
    }
}
