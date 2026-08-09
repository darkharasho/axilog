# axilog — M10: Healing Stats, Missile Analytics, Polish

**Status:** Approved (autonomous continuation; user directed "keep going with M10" 2026-08-09)

## Scope

1. **Healing & barrier stats** (healing-addon extension events). The user's squad runs the
   arcdps healing extension — tonight's EI JSON shows `extHealingStats` for 48/48 players.
   Decode the extension's events (GW2EI's HealingStatsExtensionHandler is the arbiter for the
   wire format), compute per-player outgoing healing/barrier (allies/self) and downed-ally
   healing, calibrate against EI `extHealingStats`/`extBarrierStats` (committed fixture if its
   EI JSON carries them, else local post-rework pair). Native schema `players[].healing`;
   EI adapter `extHealingStats` subset; table `--view healing`.
2. **Missile analytics (opt-in `--missiles`)** — dev-notes #9. Decode MISSILECREATE/LAUNCH/
   REMOVE (+MISSILEEFFECT) — ordinals VERIFIED per protocol (a quick grep produced provably
   wrong numbers; hand-count is mandatory). Per-player: projectiles fired, and the defensive
   headline: projectiles denied (blocked/reflected/destroyed) attributed to the denying agent,
   per GW2EI's missile handling where they model it (native-only if EI JSON exposes nothing —
   never fake EI fields). Opt-in due to event volume. Native schema `missiles` block behind the
   flag; replay/HTML visualization stays backlog (dev-notes: "visual pasta", opt-in later).
3. **Polish batch:**
   - Enemy counts = combat participants only (agents with damage dealt or received > 0):
     fixes "unknown · 391 enemies" being mostly Bags of Loot; applies to team chips AND the
     enemies list default (full agent list behind a `--all-agents` flag if cheap, else dropped
     entirely from enemies[]); replay tracks unaffected (already enemy-players only).
   - Replay parked minors: bounds finiteness guard (all four sides), empty-samples Replay tab
     message, enemy dot contrast bump, per-frame allocation trim if it fits the budget.
   - team_id widened u16→u32 end-to-end (M2 parked; dynamic WVWTEAMS ids are u32).

## Gates
- ALL existing calibration exact (233 tests); healing squad sums within 1% of EI (exact
  preferred; document deviations); missile totals sanity-checked on the real log + synthetic
  unit tests; enemy-count fix verified on the user's Red Desert log (391 unknown → ~dozens).
- Budgets: assets ceiling raised to 64KB (controller-authorized; was 60KB with 170B headroom)
  — polish + healing view need room; total report gates unchanged.
- HTML changes get controller browser verification (established M7 rule).

## Non-goals
Rotations/cast tables, PvE encounters, publishing (pre-token hardening stays parked), replay
missile visualization.
