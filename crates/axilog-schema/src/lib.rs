use serde::Serialize;
use std::collections::BTreeMap;
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
    /// Best-effort skillMap (M14, Task 2) -- see
    /// `axilog_core::analysis::skill_map`'s module doc for the full
    /// name-gap-vs-EI honesty writeup. ALWAYS present (not opt-in, unlike
    /// `replay`/`missiles`) -- `axilog_core::analysis::Metrics::skill_map`
    /// is always computed, and measured modest in size (a small object per
    /// REFERENCED skill id, not a dump of the whole log skill table -- see
    /// that module's doc comment's "Scope of referenced ids" section).
    /// Keyed by skill id as a string (plain `serde_json` object-key
    /// stringification of the `u32` key), e.g. `"5491"`.
    pub skill_map: BTreeMap<u32, SkillMapEntryOut>,
}
/// One skill's best-effort entry (M14, Task 2) -- mirrors
/// `axilog_core::analysis::skill_map::SkillMapEntry` field-for-field.
/// `auto_attack` is omitted from the JSON entirely (not serialized as
/// `null`) when `None` -- currently ALWAYS the case, see that module's doc
/// comment's "`auto_attack`: OMITTED, not guessed" section for why.
#[derive(Serialize)]
pub struct SkillMapEntryOut {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_attack: Option<bool>,
    pub is_swap: bool,
    pub can_crit: bool,
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
/// One skill id's aggregated hit stats within some grouping (M12, Task 1) --
/// mirrors `axilog_core::analysis::skill_damage::SkillEntry` field-for-field.
/// `hits`/`min`/`max` count only CONTRIBUTING (`dmg > 0`) events, matching
/// this project's established damage predicate (`analysis::damage::
/// accumulate`'s own `if dmg == 0 { continue; }` skip) -- a deliberate,
/// documented divergence from GW2EI's own `totalDamageDist[].hits` (which
/// also counts missed/blocked/invulned/evaded 0-damage attempts, none of
/// which this schema tracks anywhere else either). `crit_hits`/`flank_hits`
/// are hit COUNTS (not damage sums), derived from `result::CRIT` and
/// `is_flanking != 0` respectively -- see `skill_damage`'s module doc for
/// the full derivation/calibration writeup, including the one documented
/// systematic gap versus EI (pet/minion damage folded onto the owner here,
/// excluded entirely from EI's own per-player `totalDamageDist`).
#[derive(Serialize)]
pub struct SkillEntryOut {
    pub skill_id: u32,
    pub total: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    pub crit_hits: u32,
    pub flank_hits: u32,
}
/// One enemy's per-skill outgoing breakdown (M12, Task 1) -- explicit
/// `enemy_id`, not positional, so a consumer can look this up directly
/// against `Report.enemies[].id` without depending on array order.
#[derive(Serialize)]
pub struct PerTargetSkillsOut {
    pub enemy_id: u64,
    pub skills: Vec<SkillEntryOut>,
}
/// Per-skill damage distribution: outgoing (total + per-target) and
/// incoming, each grouped by skill id (M12, Task 1) -- mirrors
/// `axilog_core::analysis::skill_damage::SkillDamageMetrics` field-for-field.
/// `sum(outgoing[*].total) == DamageOut.total` and `sum(taken[*].total) ==
/// PlayerOut.damage_taken` hold exactly by construction (see that module's
/// doc comment). Always present (not gated like `healing`) -- computed
/// unconditionally for every player, same as `boons`/`support`.
#[derive(Serialize)]
pub struct SkillDamageOut {
    pub outgoing: Vec<SkillEntryOut>,
    pub taken: Vec<SkillEntryOut>,
    pub per_target: Vec<PerTargetSkillsOut>,
}
/// One recorded cast of a skill (M14, Task 1) -- mirrors
/// `axilog_core::analysis::rotation::Cast` field-for-field, which itself
/// mirrors GW2EI's `JsonRotation.JsonSkill` (`castTime`/`duration`/
/// `timeGained`/`quickness`) -- see that module's doc comment for the full
/// derivation/calibration writeup.
#[derive(Serialize)]
pub struct CastOut {
    pub cast_time_ms: i64,
    pub duration_ms: i64,
    pub time_gained_ms: i64,
    pub quickness: f64,
}
/// All recorded casts of one skill id, for one player (M14, Task 1) --
/// mirrors `axilog_core::analysis::rotation::SkillRotation`.
#[derive(Serialize)]
pub struct SkillRotationOut {
    pub skill_id: u32,
    pub casts: Vec<CastOut>,
}
/// One enemy's cumulative per-second outgoing-damage series (M12, Task 2) --
/// mirrors `axilog_core::analysis::timeseries::TargetSeries` field-for-
/// field. Explicit `enemy_id`, not positional -- same convention
/// `PerTargetSkillsOut` already uses.
#[derive(Serialize)]
pub struct PlayerTargetSeriesOut {
    pub enemy_id: u64,
    pub damage: Vec<u64>,
}
/// A player's per-second detail block (M12, Task 2), opt-in -- see
/// `PlayerOut::per_second`'s doc comment for the measured size rationale.
/// `damage`/`damage_taken`/every `per_target[].damage` are CUMULATIVE
/// running totals, one entry per second, bucketed the same way
/// `Report.timeline` is (`resolution_ms = 1000`, from the log's first
/// event) -- see `axilog_core::analysis::timeseries`'s module doc for the
/// GW2EI `damage1S`/`damageTaken1S`/`targetDamage1S` cumulative-vs-instant
/// citation this mirrors.
#[derive(Serialize)]
pub struct PlayerPerSecondOut {
    pub damage: Vec<u64>,
    pub damage_taken: Vec<u64>,
    pub per_target: Vec<PlayerTargetSeriesOut>,
}
/// One enemy's whole-fight dps/damage summary (M12, Task 2) -- mirrors
/// `axilog_core::analysis::timeseries::DpsTargetEntry` field-for-field:
/// `damage` is that enemy's final cumulative damage total (the same value
/// `PlayerTargetSeriesOut::damage`'s matching series ends on), and `dps` is
/// a plain `f64` computed directly as `damage / duration_secs` (no
/// fixed-point intermediate -- `DpsTargetEntry` carries no `dps_milli`
/// field or `dps()` method; this doc previously described a design that was
/// never implemented). Gated behind the SAME `include_timeseries` flag as
/// `PlayerOut::per_second` (NOT always-on, despite the M12 plan originally
/// suggesting `dps_targets` could stay always-on since "it's small") -- see
/// `PlayerOut::dps_targets`'s doc comment for the measured size rationale
/// that put it behind that flag instead.
#[derive(Serialize)]
pub struct DpsTargetOut {
    pub enemy_id: u64,
    pub damage: u64,
    pub dps: f64,
}
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
/// The arcdps-methodology contribution family's four stats (M11, Task 2) --
/// mirrors `axilog_core::analysis::contribution::ContributionMetrics`
/// field-for-field. Used for BOTH `PlayerOut::downs_contribution`
/// (outgoing: this player's own credit toward downing enemy players) and
/// `PlayerOut::downed_by` (incoming: what non-squad contributors did to
/// THIS player before each of their own downs, aggregated onto this row,
/// not broken down by attacker) -- see `contribution`'s module doc for the
/// full methodology writeup. Replaces the retired M1-era `down_contribution`
/// 10s-window approximation (schema 0.1 -> 0.2).
#[derive(Serialize)]
pub struct ContributionOut {
    pub damage: u64,
    pub cc: u32,
    pub strips: u32,
    pub movement_impairing: u64,
}
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
/// Outgoing hit-quality stats (M13, Task 1) -- mirrors
/// `axilog_core::analysis::hit_stats::HitStats` field-for-field. See that
/// module's doc comment for the exact EI `statsAll[0]` derivation/citation
/// per field, notably: `against_downed`/`above90_*` are plain per-event
/// wire flags (NOT down-interval/health-tracker state), and this block
/// deliberately does NOT fold pet/minion damage onto the owner (unlike
/// `DamageOut`/`SkillDamageOut`) -- matches EI's own actor-only
/// `statsAll[0]` scope. Always present (not gated), same as `cc`/
/// `downs_contribution` -- no per-target/per-skill combinatorial blowup
/// here, unlike `skill_damage`/`per_second`.
#[derive(Serialize, Default)]
pub struct HitStatsOut {
    pub crit_count: u32,
    pub crit_damage: u64,
    pub flank_count: u32,
    pub glance_count: u32,
    pub moving_count: u32,
    pub connected_count: u32,
    pub connected_damage: u64,
    pub direct_count: u32,
    pub direct_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    pub critable_direct_count: u32,
    pub against_downed_count: u32,
    pub against_downed_damage: u64,
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub above90_power_count: u32,
    pub above90_power_damage: u64,
    pub above90_condition_count: u32,
    pub above90_condition_damage: u64,
}
/// Incoming defenses: hit-outcome counts + damage-taken breakdown (M13,
/// Task 2) -- mirrors `axilog_core::analysis::defenses::DefenseStats`
/// field-for-field. See that module's doc comment for the exact EI
/// `defenses[0]` derivation/citation per field, notably: `dodge_count` is
/// NOT derived from any incoming event (a self-cast dodge-skill count,
/// genuinely independent of `evaded_count`), `power_count`/`power_damage`
/// always equal `strike_count`/`strike_damage` + `life_leech_count`/
/// `life_leech_damage`, and `life_leech_count`/`life_leech_damage` are the
/// TRUE values (a real GW2EI counting bug in its own
/// `lifeLeechDamageTakenCount` is deliberately NOT reproduced here). Purely
/// additive alongside `PlayerOut`'s pre-existing `downs_taken`/`deaths`/
/// `damage_taken`/`cc` fields -- always present (not gated), same
/// always-on convention as `hit_stats`.
#[derive(Serialize, Default)]
pub struct DefensesOut {
    pub blocked_count: u32,
    pub evaded_count: u32,
    pub dodge_count: u32,
    pub missed_count: u32,
    pub interrupted_count: u32,
    pub invulned_count: u32,
    pub strike_count: u32,
    pub strike_damage: u64,
    pub power_count: u32,
    pub power_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub barrier_count: u32,
    pub barrier_damage: u64,
    pub breakbar_count: u32,
    pub breakbar_damage: u64,
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
    pub downs_taken: u32, pub deaths: u32, pub damage_taken: u64,
    pub cc: CcOut,
    /// Outgoing arcdps-methodology contribution toward downing enemy
    /// players (M11, Task 2). See `ContributionOut`'s doc comment.
    pub downs_contribution: ContributionOut,
    /// The mirror: what non-squad contributors did to THIS player before
    /// each of their own downs (M11, Task 2). See `ContributionOut`'s doc
    /// comment.
    pub downed_by: ContributionOut,
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
    pub healing: Option<HealingOut>,
    /// Per-skill damage distribution (M12, Task 1), opt-in like `replay`/
    /// `missiles` -- only present when the caller requested it (CLI
    /// `--skill-damage` / SDK `skill_damage: true` passed to
    /// [`build_report`]). Omitted entirely from the JSON (not `null`) when
    /// not requested, matching `replay`/`missiles`'s own omit-when-absent
    /// convention.
    ///
    /// Measured on the committed WvW fixture (`fixtures/wvw-small.anon.
    /// zevtc`, 42 players): including this block unconditionally for every
    /// player grows the serialized native `Report` from 135,526 to 473,438
    /// bytes (+249%), driven by `per_target`'s combinatorial player x enemy
    /// x skill breakdown (2,105 per-target skill rows alone, out of ~3,500
    /// total skill-stat entries, on this one fixture) -- far past the M12
    /// plan's "~30% growth" size-discipline guideline (stated for Task 2's
    /// per-second block, but the same principle applies here: a real WvW
    /// squad log's enemy roster is large enough that eager per-skill
    /// per-target detail is not something every consumer wants paid for by
    /// default). Opt-in avoids that regression for the default `json`/`csv`/
    /// `table`/`html` output while keeping the underlying computation itself
    /// unconditional and cheap (`axilog_core::analysis::Metrics::players[].
    /// skill_damage` is always populated by `analyze()` -- see that field's
    /// doc comment; this flag only gates whether [`build_report`] copies it
    /// into the schema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_damage: Option<SkillDamageOut>,
    /// Per-second cumulative `damage`/`damage_taken`/`per_target` detail
    /// (M12, Task 2), opt-in like `skill_damage` -- only present when the
    /// caller requested it (CLI `--timeseries` / SDK `timeseries: true`
    /// passed to [`build_report`]). Omitted entirely from the JSON (not
    /// `null`) when not requested, matching `skill_damage`'s own
    /// omit-when-absent convention.
    ///
    /// Measured on the committed WvW fixture (42 players, ~50 one-second
    /// buckets, compact `serde_json::to_string`, same method
    /// `axilog-html/tests/golden_html.rs`'s size-budget tests use):
    /// baseline (`skill_damage`/`timeseries` both off) is 135,526 bytes.
    /// Including `per_second` ALONE (with `dps_targets` held back) grows
    /// that to 335,683 bytes (**+147.7%**) -- driven by `per_target`'s
    /// player x enemy x per-second breakdown -- far past the M12 plan's
    /// "~30% growth" size-discipline guideline, so this stays opt-in for
    /// the default `json`/`csv`/`table`/`html` output, same reasoning as
    /// `skill_damage`. The underlying computation itself stays
    /// unconditional and cheap (`axilog_core::analysis::Metrics::
    /// players[].timeseries` is always populated by `analyze()`); this
    /// flag only gates whether [`build_report`] copies it into the schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_second: Option<PlayerPerSecondOut>,
    /// Per-enemy whole-fight dps/damage summary (M12, Task 2). The plan's
    /// brief called for keeping this always-on ("it's small") unless it
    /// ALSO bloats the default output -- measured (same method/fixture as
    /// `per_second`'s doc comment above), it does: this fixture has up to
    /// 40 enemies per player (872 rows total across all 42 players -- a
    /// real WvW zerg fight enumerates every enemy player/siege/dolyak/
    /// guard the log tracked, not just a boss's handful of adds), so
    /// `dps_targets` ALONE (with `per_second` held back) grows the
    /// baseline from 135,526 to 184,844 bytes (**+36.4%**) -- past the
    /// M12 plan's "~30% growth" guideline on its own, and this was
    /// measured directly by first shipping it unconditionally: it grew the
    /// committed fixture's HTML report from 201,268 to 250,671 bytes,
    /// BREAKING `axilog-html/tests/golden_html.rs::
    /// total_report_size_stays_under_budget`'s existing 250,000-byte gate
    /// even with `per_second`/`skill_damage` both off. So this stays gated
    /// behind the SAME `include_timeseries` flag as `per_second` (not a
    /// second, separate flag) -- see [`build_report`]'s doc comment.
    /// Together, `include_timeseries: true` grows the baseline to 385,001
    /// bytes (**+184.1%**). Omitted entirely from the JSON (not `[]`) when
    /// not requested, same convention as every other opt-in block in this
    /// schema.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dps_targets: Vec<DpsTargetOut>,
    /// Outgoing hit-quality stats (M13, Task 1). See `HitStatsOut`'s doc
    /// comment.
    pub hit_stats: HitStatsOut,
    /// Incoming defenses: hit-outcome counts + damage-taken breakdown (M13,
    /// Task 2). Always present, same convention as `hit_stats`. See
    /// `DefensesOut`'s doc comment.
    pub defenses: DefensesOut,
    /// Per-skill cast list (M14, Task 1), opt-in like `skill_damage`/
    /// `per_second` -- only present when the caller requested it (CLI
    /// `--rotation` / SDK `rotation: true` passed to [`build_report`]).
    /// Omitted entirely from the JSON (not `null`) when not requested,
    /// matching `skill_damage`'s own omit-when-absent convention.
    ///
    /// Measured on the committed WvW fixture (`fixtures/wvw-small.anon.
    /// zevtc`, 42 players, compact `serde_json::to_string`, same method
    /// `skill_damage`/`per_second`'s doc comments use -- against TODAY's
    /// actual baseline, not the M12-era 135,526-byte figure those older
    /// comments cite, which predates M13's always-on `hit_stats`/
    /// `defenses` fields and is stale for a byte-for-byte comparison):
    /// baseline (every opt-in block off) is 170,451 bytes; including
    /// `rotation` alone (1,222 casts across 37 of 42 players -- the
    /// `AnimatedCastEvent`-pipeline subset this module computes, see
    /// `rotation`'s module doc for the ~29% of EI's own richer `rotation[]`
    /// this deliberately excludes) grows that to 284,535 bytes
    /// (**+66.9%**) -- well past the ~30% size-discipline guideline, so
    /// this stays opt-in for the default `json`/`csv`/`table`/`html`
    /// output, same reasoning as `skill_damage`/`per_second`/
    /// `dps_targets`. The underlying computation itself stays unconditional
    /// and cheap (`axilog_core::analysis::Metrics::players[].rotation` is
    /// always populated by `analyze()`); this flag only gates whether
    /// [`build_report`] copies it into the schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Vec<SkillRotationOut>> }
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

/// Converts one `axilog_core::analysis::skill_damage::SkillEntry` into the
/// native schema's `SkillEntryOut`, field-for-field. Standalone helper since
/// `build_report` maps this over three separate lists (`outgoing`, `taken`,
/// and each `per_target[].skills`).
fn skill_entry_out(e: &axilog_core::analysis::skill_damage::SkillEntry) -> SkillEntryOut {
    SkillEntryOut {
        skill_id: e.skill_id, total: e.total, hits: e.hits,
        min: e.min, max: e.max, crit_hits: e.crit_hits, flank_hits: e.flank_hits,
    }
}

/// `replay` (M9, Task 2): pass `Some(&Replay)` (from
/// `axilog_core::analysis::replay::build_replay`, called separately by the
/// caller when `--replay`/SDK `replay: true` was requested) to embed the
/// native replay block; `None` (the default for every existing call site)
/// omits it entirely, matching `ReplayOut`'s skip-when-absent serde
/// attribute. `missiles` (M10, Task 2) works the same way: pass `Some(&
/// Missiles)` (from `axilog_core::analysis::missiles::build_missiles`, when
/// `--missiles`/SDK `missiles: true` was requested) to embed the native
/// missiles block; `None` omits it entirely. `include_skill_damage` (M12,
/// Task 1) is simpler still -- unlike `replay`/`missiles`, the underlying
/// data is already unconditionally computed on every `Metrics::players[]`
/// entry (`analyze()` always populates `PlayerMetrics::skill_damage`, cheap:
/// one grouped scan reusing `analysis::damage`'s predicate), so there's no
/// separate builder call to make first -- just a `bool` gating whether this
/// function COPIES that already-computed data into `PlayerOut::
/// skill_damage`. `false` (every pre-M12 call site, via this crate's own
/// tests being updated alongside this signature change) omits it entirely;
/// `true` (CLI `--skill-damage` / SDK `skill_damage: true`) includes it.
/// See `PlayerOut::skill_damage`'s doc comment for the measured size
/// rationale behind defaulting this to opt-in rather than always-on.
/// `include_timeseries` (M12, Task 2) works the same way as
/// `include_skill_damage` -- `PlayerMetrics::timeseries` is always
/// computed by `analyze()`; this bool gates whether BOTH `PlayerOut::
/// per_second` AND `PlayerOut::dps_targets` are populated. The plan
/// suggested `dps_targets` could stay always-on ("it's small"), but
/// measured on the committed fixture it independently exceeds the same
/// ~30% size guideline on its own (+36.4%, see `PlayerOut::dps_targets`'s
/// doc comment) and broke the HTML report's existing 250,000-byte size
/// budget when shipped unconditionally -- so it's gated behind this SAME
/// flag rather than a separate one, for one simple on/off switch per
/// caller.
///
/// `include_rotation` (M14, Task 1) works the same way as
/// `include_skill_damage`/`include_timeseries` -- `PlayerMetrics::rotation`
/// is always computed by `analyze()`; this bool gates whether
/// `PlayerOut::rotation` is populated. See `PlayerOut::rotation`'s doc
/// comment for the measured size rationale. This is the 8th parameter
/// (past clippy's default `too_many_arguments` threshold of 7) -- every
/// prior opt-in block (`replay`/`missiles`/`skill_damage`/`timeseries`)
/// added one more `bool`/`Option<&T>` the same way rather than introducing
/// an options struct, so this one flag is allowed rather than restructuring
/// the whole call surface for one more parameter.
#[allow(clippy::too_many_arguments)]
pub fn build_report(
    enc: &Encounter,
    metrics: &Metrics,
    axilog_version: &str,
    replay: Option<&Replay>,
    missiles: Option<&Missiles>,
    include_skill_damage: bool,
    include_timeseries: bool,
    include_rotation: bool,
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
            downs_taken: m.map(|m| m.downs_taken).unwrap_or(0),
            deaths: m.map(|m| m.deaths).unwrap_or(0),
            damage_taken: m.map(|m| m.damage_taken).unwrap_or(0),
            cc: CcOut { applied_total: m.map(|m| m.cc_applied).unwrap_or(0),
                        applied_duration_ms: m.map(|m| m.cc_duration_ms).unwrap_or(0),
                        stun_breaks: m.map(|m| m.stun_breaks).unwrap_or(0),
                        removed_stun_duration_ms: m.map(|m| m.removed_stun_duration_ms).unwrap_or(0) },
            downs_contribution: m.map(|m| ContributionOut {
                damage: m.downs_contribution.damage, cc: m.downs_contribution.cc,
                strips: m.downs_contribution.strips, movement_impairing: m.downs_contribution.movement_impairing,
            }).unwrap_or(ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 }),
            downed_by: m.map(|m| ContributionOut {
                damage: m.downed_by.damage, cc: m.downed_by.cc,
                strips: m.downed_by.strips, movement_impairing: m.downed_by.movement_impairing,
            }).unwrap_or(ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 }),
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
            skill_damage: if include_skill_damage {
                Some(m.map(|m| SkillDamageOut {
                    outgoing: m.skill_damage.outgoing.iter().map(skill_entry_out).collect(),
                    taken: m.skill_damage.taken.iter().map(skill_entry_out).collect(),
                    per_target: m.skill_damage.per_target.iter().map(|t| PerTargetSkillsOut {
                        enemy_id: t.enemy_id,
                        skills: t.skills.iter().map(skill_entry_out).collect(),
                    }).collect(),
                }).unwrap_or(SkillDamageOut { outgoing: vec![], taken: vec![], per_target: vec![] }))
            } else {
                None
            },
            per_second: if include_timeseries {
                Some(m.map(|m| PlayerPerSecondOut {
                    damage: m.timeseries.damage.clone(),
                    damage_taken: m.timeseries.damage_taken.clone(),
                    per_target: m.timeseries.per_target.iter().map(|t| PlayerTargetSeriesOut {
                        enemy_id: t.enemy_id,
                        damage: t.damage.clone(),
                    }).collect(),
                }).unwrap_or(PlayerPerSecondOut { damage: vec![], damage_taken: vec![], per_target: vec![] }))
            } else {
                None
            },
            dps_targets: if include_timeseries {
                m.map(|m| m.timeseries.dps_targets.iter().map(|d| DpsTargetOut {
                    enemy_id: d.enemy_id, damage: d.damage, dps: d.dps,
                }).collect()).unwrap_or_default()
            } else {
                vec![]
            },
            hit_stats: m.map(|m| { let h = m.hit_stats; HitStatsOut {
                crit_count: h.crit_count, crit_damage: h.crit_damage,
                flank_count: h.flank_count, glance_count: h.glance_count,
                moving_count: h.moving_count,
                connected_count: h.connected_count, connected_damage: h.connected_damage,
                direct_count: h.direct_count, direct_damage: h.direct_damage,
                condition_count: h.condition_count, condition_damage: h.condition_damage,
                critable_direct_count: h.critable_direct_count,
                against_downed_count: h.against_downed_count, against_downed_damage: h.against_downed_damage,
                life_leech_count: h.life_leech_count, life_leech_damage: h.life_leech_damage,
                above90_power_count: h.above90_power_count, above90_power_damage: h.above90_power_damage,
                above90_condition_count: h.above90_condition_count, above90_condition_damage: h.above90_condition_damage,
            }}).unwrap_or_default(),
            defenses: m.map(|m| { let d = m.defenses; DefensesOut {
                blocked_count: d.blocked_count, evaded_count: d.evaded_count, dodge_count: d.dodge_count,
                missed_count: d.missed_count, interrupted_count: d.interrupted_count, invulned_count: d.invulned_count,
                strike_count: d.strike_count, strike_damage: d.strike_damage,
                power_count: d.power_count, power_damage: d.power_damage,
                condition_count: d.condition_count, condition_damage: d.condition_damage,
                life_leech_count: d.life_leech_count, life_leech_damage: d.life_leech_damage,
                barrier_count: d.barrier_count, barrier_damage: d.barrier_damage,
                breakbar_count: d.breakbar_count, breakbar_damage: d.breakbar_damage,
            }}).unwrap_or_default(),
            rotation: if include_rotation {
                Some(m.map(|m| m.rotation.iter().map(|s| SkillRotationOut {
                    skill_id: s.skill_id,
                    casts: s.casts.iter().map(|c| CastOut {
                        cast_time_ms: c.cast_time_ms, duration_ms: c.duration_ms,
                        time_gained_ms: c.time_gained_ms, quickness: c.quickness,
                    }).collect(),
                }).collect()).unwrap_or_default())
            } else {
                None
            },
        }
    }).collect();
    Report {
        // M11 Task 2: 0.1 -> 0.2 -- the legacy `down_contribution` field is
        // removed (replaced by `downs_contribution`/`downed_by`), a
        // deliberate breaking change (see the contribution family's module
        // doc / the M11 plan's "grep-audit for the old field name" gate).
        schema_version: "0.2", axilog_version: axilog_version.to_string(),
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
        // M14 Task 2: straight copy of `Metrics::skill_map` -- already
        // fully computed by `analyze()`, no gating flag needed (see
        // `Report::skill_map`'s doc comment for why this stays always-on).
        skill_map: metrics
            .skill_map
            .iter()
            .map(|(&id, e)| {
                (id, SkillMapEntryOut { name: e.name.clone(), auto_attack: e.auto_attack, is_swap: e.is_swap, can_crit: e.can_crit })
            })
            .collect(),
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
            combat_participant_enemies: [9u64].into_iter().collect(), skill_map: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
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
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default(), skill_map: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["schema_version"], "0.2");
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
            has_healing_extension: true, combat_participant_enemies: Default::default(), skill_map: Default::default() };
        let report = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
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

    /// M12 Task 1: `skill_damage` is opt-in like `replay`/`missiles` --
    /// omitted entirely (`include_skill_damage: false`, every existing call
    /// site) even though `PlayerMetrics::skill_damage` is ALWAYS computed
    /// (unlike `replay`/`missiles`, there's no separate builder call to
    /// skip); present (and correctly copied field-for-field) only when
    /// `include_skill_damage: true`.
    #[test]
    fn skill_damage_is_opt_in_like_replay_and_missiles() {
        use axilog_core::analysis::skill_damage::{PerTargetSkills, SkillDamageMetrics, SkillEntry};
        let enc = Encounter { kind:"wvw".into(), map:"".into(),
            duration_ms:1000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![
                Player{agent_addr:1,account:":A.1".into(),character:"A".into(),
                    profession:"Thief".into(),elite_spec:"".into(),team:"red".into(),
                    subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]},
            ],
            enemies:vec![], markers:vec![], tick_rate:None };
        let entry = SkillEntry { skill_id: 100, total: 50, hits: 1, min: 50, max: 50, crit_hits: 0, flank_hits: 0 };
        let m = Metrics { players: vec![
            PlayerMetrics{agent_addr:1,
                skill_damage: SkillDamageMetrics {
                    outgoing: vec![entry.clone()],
                    taken: vec![entry.clone()],
                    per_target: vec![PerTargetSkills { enemy_id: 9, skills: vec![entry] }],
                },
                ..Default::default()},
        ],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: false, combat_participant_enemies: Default::default(), skill_map: Default::default() };

        let omitted = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
        let v = serde_json::to_value(&omitted).unwrap();
        assert!(v["players"][0].get("skill_damage").is_none(), "skill_damage must be omitted when not requested");

        let included = build_report(&enc, &m, "0.1.0", None, None, true, false, false);
        let v = serde_json::to_value(&included).unwrap();
        let sd = &v["players"][0]["skill_damage"];
        assert_eq!(sd["outgoing"][0]["skill_id"], 100);
        assert_eq!(sd["outgoing"][0]["total"], 50);
        assert_eq!(sd["taken"][0]["skill_id"], 100);
        assert_eq!(sd["per_target"][0]["enemy_id"], 9);
        assert_eq!(sd["per_target"][0]["skills"][0]["skill_id"], 100);
    }

    /// M12 Task 2: `per_second` AND `dps_targets` are BOTH opt-in, gated by
    /// the SAME `include_timeseries` bool (independent of `include_skill_
    /// damage`) -- both omitted entirely when not requested even though
    /// `PlayerMetrics::timeseries` is ALWAYS computed; both present (and
    /// correctly copied field-for-field) only when `include_timeseries:
    /// true`. Unlike `skill_damage`, `dps_targets` is NOT always-on -- see
    /// `PlayerOut::dps_targets`'s doc comment for the measured size
    /// rationale (it independently broke the HTML size budget when shipped
    /// unconditionally).
    #[test]
    fn per_second_and_dps_targets_are_both_gated_by_include_timeseries() {
        use axilog_core::analysis::timeseries::{DpsTargetEntry, TargetSeries, TimeseriesMetrics};
        let enc = Encounter { kind:"wvw".into(), map:"".into(),
            duration_ms:2000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![
                Player{agent_addr:1,account:":A.1".into(),character:"A".into(),
                    profession:"Thief".into(),elite_spec:"".into(),team:"red".into(),
                    subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]},
            ],
            enemies:vec![], markers:vec![], tick_rate:None };
        let m = Metrics { players: vec![
            PlayerMetrics{agent_addr:1,
                timeseries: TimeseriesMetrics {
                    damage: vec![50, 80],
                    damage_taken: vec![10, 10],
                    per_target: vec![TargetSeries { enemy_id: 9, damage: vec![50, 80] }],
                    dps_targets: vec![DpsTargetEntry { enemy_id: 9, damage: 80, dps: 40.0 }],
                },
                ..Default::default()},
        ],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0,0],cc_applied:vec![0,0],downs:vec![0,0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: false, combat_participant_enemies: Default::default(), skill_map: Default::default() };

        let omitted = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
        let v = serde_json::to_value(&omitted).unwrap();
        assert!(v["players"][0].get("per_second").is_none(), "per_second must be omitted when not requested");
        assert!(v["players"][0].get("dps_targets").is_none(), "dps_targets must be omitted when not requested");

        let included = build_report(&enc, &m, "0.1.0", None, None, false, true, false);
        let v = serde_json::to_value(&included).unwrap();
        let ps = &v["players"][0]["per_second"];
        assert_eq!(ps["damage"], serde_json::json!([50, 80]));
        assert_eq!(ps["damage_taken"], serde_json::json!([10, 10]));
        assert_eq!(ps["per_target"][0]["enemy_id"], 9);
        assert_eq!(ps["per_target"][0]["damage"], serde_json::json!([50, 80]));
        assert_eq!(v["players"][0]["dps_targets"][0]["enemy_id"], 9);
        assert_eq!(v["players"][0]["dps_targets"][0]["damage"], 80);
        assert_eq!(v["players"][0]["dps_targets"][0]["dps"], 40.0);
    }

    /// M14 Task 1: `rotation` is opt-in like `skill_damage`/`per_second` --
    /// omitted entirely (`include_rotation: false`, every existing call
    /// site) even though `PlayerMetrics::rotation` is ALWAYS computed by
    /// `analyze()`; present (and correctly copied field-for-field) only
    /// when `include_rotation: true`. See `PlayerOut::rotation`'s doc
    /// comment for the measured size rationale (+66.9% on the committed
    /// fixture when always-on).
    #[test]
    fn rotation_is_opt_in_like_skill_damage_and_timeseries() {
        use axilog_core::analysis::rotation::{Cast, SkillRotation};
        let enc = Encounter { kind:"wvw".into(), map:"".into(),
            duration_ms:1000, build:"20260114".into(), revision:1, recorded_by:None,
            teams:vec![], players:vec![
                Player{agent_addr:1,account:":A.1".into(),character:"A".into(),
                    profession:"Thief".into(),elite_spec:"".into(),team:"red".into(),
                    subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,agent_addrs:vec![1]},
            ],
            enemies:vec![], markers:vec![], tick_rate:None };
        let m = Metrics { players: vec![
            PlayerMetrics{agent_addr:1,
                rotation: vec![SkillRotation { skill_id: 500, casts: vec![
                    Cast { cast_time_ms: 100, duration_ms: 700, time_gained_ms: 300, quickness: 0.0 },
                ]}],
                ..Default::default()},
        ],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: false, combat_participant_enemies: Default::default(), skill_map: Default::default() };

        let omitted = build_report(&enc, &m, "0.1.0", None, None, false, false, false);
        let v = serde_json::to_value(&omitted).unwrap();
        assert!(v["players"][0].get("rotation").is_none(), "rotation must be omitted when not requested");

        let included = build_report(&enc, &m, "0.1.0", None, None, false, false, true);
        let v = serde_json::to_value(&included).unwrap();
        let r = &v["players"][0]["rotation"][0];
        assert_eq!(r["skill_id"], 500);
        assert_eq!(r["casts"][0]["cast_time_ms"], 100);
        assert_eq!(r["casts"][0]["duration_ms"], 700);
        assert_eq!(r["casts"][0]["time_gained_ms"], 300);
        assert_eq!(r["casts"][0]["quickness"], 0.0);
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
                is_activation: 0, is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: sc::POSITION,
                is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
            }],
            guid_map: vec![],
        };
        let m = Metrics { players: vec![], timeline: Timeline { resolution_ms: 1000, squad_damage: vec![], cc_applied: vec![], downs: vec![] },
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default(), skill_map: Default::default() };
        let replay = build_replay(&raw, &enc, DEFAULT_POLL_MS);
        let report = build_report(&enc, &m, "0.1.0", Some(&replay), None, false, false, false);
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
                is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: statechange, is_flanking, is_shields: 0,
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
            has_healing_extension: Default::default(), combat_participant_enemies: Default::default(), skill_map: Default::default() };
        let missiles = build_missiles(&raw, &enc);
        let report = build_report(&enc, &m, "0.1.0", None, Some(&missiles), false, false, false);
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
