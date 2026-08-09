use serde::Serialize;
use axilog_core::model::Encounter;
use axilog_core::analysis::{buffs, Metrics};
use axilog_core::analysis::replay::Replay;
use axilog_core::analysis::missiles::Missiles;

#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub axilog_version: String,
    pub encounter: EncounterOut,
    pub players: Vec<PlayerOut>,
    /// Combat-participant enemies only (M10, Task 3) -- the enemy stayed if
    /// it dealt damage to the squad, received damage from the squad, or
    /// received CC from the squad (any nonzero interaction), or is an enemy
    /// player (always kept). A real WvW log enumerates every nearby
    /// lootable/tactivator/chest as an "enemy" NPC even though nothing in
    /// the fight ever targets them -- this is what the native output and
    /// HTML team chips show. See `axilog_core::analysis::Metrics::
    /// combat_participant_enemies`'s doc comment for the exact criteria.
    ///
    /// **Deliberately narrower than `all_enemies` below** -- see that
    /// field's doc comment for why the EI adapter needs the full,
    /// unfiltered list instead of this one.
    pub enemies: Vec<EnemyOut>,
    /// Every enemy `wvw::apply` resolved, unfiltered by combat
    /// participation (M10, Task 3). `#[serde(skip)]`: never part of the
    /// native JSON output -- this exists purely so `axilog_ei::to_ei_json`
    /// can build its `targets[]`/`statsTargets[]` from the SAME full roster
    /// real EI's own output does (EI keeps every enumerated target
    /// regardless of interaction; filtering `targets[]` down to combat
    /// participants would be a real divergence from EI's own shape, not
    /// just a cosmetic one, since `statsTargets[playerIndex][targetIndex]`
    /// is positionally keyed to `targets[]`). `enemies` above is the
    /// filtered list every other consumer (native JSON, HTML team chips)
    /// should use.
    #[serde(skip)]
    pub all_enemies: Vec<EnemyOut>,
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
    /// Opt-in missile (projectile) analytics (M10, Task 2) -- only present
    /// when the caller (CLI `--missiles` / SDK `missiles: true`) requested
    /// it by passing `Some(&Missiles)` to [`build_report`]. Omitted
    /// entirely from the JSON (not serialized as `null`), matching
    /// `replay`'s same opt-in/omit-when-absent convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missiles: Option<MissilesOut>,
}
/// Native-only opt-in missile analytics (M10, Task 2) -- see
/// `axilog_core::analysis::missiles`'s module doc for exactly what each
/// field is (and is NOT) attributable to: the arcdps wire format has no
/// blocked/reflected/destroyed reason code, so `denied` is deliberately
/// undifferentiated, and there is no per-player "denier" credit anywhere
/// (only an aggregate `squad.incoming_denied`).
#[derive(Serialize)]
pub struct MissilesOut {
    pub players: Vec<PlayerMissilesOut>,
    pub squad: SquadMissilesOut,
}
/// One squad player's missile totals -- mirrors
/// `axilog_core::analysis::missiles::PlayerMissiles` field-for-field, plus
/// `account` (final-review fix wave): `PlayerMissiles` itself has no display
/// fields (it's keyed purely by `agent_addr`, an internal join key not
/// exposed anywhere else in the native JSON), which left `missiles.players[]`
/// entries with no way for a consumer to join them back to `players[]`
/// without also having requested `--replay` (whose tracks DO carry a display
/// name) or reverse-engineering `agent_addr`. `account` is populated the same
/// way `PlayerOut.account` is -- straight from `axilog_core::model::Player`
/// -- by [`build_missiles_out`], which (as of this fix) takes the
/// `Encounter` roster alongside `Missiles` for exactly this lookup.
#[derive(Serialize)]
pub struct PlayerMissilesOut {
    pub agent_addr: u64,
    pub account: String,
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
    pub reflected_at_self: u32,
}
/// Squad-wide missile totals -- mirrors
/// `axilog_core::analysis::missiles::SquadMissiles` field-for-field.
#[derive(Serialize)]
pub struct SquadMissilesOut {
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
    pub incoming_fired: u32,
    pub incoming_denied: u32,
}

