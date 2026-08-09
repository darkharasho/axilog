# axilog — M9: Combat Replay (positions + animated replay tab)

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Why:** Position data is in every standard log (arcdps position statechanges), EI exposes it as
combatReplayData (our golden fixture has full tracks: 300ms polling, orientations, down/dead
intervals, inchToPixel 0.009), and the HTML report gives it a stage. This is the milestone the
arcdps-dev eye-candy backlog (mounts/glider #6, capping #8) eventually builds on.

## Scope

1. **Position decode:** CBTS_POSITION/velocity/facing statechanges (verify ordinals + packed
   float payloads against arcdps README + GW2EI MovementEvent) → per-agent time-stamped tracks,
   downsampled to a fixed poll interval (match EI's 300ms for comparability).
2. **Calibration:** transform our tracks with EI's inchToPixel convention and compare against
   the golden combatReplayData positions per player (tolerance: positions within 1 map-pixel for
   ≥95% of samples; document EI's exact transform from their source). Down/dead intervals
   cross-checked against our existing downs/deaths events.
3. **Schema:** opt-in `--replay` CLI flag adds `replay { poll_ms, bounds, tracks[] {agent_ref,
   team, samples[[t,x,y]], down_intervals, dead_intervals} }` to the native Report (size-budgeted;
   omitted by default).
4. **HTML Replay tab** (rendered only when replay data present): self-contained SVG/canvas
   animation — team-colored dots on an abstract dark field (NO external map images — the report's
   zero-network invariant holds; subtle grid + bounds), play/pause/scrub slider with time readout,
   commander ring highlight, down markers (pulse) and death fade, hover name tooltip
   (textContent). Controller does visual verification per the M7 process rule.

## Gates
- Calibration vs EI tracks as above; all existing calibration exact; report stays deterministic;
  replay-enabled fixture report ≤ 600KB (positions are the bulk); XSS/no-network invariants hold.

## Non-goals
Real map imagery/tiles, mounts/glider/capping animations (need post-rework events — backlog),
projectile visualization (dev-notes #9, opt-in future), pathing analytics.
