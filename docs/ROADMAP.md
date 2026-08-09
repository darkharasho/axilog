# axilog roadmap

Autonomous build loop. Each milestone: spec → plan → subagent-driven execution (isolated
worktree, per-task adversarial review, final opus whole-branch review) → merge to main + push.
Bar: first-class, performant, accurate-as-fuck. Calibrated numbers stay EI-exact or get a
documented+ruled exception. arcdps README is buggy — hand-count ordinals, GW2EI source is the
algorithm arbiter, dev-relayed arcdps methodology is authoritative.

## Done (merged to main)
- M1 WvW core · M2 polish · M3 boons/support · M4 post-rework era · M5 Node SDK · M6 Python SDK
- M7 HTML report · M8 release pipeline · M9 combat replay · M10 healing/missiles/polish
- v0.1.0 tagged (release pipeline first run)

## In flight
- M11 contribution family (arcdps-methodology down/CC/strip/move-impair, health tracking, schema
  0.2) + axibridge tier-1 ei-json (isFake, replay intervals, activeTimes)

## Queued (autonomous — build in order; reorder only for dependency)
- M12 Per-skill + per-second detail: `totalDamageDist`/`targetDamageDist`/`totalDamageTaken`
  (per-skill damage attribution), per-player `damage1S`/`targetDamage1S`/`damageTaken1S`,
  `dpsTargets`. Unblocks axibridge spike-damage, player-breakdown, incremental aggregation.
- M13 Hit-quality + defenses: statsTargets fine-grained (crit/flank/glance/miss/block/evade/
  interrupt/invuln, connected counts), defenses hit-outcome counts, breakbar damage. Needs
  GW2EI/decompile for exact crit/flank/glance definitions.
- M14 Rotation + skillMap: cast/rotation event tracking, skill name/icon map (IDTOGUID SKILL
  mappings already decoded). Unblocks Skill Usage / APM.
- M15 Combat-replay positions in EI shape: resample sparse tracks → EI fixed-rate grid;
  inchToPixel map-scale table (per-map). Unblocks axibridge replay map / heatmap / positioning.
- M16 Damage modifiers: trait/sigil/food/rune modifier attribution engine
  (`damageModifiers`/`incomingDamageModifiers` + maps). Largest; GW2EI DamageModifier defs +
  decompile for edge cases. Least WvW-critical — can slot late.
- MPERF Performance milestone: criterion/bench harness; throughput+memory on the real
  583k-event/821-agent post-rework log; single-pass analysis where possible; regression gate in
  CI. Interleave early (after M12) so later features are measured.
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
