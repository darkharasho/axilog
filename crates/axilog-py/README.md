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
python3 -m venv .venv
.venv/bin/pip install maturin
.venv/bin/maturin develop --release   # builds the extension into .venv
```

`maturin develop` compiles this crate to a native extension module and
installs it editable into whichever interpreter you point it at (here,
`.venv`). Re-run it after changing `src/lib.rs` or `axilog.pyi`.

## Usage

```python
import axilog

# Native report (axilog_schema::Report — schema_version, encounter,
# players[], enemies[], timeline, warnings), returned as a plain dict
# with the same snake_case keys the Rust struct serializes (see "Types"
# below for the typed shape).
report = axilog.parse_file("./fight.zevtc")
print(report["schema_version"], len(report["players"]))

squad_damage = sum(p["damage"]["total"] for p in report["players"])
quickness = next(
    b for b in report["players"][0]["boons"] if b["name"] == "Quickness"
)
print(squad_damage, quickness["presence_pct"])

# Same pipeline, from an already-read buffer.
with open("./fight.zevtc", "rb") as f:
    report_from_bytes = axilog.parse_bytes(f.read())

# Elite Insights-compatibility JSON (axilog_ei::to_ei_json) — the shape
# axibridge-style consumers read (players[].account, dpsAll[0].damage,
# support[0].condiCleanse, buffUptimes[], targets[].enemyPlayer,
# wvWMapData.{red,blue,green}TeamID). Returned as `Dict[str, Any]` — see
# "Types" below, this shape isn't fully typed.
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

`axilog.pyi` hand-transcribes every `#[derive(Serialize)] struct` in
`crates/axilog-schema/src/lib.rs` (the source of truth — keep this file
in sync with that one if the schema changes; mirrors
`crates/axilog-node/types.d.ts`'s equivalent transcription for the Node
SDK) into `TypedDict`s, with `Optional`/omitted-key semantics matching
each field's serde `Option`/`skip_serializing_if` attributes exactly (see
the top of `axilog.pyi` for the precise rule). `parse_file`/`parse_bytes`
are typed to return `Report`; `parse_file_ei`'s materially different,
larger EI-JSON shape is intentionally left `Dict[str, Any]` — typing it
faithfully is out of scope for this task, mirrors `types.d.ts`'s same
call for `parseFileEi`.

## Tests

`tests/test_sdk.py` (stdlib `unittest` only, no pytest/other
dependencies) covers, against the committed PII-safe fixture
`fixtures/wvw-small.anon.zevtc` (repo-root relative, resolved from the
test file's own location so it runs correctly from any cwd):

- `parse_file`: `schema_version`, player count, squad damage total, and
  the four support-stat squad sums — all pinned to this fixture's exact
  calibrated values (see `crates/axilog-core/tests/golden.rs`/
  `support_golden.rs` for how those numbers were derived against a real
  dps.report EI export). One boon uptime value (`Quickness` for account
  `:Anon132.5884`) is cross-checked live against
  `fixtures/wvw-small.ei.json`'s `players[].boons["1187"].uptime`, rather
  than only pinned to a hardcoded literal.
- `parse_bytes` equals `parse_file`.
- `parse_file_ei`: the specific keys axibridge-style consumers read
  (`players[].account`, `dpsAll[0].damage`, `support[0].condiCleanse`,
  non-empty `buffUptimes`, `targets[].enemyPlayer` booleans,
  `wvWMapData`'s three team ids).
- `anonymize_file` round-trip: writes to a tempfile directory, re-parses,
  checks player count/metrics are unchanged (anonymization only rewrites
  names).
- Error paths: a missing file raises `OSError`; corrupt bytes raise
  `ValueError` — both with a non-empty message.
- **CLI parity**: shells out to the already-built CLI
  (`cargo build -p axilog-cli`; run that first if `target/debug/axilog`
  is missing) and asserts `axilog parse <fixture> --format json`,
  `json.load`ed, exactly equals `parse_file`'s return value — with a
  small first-N-differing-paths diff helper so a future divergence prints
  something readable instead of two giant dicts.

Canonical invocation (from the repo root, using the venv interpreter the
module was built into — this is the command to wire into CI):

```sh
crates/axilog-py/.venv/bin/python -m unittest discover -s crates/axilog-py/tests
```

Equivalently, from this directory: `.venv/bin/python -m unittest
discover -s tests`.
