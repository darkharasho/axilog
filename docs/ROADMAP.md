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

## Queued (autonomous — build in order; reorder only for dependency)
- M16 Damage modifiers: trait/sigil/food/rune modifier attribution engine
  (`damageModifiers`/`incomingDamageModifiers` + maps). Largest; GW2EI DamageModifier defs +
  decompile for edge cases. Least WvW-critical — can slot late.
- MDOCS Documentation milestone: publish architecture, native+EI schema reference, arcdps-spec
  calc methodology, calibration results to the arcdps-wiki Astro site (one dir up); keep
  axilog README/docs current every milestone regardless.

## Parked (user-gated — do NOT do autonomously)
- axibridge's actual cutover from EI CLI to the axilog SDK (both registries now live, so this is
  unblocked whenever the user wants it).
- Replay eye-candy backlog (dev-notes #6/#8: mounts/glider via TRANSFORMATION/GLIDER, capping via
  GADGETCAPTURE) — cosmetic; do only if a milestone naturally reaches it.

## Cross-cutting invariants (every milestone)
- All existing calibration exact; no PII committed (raw .zevtc gitignored; anon fixtures only);
  no-literal-`</script`/textContent-only/determinism for HTML; asset budget honored; warning-free;
  node+python+JS suites green on schema ripple; HTML changes get a controller browser pass.
