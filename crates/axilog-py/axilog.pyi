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

As of Task 12, `parse_file`/`parse_bytes` return the native output format
1.0 container (`axilog_schema::v1::ReportV1`,
`crates/axilog-schema/src/v1/{mod.rs,envelope.rs,entities.rs,catalogs.rs,
series.rs,blocks/*.rs}`) instead of the legacy `Report` above -- see the
`ReportV1` section near the bottom of this file for that transcription
(same `_XRequired`/`total=False` split convention, cross-checked
field-by-field against `axilog-node/types.d.ts`'s `ReportV1` types written
for the same task). `parse_file_ei` is unaffected and keeps returning
`Dict[str, Any]`. The legacy `Report` TypedDicts above are left in place
even though nothing now returns them, per this task's own instructions.
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
    # native format 1.0 (Task 12)
    "ReportV1",
    "AxilogMeta",
    "MarkerAssignmentOutV1",
    "EncounterOutV1",
    "Role",
    "CommanderOut",
    "EntityOut",
    "SkillEntry",
    "BuffEntry",
    "DamageModEntry",
    "Catalogs",
    "SeriesOut",
    "CoverageState",
    "Coverage",
    "WarningOut",
    "DamageSquad",
    "PerTarget",
    "SkillRow",
    "DamageEntity",
    "DamageBlock",
    "DefensesEntity",
    "DefensesBlock",
    "HitStatsEntity",
    "HitStatsBlock",
    "CcSquad",
    "CcEntity",
    "CcBlock",
    "GenerationRow",
    "BoonRow",
    "BoonsBlock",
    "SupportEntity",
    "SupportBlock",
    "ContributionRow",
    "ContributionEntity",
    "ContributionBlock",
    "HealingEntity",
    "HealingBlock",
    "CastRow",
    "RotationEntity",
    "RotationBlock",
    "DamageModRow",
    "DamageModsBlock",
    "MissilesSquad",
    "MissilesEntity",
    "MissilesBlock",
    "ReplayBounds",
    "ReplayTrack",
    "ReplayBlock",
    "SquadSeries",
    "TargetSeries",
    "EntitySeries",
    "SeriesBlock",
    "Blocks",
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

class _GenerationOutRequired(TypedDict):
    """Self/group/squad boon-generation attribution, 0-100 scale."""

    self_pct: float
    group_pct: float
    squad_pct: float

class GenerationOut(_GenerationOutRequired, total=False):
    """`self_wasted`/`group_wasted`/`squad_wasted` (MSMALL item 2) are the
    WASTED counterparts, identical scale: boon-time this source generated
    that was destroyed before the target could spend it -- a stack
    overwritten at capacity, or stripped/cleansed with duration left.
    GW2EI's `BuffStatistics.Wasted`.

    Rounded to 3 decimals (GW2EI's own `BuffDigit` precision, the most the
    reference format carries) and OMITTED when exactly zero -- read them as
    0.0 when absent.
    """

    self_wasted: float
    group_wasted: float
    squad_wasted: float

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

# --- native format 1.0 (`axilog_schema::v1::ReportV1`), Task 12 -----------
#
# Hand-transcribed from `crates/axilog-schema/src/v1/{mod.rs,envelope.rs,
# entities.rs,catalogs.rs,series.rs,blocks/*.rs}`. `parse_file`/
# `parse_bytes` now return `ReportV1`, not the legacy `Report` above.
# Same required/optional-key convention as the legacy section: a
# `#[serde(skip_serializing_if = ...)]` field is an optional (`total=False`)
# key typed as plain (non-`Optional`) `T`; everything else is required.

class _AxilogMetaRequired(TypedDict):
    #: The FORMAT contract version. Moves independently of ``version``.
    #: Currently always ``"1.0"``.
    schema: str
    #: The binary that produced this document (``CARGO_PKG_VERSION``).
    version: str

class AxilogMeta(_AxilogMetaRequired, total=False):
    """`generated_from` is the input log's file NAME (never a full path),
    omitted when unknown (e.g. `parse_bytes`, which has no file name to
    offer)."""

    generated_from: str

class _MarkerAssignmentOutV1Required(TypedDict):
    agent_addr: int
    marker: str
    time_ms: int

class MarkerAssignmentOutV1(_MarkerAssignmentOutV1Required, total=False):
    """One `CBTS_MARKER` assignment, native 1.0 shape. `agent_addr` is
    always present (arcdps does not restrict `CBTS_MARKER` to squad
    members, and many carrying agents never become tracked entities);
    `entity_id` is present only when the agent resolves to a roster
    entity."""

    entity_id: int

