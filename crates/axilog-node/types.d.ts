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

/**
 * One `(player, enemy)` pair's offensive split -- mirrors
 * `axilog_schema::PerTargetStatsOut` / EI's `statsTargets[i][0]`.
 * `connected_hits`/`against_downed_count` are the actor-only hit-quality
 * counts restricted to that enemy; `downed`/`killed` are minion-inclusive
 * last-hit attributions, matching GW2EI. `downs_contribution_damage` is the
 * arcdps-methodology per-target down-contribution, NOT EI's own
 * 90%-to-downstate-window algorithm.
 */
export interface PerTargetStatsOut {
  enemy_id: number
  connected_hits: number
  connected_damage: number
  against_downed_count: number
  downed: number
  killed: number
  interrupts: number
  downs_contribution_damage: number
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
  /**
   * The non-condition half of `damage` (MEIGAP Task 2a), GW2EI's
   * `targetPowerDamage1S` -- same buckets, same cumulative shape,
   * element-wise `<= damage`. "Power" is GW2EI's `DamageType.Power`: every
   * row whose skill id is NOT in its Condition buff catalog, i.e. strike
   * damage AND life-leech AND the non-catalogued `buff == 1` bucket.
   */
  power_damage: number[]
}

/**
 * A player's per-second detail block (M12, Task 2), opt-in -- see
 * `PlayerOut.per_second`. `damage`/`damage_taken`/every `per_target[].damage`
 * are CUMULATIVE running totals, one entry per second, bucketed the same way
 * GW2EI itself buckets them (`InterpolatedGraph`: `durationInS + 2` slots
 * when the log is not a whole number of seconds, `+ 1` when it is; bucket
 * index `ceil((t - logStart) / 1000)`) -- mirrors GW2EI's
 * `damage1S`/`damageTaken1S`/`targetDamage1S` cumulative (not
 * instant-delta) shape. NOTE: as of MEIGAP Task 2 this grid is no longer
 * the same one `Report.timeline` uses; that one keeps its own
 * floor-bucketed `duration/1000 + 1` scheme.
 */
