"""Hand-maintained typing stub for axilog's PyO3 extension module (M6 Task 2).

`axilog` (this crate) has no `python-source` tree -- the compiled
extension module IS the package (see `src/lib.rs`'s `#[pymodule] fn
axilog`). maturin (>=1.5) auto-detects a `<module_name>.pyi` file and a
`py.typed` marker sitting next to `Cargo.toml`/`pyproject.toml` at the
crate root and bundles both into the wheel as `axilog/__init__.pyi` and
`axilog/py.typed` (verified by running `maturin build` against this exact
crate: it logs "Found type stub file at axilog.pyi" and the built wheel
contains `axilog/__init__.pyi` + `axilog/py.typed` alongside
`axilog/axilog.abi3.so` and the auto-generated `axilog/__init__.py` --
no `[tool.maturin]` include/package-data config needed). Because a
package's `__init__.pyi`, when present, is authoritative for static
analysis of that package regardless of what `__init__.py` actually does
at runtime (PEP 561), this single file fully types `import axilog;
axilog.parse_file(...)` etc. without needing to mirror the
`from .axilog import *` re-export at the type level.

Every `TypedDict` below transcribes one `#[derive(Serialize)] struct` from
`crates/axilog-schema/src/lib.rs` (the source of truth -- keep this file
in sync with that one if the schema changes; cf.
`crates/axilog-node/types.d.ts`, which does the same transcription for the
Node SDK and was cross-checked field-by-field against the same source
while writing this file). Each `TypedDict` is split into a `_XRequired`
base plus a `total=False` subclass so PEP 589's required/optional
distinction can express serde's actual behavior without `NotRequired`
(unavailable before Python 3.11, and this crate targets `>=3.9`):

- A plain (non-`Option`) field, or an `Option<T>` field with no
  `#[serde(skip_serializing_if = ...)]`, is always present in the JSON
  (`null` for `None`) -- modeled as a required key, typed `Optional[T]`
  only in the `Option<T>`-without-skip case (currently just
  `EncounterOut.recorded_by`).
- An `Option<T>` field with `#[serde(skip_serializing_if =
  "Option::is_none")]` is *omitted* from the JSON entirely when `None` --
  modeled as an optional (`total=False`) key, typed as plain `T` (its
  absence, not a `None` value, is what `Option` means here).
- `Report.warnings` (`Vec<String>` with `skip_serializing_if =
  "Vec::is_empty"`) is likewise an optional key, typed as `List[str]`,
  omitted rather than `[]` when there are no warnings.

`parse_file_ei`'s return value (the `axilog_ei::to_ei_json`
Elite-Insights-compatibility shape) is a materially different, larger,
partially-dynamic JSON structure and is intentionally left `Dict[str,
Any]` here -- typing it faithfully is out of scope for this task, mirrors
`axilog-node/types.d.ts`'s same call for `parseFileEi`.
"""

from typing import Any, Dict, List, Optional, TypedDict

__all__ = [
    "parse_file",
    "parse_bytes",
    "parse_file_ei",
    "anonymize_file",
    "Report",
    "EncounterOut",
    "MarkerAssignmentOut",
    "TickRateOut",
    "CommanderTagOut",
    "TeamOut",
    "DamageOut",
    "PerEnemyOut",
    "CcOut",
    "GenerationOut",
    "BoonOut",
    "SupportOut",
    "PlayerOut",
    "EnemyOut",
    "TimelineOut",
    "PerSecondOut",
    "ReplayBoundsOut",
    "ReplayTrackOut",
    "ReplayOut",
    "HealingOut",
    "PlayerMissilesOut",
    "SquadMissilesOut",
    "MissilesOut",
    "SkillMapEntryOut",
    "CastOut",
    "SkillRotationOut",
]

# --- encounter / metadata ---------------------------------------------

class MarkerAssignmentOut(TypedDict):
    """One `CBTS_MARKER` assignment (native-only, no EI equivalent)."""

    agent_addr: int
    marker: str
    time_ms: int

