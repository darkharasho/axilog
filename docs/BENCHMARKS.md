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
[After MEIGAP Task 1](#after-meigap-task-1--incoming-ccstrips-and-the-per-target-split),
[After MEIGAP Task 2](#after-meigap-task-2--the-power-series-split),
[After MEIGAP Task 3](#after-meigap-task-3--the-healingminionguild-remainder).

Memory, not time, is the subject of one section: **MSTREAM** made the
ei-json CLI path streaming and cut its peak RSS by 20× — see
[After MSTREAM](#after-mstream--streaming-ei-json-serialization) and the
[MSTREAM update](#mstream-update-2026-08-10--the-axilog-rows-only) to the
Elite Insights comparison.

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
`analyze()` entirely and so outside these numbers. The third always-on
addition, the `selfBuffs`/`groupBuffs`/`squadBuffs` ei-json arrays, costs
nothing here: it is a re-serialization of numbers `analyze()` already
computed, and shows up as payload size (+21.3% compact on the flagless
ei-json), not CPU.

Same machine and harness as the baseline above, measured 2026-08-10 against
`730212a` (the MEIGAP base).

**Method matters here, and a first pass got it wrong.** An initial
single-run-per-side measurement reported `full_pipeline` at +2.6%; a
reviewer re-measuring independently got +4.3% with non-overlapping
confidence intervals. Run-to-run drift on this machine is comparable to the
effect being measured (the real-log `full_pipeline` spans 165.9-169.1 ms
across three consecutive runs of the *same* binary), so a single
before/after pair cannot resolve it. The numbers below are the MEDIAN of
**three alternating base/tip pairs**, run back to back from two prebuilt
bench binaries so no rebuild or checkout sits between the two sides of a
pair. All six per-stage samples are listed so the spread is visible.

| Stage | Before (`730212a`) | After | Δ |
|---|---|---|---|
| fixture `decode_raw` | 9.183 ms | 9.076 ms | −1.2% (noise) |
| fixture `model::resolve` | 651.4 µs | 654.9 µs | +0.5% (noise) |
| fixture `analysis::analyze` | 18.615 ms | 19.395 ms | +4.2% (+0.78 ms) |
| fixture `full_pipeline` | 28.898 ms | 29.965 ms | **+3.7%** (+1.07 ms) |
| real-log `decode_raw` | 76.682 ms | 77.081 ms | +0.5% (noise) |
| real-log `model::resolve` | 3.830 ms | 3.819 ms | −0.3% (noise) |
| real-log `analysis::analyze` | 86.611 ms | 89.767 ms | +3.6% (+3.16 ms) |
| real-log `full_pipeline` | 168.620 ms | 171.220 ms | **+1.5%** (+2.60 ms) |

Raw samples (base / tip, three pairs, ms unless noted):

| Stage | base | tip |
|---|---|---|
| fixture `analysis::analyze` | 18.547 / 18.615 / 18.618 | 19.698 / 19.362 / 19.395 |
| fixture `full_pipeline` | 29.148 / 28.681 / 28.898 | 30.115 / 29.844 / 29.965 |
| real-log `analysis::analyze` | 85.913 / 88.092 / 86.611 | 88.996 / 90.191 / 89.767 |
| real-log `full_pipeline` | 165.92 / 168.62 / 169.12 | 169.38 / 171.24 / 171.22 |

The gate is `full_pipeline` (the plan's "no >5% pipeline regression"), and
both arms clear it — the fixture arm by the smaller margin at +3.7%. The
cost is concentrated in `analyze`, as expected for the extra event-stream
traversals, and `build_report` dilutes it on the real log.

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

### MSTREAM update (2026-08-10) — the axilog rows only

The tables above are the 2026-08-09 v0.2.0 capture and are LEFT AS
MEASURED. MSTREAM re-measured the **axilog** side only, on the same machine
and the same two logs, with the same matched-flag surface. **The Elite
Insights side was NOT re-run** — the numbers below are therefore an
axilog-vs-axilog delta, and the full head-to-head comparison is re-run
separately once MSTREAM merges.

Two things changed since the 2026-08-09 capture and both are visible here:
MSTREAM itself (streaming ei-json serialization), and MEIGAP/MEIGAP2, which
roughly doubled the ei-json document (183 MB → 366 MB on the large log) and
had therefore already pushed the pre-MSTREAM peak from 1,281 MiB up to
2,389 MiB. "Base" below is `0a8cf25` (the MEIGAP2 merge, MSTREAM's parent),
NOT the v0.2.0 tag the tables above used.

Method: 3 runs each, median wall / peak RSS via `/usr/bin/time -f '%M %e'`.
Output byte-identical between base and tip in every row (96/96 `cmp` checks
across the full flag matrix — see MSTREAM's report).

Large log (7.6 MB zevtc, 583,194 events):

| pipeline | wall base → tip | peak RSS base → tip |
|---|---|---|
| axilog, matched ei-json surface | 3.24 s → **2.07 s** (−36%) | 2,389 MiB → **117.0 MiB** (−95.1%, 20.4× less) |
| axilog, matched + gzip output | 3.71 s → **2.57 s** (−31%) | 2,389 MiB → **117.0 MiB** (−95.1%) |
| axilog, default native JSON | 0.31 s → 0.32 s (unchanged) | 84.0 MiB → 84.0 MiB (untouched by MSTREAM) |

Small log (1.5 MB zevtc, 120,435 events):

| pipeline | wall base → tip | peak RSS base → tip |
|---|---|---|
| axilog, matched ei-json surface | 0.33 s → **0.23 s** (−30%) | 158.7 MiB → **24.5 MiB** (−84.6%, 6.5× less) |
| axilog, default native JSON | — | 20.2 MiB (untouched) |

Against the EI numbers in the tables above (stale on the EI side, but EI's
own code did not change): axilog's matched ei-json peak on the large log is
now **117 MiB vs EI's 857 MiB**, i.e. axilog uses ~7.3× LESS memory where it
previously used 1.5× more. Peak RSS was the one metric on which EI beat
axilog; it no longer does. The wall-clock lead widened at the same time,
because the streaming path never allocates the document tree.

The remaining 117 MiB is what the goal called for — analysis results plus a
1 MiB write buffer — and it is now essentially FLAT in output size: the
same binary emits a 2.8 MB flagless document at 83.9 MiB peak and a 366 MB
all-flags document at 117.0 MiB peak, a 33 MiB spread across a 130× output
size range. (Pre-MSTREAM the same pair was 119 MiB → 2,389 MiB.) What is
left scales with the ANALYSIS, not the serialization: `--timeseries`'s
per-(player, enemy) series are the bulk of it.

### Honest notes

- **EI's fixed startup dominates small logs**: ~2 s of the EI number is dotnet start + JIT,
  paid PER SPAWN — and axibridge spawns the CLI once per uploaded log, so every upload pays it.
  axilog's fixed cost is ~0.
- ~~**axilog's matched-surface peak RSS is HIGHER than EI's on the large log** (1,281 vs
  857 MiB): the ei-json path builds the entire 183 MB document as an in-memory
  `serde_json::Value` tree before writing, while EI streams to `.json.gz`. The always-on
  native path (82 MiB) doesn't have this problem. Future item: stream the ei-json
  serialization. (The wall-clock lead survives regardless.)~~ **FIXED by MSTREAM
  (2026-08-10)** — the "future item" was done: `axilog_ei::write_ei_json` streams the
  document row by row and the CLI writes it through a `BufWriter`, taking the large-log
  matched-surface peak to **117 MiB**, below EI's 857 MiB. See the
  [MSTREAM update](#mstream-update-2026-08-10--the-axilog-rows-only) above.
- **Output sizes diverge for content reasons**: EI's `.json.gz` is 3.77 MB (45.9 MB
  decompressed); axilog's matched ei-json is 183 MB plain but **2.07 MB gzipped** — smaller
  than EI's, because axilog's bulk is highly-repetitive per-target arrays over its full
  624-entry target roster (EI exports a curated 57), while EI's payload includes things axilog
  doesn't emit (full skill/buff DB metadata, phases, PvE machinery).
- Not a feature-identical comparison: EI computes phases and its full DB-backed surface;
  axilog computes its documented WvW parity surface (see README's EI-JSON parity table). The
  matched-flag config is the closest apples-to-apples available and is exactly the
  axibridge-production shape.

## After MEIGAP Task 2 — the POWER series split

Same machine and harness as the baseline above, measured 2026-08-10 against
`eb57bef` (the MEIGAP Task 1 tip). Same method as Task 1's section above:
the MEDIAN of **three alternating base/tip pairs**, run back to back from
two prebuilt bench binaries so no rebuild sits between the halves of a pair.

**Only one of this task's four families costs anything here.** The three
`targets[]` mirrors (`build_enemy_series`, `build_enemy_dist`,
`target_conditions::build`) are STANDALONE passes, not part of `analyze()` —
they run only when the ei-json adapter's corresponding flag is set, so
`analyze()` and every native/table/csv path are untouched by them. What
`analyze()` gained is family (a): the POWER split inside the existing
`timeseries::build` pass — one `condition_catalog::is_condition_damage_based`
probe per already-filtered damage row, plus a second per-bucket delta series
for `damage_taken` and for each (player, enemy) pair. The 1S-grid fix adds
at most one extra bucket per series.

| Stage | Before (`eb57bef`) | After | Δ |
|---|---|---|---|
| fixture `analysis::analyze` | 19.363 ms | 20.113 ms | +3.9% |
| fixture `full_pipeline` | 29.613 ms | 30.619 ms | **+3.4%** |
| real-log `analysis::analyze` | 89.737 ms | 93.353 ms | +4.0% |
| real-log `full_pipeline` | 170.27 ms | 175.51 ms | **+3.1%** |

The gate is `full_pipeline` ("no >5% pipeline regression") and both arms
clear it.

**Noise disclosure.** This machine's spread is large relative to the effect:
across the three base runs in this session the real-log `analysis::analyze`
median-of-run values were 89.737 / 82.398 / 92.668 ms — a 12% span for the
*same binary*. The medians above are the honest summary, but the real-log
`analyze` delta in particular should be read as "a few percent", not as
4.0% ± something small.

One allocation cleanup was made and re-measured: the per-(player, enemy)
POWER fallback in `timeseries::build_with_registry` originally built a fresh
`vec![0; buckets]` per call, which on a real log fires 44 × 624 times. It is
now a single hoisted `zeros` row. The re-measurement above is the post-hoist
code; the difference versus the pre-hoist numbers (fixture `full_pipeline`
+1.1%, real-log +3.4% in the first session) sits inside the spread just
described, so the hoist is recorded as an allocation correctness cleanup,
not as a measured win.

## After MEIGAP Task 3 — the healing/minion/guild remainder

Same machine and harness as the baseline above, measured 2026-08-10 against
`dae801d` (the MEIGAP Task 2 tip). Same method as Tasks 1 and 2: the MEDIAN
of **three alternating base/tip pairs**, run back to back from two prebuilt
bench binaries so no rebuild sits between the halves of a pair.

**Task 3 adds no work at all to `analysis::analyze`.** All four new passes
are STANDALONE — `healing_detail::build` and `minions::build` run only when
the ei-json adapter is going to serialize them (`--skill-damage` /
`--timeseries`), and `healing_detail::build` additionally checks the
extension registration before it will even build an `InstidRegistry`, the
same cold-path hoist `healing::apply` already documents. The outgoing
boon-strip duration is a refactoring of counting that `support::apply`
already did (one shared primitive now produces both the count and the
duration, replacing two inline increments).

The one genuinely always-on addition is the `CBTS_GUILD` decode, and it is
folded into `markers::resolve_markers`'s existing whole-stream pass rather
than paying for its own.

| Stage | Before (`dae801d`) | After | Δ |
|---|---|---|---|
| fixture `model::resolve` | 658.09 µs | 682 µs | +3.6% |
| fixture `analysis::analyze` | 20.051 ms | 19.972 ms | −0.4% |
| fixture `full_pipeline` | 31.503 ms | 30.764 ms | **−2.3%** |
| real-log `model::resolve` | 4.675 ms | 3.993 ms | −14.6% |
| real-log `analysis::analyze` | 99.381 ms | 95.485 ms | −3.9% |
| real-log `full_pipeline` | 176.90 ms | 182.32 ms | **+3.1%** |

Every one of these is inside this machine's run-to-run spread and none of
them is a real effect: the code path they measure is unchanged except for
one `u8` compare per event inside an existing loop. The honest reading is
"no measurable change", and the gate (`full_pipeline`, no >5% regression)
is cleared on both arms.

**Noise disclosure, again.** Three consecutive base runs of the SAME binary
span 176.89–181.24 ms on real-log `full_pipeline` and 94.87–112.12 ms on
real-log `analysis::analyze` — a 18% span on the latter. The +3.1% real-log
`full_pipeline` figure above is dominated by a single 192.10 ms tip sample
whose own `decode_raw` (untouched code) also ran long in that pass; the
other two tip samples are 176.13 and 182.32 ms, straddling the base median.

### One measurement that changed the design

The guild decode was first written as its own `raw.events.iter()` pass in
`wvw::apply`, beside the existing MAP_ID/WVW_TEAMS `find`s. Measured, that
cost **+12% on fixture `model::resolve`** (645 → 725 µs) and **+18% on the
real log** (3.85 → 4.56 ms) for a single `u8` compare per event — the cost
is the iteration over a multi-hundred-thousand-element `Vec<RawEvent>`, not
the work. Folding it into `markers::resolve_markers`'s existing pass
(`markers::resolve_markers_and_guilds`) brought it back inside the noise
floor. Recorded because it is the same lesson Task 1's CC-scan merge taught
in the opposite direction: on this event volume, *how many times the stream
is walked* dominates *what is done per row*.

### Payload sizes (committed fixture, compact `ei-json`)

| Flags | `dae801d` | Task 3 tip | Δ |
|---|---|---|---|
| *(none)* | 262,226 | 264,977 | **+1.05%** |
| `--skill-damage` | 1,003,731 | 1,195,967 | **+19.2%** |
| `--timeseries` | 1,490,920 | 1,502,833 | **+0.8%** |
| both | 2,232,425 | 2,433,823 | +9.0% |

The flagless delta is `players[].guildID` (38 of 42 players) plus one
`support[0].boonStripsTime` number each — the only two always-on additions.

`--skill-damage` carries `outgoingHealingAllies` + `outgoingBarrierAllies`
(94,436 B, the bulk of it), `totalHealingDist` + `totalBarrierDist`, and
`minions[]`. `--timeseries` carries `healing1S` (11,913 B).

**The ally matrices were built always-on first and rejected on
measurement.** GW2EI emits them unconditionally
(`EXTJsonPlayerHealingStatsBuilder.cs:73` sits outside every
`RawFormatTimelineArrays` block), so always-on was the faithful shape — but
it puts the flagless document at 356,662 B (**+36.0%**), past the ~30% band
every always-on block in this schema has been held to, and the array is
`players x players`: it grows QUADRATICALLY in squad size (41x41 on this
fixture, 48x48 on the reference capture, 10,000 cells on a 100-player log).
They ride `--skill-damage` instead — the same payload-only gate, with the
same reasoning, MEIGAP Task 2c gave `targets[].totalDamageDist`, and a flag
axibridge hardcodes to `true`.

Measurement note: these figures are `json.dumps(..., separators=(',',':'))`
over the rendered document. MEIGAP Task 2's report quoted 289,017 B for the
flagless base; that came from a different compaction method and is not
comparable to this table. Base and tip here were measured with one method
in one session.

### Review-wave note (no re-measurement)

The whole-branch review wave that followed changed only docs, tests, the
committed golden and dead code, plus two guards: `anonymize_raw_evtc` now
rejects a non-revision-1 file up front, and `healing_detail`'s extension
check was hoisted so it is asked once per pass instead of three times (an
`.any()` that short-circuits on the present path and, on the absent path,
now runs once before the early return instead of twice). The four rendered
`ei-json` documents (flagless, `--skill-damage`, `--timeseries`, both) are
**sha256-identical** across the wave, so the numbers above stand unchanged.

## MEIGAP2 — the six audit rows (payload + MPERF)

### Runtime: no measurable always-on cost

MEIGAP2 added four things to the default path — the `InstidRegistry`
reverse index (`instanceID`), the enemy outgoing-damage fold
(`targets[].dpsAll`), the dealt-breakbar accumulation
(`dpsAll[0].breakbarDamage`) and the per-skill split of the existing
down-contribution credits. Every one of them was deliberately shaped to
ride an existing scan rather than add one:

| Addition | Where it runs |
|---|---|
| `instanceID` reverse index | a post-pass over `InstidRegistry`'s registration lists (one entry per ownership CHANGE, not per event) — no event scan at all |
| enemy outgoing damage | folded into `analyze()`'s existing `combat_participant_enemies` scan, which already walks every event with the same skip set |
| dealt breakbar | folded into `defenses::accumulate_breakbar_and_received_cc`, the scan that already classifies the same result byte for the incoming side |
| per-skill down contribution | one extra `+=` inside `contribution::credit_window`'s existing damage branch, over its already-narrowed per-down windows |

The two genuinely new passes (`dist_outcomes::build` and
`health::ei_health_percents`) are standalone opt-in builders on
`--skill-damage`/`--timeseries`, so a flagless parse never calls them.

Measured end-to-end, base (`be4a97c`) vs tip, `axilog parse <583k-event
real log> --format json -o /dev/null`, 12 interleaved A/B runs on a loaded
desktop, minimum taken (the noise-robust statistic — this machine's
criterion runs were varying by more than the effect being measured, with
one ablation that REMOVED work timing 15% slower than the code that
includes it):

| | base | tip |
|---|---|---|
| min | 330.5 ms | **327.3 ms** |
| mean | 355.7 ms | 359.5 ms |

i.e. no regression distinguishable from this machine's noise floor in
either direction, well inside the ~0.4% MPERF headroom the milestone was
given. `cargo bench -p axilog-cli --bench pipeline` remains the canonical
harness for a quiet machine; the interleaved-min method above is what was
used here because the machine was not quiet.

### Payload

`wc -c` over the rendered document, base vs tip:

| Document | base | tip | delta |
|---|---|---|---|
| native `json`, flagless (fixture and real log) | — | — | **+0.000%** |
| `ei-json` flagless, committed fixture | 683,155 | 694,919 | +1.72% |
| `ei-json` flagless, real log | 2,728,519 | 2,803,123 | +2.73% |
| `ei-json --skill-damage --timeseries`, fixture | 10,691,918 | 11,383,674 | +6.47% |
| `ei-json --skill-damage --timeseries`, real log | 348,635,208 | 350,320,890 | +0.48% |

**The native schema does not move at all**, by construction: every new
schema field (`PlayerOut::instid`/`breakbar_damage_dealt`/
`downs_contribution_per_skill`, `EnemyOut::instid`/`damage_out`) is
`#[serde(skip)]`, the same EI-adapter-only role `PlayerOut::agent_addr` and
`Report::all_enemies` already play. That also means no SDK typed surface
(`axilog.pyi`, `types.d.ts`) gains a field; only the two `parse_file_ei`
docstrings were updated, to say which flag now gates what.

The flagless `ei-json` growth is three always-on scalars — `instanceID`,
`dpsAll[0].breakbarDamage` and `targets[].dpsAll` — all of which real EI
also emits unconditionally and all of which axibridge reads without a flag.
The real log's larger share is `targets[]`: it has 624 of them, so a
two-field `dpsAll` object plus an `instanceID` per target dominates. The
gated growth is the distribution outcome columns (fixture: many small rows,
hence +6.5%) plus `healthPercents`/`boonsStates` (real log: dwarfed by the
timeline arrays already there, hence +0.5%).

## After MSTREAM — streaming ei-json serialization

Measured 2026-08-10 against `0a8cf25` (the MEIGAP2 merge). Same machine and
harness as the baseline (Ryzen 9 7900X3D, Linux, `cargo bench -p axilog-cli
--bench pipeline`, release), on a QUIET machine, base and tip run back to
back in one session from two prebuilt bench binaries.

### The four standard stages — unchanged, by construction

MSTREAM touches only `axilog-ei`'s serialization and the CLI's write path.
None of the four benchmarked stages calls either: `full_pipeline` stops at
`axilog_schema::build_report`. The run below is therefore a NULL-RESULT
check (did anything drift?), not a measurement of the change:

| Stage | base `0a8cf25` | tip | Δ |
|---|---|---|---|
| `decode_raw` | 9.2399 ms `[9.1983, 9.2855]` | 9.0395 ms `[9.0008, 9.0837]` | −2.2% (noise) |
| `model::resolve` | 681.06 µs `[678.09, 684.11]` | 670.62 µs `[669.04, 672.59]` | −1.5% (noise) |
| `analysis::analyze` | 20.288 ms `[20.234, 20.343]` | 20.229 ms `[20.178, 20.283]` | −0.3% |
| `full_pipeline` | 30.611 ms `[30.502, 30.731]` | 30.569 ms `[30.473, 30.671]` | −0.1% |

Every stage is inside noise and every delta is in the *favourable*
direction, i.e. nothing regressed. (These absolute numbers are lower than
the MPERF Task 1 baseline table at the top of this file for
`analysis::analyze` — 20 ms vs 39 ms — because that table was captured on a
much busier machine; base and tip here were captured minutes apart under
identical conditions, which is what makes the Δ column meaningful and the
cross-section absolute comparison not.)

### Where the change actually shows: CLI peak RSS and wall clock

`/usr/bin/time -f '%M %e'`, best of 3, `--format ei-json -o <file>`:

| Log / flags | base peak RSS | tip peak RSS | Δ | base wall | tip wall |
|---|---|---|---|---|---|
| real log, all flags | 2,446,596 KB (2,389.3 MiB) | **119,896 KB (117.1 MiB)** | **−95.1% (20.4×)** | 3.30 s | **2.20 s** (−33%) |
| real log, `--timeseries` | 2,220,164 KB (2,168.1 MiB) | 109,508 KB (106.9 MiB) | −95.1% (20.3×) | 2.26 s | 1.27 s (−44%) |
| real log, flagless | 122,044 KB (119.2 MiB) | 85,940 KB (83.9 MiB) | −29.6% | 0.35 s | 0.32 s |
| committed fixture, all flags | 162,904 KB (159.1 MiB) | 25,072 KB (24.5 MiB) | −84.6% (6.5×) | 0.32 s | 0.23 s (−28%) |

The wall-clock win is a side effect, not a separate optimization: the old
path allocated ~366 MB of `serde_json::Value` nodes, then walked them into a
~366 MB `String`, then wrote that. The new path allocates one player row at
a time and writes through a 1 MiB `BufWriter`.

The native `--format json` path was deliberately NOT changed (84.0 MiB base
→ 84.0 MiB tip on the real log, 0.31 s → 0.32 s): it was never the problem.

### The SDK path, measured rather than assumed

`to_ei_json` must hand back a materialized `serde_json::Value` — napi and
pythonize both walk a tree — so there is nothing to stream away for the
SDKs. Three implementations were candidates; two were measured against the
third (`crates/axilog-ei/examples/mstream_sdk_options.rs`, real log,
`--skill-damage --timeseries --rotation` surface, best of 2):

| `to_ei_json` implementation | peak RSS | build time |
|---|---|---|
| base `0a8cf25`: standalone `json!` tree-builder | 1,631 MB | 1.02 s |
| **shipped**: `serde_json::to_value(&ei_doc(..))` | **857 MB** | **0.74 s** |
| rejected: `write_ei_json` into a `String`, then `from_str` | 1,478 MB | 1.70 s |

The shipped option is not a compromise — it beats the code it replaced on
both axes while removing the second definition of the format. The reason
the old tree-builder was the WORST of the three is a `json!` detail:
`json!({ "players": players })` expands the non-literal `players` through
`serde_json::to_value(&players)`, which DEEP-COPIES the whole
`Vec<Value>` while the original is still alive — so the old builder briefly
held two copies of the document. The parse-back option loses for the
obvious reason (it pays a full serialize AND a full parse).

## axilog vs Elite Insights — v0.3.0 rerun (2026-08-10, post-MSTREAM)

Same methodology as the v0.2.0 section above (EI CLI v3.27 + .NET 8.0.25, axibridge's
production `settings.conf`; axilog release build, matched flag set; medians of 3 after a
warmup; `/usr/bin/time` wall + peak RSS). Both sides re-run in the same session. The matched
axilog surface is LARGER than v0.2.0's (MEIGAP/MEIGAP2 roughly doubled the ei-json document),
and still got faster and 10x lighter thanks to MSTREAM.

### Large log (7.6 MB zevtc, 583,194 events, 48 players, 5:48)

| pipeline | wall (median of 3) | peak RSS |
|---|---|---|
| Elite Insights CLI | 7.25 s (6.75/7.25/7.36) | 857 MiB |
| axilog, matched ei-json surface | **2.49 s (2.9×)** | **117 MiB (7.3× less)** |
| axilog, matched + gzip | 2.86 s | 117 MiB |
| axilog, default native JSON | **0.36 s (20×)** | **84 MiB (10× less)** |

### Small log (1.5 MB zevtc, 120,435 events, 42 players, 49 s)

| pipeline | wall (median of 3) | peak RSS |
|---|---|---|
| Elite Insights CLI | 2.43 s | 373 MiB |
| axilog, matched ei-json surface | **0.25 s (9.7×)** | **24 MiB (15× less)** |
| axilog, default native JSON | **0.06 s (40×)** | **20 MiB (18× less)** |

The v0.2.0 section's honest note about EI winning the matched-surface memory column is now
resolved: MSTREAM's streaming serializer (byte-identical, 96/96 combos) flipped 1.5×-worse
into 7.3×-better. The remaining axilog peak is analysis state, not serialization.

## After MROSTER — curating the `targets[]` roster

`ei-json`'s `targets[]` used to be every enemy agent the log enumerated —
624 on the real capture, against the 57 GW2EI's own WvW logic exposes for
the same log. MROSTER curated it to GW2EI's rule (enemy PLAYERS only,
`WvWLogic.cs:325-375`; see `axilog_schema::Report::ei_targets`), leaving 71.

This is the rare change that is a correctness fix AND a large free
performance win, because nine per-player arrays are positionally joined to
`targets[]` and are therefore all `players × targets`-shaped. Cutting the
roster 8.8× cuts every one of them by the same factor. Same machine and
harness as the v0.3.0 rerun above; medians of 3 after a warmup.

### Payload — real 5:48 log (583k events, 48 players)

| surface | before | after | delta |
|---|---|---|---|
| `ei-json` flagless | 2,843,408 B | **821,078 B** | **−71.1%** |
| `ei-json --timeseries` | 339,293,994 B | **46,986,849 B** | **−86.2%** |
| `ei-json`, matched axibridge surface | 365,635,697 B | **63,484,328 B** | **−82.6%** |

### Wall + peak RSS — matched surface

| log | before | after |
|---|---|---|
| Real 5:48 zerg (583k events, 48 players) | 2.60 s · 117 MiB | **1.70 s (1.5×) · 92 MiB** |
| 49 s skirmish (120k events, 42 players) | 0.24 s · 24.5 MiB | 0.25 s · 24.2 MiB |

`--timeseries` alone on the real log goes 1.33 s → **0.56 s (2.4×)**; it is
the mode the roster dominates, because `targetDamage1S` +
`targetPowerDamage1S` are `players × targets × seconds`.

The small log barely moves, and that is the expected shape rather than a
disappointment: it is a 42-player skirmish whose enemy roster is mostly
players already, so there is little NPC bulk to drop (15.2 MB → 10.2 MB,
−33%, with the wall time inside run-to-run noise). The real log's 624-agent
roster — the siege, guards, dolyaks, tactivators and pets of a full borderland
— is where the multiplier lived.

Native (`--format json`) output is **byte-identical** before and after on
every flag combination, on both the committed fixture and the real log
(37,172,472 B): `Report::enemies` and `Report::ei_targets` are independent
filters over the same list, and only the second one moved. So is `--format
html`.

## Native format 1.0 payload

**Task 13** (2026-08-11). Measures the two size claims the 1.0 container
design made and never checked: that catalog dedup (names, IDs, buff/skill
metadata pulled into `catalogs{}` and referenced by index instead of
repeated per row) and RLE-encoded time series (`series` block) make the
1.0 document smaller than the legacy `axilog-schema::Report` shape on the
same content.

Both documents are built from the committed fixture
(`fixtures/wvw-small.anon.zevtc`) with **every** compute gate on — replay,
missiles, skill-damage, timeseries, rotation, and damage-mods — the same
"all gates" construction `v1_shape.rs`'s `build_with_encounter()` uses.
Comparing a fully-populated 1.0 document against a fully-populated legacy
one matters here: the legacy shape's optional fields are `#[serde(skip)]`
when absent, so a partial legacy document would make the ratio look far
better than it is.

The 1.0 shape carries strictly more data than legacy even before dedup is
counted: enemy statistics that legacy's serializer skips, a
`combat_participant` flag, and per-skill `crit_hits`/`flank_hits`. So a
reduction here isn't "the same data organized differently" — it's the same
data plus more, organized to cost less.

### Total size and ratio

```
SIZE legacy=1,630,287 v1=899,179 ratio=0.552
```

The 1.0 document is **45% smaller** than the legacy one carrying equivalent
(plus strictly more) content. The catalog-dedup and RLE claims are borne
out: dedup means every player/target/skill name and every buff/skill
metadata entry is written once in `catalogs{}` and referenced by a small
integer everywhere else, instead of being repeated as a full string on
every row of `boons`, `damage`, `rotation`, etc; RLE means the `series`
block (190,586 B) — the block most exposed to idle-stretch repetition —
stays a fraction of what a flat per-second array over the same duration
would cost.

### Per-block breakdown (bytes, serialized JSON)

| block | bytes |
|---|---|
| `replay` | 294,141 |
| `series` | 190,586 |
| `rotation` | 117,802 |
| `damage_mods` | 63,507 |
| `boons` | 72,945 |
| `damage` | 53,055 |
| `catalogs` | 23,763 |
| `entities` | 21,509 |
| `defenses` | 19,797 |
| `hit_stats` | 19,283 |
| `contribution` | 6,356 |
| `healing` | 4,661 |
| `cc` | 4,102 |
| `support` | 3,768 |
| `missiles` | 2,567 |

`replay` dominates (33% of the document) — unsurprising, since it is raw
per-frame position/health/state samples for every combat participant over
the whole encounter duration and is not RLE-encoded like `series` is; it is
the block most likely to dominate on longer or higher-player-count logs.
`series` is the second-largest block despite RLE, because the fixture is a
short, busy WvW skirmish with little idle time for the RLE to collapse —
RLE's payoff scales with how much of the timeline is flat, which this
particular fixture doesn't have much of.

### Test bound

`crates/axilog-schema/tests/v1_size.rs` asserts `v1.len() <= legacy.len() *
7 / 10` (0.70), not the `* 12 / 10` (1.20) bound drafted before any
measurement existed. At the measured ratio of 0.552, the original 1.20
bound would have passed even if the entire size win regressed away and 1.0
came out noticeably *larger* than legacy — it verified nothing. 0.70
leaves headroom above the measured 0.552 for normal per-fixture and
per-change drift, while still failing if dedup or RLE silently breaks on a
code path and erodes most of the win.

Run: `cargo test -p axilog-schema --test v1_size -- --nocapture`
