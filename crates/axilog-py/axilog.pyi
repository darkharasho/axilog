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
    "GroundMarkerOutV1",
    "MarkerAssignmentOutV1",
    "EncounterOutV1",
    "Role",
    "CommanderOut",
    "EntityOut",
    "SkillEntry",
    "BuffEntry",
    "DamageModEntry",
    "MinionEntry",
    "Catalogs",
    "StateTimeline",
    "PerSourceStates",
    "SeriesOut",
    "CoverageState",
    "Coverage",
    "WarningOut",
    "DamageSquad",
    "PerTarget",
    "SkillRow",
    "SkillOutcomeCols",
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
    "HealingDetailCols",
    "AllyHealingRow",
    "HealSkillRow",
    "HealingBlock",
    "ConditionRow",
    "ConditionsBlock",
    "SelfEffectRow",
    "SelfEffectsBlock",
    "MinionSkillTakenRow",
    "MinionRow",
    "MinionsBlock",
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
    "ReplayIntervals",
    "Arena",
    "ReplayTracks",
    "GliderOut",
    "TransformationOut",
    "CaptureOut",
    "CaptureShapeOut",
    "OwnerStateOut",
    "ProgressStateOut",
    "DecorationOut",
    "DecorationShapeOut",
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
    """`guid` is omitted (not `null`) when this team has no known content
    GUID. `shard_id` is this team's WvW world/shard id from
    `CBTS_WVWTEAMS`; it is omitted the same way, and is absent for logs
    predating that event and for any team the event does not name (a team
    can therefore have a `color` but no `shard_id`)."""

    guid: str
    shard_id: int

class ObjectiveOwnerOut(TypedDict):
    """One ownership observation. `time_ms` is log-relative milliseconds."""

    team_id: int
    time_ms: int

class ObjectiveOut(TypedDict):
    """One WvW objective's ownership timeline, from
    `CBTS_WVWOBJECTIVESTATUS`. `objective_type` is one of `"Camp"`,
    `"Ruins"`, `"Tower"`, `"Keep"`, `"Castle"` -- never `"Unknown"`, since
    an objective the static catalog cannot type is dropped rather than
    emitted untyped. `owners` keeps repeats rather than collapsing them,
    matching GW2EI."""

    map_id: int
    objective_id: int
    objective_type: str
    owners: List[ObjectiveOwnerOut]

class _EncounterOutRequired(TypedDict):
    #: `"wvw"`, or an Elite Insights PvE category slug: `"raid_wing"`,
    #: `"raid_encounter"`, `"fractal"`, `"golem"`, `"story"`,
    #: `"open_world"`, `"convergence"`, `"unknown_encounter"`,
    #: `"unknown"`.
    kind: str
    #: WvW map display name -- an EMPTY STRING for a PvE log, where
    #: `map_id` is still the real instance map. Use `encounter_name` to
    #: label a PvE fight.
    map: str
    duration_ms: int
    build: str
    revision: int
    # `Option<String>` with no `skip_serializing_if` -> always present, `null` when unknown.
    recorded_by: Optional[str]
    teams: List[TeamOut]
    markers: List[MarkerAssignmentOut]
    #: WvW objective ownership timelines. Empty (not omitted) for non-WvW
    #: logs and for logs predating `CBTS_WVWOBJECTIVESTATUS`.
    objectives: List[ObjectiveOut]

