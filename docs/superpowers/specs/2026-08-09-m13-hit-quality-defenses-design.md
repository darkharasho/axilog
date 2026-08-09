# axilog — M13: Hit-Quality Stats & Defenses Detail

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** axibridge's dashboard defense tiles + per-target stat catalog (audit Tier-1 PARTIAL).
Pure classification of the result byte + flags on events we already decode — no new wire.

## Scope (all calibratable vs the committed fixture's EI JSON — statsAll + defenses)

1. **Outgoing hit-quality** (EI `statsAll`, per squad player): counts+rates for critical,
   flanking, glancing, moving-target; connected vs total (blocked/evaded/absorbed/missed
   reduce "connected"); direct (strike) vs condition damage counts and sums; critable-direct
   count; against-downed count+damage; life-leech; above-90%-HP power/condi splits. Derived from
   the result byte (verified enum: normal/crit/glance/block/evade/absorb/blind/interrupt/
   killingblow/downed/breakbar/... — GW2EI DamageResult + arcdps README are the arbiters) plus
   is_flanking/is_moving/is_ninety flags and the target's down/health state.
2. **Incoming defenses** (EI `defenses`, per squad player): blockedCount, evadedCount,
   dodgeCount, missedCount, interruptedCount, invulnedCount; damage-taken breakdown —
   strike/power/condition/life-leech/barrier counts+sums; conditionDamageTaken;
   breakbarDamageTaken. Extends the existing defenses block (downCount/deadCount/damageTaken +
   the M2 stunbreak fields already there).

## Calibration
Committed fixture EI JSON has all of these per player. Extend `fixtures/wvw-small.ei.json`;
gate: counts EXACT vs EI where the definition is unambiguous (crit/flank/glance/block/evade/
dodge/miss/interrupt counts), damage sums within 0.5%; document any EI-definition nuance
(e.g. "connected" exclusions, above-90 threshold semantics, dodge vs evade distinction —
GW2EI source is the arbiter, cite it). Real-log sanity both eras.

## Outputs
Native `players[].hit_stats { ... }` (outgoing) + extend `players[].defenses { ... }`;
ei-json `statsAll[0]` gains the hit-quality fields + `defenses[0]` the outcome/breakdown counts
(only computed fields). Table `--view defense` (dashboard-style: blocks/evades/dodges/dmg-taken).

## Non-goals
Per-second hit-quality, rotation, damage modifiers, skillMap names (M14+).