class TickRateOut(TypedDict):
    """`CBTS_TICK` tick-rate telemetry (native-only)."""

    avg: float
    min: float
    per_second: List[float]

class CommanderTagOut(TypedDict):
    variant: str
    guid: str

class _TeamOutRequired(TypedDict):
    color: str
    team_id: int

class TeamOut(_TeamOutRequired, total=False):
    """`guid` is omitted (not `null`) when this team has no known content GUID."""

    guid: str

class _EncounterOutRequired(TypedDict):
    kind: str
    map: str
    duration_ms: int
    build: str
    revision: int
    # `Option<String>` with no `skip_serializing_if` -> always present, `null` when unknown.
    recorded_by: Optional[str]
    teams: List[TeamOut]
    markers: List[MarkerAssignmentOut]

class EncounterOut(_EncounterOutRequired, total=False):
    """`tick_rate` is omitted when the log has fewer than two `CBTS_TICK` events."""

    tick_rate: TickRateOut

# --- damage / cc --------------------------------------------------------

class PerEnemyOut(TypedDict):
    enemy_id: int
    total: int

class PerTargetStatsOut(TypedDict):
    """One `(player, enemy)` pair's offensive split -- mirrors
    `axilog_schema::PerTargetStatsOut` / EI's `statsTargets[i][0]`.
    `connected_hits`/`against_downed_count` are the actor-only hit-quality
    counts restricted to that enemy; `downed`/`killed` are minion-inclusive
    last-hit attributions, matching GW2EI. `downs_contribution_damage` is
    the arcdps-methodology per-target down-contribution, NOT EI's own
    90%-to-downstate-window algorithm."""

    enemy_id: int
    connected_hits: int
    connected_damage: int
    against_downed_count: int
    downed: int
    killed: int
    interrupts: int
    downs_contribution_damage: int

class DamageOut(TypedDict):
    total: int
    dps: float
    per_enemy: List[PerEnemyOut]

class CcOut(TypedDict):
    applied_total: int
    applied_duration_ms: int
    stun_breaks: int
    removed_stun_duration_ms: int

# --- per-skill damage distribution (M12, Task 1) ---------------------------

class SkillEntryOut(TypedDict):
    """One skill id's aggregated hit stats within some grouping. `hits`/
    `min`/`max` count only CONTRIBUTING (`dmg > 0`) events -- a deliberate
    divergence from GW2EI's own `totalDamageDist[].hits` (which also counts
    0-damage missed/blocked/invulned/evaded attempts). `crit_hits`/
    `flank_hits` are hit COUNTS (not damage sums)."""

    skill_id: int
    total: int
    hits: int
    min: int
    max: int
    crit_hits: int
    flank_hits: int

class PerTargetSkillsOut(TypedDict):
    """One enemy's per-skill outgoing breakdown -- explicit `enemy_id`, not positional."""

    enemy_id: int
    skills: List[SkillEntryOut]

class SkillDamageOut(TypedDict):
    """Per-skill damage distribution: outgoing (total + per-target) and
    incoming, each grouped by skill id. `sum(outgoing[*]["total"])` ==
    `DamageOut["total"]` and `sum(taken[*]["total"])` ==
    `PlayerOut["damage_taken"]` hold exactly by construction. Pet/minion
    damage is folded onto the owner here (using the pet's own skill id),
    matching `DamageOut["total"]`'s own pet-fold -- unlike GW2EI's
    `totalDamageDist`, which tracks the player actor only and excludes
    pet/minion damage entirely."""

    outgoing: List[SkillEntryOut]
    taken: List[SkillEntryOut]
    per_target: List[PerTargetSkillsOut]

# --- per-player per-second series + dpsTargets (M12, Task 2) ---------------