class EncounterOut(_EncounterOutRequired, total=False):
    """`tick_rate` is omitted when the log has fewer than two `CBTS_TICK`
    events. `started_at_unix` is the wall-clock log start in SECONDS since
    the Unix epoch, from arcdps's `CBTS_LOGSTART`; it is omitted (not
    `None`) when the log carries no such event, so absence stays
    distinguishable from epoch zero -- do not default it to `0`."""

    tick_rate: TickRateOut
    started_at_unix: int
    #: The raw `CBTS_MAPID` value `map` is the display name for. Omitted
    #: (not `None`) when the log carries no MAP_ID event, which stays
    #: distinguishable from map id 0. Match on this, not on `map`, when
    #: joining against your own per-map assets.
    map_id: int
    #: The fight's name, for a PvE log: `"Gorseval the Multifarious"`,
    #: `"Twin Largos"`, `"Harvest Temple"`. Omitted (not `None`) for WvW
    #: logs, which have no encounter identity apart from their map --
    #: check for this key rather than testing `kind == "wvw"`.
    #:
    #: Carries NO challenge-mote suffix: a CM Skorvald reads `"Skorvald"`,
    #: where Elite Insights would say `"Skorvald CM"`.
    encounter_name: str
    #: arcdps's header trigger species id -- the one fact the log records
    #: about which encounter this is, and the join key into Elite
    #: Insights' own tables. Omitted for WvW logs.
    trigger_id: int
    #: The wing or fractal this encounter belongs to (`"SpiritVale"`,
    #: `"ShatteredObservatory"`), from Elite Insights' `SubLogCategory`.
    #: Omitted for WvW logs and for encounters with no declared grouping.
    sub_category: str
    #: Whether the squad won. Omitted for WvW logs, which have no failure
    #: state. `True` is reliable; `False` is NOT the same as "wiped" --
    #: only the generic "every trigger-species agent died" rule is
    #: implemented, so encounters won by reward chest or scripted event
    #: (Siege the Stronghold, Twisted Castle, River of Souls, the Hall of
    #: Chains statues) report `False` on a clean kill.
    success: bool

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
    #: Conditions removed from a MINION owned by a genuine squad player --
    #: the arcdps-parity extra, NOT part of GW2EI's numbers. EI's cleanse
    #: count is ``log.PlayerList``-scoped so it omits pets/minions entirely;
    #: the in-game arcdps meter folds pets into their master and counts them,
    #: hence the ~3-4%% gap. Never folded into ``cleanses``.
    cleanses_minions: int
    #: The in-game arcdps meter's OWN cleanse methodology -- an independent
    #: count, NOT a correction to ``cleanses``/``cleanses_self``/
    #: ``cleanses_minions`` and never to be summed with them. Transcribed
    #: from the reference code arcdps' author published on 2026-08-26.
    #: Three buckets because what the meter displays depends on that
    #: window's "vs npcs"/"from npcs" toggles: this field alone is both-off,
    #: add ``cleanses_arcdps_on_minion`` for "vs npcs" and
    #: ``cleanses_arcdps_by_minion`` for "from npcs".
    cleanses_arcdps: int
    #: "from npcs" adjustment: the remover was this player's pet/minion.
    cleanses_arcdps_by_minion: int
    #: "vs npcs" adjustment: the condition came off a pet/minion.
    cleanses_arcdps_on_minion: int
    strips: int
    #: The strip twin of ``cleanses_arcdps``, same bucketing.
    strips_arcdps: int
    #: "from npcs" adjustment: stripped by this player's pet/minion.
    strips_arcdps_by_minion: int
    #: "vs npcs" adjustment: the boon came off an enemy pet/minion.
    strips_arcdps_on_minion: int
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
    `axilog_core::analysis::skill_map`'s doc comment).

    MPROC: the five proc/accuracy/instant flags are omitted when FALSE --
    a proc flag is rare, and emitting ~370 x 5 literal `false`s cost 16%
    of the report. Absence means false, not unknown. `is_instant_cast` is
    the strong one: a finder actually fired in this log, not merely that
    one was available."""

    is_trait_proc: bool
    is_gear_proc: bool
    is_unconditional_proc: bool
    is_not_accurate: bool
    is_instant_cast: bool

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
    #: Boons the squad stripped off enemies, per second (1s buckets, non-cumulative).
    strips: List[int]

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

class _GroundMarkerOutV1Required(TypedDict):
    index: int
    name: str
    x: float
    y: float
    z: float
    start_ms: int

class GroundMarkerOutV1(_GroundMarkerOutV1Required, total=False):
    """One ground-placed squad marker, from `CBTS_SQUADMARKER`. Attached to
    a world POSITION rather than an agent, and identified by a fixed index
    rather than a content GUID. Positions are world inches."""

    icon: str
    end_ms: int

class MarkerAssignmentOutV1(_MarkerAssignmentOutV1Required, total=False):
    """One `CBTS_MARKER` assignment, native 1.0 shape. `agent_addr` is
    always present (arcdps does not restrict `CBTS_MARKER` to squad
    members, and many carrying agents never become tracked entities);
    `entity_id` is present only when the agent resolves to a roster
    entity."""

    entity_id: int
    marker_kind: str
    marker_label: str
    marker_icon: str

class _EncounterOutV1Required(TypedDict):
    #: `"wvw"`, or an Elite Insights PvE category slug: `"raid_wing"`,
    #: `"raid_encounter"`, `"fractal"`, `"golem"`, `"story"`,
    #: `"open_world"`, `"convergence"`, `"unknown_encounter"`,
    #: `"unknown"`.
    kind: str
    #: WvW map display name -- an EMPTY STRING for a PvE log, where
    #: `map_id` is still the real instance map. Use `encounter_name` to
    #: label a PvE fight.
    map: str
    duration_ms: int
    build: str
    revision: int
    teams: List[TeamOut]
    markers: List[MarkerAssignmentOutV1]
    ground_markers: List[GroundMarkerOutV1]
    #: The log's `t0` in arcdps SESSION-time milliseconds -- the origin
    #: every other time in this document is already measured from. Exactly
    #: two fields are not log-relative: `markers[].time_ms` and
    #: `entities[].commander.segments`. Subtract this from either for an
    #: encounter-relative value (it may go negative, e.g. a tag held before
    #: the first event). Always present; `0` for a log with no events.
    log_start_ms: int
    #: WvW objective ownership timelines -- carried verbatim from the
    #: legacy shape; an objective belongs to the map, not to an entity, so
    #: unlike `markers` there is no join key to rekey.
    objectives: List[ObjectiveOut]

class EncounterOutV1(_EncounterOutV1Required, total=False):
    """The 1.0 encounter envelope -- a reprojection of the legacy
    `EncounterOut` with `markers` rekeyed from `agent_addr` to
    `entity_id`. `recorded_by` (the entity id of the recording player) and
    `tick_rate` are both omitted (not `None`) when absent."""

    recorded_by: int
    tick_rate: TickRateOut
    #: The fight's name, for a PvE log: `"Gorseval the Multifarious"`,
    #: `"Twin Largos"`, `"Harvest Temple"`. Omitted (not `None`) for WvW
    #: logs, which have no encounter identity apart from their map --
    #: check for this key rather than testing `kind == "wvw"`.
    #:
    #: Carries NO challenge-mote suffix: a CM Skorvald reads `"Skorvald"`,
    #: where Elite Insights would say `"Skorvald CM"`.
    encounter_name: str
    #: arcdps's header trigger species id -- the one fact the log records
    #: about which encounter this is, and the join key into Elite
    #: Insights' own tables. Omitted for WvW logs.
    trigger_id: int
    #: The wing or fractal this encounter belongs to (`"SpiritVale"`,
    #: `"ShatteredObservatory"`), from Elite Insights' `SubLogCategory`.
    #: Omitted for WvW logs and for encounters with no declared grouping.
    sub_category: str
    #: Whether the squad won. Omitted for WvW logs, which have no failure
    #: state. `True` is reliable; `False` is NOT the same as "wiped" --
    #: only the generic "every trigger-species agent died" rule is
    #: implemented, so encounters won by reward chest or scripted event
    #: (Siege the Stronghold, Twisted Castle, River of Souls, the Hall of
    #: Chains statues) report `False` on a clean kill.
    success: bool
    #: Wall-clock log start, SECONDS since the Unix epoch, from arcdps's
    #: `CBTS_LOGSTART`. Omitted (not `None`) when the log carries no such
    #: event -- absence stays distinguishable from epoch zero, so do not
    #: default it to `0`.
    started_at_unix: int
    #: The raw `CBTS_MAPID` value `map` is the display name for. Omitted
    #: (not `None`) when the log carries no MAP_ID event, which stays
    #: distinguishable from map id 0. Match on this, not on `map`, when
    #: joining against your own per-map assets.
    map_id: int

#: What an entity IS -- squad member, non-squad friendly, enemy player, or
#: NPC/gadget. Declaration order is `entities[]`'s sort order.
Role = str  # Literal["squad", "friendly_player", "enemy_player", "npc"]

class CommanderOut(TypedDict):
    variant: str
    guid: str
    #: Terminated, half-open `[tag-on, tag-off)` windows in ARCDPS SESSION
    #: TIME -- the same base as `encounter["markers"][i]["time_ms"]`, NOT
    #: log-relative and NOT comparable against `encounter["duration_ms"]`.
    #: Do not clip these to `[0, duration_ms]`; subtract the log's own `t0`
    #: first if you need encounter-relative values. Literal per-instance
    #: holds, not a coalesced span: a zero-width `[t, t]` pair from a
    #: same-timestamp reassignment is normal. An empty list on a PRESENT
    #: `commander` means the tag was detected but its windows could not be
    #: resolved -- not that the player never commanded.
    segments: List[List[int]]

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
    omitted (not `None`) when unknown.

    MPROC: see `SkillMapEntryOut` -- the five proc flags are omitted
    when false.

    `icon` (Phase C) is a render-service or wiki URL, resolved from
    `skill_icons` (the GW2 API) first and `buff_icons` (GW2EI's own table)
    second; omitted when neither knows the id. Buff ids resolve here too --
    there is no separate icon field on `BuffEntry`."""

    icon: str
    auto_attack: bool
    is_trait_proc: bool
    is_gear_proc: bool
    is_unconditional_proc: bool
    is_not_accurate: bool
    is_instant_cast: bool

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

