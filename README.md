# axilog

axilog is a cross-platform, CLI-first reimplementation of Elite Insights for parsing GW2 arcdps
combat logs, part of the axi suite. It has a reusable Rust parsing core (`crates/axilog-core`)
with planned Python/Node SDKs on top, matches standard Elite Insights (EI) functionality for the
metrics it currently covers, and follows the arcdps spec more closely than EI in a few places —
notably down contribution, CC-over-time, and full per-second timeline support. Unlike the
original EI, it isn't tied to a single OS.

Current focus is WvW logs (M1/M2). PvE encounter logic, boons/support stats, healing, and
rotations are not implemented yet (see Milestones below).

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
axilog parse <log.zevtc|log.evtc> [--format json|table|csv|ei-json]
```

- `json` (default) — axilog's own native schema (`axilog_schema::Report`): encounter info, teams,
  per-player metrics (damage, downs/kills, down contribution, CC, stun breaks), and a per-second
  timeline. This is the richest, most accurate representation — the other formats are lossy views
  of it.
- `table` — human-readable summary, one row per player, sorted by damage.
- `csv` — same per-player fields as `table`, machine-readable.
- `ei-json` — a subset of the real Elite Insights / dps.report JSON shape, for tools that already
  consume EI's format. See **EI-JSON parity** below for exactly what is and isn't populated.

Example `table` output (anonymized account names, from the committed golden fixture):

```
account                  profession       damage      DPS  downs  kills  deaths
:Anon104.4848            Engineer         205612     4172      1      0       0
:Anon171.7327            Engineer         192437     3905      2      1       0
:Anon110.5070            Necromancer      169689     3443      4      0       0
:Anon116.5292            Necromancer      162652     3300      1      0       0
:Anon107.4959            Mesmer           154899     3143      0      0       0
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
| Squad markers, tick-rate telemetry | Planned, not yet implemented | see `docs/arcdps-dev-notes.md` |

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
`statsAll` CC fields, this README.

**M3 (next):** boons and support stats (uptimes, generation, cleanses/corrupts) — the biggest
remaining EI-parity gap.

**Later:** healing/barrier stats, rotation/skill-cast tracking, PvE encounter logic (boss health
phases, mechanics), Python/Node SDKs over the Rust core, HTML report output, squad markers and
tick-rate telemetry (see arcdps-dev-notes).

## License

MIT — see [LICENSE](LICENSE).
