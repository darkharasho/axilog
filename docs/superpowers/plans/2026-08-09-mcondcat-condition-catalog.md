# axilog MCONDCAT — Condition catalog: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** replace the "every buff==1 non-life-leech hit = condition" simplification with
GW2EI's skill-id catalog classification (+ the fourth bucket), turning the last confirmed
divergence into hard-EXACT calibration.

## Global Constraints

- GW2EI source (/tmp/gw2ei) is the arbiter. The catalog must be a complete transcription of
  every buff id GW2EI registers with `BuffClassification.Condition` — cite the definition
  file+lines per group; a completeness check against the source listing belongs in the report
  (machine-diff like M15's icons). NEVER WebFetch the arcdps README.
- Calibration bars: post-era local (AXILOG_LOCAL_FIXTURES route, fixtures in
  /var/home/mstephens/Documents/GitHub/axilog/fixtures/local) — catalog-affected fields EXACT
  for every joined account (they currently diverge on ~2/3 incoming, 2/44 outgoing — those
  accounts are the proof); committed pre-era fixture BYTE-IDENTICAL across every output format
  (zero fourth-bucket hits were observed there — assert it stays that way); all existing
  calibration exact; node/py/JS green on ripple; warning-free (29-line clippy baseline);
  determinism; no PII.
- The `power == strike + life_leech` by-construction identity BREAKS (correctly) on
  fourth-bucket rows — update the tests/docs that assert it; the defenses golden's derived
  life-leech reference must be re-examined (its derivation used that identity — with the
  catalog, re-derive per the module doc's own analysis).
- MIT.

---

### Task 1: The catalog + classification rework + calibration

**Files:** new `crates/axilog-core/src/analysis/condition_catalog.rs` (or in buffs/); modify
`crates/axilog-core/src/analysis/hit_stats.rs` + `defenses.rs`; tests (unit + extend
`hit_stats_golden.rs`/`defenses_golden.rs`).

**Requirements:** locate GW2EI's Condition-classification buff registrations (the Buff
definition lists — likely `GW2EIEvtcParser/EIData/Buffs/...` grouped by source; every entry
constructed with `BuffClassification.Condition`); transcribe the COMPLETE id set with citations
per group; note ids that are conditionally registered (era/mode-gated buffs) and how GW2EI
resolves membership at runtime (`log.Buffs.BuffsByIDs` — if membership depends on the log's
buff-info rather than a static set, reproduce THAT rule and document; the static set is the
fallback arbiter). Rework `classify` in both modules: Condition iff skill id ∈ catalog;
life-leech unchanged; else fourth bucket (counts toward power only, breaking the old identity
exactly as GW2EI does). Audit other buff==1 consumers (skill_damage, timeseries, damage,
healing) — align or document non-applicability. Unit tests per bucket incl. a fourth-bucket
synthetic. Calibrate: post-era EXACT on previously-divergent fields for EVERY joined account
(list the accounts that flipped in the report); committed fixture byte-identical (cmp all
formats). Real-log sanity both eras.

### Task 2: Gate flips + docs + goldens

**Files:** `hit_stats_golden.rs`/`defenses_golden.rs` (tolerances → hard EXACT), module docs in
both analysis modules (simplification disclosures → resolved-by-MCONDCAT notes with the
old text preserved as history), README parity rows (drop catalog-gap caveats), ei-json goldens
if any tolerance text changes.

**Requirements:** flip every report-only/tolerance gate that existed solely because of the
catalog gap to hard EXACT; keep genuinely-unrelated tolerances (document each survivor and
why). Rewrite the affected module-doc sections: the fourth-bucket paragraph becomes "implemented
per MCONDCAT" with the GW2EI citations; the derived-life-leech-reference caveat in
defenses_golden updates to the catalog-aware derivation. README: statsAll/defenses rows lose
the catalog caveat; note the fixed divergence in the parity narrative. GATES: full workspace +
node + python + JS green; post-era hard-EXACT calibration passing; committed fixture
byte-identical; warning-free.

## Self-Review
Two tasks; the catalog is a cited, completeness-checked transcription (M3/M15-table pattern);
the fourth bucket reproduces GW2EI's ctor logic already documented in-repo; the proof is the
previously-divergent accounts going exact. No placeholders.
