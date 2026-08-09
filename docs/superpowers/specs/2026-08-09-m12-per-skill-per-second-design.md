# axilog — M12: Per-Skill & Per-Second Damage Detail

**Status:** Approved (autonomous per [[axilog-autonomous-mandate]] / docs/ROADMAP.md)
**Why:** The largest remaining axibridge-parity gap after contribution. Per-skill damage
breakdowns and per-player per-second series unblock axibridge's spike-damage detection, player
breakdown, incremental aggregation, and per-target dps views (audit Tier-2). Pure aggregation
over events we already decode — no new wire formats.

## Scope

1. **Per-skill damage distribution** — per squad player: outgoing damage grouped by skill id
   (total, min, max, hit count, crit/flank counts where derivable), split total vs per-target,
   plus incoming (damage taken by skill). Maps to EI `totalDamageDist[][]`,
   `targetDamageDist[][][]`, `totalDamageTaken[][]`. Skill id → name deferred to M14 (skillMap);
   emit ids now.
2. **Per-player per-second series** — `damage1S`, `targetDamage1S`, `damageTaken1S`
   (cumulative-per-second arrays, EI shape). Natural extension of the existing squad-wide
   timeline accumulation, now per player. Native schema `players[].per_second { damage,
   damage_taken, per_target? }`; keep the existing `timeline` squad block.
3. **`dpsTargets`** — per player per enemy dps/damage (EI `dpsTargets[][]`), from the per-enemy
   map we already compute.

## Calibration
EI golden `totalDamageDist`/`damage1S` per-player from the committed fixture's source EI JSON
(READ-only axibridge boon fixture) → extend `fixtures/wvw-small.ei.json`. Gates: per-skill total
per player within 0.5% of EI (exact preferred); per-second array final cumulative == player
damage_total exactly; sum of per-skill == damage_total. Real-log sanity both eras. Size: the
per-second arrays are heavy — native block behind existing determinism/size discipline; consider
opt-in `--timeseries` if the fixture JSON balloons (measure first, decide in-task).

## Non-goals
skillMap names (M14), hit-quality fine detail beyond crit/flank counts (M13), rotation (M14),
damage modifiers (M16). ei-json mapping of these blocks can land here or be noted for a later
adapter pass — prefer landing the high-value ones (totalDamageDist, damage1S) if cheap.
