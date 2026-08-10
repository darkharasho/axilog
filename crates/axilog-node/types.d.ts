/**
 * Hand-maintained TypeScript definitions for axilog's native `Report`
 * shape (M5 Task 2).
 *
 * napi-rs's typegen only sees the Rust *function signatures* declared in
 * `src/lib.rs`; every exported function here returns a plain
 * `serde_json::Value` (see that file's module doc for why -- napi's
 * serde-json interop turns a `Value::Object` into a plain JS object
 * key-by-key, keeping the exact snake_case field names `axilog_schema`
 * already produces, rather than the camelCase rewriting `#[napi(object)]`
 * would apply). Because the return type is an opaque `Value`, napi's dts
 * generator can only ever emit `any` for `parseFile`/`parseBuffer` --  it
 * has no visibility into `axilog_schema::Report`'s actual field layout.
 *
 * This file closes that gap by hand-transcribing `axilog_schema::Report`
 * (`crates/axilog-schema/src/lib.rs` -- the serde struct definitions are
 * the source of truth; keep this file in sync with that one) into real
 * TypeScript types, and `index.d.ts` imports `Report` from here to type
 * `parseFile`/`parseBuffer`'s return value (see `index.d.ts`'s top-of-file
 * comment and `scripts/patch-dts.mjs` for how that reference survives
 * `napi build` regenerating `index.d.ts`).
 *
 * `parseFileEi`'s return value (the Elite-Insights-compatibility JSON from
 * `axilog_ei::to_ei_json`) is a materially different, larger shape (not a
 * serialized `Report`) and isn't typed here -- it stays `any` in
 * `index.d.ts`, called out in that file's doc comment.
 *
 * Numeric-field note: every Rust `u64`/`u32`/`f64` field below is typed as
 * plain `number` -- napi's serde-json bridge converts JSON numbers to JS
 * `number` (not `BigInt`), which is exact for every value this schema
 * actually produces (agent-table addresses/ids, ms durations, damage
 * totals) but would lose precision for a `u64` beyond 2^53; documented
 * here rather than silently assumed.
 */

/** One `CBTS_MARKER` assignment observed in the log (native-only, no EI equivalent). */
export interface MarkerAssignmentOut {
  agent_addr: number
  marker: string
  time_ms: number
}

/** `CBTS_TICK` tick-rate telemetry (native-only). Omitted from `EncounterOut` when the log has fewer than two `CBTS_TICK` events. */
export interface TickRateOut {
  avg: number
  min: number
  per_second: number[]
}

/** Commander-tag colour/variant, present alongside `PlayerOut.commander` when the player has a tag. */
export interface CommanderTagOut {
  variant: string
  guid: string
}

export interface TeamOut {
  color: string
  team_id: number
  /** Stable content GUID for this team, when known (omitted, not null, when absent). */
  guid?: string
}

export interface PerEnemyOut {
  enemy_id: number
  total: number
}

export interface DamageOut {
  total: number
  dps: number
  per_enemy: PerEnemyOut[]
}

/**
 * One skill id's aggregated hit stats within some grouping (M12, Task 1).
 * `hits`/`min`/`max` count only CONTRIBUTING (`dmg > 0`) events, matching
 * this project's established damage predicate -- a deliberate divergence
 * from GW2EI's own `totalDamageDist[].hits` (which also counts 0-damage
 * missed/blocked/invulned/evaded attempts). `crit_hits`/`flank_hits` are
 * hit COUNTS (not damage sums).
 */
export interface SkillEntryOut {
  skill_id: number
  total: number
  hits: number
  min: number
  max: number
  crit_hits: number
  flank_hits: number
}

/** One enemy's per-skill outgoing breakdown (M12, Task 1) -- explicit `enemy_id`, not positional. */
export interface PerTargetSkillsOut {
  enemy_id: number
  skills: SkillEntryOut[]
}

/**
 * Per-skill damage distribution: outgoing (total + per-target) and
 * incoming, each grouped by skill id (M12, Task 1). `sum(outgoing[*].total)
 * == DamageOut.total` and `sum(taken[*].total) == PlayerOut.damage_taken`
 * hold exactly by construction. Pet/minion damage is folded onto the owner
 * here (using the pet's own skill id), matching `DamageOut.total`'s own
 * pet-fold -- unlike GW2EI's `totalDamageDist`, which tracks the player
 * actor only and excludes pet/minion damage entirely.
 */
