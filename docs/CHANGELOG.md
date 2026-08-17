# Changelog

Version-grouped from the milestone log that used to live in the README. Grouping is by git-tag
ancestry (`git log --oneline vA..vB`), so each milestone appears under the first release that
contains its merge commit. The forward-looking view — what is queued, parked and in flight — is
[`docs/ROADMAP.md`](ROADMAP.md).

Milestones are the unit of work in this project: spec → plan → subagent-driven execution in an
isolated worktree → adversarial review per task → whole-branch review → merge. Every entry below
kept the cross-cutting invariants green (existing calibration exact, no PII committed, deterministic
output, all suites passing).

## v0.3.4 — 2026-08-16

**MSDELAY — the instant-cast server delay constant.** `SERVER_DELAY` was 150 ms; arcdps' own
figure is 10 ms. The two casts the wider window had been inventing were both spurious, so the
per-skill absolute error against the rotation golden went 54 → 52 and skills 56873 and 78358 went
from one cast over the golden to exact. Instant-cast coverage reads 338/364 rather than 340/364
for the same reason: fewer casts, none of them real losses.

Also in this release: the Windows `LNK1201` link failure (the CLI binary and the Python cdylib were
both named `axilog`, so their PDBs collided), and the 0.3.3 lockfile refresh.

## v0.3.3 — 2026-08-16

The native output format program, end to end. **Container 1.0** — six top-level keys, an
`entities[]` roster with roles and deterministic ids, referenced-ids-only `catalogs`, gated
`blocks`, honest `coverage` states and a raw/RLE series envelope — emitted from `--format json` and
both SDKs, with the legacy report proven to reproject losslessly out of it. **Phase A** absorbed the
EI side channel into those blocks (`dist_outcomes`, `healing_detail`, `boon_states`,
`target_conditions`, enemy per-skill damage and outgoing series, minions, health percents), which
let `ei-json` be rendered from the native report alone and `EiInputs` be deleted; `--all` and the
consumer-parse-cost benchmark landed with it. **Phase B** closed the gaps only native can close: the
23-field per-target split (hit quality, missed/evaded/blocked/invulned, applied CC), `distToCom` and
`stackDist` computed engine-side, commander-tag segments, `dc` despawn intervals, and the log-start
anchor centralised across its 16 call sites. **Phase C** shipped two generated icon catalogs —
4,656 skills from the GW2 API (plus `autoAttack` from `slot == "Weapon_1"`) and 2,267 buffs from
GW2EI's own table.

**MOBJ — WvW objective ownership.** Objective ownership timelines and shard ids parsed off the
encounter and emitted in both formats.

**MPROC / MCAST — instant casts.** GW2EI's `InstantCastFinder` machinery ported, a
565-of-649 catalog machine-extracted from the C# sources, the five `skillMap` proc/instant flags
emitted, arcdps effect events decoded across all three generations to feed the effect-keyed
finders, and instant casts merged with weapon swaps into the rotation per `InitCastEvents`.
Calibration: animated casts exact (1,222/1,222), weapon swaps exact (134/134), instant casts
bounded at 92.9%.

**Replay extras.** Glider, transformation and gadget capture. Both SDK type stubs were re-synced
with the 1.0 container and are now guarded in CI against the key-set golden. 1,069 tests.

## v0.3.2 — 2026-08-10

Enemy `profession`/`elite_spec` carried through to the enemy roster and declared in the Node types;
three CI failures fixed; the README rewritten from 778 lines to 232, with the deep reference moved
into `docs/`.

## v0.3.1 — 2026-08-10

**MSMALL — the deferred accuracy tail.** `HealingLogic` for Regeneration (GW2EI's only
`HealingLogic` buff) plus boon-generation `wasted` in `selfBuffs`/`groupBuffs`/`squadBuffs`,
including waste-only sources — this closed the last half-open axibridge audit row. `statsAll[0].
saved`/`timeSaved`/`wasted`/`timeWasted`, exact on 44/44 accounts, which turned out not to need the
`InstantCastEvent` pipeline; the ties-to-even fix behind it also took per-cast `timeGained` to
exactly 0 delta across all 10,878 local-capture casts. Presence tolerance tightened 2.0pp → 0.05pp
with the export's 3-decimal floor named. Breakbar damage excluded from down contribution (the
participation carve-out kept). `LazySeq` given a real take-once `debug_assert`; the Node
`index.d.ts` made regen-clean and guarded in CI.

