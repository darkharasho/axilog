# axilog MPERF — Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** measure, then collapse redundant work in the analysis pipeline (foremost: the 7×
`InstidRegistry::build` rebuild) with ZERO change to any calibrated output. Bench harness +
baseline first, shared-registry refactor second, bounded proven wins third.

## Global Constraints

- **Accuracy frozen.** All existing calibration EXACT — byte-identical committed-fixture JSON
  before/after every task. Full workspace + node + python + JS suites green (currently 467/11/14).
  Determinism (BTreeMap/BTreeSet ordering) untouched; warning-free; no PII. A perf change that
  moves ANY calibrated number is a defect — revert it.
- The refactor is mechanical-equivalence only: build-once-share is provably equal to build-N;
  fusions must be obviously output-preserving or they don't land this milestone.
- criterion is the measurement tool; the accuracy suite is the regression gate (NOT a CI
  wall-clock threshold — too noisy). Every task records before/after numbers.

---

### Task 1: Criterion bench harness + committed baseline

**Files:** add a criterion dev-dependency + `benches/pipeline.rs` (in `axilog-core`, or a small
bench target where the pipeline is reachable); `docs/BENCHMARKS.md`; `.github/workflows/ci.yml`
(a `cargo bench --no-run` build + a quick `cargo bench` smoke on the fixture); Cargo.toml
`[[bench]]` wiring with `harness = false`.

**Requirements:** benchmark `evtc::decode_raw`, `model::resolve`, `analysis::analyze`, and the
full pipeline through `axilog_schema::build_report`, over the committed anonymized fixture
(`fixtures/wvw-small.anon.zevtc` — always in CI). Add an env-gated (`AXILOG_BENCH_LOG=<path>`)
real-log arm so the 583k-event gitignored local log can be measured locally without committing
PII or breaking CI when absent. Commit `docs/BENCHMARKS.md`: baseline numbers for each stage
(fixture, reproducible in CI) + a real-log reference section with methodology + the machine it
was measured on. CI: build the benches (catch rot) and run the fixture bench once (fast); do NOT
add a hard timing pass/fail. Record the baseline in the ledger — Task 2/3 measure against it.
No analysis-code change in this task.

### Task 2: Share the InstidRegistry (build once, thread by reference)

**Files:** `analysis/mod.rs` (build one `InstidRegistry` early in `analyze()`); `analysis/
damage.rs`, `cc.rs`, `skill_damage.rs`, `contribution.rs`, `healing.rs`, `buffs/events.rs`
(accept `&InstidRegistry` instead of rebuilding). Keep a `raw`-only convenience wrapper wherever
an external/standalone caller (SDK, `replay`/`missiles` standalone builders, tests) still calls
these with just `raw`, so no public break.

**Requirements:** `InstidRegistry::build(raw)` is a pure function of `raw`; building it once and
passing `&InstidRegistry` into all seven current call sites is provably identical to seven
independent builds. Thread it through. Verify: dump committed-fixture JSON before and after and
assert byte-equality (add this as an explicit verification step); full suites green; warning-free.
Measure `analyze()` speedup vs the Task 1 baseline and record in BENCHMARKS.md. If any consumer's
registry has a subtly different construction (e.g. an era/extension-row nuance), reconcile to the
single canonical build WITHOUT changing output — if that's not possible for one consumer, document
why it keeps its own and leave it.

### Task 3: Bounded, bench-proven secondary wins + docs

**Files:** wherever Task 1's bench points (likely `analysis/mod.rs` boon double-simulation, squad/
enemy set reuse, collection pre-sizing); `docs/BENCHMARKS.md` (final before/after table); README
(perf line with real numbers).

**Requirements:** apply ONLY wins the bench shows are material AND that are mechanically
output-preserving: candidate targets — (a) the two boon simulations (uptime + generation): if the
second re-derives simulation state the first already produced, share it; if they're genuinely
different queries, leave both and note it; (b) rebuilding squad/enemy `BTreeSet`s or `addr_to_rep`
maps more than once; (c) pre-sizing hot `BTreeMap`/`Vec`s; (d) fusing two adjacent same-predicate
scans when obviously-equivalent. NO whole-pipeline event-router rewrite (out of scope — accuracy
risk). For each applied change: byte-identical JSON check + suites green. Document every applied
optimization AND every deliberately-declined one (with the reason) in BENCHMARKS.md. Update README
perf claim with the measured fixture + real-log numbers. GATES: full workspace + node + python +
JS green; committed-fixture JSON byte-identical to pre-MPERF `main`; warning-free.

## Self-Review
Three tasks: measure, then the one clean high-value win (7→1 registry builds), then only
bench-proven output-preserving extras. Accuracy is frozen and checked by byte-equality, not just
"tests pass". The risky whole-pipeline fusion is explicitly out of scope. No placeholders.
