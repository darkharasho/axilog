# axilog — MPERF: Performance milestone (bench harness + shared-scan refactor)

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** Flagged rising by the M12 and M14 whole-branch reviews. `analyze()` now runs ~15
sequential passes, most doing their own full linear scan of `raw.events`, and
`InstidRegistry::build(raw)` — itself a full O(n) scan building a `BTreeMap<u16, Vec<(u64,u64)>>`
— is rebuilt **7 times per parse** (damage `pet_credit_events`, cc, skill_damage, contribution,
healing, buffs/events ×2). Every new always-on pass compounds this. On the 583k-event real log
this is ~15+ O(n) sweeps. First-class + performant-as-fuck is a standing bar; this pays down the
accumulated cost before more passes land, and before the thin HTML budget forces further gating.

## Non-negotiable bar

**Accuracy is frozen.** Every existing calibration test (Rust golden suites, node/python/JS
parity) must stay EXACT — byte-identical JSON output on the committed fixture before and after.
This is a pure performance refactor: no metric value, field, ordering, or rounding may change.
The full test suite (currently 467 workspace / 11 node / 14 python) is the regression guard; a
perf change that alters any calibrated number is a defect, not a tradeoff.

## Scope

1. **Bench harness + baseline (measurement first).** A criterion benchmark crate/target
   measuring `evtc::decode_raw`, `model::resolve`, `analysis::analyze`, and the full
   decode→resolve→analyze→build_report pipeline, over the committed anonymized fixture (always
   available in CI) and — documented, opt-in via env var — the gitignored real 583k-event log for
   local throughput numbers. Commit a `docs/BENCHMARKS.md` capturing the baseline (fixture numbers
   reproducible in CI; real-log numbers as a local reference with methodology). Add a `cargo bench`
   CI job that *builds and runs* the benches (catches bench-rot and gross regressions) — NOT a
   hard wall-clock gate (CI timing is too noisy for a pass/fail threshold; the accuracy suite is
   the real gate).

2. **Share the InstidRegistry (the clean high-value win).** Build `InstidRegistry` ONCE in
   `analyze()` and thread `&InstidRegistry` into every consumer that currently rebuilds it
   (damage pet-credit path, cc, skill_damage, contribution, healing, buffs). The registry is a
   pure function of `raw` — one build is provably equivalent to seven. Public helper signatures
   that take `raw` only to rebuild the registry gain a `&InstidRegistry` parameter (or an internal
   variant); keep a `raw`-only convenience wrapper where an external caller (SDK/replay/missiles
   standalone paths) needs it, so nothing outside `analyze()` breaks.

3. **Bounded, bench-proven secondary wins.** ONLY changes the bench shows pay off AND that
   provably preserve output: e.g. avoid the second full boon simulation if it re-derives what the
   first already computed, reuse an already-collected squad/enemy membership set instead of
   rebuilding it, pre-size hot collections, fuse two adjacent scans that share a predicate when
   the fusion is mechanically obviously-equivalent. NO speculative whole-pipeline event-router
   rewrite — that is higher-risk than this milestone's accuracy bar allows and can be its own
   future milestone if the bench justifies it. Document what was fused and what was deliberately
   left alone and why.

## Calibration / verification

All existing calibration EXACT (byte-identical committed-fixture JSON before/after — add a
before/after JSON-equality check to the milestone's own verification, not just "tests pass").
Real-log sanity both eras (numbers unchanged). Warning-free; determinism preserved (BTreeMap/
BTreeSet ordering untouched); no PII. Report measured speedup (fixture + real-log) in BENCHMARKS.md.

## Outputs

`benches/` criterion target(s); `docs/BENCHMARKS.md` (baseline + after); `cargo bench` CI job;
shared-registry refactor across the analysis passes; README perf line updated with real numbers.

## Non-goals

Whole-pipeline single-scan event-router rewrite (deferred — accuracy risk); SIMD/parallelism
(rayon) — single-threaded correctness first, parallelism is a separate future call; memory-arena
allocation redesign; any change to a calibrated metric.