**MROSTER — curate the `ei-json` `targets[]` roster.** `targets[]` was every enemy agent the log
enumerated (624 on the real capture) against the 57 GW2EI's WvW logic exposes. Curated to GW2EI's
own rule — enemy players only (`WvWLogic.cs:325-375`) — leaving 71. A correctness fix and a large
free performance win, because nine per-player arrays are positionally joined to `targets[]`:
`ei-json` flagless −71.1%, `--timeseries` −86.2%, matched axibridge surface −82.6% and 2.60 s →
1.70 s on the real log. Native `--format json` and `--format html` byte-identical before and after.

**MINSTID — regroup enemy players by instid.** `wvw::dedupe_enemy_players` now keys on `InstID`,
GW2EI's own non-squad rule (`AgentManipulationHelper.cs:467-474`), instead of the account that WvW
anonymization leaves empty. Native `enemies[]` 140 → 125 rows (71 → 56 enemy players), `ei-json`
`targets[]` 71 → 56 over EI's exact 56 instids, and the damage-mitigation `min` mean-of-minima
residual 16/206 → 0/206. Every merged row was verified to be the sum/union of its parts; the
instid-joined calibrations widened from 43 targets to all 56, which exposed 3 pre-existing
damage-credit divergences (allowlisted and diagnosed in `meigap2_ei_golden`).

## v0.3.0 — 2026-08-10

**MEIGAP — `ei-json` adapter gap closure for axibridge.** The axibridge cutover audit found 30 of
118 read fields blank under `ei-json`. Closed the per-player generation-attribution arrays
(`selfBuffs`/`groupBuffs`/`squadBuffs`), `buffUptimes[].states`/`.statesPerSource`, incoming
CC/strips/`boonStripsTime`, the per-target offensive split in `statsTargets`, the enemy-side
`targets[]` mirrors, the healing/barrier detail, `minions[]` and `guildID`.

**MEIGAP2 — the six open-cheap audit rows.** Player-side distribution outcome columns,
`healthPercents`, `instanceID`, `boonsStates`/`boonsAppliedCount`, `targets[].dpsAll[0].damage` and
`dpsAll[0].breakbarDamage`.

**MSTREAM — streaming `ei-json` serialization.** Stream the document instead of materializing the
whole `serde_json::Value` tree: peak RSS −95% (20× lower), output verified byte-identical across 96
flag/output combinations. This flipped the one column Elite Insights still won.

## v0.2.0 — 2026-08-09

**M15 — combat-replay positions in EI's shape.** A second, independent fixed-rate engine
(`analysis::ei_replay`) emitting `combatReplayData.{positions, orientations, dc, iconURL}` and the
top-level `combatReplayMetaData`, 100% f32-*text*-exact on both eras (37/37 and 44/44 players,
50,999 samples); unified 5-map geometry table and 45-icon table, machine-diffed against GW2EI.
`wvWMapData` objective capture left as a documented gap.

**MCONDCAT — condition-skill-id classification catalog.** Reproduced
`SkillEvent.ConditionDamageBased(log)` exactly via the complete 14-id `Buff.BuffClassification.
Condition` catalog instead of the old "buff == 1 and not life-leech" approximation, adding the
fourth `HitKind` bucket both modules had misclassified. Closed the empirically-confirmed post-era
gap (up to 51.4% relative divergence on `powerDamageTakenCount`); every previously report-only
golden check is now hard-exact on all 44 accounts of a real post-era capture.

**M16 — damage modifiers.** A GW2EI-cited attribution engine (10 gain computers) over a
205-definition catalog regenerable from GW2EI source, emitted behind `--modifiers`; 69 of the
reference export's 75 ids covered, `damageModMap` 69/69 character-identical.

**MBUFFSIM — buff-simulator stacking fidelity.** Isolate-first diagnosis overturned the premise:
the simulator was faithful; the real defects were two missing event-pipeline rules
(`BuffRemoveSingleEvent.OverstackOrNaturalEnd` and the `StackingConditionalLoss` `RemovedDuration`
band aid). Stability allowlist 7 → 0, average-stack tolerance 0.05 → 0.005, damage-modifier rows
682 → 779 exact, 0 previously-exact cells disturbed.