export interface PlayerPerSecondOut {
  damage: number[]
  damage_taken: number[]
  /**
   * The non-condition half of `damage_taken` (MEIGAP Task 2a), GW2EI's
   * `powerDamageTaken1S` -- see `PlayerTargetSeriesOut.power_damage`.
   */
  power_damage_taken: number[]
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
  /**
   * WASTED counterparts (MSMALL item 2), identical scale: boon-time this
   * source generated that was destroyed before the target could spend it --
   * a stack overwritten at capacity, or stripped/cleansed with duration
   * left. GW2EI's `BuffStatistics.Wasted`.
   *
   * Rounded to 3 decimals (GW2EI's own `BuffDigit` precision, the most the
   * reference format carries) and OMITTED when exactly zero -- read them as
   * 0 when absent.
   */
  self_wasted?: number
  group_wasted?: number
  squad_wasted?: number
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
  /**
   * True total remaining duration (ms) of every boon counted by `strips`
   * (MEIGAP Task 3e). NOT the same number as EI's own
   * `support[0].boonStripsTime`, whose accumulator is buggy -- see the
   * Rust doc on `SupportMetrics::strips_duration_ms`.
   */
  strips_duration_ms: number
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
 * Aftercast/interrupt cast counters (MSMALL item 3). Always present,
 * same as `hit_stats`.
 *
 * Mirrors GW2EI's `JsonGameplayStatsAll` aftercast family, which lands in
 * its `statsAll[0]` as `saved`/`timeSaved`/`wasted`/`timeWasted`:
 * `saved_count` is casts that skipped their aftercast, `wasted_count` is
 * casts interrupted before firing. Durations are MILLISECONDS here (EI
 * emits seconds); `wasted_ms` is already the positive "time lost" figure.
 *
 * NOTE the name collision: `wasted_count`/`wasted_ms` are CAST-INTERRUPT
 * counters, unrelated to boon-generation waste. Both names are EI's.
 */
export interface AftercastOut {
  /** EI `statsAll[0].saved`: casts that skipped their aftercast. */
  saved_count: number
  /** EI `statsAll[0].timeSaved`, in MILLISECONDS (EI emits seconds). */
  saved_ms: number
  /** EI `statsAll[0].wasted`: casts interrupted before firing. */
  wasted_count: number
  /** EI `statsAll[0].timeWasted`, in MILLISECONDS (EI emits seconds). */
  wasted_ms: number
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
 *
 * `received_cc_count`/`received_cc_duration_ms` (ms) are the INCOMING
 * mirror of the outgoing `cc` block, and count CC from every source
 * (friendly included) with no pet/minion fold -- GW2EI's own two
 * asymmetries. `boon_strips_taken`/`boon_strips_taken_duration_ms` (ms) are
 * boons stripped OFF this player; the duration is the TRUE sum, not a
 * reproduction of GW2EI's own (verified buggy) `boonStripsTime`.
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
  received_cc_count: number
  received_cc_duration_ms: number
  boon_strips_taken: number
  boon_strips_taken_duration_ms: number
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
  /**
   * Guild GUID from `CBTS_GUILD` (MEIGAP Task 3c), uppercase
   * dash-separated. Omitted when the log carries no guild row for this
   * account.
   */
  guild_id?: string
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
  /**
   * Per-enemy offensive split (MEIGAP, Task 1d), sorted by `enemy_id` and
   * SPARSE (only enemies this player interacted with). Rides the same
   * `skillDamage: true` flag as `skill_damage` itself -- omitted entirely
   * unless requested; measured +56.5% rendered-HTML size when always-on.
   * The underlying pass always runs; only this serialized key is gated.
   */
  per_target?: PerTargetStatsOut[]
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
   * Aftercast/interrupt cast counters (MSMALL item 3). Always present,
   * same as `hit_stats`. See `AftercastOut`.
   */
  aftercast: AftercastOut
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
  /**
   * The enemy PLAYER's base profession / elite specialization, mirroring
   * `PlayerOut.profession`/`elite_spec`.
   *
   * Omitted for NPCs and gadgets, which have no profession at all — so
   * presence works as an "is this a real player" signal without also
   * reading `is_player`. Use these to group the enemy roster by CLASS: the
   * `name` field is the player's WvW RANK TITLE ("Mithril Scout"), not
   * their profession.
   */
  profession?: string
  /** See {@link EnemyOut.profession}. */
  elite_spec?: string
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

/* -------------------------------------------------------------------- */
/* Native format 1.0 (`axilog_schema::v1::ReportV1`), Task 12            */
/* -------------------------------------------------------------------- */

/**
 * Hand-transcribed types for `axilog_schema::v1::ReportV1`
 * (`crates/axilog-schema/src/v1/{mod.rs,envelope.rs,entities.rs,catalogs.rs,
 * series.rs,blocks/*.rs}`), the native output format 1.0 container.
 * `parseFile`/`parseBuffer` now return THIS shape, not the legacy `Report`
 * above (kept in place, unused by these two entry points as of Task 12, but
 * left untouched -- see this file's own top comment). `parseFileEi` is
 * unaffected and keeps returning the EI-compatibility JSON typed `any` in
 * `index.d.ts`.
 *
 * Same numeric-field convention as above: every Rust `u64`/`u32`/`f64`/`i32`
 * field is `number`, exact for every value this schema actually produces but
 * lossy in principle for a `u64` beyond 2^53.
 */

/** The FORMAT contract version plus the producing binary's version and the input file name. */
export interface AxilogMeta {
  /** The FORMAT contract version. Moves independently of `version`. Currently always `"1.0"`. */
  schema: string
  /** The binary that produced this document (`CARGO_PKG_VERSION`). */
  version: string
  /** The input log's file NAME, never a full path. Omitted when unknown (e.g. `parseBuffer`, which has no file name to offer). */
  generated_from?: string
}

/**
 * One `CBTS_MARKER` assignment, native 1.0 shape. `agent_addr` is always
 * present (arcdps does not restrict `CBTS_MARKER` to squad members, and many
 * carrying agents never become tracked entities); `entity_id` is present
 * only when the agent resolves to a roster entity.
 */
export interface MarkerAssignmentOutV1 {
  /** The entity this marker is on. Absent for agents that carry markers but are not tracked entities. */
  entity_id?: number
  agent_addr: number
  marker: string
  time_ms: number
}

/**
 * The 1.0 encounter envelope -- a reprojection of the legacy `EncounterOut`
 * with `markers[]` rekeyed from `agent_addr` to `entity_id`.
 */
export interface EncounterOutV1 {
  kind: string
  map: string
  duration_ms: number
  build: string
  revision: number
  /** The recording player, as an entity id. Omitted when the recorder does not resolve to an entity. */
  recorded_by?: number
  teams: TeamOut[]
  markers: MarkerAssignmentOutV1[]
  tick_rate?: TickRateOut
}

/**
 * What an entity IS -- squad member, non-squad friendly, enemy player, or
 * NPC/gadget. Declaration order is `entities[]`'s sort order.
 */
export type Role = 'squad' | 'friendly_player' | 'enemy_player' | 'npc'

export interface CommanderOut {
  variant: string
  guid: string
}

/**
 * One agent's IDENTITY only -- no statistics; those live in `blocks`, keyed
 * by `id`. The single place account and character names appear, so a PII
 * scrub is one pass rather than a hunt through nested structures.
 */
export interface EntityOut {
  /** Dense index into `entities[]`, from 0. Stable WITHIN a report only -- join across logs on `account`. */
  id: number
  role: Role
  /** Players only. */
  account?: string
  /** Players only. */
  character?: string
  /** Non-player entities only. */
  name?: string
  /** Present exactly for player roles. */
  profession?: string
  /** Empty string when the agent has no elite spec, or one this project cannot name. Never a numeric spec id. */
  elite_spec?: string
  team: string
  subgroup?: number
  commander?: CommanderOut
  guild_id?: string
  marker?: string
  /** The arcdps agent address -- a documented attribute, not a secret. */
  agent_addr: number
  instid?: number
  /**
   * Whether this entity interacted with the squad at all (dealt damage to
   * the squad, took damage from the squad, or took CC from the squad).
   * Always `true` for squad members and non-squad friendlies. Never
   * optional -- absence would be ambiguous between "did not participate"
   * and "not computed".
   */
  combat_participant: boolean
}

/** Definition metadata for one referenced skill id. */
export interface SkillEntry {
  name: string
  is_swap: boolean
  can_crit: boolean
  auto_attack?: boolean
}

/**
 * Definition metadata for one referenced buff id. `kind` is GW2's real
 * three-way taxonomy (arcdps does not distinguish these structurally).
 */
export interface BuffEntry {
  name: string
  kind: 'condition' | 'boon' | 'effect'
  stacking: 'intensity' | 'duration'
  max_stacks?: number
}

/** Definition metadata for one referenced damage-modifier id. */
export interface DamageModEntry {
  name: string
  kind: string
  approximate: boolean
}

/**
 * Definition metadata for every id any block references. No human-readable
 * name appears outside `catalogs` or `entities` -- every block references
 * integers instead. Keys serialize as decimal strings.
 */
export interface Catalogs {
  skills: Record<string, SkillEntry>
  buffs: Record<string, BuffEntry>
  /** Omitted entirely (not `{}`) when no damage-modifier id was referenced (e.g. `modifiers` was not requested). */
  damage_mods?: Record<string, DamageModEntry>
}

/**
 * One time series in the format's single series envelope. `enc` is `"raw"`
 * (a plain array of values) or `"rle"` (an array of `[value, run_length]`
 * pairs), chosen per series by whichever serializes smaller. `len` is the
 * DECODED length in both cases (NOT `data.length`), so a consumer can
 * allocate before decoding and validate after.
 */
export interface SeriesOut {
  interval_ms: number
  /** Decoded length, NOT `data.length`. */
  len: number
  enc: 'raw' | 'rle'
  data: unknown[]
}

/** Why a block is or is not present in `blocks`. */
/**
 * Why a block is or is not in `blocks`.
 *
 * - `present` -- computed, carried, at least one row.
 * - `not_computed` -- the compute gate was off. NOT the same as empty:
 *   it is a missing flag, not a fact about the log.
 * - `empty` -- computed and genuinely nothing to report. The block is
 *   still carried; only `not_computed`/`unsupported` omit it.
 * - `unsupported` -- RESERVED. No code path in this version emits it:
 *   nothing this container computes is era- or encounter-kind-gated.
 *   Named now so spec #2's era-gated surfaces can fill the slot without
 *   consumers having to learn a new value late. Handle it, but do not
 *   expect it from this version.
 */
export type CoverageState = 'present' | 'not_computed' | 'empty' | 'unsupported'

/**
 * Per-block computation/presence status, keyed by block name
 * (`"damage"`, `"defenses"`, `"hit_stats"`, `"cc"`, `"boons"`, `"support"`,
 * `"contribution"`, `"healing"`, `"rotation"`, `"damage_mods"`,
 * `"missiles"`, `"replay"`, `"series"`, `"conditions"`, `"minions"`).
 * Always names every known block, even ones this schema version never
 * computes (`"conditions"`/`"minions"`, reserved for spec #2, always
 * `"not_computed"`).
 */
export type Coverage = Record<string, CoverageState>

export type Severity = 'info' | 'warn' | 'error'

/**
 * A structured, user-facing analysis warning. `code` is a closed,
 * documented set: adding a code is additive, changing one's meaning is a
 * break.
 */
export interface WarningOut {
  code: string
  severity: Severity
  message: string
  /** The entity this warning is about, when it is about one. */
  entity_id?: number
}

/** The uniform entity-keyed map every block uses. Keys serialize as decimal strings. */
export type ByEntity<T> = Record<string, T>

/* --- damage block -------------------------------------------------------- */

/**
 * Aggregates `Role::Squad` entities ONLY -- not every friendly player (see
 * `by_entity`, which is the full roster).
 */
export interface DamageSquad {
  total: number
  dps: number
}

/**
 * Mirrors the legacy `SkillEntryOut` field-for-field. `hits`/`min`/`max`
 * count only CONTRIBUTING (`dmg > 0`) events. `crit_hits`/`flank_hits` are
 * hit COUNTS, not damage sums.
 */
export interface SkillRow {
  total: number
  hits: number
  min: number
  max: number
  crit_hits: number
  flank_hits: number
}

/**
 * Mirrors the legacy `PerTargetStatsOut` field-for-field, minus `enemy_id`
 * (the enclosing map's key here, as the target's ENTITY id). `interrupts`
 * and `downs_contribution_damage` are not derivable from any other block.
 */
export interface PerTargetDetail {
  connected_hits: number
  connected_damage: number
  against_downed_count: number
  downed: number
  killed: number
  interrupts: number
  /**
   * arcdps-methodology down-contribution DAMAGE for downs of this specific
   * target -- NOT GW2EI's 90%-to-downstate-window algorithm.
   */
  downs_contribution_damage: number
}

/**
 * One `(entity, target)` pair. `total` is ungated; `detail` and `by_skill`
 * come from the `skillDamage: true`-gated families, so a row can
 * legitimately carry `total` alone.
 */
export interface PerTarget {
  total: number
  /**
   * Grouped under one key rather than flattened so its absence has a
   * single unambiguous signal instead of seven fabricated zeros.
   */
  detail?: PerTargetDetail
  /** Per-(entity, target, skill) outgoing damage, keyed by skill id. */
  by_skill?: Record<string, SkillRow>
}

export interface DamageEntity {
  total: number
  dps: number
  taken: number
  /**
   * Enemy players this entity landed the DOWNING blow on. An outgoing
   * outcome; the incoming mirrors are `DefensesEntity.downs_taken`/
   * `deaths`, the same split GW2EI makes.
   */
  downs_dealt: number
  /** Enemy players this entity landed the KILLING blow on. */
  kills_dealt: number
  /**
   * Keyed by the TARGET's entity id. Sparse; omitted when empty. A UNION of
   * three legacy per-enemy families, so a target can appear with `detail`
   * and no damage `total`.
   */
  per_target?: Record<string, PerTarget>
  /** OUTGOING per-skill damage, keyed by skill id. Present only when the per-skill compute gate (`skillDamage: true`) was on. */
  by_skill?: Record<string, SkillRow>
  /** INCOMING per-skill damage, keyed by skill id -- mirroring this row's own `total`/`taken` pair. Same gate as `by_skill`. */
  by_skill_taken?: Record<string, SkillRow>
}

export interface DamageBlock {
  squad: DamageSquad
  by_entity: ByEntity<DamageEntity>
}

/* --- defenses / hit_stats / cc blocks ------------------------------------ */

/** Incoming defenses. Mirrors the legacy `DefensesOut` field-for-field. */
export interface DefensesEntity {
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
  /** Incoming crowd control. */
  received_cc_count: number
  received_cc_duration_ms: number
  /** Boons stripped OFF this player -- the incoming counterpart of `SupportEntity.strips`. */
  boon_strips_taken: number
  boon_strips_taken_duration_ms: number
  /**
   * Times this entity entered downstate -- GW2EI's
   * `defenses[0].downCount`. The outgoing mirrors live on
   * `DamageEntity.downs_dealt`/`kills_dealt`.
   */
  downs_taken: number
  /** Times this entity died -- GW2EI's `defenses[0].deadCount`. */
  deaths: number
}

export interface DefensesBlock {
  by_entity: ByEntity<DefensesEntity>
}

/**
 * Outgoing hit quality. Mirrors the legacy `HitStatsOut` field-for-field.
 * `above90_*` counts/damage are against a target at or above 90% health.
 */
export interface HitStatsEntity {
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

export interface HitStatsBlock {
  by_entity: ByEntity<HitStatsEntity>
}

/** Aggregates `Role::Squad` entities ONLY. */
export interface CcSquad {
  applied_total: number
  applied_duration_ms: number
}

/**
 * Mirrors the legacy `CcOut` field-for-field. Incoming CC lives on
 * `DefensesEntity.received_cc_count`/`received_cc_duration_ms` instead.
 */
export interface CcEntity {
  applied_total: number
  applied_duration_ms: number
  stun_breaks: number
  removed_stun_duration_ms: number
}

export interface CcBlock {
  squad: CcSquad
  by_entity: ByEntity<CcEntity>
}

/* --- boons / support / contribution / healing blocks --------------------- */

/**
 * Mirrors the legacy `GenerationOut` field-for-field. WASTED fields are
 * rounded to 3 decimals and omitted when exactly zero -- read them as 0
 * when absent.
 */
export interface GenerationRow {
  self_pct: number
  group_pct: number
  squad_pct: number
  self_wasted?: number
  group_wasted?: number
  squad_wasted?: number
}

/** Mirrors the legacy `BoonOut` field-for-field, minus `id` (the map key) and `name` (resolve via `catalogs.buffs`). */
export interface BoonRow {
  uptime_pct: number
  avg_stacks?: number
  generation: GenerationRow
}

/** entity id -> buff id -> row. Two levels of real ids, no positional joins. */
export interface BoonsBlock {
  by_entity: ByEntity<Record<string, BoonRow>>
}

/** Mirrors the legacy `SupportOut` field-for-field. */
export interface SupportEntity {
  cleanses: number
  cleanses_self: number
  strips: number
  strips_duration_ms: number
  resurrects: number
}

export interface SupportBlock {
  by_entity: ByEntity<SupportEntity>
}

/** Mirrors the legacy `ContributionOut` field-for-field. */
export interface ContributionRow {
  damage: number
  cc: number
  strips: number
  movement_impairing: number
}

/** Both directions of the arcdps-methodology down contribution. */
export interface ContributionEntity {
  downs_contribution: ContributionRow
  downed_by: ContributionRow
}

export interface ContributionBlock {
  by_entity: ByEntity<ContributionEntity>
}

/** Mirrors the legacy `HealingOut` field-for-field. */
export interface HealingEntity {
  outgoing_total: number
  outgoing_allies: number
  outgoing_self: number
  barrier_out: number
  downed_healing_out: number
}

export interface HealingBlock {
  by_entity: ByEntity<HealingEntity>
}

/* --- rotation / damage_mods / missiles / replay / series blocks ---------- */

/**
 * One cast. Mirrors the legacy `CastOut` field-for-field, plus `skill_id`
 * hoisted from the enclosing skill grouping so this is a flat,
 * time-ordered cast list per entity.
 */
export interface CastRow {
  skill_id: number
  cast_time_ms: number
  duration_ms: number
  /** Negative when the cast was interrupted/cancelled early. */
  time_gained_ms: number
  quickness: number
}

/**
 * Mirrors the legacy `AftercastOut`. Durations are MILLISECONDS (GW2EI
 * emits the same quantities as seconds).
 *
 * NOTE the name collision GW2EI bequeathed: `wasted_count` is a
 * CAST-INTERRUPT count, unrelated to the boon-generation `*_wasted` fields
 * under `boons`.
 */
export interface Aftercast {
  /** Casts that skipped their aftercast. */
  saved_count: number
  saved_ms: number
  /** Casts interrupted before firing. */
  wasted_count: number
  /** Already the positive "time lost" figure. */
  wasted_ms: number
}

export interface RotationEntity {
  cast_count: number
  casts: CastRow[]
  /**
   * Cast counters, computed unconditionally but published only when this
   * block is (gated on `rotation: true`).
   */
  aftercast: Aftercast
}

export interface RotationBlock {
  by_entity: ByEntity<RotationEntity>
}

/**
 * One damage-modifier row. `id` (the map key) is SIGNED -- negative means
 * incoming -- so one map naturally separates outgoing from incoming.
 */
export interface DamageModRow {
  hit_count: number
  total_hit_count: number
  damage_gain: number
  total_damage: number
}

export interface DamageModsBlock {
  /** Keyed by the signed damage-modifier id as a decimal string. */
  by_entity: ByEntity<Record<string, DamageModRow>>
}

export interface MissilesSquad {
  fired: number
  hit: number
  denied: number
  incoming_fired: number
  incoming_denied: number
}

/** Mirrors the legacy `PlayerMissilesOut`, minus `agent_addr`/`account` (identity already lives on this row's own `entities[]` entry). */
export interface MissilesEntity {
  fired: number
  hit: number
  denied: number
  reflected_at_self: number
}

/**
 * `squad` is REQUIRED, like every other squad aggregate in this format
 * (`damage`, `cc`, `series`): when the block is present, so is its
 * aggregate.
 */
export interface MissilesBlock {
  squad: MissilesSquad
  by_entity: ByEntity<MissilesEntity>
}

export interface ReplayBounds {
  min_x: number
  min_y: number
  max_x: number
  max_y: number
}

/**
 * One entity's replay track. `samples` are `[t_ms, x, y]` triples;
 * `down_intervals`/`dead_intervals` are `[start_ms, end_ms]` pairs.
 * `name`/`team`/`commander`/`is_squad` are dropped versus the legacy
 * `ReplayTrackOut` -- they live on this entity's own `entities[]` row.
 */
export interface ReplayTrack {
  samples: [number, number, number][]
  down_intervals: [number, number][]
  dead_intervals: [number, number][]
}

export interface ReplayBlock {
  /** Shared polling interval for every track. */
  poll_ms: number
  bounds?: ReplayBounds
  /** Keyed by entity id. */
  by_entity: ByEntity<ReplayTrack>
}

export interface SquadSeries {
  damage: SeriesOut
  cc_applied: SeriesOut
  downs: SeriesOut
}

/** Mirrors the legacy `PlayerTargetSeriesOut`, minus `enemy_id` (that's the map key here, joined by entity id). */
export interface TargetSeries {
  damage: SeriesOut
  power_damage: SeriesOut
}

/** Mirrors the legacy `PlayerPerSecondOut` field-for-field. `per_target` is keyed by the TARGET's entity id. */
export interface EntitySeries {
  damage: SeriesOut
  damage_taken: SeriesOut
  power_damage_taken: SeriesOut
  per_target?: Record<string, TargetSeries>
}

/**
 * `squad` is REQUIRED (see `MissilesBlock`). The squad series is computed
 * unconditionally; only `by_entity` needs `timeseries: true`.
 */
export interface SeriesBlock {
  squad: SquadSeries
  by_entity: ByEntity<EntitySeries>
}

/**
 * Every statistic block. A block is omitted entirely when `coverage` says
 * `not_computed`/`unsupported`; an `empty` block is still carried, so a
 * consumer can tell "computed and there was nothing" from "never ran".
 */
export interface Blocks {
  damage?: DamageBlock
  defenses?: DefensesBlock
  hit_stats?: HitStatsBlock
  cc?: CcBlock
  boons?: BoonsBlock
  support?: SupportBlock
  contribution?: ContributionBlock
  healing?: HealingBlock
  rotation?: RotationBlock
  damage_mods?: DamageModsBlock
  missiles?: MissilesBlock
  replay?: ReplayBlock
  series?: SeriesBlock
}

/**
 * axilog's native output format 1.0 container (`axilog_schema::v1::
 * ReportV1`), as returned by `parseFile`/`parseBuffer` (Task 12).
 */
export interface ReportV1 {
  axilog: AxilogMeta
  encounter: EncounterOutV1
  entities: EntityOut[]
  catalogs: Catalogs
  blocks: Blocks
  coverage: Coverage
  /** Structured, user-facing analysis warnings. Omitted entirely (not `[]`) when there are none. */
  warnings?: WarningOut[]
}
