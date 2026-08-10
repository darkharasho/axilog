# axilog — MBUFFSIM: Buff-simulator stacking-logic fidelity

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** M16's calibration measured non-boon buff-state fidelity for the first time and proved
(error-shape analysis, denominators exact on 98.5% of rows) that the residual on 39 bounded
modifier ids lives in the buff simulator, not the modifier engine. GW2EI's NoID duration
simulator dispatches on `BuffStackType` to THREE stacking logics — `QueueLogic`,
`HealingLogic` (Regeneration), `ForceOverrideLogic` — and its intensity side has
`StackingConditionalLoss` eviction (the M3 Stability 7-cell allowlist's root cause). axilog has
two models (M3 `run_duration` queue + `run_intensity`). Closing the gap flips M16's bounded
rows toward EXACT and may shrink the M3 allowlist.

## Evidence base (from the M16 Task 2 review — the targets)

- `d422` Might-25 saturation: hitCount −64% (all rows negative, totalHitCount exact) — stack
  fine structure at the cap.
- Stability ids `d-425/426/427/428`: systematically over-held stacks at ≥3/≥5/≥10, −1 at ≥1 —
  `StackingConditionalLoss` eviction difference (matches the M3 boons_golden allowlist).
- `d312`/`d369` (Force-type buffs): presence undercount, totalHitCount exact — mechanism NOT
  yet isolated (the capacity==1 duration branch already replaces; do not assume the fix shape).
- `d174`/`d111`: aggregate hitCount exact, all error in damageGain (distinct-boons-present
  fine structure).

## Scope

1. **Isolate before implementing.** Reproduce each target signature with instrumented
   comparisons (per-buff stack timelines vs what EI's numbers imply) and identify the exact
   divergent rule per class. GW2EI source (/tmp/gw2ei): `BuffSimulatorNoID/BuffSimulator.cs`
   dispatch, `QueueLogic`/`HealingLogic`/`ForceOverrideLogic`, `StackingLogic.cs`
   (`StackingConditionalLoss` eviction), `Buff.cs` capacity/type defaults, and
   `CombatData.cs:611` (`UseBuffInstanceSimulator = false` — the NoID family is the arbiter).
2. **Implement the missing logics** in `buffs/simulator.rs`: per-buff `BuffStackType` dispatch
   (the M16 catalog::buff_stack table already classifies intensity-vs-duration per buff — extend
   to full stack-type), ForceOverride replace semantics, Regeneration healing logic,
   StackingConditionalLoss eviction for intensity. The boon-uptime/generation outputs are
   CALIBRATED (437/444 + 7 allowlisted; generation 148/148) — every change must keep the exact
   cells exact and may only IMPROVE allowlisted cells (shrinking the allowlist is a win to
   claim explicitly, with before/after numbers).
3. **Recalibrate everything downstream:** M16's ID_BOUNDS table re-seeded (bounded ids that go
   exact become `IdBound::exact` — count the flips, that's the milestone's proof); M3
   boons_golden (allowlist shrink?); any other buff-state consumer (support cleanse counting
   uses events not simulation — verify unaffected).

## Calibration

Post-era local export: the M16 golden re-run — report per-id before/after; every id that goes
exact gets promoted to a hard assert. M3 boons_golden: presence/generation stay exact;
avg-stack cells re-measured. Committed fixture: boon outputs are always-on — any change to its
numbers must be justified cell-by-cell against the EI golden (improvements only; a regression
in an exact cell is a defect). All existing tests green; both eras; no PII.

## Non-goals

The BuffInstance (per-instance-id) simulator family (EI has it OFF for these logs);
MATTRIB (the incoming-deficit account — separately tracked, must NOT be claimed fixed here);
buff wire-format changes.
