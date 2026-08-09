# axilog — M14: Rotation (cast tracking) & skillMap

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** Unblocks axibridge's Skill Usage / APM sections (needs `rotation[]`) and name/icon
resolution across most sections (needs `skillMap`). The audit's remaining Tier-1 analysis gap.

## Scope

1. **Rotation (cast sequence)** — per squad player, from `is_activation` events (ACTV_START/
   RESET/CANCEL/… — verify the animation enum) + skill ids: EI `rotation[]` shape = grouped by
   skill id, each cast `{ castTime, duration, timeGained, quickness }`. castTime relative to log
   start (can be negative = pre-log cast); duration from the activation window; quickness =
   fraction the cast was sped/slowed (negative = quickness). GW2EI's `ComputeRotations`/
   skill-cast finalization is the arbiter. Calibratable against EI rotation[]. Fully computable.
2. **skillMap** — id → `{ name, autoAttack, isSwap, ... }`. NAMES: best-effort from the log's own
   skill table (RawSkill.name — ~800/969 named on the fixture), falling back to `"Skill <id>"`.
   `autoAttack` (heuristic: repeated no-cooldown cast — verify GW2EI's), `isSwap` (weapon-swap
   skill ids), `canCrit` (reuse M13's NonCritableSkills). **HONEST GAP:** EI's skillMap draws
   fuller names + `icon` URLs from its embedded skill DB / GW2 API; axilog emits log-table names
   only. Icon URLs are external (network) — OUT of scope (a future opt-in GW2-API enrichment,
   network-gated); document, don't fake. Native skillMap carries what we have; ei-json skillMap
   emits name/autoAttack/isSwap for computed entries.

## Calibration
Rotation: per-player cast counts + castTime/duration within tolerance vs EI rotation[] (document
quickness-fraction nuance; GW2EI arbiter). skillMap: where the log table has a name, it should be
sane; overlap-with-EI names spot-checked (many WILL differ — EI's DB is richer; that's the
documented gap, not a failure). Real-log sanity both eras. All existing calibration exact.

## Outputs
Native `players[].rotation[]` + top-level `skill_map`; ei-json `rotation[]` + `skillMap`;
`--view rotation` (per-player APM/cast count summary). Opt-in `--rotation` if size warrants
(measure — rotation arrays are per-cast, could be large; likely gate like --timeseries).

## Non-goals
GW2-API icon/name enrichment (future opt-in, network), damage modifiers (M16), the
condition-catalog milestone (separate).