class _DamageModEntryRequired(TypedDict):
    name: str

class DamageModEntry(_DamageModEntryRequired, total=False):
    """Definition metadata for one referenced damage-modifier id. Only
    `name` is guaranteed; every descriptive field is omitted (not `None`)
    when the catalog does not carry it.

    `description` is EI's full tooltip, including its derived
    `<br>Applied on ...` / `<br>Counter` / `<br>Approximate` suffixes."""

    icon: str
    description: str
    non_multiplier: bool
    is_counter: bool
    skill_based: bool
    approximate: bool

class MinionEntry(TypedDict):
    """Definition metadata for one minion group referenced by
    `MinionsBlock`. Identity lives here rather than on the row, like every
    other catalog in this format."""

    species_id: int
    name: str

class _CatalogsRequired(TypedDict):
    skills: Dict[str, SkillEntry]
    buffs: Dict[str, BuffEntry]

class Catalogs(_CatalogsRequired, total=False):
    """Definition metadata for every id any block references. No
    human-readable name appears outside `catalogs` or `entities`. Keys
    serialize as decimal strings. `damage_mods` and `minions` are each
    omitted entirely (not `{}`) when no id of that kind was referenced
    (e.g. `modifiers`/`skill_damage` was not requested)."""

    damage_mods: Dict[str, DamageModEntry]
    minions: Dict[str, MinionEntry]

# --- shared buff-timeline types (native 1.0) -------------------------------

#: `[[time_ms, stacks], ...]` -- a buff's stack count as a STEP function:
#: each pair holds until the next one. Duration-type buffs are clamped to
#: 0/1 upstream so the graph matches GW2EI's; intensity-type buffs carry
#: their real stack count.
StateTimeline = List[List[int]]

class _PerSourceStatesRequired(TypedDict):
    #: Keyed by the APPLYING entity's id (not by character name, which is
    #: what both underlying passes key on -- a name is identity data and
    #: two players can share one).
    by_source: Dict[str, StateTimeline]

