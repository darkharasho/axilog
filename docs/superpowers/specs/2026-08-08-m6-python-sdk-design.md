# axilog — M6: Python SDK (PyO3)

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Why:** Completes the original brief's SDK goals (Rust core + Node + Python). Python is the
scripting/analysis audience: log batch analysis, pandas pipelines, community tooling.

## Scope

1. **`axilog-py` crate** (PyO3 + maturin, cdylib): module `axilog` with
   - `parse_file(path) -> dict` (native Report), `parse_bytes(data: bytes) -> dict`
   - `parse_file_ei(path) -> dict` (EI-compat shape)
   - `anonymize_file(in_path, out_path)`
   Errors → Python exceptions (ValueError/OSError mapping). Dicts via serde → Python objects
   (pythonize or serde-pyobject — pick current best; keys verbatim).
2. **Typing**: `axilog.pyi` stub transcribed from the schema (same source-of-truth discipline as
   the Node types.d.ts), `py.typed` marker.
3. **Tests**: pytest-free stdlib `unittest` (zero-dep) against the committed anon fixture:
   shape/values (schema_version, players, damage 2,138,414, boon uptime, support 801/97/437/6),
   bytes/file equivalence, EI keys, anonymize round-trip, error case, CLI parity deep-equal.
4. **CI**: ubuntu leg builds the wheel (maturin) + runs the unittest suite; win/mac build-only.
5. **README**: Python usage section; publishing to PyPI deferred (mirror the npm stance).

## Gates
- Python-reported numbers identical to CLI JSON (parity deep-equal).
- Rust workspace stays green; additive crate only.

## Non-goals
PyPI publishing, wheels matrix/abi3 optimization beyond what maturin defaults give, async APIs,
pandas helpers (future sugar).
