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

class DamageOut(TypedDict):
    total: int
    dps: float
    per_enemy: List[PerEnemyOut]

class CcOut(TypedDict):
    applied_total: int
    applied_duration_ms: int
    stun_breaks: int
    removed_stun_duration_ms: int

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
    resurrects: int

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
    down_contribution: int
    downs_taken: int
    deaths: int
    damage_taken: int
    cc: CcOut
    boons: List[BoonOut]
    support: SupportOut

class PlayerOut(_PlayerOutRequired, total=False):
    """`marker`/`commander_tag` are omitted (not `null`) when absent."""

    marker: str
    commander_tag: CommanderTagOut

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

# --- top-level report -----------------------------------------------------

class _ReportRequired(TypedDict):
    schema_version: str
    axilog_version: str
    encounter: EncounterOut
    players: List[PlayerOut]
    enemies: List[EnemyOut]
    timeline: TimelineOut

class Report(_ReportRequired, total=False):
    """`warnings` is omitted (not `[]`) when there are no analysis warnings.
    `replay` is omitted (not `None`) unless requested via `replay=True`."""

    warnings: List[str]
    replay: ReplayOut

# --- module functions -----------------------------------------------------

def parse_file(path: str, replay: bool = False) -> Report:
    """Parse a `.evtc`/`.zevtc` file at `path` into the native `Report` shape.

    `replay` (M9, Task 2) opts into embedding the native combat-replay
    block (`Report["replay"]`); defaults to `False`.

    Raises `OSError` if `path` cannot be read, `ValueError` if the bytes
    are not a decodable/parseable arcdps log.
    """
    ...

def parse_bytes(data: bytes, replay: bool = False) -> Report:
    """Parse an already-read `.evtc`/`.zevtc` buffer into the native `Report` shape.

    `replay` (M9, Task 2) opts into embedding the native combat-replay block.

    Raises `ValueError` if `data` is not a decodable/parseable arcdps log.
    """
    ...

def parse_file_ei(path: str) -> Dict[str, Any]:
    """Parse a `.evtc`/`.zevtc` file at `path` into Elite Insights-compatibility JSON.

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