class PerSourceStates(_PerSourceStatesRequired, total=False):
    """A buff's stack timeline split by who applied it.

    `unresolved` merges every applier that resolves to no `entities[]` row
    at all -- the honest spelling of GW2EI's `UNKNOWN` key, and a genuine
    remainder rather than EI's much larger "not a squad player" bucket,
    since native resolves every applier it can. Omitted when empty."""

    unresolved: StateTimeline

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
#: ``"minions"``). Always names every known block.
#:
#: NOTE ``"conditions"``/``"minions"`` are REAL blocks now -- an earlier
#: version of this stub described them as reserved for spec #2 and "always
#: not_computed", which stopped being true when the side-channel absorption
#: phase landed them. See ``ConditionsBlock``/``MinionsBlock``.
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

class SkillOutcomeCols(TypedDict):
    """The hit-OUTCOME breakdown of one `SkillRow`.

    THREE different hit counts exist across this row and its parent, and
    they are separate fields precisely so a consumer cannot divide by the
    wrong denominator: `attempt_hits` here is GW2EI's own `hits` (every
    non-marker row), `SkillRow["hits"]` is CONTRIBUTING rows (`dmg > 0`),
    and `SkillRow["connected_hits"]` is `HasHit` rows.

    `glance`/`missed`/`evaded`/`blocked` are zero on a condition skill (EI
    zeroes them inside its `if (!IndirectDamage)` guard); `invulned` is
    NOT, because a condition tick can land on an invulnerable target and EI
    counts it."""

    attempt_hits: int
    glance: int
    missed: int
    evaded: int
    blocked: int
    invulned: int
    interrupted: int
    #: This skill produced at least one non-direct (condition) damage row.
    #: Every strike-damage surface downstream uses it as a skip filter.
    indirect: bool

class _SkillRowRequired(TypedDict):
    total: int
    min: int
    max: int
    #: Hit COUNTS, not damage sums.
    crit_hits: int
    flank_hits: int