class _EncounterOutV1Required(TypedDict):
    kind: str
    map: str
    duration_ms: int
    build: str
    revision: int
    teams: List[TeamOut]
    markers: List[MarkerAssignmentOutV1]

class EncounterOutV1(_EncounterOutV1Required, total=False):
    """The 1.0 encounter envelope -- a reprojection of the legacy
    `EncounterOut` with `markers` rekeyed from `agent_addr` to
    `entity_id`. `recorded_by` (the entity id of the recording player) and
    `tick_rate` are both omitted (not `None`) when absent."""

    recorded_by: int
    tick_rate: TickRateOut

#: What an entity IS -- squad member, non-squad friendly, enemy player, or
#: NPC/gadget. Declaration order is `entities[]`'s sort order.
Role = str  # Literal["squad", "friendly_player", "enemy_player", "npc"]

class CommanderOut(TypedDict):
    variant: str
    guid: str

class _EntityOutRequired(TypedDict):
    #: Dense index into `entities[]`, from 0. Stable WITHIN a report only --
    #: join across logs on `account`.
    id: int
    role: Role
    team: str
    #: The arcdps agent address -- a documented attribute, not a secret.
    agent_addr: int
    #: Whether this entity interacted with the squad at all (dealt damage
    #: to the squad, took damage from the squad, or took CC from the
    #: squad). Always `True` for squad members and non-squad friendlies.
    #: Never optional.
    combat_participant: bool

class EntityOut(_EntityOutRequired, total=False):
    """One agent's IDENTITY only -- no statistics; those live in `blocks`,
    keyed by `id`. `account`/`character` are players only; `name` is
    non-player entities only; `profession` is present exactly for player
    roles; `elite_spec` is empty string (not omitted) when the agent has no
    nameable elite spec. All optional fields here are omitted (not `None`)
    when absent."""

    account: str
    character: str
    name: str
    profession: str
    elite_spec: str
    subgroup: int
    commander: CommanderOut
    guild_id: str
    marker: str
    instid: int

class _SkillEntryRequired(TypedDict):
    name: str
    is_swap: bool
    can_crit: bool

class SkillEntry(_SkillEntryRequired, total=False):
    """Definition metadata for one referenced skill id. `auto_attack` is
    omitted (not `None`) when unknown."""

    auto_attack: bool

class _BuffEntryRequired(TypedDict):
    name: str
    #: GW2's real three-way taxonomy (arcdps does not distinguish these
    #: structurally): ``"condition"``, ``"boon"``, or ``"effect"``.
    kind: str
    #: ``"intensity"`` or ``"duration"``.
    stacking: str

class BuffEntry(_BuffEntryRequired, total=False):
    """Definition metadata for one referenced buff id. `max_stacks` is
    omitted (not `None`) when the catalog has no known capacity."""

    max_stacks: int

class DamageModEntry(TypedDict):
    """Definition metadata for one referenced damage-modifier id."""

    name: str
    kind: str
    approximate: bool

class _CatalogsRequired(TypedDict):
    skills: Dict[str, SkillEntry]
    buffs: Dict[str, BuffEntry]

class Catalogs(_CatalogsRequired, total=False):
    """Definition metadata for every id any block references. No
    human-readable name appears outside `catalogs` or `entities`. Keys
    serialize as decimal strings. `damage_mods` is omitted entirely (not
    `{}`) when no damage-modifier id was referenced (e.g. `modifiers` was
    not requested)."""

    damage_mods: Dict[str, DamageModEntry]

class SeriesOut(TypedDict):
    """One time series in the format's single series envelope. `enc` is
    ``"raw"`` (a plain array of values) or ``"rle"`` (an array of
    ``[value, run_length]`` pairs), chosen per series by whichever
    serializes smaller. `len` is the DECODED length in both cases (NOT
    ``len(data)``)."""

    interval_ms: int
    #: Decoded length, NOT `len(data)`.
    len: int
    enc: str  # Literal["raw", "rle"]
    data: List[Any]