**MATTRIB — orphaned-instid attribution repair.** GW2EI's `CompleteAgents` repair transcribed
exactly (two probe points at ±300 ms, *not* a widened window) into `decode_raw`. Also root-caused
and fixed M16's residual: a self-damage filter on the incoming modifier pool GW2EI does not have.
Modifier rows 792/958 exact, every denominator residual pinned at 0.0.

## v0.1.1 — 2026-08-09

**M13 — hit quality and defenses.** Outgoing `statsAll` hit-quality (20 fields, exact vs EI) and
incoming defenses (block/evade/dodge/miss/interrupt/invuln counts plus the
strike/power/condition/life-leech/barrier/breakbar damage-taken breakdown), `ei-json` mapping and
`--view defense`. Surpasses EI on one field: the true life-leech count, which EI's own verified bug
zeroes.

**M14 — rotation and `skillMap`.** Per-player cast tracking (an `AnimatedCastEvent`-pipeline subset,
opt-in `--rotation`, cast count exact vs EI), a best-effort always-on `skillMap` scoped to
referenced skill ids, extended `is_swap`, `ei-json` mapping and `--view rotation`. Closed the last
axibridge-flagged Tier-1 analysis gap.

**MPERF — performance.** Criterion bench harness and CI job; `InstidRegistry` built once (11 full-log
builds → 1); shared `BoonInputs`; time-ordered contribution index; flat-`Vec` registry; healing gate
hoist. Fixture pipeline 50.5 → 28.9 ms, real 583k-event log 325.5 → 174.5 ms (`analyze` 2.63×).
Output verified byte-identical to pre-MPERF `main` across 30/30 surfaces.

**MPUB — registry publishing.** npm `@axiapps/axilog` (plus 5 platform packages) and PyPI `axilog`
both live, install-smoke verified from the public registries. PyPI uses trusted publishing (OIDC);
npm uses an `NPM_TOKEN` secret in `release.yml`. Both idempotent; future releases publish to both
automatically on tag push.

## v0.1.0 — 2026-08-09

The first release: 22 assets (5 CLI binaries + checksums, 6 npm tarballs, 4 wheels + an sdist).

**M11 (done):** the arcdps-methodology contribution family (`downs_contribution`/`downed_by`,
schema 0.1 → 0.2) — a health-anchored attribution window, max(last-≥99%-health − 2000 ms, log start,
prev-down + 2100 ms reset), with four stats (damage/CC/strips/movement-impairing) in both
directions, replacing the retired M1-era 10 s-window approximation. Plus the axibridge Tier-1
surface (`activeTimes`, `combatReplayData.{start,end,down,dead}`).

**M12 (done):** per-skill damage distribution (`totalDamageDist` exact vs EI) behind
`--skill-damage`, per-second cumulative series (`damage1S`/`targetDamage1S`/`damageTaken1S`) and
`dpsTargets` behind `--timeseries`, the `ei-json` mapping for all of it, and matching SDK options.

**M1 (done):** EVTC/zevtc decode, agent/skill resolution, WvW team/friend-foe resolution, damage +
DPS, downs/kills/deaths/down-contribution, CC + per-second timeline, native JSON schema, CLI
(`parse` with `json`/`table`/`csv`/`ei-json`), EI-compat adapter, golden parity test + CI.

**M2 (done):** real elite-spec profession naming, real team IDs from the log itself (`CBTS_WVWTEAMS`)
with a static fallback table, `CBTS_IDTOGUID` content-GUID decoding (teams now, skill/species
retained for M3), CC/stun-break metrics from real `CROWD_CONTROL`/`CBTS_STUNBREAK` events, enemy
relog dedupe, time-aware pet-damage/CC attribution across instid reuse, `axilog anonymize` +
PII-safe committed golden fixture (CI now runs real parity checks, not skip-and-pass), EI adapter
`statsAll` CC fields, squad markers (`CBTS_MARKER`) + commander-tag colour/variant + tick-rate
telemetry (`CBTS_TICK`) — native-schema-only.

**M3 (done):** the 12 tracked boons' stack-count timelines, uptime/presence/average-stacks, and
self/group/squad generation attribution (calibrated exact-to-near-exact vs. EI, see
[`docs/EI-PARITY.md`](EI-PARITY.md)); condi-cleanse/boon-strip/resurrect support stats
(calibrated exact vs. EI, no allowlist); exposed in the native schema (`players[].boons[]`,
`players[].support`), the EI adapter
(`buffMap`, `buffUptimes[]`, extended `support[0]`), and two new CLI table views (`--view
support`/`--view boons`).

