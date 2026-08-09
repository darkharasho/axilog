# axilog M12 — Per-Skill & Per-Second Detail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** per-skill damage distributions + per-player per-second series + dpsTargets, calibrated
vs EI, native schema + ei-json where cheap.

## Global Constraints

- Autonomous per docs/ROADMAP.md. arcdps README hand-count for any new field (none expected —
  this is aggregation over already-decoded strike/condi events); GW2EI source is the arbiter for
  EI's totalDamageDist/damage1S SHAPE (array layouts, indexing, cumulative-vs-instant).
- Calibration: committed fixture's source EI JSON (`/var/home/mstephens/Documents/GitHub/axibridge/test-fixtures/boon/20260117-181030.json`, READ-only, anon) → extend `fixtures/wvw-small.ei.json`. Per-skill totals within 0.5% (exact preferred); per-second final cumulative == damage_total EXACT; sum(per-skill) == damage_total EXACT.
- ALL existing calibration exact (336 tests); node/python/JS green on schema ripple; warning-free;
  no PII; determinism. Measure per-second JSON size on the committed fixture; if native output
  grows >~30%, gate the per-second block behind `--timeseries` (decide in Task 2 with numbers).
- MIT.

---

### Task 1: Per-skill damage distribution (outgoing + taken, per-target)

**Files:** create `crates/axilog-core/src/analysis/skill_damage.rs`; modify `analysis/mod.rs`
(Metrics), `axilog-schema` (blocks), test `crates/axilog-core/tests/skill_damage_golden.rs`.

**Requirements:** per squad player (account-folded), group outgoing damage-to-enemies by skill
id: `{ skill_id, total, hits, min, max, crit_hits?, flank_hits? }` (crit/flank from the result
byte / is_flanking — include only if cleanly derivable now, else omit and note for M13); a total
list and a per-target list (`per_target[enemy_id][skill_id]`). Also incoming `damage_taken` by
skill id. Reuse the established damage predicate (statechange/activation/buffremove excluded, CC
excluded, buff==1→buff_dmg else value, pet-fold to owner). Native schema:
`players[].skill_damage { outgoing[], taken[], per_target[] }` (shapes your call — document;
keep it queryable: enemy_id + skill_id explicit, not positional). Calibration vs EI
`totalDamageDist`/`totalDamageTaken`/`targetDamageDist`: extract per-player per-skill totals into
the fixture json; assert sum(per-skill)==damage_total exact, and top-skill totals within 0.5% of
EI (document EI's grouping — EI splits by skill AND indirect/direct; mirror its total). Unit
tests: synthetic multi-skill/multi-target; real-log sanity both eras. `analyze()` may compute
this inline (it's one pass) — keep it from bloating hot path unnecessarily (single grouped scan).

### Task 2: Per-player per-second series + dpsTargets + size decision

**Files:** create `crates/axilog-core/src/analysis/timeseries.rs`; modify schema, CLI (`--timeseries` if needed), test.

**Requirements:** per squad player: `damage1S`, `damage_taken1S`, and `target_damage1S` (EI:
cumulative per-second totals; verify cumulative-vs-instant against GW2EI — EI's *1S arrays are
cumulative). Bucketed at 1000ms from log start like the existing timeline. `dpsTargets`: per
player per enemy {dps, damage} from the per-enemy map. Native schema
`players[].per_second { damage[], damage_taken[], per_target[] }` and `players[].dps_targets[]`.
MEASURE: serialize the committed fixture with/without per-second; if size grows >30% OR the
report/json becomes unwieldy, gate `per_second` behind a `--timeseries` CLI flag (json + ei-json
only; default off) — document the measured numbers and the decision. Calibration: final
cumulative value of damage1S == player damage_total EXACT; per-second monotonic non-decreasing;
spot-check against EI damage1S final values. dpsTargets total == sum per_enemy. Unit + real-log
sanity both eras. SDKs: if `--timeseries` added, thread the option like `replay`/`missiles`
(back-compat); stubs updated.

### Task 3: ei-json mapping + docs

**Files:** `crates/axilog-ei/src/lib.rs` (+tests), README parity table.

**Requirements:** map the high-value blocks into ei-json: `totalDamageDist`/`targetDamageDist`/
`totalDamageTaken` (EI's array-of-arrays-of-{id,...} shape — match exactly, only computed fields),
`damage1S`/`targetDamage1S`/`damageTaken1S` (respect the --timeseries gate if added — when off,
omit rather than emit empty), `dpsTargets`. Only emit fields we compute; verify shapes against a
real EI export. Calibrate the ei-json arrays against the golden where present. README: parity
table rows flip from ABSENT/PARTIAL to EMITTED for these; update the axibridge-gap status.
GATES: full workspace + node + python + JS green; calibration exact; warning-free.

## Self-Review
Three tasks: per-skill, per-second (+size gate), ei-mapping. Aggregation-only (no wire risk);
calibration bars explicit (exact sums, 0.5% per-skill); size measured-then-decided. No placeholders.
