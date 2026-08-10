# Benchmarks

This is the MPERF (performance milestone) measurement baseline — **Task 1**.
It is a pure-measurement artifact: no analysis/parsing code changed to
produce these numbers. Tasks 2/3 of the MPERF milestone (any actual
refactor work) measure against this baseline using the same harness.

Results of the refactor tasks are appended below the baseline, newest last:
see [After Task 2](#after-task-2--shared-instidregistry-build-once-thread-by-reference)
and [After Task 3](#after-task-3--bounded-bench-proven-secondary-wins).
The consolidated end-of-milestone numbers (baseline -> Task 2 -> Task 3, for
both the committed fixture and a real 583k-event log) are in
[MPERF final results](#mperf-final-results-baseline---task-2---task-3).

Later milestones that add work to a measured stage record their delta here
too, against the MPERF tip:
[After MATTRIB Task 1](#after-mattrib-task-1--the-orphaned-instid-repair-pre-pass),
[After MEIGAP Task 1](#after-meigap-task-1--incoming-ccstrips-and-the-per-target-split).

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

## After Task 3 — bounded, bench-proven secondary wins

MPERF **Task 3** applied three further optimizations to `analysis::analyze`,
each one measured on its own and kept only because the bench showed a
material improvement on at least one arm. Everything else that was
considered is listed under [Declined](#declined-in-task-3-and-why) below,
with the reason.

**Accuracy is frozen and verified**, by the same procedure Task 2 used but
extended to the real log as well — see
[Byte-equality procedure](#byte-equality-procedure) at the end of this
section. All six reference outputs are byte-identical before and after every
individual change and after the final state.

### Applied #1 — one boon extraction, shared by both simulations

`analyze()` ran the boon-event extraction **three** times per parse:
once inside `buffs::simulate_boons` (stack-count timelines), once inside
`buffs::generation::simulate_boon_generation_ms` (per-source attribution),
and a third time for the post-2026-05-01 "no buff events found" warning
check. Each of those is a full scan over `raw.events`
(`events::extract_buff_events_with_registry`) plus a second full scan for
`events::extract_buff_capacities` — and all three produced bit-identical
results, since both are pure functions of `(raw, registry)` and the
compile-time `BOON_IDS` table.

The two *simulations* are genuinely different queries and were **not**
merged (the first tracks only stack COUNT, the second tracks WHICH source
owns each held stack; `buffs::generation`'s module doc explains why they are
deliberately kept independently verifiable). Only their identical raw input
is now shared, via a new `buffs::BoonInputs` extracted once in `analyze()`
and lent to both, plus the warning check which reads `boon_inputs.events.
is_empty()` instead of re-extracting.

| Arm | `analysis::analyze` | `full_pipeline` |
|---|---|---|
| fixture | 22.052 ms -> 21.889 ms (−0.7%, noise) | 31.632 ms -> 31.956 ms (+1.0%, noise) |
| real log | 143.95 ms -> **132.57 ms** (−7.9%) | 226.02 ms -> **212.66 ms** (−5.9%) |

Kept on the strength of the real-log arm. The fixture is a *pre*-rework log,
where extraction takes the cheaper pre-era branch and the third
(warning-check) extraction never runs at all — so the fixture genuinely has
much less duplicated work to remove here. This is the one applied change
whose value is real-log-only.

API: `buffs::simulate_boons_with_inputs` and
`buffs::generation::simulate_boon_generation_ms_with_inputs` are new; the
existing `_with_registry` and `raw`-only entry points are unchanged and now
delegate through them, so no SDK/test caller moved.

### Applied #2 — `contribution`: a time-ordered event index instead of a rescan per down

`contribution::credit_window` filters the whole event list down to one
down's `[lo, hi]` window, and it is called once per down — so the pass was
O(downs × events). On the real log that is 53 downs × 583,194 events ≈ 31M
filtered iterations, making `contribution::apply` the single most expensive
pass in `analyze()` by a wide margin (profiled at 33-52 ms of a ~140 ms
`analyze`).

`apply_with_registry` now builds **one** time-sorted `Vec<&RawEvent>` index
and binary-searches each down's window out of it, so the total cost is one
sort plus the sum of the (2-4 s wide) window sizes. Two properties make this
bit-for-bit output-preserving, and both are spelled out in the code comment:

1. `[partition_point(time < lo), partition_point(time <= hi))` over a
   time-sorted index selects exactly the event **set** `credit_window`'s own
   `e.time < lo || e.time > hi` guard keeps — and that guard is deliberately
   left in place, so the narrowing is pure and the function stays correct
   standalone.
2. Order **within** a window cannot matter: every credit is an additive
   `+=` into a `BTreeMap<contributor, ContributionMetrics>` whose four fields
   are all plain sums, with no running state and no first/last-wins
   anywhere. This is the same commutativity `credit_window`'s own doc
   comment already relied on ("these are pure sums, so scan direction can't
   change the result"), and the output map is keyed by contributor addr, not
   by event order.

One edge case surfaced while implementing this and is now explicitly
handled: `lo` can legitimately exceed `hi`, when an earlier down of the
same target pushes the `RESET_GAP_MS` invuln floor past this down's own
time. The old full-scan form expressed that as "no event matches" (empty
credit set); `start.min(end)` reproduces it as an empty subslice. The
committed fixture exercises this path, so it is covered by the byte-equality
check (and it panicked loudly on the first attempt, which is how it was
found).

| Arm | `analysis::analyze` | `full_pipeline` |
|---|---|---|
| fixture | 21.889 ms -> 21.085 ms (−3.7%) | 31.956 ms -> 31.401 ms (−1.7%) |
| real log | 132.57 ms -> **100.43 ms** (−24.2%) | 212.66 ms -> **192.11 ms** (−9.7%) |

### Applied #3 — `InstidRegistry`: flat `Vec` backing store instead of `BTreeMap<u16, _>`

`damage::InstidRegistry` keyed its per-instid registration lists by a
`BTreeMap<u16, Vec<(u64, u64)>>`. The key space is a `u16`, i.e. bounded at
65,536 entries no matter how large the log is, so a flat `Vec` of exactly
that length (~1.5 MiB of `Vec` headers, allocated once) replaces an O(log n)
tree descent with an O(1) index on **every** registration during `build`
(two per event — over a million on the real log) and on every `resolve_at`
query (which every pet-crediting pass in the codebase makes, constantly).

`by_instid` is private and is only ever indexed by a known instid — it is
never iterated — so there is no ordering-dependent behaviour to change, and
the per-instid `Vec<(u64, u64)>` contents plus `resolve_at`'s
`partition_point` query over them are untouched. The old
`BTreeMap::get(&instid)?` miss and the retained `idx == 0` check collapse
into the same "no addr known at this time" answer.

| Arm | `analysis::analyze` | `full_pipeline` |
|---|---|---|
| fixture | 21.085 ms -> **19.516 ms** (−7.4%) | 31.401 ms -> **29.958 ms** (−4.6%) |
| real log | 100.43 ms -> **97.170 ms** (−3.2%) | 192.11 ms -> **179.74 ms** (−6.4%) |

The only applied change that is material on *both* arms.

### Applied #4 (cold path, not bench-visible) — `healing::apply` gate hoist

Carried over from Task 2's review as a deferred minor. `healing::
apply_with_registry` early-returns `false` when the log carries no arcdps
healing extension, so `healing::apply` (the `raw`-only wrapper) was building
an entire `InstidRegistry` — a full linear scan over every event — and then
immediately throwing it away on every extension-less log. The same gate is
now checked in the wrapper *before* the build.

Deliberately **not** bench-measured, because it cannot be: `analyze()` never
takes this path (it passes its own shared registry, already built for every
other pass), and both benchmark logs carry the healing extension anyway.
This is a pure waste-removal for standalone SDK/test callers of the
`raw`-only entry point, kept because it is two lines and provably cannot
change a result (identical gate, identical return value).

### Declined in Task 3, and why

| Candidate | Verdict | Reason |
|---|---|---|
| Merge the two boon simulations (uptime + generation) into one pass | **Declined** | They are genuinely different queries, not a re-derivation: `simulate_boons` tracks stack COUNT over time, `generation` tracks WHICH source holds each concurrent stack and for how long. `buffs::generation`'s module doc requires the two simulators stay independently verifiable against each other precisely so a future fix to one can't silently corrupt the other. Only their identical *input* was shared (Applied #1) — the brief's "if they're genuinely different queries, leave both and note it" arm. |
| `contribution`: "is the event list already time-sorted?" fast path | **Tried, reverted** | The first attempt at Applied #2 kept the full scan and only narrowed to a subslice when `raw.events` was verified globally non-decreasing in `time`. Measured with a debug print: this project's real post-rework WvW capture is **not** globally time-sorted, so the fast path never fired, and the sortedness check itself added a scan. Its apparent −5.5% in one run was run-to-run drift. Replaced wholesale by the always-sort index, which does not depend on the input being pre-sorted. |
| Share the squad/enemy `BTreeSet`s and `addr_to_rep` maps across passes | **Declined** | Real, but not material. These are built from `enc.players`/`enc.enemies` (48 players / 140 enemies on the real log), not from `raw.events` — rebuilding one is tens of map inserts, far below measurement noise. `simulate_boons`/`generation` each rebuild their own `addr_to_rep`, ~microseconds. Threading them would touch every pass signature in `analysis/` for no measurable gain. |
| Pre-size hot `BTreeMap`s / `Vec`s | **Declined** | No material target. `BTreeMap` has no `reserve` at all (the dominant collections here — `dmg_by_rep`, the credit maps, the boon maps — are all `BTreeMap`, kept for determinism). The hot `Vec` growth is the per-instid registration lists, which hold a handful of entries each. The one `Vec` worth sizing up front, the registry's 65,536-slot backing store, is exactly Applied #3. |
| Fuse the three "one scan over every event" classification passes (`accumulate_damage_taken` + `hit_stats::build` + `defenses::build`) | **Declined** | Fails the brief's "mechanically obviously-equivalent" bar. The three have *different* scopes (`hit_stats` is squad -> enemies and actor-only with no pet fold; `defenses` and `accumulate_damage_taken` are any-source -> squad; only `defenses` reads cast events for `dodge_count`) and different per-row classification state. Fusing means interleaving three hand-verified state machines that are individually calibrated against GW2EI — an accuracy risk well out of proportion to the ~6 ms (real log) on offer. Left as-is deliberately. |
| Whole-pipeline event-router rewrite (one pass feeding every consumer) | **Out of scope** | Explicitly excluded by the milestone brief — accuracy risk. |
| Parallelism (rayon) across passes or event chunks | **Out of scope** | Explicitly excluded by the milestone brief. Also note the pass set is not embarrassingly parallel: `skill_map` depends on finished `skill_damage`/`rotation`, and `boon_uptime` on finished `boons`. |
| `unsafe` (e.g. unchecked indexing in the registry hot loop) | **Out of scope** | Explicitly excluded by the milestone brief. |

### Remaining hotspots (for any future perf work)

Profiled on the real log after Task 3, the largest remaining single passes
are `timeseries::build` (~13 ms), `buffs::generation` (~12 ms),
`buffs::simulate_boons` (~9 ms), `skill_damage::build` (~8 ms) and
`rotation::build` (~7 ms). None of them contains an obvious
duplicated-work item of the kind Tasks 2 and 3 removed — they are each doing
one pass of genuinely distinct work — so the next real step up would be the
event-router consolidation this milestone deliberately ruled out.

### Byte-equality procedure

Before any code change, six reference outputs were dumped at the Task 2
commit (`d4305de`) — three per log, for the committed fixture and for a real
583k-event post-rework WvW log (gitignored, never committed; see
**Fixture policy** in the README):

```
axilog parse <log> --format json                 -o <tag>.json
axilog parse <log> --format json    --replay --missiles --skill-damage --timeseries --rotation -o <tag>.full.json
axilog parse <log> --format ei-json --replay --missiles --skill-damage --timeseries --rotation -o <tag>.ei.json
```

They were regenerated and `cmp`'d after **each** applied optimization and
again at the final state. Every one of the six was byte-identical every
time, including the 169 MB `ei-json` full-flag dump of the real log.

## MPERF final results (baseline -> Task 2 -> Task 3)

All three columns were re-measured **back to back in one session** on the
same machine (AMD Ryzen 9 7900X3D 12-core, Linux kernel 7.1.5, 30 GiB RAM,
Rust 1.95.0, `cargo bench` release profile, 2026-08-09) by benchmarking the
Task 1 commit (`e4c99f2`), the Task 2 commit (`d4305de`) and the Task 3
working tree in sequence. That matters: the Task 1/Task 2 numbers earlier in
this file were taken in an *earlier* session, and this machine has drifted a
few percent since (visible in `decode_raw`, which no MPERF task touched).
Use the table below for cross-task comparisons and the per-task sections
above for the reasoning.

### Committed fixture (`fixtures/wvw-small.anon.zevtc`, 120,435 events)

| Stage | Baseline (Task 1) | After Task 2 | After Task 3 | Task 3 vs baseline |
|---|---|---|---|---|
| `decode_raw` | 8.8746 ms | 8.6360 ms | 9.0111 ms | +1.5% (untouched — noise) |
| `model::resolve` | 684.23 µs | 646.61 µs | 657.20 µs | −3.9% (untouched — noise) |
| `analysis::analyze` | 40.107 ms | 21.914 ms | **18.913 ms** | **−52.8%** (2.12× faster) |
| `full_pipeline` | 50.508 ms | 31.809 ms | **28.852 ms** | **−42.9%** (1.75× faster) |

Task 2 -> Task 3 alone: `analyze` −13.7%, `full_pipeline` −9.3%.

### Real log (583,194 events)

Log stats (no PII — counts and timings only): 7,600,659 bytes zipped /
37,480,572 bytes inflated, 583,194 combat events (rev 1), 48 resolved squad
players, 140 enemies, 348,362 ms fight duration, post-2026-05-01
(`is_post_buff_rework`) arcdps build, carries the healing extension. Kept
under `fixtures/local/` and never committed.

| Stage | Baseline (Task 1) | After Task 2 | After Task 3 | Task 3 vs baseline |
|---|---|---|---|---|
| `decode_raw` | 75.466 ms | 73.270 ms | 74.977 ms | −0.6% (untouched — noise) |
| `model::resolve` | 4.1731 ms | 4.0852 ms | 3.9602 ms | −5.1% (untouched — noise) |
| `analysis::analyze` | 246.60 ms | 139.98 ms | **93.700 ms** | **−62.0%** (2.63× faster) |
| `full_pipeline` | 325.54 ms | 230.12 ms | **174.49 ms** | **−46.4%** (1.87× faster) |

Task 2 -> Task 3 alone: `analyze` −33.1%, `full_pipeline` −24.2%.

`analysis::analyze` has gone from ~80% of `full_pipeline` at the baseline to
~65% on the fixture and ~54% on the real log. On the real log `decode_raw`
(zip inflate + event decode, untouched by this milestone) is now the second
biggest single item at ~43% of the pipeline.

Raw criterion runs used to produce the two tables above:

```
$ git checkout e4c99f2                # Task 1 baseline
$ cargo bench -p axilog-cli --bench pipeline
fixture/wvw-small/decode_raw            time:   [8.8180 ms 8.8746 ms 8.9342 ms]
fixture/wvw-small/model::resolve        time:   [680.38 µs 684.23 µs 688.48 µs]
fixture/wvw-small/analysis::analyze     time:   [40.004 ms 40.107 ms 40.219 ms]
fixture/wvw-small/full_pipeline ...     time:   [50.262 ms 50.508 ms 50.773 ms]
$ AXILOG_BENCH_LOG=<real log> cargo bench -p axilog-cli --bench pipeline -- real_log
real_log/decode_raw                     time:   [74.778 ms 75.466 ms 76.012 ms]
real_log/model::resolve                 time:   [4.1074 ms 4.1731 ms 4.2516 ms]
real_log/analysis::analyze              time:   [244.45 ms 246.60 ms 248.61 ms]
real_log/full_pipeline ...              time:   [324.19 ms 325.54 ms 327.16 ms]

$ git checkout d4305de                # Task 2
fixture/wvw-small/decode_raw            time:   [8.5960 ms 8.6360 ms 8.6802 ms]
fixture/wvw-small/model::resolve        time:   [644.23 µs 646.61 µs 649.73 µs]
fixture/wvw-small/analysis::analyze     time:   [21.851 ms 21.914 ms 21.984 ms]
fixture/wvw-small/full_pipeline ...     time:   [31.649 ms 31.809 ms 31.984 ms]
real_log/decode_raw                     time:   [72.570 ms 73.270 ms 73.890 ms]
real_log/model::resolve                 time:   [3.9919 ms 4.0852 ms 4.1458 ms]
real_log/analysis::analyze              time:   [137.35 ms 139.98 ms 143.83 ms]
real_log/full_pipeline ...              time:   [224.82 ms 230.12 ms 235.67 ms]

$ git checkout feat/mperf-performance # Task 3
fixture/wvw-small/decode_raw            time:   [8.9396 ms 9.0111 ms 9.0882 ms]
fixture/wvw-small/model::resolve        time:   [653.87 µs 657.20 µs 661.03 µs]
fixture/wvw-small/analysis::analyze     time:   [18.829 ms 18.913 ms 19.001 ms]
fixture/wvw-small/full_pipeline ...     time:   [28.710 ms 28.852 ms 29.009 ms]
real_log/decode_raw                     time:   [74.239 ms 74.977 ms 75.707 ms]
real_log/model::resolve                 time:   [3.8154 ms 3.9602 ms 4.0698 ms]
real_log/analysis::analyze              time:   [92.611 ms 93.700 ms 94.468 ms]
real_log/full_pipeline ...              time:   [174.00 ms 174.49 ms 175.07 ms]
```

The per-candidate deltas quoted in the Task 3 sections above were measured
incrementally as each change landed, so they do not sum exactly to the
back-to-back sweep here — run-to-run drift on a desktop machine is a few
percent. The sweep above is the authoritative comparison.

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

**Real-log numbers:** measured in Task 3 — see
[MPERF final results](#mperf-final-results-baseline---task-2---task-3)
above for the full baseline -> Task 2 -> Task 3 table, plus the log's own
(PII-free) stats: 583,194 events, 48 resolved squad players, 140 enemies,
348,362 ms fight duration, post-2026-05-01 arcdps build. Headline, at the
Task 3 tip: `decode_raw` 74.977 ms, `model::resolve` 3.9602 ms,
`analysis::analyze` 93.700 ms, `full_pipeline` 174.49 ms.

Only counts and timings are ever recorded here. The log itself stays under
`fixtures/local/` (gitignored) and is never committed, so these numbers are
not independently reproducible by a third party the way the fixture arm is —
which is exactly why the committed fixture arm exists alongside them.

## After MATTRIB Task 1 — the orphaned-instid repair pre-pass

MATTRIB Task 1 adds a decode post-pass (`evtc::repair`, GW2EI's
`EvtcParser.CompleteAgents` orphaned-instid rewrite) that runs inside
`decode_raw`, so it is the first change since MPERF to add work to a
measured stage. One extra full scan of the event stream, with two agent-slot
lookups per row; the repair loops themselves are proportional to the orphan
count (43 rows on the fixture, 725 on the real log), not to the stream.

Same machine and harness as the baseline above. Measured 2026-08-09 against
`bd8b3d5`.

| Stage | Before (`bd8b3d5`) | After | Δ |
|---|---|---|---|
| fixture `decode_raw` | 9.006 ms | 9.851 ms | +9.4% (+0.85 ms) |
| fixture `model::resolve` | 694.2 µs | 709.9 µs | +2.3% (noise) |
| fixture `analysis::analyze` | 20.293 ms | 20.240 ms | −0.3% (noise) |
| fixture `full_pipeline` | 30.175 ms | 31.434 ms | +4.2% (+1.26 ms) |
| real-log `decode_raw` | 77.40 ms | 81.73 ms | +5.6% (+4.3 ms) |
| real-log `model::resolve` | 4.451 ms | 4.419 ms | −0.7% (noise) |
| real-log `analysis::analyze` | 96.89 ms | 96.65 ms | −0.2% (noise) |
| real-log `full_pipeline` | 183.75 ms | 183.88 ms | +0.07% (noise) |

The bounded cost is entirely in `decode_raw`; `full_pipeline` absorbs it into
noise on the real log. Worth recording how it got there: the first
implementation keyed the addr -> agent-slot map with a `BTreeMap<u64,
usize>` and cost **+33.6% / +19.6%** on `decode_raw` — ~2.4x the final
figure. Replacing it with a `HashMap` behind a three-instruction
multiply-shift hasher (`repair::AddrHasher`; the map is looked up, never
iterated, so this cannot affect ordering or determinism) is the whole
difference. Per-row hashing on a 583k-row scan is not a micro-optimisation.

## After MEIGAP Task 1 — incoming CC/strips and the per-target split

MEIGAP Task 1 adds two always-on units of work to `analysis::analyze`:
`analysis::per_target` (a new full scan producing the per-(player, enemy)
offensive split) and two additions inside `analysis::defenses` (incoming
crowd control, folded into the existing breakbar scan; incoming boon
strips, one more scan over `BUFFREMOVE_ALL`-shaped rows). The two OPT-IN
families it adds — the serialized `per_target` block behind
`--skill-damage`, and `buffs::states` behind `--timeseries` — are outside
`analyze()` entirely and so outside these numbers.

Same machine and harness as the baseline above. Measured 2026-08-10 against
`730212a` (the MEIGAP base).

| Stage | Before (`730212a`) | After | Δ |
|---|---|---|---|
| fixture `decode_raw` | 9.177 ms | 9.138 ms | −0.4% (noise) |
| fixture `model::resolve` | 651.98 µs | 667.51 µs | +2.4% (noise) |
| fixture `analysis::analyze` | 18.733 ms | 19.677 ms | +5.0% (+0.94 ms) |
| fixture `full_pipeline` | 29.096 ms | 29.854 ms | **+2.6%** (+0.76 ms) |
| real-log `decode_raw` | 77.460 ms | 76.585 ms | −1.1% (noise) |
| real-log `model::resolve` | 3.741 ms | 3.813 ms | +1.9% (noise) |
| real-log `analysis::analyze` | 85.253 ms | 89.046 ms | +4.4% (+3.79 ms) |
| real-log `full_pipeline` | 167.54 ms | 171.08 ms | **+2.1%** (+3.54 ms) |

The gate is `full_pipeline` (the plan's "no >5% pipeline regression"), and
both arms land at roughly half of it. The cost is concentrated in
`analyze`, as expected for three extra event-stream traversals.

Worth recording one measurement that went the wrong way first. Merging the
incoming-CC classification into the pre-existing breakbar scan is a clear
win on paper — one traversal instead of two — but the obvious way to write
it, hoisting the shared `squad.contains(&e.dst_agent)` test to the top of
the loop, made `analysis::analyze` **slower than two separate scans**
(93.09 ms vs 89.54 ms on the real log, +4% over the unmerged form). The
`squad` membership test is a `BTreeSet<u64>` probe; the classifications are
a handful of byte compares that reject almost every row. Ordering the cheap
byte filters first and the set probe last recovers the win. Same lesson as
MATTRIB's `BTreeMap` -> `HashMap` finding: on a 583k-row scan, what you do
*per row* is the whole cost model.

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

## axilog vs Elite Insights (v0.2.0 vs EI CLI v3.27, 2026-08-09)

Same machine as above (Ryzen 9 7900X3D, 24 threads, Linux). EI = the exact CLI + .NET 8.0.25
runtime axibridge ships (multithreaded, axibridge's production `settings.conf`: DetailledWvW,
ComputeDamageModifiers, ParseCombatReplay, RawTimelineArrays, phases, JSON.gz out). axilog =
release build, matched surface: `--format ei-json --replay --skill-damage --timeseries
--rotation --modifiers`. 3 runs each after a warmup; median wall / peak RSS via
`/usr/bin/time`.

### Large log (7.6 MB zevtc, 583,194 events, 48 players, 5:48 fight)

| pipeline | wall (median of 3) | peak RSS |
|---|---|---|
| Elite Insights CLI | 6.41 s | 857 MiB |
| axilog, matched ei-json surface | **2.40 s** (2.7× faster) | 1,281 MiB |
| axilog, matched + gzip output | 2.70 s (2.4× faster) | 1,282 MiB |
| axilog, default native JSON | **0.32 s** (20× faster) | **82 MiB** (10× less) |

### Small log (1.5 MB zevtc, 120,435 events, 42 players, 49 s fight)

| pipeline | wall (median of 3) | peak RSS |
|---|---|---|
| Elite Insights CLI | 2.27 s | 373 MiB |
| axilog, matched ei-json surface | **0.28 s** (8× faster) | 105 MiB |
| axilog, default native JSON | **0.06 s** (38× faster) | **20 MiB** (18× less) |

### Honest notes

- **EI's fixed startup dominates small logs**: ~2 s of the EI number is dotnet start + JIT,
  paid PER SPAWN — and axibridge spawns the CLI once per uploaded log, so every upload pays it.
  axilog's fixed cost is ~0.
- **axilog's matched-surface peak RSS is HIGHER than EI's on the large log** (1,281 vs
  857 MiB): the ei-json path builds the entire 183 MB document as an in-memory
  `serde_json::Value` tree before writing, while EI streams to `.json.gz`. The always-on
  native path (82 MiB) doesn't have this problem. Future item: stream the ei-json
  serialization. (The wall-clock lead survives regardless.)
- **Output sizes diverge for content reasons**: EI's `.json.gz` is 3.77 MB (45.9 MB
  decompressed); axilog's matched ei-json is 183 MB plain but **2.07 MB gzipped** — smaller
  than EI's, because axilog's bulk is highly-repetitive per-target arrays over its full
  624-entry target roster (EI exports a curated 57), while EI's payload includes things axilog
  doesn't emit (full skill/buff DB metadata, phases, PvE machinery).
- Not a feature-identical comparison: EI computes phases and its full DB-backed surface;
  axilog computes its documented WvW parity surface (see README's EI-JSON parity table). The
  matched-flag config is the closest apples-to-apples available and is exactly the
  axibridge-production shape.