#: Why a block is or is not present in `blocks`.
#: Why a block is or is not in `blocks`:
#:
#: - "present"      -- computed, carried, at least one row.
#: - "not_computed" -- the compute gate was off. NOT the same as empty:
#:                     a missing flag, not a fact about the log.
#: - "empty"        -- computed and genuinely nothing to report. The block
#:                     is still carried; only "not_computed"/"unsupported"
#:                     omit it.
#: - "unsupported"  -- RESERVED. No code path in this version emits it:
#:                     nothing this container computes is era- or
#:                     encounter-kind-gated. Named now so spec #2's
#:                     era-gated surfaces can fill the slot without
#:                     consumers learning a new value late. Handle it, but
#:                     do not expect it from this version.
CoverageState = str  # Literal["present", "not_computed", "empty", "unsupported"]

#: Per-block computation/presence status, keyed by block name (``"damage"``,
#: ``"defenses"``, ``"hit_stats"``, ``"cc"``, ``"boons"``, ``"support"``,
#: ``"contribution"``, ``"healing"``, ``"rotation"``, ``"damage_mods"``,
#: ``"missiles"``, ``"replay"``, ``"series"``, ``"conditions"``,
#: ``"minions"``). Always names every known block, even ones this schema
#: version never computes (``"conditions"``/``"minions"``, reserved for
#: spec #2, always ``"not_computed"``).
Coverage = Dict[str, CoverageState]

class _WarningOutRequired(TypedDict):
    code: str
    severity: str  # Literal["info", "warn", "error"]
    message: str

class WarningOut(_WarningOutRequired, total=False):
    """A structured, user-facing analysis warning. `entity_id` is omitted
    (not `None`) when the warning is not about a specific entity."""

    entity_id: int

# --- damage block (native 1.0) ---------------------------------------------

class DamageSquad(TypedDict):
    """Aggregates `Role.squad` entities ONLY -- not every friendly player
    (see `DamageBlock.by_entity`, which is the full roster)."""

    total: int
    dps: float

class SkillRow(TypedDict):
    """Mirrors the legacy `SkillEntryOut` field-for-field (minus `skill_id`,
    which is the map key here). `hits`/`min`/`max` count only CONTRIBUTING
    (`dmg > 0`) events. `crit_hits`/`flank_hits` are hit COUNTS, not damage
    sums."""

    total: int
    hits: int
    min: int
    max: int
    crit_hits: int
    flank_hits: int

class PerTargetDetail(TypedDict):
    """Mirrors the legacy `PerTargetStatsOut` field-for-field, minus
    `enemy_id` (the enclosing map's key here, as the target's ENTITY id).
    `interrupts` and `downs_contribution_damage` are not derivable from any
    other block."""

    connected_hits: int
    connected_damage: int
    against_downed_count: int
    downed: int
    killed: int
    interrupts: int
    #: arcdps-methodology down-contribution DAMAGE for downs of this
    #: specific target -- NOT GW2EI's 90%-to-downstate-window algorithm.
    downs_contribution_damage: int

class _PerTargetRequired(TypedDict):
    #: Ungated: the legacy `DamageOut.per_enemy` total.
    total: int

class PerTarget(_PerTargetRequired, total=False):
    """One `(entity, target)` pair. `total` is always present; `detail` and
    `by_skill` come from the `skill_damage=True`-gated families, so a row
    can legitimately carry `total` alone.

    `detail` is grouped under one key rather than flattened so its absence
    has a single unambiguous signal instead of seven fabricated zeros."""

    detail: PerTargetDetail
    #: Per-(entity, target, skill) outgoing damage, keyed by skill id.
    by_skill: Dict[str, SkillRow]

class _DamageEntityRequired(TypedDict):
    total: int
    dps: float
    taken: int
    #: Enemy players this entity landed the DOWNING blow on. Outgoing
    #: outcome; the incoming mirrors are `DefensesEntity.downs_taken`/
    #: `deaths`, the same split GW2EI makes.
    downs_dealt: int
    #: Enemy players this entity landed the KILLING blow on.
    kills_dealt: int

class DamageEntity(_DamageEntityRequired, total=False):
    """`per_target` (keyed by the TARGET's entity id) and the two per-skill
    maps (keyed by skill id) are all omitted (not `{}`) when empty; the
    per-skill maps are additionally gated by `skill_damage=True`.

    `by_skill` is OUTGOING, `by_skill_taken` is INCOMING -- mirroring this
    row's own `total`/`taken` pair."""

    per_target: Dict[str, PerTarget]
    by_skill: Dict[str, SkillRow]
    by_skill_taken: Dict[str, SkillRow]

class DamageBlock(TypedDict):
    squad: DamageSquad
    by_entity: Dict[str, DamageEntity]