class PlayerTargetSeriesOut(TypedDict):
    """One enemy's cumulative per-second outgoing-damage series."""

    enemy_id: int
    damage: List[int]
    power_damage: List[int]
    """The non-condition half of `damage` (MEIGAP Task 2a), GW2EI's
    `targetPowerDamage1S` -- same buckets, same cumulative shape,
    element-wise `<= damage`. "Power" is GW2EI's `DamageType.Power`: every
    row whose skill id is NOT in its Condition buff catalog, i.e. strike
    damage AND life-leech AND the non-catalogued `buff == 1` bucket."""

class PlayerPerSecondOut(TypedDict):
    """A player's per-second detail block, opt-in -- see
    `PlayerOut["per_second"]`. `damage`/`damage_taken`/every
    `per_target[]["damage"]` are CUMULATIVE running totals, one entry per
    second, bucketed the way GW2EI itself buckets them (`InterpolatedGraph`:
    `durationInS + 2` slots when the log is not a whole number of seconds,
    `+ 1` when it is; bucket index `ceil((t - logStart) / 1000)`) -- mirrors
    GW2EI's `damage1S`/`damageTaken1S`/`targetDamage1S` cumulative (not
    instant-delta) shape. NOTE: as of MEIGAP Task 2 this grid is no longer
    the same one `Report["timeline"]` uses; that one keeps its own
    floor-bucketed `duration/1000 + 1` scheme."""

    damage: List[int]
    damage_taken: List[int]
    power_damage_taken: List[int]
    """The non-condition half of `damage_taken` (MEIGAP Task 2a), GW2EI's
    `powerDamageTaken1S` -- see `PlayerTargetSeriesOut["power_damage"]`."""
    per_target: List[PlayerTargetSeriesOut]

class DpsTargetOut(TypedDict):
    """One enemy's whole-fight dps/damage summary -- see `PlayerOut["dps_targets"]`."""

    enemy_id: int
    damage: int
    dps: float

# --- boons / support ------------------------------------------------------

class GenerationOut(TypedDict):
    """Self/group/squad boon-generation attribution, 0-100 scale."""

    self_pct: float
    group_pct: float
    squad_pct: float

class _BoonOutRequired(TypedDict):
    id: int
    name: str
    presence_pct: float
    generation: GenerationOut

class BoonOut(_BoonOutRequired, total=False):
    """`avg_stacks` is present only for the two intensity-type boons
    (Might, Stability); omitted (not a meaningless 0) for the other 10
    duration-type boons."""

    avg_stacks: float

class SupportOut(TypedDict):
    cleanses: int
    cleanses_self: int
    strips: int
    #: True total remaining duration (ms) of every boon counted by
    #: ``strips`` (MEIGAP Task 3e). NOT EI's own ``boonStripsTime``, whose
    #: accumulator is buggy -- see ``SupportMetrics::strips_duration_ms``.
    strips_duration_ms: int
    resurrects: int

# --- contribution family (M11, Task 2) --------------------------------------

class ContributionOut(TypedDict):
    """The arcdps-methodology contribution family's four stats -- used for
    both `PlayerOut.downs_contribution` (outgoing: this player's own credit
    toward downing enemy players) and `PlayerOut.downed_by` (incoming: what
    non-squad contributors did to THIS player before each of their own
    downs, aggregated onto this row, not broken down by attacker). Replaces
    the retired M1-era `down_contribution` 10s-window approximation (schema
    0.1 -> 0.2)."""

    damage: int
    cc: int
    strips: int
    movement_impairing: int

# --- healing (M10, Task 1) -------------------------------------------------

class HealingOut(TypedDict):
    """arcdps healing-extension totals -- outgoing healing/barrier scalars,
    mirroring EI's `extHealingStats`/`extBarrierStats`. `healing_out_allies`
    is `healing_out_total - healing_out_self`."""

    healing_out_total: int
    healing_out_allies: int
    healing_out_self: int
    barrier_out: int
    downed_healing_out: int

# --- hit-quality stats (M13, Task 1) ---------------------------------------

