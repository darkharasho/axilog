# axilog roadmap

Autonomous build loop. Each milestone: spec → plan → subagent-driven execution (isolated
worktree, per-task adversarial review, final opus whole-branch review) → merge to main + push.
Bar: first-class, performant, accurate-as-fuck. Calibrated numbers stay EI-exact or get a
documented+ruled exception. arcdps README is buggy — hand-count ordinals, GW2EI source is the
algorithm arbiter, dev-relayed arcdps methodology is authoritative.

## Done (merged to main)
- M1 WvW core · M2 polish · M3 boons/support · M4 post-rework era · M5 Node SDK · M6 Python SDK
- M7 HTML report · M8 release pipeline · M9 combat replay · M10 healing/missiles/polish
- **v0.1.0 RELEASED** — GitHub Release with 22 assets (5 CLI binaries+checksums, 6 npm tarballs, 4 wheels+sdist); publish steps gated no-ops (no tokens)
- M11 contribution family + axibridge tier-1
- M12 per-skill (totalDamageDist EXACT vs EI) + per-second (damage1S, --timeseries gate) + dpsTargets + ei-json mapping + SDK ei options
- M13 Hit-quality + defenses: outgoing statsAll hit-quality (crit/flank/glance/against-moving/
  connected/direct/condition/critable/against-downed/life-leech/above-90%-HP, EXACT vs EI) +
  incoming defenses (block/evade/dodge/miss/interrupt/invuln counts, strike/power/condition/
  life-leech/barrier/breakbar damage-taken breakdown, EXACT vs EI except a documented real
  GW2EI `lifeLeechDamageTakenCount` counting bug axilog deliberately doesn't reproduce) + ei-json
  mapping (`statsAll[0]`/`defenses[0]`, EXACT vs EI) + `--view defense`. Post-era classification
  now has REAL (not just synthetic) local calibration; first real capture confirmed the
  documented condition-skill-id-catalog simplification gap is real (not just theoretical) on the
  incoming side, see `analysis::defenses`'s module doc.
- CI: x86_64-apple-darwin cross-compiles on arm64 (macos-13 retired); ci concurrency-cancel
- M13 hit-quality (statsAll, 20 fields EXACT) + defenses (SURPASSES EI: true life-leech count EI's own bug zeroes) + --view defense
- M14 Rotation + skillMap: per-player cast tracking (`AnimatedCastEvent`-pipeline subset, opt-in
  `--rotation`, cast COUNT exact vs EI) + best-effort skillMap (log-table names, always-on,
  scoped to referenced skill ids only) + extended `is_swap` (WeaponSwap sentinel + elementalist
  attunement swaps + revenant legend swaps [5 variants] + necro shroud transforms [3 variants],
  Weaver's own combo-attunement table still out of scope) + ei-json mapping (`rotation[]`/
  `skillMap`, gate-respecting) + `--view rotation` (cast count + APM). Closes the last
  axibridge-flagged Tier-1 analysis gap — see README's EI-JSON parity section.
  Follow-up: py `.pyi` stub synced to the M14 surface (rotation/skillMap TypedDicts + params).

- MPERF Performance milestone (merged 1b5eb5b, reviewed SHIP): criterion bench harness + CI job;
  InstidRegistry built once (11 full-log builds → 1); shared BoonInputs; contribution
  time-ordered index; flat-Vec registry; healing gate hoist. Fixture pipeline 50.5→28.9ms;
  real 583k-event log 325.5→174.5ms (analyze 2.63×). Output verified byte-identical to
  pre-MPERF main across 30/30 surfaces. docs/BENCHMARKS.md has the full applied/declined record.
- **v0.1.1 RELEASED + npm LIVE**: packages renamed to the @axiapps scope and published —
  `npm install @axiapps/axilog` works from the public registry (install-smoke verified).
  M8-parked publish hardening landed (index.js version-literal guard in check-versions.sh,
  pypi-validate wheel install-smoke gate, npm/PyPI publish idempotency).

## Native format program (the axibridge cutover)

The destination: **axibridge runs entirely off axilog's native output, with no
ei-json shim.** The `to_ei_json` layer is PERMANENT regardless — it stays as a
thin compat path so other downstream consumers can migrate on their own
schedule. Absorption removed ei-json's *private data*, not ei-json.

- **Spec #1 / native container 1.0 — DONE**, merged 2026-08-11 (`747875b`).
  `--format json` emits the 1.0 document (`axilog`/`encounter`/`entities`/
  `catalogs`/`blocks`/`coverage`); `docs/NATIVE-FORMAT.md` is the reference.
  Was missing from this roadmap entirely until spec #2 Task 14.
- **Phase A / spec #2 side-channel absorption — DONE** (this branch, Tasks
  1–14; spec `docs/superpowers/specs/2026-08-13-side-channel-absorption-design.md`,
  plan `docs/superpowers/plans/2026-08-13-side-channel-absorption.md`).
  Absorbed the whole `EiInputs` side channel into native blocks, re-pointed the
  adapter at `ReportV1`, deleted `EiInputs` and the two-`Report` split, and
  added `--all` / `everything`. **The ei-json goldens never moved** — which is
  the proof the spec was built to produce: every field ei-json emits is now
  read from the native document, so the goldens attest to native's
  completeness. One named exception, the GW2EI combat-replay position surface
  (`axilog_ei::EiReplayInput`), which spec #1 decision 6 deliberately keeps out
  of the native shape.
- **Phase B — NEXT.** Gaps native can close that ei-json can't: the
  `statsTargets` field subset, replay join keys + down/dead export, enemy class
  as a field, and the six values axibridge derives client-side today
  (zone/map split, encounterDuration, timeStart, distToCom/stackDist). Absorbs
  what an older roadmap called spec #3.
- **Phase C — icons DONE, proc flags open.** Two generated catalogs ship:
  `analysis::skill_icons` (GW2 `/v2/skills`, 4,656 entries, plus `autoAttack`
  from `slot == "Weapon_1"`) and `analysis::buff_icons` (GW2EI's `new Buff(...)`
  table, 2,267 entries). Fixture coverage 329/368; the 39 misses are internal
  damage-proc ids neither source has art for — a floor, not a gap. Proc flags
  still unsourced.
- **Phase D — the axibridge-side reader rewrite. NOT ours**: axilog-side
  readiness only unless the owner says otherwise.

## In flight
(none)

## Done (publishing)
- MPUB COMPLETE: npm `@axiapps/axilog` (+5 platform packages) AND PyPI `axilog` both LIVE at
  0.1.1, install-smoke verified from the public registries. PyPI = trusted publishing (OIDC) via
  `.github/workflows/pypi-publish.yml` (environment `pypi`), dispatched by release.yml after each
  release; npm = NPM_TOKEN secret in release.yml. Both idempotent. Future releases publish to
  both registries automatically on tag push.

- M15 Combat-replay positions in EI shape (merged 549a518, reviewed SHIP): EI fixed-rate
  engine (positions/orientations/dc/start/end, 100% f32-TEXT-exact both eras — 37/37 + 44/44
  players, 50,999 samples); unified 5-map geometry table + 45-icon table (GW2EI machine-diffed
  exact); ei-json combatReplayData/combatReplayMetaData gated --replay; M11 always-on surface
  byte-identical. Fix waves: PlayerActor CR-trim, dst-side awareness, forcePolling squad-only,
  always-emit combatReplayData, PII scrub (incl. pre-existing _note names). wvWMapData
  (objective capture) = documented gap. Follow-up seed: to_ei_json options struct before a 4th arg.

- MCONDCAT Condition-skill-id classification catalog (merged c351e93, task-2 gate flips): reproduced
  `SkillEvent.ConditionDamageBased(log)` exactly via the complete 14-id `Buff.BuffClassification.
  Condition` catalog (`analysis::condition_catalog`, exhaustively scanned + machine-diffed against
  GW2EI source) instead of the old "buff==1 and not life-leech" approximation. Added the fourth
  `HitKind` bucket (buff==1, uncatalogued, not life-leech) both modules previously misclassified.
  The empirically-confirmed post-era gap (M13: up to 51.4% relative divergence on `power_count`,
  33/44 incoming + 2/44 outgoing accounts affected, pure reclassification — conserved total, never a
  dropped/extra event) is now closed: all previously report-only/tolerant golden checks in
  `hit_stats_golden.rs`/`defenses_golden.rs`/`ei_golden.rs` are hard-EXACT on all 44 joined accounts
  of a real post-era capture. Pre-era committed fixture output byte-identical across all 7 formats.

- MDOCS COMPLETE (arcdps-wiki c06aaf2): five-page axilog section live on the wiki (overview,
  quickstart with registry-verified commands, methodology, schema reference, accuracy story —
  all facts source-verified, zero dead links). axilog README de-staled (registries live,
  M11 down-contribution row, Later list).

- MBUFFSIM COMPLETE (merged 2f25110, reviewed SHIP): isolate-first diagnosis OVERTURNED the
  premise — simulator.rs was faithful; the real rules were event-pipeline (natural-expiry
  removal drop + conditional-loss RemovedDuration rewrite, both ported clause-exact). Stability
  allowlist 7→0 with teeth; avg-stack tolerance 0.05→0.005 (10x); modifier rows 682→779 exact;
  0 previously-exact cells disturbed. Deferred (ledgered in-repo): Regeneration/HealingLogic
  (4800x inside tolerance), PRESENCE_TOLERANCE_PP tighten (~0.05pp), boons_golden
  LOCAL_FIXTURE_PATH env-var migration, OffsetBuffExtensionEvents, duration-graph-value latent.

## Queued (autonomous — build in order; reorder only for dependency)
- M16 Damage modifiers COMPLETE (merged 1545930, reviewed SHIP): EiInputs refactor; GW2EI-cited
  engine (10 GainComputers, AlwaysMaster); 205-definition catalog regenerable from GW2EI source
  (scripts/gen_damage_mod_catalog.py, regen diff empty; 69/75 reference ids); emission behind
  --modifiers (native +44.2%, ei-json +441.5% incl. WvW Target variants; damageGain f64 text
  discipline). 30 ids hard-exact all-fields x 44 accounts; 39 bounded per-field (residual proven
  simulator-side -> MBUFFSIM); 69/69 damageModMap identical; 207/207 + 1,978/1,978 rows
  text-identical. NOTE for next MDOCS touch: /axilog/accuracy on the wiki needs a damage-modifier
  row (coverage + bounded-class statement).
- MATTRIB COMPLETE (merged 6edb3c0, reviewed SHIP): GW2EI's CompleteAgents orphan repair
  transcribed exactly (two-probe-point ±300ms, NOT a window) into decode_raw; M16's
  NonZeroAddrIndex retired byte-identically; 106/121 EI-comparable values toward EI (15 exact),
  0 previously-exact disturbed. BONUS: the M16 deficit ROOT-CAUSED AND FIXED — a self-damage
  filter on the incoming modifier pool GW2EI doesn't have (EI filters outgoing only); modifier
  rows 792/958 exact, ids 38/69, all denominator residuals 0.0. 36 local calibration tests now
  run from any worktree via AXILOG_LOCAL_FIXTURES.

## Queued (autonomous)
- MEIGAP ei-json adapter gap closure for axibridge: the axibridge cutover audit
  (axibridge:docs/axilog-cutover-report.md) found 30 of 118 read fields blank under axilog's
  ei-json — chiefly the per-player generation attribution arrays (selfBuffs/groupBuffs/
  squadBuffs — native generation EXISTS since M3, unmapped), targets[].buffs /
  targets[].totalDamageDist / targets[].damage1S mirrors, and powerDamageTaken1S. Mostly
  adapter mapping over existing native data. When closed, axibridge flips its parser default
  from elite-insights to axilog (toggle already shipped).
- MINSTID Enemy-player instid regroup (DONE): `wvw::dedupe_enemy_players` now keys on INSTID,
  GW2EI's own non-squad rule (`AgentManipulationHelper.cs:467-474`), instead of the ACCOUNT that
  WvW anonymization leaves empty. Native `enemies[]` 140 -> 125 rows (71 -> 56 enemy players),
  ei-json `targets[]` 71 -> 56 over EI's exact 56 instids, and the mitigation min-mean residual
  went 16/206 -> 0/206. Every merged row was verified to be the sum/union of its parts; the
  instid-joined calibrations widened from 43 targets to all 56, which exposed 3 pre-existing
  damage-CREDIT divergences (allowlisted + diagnosed in `meigap2_ei_golden`) as the one
  follow-up.

## Queued (autonomous — next session)
- MOBJ wvWMapData objectives: the last whole EI feature surface axilog doesn't emit
  (shard/team ids + GADGETCAPTURE-derived objective ownership timelines; reference shape verified
  — 13 entries on the local export, `{mapID, objectiveID, objectiveType, owners:[[team,time]]}`).
  Two attempts died to process exits before producing commits; no partial work exists. Lowest
  value of the remaining items — nothing consumes it today.

## Parked (user-gated — do NOT do autonomously)
- axibridge's actual cutover from EI CLI to the axilog SDK (both registries now live, so this is
  unblocked whenever the user wants it).
- Replay eye-candy backlog (dev-notes #6/#8: mounts/glider via TRANSFORMATION/GLIDER, capping via
  GADGETCAPTURE) — cosmetic; do only if a milestone naturally reaches it.

## Cross-cutting invariants (every milestone)
- All existing calibration exact; no PII committed (raw .zevtc gitignored; anon fixtures only);
  no-literal-`</script`/textContent-only/determinism for HTML; asset budget honored; warning-free;
  node+python+JS suites green on schema ripple; HTML changes get a controller browser pass.