export interface SkillDamageOut {
  outgoing: SkillEntryOut[]
  taken: SkillEntryOut[]
  per_target: PerTargetSkillsOut[]
}

/** One enemy's cumulative per-second outgoing-damage series (M12, Task 2). */
export interface PlayerTargetSeriesOut {
  enemy_id: number
  damage: number[]
}

/**
 * A player's per-second detail block (M12, Task 2), opt-in -- see
 * `PlayerOut.per_second`. `damage`/`damage_taken`/every `per_target[].damage`
 * are CUMULATIVE running totals, one entry per second, bucketed the same way
 * `Report.timeline` is (`resolution_ms = 1000`, from the log's first event)
 * -- mirrors GW2EI's `damage1S`/`damageTaken1S`/`targetDamage1S` cumulative
 * (not instant-delta) shape.
 */
export interface PlayerPerSecondOut {
  damage: number[]
  damage_taken: number[]
  per_target: PlayerTargetSeriesOut[]
}

/** One enemy's whole-fight dps/damage summary (M12, Task 2) -- see `PlayerOut.dps_targets`. */
export interface DpsTargetOut {
  enemy_id: number
  damage: number
  dps: number
}

export interface CcOut {
  applied_total: number
  applied_duration_ms: number
  stun_breaks: number
  removed_stun_duration_ms: number
}

/** Self/group/squad boon-generation attribution, 0-100 (duration boons) / raw average-concurrent-stack-count (intensity boons) scale -- same scale as `BoonOut.presence_pct`/`avg_stacks`. */
export interface GenerationOut {
  self_pct: number
  group_pct: number
  squad_pct: number
}

/**
 * One tracked boon's whole-fight summary for one player.
 * `presence_pct` is "% of the fight with >=1 held stack" (0-100), for every
 * boon. `avg_stacks` (time-weighted mean held-stack count) is only present
 * for the two INTENSITY-type boons (Might, Stability); omitted (not a
 * meaningless 0) for the other 10 duration-type boons.
 */
export interface BoonOut {
  id: number
  name: string
  presence_pct: number
  avg_stacks?: number
  generation: GenerationOut
}

/** Condition-cleanse/boon-strip/resurrect counts. Stun-break counts stay on `CcOut`. */
export interface SupportOut {
  cleanses: number
  cleanses_self: number
  strips: number
  resurrects: number
}

/**
 * arcdps healing-extension totals (M10, Task 1) -- outgoing healing/barrier
 * scalars, mirroring EI's `extHealingStats`/`extBarrierStats`.
 * `healing_out_allies` is `healing_out_total - healing_out_self`.
 */
export interface HealingOut {
  healing_out_total: number
  healing_out_allies: number
  healing_out_self: number
  barrier_out: number
  downed_healing_out: number
}

/**
 * The arcdps-methodology contribution family's four stats (M11, Task 2) --
 * used for both `PlayerOut.downs_contribution` (outgoing: this player's own
 * credit toward downing enemy players) and `PlayerOut.downed_by` (incoming:
 * what non-squad contributors did to THIS player before each of their own
 * downs, aggregated onto this row, not broken down by attacker). Replaces
 * the retired M1-era `down_contribution` 10s-window approximation (schema
 * 0.1 -> 0.2).
 */
export interface ContributionOut {
  damage: number
  cc: number
  strips: number
  movement_impairing: number
}

/**
 * Outgoing hit-quality stats (M13, Task 1) -- mirrors EI's `statsAll[0]`.
 * `against_downed`/`above90_*` are plain per-event wire flags (NOT
 * down-interval/health-tracker state); this block deliberately does NOT
 * fold pet/minion damage onto the owner (unlike `DamageOut`/
 * `SkillDamageOut`) -- EI's own `statsAll[0]` is actor-only. Always present
 * (not gated) -- no per-target/per-skill combinatorial blowup here.
 */
export interface HitStatsOut {
  crit_count: number
  crit_damage: number
  flank_count: number
  glance_count: number
  moving_count: number
  connected_count: number
  connected_damage: number
  direct_count: number
  direct_damage: number
  condition_count: number
  condition_damage: number
  critable_direct_count: number
  against_downed_count: number
  against_downed_damage: number
  life_leech_count: number
  life_leech_damage: number
  above90_power_count: number
  above90_power_damage: number
  above90_condition_count: number
  above90_condition_damage: number
}