class AftercastOut(TypedDict):
    """Aftercast/interrupt cast counters (MSMALL item 3). Always present,
    like `hit_stats`.

    Mirrors GW2EI's `JsonGameplayStatsAll` aftercast family, which lands in
    its `statsAll[0]` as `saved`/`timeSaved`/`wasted`/`timeWasted`:
    `saved_count` is casts that skipped their aftercast, `wasted_count` is
    casts interrupted before firing. Durations are MILLISECONDS here (EI
    emits seconds); `wasted_ms` is already the positive "time lost" figure.

    NOTE the name collision: `wasted_count`/`wasted_ms` are CAST-INTERRUPT
    counters, unrelated to boon-generation waste. Both names are EI's.
    """

    saved_count: int
    saved_ms: int
    wasted_count: int
    wasted_ms: int

class HitStatsOut(TypedDict):
    """Outgoing hit-quality stats -- mirrors EI's `statsAll[0]`.
    `against_downed_*`/`above90_*` are plain per-event wire flags (NOT
    down-interval/health-tracker state); this block deliberately does NOT
    fold pet/minion damage onto the owner (unlike `DamageOut`/
    `SkillDamageOut`) -- EI's own `statsAll[0]` is actor-only. Always
    present (not gated), like `boons`/`support`."""

    crit_count: int
    crit_damage: int
    flank_count: int
    glance_count: int
    moving_count: int
    connected_count: int
    connected_damage: int
    direct_count: int
    direct_damage: int
    condition_count: int
    condition_damage: int
    critable_direct_count: int
    against_downed_count: int
    against_downed_damage: int
    life_leech_count: int
    life_leech_damage: int
    above90_power_count: int
    above90_power_damage: int
    above90_condition_count: int
    above90_condition_damage: int

# --- incoming defenses (M13, Task 2) ----------------------------------------

class DefensesOut(TypedDict):
    """Incoming defenses: hit-outcome counts + damage-taken breakdown --
    mirrors EI's `defenses[0]`. `dodge_count` is NOT derived from any
    incoming event (a self-cast dodge-skill count, independent of
    `evaded_count`); `power_count`/`power_damage` always equal
    `strike_count`/`strike_damage` + `life_leech_count`/`life_leech_damage`;
    `life_leech_count`/`life_leech_damage` are the TRUE values (a real GW2EI
    counting bug in its own `lifeLeechDamageTakenCount` is deliberately not
    reproduced). Purely additive alongside `downs_taken`/`deaths`/
    `damage_taken`/`cc`. Always present (not gated), like `hit_stats`.

    `received_cc_count`/`received_cc_duration_ms` (ms) are the INCOMING
    mirror of the outgoing `cc` block, and count CC from every source
    (friendly included) with no pet/minion fold -- GW2EI's own two
    asymmetries. `boon_strips_taken`/`boon_strips_taken_duration_ms` (ms)
    are boons stripped OFF this player; the duration is the TRUE sum, not a
    reproduction of GW2EI's own (verified buggy) `boonStripsTime`."""

    blocked_count: int
    evaded_count: int
    dodge_count: int
    missed_count: int
    interrupted_count: int
    invulned_count: int
    strike_count: int
    strike_damage: int
    power_count: int
    power_damage: int
    condition_count: int
    condition_damage: int
    life_leech_count: int
    life_leech_damage: int
    barrier_count: int
    barrier_damage: int
    breakbar_count: int
    breakbar_damage: int
    received_cc_count: int
    received_cc_duration_ms: int
    boon_strips_taken: int
    boon_strips_taken_duration_ms: int

# --- skillMap (M14, Task 2) ------------------------------------------------

class _SkillMapEntryOutRequired(TypedDict):
    """One referenced skill id's best-effort metadata -- mirrors
    `axilog_schema::SkillMapEntryOut` / EI's `skillMap` values. `name` is a
    log-table best-effort (falls back to `"Skill <id>"`); `can_crit` reuses
    M13's NonCritableSkills; `is_swap` marks weapon/attunement/legend/shroud
    swap sentinel ids. Names are deliberately log-table-only -- EI's richer
    embedded DB names + `icon` URLs are a documented, out-of-scope gap."""

    name: str
    is_swap: bool
    can_crit: bool

