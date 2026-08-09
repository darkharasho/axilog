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

## In flight
- MPERF Performance milestone: criterion bench harness (baseline first), then a
  shared-scan/registry-dispatch refactor to collapse the ~15–18 unconditional full passes over
  `raw.events` that `analyze()` currently runs. Flagged rising by the M12 + M14 whole-branch
  reviews. Regression gate + accuracy-preservation (all calibration stays EXACT) are the bars.

## Queued (autonomous — build in order; reorder only for dependency)
- M15 Combat-replay positions in EI shape: resample sparse tracks → EI fixed-rate grid;
  inchToPixel map-scale table (per-map). Unblocks axibridge replay map / heatmap / positioning.
- MCONDCAT Condition-skill-id classification catalog: EMPIRICALLY-CONFIRMED gap (M13 post-era: up to 35%% divergence on condition/power/life-leech buff==1 split; immune fields exact). Pull GW2EI Buff Classification==Condition catalog (like M3's cleanse set, complete). Unblocks exact condition/power split on post-era logs.
- M16 Damage modifiers: trait/sigil/food/rune modifier attribution engine
  (`damageModifiers`/`incomingDamageModifiers` + maps). Largest; GW2EI DamageModifier defs +
  decompile for edge cases. Least WvW-critical — can slot late.
- MDOCS Documentation milestone: publish architecture, native+EI schema reference, arcdps-spec
  calc methodology, calibration results to the arcdps-wiki Astro site (one dir up); keep
  axilog README/docs current every milestone regardless.

## Parked (user-gated — do NOT do autonomously)
- npm/PyPI publishing (needs NPM_TOKEN/PYPI_TOKEN + the M8 publish-hardening: index.js version
  regen, wheel install-smoke gate, publish idempotency/--skip-existing).
- First non-v0.1.0 release tags; axibridge's actual cutover from EI CLI to the axilog SDK.
- Replay eye-candy backlog (dev-notes #6/#8: mounts/glider via TRANSFORMATION/GLIDER, capping via
  GADGETCAPTURE) — cosmetic; do only if a milestone naturally reaches it.

## Cross-cutting invariants (every milestone)
- All existing calibration exact; no PII committed (raw .zevtc gitignored; anon fixtures only);
  no-literal-`</script`/textContent-only/determinism for HTML; asset budget honored; warning-free;
  node+python+JS suites green on schema ripple; HTML changes get a controller browser pass.