/**
 * Incoming defenses: hit-outcome counts + damage-taken breakdown (M13,
 * Task 2) -- mirrors EI's `defenses[0]`. `dodge_count` is NOT derived from
 * any incoming event (a self-cast dodge-skill count, independent of
 * `evaded_count`); `power_count`/`power_damage` always equal
 * `strike_count`/`strike_damage` + `life_leech_count`/`life_leech_damage`;
 * `life_leech_count`/`life_leech_damage` are the TRUE values (a real GW2EI
 * counting bug in its own `lifeLeechDamageTakenCount` is deliberately not
 * reproduced). Purely additive alongside `downs_taken`/`deaths`/
 * `damage_taken`/`cc`. Always present (not gated), like `hit_stats`.
 */
export interface DefensesOut {
  blocked_count: number
  evaded_count: number
  dodge_count: number
  missed_count: number
  interrupted_count: number
  invulned_count: number
  strike_count: number
  strike_damage: number
  power_count: number
  power_damage: number
  condition_count: number
  condition_damage: number
  life_leech_count: number
  life_leech_damage: number
  barrier_count: number
  barrier_damage: number
  breakbar_count: number
  breakbar_damage: number
}

export interface PlayerOut {
  account: string
  character: string
  profession: string
  elite_spec: string
  team: string
  subgroup: number
  in_squad: boolean
  commander: boolean
  /** The player's current squad marker, name or hex GUID fallback. Omitted when no marker is assigned. */
  marker?: string
  /** Present when `commander` is true. */
  commander_tag?: CommanderTagOut
  damage: DamageOut
  downs_dealt: number
  kills_dealt: number
  downs_taken: number
  deaths: number
  damage_taken: number
  cc: CcOut
  /**
   * Outgoing arcdps-methodology contribution toward downing enemy players
   * (M11, Task 2). See `ContributionOut`.
   */
  downs_contribution: ContributionOut
  /**
   * The mirror: what non-squad contributors did to THIS player before each
   * of their own downs, aggregated onto this row, not broken down by
   * attacker (M11, Task 2).
   */
  downed_by: ContributionOut
  /** One entry per tracked boon id, in `axilog_core::analysis::buffs::BOON_IDS` order. */
  boons: BoonOut[]
  support: SupportOut
  /**
   * arcdps healing-extension totals (M10, Task 1). Omitted entirely (not
   * present as a `null`/all-zero object) when the log carries no
   * healing-extension data at all -- a real "no data" signal, not "the
   * player never healed".
   */
  healing?: HealingOut
  /**
   * Per-skill damage distribution (M12, Task 1), opt-in like `replay`/
   * `missiles` -- present only when requested (`{ skillDamage: true }`,
   * see `ParseOptions.skillDamage` in `index.d.ts`). Omitted entirely
   * (not `null`) when not requested; measured +249% native JSON size on
   * the committed fixture when always-on, hence opt-in rather than
   * always-present like `boons`/`support`.
   */
  skill_damage?: SkillDamageOut
  /**
   * Per-second cumulative `damage`/`damage_taken`/`per_target` detail
   * (M12, Task 2), opt-in like `skill_damage` -- present only when
   * requested (`{ timeseries: true }`, see `ParseOptions.timeseries` in
   * `index.d.ts`). Omitted entirely (not `null`) when not requested;
   * measured +147.7% native JSON size on the committed fixture when
   * always-on, hence opt-in.
   */
  per_second?: PlayerPerSecondOut
  /**
   * Per-enemy whole-fight dps/damage summary (M12, Task 2). Gated by the
   * SAME `{ timeseries: true }` option as `per_second` (NOT always-on --
   * measured +36.4% native JSON size alone on the committed fixture, a
   * real WvW log can enumerate dozens of enemies per player). Omitted
   * entirely (not `[]`) when not requested.
   */
  dps_targets?: DpsTargetOut[]
  /**
   * Outgoing hit-quality stats (M13, Task 1). Always present, unlike
   * `skill_damage`/`per_second`/`dps_targets`. See `HitStatsOut`.
   */
  hit_stats: HitStatsOut
  /**
   * Incoming defenses: hit-outcome counts + damage-taken breakdown (M13,
   * Task 2). Always present, same as `hit_stats`. See `DefensesOut`.
   */
  defenses: DefensesOut
  /**
   * Per-skill cast list (M14, Task 1), opt-in like `skill_damage` --
   * present only when requested (`{ rotation: true }`, see
   * `ParseOptions.rotation` in `index.d.ts`). Omitted entirely (not
   * `null`) when not requested.
   */
  rotation?: SkillRotationOut[]
  /**
   * Per-modifier outgoing/incoming damage-modifier stats (M16), opt-in --
   * present only when requested (`{ modifiers: true }`, see
   * `ParseOptions.modifiers` in `index.d.ts`). Omitted entirely (not
   * `null`) when not requested. The ids index `Report.damage_mod_map`.
   */
  damage_mods?: DamageModsOut
}