**M4 (done):** post-`20260501` (buff-statechange-rework) log support — era-gated boon/support/CC
extraction (dedicated `BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`
statechanges, `ANIMATION_START`-gated resurrect detection, `buff == 1` CC rows), verified by
construction against GW2EI source + synthetic era-equivalence tests (no real post-rework capture
existed yet); downgraded the M3-era unconditional post-rework warning to fire only on genuinely
zero extracted buff events; added `tests/postrework_golden.rs`, a real-capture calibration hook
that activates automatically the moment a `fixtures/local/wvw-postrework.zevtc` fixture exists —
see [`docs/EI-PARITY.md`](EI-PARITY.md#supported-log-eras).

**M5 (done):** Node SDK (`crates/axilog-node`, `@axiapps/axilog`) — napi-rs native addon exporting
`parseFile`/`parseBuffer`/`parseFileEi`/`anonymizeFile` over the same decode → resolve → analyze →
build_report pipeline the CLI drives (no reimplementation, no JSON-over-subprocess); hand-maintained
TypeScript types (`types.d.ts`) for the native schema, patched into the generated `index.d.ts`; a
`node --test` suite covering all four exports plus a dual-path parity test against the CLI's own
`--format json` output; CI builds the addon on Linux/Windows/macOS and runs the node test suite on
Linux (see `.github/workflows/ci.yml`). npm publishing was deferred to MPUB.

**M6 (done):** Python SDK (`crates/axilog-py`, package `axilog`) — PyO3 native extension module
(`abi3-py39`) exporting `parse_file`/`parse_bytes`/`parse_file_ei`/`anonymize_file` over the same
decode → resolve → analyze → build_report pipeline the CLI and Node SDK drive (no
reimplementation); hand-maintained typed stubs (`axilog.pyi` + `py.typed`) for the native schema,
auto-bundled into the wheel by maturin; a stdlib `unittest` suite covering all four exports plus a
CLI-parity test against the CLI's own `--format json` output; CI builds the extension on
Linux/Windows/macOS (`maturin build`) and runs `maturin develop` + the unittest suite on Linux (see
`.github/workflows/ci.yml`). PyPI publishing was deferred to MPUB.

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
`-o/--output FILE` (any `--format`, not just html) added to the CLI alongside it.

**M8 (done):** tag-triggered release pipeline (`.github/workflows/release.yml`, `v*` tags) — CLI
binaries for all 5 targets, `@axiapps/axilog` npm main + platform packages (all 5), and `axilog`
Python wheels (abi3, 4 platforms) + sdist, all attached to one GitHub Release with a consolidated
`SHA256SUMS`; a version single-source guard (`scripts/check-versions.sh`, wired into `ci.yml`)
keeps `Cargo.toml`/`package.json`/npm platform packages/`pyproject.toml` from drifting apart, plus
a tag==Cargo-version guard (`scripts/check-tag-version.sh`) before every release; `npm publish`/
`twine upload` are wired in but gated on `NPM_TOKEN`/`PYPI_TOKEN` repository secrets being
configured (log-skip otherwise — the Release itself, with every artifact attached, is created
either way) and on the triggering event being a real tag push, never a `workflow_dispatch` dry
run. See [`RELEASING.md`](../RELEASING.md) for the full flow.

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
— SVG stage, play/pause/scrub/speed controls, pure node-tested
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
never actually part of the fight. (`ei-json`'s `targets[]`/`statsTargets[]` were left on the full
unfiltered roster at the time; MROSTER later curated them to GW2EI's own WvW rule instead — the two
surfaces are now independent filters over the same enemy list, see `Report::ei_targets`.) Team ids
(`TeamOut.team_id` and the model/analysis layers feeding it) widened `u16` → `u32`, removing a
truncating cast on dynamic `CBTS_WVWTEAMS` ids (future-proofing; no real fixture currently has an
id large enough for the truncation to have mattered).

---

For what is queued, parked or in flight after v0.3.1 — PvE encounter logic, `wvWMapData` objectives,
HTML report extras — see [`docs/ROADMAP.md`](ROADMAP.md).
