# axilog (Python)

Python bindings ([PyO3](https://pyo3.rs)) for axilog's Rust GW2 arcdps WvW
parsing core. Every export is a thin wrapper around the same
decode → resolve → analyze → build_report pipeline the CLI (`axilog-cli`)
and the Node SDK (`crates/axilog-node`) drive — see `src/lib.rs`'s module
doc for the exact FFI contract (no Rust panic ever crosses the boundary;
every fallible step becomes a Python `OSError`/`ValueError` carrying the
underlying Rust error text).

## Build

```sh
cd crates/axilog-py
python3 -m venv .venv                 # gitignored; may live anywhere
.venv/bin/pip install maturin
.venv/bin/maturin develop --release   # builds the extension into .venv
```

`maturin develop` compiles this crate to a native extension module and
installs it editable into whichever interpreter you point it at. Re-run it
after changing `src/lib.rs` or `axilog.pyi`.

To build into a venv outside the crate — worth doing on a machine where the
repo lives on a small or slow volume — set `VIRTUAL_ENV` so maturin targets
it instead of guessing:

```sh
VIRTUAL_ENV=/path/to/venv /path/to/venv/bin/maturin develop --release
```

## Usage

`parse_file`/`parse_bytes` return the **native 1.0 container** (`schema:
"1.0"`), as a plain dict of dicts and lists. Its six top-level keys are
always present:

| Key | What it is |
| --- | --- |
| `axilog` | `{ schema, version, generated_from }` — format contract version, the binary that produced the document, and the input log's file *name* |
| `encounter` | One object: `kind`, `map`, `duration_ms`, `teams[]`, `markers[]`, `objectives[]`, `recorded_by`, … |
| `entities` | The single roster — every tracked agent, identity only, no statistics |
| `catalogs` | `skills`/`buffs`/`damage_mods`/`minions` metadata, referenced by id from `blocks` |
| `blocks` | Id-keyed statistics maps — `damage`, `defenses`, `boons`, `support`, … |
| `coverage` | Per-block status: `present`, `empty`, `not_computed` or `unsupported` |

A seventh key, `warnings`, appears only when non-empty.

There is **no top-level `players[]` and no `schema_version`**. The roster is
`entities[]`, and every per-entity statistic is a `blocks.<name>.by_entity`
map keyed by `entities[].id` — as a **string**, since JSON object keys are
strings while `entities[].id` is an int.

```python
import axilog

report = axilog.parse_file("./fight.zevtc")

print(report["axilog"])
# {'generated_from': 'wvw-small.anon.zevtc', 'schema': '1.0', 'version': '0.3.2'}

print(report["encounter"]["map"], report["encounter"]["duration_ms"])
# Green Alpine Borderlands 49285

# The roster is filtered by `role`, not by a separate list. WvW logs carry
# four: "squad", "friendly_player", "enemy_player" and "npc".
squad = [e for e in report["entities"] if e["role"] == "squad"]
print(len(squad), "squad of", len(report["entities"]), "entities")
# 38 squad of 122 entities

damage = report["blocks"]["damage"]
print(damage["squad"]["total"])
# 2138414

top = max(squad, key=lambda e: damage["by_entity"][str(e["id"])]["total"])
row = damage["by_entity"][str(top["id"])]
print(top["account"], top["profession"], row["total"], round(row["dps"], 1))
# :Anon104.4848 Engineer 205612 4171.9

# Buffs are id-keyed too; names live once in the catalog, never repeated
# per entity.
boons = report["blocks"]["boons"]["by_entity"][str(top["id"])]
print(report["catalogs"]["buffs"]["1187"]["name"], round(boons["1187"]["uptime_pct"], 1))
# Quickness 6.0

# Same pipeline, from an already-read buffer.
with open("./fight.zevtc", "rb") as f:
    report_from_bytes = axilog.parse_bytes(f.read())
```

### Check `coverage` before reading a block

`coverage` is how a consumer tells "the flag was off" apart from "the log
had nothing to say", which is otherwise indistinguishable — both look like
a missing block.

```python
report = axilog.parse_file("./fight.zevtc")
print(report["coverage"]["rotation"], report["coverage"]["damage_mods"])
# empty not_computed
```

| Value | Meaning | What to do |
| --- | --- | --- |
| `present` | Computed, has rows | Read it |
| `empty` | Computed, genuinely nothing to report | Treat as zero rows, not an error. The block is still carried in `blocks` |
| `not_computed` | The gate for it was off | Re-parse with the gate on. Do **not** report this as "the log had none" |
| `unsupported` | This log cannot produce it — its era, encounter kind, or recorder | Do not retry with a gate; see `docs/EI-PARITY.md` |

### Opt-in gates

Seven keyword arguments, all `False` by default:

| Keyword | Adds |
| --- | --- |
| `skill_damage=True` | `blocks.damage.by_entity[].by_skill` / `.by_skill_taken` — per-skill outgoing and incoming splits |
| `timeseries=True` | `blocks.series` per-entity channels, the buff stack timelines in `blocks.boons`/`blocks.conditions`, and all of `blocks.self_effects` — squad-side condition/Stun/Daze uptime and stack timelines, which is gated on this flag in its entirety |
| `rotation=True` | `blocks.rotation.by_entity[].{casts, aftercast}` |
| `replay=True` | `blocks.replay.tracks` — position samples. The down/dead intervals under `blocks.replay.by_entity` are computed on **every** parse; only the positions are gated |
| `missiles=True` | `blocks.missiles` — projectile fired/hit/denied counts |
| `modifiers=True` | `blocks.damage_mods` and `catalogs.damage_mods` |
| `everything=True` | Every analysis pass **this version** knows about |

```python
report = axilog.parse_file("./fight.zevtc", skill_damage=True, rotation=True)

row = report["blocks"]["damage"]["by_entity"]["1"]
print(sorted(row.keys()))
# ['breakbar_damage_dealt', 'by_skill', 'by_skill_taken', 'downs_dealt',
#  'dps', 'kills_dealt', 'per_target', 'taken', 'total']
```

Prefer `everything=True` over enumerating gates if you want complete
documents: it is defined as "everything that exists in this version", so a
pass added by a later release is included automatically. The first axibridge
cutover audit found 30 blank fields caused by exactly the opposite — a
consumer's option list drifting from the parser's. It is a union with the
individual gates, never an override.

Gates are not free, and they are not uniformly the same kind of cost.
`skill_damage`, `timeseries` and `rotation` only control whether an
already-computed result is serialized; `replay` and `modifiers` gate the
computation itself. Measured on the committed fixture
(`fixtures/wvw-small.anon.zevtc` — 38 squad entities, 49.3 s — release
build, compact JSON, 461,086-byte baseline at 0.07 s):

| Gate | Bytes | Wall |
| --- | --- | --- |
| *(none)* | 461,086 | 0.07 s |
| `missiles` | 466,565 | 0.07 s |
| `rotation` | 708,367 | 0.07 s |
| `modifiers` | 616,591 | 0.08 s |
| `replay` | 1,679,284 | 0.12 s |
| `skill_damage` | 3,338,819 | 0.08 s |
| `timeseries` | 3,563,403 | 0.09 s |
| `everything` | 8,067,599 | 0.16 s |

That fixture is a 49-second skirmish. The per-skill and per-second blocks
are combinatorial — entity × target × skill and entity × target × second —
so on a real multi-minute zerg log they grow far faster than the table
above suggests. See `docs/BENCHMARKS.md` for real-log numbers.

### The other two functions

```python
# Elite Insights-compatibility JSON (axilog_ei::to_ei_json) — the shape
# axibridge-style consumers read (players[].account, dpsAll[0].damage,
# support[0].condiCleanse, buffUptimes[], targets[].enemyPlayer,
# wvWMapData.{red,blue,green}TeamID). Returned as `Dict[str, Any]`; unlike
# the native container this one does still have a flat `players[]`, because
# EI's shape does. Gates are keyword-only and mirror `parse_file`'s, except
# `missiles`, which is accepted for signature parity and has no effect (EI's
# shape has no comparable field).
ei = axilog.parse_file_ei("./fight.zevtc")

# Rewrite every player's character/account name in a .zevtc to a
# deterministic Anon<N> placeholder (does not mutate metrics — safe for
# producing PII-safe fixtures). Returns the number of player agents
# rewritten.
rewritten = axilog.anonymize_file("./fight.zevtc", "./fight.anon.zevtc")
```

All four functions raise a real Python exception (never a raw Rust
panic) on failure: `OSError` for a file that can't be read/written,
`ValueError` for bytes that don't decode/parse as an arcdps log.

### Further reference

`docs/NATIVE-FORMAT.md` in the repo root is the authoritative field-level
reference for the 1.0 container — the `entities[].role` rules, the catalog
join semantics, the series RLE envelope, the two replay halves, and the 1.x
compatibility rules a consumer can rely on. This README is the SDK surface;
that document is the format.

## Types

This crate has no `python-source` tree — the compiled extension module
*is* the package (see `src/lib.rs`'s `#[pymodule] fn axilog`). maturin
(>=1.5) auto-detects a `<module_name>.pyi` stub and a `py.typed` marker
sitting next to `Cargo.toml`/`pyproject.toml` at the crate root and
bundles both into the wheel as `axilog/__init__.pyi` and
`axilog/py.typed`, alongside the compiled `axilog/axilog.abi3.so` and the
auto-generated `axilog/__init__.py` — no `[tool.maturin]` include/
package-data configuration needed (verified directly against this crate:
`maturin build` logs `Found type stub file at axilog.pyi`, and the built
wheel contains `axilog/__init__.pyi` + `axilog/py.typed`).

`axilog.pyi` hand-transcribes the serialized shape into `TypedDict`s, with
`Optional`/omitted-key semantics matching each field's serde
`Option`/`skip_serializing_if` attributes exactly (see the top of
`axilog.pyi` for the precise rule). The 1.0 container's types live under the
`# native format 1.0` heading — `ReportV1`, `AxilogMeta`, `EncounterOutV1`,
`EntityOut`, `Catalogs`, `Coverage`, `SeriesOut` and friends, transcribed
from `crates/axilog-schema/src/v1/`. It mirrors
`crates/axilog-node/types.d.ts`'s equivalent transcription for the Node SDK;
keep the two in step.

`parse_file`/`parse_bytes` are typed to return `ReportV1`. `parse_file_ei`'s
materially different, larger EI-JSON shape is intentionally left
`Dict[str, Any]` — typing it faithfully is out of scope, mirroring
`types.d.ts`'s same call for `parseFileEi`.

## Tests

`tests/test_sdk.py` (stdlib `unittest` only, no pytest/other
dependencies) covers, against the committed PII-safe fixture
`fixtures/wvw-small.anon.zevtc` (repo-root relative, resolved from the
test file's own location so it runs correctly from any cwd):

- `parse_file`: the six-key container shape, player count, squad damage
  total, and the four support-stat squad sums — all pinned to this
  fixture's exact calibrated values (see
  `crates/axilog-core/tests/golden.rs`/`support_golden.rs` for how those
  numbers were derived against a real dps.report EI export). One boon
  uptime value is cross-checked live against
  `fixtures/wvw-small.ei.json`'s `players[].boons["1187"].uptime`, rather
  than only pinned to a hardcoded literal.
- `parse_bytes` equals `parse_file`.
- `replay`, `skill_damage`, `timeseries`, `missiles` and `modifiers`:
  absent by default, present and correctly shaped when requested, and
  absent again when passed explicitly `False`. `replay`'s test
  additionally pins the split halves — intervals always on, `tracks`
  gated — and asserts every track id joins to an `entities[]` row.
  **`rotation` has no such test** — in either SDK. Its shape and size are
  guarded schema-side (`crates/axilog-schema/tests/v1_shape.rs`'s keyset
  golden and `v1_size.rs`, both of which drive every gate), but there is
  no absent-by-default/present-when-requested assertion for it at the SDK
  boundary. Known gap.
- `everything=True`: every gate computes, leaving nothing `not_computed`
  in `coverage`.
- `parse_file_ei`: the specific keys axibridge-style consumers read
  (`players[].account`, `dpsAll[0].damage`, `support[0].condiCleanse`,
  non-empty `buffUptimes`, `targets[].enemyPlayer` booleans,
  `wvWMapData`'s three team ids).
- `anonymize_file` round-trip: writes to a tempfile directory, re-parses,
  checks entity count/metrics are unchanged (anonymization only rewrites
  names).
- Error paths: a missing file raises `OSError`; corrupt bytes raise
  `ValueError` — both with a non-empty message.
- **CLI parity**: shells out to the already-built CLI
  (`cargo build -p axilog-cli`; run that first if `target/debug/axilog`
  is missing) and asserts `axilog parse <fixture> --format json`,
  `json.load`ed, exactly equals `parse_file`'s return value — with a
  small first-N-differing-paths diff helper so a future divergence prints
  something readable instead of two giant dicts.

Canonical invocation, using the interpreter the module was built into:

```sh
.venv/bin/python -m unittest discover -s tests
```

Equivalently from the repo root, `crates/axilog-py/.venv/bin/python -m
unittest discover -s crates/axilog-py/tests`. Substitute your own venv path
if you built outside the crate.
