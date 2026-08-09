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

/// One skill entry's EI-shaped JSON row (M12, Task 3) -- mirrors real EI's
/// `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` entry shape
/// (verified against axibridge's `test-fixtures/boon/20260117-181030.json`,
/// `players[0].totalDamageDist[0][0]`), emitting ONLY the fields this
/// project actually computes (`axilog_schema::SkillEntryOut`'s own fields):
/// `id`, `totalDamage`, `min`, `max`, `hits`, `crit`, `flank`. Real EI's
/// sibling fields on the same entry (`totalBreakbarDamage`, `connectedHits`,
/// `glance`, `againstMoving`, `missed`, `invulned`, `interrupted`, `evaded`,
/// `blocked`, `shieldDamage`, `critDamage`, `downContribution`,
/// `indirectDamage`) aren't computed anywhere in this project's damage
/// predicate (see `axilog_core::analysis::skill_damage`'s module doc: only
/// CONTRIBUTING, `dmg > 0` events are tracked at all, with no missed/
/// blocked/evaded/etc. outcome tracking anywhere else in this schema
/// either) -- omitted rather than faked, same "don't fake absent data"
/// convention `statsTargets`/`support`/`extHealingStats` above already
/// follow. `crit`/`flank` map directly to `SkillEntryOut::crit_hits`/
/// `flank_hits` (hit COUNTS, matching real EI's own `crit`/`flank`
/// semantics exactly -- both are cleanly available, unlike the omitted
/// fields above, so they're included here even though the Task 3 brief's
/// minimal-field list only named `id`/`totalDamage`/`min`/`max`/`hits`).
fn skill_entry_ei_json(e: &axilog_schema::SkillEntryOut) -> Value {
    json!({
        "id": e.skill_id,
        "totalDamage": e.total,
        "min": e.min,
        "max": e.max,
        "hits": e.hits,
        "crit": e.crit_hits,
        "flank": e.flank_hits,
    })
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
            "appliedCrowdControlDuration": p.cc.applied_duration_ms,
            // M13 Task 3: outgoing hit-quality fields, mapped from
            // `p.hit_stats` (`HitStatsOut`, mirrors `axilog_core::
            // analysis::hit_stats::HitStats` field-for-field -- see that
            // module's doc comment for the exact per-field EI derivation/
            // citation, including WHY `criticalRate`/`flankingRate`/
            // `glanceRate`/`againstMovingRate` are plain COUNTS despite the
            // "Rate" naming, verbatim from real EI). Field names verified
            // against a real dps.report export
            // (`fixtures/wvw-small.ei.json`'s `players[].hitStats` sidecar,
            // itself a verbatim subset of a real `statsAll[0]`). Always
            // present (not gated) -- `hit_stats` is unconditionally
            // computed by `analyze()`, same "always-on" convention as the
            // down-contribution/CC fields above.
            "criticalRate": p.hit_stats.crit_count,
            "criticalDmg": p.hit_stats.crit_damage,
            "flankingRate": p.hit_stats.flank_count,
            "glanceRate": p.hit_stats.glance_count,
            "againstMovingRate": p.hit_stats.moving_count,
            "connectedDamageCount": p.hit_stats.connected_count,
            "connectedDmg": p.hit_stats.connected_damage,
            "connectedDirectDamageCount": p.hit_stats.direct_count,
            "connectedDirectDmg": p.hit_stats.direct_damage,
            "connectedConditionCount": p.hit_stats.condition_count,
            "connectedConditionDamage": p.hit_stats.condition_damage,
            "critableDirectDamageCount": p.hit_stats.critable_direct_count,
            "againstDownedCount": p.hit_stats.against_downed_count,
            "againstDownedDamage": p.hit_stats.against_downed_damage,
            "connectedLifeLeechCount": p.hit_stats.life_leech_count,
            "connectedLifeLeechDamage": p.hit_stats.life_leech_damage,
            "connectedPowerAbove90HPCount": p.hit_stats.above90_power_count,
            "connectedPowerAbove90HPDamage": p.hit_stats.above90_power_damage,
            "connectedConditionAbove90HPCount": p.hit_stats.above90_condition_count,
            "connectedConditionAbove90HPDamage": p.hit_stats.above90_condition_damage
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
                "damageTaken": p.damage_taken,
                // M13 Task 3: hit-outcome + damage-taken breakdown fields,
                // mapped from `p.defenses` (`DefensesOut`, mirrors
                // `axilog_core::analysis::defenses::DefenseStats`
                // field-for-field -- see that module's doc comment for the
                // exact per-field EI derivation/citation). Field names
                // verified against a real dps.report export
                // (`fixtures/wvw-small.ei.json`'s `players[].defenses`
                // sidecar, itself a verbatim subset of a real
                // `defenses[0]`). Always present (not gated), same
                // always-on convention as `downCount`/`deadCount`/
                // `damageTaken` above.
                //
                // IMPORTANT -- `lifeLeechDamageTakenCount` intentionally
                // diverges from real EI: this emits OUR correct derived
                // value (`p.defenses.life_leech_count`), NOT a
                // reproduction of GW2EI's own real, verified counting bug
                // (`DefensePerTargetStatistics.cs`'s life-leech branch
                // increments the SUM field a second time instead of the
                // COUNT field, so real EI always reports 0 here even on a
                // fight with substantial nonzero `lifeLeechDamageTaken` --
                // see `axilog_core::analysis::defenses`'s module doc for
                // the full source-line citation). An exact-vs-real-EI diff
                // on this ONE field is therefore an intentional, documented
                // divergence -- axilog is more correct here, not less.
                // `crates/axilog-ei/tests/ei_golden.rs` calibrates every
                // other `defenses[0]` field exactly against the golden
                // fixture and asserts THIS one against the algebraically
                // derived TRUE reference instead of the fixture's raw
                // (buggy) value, mirroring `defenses_golden.rs`'s own
                // native-layer calibration.
                "blockedCount": p.defenses.blocked_count,
                "evadedCount": p.defenses.evaded_count,
                "dodgeCount": p.defenses.dodge_count,
                "missedCount": p.defenses.missed_count,
                "interruptedCount": p.defenses.interrupted_count,
                "invulnedCount": p.defenses.invulned_count,
                "strikeDamageTaken": p.defenses.strike_damage,
                "strikeDamageTakenCount": p.defenses.strike_count,
                "powerDamageTaken": p.defenses.power_damage,
                "powerDamageTakenCount": p.defenses.power_count,
                "conditionDamageTaken": p.defenses.condition_damage,
                "conditionDamageTakenCount": p.defenses.condition_count,
                "lifeLeechDamageTaken": p.defenses.life_leech_damage,
                "lifeLeechDamageTakenCount": p.defenses.life_leech_count,
                "damageBarrier": p.defenses.barrier_damage,
                "damageBarrierCount": p.defenses.barrier_count,
                "breakbarDamageTaken": p.defenses.breakbar_damage,
                "breakbarDamageTakenCount": p.defenses.breakbar_count
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
        // `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` (M12,
        // Task 3): only present when this player's native `skill_damage`
        // block is present (`--skill-damage`/SDK `skill_damage: true` was
        // requested when the `Report` was built --
        // `axilog_schema::PlayerOut::skill_damage`'s doc comment) -- keyed
        // off THAT presence, not a separate flag threaded through
        // `to_ei_json` itself (the M12 Task 3 brief: "key off presence, not
        // a flag"), so a `Report` built without `--skill-damage` gets these
        // three arrays OMITTED entirely, not emitted empty. Real EI shape
        // verified against axibridge's `test-fixtures/boon/
        // 20260117-181030.json`: `totalDamageDist`/`totalDamageTaken` are
        // `[phase][skillEntry]` (a single-element phase array wrapping the
        // skill list -- this project's one-phase convention, matching
        // `statsAll`/`extHealingStats` elsewhere in this fn);
        // `targetDamageDist` is `[targetIndex][phase][skillEntry]`,
        // positionally keyed to `targets[]` (built from
        // `report.all_enemies`, the same unfiltered roster/positional
        // convention `statsTargets` above already uses) -- a target this
        // player never damaged gets an empty skill list (`[[]]`), not an
        // absent entry, matching real EI's own always-present-per-target
        // shape.
        if let Some(sd) = &p.skill_damage {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert(
                "totalDamageDist".to_string(),
                json!([ sd.outgoing.iter().map(skill_entry_ei_json).collect::<Vec<_>>() ]),
            );
            obj.insert(
                "totalDamageTaken".to_string(),
                json!([ sd.taken.iter().map(skill_entry_ei_json).collect::<Vec<_>>() ]),
            );
            let target_dist: Vec<Value> = report
                .all_enemies
                .iter()
                .map(|e| {
                    let skills = sd
                        .per_target
                        .iter()
                        .find(|t| t.enemy_id == e.id)
                        .map(|t| t.skills.iter().map(skill_entry_ei_json).collect::<Vec<_>>())
                        .unwrap_or_default();
                    json!([skills])
                })
                .collect();
            obj.insert("targetDamageDist".to_string(), json!(target_dist));
        }
        // `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets` (M12,
        // Task 3): only present when this player's native `per_second`
        // block is present (`--timeseries`/SDK `timeseries: true` --
        // `axilog_schema::PlayerOut::per_second`'s doc comment); `dpsTargets`
        // is gated by that SAME `p.per_second.is_some()` check rather than
        // its own `p.dps_targets.is_empty()` check -- an empty `dps_targets`
        // Vec can't distinguish "not requested" from "requested, but this
        // player never damaged any enemy", while `axilog_schema::
        // build_report` populates BOTH `per_second` and `dps_targets` off
        // the identical `include_timeseries` bool (see that fn's doc
        // comment), so keying off `per_second`'s presence is the correct
        // "presence, not a flag" signal for both. Real EI shape (same
        // fixture): `damage1S`/`damageTaken1S` are `[phase][second]`
        // (single-element phase array wrapping the per-second numbers --
        // this project's own `per_second.damage`/`damage_taken` are ALREADY
        // cumulative running totals by construction, see
        // `axilog_core::analysis::timeseries`'s module doc, so no extra
        // transform is needed here, just the EI phase-array wrapper);
        // `targetDamage1S` is `[targetIndex][phase][second]`, `dpsTargets`
        // is `[targetIndex][phase]{dps, damage}` -- both positionally keyed
        // to `targets[]`/`report.all_enemies`, same convention
        // `targetDamageDist` above uses. A target this player never damaged
        // gets an all-zero series (length matching this player's own
        // `per_second.damage`) / a `{dps: 0, damage: 0}` entry, not an
        // absent one. `dps` is rounded to the nearest integer, matching
        // `dpsAll[0].dps`'s own convention above (real EI's own
        // `dpsTargets[][].dps` is likewise an integer on the source
        // fixture) -- only `dps`/`damage` are emitted, real EI's many other
        // `dpsTargets[][]` fields (`condiDps`, `powerDps`, `breakbarDamage`,
        // the `actor*` duplicates, ...) aren't computed here, omitted
        // rather than faked.
        if let Some(ps) = &p.per_second {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert("damage1S".to_string(), json!([ps.damage]));
            obj.insert("damageTaken1S".to_string(), json!([ps.damage_taken]));
            let buckets = ps.damage.len();
            let target_damage_1s: Vec<Value> = report
                .all_enemies
                .iter()
                .map(|e| {
                    let series = ps
                        .per_target
                        .iter()
                        .find(|t| t.enemy_id == e.id)
                        .map(|t| t.damage.clone())
                        .unwrap_or_else(|| vec![0u64; buckets]);
                    json!([series])
                })
                .collect();
            obj.insert("targetDamage1S".to_string(), json!(target_damage_1s));

            let dps_targets: Vec<Value> = report
                .all_enemies
                .iter()
                .map(|e| match p.dps_targets.iter().find(|d| d.enemy_id == e.id) {
                    Some(d) => json!([ { "dps": d.dps.round() as i64, "damage": d.damage } ]),
                    None => json!([ { "dps": 0, "damage": 0 } ]),
                })
                .collect();
            obj.insert("dpsTargets".to_string(), json!(dps_targets));
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
        // `rotation[]` (M14, Task 3): only present when this player's
        // native `rotation` block is present (`--rotation`/SDK
        // `rotation: true` was requested when the `Report` was built --
        // `axilog_schema::PlayerOut::rotation`'s doc comment) -- keyed off
        // THAT presence, not a separate flag threaded through `to_ei_json`
        // itself, same "presence, not a flag" convention `skill_damage`'s
        // `totalDamageDist`/`per_second`'s `damage1S` mappings above already
        // establish. Real EI shape (verified against a real dps.report
        // export, axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].rotation[0]`): a flat array of `{ id, skills: [ {
        // castTime, duration, timeGained, quickness } ] }`, one entry per
        // skill id this player cast -- NOT wrapped in a phase array (unlike
        // `statsAll`/`totalDamageDist`/etc above, real EI's own
        // `rotation[]` has no phase dimension at all). Field-for-field
        // straight copy of `axilog_schema::CastOut`/`SkillRotationOut`
        // (themselves a mirror of `axilog_core::analysis::rotation::Cast`/
        // `SkillRotation` -- see that module's doc comment for the full
        // GW2EI `AnimatedCastEvent` derivation this reproduces, and its
        // documented `InstantCastEvent`-pipeline scope gap, which applies
        // here identically since this is a direct copy of the same
        // already-computed data).
        if let Some(rotation) = &p.rotation {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            let rotation_json: Vec<Value> = rotation
                .iter()
                .map(|sr| {
                    json!({
                        "id": sr.skill_id,
                        "skills": sr.casts.iter().map(|c| json!({
                            "castTime": c.cast_time_ms,
                            "duration": c.duration_ms,
                            "timeGained": c.time_gained_ms,
                            "quickness": c.quickness,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            obj.insert("rotation".to_string(), json!(rotation_json));
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
    // `skillMap` (M14, Task 3): keyed `"s<id>"` per real EI's convention
    // (verified against axibridge's `test-fixtures/boon/
    // 20260117-181030.json`, `skillMap.s45534` etc -- same `"s"`-prefix
    // pattern `buffMap`'s `"b"` prefix above already mirrors for boons).
    // ALWAYS present (not gated) -- `Report::skill_map` itself is always
    // populated (`axilog_core::analysis::Metrics::skill_map`'s doc comment:
    // "Always computed (not opt-in)"), same always-on convention as
    // `buffMap`/`targets` above. Only the fields this project actually
    // computes are emitted (`axilog_core::analysis::skill_map::
    // SkillMapEntry`'s own fields, via `axilog_schema::SkillMapEntryOut`):
    // `name` (log-table best-effort, NOT EI's richer API-backed name --
    // see that module's doc comment's honesty writeup), `isSwap`, `canCrit`.
    // Real EI's sibling fields on the same entry -- `icon` (a
    // render.guildwars2.com URL) and `autoAttack` (needs the external GW2
    // API, this project's `auto_attack` is always `None` -- see that
    // module's doc comment's "`auto_attack`: OMITTED, not guessed"
    // section) -- as well as `isInstantCast`/`isTraitProc`/
    // `isUnconditionalProc`/`isGearProc`/`isNotAccurate`/
    // `conversionBasedHealing`/`hybridHealing` (all needing the same
    // external DB) are NOT computed anywhere in this project, so they're
    // omitted rather than faked, same "don't fake absent data" convention
    // `statsTargets`/`support`/`extHealingStats` above already follow.
    let skill_map: BTreeMap<String, Value> = report.skill_map.iter().map(|(&id, e)| {
        (format!("s{id}"), json!({
            "name": e.name,
            "isSwap": e.is_swap,
            "canCrit": e.can_crit,
        }))
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
        "skillMap": skill_map,
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
            combat_participant_enemies: [9u64].into_iter().collect(), skill_map: Default::default()};
        axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false, false, false)
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
            skill_map: Default::default(),
        };
        axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false, false, false)
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
            CcOut, ContributionOut, DamageOut, DefensesOut, EncounterOut, HealingOut, HitStatsOut,
            PlayerOut, SupportOut, TimelineOut, PerSecondOut, Report,
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
                per_second: None,
                dps_targets: vec![],
                hit_stats: HitStatsOut::default(),
                defenses: DefensesOut::default(),
                rotation: None,
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
            skill_map: Default::default(),
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

    /// Shared player-row builder for the M12 Task 3 tests below -- same
    /// "hand-build a `PlayerOut`" pattern `heals_and_barrier_map_to_ei_
    /// field_names_only_when_present`'s `base_player` already uses, extended
    /// with `skill_damage`/`per_second`/`dps_targets` parameters so each
    /// test only has to specify what it cares about.
    fn skill_and_timeseries_player(
        skill_damage: Option<axilog_schema::SkillDamageOut>,
        per_second: Option<axilog_schema::PlayerPerSecondOut>,
        dps_targets: Vec<axilog_schema::DpsTargetOut>,
    ) -> axilog_schema::PlayerOut {
        use axilog_schema::{CcOut, ContributionOut, DamageOut, DefensesOut, HitStatsOut, PlayerOut, SupportOut};
        PlayerOut {
            account: ":A.1".into(), character: "A".into(), profession: "Guardian".into(),
            elite_spec: "".into(), team: "red".into(), subgroup: 1, in_squad: true,
            commander: false, marker: None, commander_tag: None,
            damage: DamageOut { total: 0, dps: 0.0, per_enemy: vec![] },
            downs_dealt: 0, kills_dealt: 0, downs_taken: 0, deaths: 0, damage_taken: 0,
            cc: CcOut { applied_total: 0, applied_duration_ms: 0, stun_breaks: 0, removed_stun_duration_ms: 0 },
            downs_contribution: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
            downed_by: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
            boons: vec![],
            support: SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, resurrects: 0 },
            healing: None,
            skill_damage, per_second, dps_targets,
            hit_stats: HitStatsOut::default(),
            defenses: DefensesOut::default(),
            rotation: None,
        }
    }

    fn report_with_players(
        all_enemies: Vec<axilog_schema::EnemyOut>,
        players: Vec<axilog_schema::PlayerOut>,
    ) -> axilog_schema::Report {
        use axilog_schema::{EncounterOut, PerSecondOut, Report, TimelineOut};
        Report {
            schema_version: "0.2", axilog_version: "0.1.0".to_string(),
            encounter: EncounterOut { kind: "wvw".into(), map: "".into(), duration_ms: 2_000,
                build: "".into(), revision: 1, recorded_by: None, teams: vec![], markers: vec![], tick_rate: None },
            players,
            enemies: vec![],
            all_enemies,
            timeline: TimelineOut { resolution_ms: 1000, per_second: PerSecondOut { squad_damage: vec![], cc_applied: vec![], downs: vec![] } },
            warnings: vec![],
            replay: None,
            missiles: None,
            skill_map: Default::default(),
        }
    }

    /// M12 Task 3: `totalDamageDist`/`totalDamageTaken`/`targetDamageDist`
    /// are present, correctly shaped (`[phase][skillEntry]` /
    /// `[targetIndex][phase][skillEntry]`), and carry the exact computed
    /// values only when `skill_damage` was requested (`PlayerOut::
    /// skill_damage: Some(..)`).
    #[test]
    fn total_damage_dist_shape_and_known_value_when_skill_damage_present() {
        use axilog_schema::{EnemyOut, PerTargetSkillsOut, SkillDamageOut, SkillEntryOut};
        fn entry() -> SkillEntryOut {
            SkillEntryOut { skill_id: 42009, total: 32503, hits: 5, min: 100, max: 20000, crit_hits: 2, flank_hits: 1 }
        }
        let taken_entry = SkillEntryOut { skill_id: 700, total: 275, hits: 2, min: 75, max: 200, crit_hits: 0, flank_hits: 0 };
        let sd = SkillDamageOut {
            outgoing: vec![entry()],
            taken: vec![taken_entry],
            per_target: vec![PerTargetSkillsOut { enemy_id: 9, skills: vec![entry()] }],
        };
        let enemies = vec![
            EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None },
            EnemyOut { id: 10, name: "Untouched".into(), team: "blue".into(), is_player: false, marker: None },
        ];
        let player = skill_and_timeseries_player(Some(sd), None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&report, &[]);
        let p = &v["players"][0];

        // totalDamageDist: [phase][skillEntry], only the computed fields.
        assert_eq!(p["totalDamageDist"][0][0]["id"], 42009);
        assert_eq!(p["totalDamageDist"][0][0]["totalDamage"], 32503);
        assert_eq!(p["totalDamageDist"][0][0]["min"], 100);
        assert_eq!(p["totalDamageDist"][0][0]["max"], 20000);
        assert_eq!(p["totalDamageDist"][0][0]["hits"], 5);
        assert_eq!(p["totalDamageDist"][0][0]["crit"], 2);
        assert_eq!(p["totalDamageDist"][0][0]["flank"], 1);
        assert!(p["totalDamageDist"][0][0].get("connectedDamage").is_none(), "uncomputed fields must not be faked");
        assert!(p["totalDamageDist"][0][0].get("indirectDamage").is_none());

        // totalDamageTaken: same [phase][skillEntry] shape.
        assert_eq!(p["totalDamageTaken"][0][0]["id"], 700);
        assert_eq!(p["totalDamageTaken"][0][0]["totalDamage"], 275);

        // targetDamageDist: [targetIndex][phase][skillEntry], positionally
        // keyed to `all_enemies` (enemy 9 first, enemy 10 second) -- enemy
        // 10 was never damaged, so its skill list is empty, not absent.
        assert_eq!(p["targetDamageDist"].as_array().unwrap().len(), 2, "one entry per real target");
        assert_eq!(p["targetDamageDist"][0][0][0]["id"], 42009);
        assert_eq!(p["targetDamageDist"][0][0][0]["totalDamage"], 32503);
        assert_eq!(p["targetDamageDist"][1][0], json!([]), "untouched target gets an empty skill list");
    }

    /// M12 Task 3: `totalDamageDist`/`targetDamageDist`/`totalDamageTaken`
    /// must be OMITTED entirely (not emitted empty) when the `Report` was
    /// built without `--skill-damage` (`PlayerOut::skill_damage: None`) --
    /// the gate-respecting requirement keyed off presence, not a flag.
    #[test]
    fn total_damage_dist_omitted_when_skill_damage_absent() {
        use axilog_schema::EnemyOut;
        let enemies = vec![EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None }];
        let player = skill_and_timeseries_player(None, None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&report, &[]);
        let p = &v["players"][0];
        assert!(p.get("totalDamageDist").is_none(), "totalDamageDist must be omitted, not emitted empty");
        assert!(p.get("totalDamageTaken").is_none());
        assert!(p.get("targetDamageDist").is_none());
    }

    /// M12 Task 3: `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets`
    /// are present, correctly shaped, and the final cumulative element of
    /// `damage1S`/`damageTaken1S` matches the whole-fight scalar -- only
    /// when `per_second` was requested (`PlayerOut::per_second: Some(..)`).
    #[test]
    fn per_second_ei_fields_shape_and_known_value_when_present() {
        use axilog_schema::{DpsTargetOut, EnemyOut, PlayerPerSecondOut, PlayerTargetSeriesOut};
        let ps = PlayerPerSecondOut {
            damage: vec![50, 80, 80],
            damage_taken: vec![10, 10, 10],
            per_target: vec![PlayerTargetSeriesOut { enemy_id: 9, damage: vec![50, 80, 80] }],
        };
        let dps_targets = vec![DpsTargetOut { enemy_id: 9, damage: 80, dps: 40.0 }];
        let enemies = vec![
            EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None },
            EnemyOut { id: 10, name: "Untouched".into(), team: "blue".into(), is_player: false, marker: None },
        ];
        let player = skill_and_timeseries_player(None, Some(ps), dps_targets);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&report, &[]);
        let p = &v["players"][0];

        // damage1S/damageTaken1S: [phase][second], cumulative, final ==
        // the whole-fight total (already cumulative by construction).
        assert_eq!(p["damage1S"], json!([[50, 80, 80]]));
        assert_eq!(p["damage1S"][0].as_array().unwrap().last().unwrap(), &json!(80));
        assert_eq!(p["damageTaken1S"], json!([[10, 10, 10]]));

        // targetDamage1S: [targetIndex][phase][second] -- enemy 9 gets the
        // real series, enemy 10 (untouched) gets an all-zero series of the
        // SAME length.
        assert_eq!(p["targetDamage1S"].as_array().unwrap().len(), 2);
        assert_eq!(p["targetDamage1S"][0], json!([[50, 80, 80]]));
        assert_eq!(p["targetDamage1S"][1], json!([[0, 0, 0]]), "untouched target gets an all-zero series, not absent");

        // dpsTargets: [targetIndex][phase]{dps, damage} -- enemy 9 carries
        // the real dps/damage, enemy 10 defaults to zero.
        assert_eq!(p["dpsTargets"][0][0]["dps"], 40);
        assert_eq!(p["dpsTargets"][0][0]["damage"], 80);
        assert_eq!(p["dpsTargets"][1][0]["dps"], 0);
        assert_eq!(p["dpsTargets"][1][0]["damage"], 0);
    }

    /// M12 Task 3: `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets`
    /// must ALL be OMITTED (not emitted empty) when the `Report` was built
    /// without `--timeseries` (`PlayerOut::per_second: None`) -- including
    /// `dpsTargets`, which is gated by `per_second`'s presence rather than
    /// its own (possibly legitimately empty) `p.dps_targets` vec.
    #[test]
    fn per_second_ei_fields_omitted_when_absent() {
        use axilog_schema::EnemyOut;
        let enemies = vec![EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None }];
        let player = skill_and_timeseries_player(None, None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&report, &[]);
        let p = &v["players"][0];
        assert!(p.get("damage1S").is_none(), "damage1S must be omitted, not emitted empty");
        assert!(p.get("damageTaken1S").is_none());
        assert!(p.get("targetDamage1S").is_none());
        assert!(p.get("dpsTargets").is_none(), "dpsTargets omitted even though the Vec field itself is always present/empty on PlayerOut");
    }

    /// M14 Task 3: `rotation[]` is present, correctly shaped (a flat array
    /// of `{ id, skills: [ { castTime, duration, timeGained, quickness } ]
    /// }`, NOT phase-wrapped -- see `to_ei_json`'s own `rotation` mapping
    /// comment for the citation against a real dps.report export), and
    /// carries the exact computed values only when `rotation` was requested
    /// (`PlayerOut::rotation: Some(..)`).
    #[test]
    fn rotation_ei_json_shape_and_known_value_when_present() {
        use axilog_schema::{CastOut, SkillRotationOut};
        let mut player = skill_and_timeseries_player(None, None, vec![]);
        player.rotation = Some(vec![SkillRotationOut {
            skill_id: 5008,
            casts: vec![
                CastOut { cast_time_ms: -100, duration_ms: 500, time_gained_ms: 20, quickness: 0.15 },
                CastOut { cast_time_ms: 600, duration_ms: 250, time_gained_ms: -50, quickness: -0.3 },
            ],
        }]);
        let report = report_with_players(vec![], vec![player]);

        let v = to_ei_json(&report, &[]);
        let rotation = &v["players"][0]["rotation"];
        assert_eq!(rotation.as_array().unwrap().len(), 1, "one entry per cast skill id, no phase wrapper");
        assert_eq!(rotation[0]["id"], 5008);
        let skills = rotation[0]["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0]["castTime"], -100);
        assert_eq!(skills[0]["duration"], 500);
        assert_eq!(skills[0]["timeGained"], 20);
        assert_eq!(skills[0]["quickness"], 0.15);
        assert_eq!(skills[1]["castTime"], 600);
        assert_eq!(skills[1]["quickness"], -0.3);
    }

    /// M14 Task 3: `rotation` must be OMITTED entirely (not emitted empty)
    /// when the `Report` was built without `--rotation` (`PlayerOut::
    /// rotation: None`) -- the gate-respecting requirement keyed off
    /// presence, not a flag, same convention `skill_damage`/`per_second`
    /// above already establish.
    #[test]
    fn rotation_ei_json_omitted_when_absent() {
        let player = skill_and_timeseries_player(None, None, vec![]); // rotation: None inside the builder
        let report = report_with_players(vec![], vec![player]);

        let v = to_ei_json(&report, &[]);
        assert!(v["players"][0].get("rotation").is_none(), "rotation must be omitted, not emitted empty");
    }

    /// M14 Task 3: top-level `skillMap` is ALWAYS present (unlike
    /// `rotation`/`skill_damage`/`per_second` above), keyed `"s<id>"`, and
    /// carries only the fields this project actually computes (`name`/
    /// `isSwap`/`canCrit`) -- real EI's sibling `icon`/`autoAttack` fields
    /// (and every proc/instant/accuracy classifier) must NOT be faked in.
    #[test]
    fn skill_map_ei_json_keyed_and_carries_only_computed_fields() {
        use axilog_schema::SkillMapEntryOut;
        let player = skill_and_timeseries_player(None, None, vec![]);
        let mut report = report_with_players(vec![], vec![player]);
        report.skill_map.insert(5492, SkillMapEntryOut {
            name: "Fire Attunement".into(),
            auto_attack: None,
            is_swap: true,
            can_crit: false,
        });
        report.skill_map.insert(5008, SkillMapEntryOut {
            name: "Skill 5008".into(),
            auto_attack: None,
            is_swap: false,
            can_crit: true,
        });

        let v = to_ei_json(&report, &[]);
        let skill_map = v["skillMap"].as_object().expect("skillMap must be an object");
        assert_eq!(skill_map.len(), 2);
        assert_eq!(v["skillMap"]["s5492"]["name"], "Fire Attunement");
        assert_eq!(v["skillMap"]["s5492"]["isSwap"], true);
        assert_eq!(v["skillMap"]["s5492"]["canCrit"], false);
        assert_eq!(v["skillMap"]["s5008"]["name"], "Skill 5008");
        assert_eq!(v["skillMap"]["s5008"]["isSwap"], false);
        assert_eq!(v["skillMap"]["s5008"]["canCrit"], true);
        // Real EI's `icon`/`autoAttack` (and every proc/instant/accuracy
        // classifier) are never computed by this project -- must be
        // omitted, not faked as `null`/`false`.
        assert!(v["skillMap"]["s5492"].get("icon").is_none());
        assert!(v["skillMap"]["s5492"].get("autoAttack").is_none());
    }

    /// M14 Task 3: `skillMap` is present (as an empty object, not omitted)
    /// even when `Report::skill_map` is empty -- always-on convention,
    /// matching `buffMap`'s own unconditional presence above.
    #[test]
    fn skill_map_ei_json_present_and_empty_when_report_skill_map_empty() {
        let v = to_ei_json(&sample_report(), &[]);
        assert!(v.get("skillMap").is_some(), "skillMap key must always be present");
        assert_eq!(v["skillMap"].as_object().unwrap().len(), 0, "sample_report's Metrics::skill_map is Default::default() (empty)");
    }
}