class SkillRow(_SkillRowRequired, total=False):
    """Mirrors the legacy `SkillEntryOut` (minus `skill_id`, the map key).

    `hits` (CONTRIBUTING rows, `dmg > 0`) is present on player rows and
    absent on ENEMY rows, which come from a different pass that never
    computes it -- absent rather than `0`, so nothing divides `total` by a
    fabricated denominator. `connected_hits` (`HasHit` rows, GW2EI's
    `connectedHits`) is the converse-ish case: it is what axibridge's
    mitigation math divides by.

    `outcomes` is present on PLAYER rows when `skill_damage=True` and
    absent on enemy rows -- absent means "this pass did not measure this
    row", never "every attempt connected"."""

    hits: int
    connected_hits: int
    outcomes: SkillOutcomeCols

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
    direct_count: int
    direct_damage: int
    crit_count: int
    crit_damage: int
    flank_count: int
    glance_count: int
    critable_direct_count: int
    against_downed_damage: int
    missed: int
    evaded: int
    blocked: int
    invulned: int
    applied_total: int
    applied_duration_ms: int
    applied_downs_contribution: int
    applied_duration_downs_contribution_ms: int

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
    #: Breakbar damage this entity DEALT. Outgoing, hence here rather than
    #: on `defenses`, whose `breakbar_count`/`breakbar_damage` are its
    #: INCOMING mirror. Feeds ei-json's `dpsAll[0].breakbarDamage`.
    breakbar_damage_dealt: int

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
    omitted for duration-type boons.

    `states`/`per_source` are what make `boons` a TWO-GATE block like
    `replay`: the uptime numbers above are computed on every parse, these
    two need `timeseries=True`. So `coverage["boons"]` answers the uptime
    question only and is NOT a statement about whether the timelines are
    here -- check for these keys."""

    avg_stacks: float
    states: StateTimeline
    per_source: PerSourceStates

class BoonsBlock(TypedDict):
    """entity id -> buff id -> row. Two levels of real ids, no positional joins."""

    by_entity: Dict[str, Dict[str, BoonRow]]

class SupportEntity(TypedDict):
    """Mirrors the legacy `SupportOut` field-for-field."""

    cleanses: int
    cleanses_self: int
    #: Conditions removed from a MINION owned by a genuine squad player --
    #: the arcdps-parity extra, NOT part of GW2EI's numbers. EI's cleanse
    #: count is ``log.PlayerList``-scoped so it omits pets/minions entirely;
    #: the in-game arcdps meter folds pets into their master and counts them,
    #: hence the ~3-4%% gap. Never folded into ``cleanses``.
    cleanses_minions: int
    #: The in-game arcdps meter's OWN cleanse methodology -- an independent
    #: count, NOT a correction to ``cleanses``/``cleanses_self``/
    #: ``cleanses_minions`` and never to be summed with them. Transcribed
    #: from the reference code arcdps' author published on 2026-08-26.
    #: Three buckets because what the meter displays depends on that
    #: window's "vs npcs"/"from npcs" toggles: this field alone is both-off,
    #: add ``cleanses_arcdps_on_minion`` for "vs npcs" and
    #: ``cleanses_arcdps_by_minion`` for "from npcs".
    cleanses_arcdps: int
    #: "from npcs" adjustment: the remover was this player's pet/minion.
    cleanses_arcdps_by_minion: int
    #: "vs npcs" adjustment: the condition came off a pet/minion.
    cleanses_arcdps_on_minion: int
    strips: int
    #: The strip twin of ``cleanses_arcdps``, same bucketing.
    strips_arcdps: int
    #: "from npcs" adjustment: stripped by this player's pet/minion.
    strips_arcdps_by_minion: int
    #: "vs npcs" adjustment: the boon came off an enemy pet/minion.
    strips_arcdps_on_minion: int
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

class AllyHealingRow(TypedDict):
    """One cell of `HealingDetailCols["by_ally"]`. The healer appears at
    its OWN entity id -- self-healing is one of these cells, exactly as in
    GW2EI, not a separate scalar."""

    healing: int
    #: The subset of `healing` that landed while the ally was downed.
    downed_healing: int
    barrier: int

class _HealSkillRowRequired(TypedDict):
    total: int
    #: Counts EVERY event in the group -- GW2EI's healing dist has no
    #: `HasHit` gate, unlike its damage dist, which is why this row carries
    #: one hit count where `SkillRow` carries three.
    hits: int
    min: int
    max: int
    #: The group contains at least one healing-over-time tick.
    indirect: bool

class HealSkillRow(_HealSkillRowRequired, total=False):
    """One skill's row of EI's `totalHealingDist` / `totalBarrierDist`.

    `total_downed` is omitted when zero, which is ALWAYS on a barrier row:
    `EXTJsonBarrierDist` has no downed field at all, so a zero there would
    invent a measurement GW2EI never makes."""

    total_downed: int

class HealingDetailCols(TypedDict):
    """The three per-ally / per-skill breakdowns of one entity's outgoing
    healing and barrier.

    `by_ally` is keyed by the ALLY's entity id (EI's positional
    `outgoingHealingAllies`/`outgoingBarrierAllies` over `log.Friendlies`).
    Within a PRESENT map, an absent ally is a MEASURED zero; the `Option`
    one level up (`HealingEntity["detail"]` itself) is what carries "not
    measured".

    `barrier_by_skill` is a separate map rather than a column on
    `by_skill`: a skill can appear in one and not the other, and merging
    them would force every healing row to publish a barrier it never
    measured."""

    by_ally: Dict[str, AllyHealingRow]
    by_skill: Dict[str, HealSkillRow]
    barrier_by_skill: Dict[str, HealSkillRow]

class _HealingEntityRequired(TypedDict):
    outgoing_total: int
    outgoing_allies: int
    outgoing_self: int
    barrier_out: int
    downed_healing_out: int
    # Whether this player's OWN arcdps healing-stats addon reported --
    # GW2EI's `RunningExtension` roster membership. NOT implied by
    # `outgoing_total > 0`: a peer's addon can relay heals on a player's
    # behalf, so real numbers with `runs_extension = False` mean "partial,
    # someone else saw this", which is the distinction to surface.
    runs_extension: bool

class HealingEntity(_HealingEntityRequired, total=False):
    """Mirrors the legacy `HealingOut`, plus the gated `detail`
    breakdowns. `detail` is omitted when that pass did not run.

    Note the cumulative healing SERIES is not here -- it lives at
    `EntitySeries["healing_1s"]`, because what a field belongs to in this
    format is its grid and its gate, not its subject matter."""

    detail: HealingDetailCols

class HealingExtensionDesc(TypedDict):
    """GW2EI's `ExtensionDesc` minus `runningExtension` and `name` -- this
    descriptor hangs off the healing block, so which extension it describes
    is never in question. `version` is the addon's self-reported string
    (e.g. `"2.16rc1"`), or `"Unknown"`."""

    version: str
    revision: int
    signature: int

class _HealingBlockRequired(TypedDict):
    by_entity: Dict[str, HealingEntity]

class HealingBlock(_HealingBlockRequired, total=False):
    """`extension` is absent only on a block that somehow exists without a
    registration row; the whole block is omitted when the extension is
    absent, so in practice it is present."""

    extension: HealingExtensionDesc

# --- conditions / minions blocks (native 1.0) ------------------------------

class ConditionRow(TypedDict):
    """One condition on one ENEMY entity: who applied it, and when it was
    up.

    There is deliberately no sibling `states` total here, unlike
    `BoonRow`: the enemy-side pass computes only the source split, and
    summing the sources would NOT reconstruct a fused total (two appliers
    holding the same duration condition overlap rather than stack)."""

    per_source: PerSourceStates

class ConditionsBlock(TypedDict):
    """enemy entity id -> condition buff id -> row. The condition id
    resolves through `Catalogs["buffs"]`."""

    by_entity: Dict[str, Dict[str, ConditionRow]]

class _SelfEffectRowRequired(TypedDict):
    uptime_pct: float
    states: StateTimeline

class SelfEffectRow(_SelfEffectRowRequired, total=False):
    """One condition or control effect held BY a squad player.

    The squad-side counterpart to `ConditionRow` (enemy-side) and the
    missing half of `BoonRow` (squad-side, but only the 12 boons).
    `CcBlock` is not a substitute -- it counts crowd-control events, which
    carries no timeline.

    `avg_stacks` is present for intensity-stacking effects (the 6
    `BuffStackType.Stacking` conditions) and omitted for duration ones,
    the same rule
    `BoonRow.avg_stacks` follows. `states` is REQUIRED, unlike
    `BoonRow.states`: this whole block rides one gate, so if the block is
    here the timeline is."""

    avg_stacks: float

class SelfEffectsBlock(TypedDict):
    """squad entity id -> buff id -> row, for the 14 conditions plus Stun
    (872) and Daze (833). Wholly gated on `timeseries=True`."""

    by_entity: Dict[str, Dict[str, SelfEffectRow]]

class _SquadBuffRowRequired(TypedDict):
    uptime_pct: float

class SquadBuffRow(_SquadBuffRowRequired, total=False):
    """One squad player's uptime for one non-boon, non-condition buff.

    No `states`: nothing plots a sigil's stack count over time, and a
    timeline per player per buff would multiply this block's payload by an
    order of magnitude for a graph no consumer draws.

    `avg_stacks` is present for intensity-stacking buffs and omitted for
    duration ones, the same rule `BoonRow.avg_stacks` and
    `SelfEffectRow.avg_stacks` follow."""

    avg_stacks: float

class SquadBuffsBlock(TypedDict):
    """squad entity id -> buff id -> row, for every buff that is neither one
    of the 12 boons nor a condition/control effect: sigils, relics, food,
    utilities, auras, signets, trait buffs.

    The third piece of the population Elite Insights keeps in one
    `buffUptimes` array, and the only one of the three that is ALWAYS-ON --
    it emits uptime only, at the cost `boons`' own always-on half already
    carries, so no option gates it. The three id sets are disjoint by
    construction, which is what lets a consumer concatenate them."""

    by_entity: Dict[str, Dict[str, SquadBuffRow]]

class MinionSkillTakenRow(TypedDict):
    """One minion group's damage-TAKEN row for one skill.

    Deliberately NOT a `SkillRow`: that row carries `crit_hits`/
    `flank_hits`, which a damage-taken rollup does not have, while this one
    carries the outcome counters inline rather than nested."""

    total: int
    #: Attempts, marker rows excluded.
    hits: int
    #: `HasHit` rows only.
    connected_hits: int
    min: int
    max: int
    blocked: int
    evaded: int
    glance: int
    missed: int
    invulned: int
    interrupted: int
    #: Condition damage rather than strike damage.
    indirect: bool

class MinionRow(TypedDict):
    """One minion group belonging to one player. `minion_id` resolves
    through `Catalogs["minions"]`, which carries the species id and name."""

    minion_id: int
    #: The damage this species took, keyed by skill id.
    taken: Dict[str, MinionSkillTakenRow]

class MinionsBlock(TypedDict):
    """Per-player minion damage-taken rollups, gated on
    `skill_damage=True`. One entry per player that HAS minions -- a player
    with none is absent rather than carrying an empty list."""

    by_entity: Dict[str, List[MinionRow]]

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

    #: SPEC name (``"Firebrand"``, matching an entity's ``elite_spec`` or,
    #: for a core build, its ``profession``) -> the signed modifier ids
    #: that belong to that spec rather than to the shared pool -- relics,
    #: food, squad buffs, whose gain every benefiting player is credited
    #: with. Elite Insights' top-level ``personalDamageMods``.
    #:
    #: Omitted when the classification is unavailable. Read an absent or
    #: empty map as UNCLASSIFIED, never as "nothing is personal":
    #: filtering on the latter reading hides every modifier there is.
    personal: Dict[str, List[int]]

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
    #: Disconnect/not-yet-spawned windows, half-open like the two above.
    #: See `ReplayIntervals.dc` for the GW2EI divergence and the log-end
    #: rule.
    dc_intervals: List[List[int]]

class _ReplayIntervalsRequired(TypedDict):
    start_ms: int
    end_ms: int
    #: Subtracts DEAD time only, not down time -- GW2EI's own definition.
    active_ms: int
    down: List[List[int]]
    dead: List[List[int]]
    #: Disconnect/not-yet-spawned windows (`CBTS_DESPAWN` to the matching
    #: `CBTS_SPAWN`), half-open and NOT mutually exclusive with
    #: `down`/`dead`. An agent still disconnected at log end gets NO
    #: interval for that trailing window -- it is dropped, not closed -- so
    #: `end_ms - start_ms` minus summed `dc` over-counts activity for anyone
    #: who disconnects and never returns. Use `active_ms`.
    dc: List[List[int]]

class ReplayIntervals(_ReplayIntervalsRequired, total=False):
    """One SQUAD entity's activity window, computed on EVERY parse.

    `dist_to_com`/`stack_dist` are GW2EI's `distToCom`/`stackDist` -- mean
    distance, in world inches, to the commander and to the squad centre.
    Three states, and the first two must not be collapsed: OMITTED means the
    position pass never ran (`replay=True` was not passed, nothing was
    measured); `-1.0` means the pass ran and nothing qualified (GW2EI's own
    sentinel); `>= 0.0` is a real distance (`0.0` is legitimate -- the
    commander's own value).

    These two scalars are the only part of this row that depends on the
    `replay=True` gate; every interval field above is computed on every
    parse and is identical with and without it."""

    dist_to_com: float
    stack_dist: float

class _ReplayTracksRequired(TypedDict):
    #: Shared polling interval for every track.
    poll_ms: int
    #: Keyed by entity id. WIDER than `ReplayBlock.by_entity`: enemy players
    #: appear here too.
    by_entity: Dict[str, ReplayTrack]

class Arena(TypedDict):
    """The fixed world rectangle a WvW map's arena image covers, plus the
    image itself: everything needed to project `ReplayTrack["samples"]`
    onto a map without knowing any GW2 map geometry.

    Samples are raw world (game-inch) coordinates -- what arcdps records,
    and independent of anybody's canvas. World y grows northward and image
    y grows downward, so the y axis flips::

        px = (x - a["world_min_x"]) / (a["world_max_x"] - a["world_min_x"]) * a["image_width"]
        py = (1 - (y - a["world_min_y"]) / (a["world_max_y"] - a["world_min_y"])) * a["image_height"]

    Scale both by ``canvas / image_*`` to render at any size. Nothing here
    is pre-rounded or pre-rescaled.
    """

    #: The arena image's native width in pixels.
    image_width: int
    #: The arena image's native height in pixels.
    image_height: int
    image_url: str
    #: World (game-inch) x of the image's LEFT edge.
    world_min_x: float
    #: World y of the image's BOTTOM edge -- the LARGER `py`, per the flip.
    world_min_y: float
    #: World x of the image's RIGHT edge.
    world_max_x: float
    #: World y of the image's TOP edge.
    world_max_y: float

class ReplayTracks(_ReplayTracksRequired, total=False):
    """The gated half of `ReplayBlock` -- present only under
    `replay=True`. `bounds` is omitted (not `None`) when there is nothing
    to bound. `arena` is omitted for a map id with no hand-authored arena
    image; you then have only `bounds`, which is the union of the OBSERVED
    positions rather than a fixed frame, and so is not comparable between
    two logs on the same map."""

    bounds: ReplayBounds
    arena: Arena

class _ReplayBlockRequired(TypedDict):
    #: Keyed by entity id. Squad players only.
    by_entity: Dict[str, ReplayIntervals]

class ReplayBlock(_ReplayBlockRequired, total=False):
    """Two halves on two different gates -- the only block in the format
    shaped this way. `by_entity` (down/dead/dc intervals, squad only) is
    computed on every parse; `tracks` (positions) needs `replay=True`. So
    `coverage["replay"] == "present"` does NOT mean positions are available
    -- check for `tracks`.

    The four eye-candy families below (`gliding`, `transformations`,
    `captures`, `decorations`) ride NEITHER gate -- they are computed on
    every parse and are simply omitted when the log has none."""

    tracks: ReplayTracks
    #: Glider deploy/stow windows. A flat list rather than an entity-keyed
    #: map: `CBTS_GLIDER` is not restricted to the squad, so a window can
    #: belong to an agent that never becomes a tracked entity. `agent_addr`
    #: is always present, `entity_id` only when the join resolves.
    gliding: List[GliderOut]
    #: Transformation (mount/tonic/form) windows. Same shape rules as
    #: `gliding`.
    transformations: List[TransformationOut]
    #: Capture-point areas as decoded. Absent on every log written before
    #: arcdps build 20260602, which does not emit the family at all.
    captures: List[CaptureOut]
    #: The renderable projection of `captures`. Neither form reconstructs
    #: the other.
    decorations: List[DecorationOut]

class _GliderOutRequired(TypedDict):
    agent_addr: int
    start_ms: int

class GliderOut(_GliderOutRequired, total=False):
    """One glider deployment. A MISSING `end_ms` means the glider was still
    deployed at the last event in the log -- not that it closed at log
    end."""

    entity_id: int
    end_ms: int

class _TransformationOutRequired(TypedDict):
    agent_addr: int
    #: The arcdps SESSION-LOCAL id -- meaningless across logs on its own.
    #: `guid` is the portable identity.
    transformation_id: int
    start_ms: int

class TransformationOut(_TransformationOutRequired, total=False):
    """One transformation window. `guid` is omitted when the log carried no
    `CBTS_IDTOGUID` mapping for this id."""

    entity_id: int
    guid: str
    end_ms: int

#: The arcdps "wrbg" capture owner: `"white"` (unowned), `"red"`, `"blue"`,
#: `"green"`. An owner index arcdps adds later serializes as `"unknown_<n>"`
#: rather than folding into `"white"`, which is why this is open `str` and
#: not a closed literal set.
CaptureOwner = str

class _CaptureOutRequired(TypedDict):
    agent_addr: int
    start_ms: int
    original_owner: CaptureOwner
    owner_states: List[OwnerStateOut]
    progress_states: List[ProgressStateOut]

class CaptureOut(_CaptureOutRequired, total=False):
    """One capture-point area over its lifetime.

    `entity_id` almost never resolves -- a capture point is a gadget, not a
    tracked entity. `end_ms` is omitted when the area never got a hide row
    and no later show superseded it; that is deliberately NOT defaulted to
    the gadget's last-aware time here, because the substitution is a
    rendering decision and is made in `DecorationOut`. `shape` is omitted
    when no geometry row ever arrived, and such an area produces no
    decoration."""

    entity_id: int
    end_ms: int
    shape: CaptureShapeOut

class CaptureShapeOut(TypedDict, total=False):
    """The capture area's geometry, in WORLD coordinates -- the decoration
    form carries polygon vertices relative to the anchor instead.

    `kind` selects which of the other two keys is present: `"circle"` gives
    `radius` (the arcdps single-point overload -- a radius around the
    gadget, NOT a degenerate polygon), `"polygon"` gives `points` as
    `[x, y]` pairs."""

    kind: str  # Literal["circle", "polygon"]
    radius: float
    points: List[List[float]]

#: At `time_ms` the area was held by `from` and being taken by `by`.
#:
#: Declared with the functional `TypedDict` syntax because `from` is a
#: Python keyword and cannot be a class-body field name. Read it as
#: `state["from"]`.
OwnerStateOut = TypedDict(
    "OwnerStateOut", {"time_ms": int, "from": CaptureOwner, "by": CaptureOwner}
)

#: A run of progress samples sharing one owner pair. `decaying` means `by`
#: is nobody, so the bar is falling back toward `from` rather than being
#: captured. `progress` is `[time_ms, percent]` pairs, percent in 0..100 at
#: 2 decimal places. Same `from`-is-a-keyword note as `OwnerStateOut`.
ProgressStateOut = TypedDict(
    "ProgressStateOut",
    {
        "from": CaptureOwner,
        "by": CaptureOwner,
        "decaying": bool,
        "progress": List[List[float]],
    },
)

class _DecorationOutRequired(TypedDict):
    kind: str  # Literal["capture_outline", "capture_progress"]
    #: Log-relative ms, SIGNED. Unlike every other time in this format this
    #: can be negative by exactly one millisecond: the capture-progress
    #: splitter synthesizes a sample at `time - 1`.
    start_ms: int
    end_ms: int
    #: World-space `[x, y]` the shape is drawn around.
    anchor: List[float]
    #: CSS `rgba(...)`. For a progress bar the two colour slots do NOT have
    #: fixed owner roles: capturing puts the capper here, decaying the
    #: holder.
    color: str
    shape: DecorationShapeOut

class DecorationOut(_DecorationOutRequired, total=False):
    """One drawable environment decoration."""

    secondary_color: str

class DecorationShapeOut(TypedDict, total=False):
    """A decoration's geometry, RELATIVE to `DecorationOut["anchor"]` --
    unlike `CaptureShapeOut`, which is in world coordinates.

    `kind` selects which of the other keys are present: `"circle"` gives
    `radius`/`filled`, `"polygon"` gives `points`/`filled`, and
    `"progress_bar"` gives `width`/`height`/`progress`."""

    kind: str  # Literal["circle", "polygon", "progress_bar"]
    radius: float
    points: List[List[float]]
    filled: bool
    width: int
    height: int
    progress: List[List[float]]

class SquadSeries(TypedDict):
    damage: SeriesOut
    cc_applied: SeriesOut
    downs: SeriesOut
    #: Boons the squad removed from enemies, per second. Folded from the
    #: same `support::outgoing_boon_strips` primitive as the `strips`
    #: scalar, so this lane sums to the squad total by construction.
    strips: SeriesOut

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
    """Mirrors the legacy `PlayerPerSecondOut`, plus three optional series.
    `per_target` is keyed by the TARGET's entity id, omitted (not `{}`)
    when empty.

    `power_damage` is the non-condition half of OUTGOING `damage`. In
    practice it is present only on ENEMY rows: no pass computes an outgoing
    power split for players. Absent means "no pass measured this", which a
    zero-filled series would misreport as "measured, and it was all
    condition damage".

    `healing_1s` is cumulative outgoing healing (EI's
    `extHealingStats.healing1S`). Absent for enemies, and for everyone on a
    log with no healing extension.

    `healing_received_1s` and `barrier_received_1s` are the receiver-indexed
    counterparts of `healing_1s`, on the same grid and the same gate. Both
    are ALLY-ATTRIBUTED, unlike `healing_1s`: a heal/barrier only lands
    here when its recipient is one of the tracked players, so these are
    incoming amounts from tracked recipients, not the total incoming
    amount.

    `health_percents` is `[[time_ms, percent], ...]` -- a STEP function,
    which is why it is a plain pair list rather than a `SeriesOut`:
    re-sampling it onto a fixed grid would either invent readings between
    updates or lose updates inside a bucket. A value holds until the next
    pair. Absent when the entity emitted no health updates at all (EI omits
    `healthPercents` for such a player rather than writing `[]`)."""

    per_target: Dict[str, TargetSeries]
    power_damage: SeriesOut
    health_percents: List[List[float]]
    healing_1s: SeriesOut
    healing_received_1s: SeriesOut
    barrier_received_1s: SeriesOut

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
    conditions: ConditionsBlock
    self_effects: SelfEffectsBlock
    squad_buffs: SquadBuffsBlock
    minions: MinionsBlock

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
    everything: bool = False,
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

    `everything` is the SDK mirror of the CLI's `--all`: compute every
    analysis pass this version knows about. Deliberately defined as
    "everything that exists in this version" rather than as a fixed option
    list, so a caller that sets it keeps getting complete documents as
    later versions add passes -- the first axibridge cutover audit found 30
    blank fields caused by exactly the opposite. It is a UNION with the
    individual options, never an override.

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
    everything: bool = False,
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

    `everything` is the SDK mirror of the CLI's `--all`: compute every
    analysis pass this version knows about, a UNION with the individual
    options rather than an override. See `parse_file` for why it is
    defined that way.

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
    everything: bool = False,
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

    `everything` is the SDK mirror of the CLI's `--all`: compute every
    analysis pass this version knows about, a UNION with the individual
    options rather than an override. See `parse_file` for why it is
    defined that way.

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
