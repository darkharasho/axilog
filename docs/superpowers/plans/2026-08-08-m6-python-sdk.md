# axilog M6 — Python SDK Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** PyO3/maturin Python module `axilog` (parse_file/parse_bytes/parse_file_ei/
anonymize_file), typed stubs, zero-dep unittest suite with CLI parity, CI.

## Global Constraints

- Mirror the M5 Node SDK discipline: additive crate; serde key names verbatim in returned dicts;
  no panics across FFI (map to Python exceptions); tests use the committed anon fixture; parity
  gate vs the CLI JSON. `cargo test --workspace` (188) stays green after each task.
- Python: use system python3 (verify ≥3.9); maturin for builds (`pipx`/`pip install maturin` in a
  venv — document; do NOT pollute system site-packages: venv under the crate dir, gitignored).
- Value conversion: research current best PyO3 serde bridge (pythonize vs serde-pyobject) —
  verify on crates.io which is maintained for the current PyO3 major; cite the choice.
- MIT; warning-free; MSRV override only in the new crate if a dep requires it (document).

---

### Task 1: PyO3 crate + bindings + smoke

**Files:** Create `crates/axilog-py/` (Cargo.toml cdylib abi3, pyproject.toml, src/lib.rs),
workspace membership.

**Requirements:** module `axilog` exposing the four functions wrapping the existing pipeline
(decode_raw → resolve → analyze → build_report [→ to_ei_json]; anonymize reuses core fns).
Errors: decode/io errors → ValueError/OSError with the Rust message. Build via maturin in a
gitignored venv; smoke: `python -c "import axilog; r=axilog.parse_file('...anon.zevtc'); print(r['schema_version'], len(r['players']))"`
→ 0.1 / 42, plus damage sum 2138414. GATES: workspace tests green; smoke numbers exact.

### Task 2: typing stubs + unittest suite + CLI parity

**Files:** `crates/axilog-py/axilog.pyi`, `py.typed`, `crates/axilog-py/tests/test_sdk.py`,
crate README.

**Requirements:** `.pyi` transcribed from `axilog-schema` structs (optional markers faithful);
unittest suite (stdlib only) covering: shape/values (players len, damage 2138414, one boon
uptime from fixtures/wvw-small.ei.json, support 801/97/437/6), bytes/file equivalence, EI keys
(players[0].account, dpsAll[0].damage, support[0].condiCleanse, buffUptimes, wvWMapData),
anonymize round-trip (tempfile), missing-file error, CLI parity deep-equal (build CLI, run
--format json, deep-compare with first-N-diffs reporting). Run via
`python -m unittest discover`. GATES: suite green; workspace green.

### Task 3: CI + docs

**Files:** `.github/workflows/ci.yml`, root `README.md`.

**Requirements:** ubuntu leg: setup-python@v5 (3.11), pip install maturin (cached), maturin
develop/build into a venv, build CLI, run unittest suite. win/mac: `maturin build` only, with
comment. Root README: Python usage example, M6 milestone entry, PyPI-deferred note. Local
simulation of the exact CI steps green. GATES: workspace + node + python all green locally.

## Self-Review
Mirrors the proven M5 shape. Conversion-crate choice delegated with verify-then-use. No
placeholders.