/** One skill's cast list (M14, Task 1). */
export interface SkillRotationOut {
  skill_id: number
  casts: CastOut[]
}

/** One animated cast (M14, Task 1). */
export interface CastOut {
  cast_time_ms: number
  duration_ms: number
  /** Negative when the cast was interrupted/cancelled early. */
  time_gained_ms: number
  quickness: boolean
}

/**
 * A player's damage-modifier block (M16), split by direction exactly as
 * Elite Insights' own `damageModifiers`/`incomingDamageModifiers` are.
 * Only modifiers with at least one qualifying hit appear (EI's own
 * emission rule).
 */
export interface DamageModsOut {
  outgoing: DamageModEntryOut[]
  /** Ids here are NEGATIVE -- the sign is the direction. */
  incoming: DamageModEntryOut[]
}

/** One `(player, damage modifier)` row (M16). */
export interface DamageModEntryOut {
  /** Signed modifier id; negative means incoming. Key into `Report.damage_mod_map`. */
  id: number
  /** Hits the modifier actually applied to. */
  hit_count: number
  /** Hits that were eligible for it at all. */
  total_hit_count: number
  /** `sum(gain * damage)`, rounded to 3 decimals. */
  damage_gain: number
  /** The damage aggregate `damage_gain` is measured against. */
  total_damage: number
}

/** One `Report.damage_mod_map` entry (M16) -- EI's `DamageModDesc`. */
export interface DamageModDescOut {
  name: string
  icon: string
  /** EI's full tooltip: the base description plus the derived `<br>...` suffixes. */
  description: string
  non_multiplier: boolean
  is_counter: boolean
  skill_based: boolean
  approximate: boolean
  incoming: boolean
}

/** One `Report.skill_map` entry (M14, Task 2), best-effort. */
export interface SkillMapEntryOut {
  name: string
  /** Omitted entirely -- never derivable without the external GW2 API. */
  auto_attack?: boolean
  is_swap: boolean
  can_crit: boolean
}

export interface EnemyOut {
  id: number
  name: string
  team: string
  is_player: boolean
  /** Mirrors `PlayerOut.marker`. Omitted when absent. */
  marker?: string
}

export interface PerSecondOut {
  squad_damage: number[]
  cc_applied: number[]
  downs: number[]
}

export interface TimelineOut {
  resolution_ms: number
  per_second: PerSecondOut
}

export interface EncounterOut {
  kind: string
  map: string
  duration_ms: number
  build: string
  revision: number
  /** `null` (not omitted) when unknown -- `Option<String>` without `skip_serializing_if`. */
  recorded_by: string | null
  teams: TeamOut[]
  /** Every `CBTS_MARKER` assignment observed in the log, across all agents -- not just squad/enemy players. Always present (possibly empty), never omitted. */
  markers: MarkerAssignmentOut[]
  /** Omitted entirely (not `null`) when the log has fewer than two `CBTS_TICK` events. */
  tick_rate?: TickRateOut
}

/** Min/max `x`/`y` observed across every `ReplayOut.tracks[].samples` -- lets a consumer size a viewBox without a second pass over `tracks`. */
export interface ReplayBoundsOut {
  min_x: number
  min_y: number
  max_x: number
  max_y: number
}

