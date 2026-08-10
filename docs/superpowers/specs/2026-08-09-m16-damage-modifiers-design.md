# axilog — M16: Damage modifiers

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** The last major EI-parity surface: trait/sigil/food/rune/relic damage-modifier
attribution (`damageModifiers`, `incomingDamageModifiers`, the `Target` variants, and the
top-level `damageModMap`). The largest milestone by definition count; WvW-first scoping keeps
it tractable.

## Reference shape (verified against the local post-era EI export)

- Per player: `damageModifiers` (11 entries on the sampled player), `incomingDamageModifiers`
  (12), `damageModifiersTarget`, `incomingDamageModifiersTarget` — each entry
  `{ id, damageModifiers: [{ hitCount, totalHitCount, damageGain, totalDamage }] }`.
- Top-level `damageModMap`: `"d<id>"` → `{ name, icon, description, nonMultiplier, ... }` —
  75 entries on the reference log.

## Scope (definition-driven, coverage measured against the reference logs)

1. **Framework** — a modifier-definition model (id, name, icon, description, flags like
   nonMultiplier/skillBased/approximate, source category, and the CHECK predicate: buff
   present on source at hit time / buff on target / stack-count scaling / skill-list gating /
   mode-era gating) + an evaluation engine over damage events producing GW2EI's exact four
   fields per modifier per player (verify each field's semantics in GW2EI's DamageModifier
   classes — gain computation differs between multiplicative %-gain and nonMultiplier
   modifiers; `hitCount` = hits where the check passed, `totalHitCount` = eligible hits).
   Reuses the boon-simulation stack state where checks need "buff active at time t".
2. **Definition catalog** — transcribe from GW2EI's DamageModifier definition files, scoped
   to: every modifier that actually appears in either reference log's `damageModMap`
   (75 post-era + committed-fixture set) plus the full shared/common groups they belong to
   (food/utility/sigil/rune/relic/universal). Full all-professions exhaustiveness is NOT the
   bar — coverage of the observed WvW surface is, with the catalog structured so future
   definitions are additive one-liners. Report coverage % against GW2EI's total.
3. **Emission** — native schema block (gated: measure size; likely `--modifiers` or riding
   an existing flag — decide with numbers) + ei-json `damageModifiers`/`incoming*`/`*Target`
   + `damageModMap` (gate consistent with EI-presence semantics); `to_ei_json` converts to an
   options struct FIRST (the M15 review's carried requirement — before this milestone adds a
   4th argument).

## Calibration

Local post-era export: per-player `damageModifiers[].damageModifiers[0]` rows EXACT
(hitCount/totalHitCount/totalDamage; damageGain within a documented tolerance if GW2EI's
floating gain accumulation demands it — verify, aim EXACT) for every modifier id covered by
the catalog, every joined account; explicitly enumerate any reference-log modifier id NOT
covered and why. Committed fixture golden extended (same route as always). All existing
calibration byte-frozen; both eras sane.

## Non-goals

PvE-only encounter modifiers; GW2-API icon fetching (icons are static URLs in GW2EI source);
exhaustive all-spec catalog beyond the observed WvW surface (additive follow-ups); buff
simulation changes.
