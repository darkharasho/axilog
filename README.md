# axilog

axilog is a cross-platform, CLI-first reimplementation of Elite Insights for parsing GW2 arcdps
combat logs, part of the axi suite. It has a reusable Rust parsing core (`crates/axilog-core`)
with planned Python/Node SDKs on top, matches standard Elite Insights (EI) functionality for the
metrics it currently covers, and follows the arcdps spec more closely than EI in a few places —
notably down contribution, CC-over-time, and full per-second timeline support. Unlike the
original EI, it isn't tied to a single OS.

Current focus is WvW logs (M1/M2/M3). PvE encounter logic, healing, and rotation/skill-cast
tracking are not implemented yet (see Milestones below).

## Install / build

Requires a Rust toolchain (see `rust-toolchain.toml`).

```sh
cargo build --workspace --release
# binary at target/release/axilog
```

Or run directly via cargo during development:

```sh
cargo run -p axilog-cli -- parse <log.zevtc> --format table
```

## Usage

### Parse a log

```sh
axilog parse <log.zevtc|log.evtc> [--format json|table|csv|ei-json] [--view default|support|boons]
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

`--view` (M3) selects the `table` format's column layout — ignored for every other `--format`:

- `default` (default) — damage/DPS/downs/kills/deaths, sorted by damage.
- `support` — condi cleanses, boon strips, resurrects, stun breaks, sorted by cleanses.
- `boons` — Might average stacks plus presence % for Quickness/Alacrity/Stability/Protection,
  sorted by account.

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

### Anonymize a log for sharing

```sh
axilog anonymize <in.zevtc> <out.zevtc>
```

Rewrites every player agent's character/account name to a deterministic `Anon<N>` /
`:Anon<N>.<4 digits>` placeholder in place; every other byte (including every combat event, the
skill table, and NPC/gadget agents) is preserved exactly, so parsed metrics are identical before
and after. Use this to produce a PII-safe log before filing a bug report, sharing a log publicly,
or committing it as a test fixture — never commit a raw `.zevtc`/`.evtc` (see Fixture policy).

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
| Professions (incl. elite specs) | Exact | EI-style naming (elite-spec name wins when active), joined by account across 37+ players |
| Map name | Exact | resolved from the log's `MAP_ID` event |
| Team colors / team IDs | Exact | prefers the log's own `CBTS_WVWTEAMS` event when present (recent arcdps builds); falls back to a static id→color table (sourced from axibridge, itself reconciled from two community EVTC tools) for older logs without it |
| Friendly player count | Approximate | within ±2 of EI's count (one known relog straggler with a blank account, contributes 0 to every metric) |
| Down contribution | Approximate | our own algorithm: damage from squad → that enemy in the 10s window before its down event, excluding CC-only events; not calibrated against EI's own (undocumented) down-contribution algorithm, only unit-tested |
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

The `ei-json` output only emits fields backed by a real computed metric. Where real EI has a field
we don't compute (e.g. per-target down-contribution/CC splits, most of `statsAll`'s damage-modifier
and rotation detail), it's simply omitted — never faked — and the omission is documented inline in
`crates/axilog-ei/src/lib.rs`.

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

**Later:** healing/barrier stats, rotation/skill-cast tracking, PvE encounter logic (boss health
phases, mechanics), Python/Node SDKs over the Rust core, HTML report output (incl. the tick-rate
corner widget and marker-driven combat-replay eye candy — see arcdps-dev-notes), real-capture
calibration of the M4 post-rework code paths once a fixture is available.

## License

MIT — see [LICENSE](LICENSE).
