# axilog M4 — Post-Rework Log Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** First-class support for arcdps ≥ 20260501 logs: era-gated buff/support/CC extraction
producing identical downstream behavior, era-equivalence tested, with a drop-in calibration hook
for the first real post-rework capture.

**Architecture:** All era differences isolated in extraction (`analysis/buffs/events.rs`,
`analysis/support.rs`, `analysis/cc.rs` predicate). Simulator/uptime/generation/schema untouched.

## Global Constraints

- ALL existing calibration stays EXACT (49285 / 2,138,414 / CC 34/50460 / stunbreak 20/16907 /
  support 801/97/437/6 / uptimes 437+7 / generation). `cargo test --workspace` after each task.
- Every post-era statechange ordinal and payload field verified by: curl + hand-count of the
  arcdps README enum (NEVER WebFetch it — fabricated content observed 3x in this project), AND
  GW2EI source (/tmp/gw2ei clone if still present, else GitHub raw) — cite file+line in comments.
  GW2EI's CombatEventFactory era branches are the arbiter for payload interpretation.
- The /tmp/eiharness DLL harness (if present) may be used to interrogate ambiguous semantics.
- Era dispatch keyed on `RawHeader::is_post_buff_rework` (exists since M3).
- MIT, edition 2021, warning-free, no new runtime crates.

---

### Task 1: Post-era buff statechange decode → BuffEvent model

**Files:** Modify `crates/axilog-core/src/evtc/event.rs` (new sc consts), `crates/axilog-core/src/analysis/buffs/events.rs`.

**Requirements:**
- Verify the post-20260501 buff statechange ordinals (GW2EI names like BuffApply/BuffRemove/
  BuffInitial/BuffStackActive era variants — hand-count README + GW2EI ArcDPSEnums; the M3 report
  estimated 69-72, VERIFY). Decode payloads per GW2EI's post-era factory branches: applier/owner
  agents, duration, is_shields-equivalent activation flag, removal kinds (ALL/SINGLE/MANUAL) and
  removed-duration fields, capacity/BUFFINFO era differences (if any).
- `extract_buff_events`: era dispatch — pre-era branch byte-identical to today (do NOT touch its
  logic); post-era branch produces the same `BuffEvent` structs. Same for
  `extract_buff_capacities`.
- Era-equivalence unit tests: for each scenario in the existing simulator test suite (apply,
  queue, SINGLE removal, ALL clear, initial stacks, capacity override), a post-era synthetic
  twin asserting the extracted `BuffEvent` stream is identical to the pre-era equivalent.

### Task 2: Support + CC era-gating

**Files:** Modify `crates/axilog-core/src/analysis/support.rs`, `crates/axilog-core/src/analysis/cc.rs`.

**Requirements:**
- Support: cleanse/strip removal sourcing goes through the era-dispatched extraction (Task 1's
  removal events carry remover identity — verify post-era remover field vs pre-era inversion
  semantics from GW2EI; resurrect skill-cast detection is era-independent, confirm).
- CC: per the M3 TODO — verify in GW2EI's post-era factory whether genuine CC arrives as
  `buff==1` events with shared DamageResult::CrowdControl; extend `is_cc` era-gated if so.
  Ensure CC damage-exclusion predicates stay consistent everywhere (damage/timeline/down-contrib).
- Era-equivalence tests: synthetic post-era cleanse/strip/CC sequences produce identical counts
  to pre-era twins.

### Task 3: Warning downgrade + calibration hook + README

**Files:** Modify `crates/axilog-core/src/analysis/mod.rs`, create
`crates/axilog-core/tests/postrework_golden.rs`, README.

**Requirements:**
- Warning fires only when post-era log yields zero buff events from BOTH paths (genuinely absent
  data), with updated message.
- `postrework_golden.rs`: skip-when-absent on `fixtures/local/wvw-postrework.zevtc`; when present:
  decode, assert non-zero boon timelines/support counts, print a summary table (so the first real
  capture immediately shows numbers); if `fixtures/local/wvw-postrework.ei.json` also present,
  assert parity within M3 tolerances (reuse the join/tolerance helpers — factor them into a small
  shared test-support module if needed).
- README: "Supported log eras" updated (post-era now supported, calibration pending first real
  capture — instructions for dropping a fixture); parity table era columns.

## Self-Review
Three tasks; era isolation keeps calibrated paths frozen; equivalence testing is the strongest
pre-fixture check; the hook makes real-log validation a file-drop. Verification protocol repeated.
No placeholders.