class SkillMapEntryOut(_SkillMapEntryOutRequired, total=False):
    """`auto_attack` is omitted (not `null`) when unknown -- currently ALWAYS
    omitted (the heuristic is refused rather than guessed, per
    `axilog_core::analysis::skill_map`'s doc comment)."""

    auto_attack: bool

# --- rotation / casts (M14, Task 1) ----------------------------------------

class CastOut(TypedDict):
    """One recorded cast -- mirrors GW2EI's `JsonRotation.JsonSkill`.
    `cast_time_ms` is relative to log start (may be negative = pre-log cast);
    `quickness` is the sped/slowed fraction (negative = quickness-hasted)."""

    cast_time_ms: int
    duration_ms: int
    time_gained_ms: int
    quickness: float

class SkillRotationOut(TypedDict):
    """All recorded casts of one skill id, for one player -- see
    `PlayerOut["rotation"]`."""

    skill_id: int
    casts: List[CastOut]

# --- damage modifiers (M16) ------------------------------------------------

class DamageModEntryOut(TypedDict):
    """One `(player, damage modifier)` row -- GW2EI's
    `JsonDamageModifierItem`, plus the modifier id, flattened (this project
    does not model phases, so EI's per-phase nesting collapses to one row).

    `id` is SIGNED: negative means an incoming (damage-taken) modifier, and
    it is the key into `Report["damage_mod_map"]`. `damage_gain` is
    `sum(gain * damage)` rounded to 3 decimals -- for a `skill_based` or
    `is_counter` modifier it is instead the raw damage done under the
    effect, exactly as EI documents its own field."""

    id: int
    hit_count: int
    total_hit_count: int
    damage_gain: float
    total_damage: int

class DamageModsOut(TypedDict):
    """A player's damage-modifier block (M16), split by direction the way
    EI's `damageModifiers`/`incomingDamageModifiers` are. Only modifiers
    with at least one qualifying hit appear (EI's own emission rule), so an
    absent id means "never triggered", not "zero"."""

    outgoing: List[DamageModEntryOut]
    incoming: List[DamageModEntryOut]

class DamageModDescOut(TypedDict):
    """One `Report["damage_mod_map"]` entry -- GW2EI's `DamageModDesc`,
    field for field. `description` is EI's full tooltip: the catalogued
    text plus the derived `<br>Applied on ...`/`<br>Compared against ...`/
    `<br>Counter`/`<br>Non multiplier`/`<br>Approximate` suffixes."""

    name: str
    icon: str
    description: str
    non_multiplier: bool
    is_counter: bool
    skill_based: bool
    approximate: bool
    incoming: bool

# --- players / enemies -----------------------------------------------------

class _PlayerOutRequired(TypedDict):
    account: str
    character: str
    profession: str
    elite_spec: str
    team: str
    subgroup: int
    in_squad: bool
    commander: bool
    damage: DamageOut
    downs_dealt: int
    kills_dealt: int
    downs_taken: int
    deaths: int
    damage_taken: int
    cc: CcOut
    downs_contribution: ContributionOut
    downed_by: ContributionOut
    boons: List[BoonOut]
    support: SupportOut
    hit_stats: HitStatsOut
    aftercast: AftercastOut
    defenses: DefensesOut

