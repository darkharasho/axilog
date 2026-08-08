# axilog — M3: Boons & Support Stats

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Scope:** Buff tracking for the 12 core boons + support metrics (cleanses, strips, resurrects),
calibrated against EI's boon fixture for the same golden log. This is the largest EI-parity
subsystem after damage: a buff state machine over apply/remove events.

## Items

1. **Buff event model.** Decode buff application (buff==1, is_buffremove==0, value=duration_ms),
   removal (is_buffremove: ALL/SINGLE/MANUAL, with src attribution), and initial-state events into
   a per-agent, per-buff stack timeline. Handle intensity-stacking (Might 25, Stability 25) vs
   duration-stacking (queue) semantics.
2. **Boon uptimes.** Per squad player, per boon: presence % (time with ≥1 stack) and, for
   intensity boons, average stacks. The 12 boons and their skill ids (from EI buffMap, fixture-
   verified): Might 740, Fury 725, Regeneration 718, Vigor 726, Swiftness 719, Protection 717,
   Aegis 743, Resolution 873, Stability 1122, Quickness 1187, Resistance 26980, Alacrity 30328.
3. **Support stats.** Condition cleanses (total + self), boon strips, resurrects. Golden squad
   sums from EI: cleanse 801, cleanseSelf 97, strips 437, resurrects 6.
4. **Generation attribution.** Who applied each boon (per-source generation), rolled up as
   self/group/squad generation percentages per player.
5. **Outputs.** Native schema `players[].boons` (uptimes + generation) and `players[].support`;
   EI adapter: `buffUptimes[]` (id + uptime/presence + generated), `support[0]`
   (condiCleanse/Self, boonStrips, resurrects — extending the existing stunBreak entry),
   `buffMap` subset for the 12 boons. Table format gains a support summary variant
   (`--format table --view support` or similar minimal flag).

## Correctness gates

- Existing golden parity stays EXACT (49285 / 2,138,414 / CC 34/50460 / stunbreak 20/16907).
- Support squad sums match EI exactly (801 / 97 / 437 / 6) or document precisely why not.
- Boon uptimes: per-player presence within 2 percentage points of EI for the 12 boons across the
  41 fixture players (tolerance reflects simulator simplifications; iterate until met; document
  residual divergence causes). Intensity boons: average stacks within 5% relative.
- Buff-skill identity: use skill ids now; note IDTOGUID SKILL mappings (already decoded in
  RawLog.guid_map) as the stable-identity upgrade path.

## Non-goals
Healing/barrier (needs healing-addon events), rotations/cast tracking, PvE encounters, SDKs,
HTML report, missile analytics (deferred, opt-in — dev-notes #9).