# --- defenses / hit_stats / cc blocks (native 1.0) --------------------------

class DefensesEntity(TypedDict):
    """Incoming defenses. Mirrors the legacy `DefensesOut` field-for-field."""

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
    #: Incoming crowd control.
    received_cc_count: int
    received_cc_duration_ms: int
    #: Boons stripped OFF this player -- the incoming counterpart of
    #: `SupportEntity.strips`.
    boon_strips_taken: int
    boon_strips_taken_duration_ms: int
    #: Times this entity entered downstate -- GW2EI's
    #: `defenses[0].downCount`. The outgoing mirrors live on
    #: `DamageEntity.downs_dealt`/`kills_dealt`.
    downs_taken: int
    #: Times this entity died -- GW2EI's `defenses[0].deadCount`.
    deaths: int

class DefensesBlock(TypedDict):
    by_entity: Dict[str, DefensesEntity]

class HitStatsEntity(TypedDict):
    """Outgoing hit quality. Mirrors the legacy `HitStatsOut`
    field-for-field. `above90_*` counts/damage are against a target at or
    above 90% health."""

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

class HitStatsBlock(TypedDict):
    by_entity: Dict[str, HitStatsEntity]

class CcSquad(TypedDict):
    """Aggregates `Role.squad` entities ONLY."""

    applied_total: int
    applied_duration_ms: int

class CcEntity(TypedDict):
    """Mirrors the legacy `CcOut` field-for-field. Incoming CC lives on
    `DefensesEntity.received_cc_count`/`received_cc_duration_ms` instead."""

    applied_total: int
    applied_duration_ms: int
    stun_breaks: int
    removed_stun_duration_ms: int

class CcBlock(TypedDict):
    squad: CcSquad
    by_entity: Dict[str, CcEntity]

# --- boons / support / contribution / healing blocks (native 1.0) ----------

class _GenerationRowRequired(TypedDict):
    self_pct: float
    group_pct: float
    squad_pct: float

class GenerationRow(_GenerationRowRequired, total=False):
    """Mirrors the legacy `GenerationOut` field-for-field. WASTED fields
    are rounded to 3 decimals and omitted when exactly zero -- read them
    as 0.0 when absent."""

    self_wasted: float
    group_wasted: float
    squad_wasted: float

class _BoonRowRequired(TypedDict):
    uptime_pct: float
    generation: GenerationRow

class BoonRow(_BoonRowRequired, total=False):
    """Mirrors the legacy `BoonOut` field-for-field, minus `id` (the map
    key) and `name` (resolve via `Catalogs.buffs`). `avg_stacks` is
    omitted for duration-type boons."""

    avg_stacks: float

class BoonsBlock(TypedDict):
    """entity id -> buff id -> row. Two levels of real ids, no positional joins."""

    by_entity: Dict[str, Dict[str, BoonRow]]

class SupportEntity(TypedDict):
    """Mirrors the legacy `SupportOut` field-for-field."""

    cleanses: int
    cleanses_self: int
    strips: int
    strips_duration_ms: int
    resurrects: int

class SupportBlock(TypedDict):
    by_entity: Dict[str, SupportEntity]

class ContributionRow(TypedDict):
    """Mirrors the legacy `ContributionOut` field-for-field."""

    damage: int
    cc: int
    strips: int
    movement_impairing: int

class ContributionEntity(TypedDict):
    """Both directions of the arcdps-methodology down contribution."""

    downs_contribution: ContributionRow
    downed_by: ContributionRow
    #: `downs_contribution["damage"]` sliced by the skill that dealt it,
    #: keyed by skill id as a decimal string. Sparse -- only skills with a
    #: nonzero credit appear -- and omitted entirely when there are none.
    #: Ungated, unlike the per-skill DAMAGE rows on `blocks.damage`, which is
    #: why it lives here rather than beside them.
    downs_contribution_by_skill: Dict[str, int]

class ContributionBlock(TypedDict):
    by_entity: Dict[str, ContributionEntity]

class HealingEntity(TypedDict):
    """Mirrors the legacy `HealingOut` field-for-field."""

    outgoing_total: int
    outgoing_allies: int
    outgoing_self: int
    barrier_out: int
    downed_healing_out: int

class HealingBlock(TypedDict):
    by_entity: Dict[str, HealingEntity]

# --- rotation / damage_mods / missiles / replay / series blocks (1.0) ------