class PlayerOut(_PlayerOutRequired, total=False):
    """`marker`/`commander_tag` are omitted (not `null`) when absent.
    `healing` is omitted entirely (not a `null`/all-zero dict) when the log
    carries no healing-extension data at all -- a real "no data" signal, not
    "the player never healed". `skill_damage` (M12, Task 1) is opt-in like
    `Report["replay"]`/`Report["missiles"]` -- omitted unless requested via
    `skill_damage=True` (see `parse_file`/`parse_bytes`); measured +249%
    native JSON size on the committed fixture when always-on, hence opt-in
    rather than always-present like `boons`/`support`. `per_second`/
    `dps_targets` (M12, Task 2) are BOTH gated by the SAME `timeseries=True`
    flag -- omitted unless requested; measured +147.7%/+36.4% native JSON
    size respectively when always-on (a real WvW log can enumerate dozens
    of enemies per player, so `dps_targets` is not small enough to stay
    always-on the way `boons`/`support` are). `rotation` (M14, Task 1) is
    opt-in like `skill_damage`/`per_second`/`dps_targets` -- omitted unless
    requested via `rotation=True` (see `parse_file`/`parse_bytes`); measured
    +66.9% native JSON size on the committed fixture when always-on. The
    underlying `PlayerMetrics.rotation` is ALWAYS computed (so `--view
    rotation` works flag-free); only this serialized key is gated.
    `per_target` (MEIGAP, Task 1d) rides the SAME `skill_damage=True` flag
    as `skill_damage` itself (it is the other per-target family) -- omitted
    unless requested; measured +56.5% rendered-HTML size when always-on.
    Like `rotation`, the underlying pass always runs; only the serialized
    key is gated."""

    per_target: List[PerTargetStatsOut]
    marker: str
    commander_tag: CommanderTagOut
    #: Guild GUID from ``CBTS_GUILD`` (MEIGAP Task 3c), uppercase
    #: dash-separated. Omitted when the log has no guild row for this
    #: account.
    guild_id: str
    healing: HealingOut
    skill_damage: SkillDamageOut
    per_second: PlayerPerSecondOut
    dps_targets: List[DpsTargetOut]
    rotation: List[SkillRotationOut]
    damage_mods: DamageModsOut

class _EnemyOutRequired(TypedDict):
    id: int
    name: str
    team: str
    is_player: bool

class EnemyOut(_EnemyOutRequired, total=False):
    """`marker` is omitted (not `null`) when absent, mirroring `PlayerOut.marker`."""

    marker: str

# --- timeline -----------------------------------------------------------

class PerSecondOut(TypedDict):
    squad_damage: List[int]
    cc_applied: List[int]
    downs: List[int]

class TimelineOut(TypedDict):
    resolution_ms: int
    per_second: PerSecondOut

# --- combat replay (M9, Task 2) --------------------------------------------

class ReplayBoundsOut(TypedDict):
    """Min/max `x`/`y` observed across every `ReplayOut.tracks[].samples`."""

    min_x: float
    min_y: float
    max_x: float
    max_y: float

class ReplayTrackOut(TypedDict):
    """One tracked agent's combat-replay track. `name`/`team` mirror the
    display-field precedence used elsewhere (`PlayerOut.character` for squad
    players, `EnemyOut.name` for enemy-player representatives). `samples`
    are `[t_ms, x, y]` triples (`x`/`y` rounded to 1 decimal place);
    `down_intervals`/`dead_intervals` are `[start_ms, end_ms]` pairs."""

    name: str
    team: str
    commander: bool
    is_squad: bool
    samples: List[List[float]]
    down_intervals: List[List[int]]
    dead_intervals: List[List[int]]

class ReplayOut(TypedDict):
    """Combat-replay position tracks, native-only -- present only when
    `replay=True` was passed to `parse_file`/`parse_bytes`."""

    poll_ms: int
    bounds: ReplayBoundsOut
    tracks: List[ReplayTrackOut]

# --- missiles (M10, Task 2) ------------------------------------------------

