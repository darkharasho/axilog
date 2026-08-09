# axilog — M11: The Contribution Family (arcdps methodology) + axibridge Tier-1

**Status:** Approved (user provided authoritative arcdps methodology 2026-08-09 with the
instruction: copy the METHODOLOGY, never the code)
**Why:** Down/CC/strip contribution done the arcdps way is the project's founding differentiator
(the original brief's first bullet). The methodology is now authoritative, not guessed. Bundled
with the audit's cheap axibridge-adoption wins since health tracking unlocks both.

## The methodology (normative, re-expressed from the dev's description — dev-notes #11)

On a downing blow against a player (strike damage with the down result):
- **Window:** from `max(last_time_target_was_above_99%_health − 2000ms, log_start)` to the down.
  After processing a down, the target's over-99 anchor is reset to `down_time + 2100ms` so a
  subsequent down cannot attribute into the previous burst or the downstate-invuln period.
- **Scan backward** through combat events inside the window; every credit goes to the
  contributor's ultimate master (pet→owner, self included), with an instid→agent consistency
  guard (resolved agent must match the event's agent id), and only when the contributor is NOT
  a friend of the downed player.
- **Four stats:**
  1. `damage_to_downs` — sum of damage dealt to the target in-window.
  2. `cc_to_downs` — +1 per crowd-control application on the target in-window.
  3. `strips_to_downs` — +1 per hostile full-buff-removal (BUFFREMOVE_ALL, iff FOE) of a
     boon-category buff from the target; stability counts only when more than one stack was
     removed (single-stack loss is self-consumption, not a strip). Credit the remover.
  4. `movement_impairing_to_downs` — on single-buff-removals flagged as movement-impairing
     (the is_shields single-remove form), credit the IMPAIRER (resolved from the packed
     source-instid in the overstack field) with the impairment amount.

## Scope

1. **Health tracking:** decode health-update statechanges (ordinal verified per protocol) into
   per-agent health timelines → the over-99 anchor, plus `healthPercents`-shaped data for the
   native schema and future EI adapter use (audit gap).
2. **Contribution engine:** the four stats per squad player (outgoing, vs enemy-player downs)
   AND the incoming mirror (what downed US). Native schema replaces the old approximation:
   `players[].downs_contribution { damage, cc, strips, movement_impairing }` (+ incoming block).
   The legacy 10s-window `down_contribution` field is REMOVED from the native schema
   (schema_version bump 0.1 → 0.2); ei-json keeps emitting `downContribution` mapped from the
   new arcdps-method damage_to_downs with a doc note that EI's own algorithm differs by design.
3. **axibridge Tier-1 (audit cheap wins):** `targets[].isFake`; `combatReplayData.{down,dead}`
   intervals + `start`/`end` in ei-json (reshape of the verified replay intervals — positions
   resampling stays out of scope); `activeTimes` derived from down/dead intervals.

## Validation
No EI calibration possible for the contribution stats (different algorithm BY DESIGN — that is
the point). Gates: synthetic unit tests per stat incl. every nuance (anchor reset, 2s lead-in,
stability stack rule, pet folding, instid guard, friend exclusion); real-log sanity on both
fixture eras (non-zero, downs-count-consistent, printed summary); all existing calibration
EXACT; the removed legacy field's tests updated deliberately. isFake/activeTimes/intervals
validated against the EI goldens (those ARE EI-defined).

## Non-goals
Positions resampling to EI's grid, rotation/skillMap, damage modifiers, per-skill breakdowns
(M12+); movement-impair tooltip niceties.
