# axilog

axilog is a cross-platform, CLI-first reimplementation of Elite Insights for parsing GW2 arcdps
combat logs, part of the axi suite. It has a reusable Rust parsing core (`crates/axilog-core`)
with a Node SDK (`crates/axilog-node`, native [napi-rs](https://napi.rs) bindings) and a Python SDK
(`crates/axilog-py`, native [PyO3](https://pyo3.rs) bindings) on top, matches standard Elite
Insights (EI) functionality for the metrics it currently covers, and follows the arcdps spec more
closely than EI in a few places — notably down
contribution, CC-over-time, and full per-second timeline support. Unlike the original EI, it isn't
tied to a single OS.

Current focus is WvW logs. Per-player rotation (cast tracking) and a best-effort skill map are
implemented (M14, opt-in `--rotation` for the former, always-on for the latter), as is
damage-modifier attribution (M16, opt-in `--modifiers` — see **Usage**
below). PvE encounter logic (boss health phases, mechanics) is not implemented yet (see Milestones
below).

## Install

Every option below is produced by the same tag-triggered release pipeline
(`.github/workflows/release.yml` — see `RELEASING.md`) and attached to one GitHub Release per
version, alongside a consolidated `SHA256SUMS`.

### CLI binary (GitHub Release)

Download the archive for your platform from the
[Releases page](https://github.com/darkharasho/axilog/releases) — e.g.
`axilog-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` (`.zip` on Windows) — then verify its checksum
against the release's `SHA256SUMS` before extracting:

```sh
sha256sum -c axilog-X.Y.Z-<target>.tar.gz.sha256
tar xzf axilog-X.Y.Z-<target>.tar.gz
./axilog parse <log.zevtc> --format table
```

Targets published per release: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-apple-darwin`.

### Node SDK (npm)

`@axiapps/axilog` is published to the npm registry (platform binaries resolve automatically
via optionalDependencies):

```sh
npm install @axiapps/axilog
```

```js
const { parseFile } = require('@axiapps/axilog')
console.log(parseFile('./fight.zevtc').players.length)
```

### Python SDK (pip)

`axilog` is published to PyPI (`cp39-abi3` — one wheel per platform covers every
CPython ≥3.9):

```sh
pip install axilog
```

```python
import axilog
print(len(axilog.parse_file("./fight.zevtc")["players"]))
```

### Building from source

Requires a Rust toolchain (see `rust-toolchain.toml`).

```sh
cargo build --workspace --release
# binary at target/release/axilog
```

Or run directly via cargo during development:

```sh
cargo run -p axilog-cli -- parse <log.zevtc> --format table
```

See **SDKs** below for building the Node/Python bindings from source instead of a packaged
release artifact.

## Usage

### Parse a log

```sh
axilog parse <log.zevtc|log.evtc> [--format json|table|csv|ei-json|html] [--view default|support|boons|healing|defense|rotation] [-o FILE] [--replay] [--missiles] [--skill-damage] [--timeseries] [--rotation] [--modifiers]
```

- `json` (default) — axilog's own native schema (`axilog_schema::Report`): encounter info, teams,
  per-player metrics (damage, downs/kills, down contribution, CC, stun breaks, boons, support), and
  a per-second timeline. This is the richest, most accurate representation — the other formats are
  lossy views of it.
- `table` — human-readable summary, one row per player, sorted by damage (or by `--view`'s own sort
  key — see below).
- `csv` — same per-player fields as the default `table` view, machine-readable.
- `ei-json` — a subset of the real Elite Insights / dps.report JSON shape, for tools that already
  consume EI's format. See **EI-JSON parity** below for exactly what is and isn't populated.
- `html` — a single, self-contained interactive report (see **HTML report** below).

`-o/--output FILE` writes the rendered output to `FILE` instead of stdout — works with every
`--format`, not just `html`.

`--replay` (M9) computes and embeds a native-only combat-replay block (per-squad-player and
per-enemy-player-representative position tracks, downsampled to 300ms, plus down/dead intervals) —
`--format json` embeds it as a top-level `replay` field, `--format html` feeds the animated Replay
tab (see **HTML report** below). Off by default (adds meaningfully to output size — see **HTML
report**'s size-budget notes).

The same flag (M15) turns on GW2EI's OWN replay shape for `--format ei-json`: per-actor
`combatReplayData.{positions, orientations, dc, iconURL}` (map pixels on GW2EI's fixed 300ms
polling grid) plus the top-level `combatReplayMetaData`. That is a second, independent engine
(`axilog_core::analysis::ei_replay`) over the same events — not a reshaping of the native block —
because the two shapes differ in grid bounds, units, interval semantics and rounding; see that
module's doc comment. It roughly triples the ei-json payload, hence the same opt-in. `--format
table`/`csv` ignore the flag.

`--missiles` (M10) computes and embeds a native-only, opt-in missile (projectile) analytics
block — per-squad-player `fired`/`hit`/`denied`/`reflected_at_self` counts plus a squad-wide
`incoming_fired`/`incoming_denied` defensive rollup. `--format json` embeds it as a top-level
`missiles` field; every other format ignores it (no comparable shape). Off by default. See
`axilog_core::analysis::missiles`'s module doc for exactly what's attributable — the arcdps wire
format has no blocked/reflected/destroyed reason code, so `denied` is deliberately undifferentiated
and there is no per-player "who denied this" credit anywhere.

`--skill-damage` (M12 Task 1) computes and embeds each squad player's per-skill damage
distribution — outgoing (whole-fight and per-target) and incoming, grouped by skill id.
`--format json` embeds it as each player's `skill_damage` field; `--format ei-json` (M12 Task 3)
maps the same data into EI's own `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` array
shapes; every other format ignores it. Off by default — measured +249% JSON size on the committed
fixture when always-on (see **EI-JSON parity** below and `axilog_schema::Report::players`'s
`PlayerOut::skill_damage` doc comment).

`--timeseries` (M12 Task 2) computes and embeds each squad player's per-second cumulative
`damage`/`damage_taken`/per-target series, plus a per-enemy `dps_targets` whole-fight summary.
`--format json` embeds them as each player's `per_second`/`dps_targets` fields; `--format ei-json`
(M12 Task 3) maps the same data into EI's own `damage1S`/`targetDamage1S`/`damageTaken1S`/
`dpsTargets` array shapes; every other format ignores it. Off by default — measured +147.7%/
+36.4% JSON size respectively on the committed fixture when always-on (see **EI-JSON parity**
below).

`--rotation` (M14 Task 1) computes and embeds each squad player's per-skill cast list — a
GW2EI-`AnimatedCastEvent`-pipeline reproduction, grouped by skill id, each cast carrying
`cast_time_ms`/`duration_ms`/`time_gained_ms`/`quickness` (see `axilog_core::analysis::rotation`'s
module doc for the full cast-boundary/quickness derivation this mirrors, and its documented
`InstantCastEvent`-pipeline scope gap — weapon swaps/procs/instant-cast mechanics aren't decoded).
`--format json` embeds it as each player's `rotation` field; `--format ei-json` maps the same data
into EI's own flat `rotation[]` shape (`{id, skills:[{castTime,duration,timeGained,quickness}]}`,
gated by the SAME presence, not a separate flag); every other format ignores it. Off by default —
measured +66.9% JSON size on the committed fixture when always-on (see **EI-JSON parity** below).
`--view rotation` (M14 Task 3) does NOT require this flag — it reads the underlying per-player cast
data directly, always computed regardless of `--rotation` (that flag only gates the full per-cast
JSON payload).

`--modifiers` (M16) computes and embeds each squad player's damage-modifier attribution — for
every trait/rune/relic/sigil/food modifier the player actually triggered, how many of the eligible
hits it applied to and how much of the damage it accounts for. Backed by a 205-definition
transcription of GW2EI's own descriptor tables (`axilog_core::analysis::damage_mods`), with the
gain formula reproduced exactly: a modifier's share of an observed hit is `g/(100+g)`, not `g/100`,
because the logged damage already contains the bonus. `--format json` embeds each player's
`damage_mods` block plus the top-level `damage_mod_map`; `--format ei-json` maps the same data into
EI's `damageModifiers`/`incomingDamageModifiers`/`damageModifiersTarget`/
`incomingDamageModifiersTarget` plus `damageModMap`; every other format (including `html`) ignores
it. Off by default, and unlike `--rotation`/`--skill-damage`/`--timeseries` this flag gates the
COMPUTATION, not just the serialization — the engine is a separate pass over every damage event
crossed with the whole catalogue, so nothing pays for it unless asked. Measured on the committed
fixture: `--format json` +44.2%, `--format ei-json` +441.5% (the difference is EI's per-target
arrays, which have no native counterpart), wall clock 0.074s → 0.155s. See **EI-JSON parity** below
for the per-id coverage and accuracy numbers.

`--view` (M3/M10) selects the `table` format's column layout — ignored for every other `--format`:

- `default` (default) — damage/DPS/downs/kills/deaths, sorted by damage.
- `support` — condi cleanses, boon strips, resurrects, stun breaks, sorted by cleanses.
- `boons` — Might average stacks plus presence % for Quickness/Alacrity/Stability/Protection,
  sorted by account.
- `healing` (M10) — arcdps healing-extension totals: healing out (total), allies, barrier out,
  downed-ally healing. Renders `-` per row (not misleading zeros) when the log carries no
  healing-extension data at all.
- `defense` (M13) — incoming defenses: blocks, evades, dodges, total damage taken, a strike/condi
  split, downs taken, sorted by damage taken.
- `rotation` (M14) — total animated-cast count plus APM (Actions Per Minute, `casts /
  active_minutes`, using the same active-time base as `ei-json`'s `activeTimes`), sorted by cast
  count. Unlike every other view, this one does NOT require `--rotation` to also be passed — it
  reads the underlying per-player cast data directly (always computed by `analyze()`); `--rotation`
  only gates whether the full per-cast JSON detail is also emitted.

Example `table --view default` output (anonymized account names, from the committed golden fixture):

```
account                  profession       damage      DPS  downs  kills  deaths
:Anon104.4848            Engineer         205612     4172      1      0       0
:Anon171.7327            Engineer         192437     3905      2      1       0
:Anon110.5070            Necromancer      169689     3443      4      0       0
:Anon116.5292            Necromancer      162652     3300      1      0       0
:Anon107.4959            Mesmer           154899     3143      0      0       0
```

Example `table --view support` output:

```
account                  profession    cleanses  strips resurrects stunbreaks
:Anon133.5921            Elementalist        93       0          0          1
:Anon118.5366            Mesmer              76       1          0          1
:Anon125.5625            Ranger              66       0          0          0
:Anon108.4996            Ranger              56       1          1          1
:Anon109.5033            Elementalist        56       0          1          1
```

Example `table --view boons` output:

```
account                  profession   Might(avg)  Quick%   Alac%   Stab%   Prot%
:Anon104.4848            Engineer          19.07    66.2     0.0    72.7    90.9
:Anon105.4885            Ranger            10.15    31.3     0.0    79.5    67.6
:Anon106.4922            Guardian          13.81    44.4     0.0    70.1    98.0
:Anon107.4959            Mesmer             8.64    63.8     0.0    72.8    87.6
:Anon163.7031            Mesmer             8.60    29.0    37.4    77.8    66.2
```

Example `table --view defense` output:

```
account                  profession    blocks  evades  dodges  dmg taken    strike     condi  downs
:Anon119.5403            Guardian          24       2       4      81974     79652      1652      0
:Anon123.5551            Mesmer            15       1       0      69439     67535      1160      0
:Anon130.5810            Guardian          19       0       0      66177     64312       661      1
:Anon105.4885            Ranger            16       0       1      63912     62634      1120      1
:Anon133.5921            Elementalist       3       5       3      63506     62613       667      2
```

Example `table --view rotation` output:

```
account                  profession      casts      APM
:Anon129.5773            Mesmer             55     67.0
:Anon125.5625            Ranger             52     63.3
:Anon118.5366            Mesmer             51     62.1
:Anon140.6180            Mesmer             44     53.6
:Anon119.5403            Guardian           43     52.4
```

### HTML report

```sh
axilog parse <log.zevtc|log.evtc> --format html -o report.html
```

Renders a single, self-contained dark-theme (with a light-mode toggle) HTML document — open
`report.html` directly in a browser, no server or network access needed. Built by the
`axilog-html` crate (`axilog_html::render(&Report) -> String`): the CSS, JS, and the report's own
JSON data are all inlined into one file (`<style>`/`<script>` blocks), so it's safe to email, drop
in a chat, or archive next to the source log — there are no external requests (fonts, CDNs,
analytics, or otherwise) and no other files to ship alongside it.

The report contains:

- **Header** — map name, fight duration, recorded-by account, commander (with commander-tag
  variant when present), a warnings banner (only shown when `Report.warnings` is non-empty), and
  one color-coded chip per team (`red`/`blue`/`green`) with a squad or enemy player count.
- **Damage / Support / Boons tabs** — sortable tables (click a column header to sort, click again
  to reverse) with a squad-totals footer row on the Damage tab:
  - *Damage*: account, character, profession (elite-spec name when active), damage, DPS, downs,
    kills, deaths, down contribution, damage taken.
  - *Support*: cleanses (total/self), boon strips, resurrects, stun breaks, removed-stun seconds.
  - *Boons*: Might average stacks, presence % for Quickness/Alacrity/Stability/Protection/Fury/
    Resistance, and a self/group/squad toggle for Might/Quickness/Alacrity/Stability *generation*
    attribution.

  Non-squad players (subgroup 0) render visually muted in every table.
- **Damage timeline** — an inline SVG chart built from `timeline.per_second`: squad damage as a
  filled area + line (the primary series), downs plotted as circular markers directly on the
  damage line at the second they occurred, and CC-applied plotted as a translucent bar overlay
  normalized to its own scale (a different unit/magnitude than damage, so it intentionally doesn't
  share the damage y-axis — see `buildTimelinePaths`'s doc comment in `report.js`). Time axis in
  `mm:ss`, damage axis in `k`-format (e.g. `45k`). The chart uses an SVG `viewBox` with
  `width: 100%`, so it scales with the browser window rather than being a fixed-size image.
- **Replay tab** (M9, only shown when the report was generated with `--replay`) — an animated combat
  replay: play/pause, a scrub slider, an `mm:ss`/`mm:ss` time readout, and a 1x/4x/8x speed toggle
  (4x default) driving an inline SVG "stage" over `replay.tracks[]`. Squad players render as filled,
  team-colored dots; enemies as hollow (team-colored stroke only, background fill); the commander
  gets a gold ring; a track pulses with a red ring during its `down_intervals` and fades to 25%
  opacity during its `dead_intervals`. There's no real map imagery (the zero-network invariant
  holds) — just an abstract dark field with a subtle grid, sized from `replay.bounds` (padded 5%,
  height capped so a tall/narrow map's real proportions don't stretch the page vertically — the
  SVG's own `preserveAspectRatio` letterboxes the rest). Hovering a dot shows its name via a native
  SVG `<title>` tooltip. All motion is driven by a pure `positionsAt(tracks, t)` function (linear
  interpolation between a track's downsampled samples, holding-then-fading before its first/after
  its last sample) called every animation frame — see `report.js`'s "replay (pure)" section and
  `tests/js/pure_fn_tests.mjs` for the node-tested edge cases (exact-sample hit, between-samples,
  before-first, after-last, empty track).

All number/date formatting and interactive behavior (sorting, the theme toggle, the boon
generation-mode toggle, the timeline, the replay animation) runs client-side in `report.js` against
the embedded JSON — Rust only builds the skeleton and inlines the data, so there's one source of
truth for every value shown. See `crates/axilog-html/assets/report.js`'s header comment for the
project's XSS contract (every log-derived string — player/account names, map name, warnings, ... —
reaches the DOM via `textContent` only, never `innerHTML`), and `crates/axilog-html/tests/golden_html.rs`
for the structural/size/determinism tests that gate this format in CI (replay-enabled reports stay
under a 600KB budget; the combined raw `report.css`+`report.js` assets stay under 60KB).

### Anonymize a log for sharing

```sh
axilog anonymize <in.zevtc> <out.zevtc>
```

Rewrites every player agent's character/account name to a deterministic `Anon<N>` /
`:Anon<N>.<4 digits>` placeholder in place; every other byte (including every combat event, the
skill table, and NPC/gadget agents) is preserved exactly, so parsed metrics are identical before
and after. Use this to produce a PII-safe log before filing a bug report, sharing a log publicly,
or committing it as a test fixture — never commit a raw `.zevtc`/`.evtc` (see Fixture policy).

## SDKs

### Node

`crates/axilog-node` (package `@axiapps/axilog`) is a native addon over the same Rust core the CLI
uses — [napi-rs](https://napi.rs) bindings, not a subprocess wrapper, so there's no JSON-over-pipe
overhead and no separate implementation to drift from the CLI's output (a dual-path parity test
asserts the two stay identical). Not yet published to npm — see below.

```sh
cd crates/axilog-node
npm install
npm run build   # compiles the Rust crate to a platform .node addon
```

```js
const { parseFile, parseFileEi } = require('@axiapps/axilog')

// Native schema (axilog_schema::Report) — the same JSON the CLI's
// `--format json` prints, typed via index.d.ts/types.d.ts.
const report = parseFile('./fight.zevtc')
const squadDamage = report.players.reduce((sum, p) => sum + p.damage.total, 0)
const quickness = report.players[0].boons.find((b) => b.name === 'Quickness')
console.log(report.players.length, squadDamage, quickness?.presence_pct)

// EI-compatibility JSON (axilog_ei::to_ei_json) — the shape axibridge-style
// consumers already read (players[].account, dpsAll[0].damage, buffUptimes[], ...).
const ei = parseFileEi('./fight.zevtc')
```

See [`crates/axilog-node/README.md`](crates/axilog-node/README.md) for the full API
(`parseFile`/`parseBuffer`/`parseFileEi`/`anonymizeFile`), build/test instructions, and how the
TypeScript types are generated/patched.

**Not yet on the npm registry.** The package builds and tests cleanly (CI: linux build + `npm
test`; the addon also builds on Windows/macOS every run), and every tagged release publishes
installable tarballs to its GitHub Release (see **Install** above) — but `npm publish` to the
registry itself is gated on the `NPM_TOKEN` repository secret, which isn't configured yet.

### Python

`crates/axilog-py` (package `axilog`) is a native extension module ([PyO3](https://pyo3.rs)
bindings, `abi3-py39`) over the same Rust core the CLI and Node SDK use — again no subprocess
wrapper, and a CLI-parity test keeps it from drifting from the CLI's own output. Not yet published
to PyPI — see below.

```sh
cd crates/axilog-py
python3 -m venv .venv
.venv/bin/pip install maturin
.venv/bin/maturin develop --release   # compiles the Rust crate into .venv as the `axilog` module
```

```python
import axilog

# Native schema (axilog_schema::Report) — the same JSON the CLI's
# `--format json` prints, typed via axilog.pyi.
report = axilog.parse_file("./fight.zevtc")
squad_damage = sum(p["damage"]["total"] for p in report["players"])
quickness = next(b for b in report["players"][0]["boons"] if b["name"] == "Quickness")
print(len(report["players"]), squad_damage, quickness["presence_pct"])

# EI-compatibility JSON (axilog_ei::to_ei_json) — the shape axibridge-style
# consumers already read (players[].account, dpsAll[0].damage, buffUptimes[], ...).
ei = axilog.parse_file_ei("./fight.zevtc")
```

See [`crates/axilog-py/README.md`](crates/axilog-py/README.md) for the full API
(`parse_file`/`parse_bytes`/`parse_file_ei`/`anonymize_file`), build/test instructions, and the
`axilog.pyi` typed-stub layout.

**Not yet on the PyPI registry.** The extension builds and tests cleanly (CI: linux `maturin
develop` + the stdlib `unittest` suite; the wheel also builds on Windows/macOS every run via
`maturin build`), and every tagged release publishes installable wheels + an sdist to its GitHub
Release (see **Install** above) — but `twine upload` to the registry itself is gated on the
`PYPI_TOKEN` repository secret, which isn't configured yet.

## EI-JSON parity

Calibrated against a real dps.report EI export for one WvW log (Green Alpine Borderlands,
41 friendly players). "Golden" below means the committed anonymized fixture
(`fixtures/wvw-small.anon.zevtc` + `fixtures/wvw-small.ei.json`), asserted exactly by
`crates/axilog-core/tests/golden.rs` in CI on every run — not spot-checked once and left to drift.

| Metric | Status | Detail |
|---|---|---|
| Fight duration | Exact | `durationMS` matches EI's `durationMS` |
| Squad total damage | Exact | matches EI's `squadTotalDamage` |
| CC applied (count / duration) | Exact | `34` events / `50460`ms, matches EI's `squadAppliedCrowdControl(Duration)` |
| Stun breaks (count / duration) | Exact | `20` / `16907`ms, matches EI's `squadStunBreak`/`squadRemovedStunDuration` |
| Professions (incl. elite specs) | Exact | EI-style naming (elite-spec name wins when active), joined by account across 37+ players. Post-SotO spec ids 73/75/80/81 (Troubadour/Amalgam/Evoker/Luminary) and 74 (Paragon) are named by elimination — not sourced from GW2's public API — by matching this project's unmapped elite-spec ids against EI's own exported `profession` string for the same account/profession on a calibration log; id 74 in particular was grounded against a forced 48-player bijection between this project's and EI's exports of the same post-rework capture (see `crates/axilog-core/src/model/mod.rs`). Because this mapping is applied unconditionally, it changes always-on output for any post-era log containing that spec, not just the calibration fixture |
| Map name | Exact | resolved from the log's `MAP_ID` event |
| Team colors / team IDs | Exact | prefers the log's own `CBTS_WVWTEAMS` event when present (recent arcdps builds); falls back to a static id→color table (sourced from axibridge, itself reconciled from two community EVTC tools) for older logs without it |
| Friendly player count | Approximate | within ±2 of EI's count (one known relog straggler with a blank account, contributes 0 to every metric) |
| Down contribution | Emitted (arcdps methodology) | the M11 contribution family (`downs_contribution`/`downed_by`): health-anchored window per the dev-relayed arcdps methodology — max(last-≥99%-health − 2000ms, log start, prev-down + 2100ms reset) — with four stats (damage/CC/strips/movement-impairing), both directions; replaces the retired M1-era 10s-window approximation (schema 0.1 → 0.2). EI has no equivalent surface — this follows arcdps itself, not EI |
| CC detection (`is_cc` predicate) | Approximate, era-gated | exact vs. EI on a pre-`ResultEnumRework` arcdps build (< 20260501; `34`/`50460`ms, see the CC row above); post-era (≥ 20260501) now also accepts genuine `buff == 1` CC rows, era-gated off `RawHeader::is_post_buff_rework` (M4 Task 2, verified against GW2EI's post-`ResultEnumRework` source — no real post-rework capture to calibrate the post-era branch's numbers against yet), see "Supported log eras" below |
| Per-second timeline (squad damage / CC applied / downs) | Native-only | EI's JSON doesn't expose a comparable per-second series; ours does (`timeline.per_second`) |
| Down-contribution timeline (per-window breakdown) | Native-only, not yet exposed | the down-contribution algorithm already works in time windows internally; a windowed *timeline* (vs. today's single per-player total) is a planned native-only extension |
| Squad markers (incl. commander-tag colour/variant), tick-rate telemetry | Native-only, implemented | `CBTS_MARKER`/`CBTS_TICK` decode: per-player/enemy `marker`, commander player `commander_tag { variant, guid }`, `encounter.markers[]` assignment timeline, `encounter.tick_rate { avg, min, per_second[] }`; EI's JSON can't express any of this, so the EI adapter is unaffected — see `docs/arcdps-dev-notes.md` |
| Boon uptime — duration-type boons (Fury, Regeneration, Vigor, Swiftness, Protection, Aegis, Resolution, Quickness, Resistance, Alacrity) | Exact, era-gated | `presence_pct` (our field) == EI's `buffUptimes[].buffData[0].uptime`; 0/370 cells (10 boons × 37 joined players) over the 2pp tolerance — calibrated only against a pre-"BuffAppliesAndRemovesAsStateChanges" build (< 20260501); post-era (≥ 20260501) extraction is implemented and era-gated (M4 Tasks 1-2) but not yet calibrated against a real capture, see "Supported log eras" below |
| Boon uptime — intensity-type boons (Might, Stability) presence % | Exact, era-gated | 0/74 cells over 2pp on the pre-rework fixture (< 20260501); post-era (≥ 20260501) extraction implemented and era-gated, calibration pending a real capture, see "Supported log eras" below |
| Boon uptime — intensity-type boons (Might, Stability) average stacks | Approximate, era-gated | 67/74 cells exact-tolerance (≤5% relative) on the pre-rework fixture; 7 cells (all Stability) allowlisted — GW2EI types Stability `BuffStackType.StackingConditionalLoss` (loses a stack instead of being CC'd) vs. Might's plain `Stacking`, but GW2EI's own current simulator source has no `StackingConditionalLoss`-specific branching either; the affected players show legitimate multi-stack Stability grants with zero `CROWD_CONTROL` events, so the divergence is a genuine GW2EI-internal nuance not reverse-engineerable from the raw EVTC stream with confidence — allowlisted rather than guessed, see `INTENSITY_STACK_ALLOWLIST` in `crates/axilog-core/tests/boons_golden.rs`; post-era (≥ 20260501) extraction implemented and era-gated, calibration pending a real capture, see "Supported log eras" below |
| Boon generation (self/group/squad attribution) — squad-average % for Might/Quickness/Alacrity/Stability | Exact, era-gated | 148 cells (4 boons × 37 players) checked on the pre-rework fixture, worst delta 0.097pp, 0 over the 2pp tolerance, no allowlist needed; post-era (≥ 20260501) extraction implemented and era-gated, calibration pending a real capture, see "Supported log eras" below |
| Support: condi cleanses (squad total / self total) | Exact, era-gated | `801` / `97` on the pre-rework fixture, matches EI's `condiCleanse`/`condiCleanseSelf` sums, per-player exact (no allowlist); post-era (≥ 20260501) extraction implemented and era-gated (M4 Task 2's `apply_post_era`), calibration pending a real capture, see "Supported log eras" below |
| Support: boon strips (squad total) | Exact, era-gated | `437` on the pre-rework fixture, matches EI's `boonStrips` sum, per-player exact; post-era (≥ 20260501) extraction implemented and era-gated, calibration pending a real capture, see "Supported log eras" below |
| Support: resurrect casts (squad total) | Exact, era-gated | `6` on the pre-rework fixture, matches EI's `resurrects` sum, per-player exact; pre-era reads plain `is_activation` cast-start events, post-era (≥ 20260501) reads the dedicated `ANIMATION_START` statechange instead (a *different*, earlier threshold than the buff rework — GW2EI's `AnimationAsStateChanges = 20260430` — see "Supported log eras" below for the resulting narrow gap) |
| `buffMap` / `buffUptimes[]` / `support[0]` condi-cleanse/boon-strip/resurrect fields (`ei-json`) | Implemented | subset covering only the 12 tracked boons and the fields we actually compute — see **EI-JSON parity** note below |
| `targets[].isFake` (`ei-json`) | Exact | always `false` — every one of this project's `all_enemies` is a real, individually tracked agent, never one of real EI's synthetic aggregate pseudo-targets |
| `players[].combatReplayData.{start,end,down,dead}` (`ei-json`) | Exact (down/dead), Approximate (start/end) | `down`/`dead` byte-exact vs. the golden (`crates/axilog-ei/tests/ei_golden.rs`); `start`/`end` (first/last event of any kind for that agent) match GW2EI's `FirstAware`/`LastAware` on every joined player in the golden fixture. Computed **unconditionally** (not gated on `--replay`) — cheap, unlike the position track. Enabling `--replay` is purely ADDITIVE to these four fields (byte-identical either way, asserted by `ei_json_replay_fields_do_not_disturb_the_always_on_surface`) |
| `players[].combatReplayData.{positions,orientations,dc,iconURL}` + `targets[].combatReplayData` (`ei-json`) | Emitted, opt-in — **exact** | GW2EI's own fixed-rate replay shape (`axilog_core::analysis::ei_replay`, M15): map-PIXEL coordinates on GW2EI's 300ms polling grid, degrees for `orientations`, `long.MinValue`/`long.MaxValue`-bracketed `dc`, base-resolution profession `iconURL`. Present only when `--replay`/SDK `replay: true` was requested (measured +184% pretty-printed / +142% compact on the committed fixture). Calibrated **text-exact** (not just value-exact — EI serializes C# `float`s, so the emitted decimals must match too, see `axilog_ei`'s `ei_float`) against the golden fixture for all 37 joined accounts / 6,074 samples, and against the local post-rework GW2EI export for 44/44 players / 50,999 samples. One gap: an enemy-player target's `iconURL` is EI's own unknown-spec fallback, because this project resolves no profession for enemies (`model::Enemy` has no spec field) |
| `combatReplayMetaData` (`ei-json`) | Emitted, opt-in — **exact** | top-level `{inchToPixel, pollingRate, sizes, maps:[{url, interval, position}]}` (M15 Task 2's `ei_replay::combat_replay_meta`, transcribed from `JsonCombatReplayMetaDataBuilder`); the arena image every `positions` pair is a pixel coordinate on. Present only with `--replay`/SDK `replay: true` **and** a map GW2EI ships an image for (the five WvW maps in `wvw::maps::WVW_MAPS`); omitted — while `combatReplayData` is still emitted, from the computed bounding box, exactly as GW2EI does — for Obsidian Sanctum/Armistice Bastion/any other id. Text-exact against both references, including `inchToPixel: 0.009` (see `axilog_ei`'s `ei_float` for why the decimal text, not just the value, is gated) |
| `wvWMapData` (`ei-json`) | Emitted, partial — **documented gap** | only the three `{red,blue,green}TeamID` fields (from the log's own `CBTS_WVWTEAMS`/`TEAM_CHANGE` events). Real EI additionally carries `{red,blue,green}ShardID` and `objectiveData[]` (per-objective `{mapID, objectiveID, objectiveType, owners:[[teamID, ms], ...]}` capture-ownership timelines for every Camp/Tower/Keep on the map). Objective capture tracking is a whole separate event family this project does not decode yet — **parked**, not faked |
| `players[].activeTimes` (`ei-json`) | Exact | `SingleActor.GetActiveDuration(log, 0, durationMS)` reproduced as `(last_aware − first_aware) − dead_ms` (down time is NOT subtracted, verified against GW2EI source); 0.0000% max error across all 37 joined players on the golden fixture (gate: ≤0.5%) |
| `players[].totalDamageDist[][]` / `targetDamageDist[][][]` / `totalDamageTaken[][]` (`ei-json`) | Emitted, opt-in | `[phase][skillEntry]` / `[targetIndex][phase][skillEntry]` array shapes, verified against a real dps.report export; each entry carries only the fields this project computes (`id`, `totalDamage`, `min`, `max`, `hits`, `crit`, `flank` — real EI's `connectedHits`/`glance`/`missed`/`invulned`/`blocked`/`downContribution`/`indirectDamage`/etc. aren't tracked anywhere in this schema, omitted rather than faked). Present only when the `Report` was built with `--skill-damage`/SDK `skill_damage: true` (M12 Task 1's opt-in gate — measured +249% JSON size when always-on); omitted entirely (not emitted empty) otherwise. Every shared skill id matches the golden fixture's `skillDamage.outgoing` **exactly** (37/37 joined accounts, `crates/axilog-ei/tests/ei_golden.rs`) |
| `players[].damage1S[][]` / `targetDamage1S[][][]` / `damageTaken1S[][]` (`ei-json`) | Emitted, opt-in | `[phase][second]` / `[targetIndex][phase][second]` cumulative running-total arrays (this project's own per-second series are already cumulative by construction, matching GW2EI's own `*1S` semantics — see `axilog_core::analysis::timeseries`'s module doc). Present only when the `Report` was built with `--timeseries`/SDK `timeseries: true` (M12 Task 2's opt-in gate — measured +147.7% JSON size when always-on); omitted entirely otherwise. `damage1S[0].last()` matches the golden fixture's whole-fight damage scalar **exactly** (37/37 joined accounts) |
| `players[].dpsTargets[][]` (`ei-json`) | Emitted, opt-in | `[targetIndex][phase]{dps, damage}`, positionally keyed to `targets[]`; only the two fields this project computes (real EI's `condiDps`/`powerDps`/`breakbarDamage`/`actor*` duplicates aren't tracked, omitted). Gated by the SAME `--timeseries` flag as `damage1S` above (not a separate one — `axilog_schema::build_report` populates both off one bool); a real WvW log's large enemy roster makes `dpsTargets` alone exceed the size-discipline guideline (+36.4%), so it's opt-in too, not always-on as originally considered |
| `players[].statsAll[0]` hit-quality fields (`criticalRate`/`criticalDmg`/`flankingRate`/`glanceRate`/`againstMovingRate`/`connectedDamageCount`/`connectedDmg`/`connectedDirectDamageCount`/`connectedDirectDmg`/`connectedConditionCount`/`connectedConditionDamage`/`critableDirectDamageCount`/`againstDownedCount`/`againstDownedDamage`/`connectedLifeLeechCount`/`connectedLifeLeechDamage`/`connectedPowerAbove90HPCount`/`connectedPowerAbove90HPDamage`/`connectedConditionAbove90HPCount`/`connectedConditionAbove90HPDamage`) (`ei-json`) | Emitted, always-on | mapped from the native `hit_stats` block (M13 Task 1); EI field names exact, actor-only scope (no pet-credit fold — matches real EI's own `statsAll[0]`). Pre-era (< 20260501): every field matches the golden fixture **exactly** (37/37 joined accounts, `crates/axilog-ei/tests/ei_golden.rs`); post-era (≥ 20260501): every field, including the condition/power/life-leech buff==1 split, is now **exact** too — MCONDCAT (`axilog_core::analysis::condition_catalog`) closed the previously-documented condition-skill-id catalog gap (44/44 joined accounts on a real post-era capture, `hit_stats_golden.rs`'s `CATALOG_EXACT_FIELDS`) |
| `players[].defenses[0]` hit-outcome + damage-taken-breakdown fields (`blockedCount`/`evadedCount`/`dodgeCount`/`missedCount`/`interruptedCount`/`invulnedCount`/`strikeDamageTaken(Count)`/`powerDamageTaken(Count)`/`conditionDamageTaken(Count)`/`lifeLeechDamageTaken(Count)`/`damageBarrier(Count)`/`breakbarDamageTaken(Count)`) (`ei-json`) | Emitted, always-on | mapped from the native `defenses` block (M13 Task 2). Pre-era (< 20260501): every field matches the golden fixture **exactly** except `lifeLeechDamageTakenCount` (37/37 joined accounts, `crates/axilog-ei/tests/ei_golden.rs`) — this project deliberately emits the TRUE derived life-leech count rather than reproducing a real, verified GW2EI bug; post-era (≥ 20260501): the condition/power/life-leech damage-taken split is now **exact** too — MCONDCAT (`axilog_core::analysis::condition_catalog`) closed the previously-documented condition-skill-id catalog gap (44/44 joined accounts on a real post-era capture, up to 51.4% relative divergence on `powerDamageTakenCount` before the fix — see `defenses_golden.rs`'s `CATALOG_EXACT_FIELDS`); hit-outcome counts (blocked/evaded/dodge/miss/interrupt/invuln) remain exact both eras, as before. See `axilog_core::analysis::defenses`'s module doc for the full citation |
| `players[].rotation[]` (`ei-json`) | Emitted, opt-in | flat array of `{id, skills:[{castTime,duration,timeGained,quickness}]}` — NOT phase-wrapped (unlike `statsAll`/`totalDamageDist`/etc above, real EI's own `rotation[]` has no phase dimension). Straight copy of the native `rotation` block (M14 Task 1's `AnimatedCastEvent`-pipeline reproduction — see `axilog_core::analysis::rotation`'s module doc for the documented `InstantCastEvent`-pipeline scope gap, ~29% of a real log's cast entries). Present only when the `Report` was built with `--rotation`/SDK `rotation: true` (keyed off that presence, not a separate flag — same convention `skill_damage`/`per_second` above establish); omitted entirely (not emitted empty) otherwise. Per-player total cast COUNT matches the golden fixture's own `rotation[]` **exactly** (37/37 joined accounts, `crates/axilog-ei/tests/ei_golden.rs`) |
| `players[].damageModifiers[]` / `incomingDamageModifiers[]` (`ei-json`) | Emitted, opt-in, partial coverage | `[{id, damageModifiers:[{hitCount, totalHitCount, damageGain, totalDamage}]}]`, the inner array being EI's per-phase dimension (one element -- this project doesn't model phases). Present only when the `Report` was built with `--modifiers`/SDK `modifiers: true`; omitted entirely otherwise. Backed by a 205-definition transcription of GW2EI's own `DamageModifierDescriptor` tables (M16 -- `axilog_core::analysis::damage_mods`, every entry carrying the `file:line` it came from). **Coverage on the WvW reference capture: 69 of the export's 75 ids**; the 6 uncovered ones each need an engine feature this project doesn't have (absorbed-hit classification, a condition-buff-count graph, EI-synthetic weaver attunement ids, a source-HP probe, minion-species/illusion predicates) and are listed with reasons in that module's skipped table -- omitted, never approximated. **Accuracy:** 30 ids are exact on every field of every account and their emitted JSON is asserted TEXT-identical to the reference export (207/207 rows, `crates/axilog-ei/tests/damage_mods_ei_golden.rs`); the remaining 39 carry a bounded residual that is **not** a damage-modifier defect but the accuracy of the underlying per-`(actor, buff)` stack timelines (M3's simulator, already documented as approximate in `boons_golden.rs`) -- each id has its own measured per-field bound in `damage_mods_golden.rs`'s `ID_BOUNDS`, so the residual is visible and cannot grow silently. `damageGain` is emitted as a `double` rounded to 3 decimals, matching GW2EI's `Math.Round(_, ParserHelper.DamageModGainDigit)` over a `double` -- deliberately NOT through this crate's `f32`-narrowing `ei_float` |
| `players[].damageModifiersTarget[][]` / `incomingDamageModifiersTarget[][]` (`ei-json`) | Emitted, opt-in, index space differs | `[targetIndex][]` of the same entry shape, one slot per `targets[]` entry, `[]` where the player exchanged no qualifying hit with that target. Verified non-empty in WvW rather than assumed (the reference export's first player populates 22 of 57 outgoing and 14 of 57 incoming slots). Gated by the same `--modifiers` flag, and they dominate its cost: 854,077 of the 954,397 bytes it adds on the committed fixture. **Known divergence, inherited not introduced:** the index space is this project's own `targets[]` -- the full unfiltered enemy roster (624 entries on the reference capture) -- while GW2EI's WvW logic exposes 57, so these arrays are NOT positionally interchangeable with a real EI export's. That is the same pre-existing `targets[]` scope difference `statsTargets`/`dpsTargets`/`targetDamage1S` already carry (see `Report::all_enemies`). Calibrated by joining on arcdps agent identity instead: 1,978/1,978 per-target rows across 43 joined enemy-player targets are TEXT-identical to the reference export |
| `damageModMap` (`ei-json`) | Emitted, opt-in | top-level, keyed `"d<signed id>"` per real EI's convention (negative = incoming), scoped to only the ids some player actually triggered -- GW2EI fills its own map the same lazy way. All eight of EI's `DamageModDesc` fields are emitted (`name`, `icon`, `description`, `nonMultiplier`, `isCounter`, `skillBased`, `approximate`, `incoming`), none faked: `description` is GW2EI's full composed tooltip, including the derived `<br>Applied on ...`/`<br>Compared against ...`/`<br>Counter`/`<br>Non multiplier`/`<br>Approximate` suffixes. **All 69 emitted entries are character-for-character identical to the reference export's**, every field (`crates/axilog-ei/tests/damage_mods_ei_golden.rs`). Omitted entirely (not emitted empty) without `--modifiers` -- an empty map would claim no player triggered anything, rather than that the engine never ran |
| `personalDamageMods` (`ei-json`) | Not emitted | real EI's top-level `spec -> [modifier ids]` index (`JsonDamageModifierDataBuilder.cs:52-66`). It is a pure re-index of data `damageModMap` + the per-player arrays already carry, keyed by GW2EI's own `Spec` enum spelling, which this project does not reproduce -- omitted rather than faked with a near-miss spec name |
| `skillMap` (`ei-json`) | Emitted, always-on, partial | top-level, keyed `"s<id>"` per real EI's convention, scoped to only the skill ids squad players' `skill_damage`/`rotation`/tracked-boons actually reference (M14 Task 2 — not a dump of the log's whole ~1,000-entry skill table). Only the fields this project computes are emitted: `name` (this log's own skill-table text, best-effort — falls back to `"Skill <id>"`; a genuinely different, narrower data source than EI's bundled/API-backed skill DB, so name strings are NOT calibrated against EI, only spot-checked, see `axilog_core::analysis::skill_map`'s module doc), `isSwap` (the `WeaponSwap` sentinel plus 3 curated non-sentinel categories — elementalist attunement swaps, revenant legend swaps, necromancer shroud transforms — still narrower than EI's own broader check, which also covers Weaver's separate combo-attunement table), `canCrit` (reused verbatim from M13's already-calibrated `NonCritableSkills` table, matches EI exactly on every overlapping id). Real EI's `icon` (a render.guildwars2.com URL) and `autoAttack` (needs the external, live GW2 API — this project's `auto_attack` is always omitted, not guessed) are NOT computed here, so they're omitted rather than faked, same for every proc/instant/accuracy classifier flag (`isInstantCast`/`isTraitProc`/etc) |

The `ei-json` output only emits fields backed by a real computed metric. Where real EI has a field
we don't compute (e.g. per-target down-contribution/CC splits, most of `statsAll`'s damage-modifier
detail, skill icons/DB names), it's simply omitted — never faked — and the omission is documented
inline in `crates/axilog-ei/src/lib.rs`. As of M14 (rotation/skillMap), the remaining
axibridge-flagged Tier-1 analysis gaps are closed: the per-skill/per-second/dpsTargets family (M12),
the hit-quality/defenses fine-grained outcome counts (M13), and rotation/skillMap (M14) that used to
be entirely absent from `ei-json` are now all emitted — as are replay positions in EI's own
coordinate grid (M15) and damage-modifier attribution (M16, opt-in `--modifiers`).

### Supported log eras

arcdps has two wire shapes for boon apply/remove/initial rows, split by GW2EI's
`ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges`/`ResultEnumRework` threshold (build string
`20260501`): **pre**-threshold builds report them as ordinary `is_statechange == 0` combat events;
**post**-threshold builds report dedicated statechange event kinds instead (`BUFF_APPLY`/
`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`), and also move CC detection (`buff == 1` rows
can carry real CC) and resurrect-cast detection (via the separate, one-day-earlier
`AnimationAsStateChanges = 20260430` threshold, `ANIMATION_START`) onto their own dedicated
statechanges.

- **Pre-`20260501` (fully calibrated):** every metric in the parity table above is calibrated
  exact-to-near-exact against a real dps.report EI export for a pre-rework WvW log — this is what
  every "Exact"/"Approximate" status above without further qualification means.
- **`≥ 20260501` (supported, calibration pending):** this project's buff/support/CC extraction is
  **era-gated** (`RawHeader::is_post_buff_rework`) and decodes the post-rework wire shape too
  (M4 Tasks 1-2) — boon uptimes, boon generation, condi cleanses, boon strips, resurrects, and CC
  detection all work on a post-rework log, not just pre-rework ones. This was verified by
  construction: every post-era code path was checked line-by-line against the current GW2EI
  parser source (`GW2EIEvtcParser`, the arbiter for ambiguous field roles) and exercised by
  synthetic **era-equivalence tests** (`analysis/buffs/events.rs`, `analysis/support.rs`,
  `analysis/cc.rs` — each has a post-era twin of its pre-era test producing the identical output).
  What's still missing is calibration against a *real* post-rework capture, since none existed at
  implementation time — `crates/axilog-core/tests/postrework_golden.rs` is the hook that closes
  this gap the moment one is available (see below).
- **Known narrow gap, `[20260430, 20260501)`:** this project has only the single
  `is_post_buff_rework` (`20260501`) header flag, not a separate `AnimationAsStateChanges`
  (`20260430`) one. A log built in that one-day-to-one-month window (`AnimationAsStateChanges` has
  landed but `BuffAppliesAndRemovesAsStateChanges`/`ResultEnumRework` hasn't yet) would be
  classified pre-era and scanned with the pre-era shape throughout — including resurrect
  detection, which is actually gated on the *earlier* threshold. Flagged honestly rather than
  silently mishandled; considered out of scope to fix without adding a header field this project
  otherwise has no use for (see `sc::ANIMATION_START`'s doc comment and the M4 Task 2 report for
  the full analysis).

To make the "genuinely zero buff data" case visible rather than silent, `analyze` detects it
(post-rework build, per `RawHeader::is_post_buff_rework`, **and** zero buff events actually
extracted — e.g. a truncated/filtered log) and records a warning in `Metrics::warnings`. This does
**not** fire just because a log is post-era; it fires only when post-era extraction genuinely finds
nothing to extract. The native JSON schema surfaces this as a top-level `warnings: [...]` array
(omitted when empty); the CLI's `--format table` prints each warning to stderr; `ei-json` has no
comparable field and doesn't carry it.

#### How to provide a post-rework fixture

The one thing this project can't do without a real log: **calibrate** the post-era code paths
against real dps.report numbers, the same way the pre-era paths are calibrated in the parity table
above. To help with that:

1. Capture a WvW fight with a current (post-`20260501`) arcdps build.
2. Drop the raw `.zevtc` at `fixtures/local/wvw-postrework.zevtc` (gitignored — see "Fixture
   policy" below; never commit a raw log).
3. Optionally, run it through dps.report's `getJson` endpoint and drop that JSON alongside at
   `fixtures/local/wvw-postrework.ei.json` for full EI-parity assertions (duration/damage within
   0.5%, support sums exact).
4. Run `cargo test -p axilog-core --test postrework_golden` — no code changes needed. The tests
   pick the fixture(s) up automatically, assert the post-era metrics are non-zero and warning-free
   (and, if the EI JSON is present, that they match it), and print a compact summary table
   (players, duration, squad damage, Might average stacks, Quickness %, cleanses, strips,
   resurrects) so the first real capture's numbers are immediately visible.

## Performance

Measured end to end (decode → resolve → analyze → build the native report), release build, on an
AMD Ryzen 9 7900X3D:

| Log | Events | Full parse | of which `analyze` |
|---|---|---|---|
| Committed fixture (`fixtures/wvw-small.anon.zevtc`) | 120,435 | **28.9 ms** | 18.9 ms |
| Real WvW zerg log (48 players, 5:48 fight) | 583,194 | **174 ms** | 93.7 ms |

That is a whole real 583k-event WvW log parsed and fully analyzed — damage, downs/CC,
arcdps-methodology down contribution, boons + generation, support, healing, per-skill damage,
per-second series, hit quality, defenses, rotation — in under a fifth of a second, single-threaded,
with no `unsafe`.

The MPERF milestone made `analysis::analyze` 2.1× faster on the fixture and 2.6× faster on the real
log (1.75× / 1.87× end to end), with every step verified byte-identical against the previous
output. The benchmark harness (`crates/axilog-cli/benches/pipeline.rs`, criterion), the full
baseline → after-Task-2 → after-Task-3 tables, and every optimization that was *declined* along
with why, are in [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md). Reproduce the committed-fixture arm
with:

```bash
cargo bench -p axilog-cli --bench pipeline
```

## Fixture policy

- Committed fixtures (`fixtures/wvw-small.anon.zevtc`, `fixtures/wvw-small.ei.json`) are
  anonymized/PII-safe — verified by both an automated PII scan (`anonymize_fixture.rs`) and a
  manual independent scan before commit. CI runs the full golden-parity suite against them.
- **Never commit a raw `.zevtc`/`.evtc`.** Real logs contain real GW2 account names.
- `fixtures/local/` is gitignored and meant for real, non-anonymized logs used only for local
  development/calibration (tests that need one skip gracefully — printing a `skip: ... absent`
  message — when it's not present, e.g. in CI).

## arcdps-dev guidance

Implementation guidance relayed directly from the arcdps developer (event ids, payload layouts,
upcoming features to build against) is tracked as a running log in
[`docs/arcdps-dev-notes.md`](docs/arcdps-dev-notes.md), with a status per item and a pointer to the
milestone task that implements it.

## Milestones

**M1 (done):** EVTC/zevtc decode, agent/skill resolution, WvW team/friend-foe resolution, damage +
DPS, downs/kills/deaths/down-contribution, CC + per-second timeline, native JSON schema, CLI
(`parse` with `json`/`table`/`csv`/`ei-json`), EI-compat adapter, golden parity test + CI.

**M2 (done):** real elite-spec profession naming, real team IDs from the log itself (`CBTS_WVWTEAMS`)
with a static fallback table, `CBTS_IDTOGUID` content-GUID decoding (teams now, skill/species
retained for M3), CC/stun-break metrics from real `CROWD_CONTROL`/`CBTS_STUNBREAK` events, enemy
relog dedupe, time-aware pet-damage/CC attribution across instid reuse, `axilog anonymize` +
PII-safe committed golden fixture (CI now runs real parity checks, not skip-and-pass), EI adapter
`statsAll` CC fields, squad markers (`CBTS_MARKER`) + commander-tag colour/variant + tick-rate
telemetry (`CBTS_TICK`) — native-schema-only, this README.

**M3 (done):** the 12 tracked boons' stack-count timelines, uptime/presence/average-stacks, and
self/group/squad generation attribution (calibrated exact-to-near-exact vs. EI, see the parity
table above); condi-cleanse/boon-strip/resurrect support stats (calibrated exact vs. EI, no
allowlist); exposed in the native schema (`players[].boons[]`, `players[].support`), the EI adapter
(`buffMap`, `buffUptimes[]`, extended `support[0]`), and two new CLI table views (`--view
support`/`--view boons`).

**M4 (done):** post-`20260501` (buff-statechange-rework) log support — era-gated boon/support/CC
extraction (dedicated `BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`
statechanges, `ANIMATION_START`-gated resurrect detection, `buff == 1` CC rows), verified by
construction against GW2EI source + synthetic era-equivalence tests (no real post-rework capture
existed yet); downgraded the M3-era unconditional post-rework warning to fire only on genuinely
zero extracted buff events; added `tests/postrework_golden.rs`, a real-capture calibration hook
that activates automatically the moment a `fixtures/local/wvw-postrework.zevtc` fixture exists —
see "Supported log eras" above.

**M5 (done):** Node SDK (`crates/axilog-node`, `@axiapps/axilog`) — napi-rs native addon exporting
`parseFile`/`parseBuffer`/`parseFileEi`/`anonymizeFile` over the same decode → resolve → analyze →
build_report pipeline the CLI drives (no reimplementation, no JSON-over-subprocess); hand-maintained
TypeScript types (`types.d.ts`) for the native schema, patched into the generated `index.d.ts`; a
`node --test` suite covering all four exports plus a dual-path parity test against the CLI's own
`--format json` output; CI builds the addon on Linux/Windows/macOS and runs the node test suite on
Linux (see `.github/workflows/ci.yml`). npm publishing deferred — see SDKs above.

**M6 (done):** Python SDK (`crates/axilog-py`, package `axilog`) — PyO3 native extension module
(`abi3-py39`) exporting `parse_file`/`parse_bytes`/`parse_file_ei`/`anonymize_file` over the same
decode → resolve → analyze → build_report pipeline the CLI and Node SDK drive (no
reimplementation); hand-maintained typed stubs (`axilog.pyi` + `py.typed`) for the native schema,
auto-bundled into the wheel by maturin; a stdlib `unittest` suite covering all four exports plus a
CLI-parity test against the CLI's own `--format json` output; CI builds the extension on
Linux/Windows/macOS (`maturin build`) and runs `maturin develop` + the unittest suite on Linux (see
`.github/workflows/ci.yml`). PyPI publishing deferred — see SDKs above.

**M7 (done):** self-contained HTML report (`crates/axilog-html`, `axilog parse --format html`) —
dark-theme (light-mode toggle) single-file document, zero external requests, built from
`include_str!`-inlined `report.css`/`report.js` assets around the embedded `Report` JSON; header
(map/duration/recorder/commander/warnings/team chips); sortable Damage/Support/Boons tabs (keyboard-
accessible tab bar and column-sort buttons, `aria-sort` on the `<th>` per WAI-ARIA, squad-totals
footer row, muted non-squad rows, boon generation-mode self/group/squad toggle); an inline responsive
SVG damage timeline (squad-damage area/line, downs-on-the-line markers, normalized CC-applied bar
overlay, mm:ss/k-format axes) built by pure, node-tested path-generation functions
(`buildTimelinePaths`); an XSS contract (log-derived strings via `textContent` only, JSON escaped
`<`-safe for inline `<script>` embedding) with regression tests for both halves; a golden structural
test against the real committed fixture (calibrated squad-damage/support sums, all view/timeline
containers, byte-for-byte determinism, size budgets: <250KB total report, <50KB combined raw CSS+JS).
`-o/--output FILE` (any `--format`, not just html) added to the CLI alongside it. See **HTML
report** above.

**M8 (done):** tag-triggered release pipeline (`.github/workflows/release.yml`, `v*` tags) — CLI
binaries for all 5 targets, `@axiapps/axilog` npm main + platform packages (all 5), and `axilog`
Python wheels (abi3, 4 platforms) + sdist, all attached to one GitHub Release with a consolidated
`SHA256SUMS`; a version single-source guard (`scripts/check-versions.sh`, wired into `ci.yml`)
keeps `Cargo.toml`/`package.json`/npm platform packages/`pyproject.toml` from drifting apart, plus
a tag==Cargo-version guard (`scripts/check-tag-version.sh`) before every release; `npm publish`/
`twine upload` are wired in but gated on `NPM_TOKEN`/`PYPI_TOKEN` repository secrets being
configured (log-skip otherwise — the Release itself, with every artifact attached, is created
either way) and on the triggering event being a real tag push, never a `workflow_dispatch` dry
run. See **Install** above and `RELEASING.md` for the full flow.

**M9 (done):** animated combat replay — `axilog_core::analysis::replay::build_replay` decodes
`CBTS_POSITION`/`CBTS_VELOCITY`/`CBTS_FACING` packed-float payloads (ordinals/layout verified
against the arcdps README and GW2EI's `MovementEvent` source) into per-squad-player and
per-enemy-player-representative position tracks, downsampled to a 300ms grid (matching GW2EI's own
combat-replay polling) with linear interpolation between bracketing samples, plus down/dead
intervals from existing event analysis — calibrated to ≥95% of samples within 1.0 map-pixel of
GW2EI's exported `combatReplayData` on both golden fixtures (in practice 99.77–100%, see
`crates/axilog-core/tests/replay_golden.rs`); an opt-in `replay: Option<ReplayOut>` schema block
(`axilog-schema`) wired through `axilog parse --replay` (json/html) and both SDKs
(`replay`/`replay=False` params, additive/back-compat); the HTML report's animated **Replay tab**
(see **HTML report** above) — SVG stage, play/pause/scrub/speed controls, pure node-tested
interpolation (`positionsAt`/`replayViewBox`/`isDownAt`/`isDeadAt`), rendered only when `--replay`
data is present. Size gates: replay-enabled reports <600KB, combined raw CSS+JS <60KB (raised from
M7's 50KB — controller-authorized, the animated stage/controls needed the extra headroom).

**M10 (done):** arcdps healing-extension stats (`players[].healing`: `healing_out_total`/
`healing_out_allies`/`healing_out_self`/`barrier_out`/`downed_healing_out`), calibrated exact
(`healing_out_self`/`downed_healing_out`, 41/41 accounts) to near-exact (`healing_out_total`/
`healing_out_allies` within 0.68%/0.71% squad-wide) against EI — except `barrier_out`, held to an
explicitly authorized wider 8.0% squad-wide tolerance (a single repeating-skill peer-report
cluster this project's byte-level replication of GW2EI's `SanitizeForSrc` rule can't perfectly
reconstruct without also replicating GW2EI's internal per-agent-lifetime identity tracking — see
`axilog_core::analysis::healing`'s module doc for the full trace); omitted entirely (not a
`null`/all-zero block) when the log carries no healing-extension data, a real "no data" signal
surfaced via a `Report.warnings` entry; new CLI `--view healing`. Opt-in missile (projectile)
analytics (`--missiles`, native-only): per-squad-player `fired`/`hit`/`denied`/
`reflected_at_self` plus a squad-wide `incoming_fired`/`incoming_denied` defensive rollup —
deliberately honest about its scope: the arcdps wire format carries no blocked/reflected/
destroyed reason code, so `denied` is one undifferentiated bucket, and there is no per-player
"who denied this" credit anywhere (only the aggregate `squad.incoming_denied`); `missiles.
players[]` entries carry `account` so they join back to `players[]` without needing `--replay`
too. A combat-participant enemy filter (`Report.enemies`, and the HTML team chips that read it)
now excludes NPC/gadget agents the squad never interacted with (no damage/CC either direction) —
a real WvW log enumerates every nearby lootable/tactivator/chest as an "enemy", most of which are
never actually part of the fight; `ei-json`'s `targets[]`/`statsTargets[]` are unaffected (kept
against the full, unfiltered roster, matching real EI's own behavior). Team ids
(`TeamOut.team_id` and the model/analysis layers feeding it) widened `u16` → `u32`, removing a
truncating cast on dynamic `CBTS_WVWTEAMS` ids (future-proofing; no real fixture currently has an
id large enough for the truncation to have mattered).

**Later:** PvE encounter logic (boss health phases, mechanics), damage-modifier attribution
(M16), HTML report extras (tick-rate corner widget, mounts/glider/capping replay eye candy, a
healing tab — see arcdps-dev-notes). Registry publishing is LIVE (npm `@axiapps/axilog`, PyPI
`axilog` — automated on tag push via NPM_TOKEN + PyPI trusted publishing), and the post-rework
era is fully real-capture calibrated.

## License

MIT — see [LICENSE](LICENSE).
