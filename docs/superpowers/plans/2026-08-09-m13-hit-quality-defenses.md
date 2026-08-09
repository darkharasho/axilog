# axilog M13 — Hit-Quality & Defenses Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** outgoing hit-quality (statsAll) + incoming defenses hit-outcome/breakdown, calibrated
EXACT vs EI, native + ei-json + `--view defense`.

## Global Constraints

- Result-byte enum + flag semantics VERIFIED (arcdps README hand-count of the cbtresult enum +
  GW2EI DamageResult/ConditionResult; note the pre/post-ResultEnumRework era split already
  handled for CC — the same era gating governs which enum a buff-damage row uses). is_flanking
  (offset 57, decoded M10), is_moving, is_ninety offsets verified against the layout.
- Calibrate vs committed fixture EI JSON (extend `fixtures/wvw-small.ei.json` from the READ-only
  axibridge boon JSON). Counts EXACT where unambiguous; sums ≤0.5%; document EI nuances (GW2EI
  arbiter). ALL existing calibration exact (363 tests); node/python/JS green on ripple; warning-free.
- MIT; determinism (BTreeMap); no PII.

---

### Task 1: Outgoing hit-quality (statsAll)

**Files:** create `crates/axilog-core/src/analysis/hit_stats.rs`; schema `players[].hit_stats`;
test `crates/axilog-core/tests/hit_stats_golden.rs`.

**Requirements:** per squad player (account-folded), over outgoing damage events vs enemies
(same predicate family), classify by result byte + flags: `crit_count`/`crit_damage`,
`flank_count` (is_flanking), `glance_count`, `moving_count` (is_moving), `connected_count`
(landed = not blocked/evaded/absorbed/missed/blinded — verify EI's "connected" exclusion set),
`direct_count`/`direct_damage` (strike) vs `condition_count`/`condition_damage`,
`critable_direct_count`, `against_downed_count`/`against_downed_damage` (target in down state at
event time — reuse M11 down intervals / down state), `life_leech_count`/`life_leech_damage`,
`above90_power_*`/`above90_condition_*` (target health ≥90% via M11 HealthTracker). Verify each
EI field's exact definition in GW2EI (FinalStatsAll / GeneralStatsCalculator) — cite. Native
schema block; calibrate counts EXACT, damage ≤0.5%; document nuances. Unit tests per result
class (synthetic events with each result byte + flag) — wrong classification must fail. Real-log
sanity both eras.

### Task 2: Incoming defenses (hit-outcome + damage-taken breakdown)

**Files:** create `crates/axilog-core/src/analysis/defenses.rs` (or extend downs/damage-taken
path); extend schema `players[].defenses`; test.

**Requirements:** per squad player, over INCOMING events (dst∈squad): `blocked_count`,
`evaded_count`, `dodge_count`, `missed_count`, `interrupted_count`, `invulned_count` (from the
result byte on incoming strikes — dodge vs evade distinction per GW2EI); damage-taken breakdown:
`strike`/`power`/`condition`/`life_leech`/`barrier` counts+sums, `condition_damage_taken`,
`breakbar_damage_taken`. Keep the existing defenses fields (downCount/deadCount/damageTaken/
stunbreak). Calibrate EXACT counts / ≤0.5% sums vs EI defenses[0]. Unit + real-log both eras.

### Task 3: ei-json mapping + --view defense + docs

**Files:** `crates/axilog-ei/src/lib.rs` (+tests), CLI (`--view defense`), README.

**Requirements:** map hit_stats → `statsAll[0]` fields and defenses → `defenses[0]` fields
(EI names exact; only computed fields; verify shapes vs a real export). `--view defense` table
(account, profession, blocks, evades, dodges, dmg taken, downs taken). README parity rows flip.
Calibrate ei-json vs golden. GATES: full workspace + node + python + JS green; calibration exact;
warning-free.

## Self-Review
Three tasks; classification-only over decoded events; EI definitions delegated to GW2EI-cited
verification; exact-count calibration bars. No placeholders.
