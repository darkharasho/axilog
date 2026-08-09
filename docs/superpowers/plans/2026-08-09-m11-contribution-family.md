# axilog M11 — Contribution Family + axibridge Tier-1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** arcdps-methodology down/CC/strip/movement-impair contribution (replacing the M1
approximation, schema 0.2), health tracking, and the audit's cheap ei-json wins.

## Global Constraints

- THE METHODOLOGY IS NORMATIVE and lives in the spec (docs/superpowers/specs/2026-08-09-m11-…)
  + dev-notes #11. It was re-expressed from arcdps source the dev shared: implement the
  METHODOLOGY; never transcribe arcdps code, comments, or identifier names.
- Verification protocol unchanged (curl+hand-count ordinals; GW2EI source for payloads —
  health-update event fields, buff-category classification, the packed-instid single-remove
  form and its era behavior). /tmp/gw2ei may exist; fixtures as in M10 (committed pair + local
  post-rework pair + Red Desert log READ-only).
- ALL existing calibration exact after every task (295 tests); node/python/JS suites green on
  schema ripple. Schema bump 0.1→0.2 (legacy down_contribution removed): update every consumer
  (schema tests, ei adapter mapping, CLI views, html JS, SDK stubs, README) deliberately —
  grep-audit for the old field name at the end of Task 2.
- MIT, warning-free, budgets unchanged (65,536B assets).

---

### Task 1: Health tracking + contribution window infrastructure

**Files:** Create `crates/axilog-core/src/analysis/health.rs`; modify `evtc/event.rs`
(health-update ordinal + payload), `analysis/mod.rs` (wire health pass).

**Requirements:** verify the health-update statechange ordinal + payload (arcdps README:
health percent updates; GW2EI HealthUpdateEvent for the encoding — percent scaling factor).
Build per-agent health timelines (time, percent) with an `over_99_anchor(agent, t) -> u64`
query (last time ≥99% before t, log-start default) — the exact anchor semantics the methodology
needs, including the post-down reset hook (reset applied by the contribution engine, not the
health pass — keep health.rs pure). Expose `health_percents`-shaped per-player step series on
`Metrics` (native schema exposure comes with Task 3's ei work or later — this task: compute +
unit tests only; sanity: fixture players' health series non-empty, values 0-100).

### Task 2: The contribution engine (four stats, both directions) + schema 0.2

**Files:** Create `crates/axilog-core/src/analysis/contribution.rs`; modify `analysis/mod.rs`,
`downs.rs` (retire the 10s-window code), `axilog-schema` (0.2: new blocks, legacy field
removed), `axilog-ei` (downContribution ← damage_to_downs + doc note), CLI damage view
(down-contrib column now the new damage_to_downs), html JS if field names ripple, SDK stubs.

**Requirements:** implement the methodology exactly as the spec states: per enemy-player down
(squad outgoing) and per squad-player down (incoming mirror): window from over-99 anchor −2000ms
(clamped to log start and to the previous-down reset of down_time+2100ms), backward scan,
ultimate-master folding via existing time-aware instid machinery WITH the id-consistency guard,
friend-of-target exclusion. Stats: damage (sum), cc (+1 — reuse the era-gated is_cc), strips
(+1: BUFFREMOVE_ALL + iff FOE + boon-category buff + the stability >1-stack rule — boon
category from BOON_IDS; stability id 1122), movement_impairing (single-remove is_shields form:
verify the packed u16 pair in overstack — amount + source instid — against GW2EI/README for BOTH
eras; if the post-era shape differs or is unverifiable, implement pre-era + document).
Native schema: `players[].downs_contribution {damage, cc, strips, movement_impairing}` and
`players[].downed_by {damage, cc, strips, movement_impairing}`; schema_version "0.2"; legacy
field gone (grep-audit). Unit tests per nuance: anchor math incl. reset, 2s lead-in boundary,
stability 1-stack vs 2-stack, pet folding, instid mismatch dropped, friend excluded, window
clamp at log start. Real-log sanity both eras: totals non-zero, per-down attribution ≤ window
damage, printed summary. Update ei-json mapping + adapter doc note; keep statsAll shape.

### Task 3: axibridge Tier-1 ei-json wins

**Files:** `crates/axilog-ei/src/lib.rs` (+tests), possibly `axilog-schema` internal plumbing.

**Requirements:** (a) `targets[].isFake` — from participant/all_enemies distinction: real EI
marks aggregate pseudo-targets; our all_enemies are all real agents, so isFake=false for all —
BUT verify axibridge's expectation (it filters !isFake; emitting false everywhere is correct
and unblocks); (b) `players[].combatReplayData {start, end, down [[s,e]…], dead [[s,e]…]}` from
the replay intervals WITHOUT requiring --replay (intervals are cheap — compute always; positions
stay absent, document); (c) `activeTimes: [duration − down − dead]` per player. Validate
against the committed EI golden: activeTimes within 0.5%, down/dead intervals matching the EI
fixture values (the replay module doc says byte-exact — assert it in the adapter test). Update
adapter docs/README parity table.

## Self-Review
Methodology-normative implementation with every nuance enumerated as a test; schema bump
handled as a deliberate breaking change with grep-audit; ei wins are calibratable and gated.
No placeholders.
