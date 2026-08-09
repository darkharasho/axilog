# Benchmarks

This is the MPERF (performance milestone) measurement baseline — **Task 1**.
It is a pure-measurement artifact: no analysis/parsing code changed to
produce these numbers. Tasks 2/3 of the MPERF milestone (any actual
refactor work) measure against this baseline using the same harness.

Results of the refactor tasks are appended below the baseline, newest last:
see [After Task 2](#after-task-2--shared-instidregistry-build-once-thread-by-reference).

## Harness

- Location: `crates/axilog-cli/benches/pipeline.rs`, wired via
  `crates/axilog-cli/Cargo.toml`'s `[[bench]]` (`harness = false`) +
  a `criterion` dev-dependency.
- Why `axilog-cli`: the full pipeline needs `axilog_schema::build_report`
  in addition to `axilog-core`'s `evtc::decode_raw` / `model::resolve` /
  `analysis::analyze`. `axilog-cli` already depends on both crates as
  regular dependencies, so putting the bench there adds exactly one new
  dependency edge (criterion, dev-only). The alternative — benching from
  `axilog-core` — would require adding a new `axilog-core -> axilog-schema`
  dev-dependency edge that doesn't exist today (the graph is directionally
  fine either way, since `axilog-schema` already depends on `axilog-core`,
  not the reverse, so there's no cycle risk — `axilog-cli` was chosen
  purely for fewest new edges).
- Run locally: `cargo bench -p axilog-cli --bench pipeline`.
- Build-only smoke (rot check, no timing): `cargo bench -p axilog-cli --no-run`.

Four benchmark functions run per log arm:

| Benchmark | What it measures |
|---|---|
| `decode_raw` | `axilog_core::evtc::decode_raw(&bytes)` — zip inflate (for `.zevtc`) + header/agent/skill/event decode |
| `model::resolve` | `axilog_core::model::resolve(&raw)` — agent resolution into the `Encounter` domain model |
| `analysis::analyze` | `axilog_core::analysis::analyze(&enc, &raw)` — the full metrics pass (damage/downs/CC/buffs/support/healing/contribution/skill-damage/timeseries/hit-stats/defenses/rotation/skill-map) |
| `full_pipeline` | decode → resolve → analyze → `axilog_schema::build_report(..., None, None, false, false, false)` — the always-on path (no `--replay`/`--missiles`/`--skill-damage`/`--timeseries`/`--rotation` opt-in blocks), i.e. what a plain `axilog parse` with no flags does per invocation |

Each stage benchmark measures **only its own stage's cost** — upstream
stages (e.g. `raw`/`enc` feeding `model::resolve`/`analysis::analyze`) are
computed once outside the timed closure. `full_pipeline` intentionally
redoes every stage inside the timed closure, since that's what a real CLI
invocation does end to end.

## Baseline — committed fixture (`fixtures/wvw-small.anon.zevtc`)

Fixture stats: 1,539,766 bytes zipped (`.zevtc` container) / 7,790,364 bytes
inflated raw `.evtc`, 173 agents, 42 resolved players, 120,435 combat
events (rev 1). This fixture is anonymized (M2's `axilog anonymize`
subcommand) and always committed, so this arm always runs — including in
CI.

Machine: AMD Ryzen 9 7900X3D (12-core), Linux (Fedora/Bazzite,
kernel 7.1.5), 30 GiB RAM. Rust 1.95.0, `cargo bench` release profile.
Measured 2026-08-09.

| Stage | Time (criterion mean, [low, high] of the 95% CI) |
|---|---|
| `decode_raw` | 8.3730 ms `[8.3215 ms, 8.4307 ms]` |
| `model::resolve` | 659.20 µs `[655.66 µs, 663.08 µs]` |
| `analysis::analyze` | 39.495 ms `[39.331 ms, 39.674 ms]` |
| `full_pipeline` (decode+resolve+analyze+build_report) | 49.215 ms `[49.015 ms, 49.430 ms]` |