/// Build the native [`MissilesOut`] schema block from a computed
/// [`Missiles`] (`axilog_core::analysis::missiles::build_missiles`) plus the
/// same `Encounter` roster it was built from. Standalone from
/// [`build_report`], mirroring `build_replay_out`. `enc` is only used to
/// look up each `PlayerMissiles::agent_addr`'s `account` (final-review fix
/// wave) -- see [`PlayerMissilesOut`]'s doc comment for why that join key
/// was missing.
pub fn build_missiles_out(missiles: &Missiles, enc: &Encounter) -> MissilesOut {
    let accounts: std::collections::BTreeMap<u64, &str> =
        enc.players.iter().map(|p| (p.agent_addr, p.account.as_str())).collect();
    MissilesOut {
        players: missiles
            .players
            .iter()
            .map(|p| PlayerMissilesOut {
                agent_addr: p.agent_addr,
                account: accounts.get(&p.agent_addr).copied().unwrap_or("").to_string(),
                fired: p.fired,
                hit: p.hit,
                denied: p.denied,
                reflected_at_self: p.reflected_at_self,
            })
            .collect(),
        squad: SquadMissilesOut {
            fired: missiles.squad.fired,
            hit: missiles.squad.hit,
            denied: missiles.squad.denied,
            incoming_fired: missiles.squad.incoming_fired,
            incoming_denied: missiles.squad.incoming_denied,
        },
    }
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
    // or, since M10 Task 3, ANY single one of the four bounds ending up
    // non-finite -- falls back to a degenerate zero-sized bounds rather
    // than emit an infinity/NaN into the JSON. Checking `min_x` alone (the
    // pre-Task-3 guard) misses a real gap: `f64::min`/`f64::max` treat NaN
    // as "not comparable" and just keep the other operand, so a track whose
    // x samples are all NaN (but y samples are ordinary finite floats)
    // leaves `min_x`/`max_x` stuck at their `INFINITY`/`NEG_INFINITY`
    // sentinels while `min_y`/`max_y` go finite from the real y data --
    // `min_x` alone still catches that particular case. But a single
    // literal-`Infinity` sample mixed in among otherwise-finite samples on
    // the SAME axis does not: `max_x.max(f64::INFINITY)` locks `max_x` at
    // `Infinity` forever after that one bad sample, while `min_x` (fed by
    // the surrounding finite samples) stays perfectly finite -- so the old
    // `min_x`-only check would have let `max_x: inf` reach the embedded
    // JSON silently. Checking all four closes that gap.
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
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
    pub team_id: u32,
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
/// attribute. `missiles` (M10, Task 2) works the same way: pass `Some(&
/// Missiles)` (from `axilog_core::analysis::missiles::build_missiles`, when
/// `--missiles`/SDK `missiles: true` was requested) to embed the native
/// missiles block; `None` omits it entirely.
pub fn build_report(
    enc: &Encounter,
    metrics: &Metrics,
    axilog_version: &str,
    replay: Option<&Replay>,
    missiles: Option<&Missiles>,
) -> Report {
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
        // M10 Task 3: `enemies` (native/HTML) is filtered to combat
        // participants; `all_enemies` (EI-adapter-only, `#[serde(skip)]`)
        // stays the full roster -- see both fields' doc comments above.
        enemies: enc.enemies.iter()
            .filter(|e| metrics.combat_participant_enemies.contains(&e.id))
            .map(|e| EnemyOut{id:e.id,name:e.name.clone(),
                team:e.team.clone(),is_player:e.is_player,marker:e.marker.clone()})
            .collect(),
        all_enemies: enc.enemies.iter().map(|e| EnemyOut{id:e.id,name:e.name.clone(),
            team:e.team.clone(),is_player:e.is_player,marker:e.marker.clone()}).collect(),
        timeline: TimelineOut { resolution_ms: metrics.timeline.resolution_ms,
            per_second: PerSecondOut { squad_damage: metrics.timeline.squad_damage.clone(),
                cc_applied: metrics.timeline.cc_applied.clone(),
                downs: metrics.timeline.downs.clone() } },
        warnings: metrics.warnings.clone(),
        replay: replay.map(build_replay_out),
        missiles: missiles.map(|m| build_missiles_out(m, enc)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{Encounter, Player};
    use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};

    /// M10 Task 3: `enemies` is filtered to `Metrics::
    /// combat_participant_enemies`, while `all_enemies` (the `#[serde(skip)]`
    /// field the EI adapter reads) always carries the full roster --
    /// verifies both halves of the design choice documented on `Report::
    /// enemies`/`Report::all_enemies`.
    #[test]
    fn enemies_filtered_to_combat_participants_but_all_enemies_stays_full() {
        use axilog_core::model::Enemy;
        let enc = Encounter { kind:"wvw".into(), map:"".into(), duration_ms:1000,
            build:"".into(), revision:1, recorded_by:None, teams:vec![], players:vec![],
            enemies: vec![
                Enemy { id: 9, instid: 0, name: "Participant".into(), team: "blue".into(),
                    is_player: false, marker: None, agent_addrs: vec![9] },
                Enemy { id: 10, instid: 0, name: "LootBag".into(), team: "blue".into(),
                    is_player: false, marker: None, agent_addrs: vec![10] },
            ],
            markers:vec![], tick_rate:None };
        let m = Metrics { players: vec![],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![],cc_applied:vec![],downs:vec![]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: [9u64].into_iter().collect() };
        let report = build_report(&enc, &m, "0.1.0", None, None);
        assert_eq!(report.enemies.len(), 1, "only the participant enemy stays in the filtered list");
        assert_eq!(report.enemies[0].id, 9);
        assert_eq!(report.all_enemies.len(), 2, "all_enemies keeps the full roster, including the loot bag");

        // `all_enemies` must not leak into the native JSON (`#[serde(skip)]`).
        let v = serde_json::to_value(&report).unwrap();
        assert!(v.get("all_enemies").is_none(), "all_enemies must not appear in the serialized JSON");
        assert_eq!(v["enemies"].as_array().unwrap().len(), 1);
    }

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
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None, None);
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.1");
        assert_eq!(v["axilog_version"], "0.1.0");
        assert_eq!(v["players"][0]["damage"]["total"], 500);
        assert_eq!(v["encounter"]["map"], "Eternal Battlegrounds");
        assert!(v.get("replay").is_none(), "replay must be omitted when not requested");
        assert!(v.get("missiles").is_none(), "missiles must be omitted when not requested");
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
            has_healing_extension: true, combat_participant_enemies: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None, None);
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

    /// M10 Task 3: a single non-finite (`Infinity`) sample coordinate mixed
    /// in among otherwise-ordinary finite samples must not leak an
    /// `Infinity`/`NaN` bound into `ReplayBoundsOut` -- see
    /// `build_replay_out`'s doc comment above the finiteness guard for why
    /// checking `min_x` alone (the pre-Task-3 guard) misses this: a single
    /// `Infinity` sample locks `max_x` at `Infinity` via `f64::max` while
    /// `min_x`/`min_y`/`max_y` all stay perfectly finite off the surrounding
    /// real samples.
    #[test]
    fn replay_bounds_fall_back_to_zero_when_any_side_is_non_finite() {
        use axilog_core::analysis::replay::{Replay, Sample, Track};
        let replay = Replay {
            poll_ms: 300,
            tracks: vec![Track {
                agent_addr: 1,
                name: "Alice".into(),
                team: "red".into(),
                commander: false,
                is_squad: true,
                samples: vec![
                    Sample { t_ms: 0, x: 10.0, y: 20.0, z: 0.0 },
                    // A single corrupt/degenerate sample: x is Infinity,
                    // y/z stay ordinary finite values.
                    Sample { t_ms: 300, x: f32::INFINITY, y: 25.0, z: 0.0 },
                    Sample { t_ms: 600, x: 12.0, y: 22.0, z: 0.0 },
                ],
                down_intervals: vec![],
                dead_intervals: vec![],
            }],
        };
        let out = build_replay_out(&replay);
        assert_eq!(out.bounds.min_x, 0.0, "any non-finite bound resets ALL FOUR to the degenerate zero fallback");
        assert_eq!(out.bounds.min_y, 0.0);
        assert_eq!(out.bounds.max_x, 0.0);
        assert_eq!(out.bounds.max_y, 0.0);
        // The samples themselves are untouched (Infinity is a legitimate
        // f32 -> the fallback only applies to the summary `bounds`, not the
        // per-sample data) -- rounding an Infinity stays Infinity.
        assert!(out.tracks[0].samples[1].1.is_infinite());
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
                is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
            }],
            guid_map: vec![],
        };
        let m = Metrics { players: vec![], timeline: Timeline { resolution_ms: 1000, squad_damage: vec![], cc_applied: vec![], downs: vec![] },
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default() };
        let replay = build_replay(&raw, &enc, DEFAULT_POLL_MS);
        let report = build_report(&enc, &m, "0.1.0", Some(&replay), None);
        assert!(report.replay.is_some());
        let r = report.replay.unwrap();
        assert_eq!(r.poll_ms, DEFAULT_POLL_MS);
        assert_eq!(r.tracks.len(), 1);
        assert_eq!(r.tracks[0].name, "Alice");
        assert_eq!(r.tracks[0].samples[0], (0, 123.5, -9.9), "x/y rounded to 1dp");
    }

    /// M10 Task 2: `missiles` is present (not omitted) only when requested,
    /// and its fields carry the `axilog_core::analysis::missiles::Missiles`
    /// values through unchanged.
    #[test]
    fn missiles_block_present_and_summed_when_requested() {
        use axilog_core::analysis::missiles::build_missiles;
        use axilog_core::evtc::{sc, RawEvent, RawHeader, RawLog};
        use axilog_core::model::Player;

        let player = Player {
            agent_addr: 1, account: ":A.1".into(), character: "Alice".into(),
            profession: "Thief".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None,
            agent_addrs: vec![1],
        };
        let enc = Encounter {
            kind: "wvw".into(), map: "".into(), duration_ms: 1000, build: "".into(),
            revision: 1, recorded_by: None, teams: vec![], players: vec![player],
            enemies: vec![], markers: vec![], tick_rate: None,
        };
        fn missile_ev(time: u64, statechange: u8, src: u64, dst: u64, is_flanking: u8, pad: u32) -> RawEvent {
            RawEvent {
                time, src_agent: src, dst_agent: dst, value: 0, buff_dmg: 0, overstack: 0,
                skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
                dst_master_instid: 0, iff: 0, buff: 0, result: 0, is_activation: 0,
                is_buffremove: 0, is_statechange: statechange, is_flanking, is_shields: 0,
                is_offcycle: 0, pad,
            }
        }
        let raw = RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![],
            events: vec![
                missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
                missile_ev(10, sc::MISSILE_REMOVE, 1, 0, 1, 100),
            ],
            guid_map: vec![],
        };
        let m = Metrics { players: vec![], timeline: Timeline { resolution_ms: 1000, squad_damage: vec![], cc_applied: vec![], downs: vec![] },
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default() };
        let missiles = build_missiles(&raw, &enc);
        let report = build_report(&enc, &m, "0.1.0", None, Some(&missiles));
        assert!(report.missiles.is_some());
        let mo = report.missiles.unwrap();
        assert_eq!(mo.players.len(), 1);
        assert_eq!(mo.players[0].agent_addr, 1);
        assert_eq!(mo.players[0].account, ":A.1", "account must be populated so missiles.players[] joins to players[] by account");
        assert_eq!(mo.players[0].fired, 1);
        assert_eq!(mo.players[0].hit, 1);
        assert_eq!(mo.squad.fired, 1);
        assert_eq!(mo.squad.hit, 1);
    }
}
