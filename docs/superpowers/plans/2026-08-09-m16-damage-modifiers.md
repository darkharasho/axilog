# axilog M16 — Damage modifiers: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** GW2EI-calibrated damage-modifier attribution (outgoing/incoming/Target variants +
damageModMap), definition-driven with WvW-observed coverage, landing the to_ei_json
options-struct refactor first.

## Global Constraints

- GW2EI source (/tmp/gw2ei) is the arbiter for: the four output fields' semantics, each
  modifier's check predicate, gain math (multiplicative vs nonMultiplier), and every
  definition transcribed (cite file+line per definition; machine-diff the catalog against the
  source listing in the report — M15-icons pattern).
- Calibration: local post-era export (AXILOG_LOCAL_FIXTURES route) — covered modifier rows
  EXACT per player (hitCount/totalHitCount/totalDamage; damageGain EXACT unless a documented
  GW2EI-float reason emerges); enumerate uncovered reference ids + why. Committed fixture
  golden extended + byte-frozen elsewhere. All existing tests green (~578+); clippy 29-line
  baseline; determinism; no PII; node/py/JS green on ripple.
- The catalog must be structured for additive growth (one definition = one table entry with
  its citation; no logic edits to add one).
- MIT.

---

### Task 1: to_ei_json options struct + modifier framework + engine

**Files:** `crates/axilog-ei/src/lib.rs` (options-struct refactor FIRST — separate commit,
byte-identical gate: all callers CLI/node/py updated, cmp committed-fixture ei-json all-flag
variants before/after); new `crates/axilog-core/src/analysis/damage_mods/` (model + engine);
unit tests.

**Requirements:** options struct `EiOptions { activity, replay, ... }` replacing the growing
positional args (M15 final-review requirement) — mechanical, byte-identical, all suites green.
Then the framework: definition model (id/name/icon/description/flags/source/check) + engine
evaluating per squad player over outgoing (and incoming) damage events: per modifier
`{ hit_count, total_hit_count, damage_gain, total_damage }` with GW2EI's exact semantics
(verify in GW2EI: DamageModifier hierarchy — BuffOnActorDamageModifier / BuffOnFoeDamageModifier
/ SkillDamageModifier etc.; GainComputer variants (GainComputerByPresence/ByStack/
ByMultiPresence, nonMultiplier); which events are "eligible" (totalHitCount) — cite each).
Buff-presence checks reuse the M3 boon-simulation stack state / buff events (era-gated wire
already handled) — expose what the engine needs WITHOUT altering simulation outputs. Unit
tests: synthetic events + a fake modifier per gain-computer variant. No emission yet.

### Task 2: Definition catalog (WvW-observed coverage) + calibration

**Files:** `crates/axilog-core/src/analysis/damage_mods/catalog/` (grouped: gear —
food/utility/sigil/rune/relic; common/universal; profession files for specs observed in the
fixtures); calibration test `damage_mods_golden.rs`.

**Requirements:** enumerate the union of modifier ids in both reference logs' damageModMap
(75 post-era + committed set); transcribe every one from GW2EI's definition files (+ complete
the shared group each belongs to even if unobserved — cheap and keeps groups coherent); cite
per definition; machine-diff in the report. Calibrate: per-player rows EXACT vs the local
export for every covered id/account (this is the milestone's proof); enumerate any id left
uncovered + why (e.g. PvE-only in a WvW log = definitional impossibility — justify).
Committed-fixture calibration via the established golden-extension route. Real-log sanity both
eras. Coverage % vs GW2EI total reported.

### Task 3: Emission — native + ei-json + docs

**Files:** `crates/axilog-schema` (gated block), `crates/axilog-ei` (damageModifiers/
incomingDamageModifiers/*Target + damageModMap via the new options struct), CLI flag, SDK
options, README parity rows, goldens, wiki-page follow-up note (do NOT edit the wiki repo —
leave a one-line roadmap note that /axilog/accuracy needs a modifier row next MDOCS touch).

**Requirements:** measure size; gate (likely `--modifiers`; decide with numbers, precedent
--timeseries); ei-json emission matching EI shape exactly (float text discipline per M15's
ei_float where floats appear); extend committed golden; flip README parity rows
(damageModifiers → EMITTED with coverage note). GATES: full workspace + node + python + JS
green; calibration bars met; committed fixture byte-identical on non-modifier surfaces;
warning-free.

## Self-Review
Three tasks; options-struct debt paid first with a byte-identical gate; the engine is
GW2EI-cited with per-variant unit tests; the catalog is citation-per-definition and
machine-diffed; the proof is per-player EXACT rows on a real WvW log. No placeholders.
