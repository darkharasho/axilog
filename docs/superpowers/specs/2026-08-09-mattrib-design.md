# axilog — MATTRIB: Orphaned-instid attribution repair

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** The last known attribution gap. arcdps emits damage rows whose `src_agent`/`dst_agent`
is 0 while the corresponding instid is live (observed: an enemy ranger pet, found during M16
Task 1). GW2EI repairs these in `EvtcParser.CompleteAgents`: addr-0 rows land in
`orphanedDstInstidCombatItems` and get their address REWRITTEN from the instid
(`EvtcParser.cs:1207-1243`), with the candidate agent required to be within ±300ms of its
aware window. axilog's addr-keyed squad/enemy sets silently drop such rows in `damage`,
`hit_stats`, `defenses`, `skill_damage` (and any other addr-keyed pass). M16 fixed it
module-locally (`damage_mods::NonZeroAddrIndex`); this milestone repairs it globally — which
MOVES calibrated outputs and therefore needs cell-by-cell justification.

Additional prize: M16's quarantined one-account incoming deficit (exactly 7 incoming condition
hits / 239 damage across all 14 of that account's incoming modifier rows) is HYPOTHESIZED to be
this family. This milestone must test the hypothesis and either claim it (deficit → 0, allowlist
removed) or refute it (documented).

## Scope

1. **The repair, done once, where GW2EI does it** — a decode/model-layer pre-pass (not per-module
   indexes): rewrite addr-0 src/dst on eligible rows from the instid per GW2EI's exact rule
   (which rows are eligible, the ±300ms aware-window bound, the earliest-vs-latest candidate
   choice — transcribe `CompleteAgents`' actual algorithm with citations; note GW2EI runs this
   BEFORE analysis, so axilog should too — likely in `evtc` decode post-pass or `model::resolve`).
   M16's module-local `NonZeroAddrIndex` becomes redundant — retire it in favor of the shared
   repair, PROVING the modifier calibration is unchanged-or-improved.
2. **Recalibrate every moved surface** — the repair changes always-on outputs (damage totals,
   hit_stats, defenses, skill_damage, timeline, contribution?). Every moved cell/value on the
   committed fixture AND the local post-era export must be justified: either now-closer-to-EI
   (the expectation — EI does this repair) or explained. Hard gates stay hard; any previously-
   exact calibration that MOVES must move TO the EI value (it was only exact before because both
   sides dropped or both kept — determine which per surface).
3. **The deficit hypothesis** — re-run the M16 modifier calibration: if the deficit account's 14
   rows go exact, remove the structural allowlist and claim it with the mechanism; if not,
   document what the repair did/didn't change there and leave the quarantine.

## Calibration

Committed fixture: full-format diffs vs pre-MATTRIB main — every changed value enumerated and
justified vs the EI golden (closer-or-equal required). Local post-era: all goldens re-run;
report per-suite before/after. All hard-exact gates stay green (or move to the EI value with
proof). Both eras. No PII.

## Non-goals

Wire-format changes; the deferred MBUFFSIM ledger items; `Enemy::instid` population (the
separate latent trap M16 noted — touch only if the repair naturally requires it, and say so).
