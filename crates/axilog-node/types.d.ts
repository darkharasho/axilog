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
  down_contribution: number
  downs_taken: number
  deaths: number
  damage_taken: number
  cc: CcOut
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
 * `missiles: true`). Not yet exposed via this Node SDK's `ParseOptions`
 * (see that interface's doc comment) -- declared here so the type surface
 * matches the schema `axilog_schema::Report` can actually produce.
 */
export interface MissilesOut {
  players: PlayerMissilesOut[]
  squad: SquadMissilesOut
}

/**
 * axilog's native report shape (`axilog_schema::Report`), as returned by
 * `parseFile`/`parseBuffer`. `schema_version` is currently always `"0.1"`.
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
   * Opt-in missile (projectile) analytics block (M10, Task 2). Not yet
   * requestable through this Node SDK (`ParseOptions` has no `missiles`
   * flag) -- declared `?` here purely so the type surface matches what
   * `axilog_schema::Report` can produce; always omitted (`undefined`) in
   * practice until the SDK grows that flag.
   */
  missiles?: MissilesOut
}