class PlayerMissilesOut(TypedDict):
    """One squad player's missile totals, account-folded across
    relog/build-swap addrs like every other per-player metric. `account`
    (final-review fix wave) is the join key back to `Report["players"]` --
    `agent_addr` alone isn't exposed anywhere else in the native JSON. See
    `axilog_core::analysis::missiles`'s module doc for exactly what
    `fired`/`hit`/`denied`/`reflected_at_self` are (and are NOT)
    attributable to -- `denied` deliberately does not distinguish
    blocked/reflected/destroyed/expired outcomes, and `reflected_at_self`
    is an explicitly labeled heuristic, not a certainty."""

    agent_addr: int
    account: str
    fired: int
    hit: int
    denied: int
    reflected_at_self: int

class SquadMissilesOut(TypedDict):
    """Squad-wide missile totals -- the sum of every `PlayerMissilesOut`
    entry, plus the aggregate, unattributed "incoming, denied" defensive
    rollup (no per-player credit exists for who denied an incoming
    missile)."""

    fired: int
    hit: int
    denied: int
    incoming_fired: int
    incoming_denied: int

class MissilesOut(TypedDict):
    """Opt-in missile (projectile) analytics, native-only. Requestable via
    `parse_file`/`parse_bytes`'s `missiles=True` keyword arg (final-review
    fix wave)."""

    players: List[PlayerMissilesOut]
    squad: SquadMissilesOut

# --- top-level report -----------------------------------------------------

class _ReportRequired(TypedDict):
    schema_version: str
    axilog_version: str
    encounter: EncounterOut
    players: List[PlayerOut]
    enemies: List[EnemyOut]
    timeline: TimelineOut
    # skillMap (M14, Task 2): always present. Keyed by skill id as a string
    # (serde object-key stringification of the u32 key), e.g. `"5491"`.
    skill_map: Dict[str, SkillMapEntryOut]

class Report(_ReportRequired, total=False):
    """`warnings` is omitted (not `[]`) when there are no analysis warnings.
    `replay` is omitted (not `None`) unless requested via `replay=True`.
    `missiles` (final-review fix wave) is omitted (not `None`) unless
    requested via `missiles=True` (see `parse_file`/`parse_bytes`).
    `damage_mod_map` (M16) is omitted unless requested via `modifiers=True`;
    it is keyed by the SIGNED modifier id as a decimal string (`"174"`,
    `"-431"`) -- WITHOUT Elite Insights' `"d"` prefix, the same way
    `skill_map` drops EI's `"s"` -- and is scoped to the ids
    `players[]["damage_mods"]` actually references, not the whole
    catalog."""

    warnings: List[str]
    replay: ReplayOut
    missiles: MissilesOut
    damage_mod_map: Dict[str, DamageModDescOut]

# --- module functions -----------------------------------------------------

def parse_file(
    path: str,
    replay: bool = False,
    skill_damage: bool = False,
    timeseries: bool = False,
    missiles: bool = False,
    rotation: bool = False,
    modifiers: bool = False,
) -> Report:
    """Parse a `.evtc`/`.zevtc` file at `path` into the native `Report` shape.

    `replay` (M9, Task 2) opts into embedding the native combat-replay
    block (`Report["replay"]`); `skill_damage` (M12, Task 1) opts into
    embedding the native per-skill damage distribution block on every
    `players[]` entry (`PlayerOut["skill_damage"]`). `timeseries` (M12,
    Task 2) opts into embedding the native per-player per-second series
    block AND the per-enemy `dps_targets` summary (`PlayerOut["per_second"]`/
    `PlayerOut["dps_targets"]`). `missiles` (final-review fix wave) opts
    into embedding the native top-level missile analytics block
    (`Report["missiles"]`), mirroring the CLI's `--missiles` flag.
    `rotation` (M14, Task 1) opts into embedding the native per-player
    rotation (cast-tracking) block (`PlayerOut["rotation"]`), mirroring the
    CLI's `--rotation` flag. `modifiers` (M16) opts into the per-player
    damage-modifier block (`PlayerOut["damage_mods"]`) plus the top-level
    `Report["damage_mod_map"]`, mirroring the CLI's `--modifiers` flag.
    All six default to `False`.

    Raises `OSError` if `path` cannot be read, `ValueError` if the bytes
    are not a decodable/parseable arcdps log.
    """
    ...

