# Contributing to axilog

## Build and test

```sh
cargo build --workspace --release      # binary at target/release/axilog
cargo test --workspace
cargo bench -p axilog-cli --bench pipeline
```

The Node and Python SDKs build from their own crate directories — see
[`crates/axilog-node/README.md`](crates/axilog-node/README.md) and
[`crates/axilog-py/README.md`](crates/axilog-py/README.md).

## Fixture policy

- **Never commit a raw `.zevtc`/`.evtc`.** Real logs contain real GW2 account names.
- Committed fixtures (`fixtures/wvw-small.anon.zevtc`, `fixtures/wvw-small.ei.json`) are
  anonymized and PII-safe — verified by both an automated PII scan (`anonymize_fixture.rs`) and a
  manual independent scan before commit. CI runs the full golden-parity suite against them on
  every run.
- `fixtures/local/` is gitignored and is where real, non-anonymized logs used for local
  development and calibration go. Tests that need one skip gracefully — printing a
  `skip: ... absent` message — when it is not present, e.g. in CI. `AXILOG_LOCAL_FIXTURES` lets
  those tests run from any worktree.
- Run `axilog anonymize <in.zevtc> <out.zevtc>` before sharing a log or filing a bug report. It
  rewrites every player agent's character/account name to a deterministic `Anon<N>` placeholder in
  place and preserves every other byte, so parsed metrics are identical before and after.

There is a standing invitation for a **post-rework capture**: a WvW fight recorded with a current
(post-`20260501`) arcdps build, dropped at `fixtures/local/wvw-postrework.zevtc`, optionally with a
dps.report `getJson` export alongside it at `fixtures/local/wvw-postrework.ei.json`. Then run
`cargo test -p axilog-core --test postrework_golden` — no code changes needed; the tests pick the
fixtures up automatically. See
[`docs/EI-PARITY.md`](docs/EI-PARITY.md#how-to-provide-a-post-rework-fixture) for the details.

## The accuracy bar

Calibrated numbers stay EI-exact or get a documented, ruled exception. An exception is not a
loosened tolerance: it is a written trace of the divergence's root cause, an explicitly authorized
bound set at the *measured* residual plus a margin, and a named allowlist in the test file.

Three sources settle disputes, in order of authority:

1. Implementation guidance relayed directly from the arcdps developer.
2. The [GW2 Elite Insights](https://github.com/baaron4/GW2-Elite-Insights-Parser) source, read at a
   pinned commit and cited by file and method — the arbiter for any algorithm that has to match
   EI's output.
3. The arcdps EVTC reference, hand-counted for enum ordinals, because the published reference
   contains errors.

## arcdps-dev guidance

Guidance relayed directly from the arcdps developer (event ids, payload layouts, upcoming features
to build against) is tracked as a running log in
[`docs/arcdps-dev-notes.md`](docs/arcdps-dev-notes.md), with a status per item and a pointer to the
milestone task that implements it.

## Milestone workflow

Work is organized as milestones, not loose PRs. Each one runs
spec → plan → subagent-driven execution in an isolated git worktree → an adversarial review per
task → a final whole-branch review → merge to `main`. Reviewers reproduce numbers independently
rather than reading the implementer's summary.

- [`docs/ROADMAP.md`](docs/ROADMAP.md) — what is done, in flight, queued and parked.
- [`docs/CHANGELOG.md`](docs/CHANGELOG.md) — the shipped history, grouped by release.
- [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) — the performance record, including every
  optimization that was *declined* and why.
- `docs/superpowers/` — per-milestone specs, plans and reports.

Cross-cutting invariants every milestone has to keep green: all existing calibration exact; no PII
committed; HTML determinism, the `textContent`-only XSS contract and the asset size budgets
honored; warning-free builds; the Node, Python and JS suites green on any schema ripple.

## Releasing

Tag-triggered — see [`RELEASING.md`](RELEASING.md).
