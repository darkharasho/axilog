# axilog

A fast, cross-platform combat-log parser for Guild Wars 2 arcdps logs. Part of the axi suite.

A Rust parsing core (`axilog-core`) with a CLI, a Node SDK ([napi-rs](https://napi.rs)) and a
Python SDK ([PyO3](https://pyo3.rs)) built on it as native extension modules — not subprocess
wrappers. Point it at a `.zevtc` and get back structured JSON, a terminal table, CSV, or a
single-file interactive HTML report.

- **Fast.** A real 583k-event WvW log parses and fully analyzes in **~174 ms**, single threaded,
  with no `unsafe`. Small logs land in **60 ms**. Fixed startup cost is effectively zero, so
  per-log tooling pays nothing per invocation. See [Speed](#speed).
- **Deep analysis, not just DPS.** Damage and down contribution, CC and stun breaks, boon uptime
  with self/group/squad generation attribution, condi cleanses and boon strips, healing and
  barrier, per-skill distributions, per-second timelines, hit quality, defenses, cast rotations,
  damage-modifier attribution over a 205-definition catalog, and combat-replay position tracks.
- **Three ways to use it.** A single static binary, `npm install @axiapps/axilog`, or
  `pip install axilog` — the same core behind all three, no runtime to install alongside.
- **Calibrated, not approximated.** Every metric is asserted against a real reference export in CI
  on every run; divergences are documented with a traced cause, never a loosened tolerance. See
  [Accuracy](#accuracy).
- **WvW-first.** Validated against real WvW logs on both arcdps log eras (pre- and
  post-`20260501`). PvE encounter logic — boss health phases, mechanics, phase splits — is not
  implemented; the report exposes a single whole-fight phase.

## Speed

Release build, Ryzen 9 7900X3D, end to end (decode → resolve → analyze → serialize), medians of 3.
Elite Insights CLI v3.27 on the same machine and logs for scale:

| Log | Events | Players | axilog | Elite Insights |
| --- | --- | --- | --- | --- |
| 49 s skirmish | 120,435 | 42 | **70 ms** · 23 MiB | 2.27 s · 374 MiB |
| Real 5:48 zerg fight | 583,194 | 48 | **400 ms** · 90 MiB | 6.75 s · 860 MiB |

Every metric listed above, computed on a whole zerg fight in under half a second on one
thread — **17–32× faster at 10–16× less memory**. Emitting the full Elite Insights-compatible
document instead, the heaviest thing axilog can produce (replay tracks and all), takes 1.59 s at
107 MiB — still 4.2× faster at 8.0× less memory than EI's own equivalent configuration.
[Full head-to-head and methodology →](#vs-elite-insights)

## Install

Every artifact comes from one tag-triggered release pipeline, attached to a single GitHub Release
per version alongside a consolidated `SHA256SUMS`.

```sh
# Node SDK — https://www.npmjs.com/package/@axiapps/axilog
npm install @axiapps/axilog

# Python SDK — https://pypi.org/project/axilog/ (cp39-abi3, one wheel per platform)
pip install axilog
```

For the CLI, take the archive for your platform from the
[Releases page](https://github.com/darkharasho/axilog/releases) and verify it before extracting:

```sh
sha256sum -c axilog-X.Y.Z-<target>.tar.gz.sha256   # .zip on Windows
tar xzf axilog-X.Y.Z-<target>.tar.gz
./axilog parse fight.zevtc --format table
```

Published targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. From source, with the
toolchain in `rust-toolchain.toml`: `cargo build --workspace --release`.

## Usage

```sh
axilog parse <log.zevtc|log.evtc> [--format FORMAT] [--view VIEW] [-o FILE] [FLAGS...]
axilog anonymize <in.zevtc> <out.zevtc>
```

`-o/--output FILE` writes to a file instead of stdout, for every format. `axilog parse --help` is
the authoritative, measured reference for every flag — what it computes, which formats consume it,
and its size/time cost. The tables below are the index.

### Formats (`--format`, default `json`)

| Format | What it is |
| --- | --- |
| `json` | axilog's native format 1.0 (`axilog_schema::v1::ReportV1`) — the richest, most accurate output; every other format is a lossy view of it. [Reference →](docs/NATIVE-FORMAT.md) |
| `table` | Human-readable per-player summary, one row per player (see `--view`) |
| `csv` | The same fields as the default `table` view, machine-readable |
| `ei-json` | A subset of the real Elite Insights / dps.report JSON shape, for tools that already consume it |
| `html` | A single self-contained interactive report — inlined CSS/JS/data, zero network requests |

### Table views (`--view`, `table` format only; `default` shown)

```
account                  profession       damage      DPS  downs  kills  deaths
:Anon104.4848            Engineer         205612     4172      1      0       0
```

| View | Columns |
| --- | --- |
| `default` | damage, DPS, downs, kills, deaths |
| `support` | condi cleanses, boon strips, resurrects, stun breaks |
| `boons` | Might average stacks, presence % for Quickness/Alacrity/Stability/Protection |
| `healing` | arcdps healing-extension totals; renders `-`, not misleading zeros, when the log has no healing data |
| `defense` | blocks, evades, dodges, damage taken with a strike/condi split, downs taken |
| `rotation` | animated-cast count and APM (does **not** require `--rotation`) |

### Opt-in analysis flags

All off by default — each materially inflates the output. Percentages are measured on the fixture.

| Flag | Adds | Cost |
| --- | --- | --- |
| `--replay` | Combat replay — native position tracks (`json`, and the HTML Replay tab), or GW2EI's own map-pixel `combatReplayData`/`combatReplayMetaData` (`ei-json`) | `ei-json` +142% compact |
| `--missiles` | Projectile analytics: per-player fired/hit/denied, squad-wide incoming rollup (native only) | small |
| `--skill-damage` | Per-skill damage distribution, outgoing and incoming → EI's `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` | `json` +249% |
| `--timeseries` | Per-second cumulative series and `dpsTargets` → EI's `damage1S`/`targetDamage1S`/`damageTaken1S`/`dpsTargets` | `json` +148% / +36% |
| `--rotation` | Full per-cast list → EI's flat `rotation[]` | `json` +67% |
| `--modifiers` | Damage-modifier attribution over a 205-definition GW2EI-derived catalog → EI's `damageModifiers*` and `damageModMap` | `json` +44%, `ei-json` +442%; the only flag that gates *computation*, 0.074 s → 0.155 s |

### HTML report

`axilog parse fight.zevtc --format html -o report.html` renders one self-contained dark-theme
(light-mode toggle) document: CSS, JS and the report's own JSON are all inlined, so there are no
external requests and nothing to ship alongside it. Sortable Damage/Support/Boons tables, a header
with map/duration/commander/team chips, an inline SVG damage timeline with downs and CC overlaid,
and — with `--replay` — an animated combat-replay tab. Every log-derived string reaches the DOM via
`textContent` only; structure, determinism and size budgets are gated in CI.

### Anonymize before sharing

`axilog anonymize <in.zevtc> <out.zevtc>` rewrites every player agent's character/account name to a
deterministic `Anon<N>` placeholder in place, preserving every other byte — the event stream, the
skill table, NPC/gadget agents — so parsed metrics are identical before and after. Use it before
filing a bug report, sharing a log, or committing a fixture.

## Performance

The [Speed](#speed) figures are whole-process wall clock. Broken down by stage — decode → resolve
→ analyze → build the native report, release build, Ryzen 9 7900X3D:

| Log | Events | Full parse | of which `analyze` |
| --- | --- | --- | --- |
| Committed fixture (`fixtures/wvw-small.anon.zevtc`) | 120,435 | **28.9 ms** | 18.9 ms |
| Real WvW zerg log (48 players, 5:48 fight) | 583,194 | **174 ms** | 93.7 ms |

Analysis is roughly half the budget and every pass is single-scan where the data allows; the
benchmark harness (`crates/axilog-cli/benches/pipeline.rs`, criterion) and every optimization that
was *declined*, with reasons, are in [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md).

### vs Elite Insights

axilog is calibrated against [Elite Insights](https://github.com/baaron4/GW2-Elite-Insights-Parser),
so a like-for-like timing is worth recording. Measured 2026-08-16 on axilog v0.3.2 against the EI
CLI v3.27.1.0 plus its bundled .NET 8.0.25 runtime, same machine, medians of 3 after a warmup.
"Matched" is a production EI configuration (detailed WvW, damage modifiers, combat replay, raw
timeline arrays, phases) against axilog's equivalent flag set:

| | Real 5:48 zerg (583k events) | 49 s skirmish (120k events) |
| --- | --- | --- |
| Elite Insights CLI | 6.75 s · 860 MiB peak | 2.27 s · 374 MiB peak |
| **axilog, matched surface** | **1.59 s (4.2×) · 107 MiB (8.0× less)** | **0.26 s (8.7×) · 29 MiB (12.7× less)** |
| **axilog, native `--all`** | **1.06 s (6.4×) · 97 MiB (8.9× less)** | **0.16 s (14×) · 25 MiB (15× less)** |
| **axilog, default native JSON** | **0.40 s (17×) · 90 MiB (9.6× less)** | **0.07 s (32×) · 23 MiB (16× less)** |

Not a feature-identical comparison: EI additionally computes phases and its full skill-DB surface,
while axilog emits its documented WvW parity surface. Two structural notes — EI pays ~2 s of .NET
startup and JIT *per spawned parse* (axilog's fixed cost is effectively zero), and memory was once
EI's one win, until axilog's streaming serializer cut peak RSS 95% (byte-identical across 96
flag/output combinations). The large-log ratio improved 2.9× → 4.2× since v0.3.0 because curating
the `ei-json` enemy roster cut nine `players × targets`-shaped arrays by 8.8×; the small-log ratio
drifted 9.7× → 8.7× only because EI itself ran faster this session than in that measurement.
[Full tables, sizes and honest notes →](docs/BENCHMARKS.md)

## Accuracy

The bar: **a calibrated number is exactly what Elite Insights prints for the same log, or a
documented, ruled exception with a traced cause.** An exception is not a loosened tolerance — it is
a written root-cause trace, a bound set at the *measured* residual plus a margin, and a named
allowlist in the test file. Two references enforce it: a committed anonymized golden
(`fixtures/wvw-small.anon.zevtc` plus `fixtures/wvw-small.ei.json`, a real dps.report export of the
same log) asserted in CI on every run, and a larger post-rework capture living gitignored under
`fixtures/local/`, so the post-`20260501` wire shape has real numbers and not only synthetic ones.
Where the two disagree about method, the GW2EI source is the arbiter, read at a pinned commit and
cited by file and line. What that produces, on the surfaces axilog covers:

- **Exact vs EI** on fight duration, map, team ids, squad damage, CC applied and stun breaks, condi
  cleanses / boon strips / resurrects (squad *and* per-player), boon presence and average stacks,
  `activeTimes`, `statsAll` hit quality, `defenses` outcome counts, `healthPercents`, `instanceID`,
  per-target offensive splits, the enemy-side `targets[]` mirrors, healing/barrier detail,
  `guildID`, and the whole `--modifiers` `damageModMap`.
- **Deliberately different**, documented as such: down contribution follows the arcdps developer's
  health-anchored methodology rather than EI's window (EI has no equivalent for three of its four
  stats); `lifeLeechDamageTakenCount` and `boonStripsTime` emit the true value instead of
  reproducing verified GW2EI counting bugs.
- **Honest gaps**, never faked with a plausible value: `wvWMapData` objective-capture timelines are
  not decoded; the cast pipeline covers `AnimatedCastEvent` but not `InstantCastEvent` (~29% of a
  real log's cast entries); `skillMap` names come from the log's own table, not EI's skill DB; 6 of
  the reference export's 75 damage-modifier ids need engine features that do not exist yet. A field
  axilog cannot compute is left out of `ei-json`, not guessed.

Every row, its status, its measured residual and the test that pins it:
[`docs/EI-PARITY.md`](docs/EI-PARITY.md), or the wiki's
[accuracy page](https://arcdps.axi.link/axilog/accuracy/) for the reader-facing version.

## SDKs

Both SDKs are native extension modules over the same Rust core the CLI drives — no reimplementation
and no JSON-over-subprocess; parity tests assert they stay identical to the CLI's output.

```js
const { parseFile, parseFileEi } = require('@axiapps/axilog')
const report = parseFile('./fight.zevtc')                 // native format 1.0, typed via index.d.ts
const ei = parseFileEi('./fight.zevtc')                    // Elite Insights JSON shape
console.log(report.entities.length, report.blocks.damage.squad.total)
```

```python
import axilog
report = axilog.parse_file("./fight.zevtc")               # native format 1.0, typed via axilog.pyi
ei = axilog.parse_file_ei("./fight.zevtc")                 # Elite Insights JSON shape
print(len(report["entities"]), report["blocks"]["damage"]["squad"]["total"])
```

See [`docs/NATIVE-FORMAT.md`](docs/NATIVE-FORMAT.md) for the shape of `report` — six top-level
keys (`axilog`, `encounter`, `entities`, `catalogs`, `blocks`, `coverage`), id-first, no positional
joins.

Full API (`parseFile`/`parseBuffer`/`parseFileEi`/`anonymizeFile` and their Python equivalents), the
opt-in analysis options and build-from-source instructions:
[`crates/axilog-node/README.md`](crates/axilog-node/README.md) ·
[`crates/axilog-py/README.md`](crates/axilog-py/README.md).

## Documentation

User-facing docs live on the arcdps wiki — [overview](https://arcdps.axi.link/axilog/),
[quickstart](https://arcdps.axi.link/axilog/quickstart/),
[methodology](https://arcdps.axi.link/axilog/methodology/),
[schema](https://arcdps.axi.link/axilog/schema/), [accuracy](https://arcdps.axi.link/axilog/accuracy/).
In this repo:
[`docs/NATIVE-FORMAT.md`](docs/NATIVE-FORMAT.md) ·
[`docs/EI-PARITY.md`](docs/EI-PARITY.md) · [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) ·
[`docs/CHANGELOG.md`](docs/CHANGELOG.md) · [`docs/ROADMAP.md`](docs/ROADMAP.md) ·
[`CONTRIBUTING.md`](CONTRIBUTING.md) (build/test, fixture and PII policy, the accuracy bar, the
milestone workflow) · [`RELEASING.md`](RELEASING.md).

## License

MIT — see [LICENSE](LICENSE). Portions are derived from or verified against the MIT-licensed
[GW2 Elite Insights Parser](https://github.com/baaron4/GW2-Elite-Insights-Parser); the exact
relationship and the required license text are in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