Reading these: `analysis::analyze` dominates the pipeline (~80% of
`full_pipeline`'s wall time on this fixture), `decode_raw` is a distant
second (~17%), and `model::resolve` plus `build_report`'s own JSON-schema
assembly account for the small remainder. Any MPERF refactor work should
prioritize `analysis::analyze` first — it's where the time actually is.

Raw criterion run used to produce this table:

```
$ cargo bench -p axilog-cli --bench pipeline
fixture/wvw-small/decode_raw
                        time:   [8.3215 ms 8.3730 ms 8.4307 ms]
fixture/wvw-small/model::resolve
                        time:   [655.66 µs 659.20 µs 663.08 µs]
fixture/wvw-small/analysis::analyze
                        time:   [39.331 ms 39.495 ms 39.674 ms]
fixture/wvw-small/full_pipeline (decode+resolve+analyze+build_report)
                        time:   [49.015 ms 49.215 ms 49.430 ms]

AXILOG_BENCH_LOG not set -- skipping real-log benchmark arm
```

Full criterion output (including outlier counts) is not reproduced here;
re-run the command above to regenerate it, or see
`target/criterion/*/report/index.html` for the detailed HTML report
criterion writes locally.

## After Task 2 — shared `InstidRegistry` (build once, thread by reference)

MPERF **Task 2** made `analysis::analyze` build one
`analysis::damage::InstidRegistry` up front and thread `&InstidRegistry`
into every consumer, instead of each pass rebuilding its own. The registry
is a pure function of `raw` (one full linear scan over all 120,435 events
into a `BTreeMap<u16, Vec<(u64, u64)>>`), so all the per-pass copies were
bit-for-bit identical and ~10 of the 11 builds per parse were pure waste
(pet-credit damage, CC pet-credit, the CC timeline, contribution, healing,
skill-damage, timeseries, and three separate buff-event extractions).

**Accuracy is frozen and verified:** the committed fixture's native JSON
(plain and with every opt-in flag) and its `ei-json` output are all
byte-identical before/after — `diff` reports no differences on all three.

Same machine/fixture/toolchain as the baseline above. Measured 2026-08-09.

| Stage | Baseline (Task 1) | After Task 2 | Delta |
|---|---|---|---|
| `decode_raw` | 8.3730 ms | 8.5771 ms `[8.5065 ms, 8.6528 ms]` | +2.4% (untouched code — run-to-run noise) |
| `model::resolve` | 659.20 µs | 664.58 µs `[660.72 µs, 668.96 µs]` | +0.8% (untouched code — run-to-run noise) |
| `analysis::analyze` | 39.495 ms | **22.052 ms** `[21.972 ms, 22.137 ms]` | **−44.2%** (1.79× faster) |
| `full_pipeline` | 49.215 ms | **31.632 ms** `[31.458 ms, 31.820 ms]` | **−35.7%** (1.56× faster) |

`analysis::analyze` drops from ~80% to ~70% of `full_pipeline`'s wall time,
so it remains the right target for further MPERF work — but the single
largest duplicated-work item in it is now gone. (`decode_raw` /
`model::resolve` were not touched by this task; their small deltas are
measurement noise, not regressions.)

Raw criterion run used to produce this table:

```
$ cargo bench -p axilog-cli --bench pipeline
fixture/wvw-small/decode_raw
                        time:   [8.5065 ms 8.5771 ms 8.6528 ms]
fixture/wvw-small/model::resolve
                        time:   [660.72 µs 664.58 µs 668.96 µs]
fixture/wvw-small/analysis::analyze
                        time:   [21.972 ms 22.052 ms 22.137 ms]
fixture/wvw-small/full_pipeline (decode+resolve+analyze+build_report)
                        time:   [31.458 ms 31.632 ms 31.820 ms]

AXILOG_BENCH_LOG not set -- skipping real-log benchmark arm
```

### API shape

Each affected pass keeps its original `raw`-only signature as a thin
wrapper that builds a private registry and delegates, so SDK, standalone
(`replay`/`missiles`) and test callers are unchanged. `analyze()` calls the
new `_with_registry` variants instead:

| `raw`-only (unchanged, still public) | shared-registry variant used by `analyze()` |
|---|---|
| `damage::accumulate_pet_credit` | `damage::accumulate_pet_credit_with_registry` |
| `damage::pet_credit_events` | `damage::pet_credit_events_with_registry` |
| `cc::apply_cc` | `cc::apply_cc_with_registry` |
| `cc::timeline` | `cc::timeline_with_registry` |
| `contribution::apply` | `contribution::apply_with_registry` |
| `healing::apply` | `healing::apply_with_registry` |
| `skill_damage::build` | `skill_damage::build_with_registry` |
| `timeseries::build` | `timeseries::build_with_registry` |
| `buffs::events::extract_buff_events` | `buffs::events::extract_buff_events_with_registry` |
| `buffs::simulate_boons` | `buffs::simulate_boons_with_registry` |
| `buffs::generation::simulate_boon_generation_ms` | `..._with_registry` (crate-private) |

## Real-log reference

A second, env-gated benchmark arm (`real_log` group) exists in the same
harness for measuring against a real, much larger arcdps log (e.g. the
~583k-event WvW zerg log referenced in the MPERF spec) without ever
committing it — real logs carry player account names (PII) and are
gitignored under `fixtures/local/` (see `.gitignore`).

**Methodology:**

1. Place the real `.zevtc`/`.evtc` file anywhere outside the repo's
   tracked tree (`fixtures/local/` is already gitignored and convenient).
2. Run:
   ```
   AXILOG_BENCH_LOG=/path/to/real-log.zevtc cargo bench -p axilog-cli --bench pipeline
   ```
3. The `real_log` group runs the same four benchmarks (`decode_raw`,
   `model::resolve`, `analysis::analyze`, `full_pipeline`) with
   `sample_size(10)` instead of criterion's default 100, since real logs
   are large enough that 100 samples would take a long time. Paste the
   resulting numbers into this section, along with the log's approximate
   event count and the machine used.
4. If `AXILOG_BENCH_LOG` is unset, or set to a path that doesn't exist, the
   `real_log` group prints a one-line skip notice to stderr and registers
   no benchmarks — `cargo bench` still exits 0. This is what CI does (no
   real log is ever present there).

**Real-log numbers:** not measured in this task — no real log was
available in this environment. *Measure locally* using the steps above
before relying on this section for the 583k-event-scale log referenced in
the MPERF spec; the fixture numbers above (120k events) are the only
committed/reproducible baseline for now.

## CI

`.github/workflows/ci.yml` adds a step (ubuntu leg only) that:

1. Builds every bench target (`cargo bench -p axilog-cli --no-run`) — this
   catches bench-code rot (e.g. a signature change to `build_report`
   breaking the harness) as part of normal CI, without ever running the
   full statistical sampling.
2. Runs a quick fixture-only bench smoke
   (`cargo bench -p axilog-cli --bench pipeline -- --test`), which
   executes each benchmark function exactly once (criterion's `--test`
   mode) to confirm the harness actually runs end to end, without spending
   CI minutes on full sampling and without a hard wall-clock pass/fail
   threshold (CI runner timing is too noisy for that to be meaningful).

`AXILOG_BENCH_LOG` is never set in CI, so the `real_log` arm always skips
cleanly there.
