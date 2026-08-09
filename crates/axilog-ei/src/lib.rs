use std::collections::BTreeMap;
use serde_json::{json, Value};
use axilog_core::analysis::buffs::BOON_IDS;
use axilog_core::analysis::replay::{ActivityIntervals, Interval};
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

/// Render one interval as EI's own `[start_ms, end_ms]` two-element array
/// shape (`combatReplayData.down`/`.dead`).
fn interval_json(iv: &Interval) -> Value {
    json!([iv.start_ms, iv.end_ms])
}

/// `activity` (M11 Task 3): per-player down/dead intervals + first/last-
/// aware bounds from `axilog_core::analysis::replay::build_activity_intervals`
/// -- ALWAYS computed by every caller (CLI/Node/Python), unlike `--replay`'s
/// position track, since intervals are cheap (see that function's module
/// doc). Positionally joined to `report.players` (both built by iterating
/// `enc.players` in the same order -- see `build_activity_intervals`'s doc
/// comment); pass an empty slice if unavailable (every field this powers is
/// then a harmless zero/empty default, not a panic).
pub fn to_ei_json(report: &Report, activity: &[ActivityIntervals]) -> Value {
    let detected = detected_team_ids(report);
    let team_id_for = |color: &str| -> u64 {
        detected.get(color).copied().unwrap_or_else(|| representative_team_id(color))
    };

    // M10 Task 1: whole-fight seconds for the healing-extension `hps`/`bps`
    // fields below -- same `(duration_ms / 1000.0).max(1.0)` convention
    // `axilog_core::analysis::analyze` itself uses for `dps` (avoids a
    // divide-by-zero on a degenerate zero-duration log).
    let duration_secs = (report.encounter.duration_ms as f64 / 1000.0).max(1.0);

    let players: Vec<Value> = report.players.iter().enumerate().map(|(player_idx, p)| {
        let act = activity.get(player_idx);
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
        //
        // M11 Task 2: `downContribution` now maps to `p.downs_contribution.
        // damage` (the arcdps-methodology `damage_to_downs` value), NOT
        // EI's own down-contribution number — EI computes down-contribution
        // with a DIFFERENT algorithm BY DESIGN (see `axilog_core::analysis::
        // contribution`'s module doc: no EI golden exists to calibrate this
        // engine against, by design, since the whole point of this project's
        // founding differentiator is to diverge from EI's approximation and
        // match the real arcdps methodology instead). This mapping is a
        // best-effort "closest real EI field for this concept" placement,
        // not a parity claim — a consumer wanting EI's OWN algorithm's
        // number has no equivalent field in this adapter's output at all.
        let stats_all = json!([ {
            "downContribution": p.downs_contribution.damage,
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
        //
        // M10 Task 3: reads `report.all_enemies` (the FULL, unfiltered
        // roster), not `report.enemies` (which is now filtered to combat
        // participants for the native/HTML consumers) -- real EI's own
        // `targets[]` keeps every enumerated target regardless of
        // interaction, and `statsTargets[][]` is positionally keyed to
        // `targets[]` below, so both must stay in lockstep off the same
        // unfiltered list to preserve that faithfulness.
        let stats_targets: Vec<Value> = report.all_enemies.iter().map(|e| {
            let dmg = p.damage.per_enemy.iter().find(|pe| pe.enemy_id == e.id)
                .map(|pe| pe.total).unwrap_or(0);
            json!([ { "totalDmg": dmg } ])
        }).collect();
        let mut v = json!({
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
            // seconds (our native schema tracks whole ms). M3 Task 5: extended with
            // the condi-cleanse/boon-strip/resurrect counts (M3 Task 3) — field
            // names verified against a real dps.report EI export
            // (axibridge's `test-fixtures/boon/20260117-181030.json`,
            // `players[0].support[0]`: `condiCleanse`, `condiCleanseSelf`,
            // `boonStrips`, `resurrects`). The `*Time`/`boonStripDownContribution*`
            // sibling fields real EI also carries aren't computed here, so they're
            // omitted rather than faked (same convention as `statsTargets` above).
            "support": [ {
                "stunBreak": p.cc.stun_breaks,
                "removedStunDuration": p.cc.removed_stun_duration_ms as f64 / 1000.0,
                "condiCleanse": p.support.cleanses,
                "condiCleanseSelf": p.support.cleanses_self,
                "boonStrips": p.support.strips,
                "resurrects": p.support.resurrects
            } ],
            // `buffUptimes[]` (M3 Task 5): one entry per tracked boon (the 12 in
            // `BOON_IDS`), in that table's order. Field meanings verified against
            // GW2EI's own `uptime`/`presence` semantics (see
            // `axilog_core::analysis::buffs::uptime`'s module doc, cross-checked
            // against the same real dps.report export): for DURATION-type boons
            // `uptime` is our `presence_pct` and `presence` is always 0 (EI never
            // populates it for that branch); for INTENSITY-type boons (Might,
            // Stability) `uptime` is our raw `avg_stacks` (no `*100`) and
            // `presence` is our `presence_pct`. `generated` in real EI is a full
            // per-source-character-name ms/pct map; we only compute a self/group/
            // squad ROLLUP (M3 Task 4), not a per-character breakdown, so the only
            // entry we can honestly attribute to a specific character name is the
            // player's own self-generation — emitted as `{ <own character>:
            // self_pct }` rather than fabricating group/squad members' names.
            "buffUptimes": p.boons.iter().map(|b| {
                let (uptime, presence) = match b.avg_stacks {
                    Some(avg) => (avg, b.presence_pct), // intensity boon
                    None => (b.presence_pct, 0.0),      // duration boon
                };
                json!({
                    "id": b.id,
                    "buffData": [ {
                        "uptime": uptime,
                        "presence": presence,
                        "generated": { &p.character: b.generation.self_pct }
                    } ]
                })
            }).collect::<Vec<_>>()
        });
        // `activeTimes`/`combatReplayData` (M11 Task 3): unlike every other
        // block on this player, these are ALWAYS present -- not gated on
        // `--replay`/SDK `replay: true` (the module doc on
        // `axilog_core::analysis::replay::build_activity_intervals` explains
        // why: down/dead intervals and first/last-aware bounds are cheap,
        // only the downsampled `positions[]` track is expensive, and that
        // stays absent here regardless -- a consumer wanting the position
        // MAP (not just down/dead-derived features) still needs `--replay`'s
        // native `replay` block, deferred to M15 for ei-json specifically).
        // `activeTimes` real EI shape: a single-element array (this
        // project's one-phase-only convention, matching `statsAll`/
        // `extHealingStats` above) holding
        // `SingleActor.GetActiveDuration(log, 0, durationMS)` --
        // `ActivityIntervals::active_ms`'s doc comment has the full GW2EI
        // source citation and the real-golden verification that down time
        // is NOT subtracted (only dead time is). `combatReplayData.start`/
        // `.end` mirror GW2EI's `AgentItem.FirstAware`/`LastAware`;
        // `.down`/`.dead` are `[[start_ms, end_ms], ...]` arrays verified
        // byte-exact against the committed EI golden
        // (`crates/axilog-ei/tests/ei_golden.rs`) -- `.positions`/
        // `.orientations`/`.iconURL`/`.dc` are real EI fields NOT computed
        // here, omitted rather than faked (same "don't fake absent data"
        // convention as `statsTargets`/`support` above).
        {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            let (active_ms, start_ms, end_ms, down, dead) = match act {
                Some(a) => (
                    a.active_ms(),
                    a.start_ms,
                    a.end_ms,
                    a.down_intervals.iter().map(interval_json).collect::<Vec<_>>(),
                    a.dead_intervals.iter().map(interval_json).collect::<Vec<_>>(),
                ),
                None => (0, 0, 0, Vec::new(), Vec::new()),
            };
            obj.insert("activeTimes".to_string(), json!([active_ms]));
            obj.insert(
                "combatReplayData".to_string(),
                json!({
                    "start": start_ms,
                    "end": end_ms,
                    "down": down,
                    "dead": dead
                }),
            );
        }
        // `extHealingStats`/`extBarrierStats` (M10 Task 1): only when this
        // log carries the healing extension at all (`p.healing` is `None`
        // otherwise -- `axilog_schema::PlayerOut.healing`'s doc comment).
        // Real EI shape (verified against a real dps.report export,
        // axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].extHealingStats.outgoingHealing[0]`/
        // `extBarrierStats.outgoingBarrier[0]`): both are single-phase
        // arrays, same "one array standing in for phase 0 == the whole
        // fight" convention `statsAll` above already uses. Only the exact-
        // named real EI fields we actually compute are emitted --
        // `healing`/`downedHealing`/`hps`/`downedHps` (outgoing, target=
        // null, i.e. every friendly-directed heal including self, per
        // `axilog_core::analysis::healing`'s module doc) and `barrier`/
        // `bps`. Real EI's `healingPowerHealing`/`conversionHealing`/
        // `hybridHealing` (skill-type breakdown), `outgoingHealingAllies`
        // (per-friendly array), `incomingHealing`, and every `*1S`/`*Dist`
        // field are NOT computed by this project (native-only
        // `healing_out_self`/`healing_out_allies` cover the self/ally
        // split instead -- see the native schema's `players[].healing`,
        // which has no direct EI-field-name equivalent to reuse here) --
        // omitted rather than faked, same convention as `statsTargets`/
        // `support` above.
        if let Some(h) = &p.healing {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert(
                "extHealingStats".to_string(),
                json!({
                    "outgoingHealing": [ {
                        "healing": h.healing_out_total,
                        "hps": (h.healing_out_total as f64 / duration_secs).round() as i64,
                        "downedHealing": h.downed_healing_out,
                        "downedHps": (h.downed_healing_out as f64 / duration_secs).round() as i64,
                    } ]
                }),
            );
            obj.insert(
                "extBarrierStats".to_string(),
                json!({
                    "outgoingBarrier": [ {
                        "barrier": h.barrier_out,
                        "bps": (h.barrier_out as f64 / duration_secs).round() as i64,
                    } ]
                }),
            );
        }
        v
    }).collect();
    // M10 Task 3: `all_enemies`, not `enemies` -- see the `stats_targets`
    // comment above for why (positional lockstep with `statsTargets[][]`,
    // and EI parity: real EI keeps every enumerated target).
    //
    // M11 Task 3: `isFake` -- real EI sets this `true` for its own
    // synthetic aggregate pseudo-targets (a "sum of every real target"
    // stand-in row it adds to `targets[]` for certain fight types); every
    // one of THIS project's `all_enemies` is a real, individually-tracked
    // agent (never a synthesized aggregate), so `isFake: false` is correct
    // for every row here, not a faked/guessed value. This field was
    // previously ABSENT entirely. Verified against axibridge's own source
    // (read-only reference, `src/renderer/**`): every `targets[]` consumer
    // found (`ExpandableLogCard.tsx`, `computeCommanderStats.ts`,
    // `computeFightDiffMode.ts`, `computeFightBreakdown.ts`,
    // `incrementalAggregation.ts`) reads it via `!t.isFake`/`t?.isFake`
    // (optional-chained or negated), which already treated an absent field
    // as falsy/"not fake" the same as an explicit `false` -- so this
    // specific project never hit a live miscount from the omission. It's
    // still the correct fix: axibridge's own `dpsReportTypes.ts` declares
    // `isFake: boolean` as NON-optional (every real dps.report/EI export
    // always carries it), so an absent field was already a silent
    // contract violation for any future/less-defensive consumer that reads
    // `t.isFake` directly rather than through the truthy-check pattern
    // every current call site happens to use.
    let targets: Vec<Value> = report.all_enemies.iter().map(|e| json!({
        "id": e.id, "name": e.name, "enemyPlayer": e.is_player,
        "teamID": team_id_for(&e.team), "isFake": false
    })).collect();
    // `buffMap` (M3 Task 5): a subset covering only the 12 tracked boons,
    // keyed `"b<id>"` per real EI's convention (verified against
    // axibridge's `test-fixtures/boon/20260117-181030.json`,
    // `buffMap.b740` etc). Only the two fields we actually compute/know:
    // `name` and `stacking` (`true` for the two Intensity-type boons —
    // Might, Stability — `false` for the other 10 Queue/duration-type
    // ones, matching real EI's own `stacking` bool exactly for every one
    // of the 12 ids in that same fixture). Real EI's sibling fields
    // (`classification`, `icon`, `conversionBasedHealing`, `hybridHealing`,
    // `descriptions`) aren't computed here, so they're omitted rather than
    // faked.
    let buff_map: BTreeMap<String, Value> = BOON_IDS.iter().map(|&(id, name, is_intensity)| {
        (format!("b{id}"), json!({ "name": name, "stacking": is_intensity }))
    }).collect();
    json!({
        "fightName": format!("Detailed WvW - {}", report.encounter.map),
        "durationMS": report.encounter.duration_ms,
        "recordedBy": report.encounter.recorded_by,
        "success": true,
        "eliteInsightsVersion": null,
        "players": players,
        "targets": targets,
        "buffMap": buff_map,
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
            team:"red".into(),subgroup:2,in_squad:true,commander:true,marker:None,commander_tag:None,agent_addrs:vec![1]},
            Player{agent_addr:2,account:":B.2".into(),
            character:"B".into(),profession:"Guardian".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![2]}],
            enemies:vec![Enemy{id:9,instid:9,name:"Foe".into(),team:"blue".into(),
            is_player:true,marker:None,agent_addrs:vec![9]},
            Enemy{id:10,instid:10,name:"Gadget".into(),team:"blue".into(),
            is_player:false,marker:None,agent_addrs:vec![10]}],
            markers:vec![],tick_rate:None};
        use axilog_core::analysis::contribution::ContributionMetrics;
        let m = Metrics{players:vec![
            PlayerMetrics{agent_addr:1,damage_total:500,dps:500.0,per_enemy:vec![(9,500)],
            downs_dealt:1,kills_dealt:1,
            downs_contribution: ContributionMetrics{damage:400,..Default::default()},deaths:0,
            cc_applied:3,cc_duration_ms:1200,..Default::default()},
            PlayerMetrics{agent_addr:2,damage_total:300,dps:300.0,
            downs_dealt:0,kills_dealt:0,deaths:1,..Default::default()}],
            timeline:Timeline{resolution_ms:1000,squad_damage:vec![800],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(),
            // M10 Task 3: enemy 9 (player) took damage -- a real combat
            // participant; enemy 10 (gadget) never interacted, matching what
            // `analyze()` would actually compute for this exact scenario.
            // `to_ei_json` reads `report.all_enemies` (unfiltered) though,
            // so this doesn't change `maps_core_ei_fields`'s `targets[]`
            // assertions below -- it's set for realism, not correctness.
            combat_participant_enemies: [9u64].into_iter().collect()};
        axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false)
    }
    #[test]
    fn maps_core_ei_fields() {
        let v = to_ei_json(&sample_report(), &[]);
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
        // statsTargets has one entry per real target (two enemies here), only
        // carrying the one per-target metric we actually compute (damage).
        assert_eq!(v["players"][0]["statsTargets"].as_array().unwrap().len(), 2);
        assert_eq!(v["players"][0]["statsTargets"][0][0]["totalDmg"], 500);
        assert_eq!(v["players"][1]["statsTargets"][0][0]["totalDmg"], 0);
        assert_eq!(v["players"][1]["statsTargets"][1][0]["totalDmg"], 0);
        assert_eq!(v["players"][0]["defenses"][0]["deadCount"], 0);
        assert_eq!(v["targets"][0]["id"], 9);
        // Verify enemyPlayer flag matches the actual is_player field.
        assert_eq!(v["targets"][0]["enemyPlayer"], true, "player enemy should have enemyPlayer: true");
        assert_eq!(v["targets"][1]["id"], 10);
        assert_eq!(v["targets"][1]["enemyPlayer"], false, "NPC enemy should have enemyPlayer: false");
        // M10 Task 3: EI-parity divergence check -- `report.enemies` (native/
        // HTML) is filtered down to the one combat participant (enemy 9;
        // enemy 10 never interacted per `sample_report`'s `Metrics::
        // combat_participant_enemies`), but `targets[]` above still lists
        // BOTH, because `to_ei_json` reads `report.all_enemies` (unfiltered)
        // -- real EI keeps every enumerated target regardless of
        // interaction, and this project's ei-json output stays faithful to
        // that even though the native output no longer does.
        let report = sample_report();
        assert_eq!(report.enemies.len(), 1, "native enemies[] is filtered to the one participant");
        assert_eq!(report.all_enemies.len(), 2, "all_enemies (EI adapter's source) keeps both");
        assert_eq!(v["targets"].as_array().unwrap().len(), 2, "ei-json targets[] stays unfiltered");
    }

    /// M3 Task 5: `buffMap`, `buffUptimes[]`, and the four new `support[0]`
    /// fields — shape plus a known numeric value for each, built from
    /// explicit `boon_uptime`/`boon_generation`/`support` values (not the
    /// golden fixture, which lives in `axilog-core`'s own calibration
    /// tests) so this crate's unit test stays self-contained.
    fn sample_report_with_boons() -> axilog_schema::Report {
        use axilog_core::model::{Encounter, Player};
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        use axilog_core::analysis::buffs::{self, BoonUptime, GenerationStats};
        use axilog_core::analysis::support::SupportMetrics;
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:None,
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"Nim Iss".into(),profession:"Thief".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]}],
            enemies:vec![],markers:vec![],tick_rate:None};
        let mut boon_uptime = std::collections::BTreeMap::new();
        // Might (intensity): avg_stacks=3.5, presence_pct=100.0.
        boon_uptime.insert((1u64, buffs::MIGHT), BoonUptime { presence_pct: 100.0, avg_stacks: 3.5 });
        // Quickness (duration): presence_pct=42.0 (avg_stacks meaningless/0).
        boon_uptime.insert((1u64, buffs::QUICKNESS), BoonUptime { presence_pct: 42.0, avg_stacks: 0.0 });
        let mut boon_generation = std::collections::BTreeMap::new();
        boon_generation.insert((1u64, buffs::MIGHT), GenerationStats { self_pct: 1.5, group_pct: 2.0, squad_pct: 3.0 });
        let m = Metrics{
            players: vec![PlayerMetrics{agent_addr:1,
                support: SupportMetrics { cleanses: 5, cleanses_self: 2, strips: 7, resurrects: 1 },
                ..Default::default()}],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime, boon_generation,
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
        };
        axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false)
    }

    #[test]
    fn buff_map_covers_the_12_tracked_boons_with_computed_fields_only() {
        let v = to_ei_json(&sample_report_with_boons(), &[]);
        let buff_map = v["buffMap"].as_object().expect("buffMap must be an object");
        assert_eq!(buff_map.len(), 12, "exactly the 12 tracked boons");
        // Known value: Might (740) is Intensity-type -> stacking: true.
        assert_eq!(v["buffMap"]["b740"]["name"], "Might");
        assert_eq!(v["buffMap"]["b740"]["stacking"], true);
        // Known value: Quickness (1187) is duration-type -> stacking: false.
        assert_eq!(v["buffMap"]["b1187"]["name"], "Quickness");
        assert_eq!(v["buffMap"]["b1187"]["stacking"], false);
    }

    #[test]
    fn buff_uptimes_map_intensity_and_duration_boons_to_ei_field_meanings() {
        let v = to_ei_json(&sample_report_with_boons(), &[]);
        let entries = v["players"][0]["buffUptimes"].as_array().expect("buffUptimes must be an array");
        assert_eq!(entries.len(), 12, "one entry per tracked boon");
        let might = entries.iter().find(|e| e["id"] == 740).expect("Might entry present");
        // Intensity boon: EI's `uptime` is our raw avg_stacks (no *100),
        // `presence` is our presence_pct.
        assert_eq!(might["buffData"][0]["uptime"], 3.5);
        assert_eq!(might["buffData"][0]["presence"], 100.0);
        // `generated` only carries the one character-attributable entry we
        // actually compute: the player's own self-generation.
        assert_eq!(might["buffData"][0]["generated"]["Nim Iss"], 1.5);
        let quickness = entries.iter().find(|e| e["id"] == 1187).expect("Quickness entry present");
        // Duration boon: EI's `uptime` is our presence_pct, `presence` is
        // always 0 (EI never populates it for this branch).
        assert_eq!(quickness["buffData"][0]["uptime"], 42.0);
        assert_eq!(quickness["buffData"][0]["presence"], 0.0);
    }

    #[test]
    fn support_block_carries_the_four_new_computed_fields() {
        let v = to_ei_json(&sample_report_with_boons(), &[]);
        let support = &v["players"][0]["support"][0];
        assert_eq!(support["condiCleanse"], 5);
        assert_eq!(support["condiCleanseSelf"], 2);
        assert_eq!(support["boonStrips"], 7);
        assert_eq!(support["resurrects"], 1);
        // stunBreak fields (already present since M1) are untouched.
        assert_eq!(support["stunBreak"], 0);
    }

    /// M10 Task 1: `extHealingStats`/`extBarrierStats` are present only for
    /// a player whose native `healing` block is `Some` (i.e. the log
    /// carries the healing extension), using EI's exact real field names
    /// for the subset this project actually computes, and absent entirely
    /// otherwise (native `healing: None` -- no extension data at all).
    #[test]
    fn heals_and_barrier_map_to_ei_field_names_only_when_present() {
        use axilog_schema::{
            CcOut, ContributionOut, DamageOut, EncounterOut, HealingOut, PlayerOut,
            SupportOut, TimelineOut, PerSecondOut, Report,
        };
        fn base_player(account: &str, healing: Option<HealingOut>) -> PlayerOut {
            PlayerOut {
                account: account.to_string(), character: account.to_string(),
                profession: "Guardian".into(), elite_spec: "".into(), team: "red".into(),
                subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
                damage: DamageOut { total: 0, dps: 0.0, per_enemy: vec![] },
                downs_dealt: 0, kills_dealt: 0, downs_taken: 0, deaths: 0,
                damage_taken: 0,
                cc: CcOut { applied_total: 0, applied_duration_ms: 0, stun_breaks: 0, removed_stun_duration_ms: 0 },
                downs_contribution: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
                downed_by: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
                boons: vec![],
                support: SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, resurrects: 0 },
                healing,
                skill_damage: None,
            }
        }
        let report = Report {
            schema_version: "0.2", axilog_version: "0.1.0".to_string(),
            encounter: EncounterOut { kind: "wvw".into(), map: "".into(), duration_ms: 10_000,
                build: "".into(), revision: 1, recorded_by: None, teams: vec![], markers: vec![], tick_rate: None },
            players: vec![
                base_player(":A.1", Some(HealingOut {
                    healing_out_total: 5000, healing_out_allies: 3000, healing_out_self: 2000,
                    barrier_out: 1000, downed_healing_out: 500,
                })),
                base_player(":B.1", None),
            ],
            enemies: vec![],
            all_enemies: vec![],
            timeline: TimelineOut { resolution_ms: 1000, per_second: PerSecondOut { squad_damage: vec![], cc_applied: vec![], downs: vec![] } },
            warnings: vec![],
            replay: None,
            missiles: None,
        };
        let v = to_ei_json(&report, &[]);
        let healing = &v["players"][0]["extHealingStats"]["outgoingHealing"][0];
        assert_eq!(healing["healing"], 5000);
        assert_eq!(healing["downedHealing"], 500);
        assert_eq!(healing["hps"], 500); // 5000 / 10s
        assert_eq!(healing["downedHps"], 50); // 500 / 10s
        let barrier = &v["players"][0]["extBarrierStats"]["outgoingBarrier"][0];
        assert_eq!(barrier["barrier"], 1000);
        assert_eq!(barrier["bps"], 100); // 1000 / 10s
        // No native-only self/allies split leaked into the EI-shaped output
        // under invented field names.
        assert!(healing.get("healingSelf").is_none());
        assert!(healing.get("healingAllies").is_none());

        assert!(
            v["players"][1].get("extHealingStats").is_none(),
            "extHealingStats must be absent for a player with no native healing block"
        );
        assert!(v["players"][1].get("extBarrierStats").is_none());
    }

    /// M11 Task 3: `isFake` -- every target gets `false` (this project
    /// never emits real EI's synthetic aggregate pseudo-targets); the
    /// golden-fixture calibration test (`crates/axilog-ei/tests/
    /// ei_golden.rs`) asserts the same against a real multi-target log.
    #[test]
    fn every_target_is_marked_not_fake() {
        let v = to_ei_json(&sample_report(), &[]);
        let targets = v["targets"].as_array().expect("targets must be an array");
        assert_eq!(targets.len(), 2, "sample_report has 2 enemies (see all_enemies above)");
        for t in targets {
            assert_eq!(t["isFake"], false, "every real (non-aggregate) target must be isFake: false");
        }
    }

    /// M11 Task 3: `activeTimes`/`combatReplayData` are ALWAYS present (not
    /// gated on `--replay`), with harmless zero/empty defaults when the
    /// caller passes no `activity` data at all (`&[]`).
    #[test]
    fn active_times_and_combat_replay_data_default_to_zero_when_no_activity_supplied() {
        let v = to_ei_json(&sample_report(), &[]);
        assert_eq!(v["players"][0]["activeTimes"], json!([0]));
        assert_eq!(v["players"][0]["combatReplayData"]["start"], 0);
        assert_eq!(v["players"][0]["combatReplayData"]["end"], 0);
        assert_eq!(v["players"][0]["combatReplayData"]["down"], json!([]));
        assert_eq!(v["players"][0]["combatReplayData"]["dead"], json!([]));
    }

    /// M11 Task 3: real (non-empty) `activity` data flows through to
    /// `activeTimes`/`combatReplayData`, positionally joined to
    /// `report.players` by index -- `sample_report()` has 2 players (agent
    /// addrs 1 and 2, in that order), matching `activity`'s own order here.
    #[test]
    fn active_times_and_combat_replay_data_map_real_activity_intervals() {
        use axilog_core::analysis::replay::Interval;
        let activity = vec![
            ActivityIntervals {
                agent_addr: 1,
                start_ms: 100,
                end_ms: 10_100,
                down_intervals: vec![Interval { start_ms: 2_000, end_ms: 3_000 }],
                dead_intervals: vec![Interval { start_ms: 5_000, end_ms: 5_500 }],
            },
            ActivityIntervals {
                agent_addr: 2, start_ms: 0, end_ms: 1_000,
                down_intervals: vec![], dead_intervals: vec![],
            },
        ];
        let v = to_ei_json(&sample_report(), &activity);
        assert_eq!(v["players"][0]["combatReplayData"]["start"], 100);
        assert_eq!(v["players"][0]["combatReplayData"]["end"], 10_100);
        assert_eq!(v["players"][0]["combatReplayData"]["down"], json!([[2_000, 3_000]]));
        assert_eq!(v["players"][0]["combatReplayData"]["dead"], json!([[5_000, 5_500]]));
        // active_ms = (10100-100) - 500 dead = 9500; down NOT subtracted.
        assert_eq!(v["players"][0]["activeTimes"], json!([9_500]));
        assert_eq!(v["players"][1]["combatReplayData"]["down"], json!([]));
        assert_eq!(v["players"][1]["activeTimes"], json!([1_000]));
    }
}