class CastRow(TypedDict):
    """One cast. Mirrors the legacy `CastOut` field-for-field, plus
    `skill_id` hoisted from the enclosing skill grouping so this is a flat,
    time-ordered cast list per entity."""

    skill_id: int
    cast_time_ms: int
    duration_ms: int
    #: Negative when the cast was interrupted/cancelled early.
    time_gained_ms: int
    quickness: float

class Aftercast(TypedDict):
    """Mirrors the legacy `AftercastOut`. Durations are MILLISECONDS (GW2EI
    emits the same quantities as seconds).

    NOTE the name collision GW2EI bequeathed: `wasted_count` is a
    CAST-INTERRUPT count, unrelated to the boon-generation `*_wasted`
    fields under `boons`."""

    #: Casts that skipped their aftercast.
    saved_count: int
    saved_ms: int
    #: Casts interrupted before firing.
    wasted_count: int
    #: Already the positive "time lost" figure.
    wasted_ms: int

class RotationEntity(TypedDict):
    #: This entity's casts, in cast-start order. Present exactly when the
    #: cast gate (`rotation=True`) was on -- an EMPTY list means the pass
    #: ran and this entity cast nothing, while an absent key means it never
    #: ran. `aftercast` below is always-on, so the block's `coverage` cannot
    #: answer that question and this key's presence is the gate record.
    casts: List[CastRow]
    #: Cast counters, computed unconditionally.
    aftercast: Aftercast

class RotationBlock(TypedDict):
    by_entity: Dict[str, RotationEntity]

class DamageModRow(TypedDict):
    """One damage-modifier row. `id` (the map key) is SIGNED -- negative
    means incoming -- so one map naturally separates outgoing from
    incoming."""

    hit_count: int
    total_hit_count: int
    damage_gain: float
    total_damage: int

class DamageModEntity(TypedDict):
    """One entity's damage modifiers, in the two scopes GW2EI evaluates
    them at. Neither is derivable from the other: `overall` counts every
    qualifying hit, including hits on agents that are not targets at all
    (enemy minions), while `per_target` restricts to one foe."""

    #: Whole fight, keyed by the signed modifier id as a decimal string.
    overall: Dict[str, DamageModRow]
    #: Restricted to one foe, keyed by the TARGET's entity id then by the
    #: signed modifier id. Sparse in both dimensions. The per-target split
    #: is the expensive half and the native path does not compute it, so an
    #: absent key on a present block means "the split was not computed",
    #: not "there was none".
    per_target: Dict[str, Dict[str, DamageModRow]]

class DamageModsBlock(TypedDict):
    by_entity: Dict[str, DamageModEntity]

class MissilesSquad(TypedDict):
    fired: int
    hit: int
    denied: int
    incoming_fired: int
    incoming_denied: int

class MissilesEntity(TypedDict):
    """Mirrors the legacy `PlayerMissilesOut`, minus `agent_addr`/`account`
    (identity already lives on this row's own `entities[]` entry)."""

    fired: int
    hit: int
    denied: int
    reflected_at_self: int

class MissilesBlock(TypedDict):
    """`squad` is REQUIRED, like every other squad aggregate in this format
    (`damage`, `cc`, `series`): when the block is present, so is its
    aggregate."""

    squad: MissilesSquad
    by_entity: Dict[str, MissilesEntity]

class ReplayBounds(TypedDict):
    min_x: float
    min_y: float
    max_x: float
    max_y: float

class ReplayTrack(TypedDict):
    """One entity's replay track. `samples` are `[t_ms, x, y]` triples;
    `down_intervals`/`dead_intervals` are `[start_ms, end_ms]` pairs.
    `name`/`team`/`commander`/`is_squad` are dropped versus the legacy
    `ReplayTrackOut` -- they live on this entity's own `entities[]` row."""

    samples: List[List[float]]
    down_intervals: List[List[int]]
    dead_intervals: List[List[int]]

class _ReplayBlockRequired(TypedDict):
    #: Shared polling interval for every track.
    poll_ms: int
    by_entity: Dict[str, ReplayTrack]

class ReplayBlock(_ReplayBlockRequired, total=False):
    """`bounds` is omitted (not `None`) only in the empty-block default
    case (replay not requested)."""

    bounds: ReplayBounds

class SquadSeries(TypedDict):
    damage: SeriesOut
    cc_applied: SeriesOut
    downs: SeriesOut