def parse_bytes(
    data: bytes,
    replay: bool = False,
    skill_damage: bool = False,
    timeseries: bool = False,
    missiles: bool = False,
    rotation: bool = False,
    modifiers: bool = False,
) -> Report:
    """Parse an already-read `.evtc`/`.zevtc` buffer into the native `Report` shape.

    `replay` (M9, Task 2) opts into embedding the native combat-replay block.
    `skill_damage` (M12, Task 1) opts into embedding the native per-skill
    damage distribution block. `timeseries` (M12, Task 2) opts into
    embedding the native per-player per-second series block AND the
    per-enemy `dps_targets` summary. `missiles` (final-review fix wave)
    opts into embedding the native top-level missile analytics block.
    `rotation` (M14, Task 1) opts into embedding the native per-player
    rotation (cast-tracking) block. `modifiers` (M16) opts into
    `PlayerOut["damage_mods"]` + `Report["damage_mod_map"]`.

    Raises `ValueError` if `data` is not a decodable/parseable arcdps log.
    """
    ...

def parse_file_ei(
    path: str,
    *,
    replay: bool = False,
    skill_damage: bool = False,
    timeseries: bool = False,
    missiles: bool = False,
    rotation: bool = False,
    modifiers: bool = False,
) -> Dict[str, Any]:
    """Parse a `.evtc`/`.zevtc` file at `path` into Elite Insights-compatibility JSON.

    `skill_damage`/`timeseries` (final-review fix wave, keyword-only) are
    what actually let `totalDamageDist`/`damage1S`/`dpsTargets`/etc (M12,
    Task 3's ei-json mapping) surface in the returned JSON -- previously
    this function always omitted them regardless of caller intent.
    As of MEIGAP2 those two also gate three more GW2EI surfaces:
    `skill_damage` additionally carries the player distributions' outcome
    columns (`connectedHits`/`glance`/`missed`/`evaded`/`blocked`/
    `invulned`/`interrupted`/`indirectDamage`, plus per-skill
    `downContribution` on the outgoing one), and `timeseries` additionally
    carries `healthPercents` and `boonsStates` -- GW2EI's own
    `RawFormatTimelineArrays` gate on both. `instanceID`,
    `dpsAll[0].breakbarDamage` and `targets[].dpsAll` need no flag,
    matching GW2EI, which always emits them.
    `rotation` (M14, Task 3, keyword-only) likewise lets the ei-json
    `rotation[]` per-player block surface. `replay` (M15, Task 3) adds
    GW2EI's own combat-replay surface -- per-actor
    `combatReplayData.{positions, orientations, dc, iconURL}` (map pixels
    on GW2EI's fixed 300ms polling grid) plus the top-level
    `combatReplayMetaData`; it roughly triples the payload, hence opt-in.
    `missiles` is accepted for signature parity with `parse_file` but has
    no effect on the output (EI's JSON shape has no comparable field for
    it). `modifiers` (M16, keyword-only) adds Elite Insights' own
    `damageModifiers`/`incomingDamageModifiers`/`damageModifiersTarget`/
    `incomingDamageModifiersTarget` per-player arrays plus the top-level
    `damageModMap`; the per-target arrays dominate that payload (measured
    +441% on the committed fixture), hence opt-in. All six default to
    `False`, keeping `parse_file_ei(path)` back-compatible.

    Raises `OSError` if `path` cannot be read, `ValueError` if the bytes
    are not a decodable/parseable arcdps log.
    """
    ...

def anonymize_file(in_path: str, out_path: str) -> int:
    """Rewrite every player's character/account name in the `.zevtc` at
    `in_path` to a deterministic `Anon<N>` placeholder, writing the result
    to `out_path`. Returns the number of player agents rewritten.

    Raises `OSError` on a read/write failure, `ValueError` if `in_path`'s
    bytes are not a decodable arcdps log.
    """
    ...
