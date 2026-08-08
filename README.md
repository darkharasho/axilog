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
| CC detection (`is_cc` predicate) | Approximate, build-dependent | tuned against a pre-"ResultEnumRework" arcdps build (< 20260501); a post-rework capture may need the predicate extended (`buff == 1` events can carry real CC post-rework) — see `TODO(post-rework)` in `crates/axilog-core/src/analysis/cc.rs` |
| Per-second timeline (squad damage / CC applied / downs) | Native-only | EI's JSON doesn't expose a comparable per-second series; ours does (`timeline.per_second`) |
| Down-contribution timeline (per-window breakdown) | Native-only, not yet exposed | the down-contribution algorithm already works in time windows internally; a windowed *timeline* (vs. today's single per-player total) is a planned native-only extension |
| Squad markers (incl. commander-tag colour/variant), tick-rate telemetry | Native-only, implemented | `CBTS_MARKER`/`CBTS_TICK` decode: per-player/enemy `marker`, commander player `commander_tag { variant, guid }`, `encounter.markers[]` assignment timeline, `encounter.tick_rate { avg, min, per_second[] }`; EI's JSON can't express any of this, so the EI adapter is unaffected — see `docs/arcdps-dev-notes.md` |
| Boon uptime — duration-type boons (Fury, Regeneration, Vigor, Swiftness, Protection, Aegis, Resolution, Quickness, Resistance, Alacrity) | Exact | `presence_pct` (our field) == EI's `buffUptimes[].buffData[0].uptime`; 0/370 cells (10 boons × 37 joined players) over the 2pp tolerance |
| Boon uptime — intensity-type boons (Might, Stability) presence % | Exact | 0/74 cells over 2pp |
| Boon uptime — intensity-type boons (Might, Stability) average stacks | Approximate | 67/74 cells exact-tolerance (≤5% relative); 7 cells (all Stability) allowlisted — GW2EI types Stability `BuffStackType.StackingConditionalLoss` (loses a stack instead of being CC'd) vs. Might's plain `Stacking`, but GW2EI's own current simulator source has no `StackingConditionalLoss`-specific branching either; the affected players show legitimate multi-stack Stability grants with zero `CROWD_CONTROL` events, so the divergence is a genuine GW2EI-internal nuance not reverse-engineerable from the raw EVTC stream with confidence — allowlisted rather than guessed, see `INTENSITY_STACK_ALLOWLIST` in `crates/axilog-core/tests/boons_golden.rs` |
| Boon generation (self/group/squad attribution) — squad-average % for Might/Quickness/Alacrity/Stability | Exact | 148 cells (4 boons × 37 players) checked, worst delta 0.097pp, 0 over the 2pp tolerance, no allowlist needed |
| Support: condi cleanses (squad total / self total) | Exact | `801` / `97`, matches EI's `condiCleanse`/`condiCleanseSelf` sums, per-player exact (no allowlist) |
| Support: boon strips (squad total) | Exact | `437`, matches EI's `boonStrips` sum, per-player exact |
| Support: resurrect casts (squad total) | Exact | `6`, matches EI's `resurrects` sum, per-player exact |
| `buffMap` / `buffUptimes[]` / `support[0]` condi-cleanse/boon-strip/resurrect fields (`ei-json`) | Implemented | subset covering only the 12 tracked boons and the fields we actually compute — see **EI-JSON parity** note below |

The `ei-json` output only emits fields backed by a real computed metric. Where real EI has a field
we don't compute (e.g. per-target down-contribution/CC splits, most of `statsAll`'s damage-modifier
and rotation detail), it's simply omitted — never faked — and the omission is documented inline in
`crates/axilog-ei/src/lib.rs`.

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

**Later:** healing/barrier stats, rotation/skill-cast tracking, PvE encounter logic (boss health
phases, mechanics), Python/Node SDKs over the Rust core, HTML report output (incl. the tick-rate
corner widget and marker-driven combat-replay eye candy — see arcdps-dev-notes).

## License

MIT — see [LICENSE](LICENSE).