class TargetSeries(TypedDict):
    """Mirrors the legacy `PlayerTargetSeriesOut`, minus `enemy_id` (that's
    the map key here, joined by entity id)."""

    damage: SeriesOut
    power_damage: SeriesOut

class _EntitySeriesRequired(TypedDict):
    damage: SeriesOut
    damage_taken: SeriesOut
    power_damage_taken: SeriesOut

class EntitySeries(_EntitySeriesRequired, total=False):
    """Mirrors the legacy `PlayerPerSecondOut` field-for-field.
    `per_target` is keyed by the TARGET's entity id, omitted (not `{}`)
    when empty."""

    per_target: Dict[str, TargetSeries]

class SeriesBlock(TypedDict):
    """`squad` is REQUIRED (see `MissilesBlock`). The squad series is
    computed unconditionally; only `by_entity` needs `timeseries=True`."""

    squad: SquadSeries
    by_entity: Dict[str, EntitySeries]

class Blocks(TypedDict, total=False):
    """Every statistic block. A block is omitted entirely (key absent, not
    `None`) when `coverage` says `not_computed`/`unsupported`; an `empty`
    block is still carried, so a consumer can tell "computed and there was
    nothing" from "never ran"."""

    damage: DamageBlock
    defenses: DefensesBlock
    hit_stats: HitStatsBlock
    cc: CcBlock
    boons: BoonsBlock
    support: SupportBlock
    contribution: ContributionBlock
    healing: HealingBlock
    rotation: RotationBlock
    damage_mods: DamageModsBlock
    missiles: MissilesBlock
    replay: ReplayBlock
    series: SeriesBlock

class _ReportV1Required(TypedDict):
    axilog: AxilogMeta
    encounter: EncounterOutV1
    entities: List[EntityOut]
    catalogs: Catalogs
    blocks: Blocks
    coverage: Coverage

class ReportV1(_ReportV1Required, total=False):
    """axilog's native output format 1.0 container
    (`axilog_schema::v1::ReportV1`), as returned by `parse_file`/
    `parse_bytes` (Task 12). `warnings` is omitted (not `[]`) when there
    are none."""

    warnings: List[WarningOut]

# --- module functions -----------------------------------------------------

def parse_file(
    path: str,
    replay: bool = False,
    skill_damage: bool = False,
    timeseries: bool = False,
    missiles: bool = False,
    rotation: bool = False,
    modifiers: bool = False,
) -> ReportV1:
    """Parse a `.evtc`/`.zevtc` file at `path` into the native output format
    1.0 container (`ReportV1`, Task 12).

    `path`'s file name (never the full path) is threaded into the
    document's `axilog.generated_from`. `replay` (M9, Task 2) opts into
    embedding the native combat-replay block (`Blocks.replay`);
    `skill_damage` (M12, Task 1) opts into embedding the native per-skill
    damage distribution block on every entity's damage row
    (`DamageEntity.by_skill`). `timeseries` (M12, Task 2) opts into
    embedding the native per-entity per-second series block
    (`Blocks.series`). `missiles` (final-review fix wave) opts into
    embedding the native top-level missile analytics block
    (`Blocks.missiles`), mirroring the CLI's `--missiles` flag.
    `rotation` (M14, Task 1) opts into embedding the native per-entity
    rotation (cast-tracking) block (`Blocks.rotation`), mirroring the
    CLI's `--rotation` flag. `modifiers` (M16) opts into the per-entity
    damage-modifier block (`Blocks.damage_mods`), mirroring the CLI's
    `--modifiers` flag. All six default to `False`.

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
) -> ReportV1:
    """Parse an already-read `.evtc`/`.zevtc` buffer into the native output
    format 1.0 container (`ReportV1`, Task 12).

    A buffer has no file name to offer, so `axilog.generated_from` is
    always absent. `replay` (M9, Task 2) opts into embedding the native
    combat-replay block (`Blocks.replay`). `skill_damage` (M12, Task 1)
    opts into embedding the native per-skill damage distribution block
    (`DamageEntity.by_skill`). `timeseries` (M12, Task 2) opts into
    embedding the native per-entity per-second series block
    (`Blocks.series`). `missiles` (final-review fix wave) opts into
    embedding the native top-level missile analytics block
    (`Blocks.missiles`). `rotation` (M14, Task 1) opts into embedding the
    native per-entity rotation (cast-tracking) block (`Blocks.rotation`).
    `modifiers` (M16) opts into the per-entity damage-modifier block
    (`Blocks.damage_mods`).

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