/**
 * One tracked agent's combat-replay track (M9, Task 2). `name`/`team` mirror
 * the display-field precedence used elsewhere (`PlayerOut.character` for
 * squad players, `EnemyOut.name` for enemy-player representatives).
 * `samples` are `[t_ms, x, y]` triples (`x`/`y` rounded to 1 decimal place);
 * `down_intervals`/`dead_intervals` are `[start_ms, end_ms]` pairs.
 */
export interface ReplayTrackOut {
  name: string
  team: string
  commander: boolean
  is_squad: boolean
  samples: [number, number, number][]
  down_intervals: [number, number][]
  dead_intervals: [number, number][]
}

/**
 * Combat-replay position tracks (M9, Task 2), native-only -- present only
 * when the caller opted in (CLI `--replay` / SDK `replay: true`). See
 * `axilog_core::analysis::replay` for how `poll_ms`/samples/intervals are
 * computed.
 */
export interface ReplayOut {
  poll_ms: number
  bounds: ReplayBoundsOut
  tracks: ReplayTrackOut[]
}

/**
 * One squad player's missile totals (M10, Task 2), account-folded across
 * relog/build-swap addrs like every other per-player metric. `account`
 * (final-review fix wave) is the join key back to `Report.players[]` --
 * `agent_addr` alone isn't exposed anywhere else in the native JSON. See
 * `axilog_core::analysis::missiles`'s module doc for exactly what `fired`/
 * `hit`/`denied`/`reflected_at_self` are (and are NOT) attributable to --
 * notably `denied` deliberately does not distinguish blocked/reflected/
 * destroyed/expired outcomes, and `reflected_at_self` is an explicitly
 * labeled heuristic (GW2EI's own "Maybe"-prefixed signal), not a certainty.
 */
export interface PlayerMissilesOut {
  agent_addr: number
  account: string
  fired: number
  hit: number
  denied: number
  reflected_at_self: number
}

/**
 * Squad-wide missile totals (M10, Task 2) -- the sum of every
 * `PlayerMissilesOut` entry, plus the aggregate, unattributed "incoming,
 * denied" defensive rollup (`incoming_fired`/`incoming_denied`: no
 * per-player credit exists for who denied an incoming missile, per the
 * `axilog_core::analysis::missiles` module doc).
 */
export interface SquadMissilesOut {
  fired: number
  hit: number
  denied: number
  incoming_fired: number
  incoming_denied: number
}

/**
 * Opt-in missile (projectile) analytics (M10, Task 2), native-only --
 * present only when the caller opted in (CLI `--missiles` / SDK
 * `{ missiles: true }`, see `ParseOptions.missiles`, added final-review
 * fix wave).
 */
export interface MissilesOut {
  players: PlayerMissilesOut[]
  squad: SquadMissilesOut
}

/**
 * axilog's native report shape (`axilog_schema::Report`), as returned by
 * `parseFile`/`parseBuffer`. `schema_version` is currently always `"0.2"`.
 */
export interface Report {
  schema_version: string
  axilog_version: string
  encounter: EncounterOut
  players: PlayerOut[]
  enemies: EnemyOut[]
  timeline: TimelineOut
  /** Structured, user-facing analysis warnings (e.g. an unsupported post-rework build). Omitted entirely (not `[]`) when there are none. */
  warnings?: string[]
  /** Opt-in combat-replay block (M9, Task 2) -- present only when requested via `{ replay: true }`. Omitted entirely (not `null`) otherwise. */
  replay?: ReplayOut
  /**
   * Opt-in missile (projectile) analytics block (M10, Task 2) -- present
   * only when requested via `{ missiles: true }` (`ParseOptions.missiles`,
   * added final-review fix wave). Omitted entirely (not `null`) otherwise.
   */
  missiles?: MissilesOut
  /**
   * Best-effort skill name table (M14, Task 2), keyed by skill id as a
   * string. Always present (never opt-in), possibly empty.
   */
  skill_map: Record<string, SkillMapEntryOut>
  /**
   * Definition metadata for every damage-modifier id referenced by any
   * `players[].damage_mods` row (M16), keyed by the SIGNED id as a decimal
   * string (`"174"`, `"-431"`) -- WITHOUT Elite Insights' `"d"` prefix.
   * Present exactly when `players[].damage_mods` is, i.e. only under
   * `{ modifiers: true }`; scoped to referenced ids, not the whole catalog.
   */
  damage_mod_map?: Record<string, DamageModDescOut>
}
