# axilog M14 — Rotation & skillMap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** per-player rotation (cast tracking) calibrated vs EI + a best-effort skillMap from the
log's skill table, native + ei-json + `--view rotation`.

## Global Constraints

- Verify the animation/activation enum (`is_activation` values ACTV_*) + any cast-related
  statechange (ANIMATION_START=67 post-era already known) via arcdps README hand-count; GW2EI's
  skill-cast computation (`ComputeRotations`/`SkillEvent`/`CastFinisher`) is the algorithm arbiter
  for castTime/duration/quickness/timeGained. Cite.
- Calibrate vs committed fixture EI `rotation[]` (extend `fixtures/wvw-small.ei.json`). ALL
  existing calibration exact (442 tests); node/python/JS green on ripple; warning-free; no PII;
  determinism. Measure rotation JSON size → gate behind `--rotation` if it materially grows
  output or breaks HTML budgets (decide in Task 1 with numbers, precedent = --timeseries).
- The skillMap NAME gap vs EI's richer DB is a documented limitation, not a calibration failure.
- MIT.

---

### Task 1: Cast/activation decode + rotation + size decision

**Files:** create `crates/axilog-core/src/analysis/rotation.rs`; modify `evtc/event.rs` if new
consts; schema `players[].rotation`; CLI `--rotation` if gated; test `rotation_golden.rs`.

**Requirements:** verify the activation enum + how a cast's start/duration/cancel are expressed
(is_activation ACTV_START/ACTV_RESET/ACTV_CANCEL_FIRE/ACTV_CANCEL_CANCEL; post-era ANIMATION_START
=67). Per squad player: cast events → `rotation[]` grouped by skill id, each cast `{ cast_time
(ms, rel to log start, may be negative), duration, time_gained, quickness }`. Mirror EI's exact
fields/semantics (GW2EI ComputeRotations — quickness sign, timeGained, cancelled-cast handling).
Native `players[].rotation` (or `rotation_by_skill`). MEASURE size; gate behind `--rotation`
(+SDK opts like --timeseries) if >30% growth / HTML budget break; document. Calibrate: per-player
cast COUNT exact vs EI rotation[]; castTime/duration within a documented tolerance (EI's cast
boundary/quickness math has known rounding); document nuances. Unit tests (synthetic cast
sequences incl. cancel/pre-log-start); real-log sanity both eras.

### Task 2: skillMap from the log skill table

**Files:** create `crates/axilog-core/src/analysis/skill_map.rs` (or in model); schema top-level
`skill_map`; test.

**Requirements:** build id → `{ name, auto_attack, is_swap, can_crit }` from `RawSkill` names
(fallback `"Skill <id>"` for unnamed/numeric); auto_attack heuristic (verify GW2EI's — likely
"no cast cooldown / repeated" or a flag), is_swap (weapon-swap skill ids — WEAPON_SWAP=~ the
known swap skill ids, verify), can_crit (reuse M13 NonCritableSkills). Only skills referenced by
squad players' damage/rotation (don't dump all 969). Native `skill_map`. HONEST: document that
names are log-table best-effort, EI's DB has fuller names + icons (out of scope). Test: named
skills present, ids resolve, fallback works; spot-check a few against EI skillMap (note expected
differences). No calibration hard-fail on names (different DBs).

### Task 3: ei-json + --view rotation + docs

**Files:** `crates/axilog-ei/src/lib.rs` (+tests), CLI (`--view rotation`), README.

**Requirements:** ei-json `rotation[]` (respect the --rotation gate via Option-presence) +
`skillMap` (name/autoAttack/isSwap/canCrit for computed entries; omit icon — document EI has it,
we don't). `--view rotation`: per-player cast count + APM (casts / active-time from M11).
README parity rows: rotation → EMITTED (opt-in), skillMap → EMITTED (partial, log-table names;
icon/DB-names = documented gap). GATES: full workspace + node + python + JS green; calibration
exact; warning-free.

## Self-Review
Three tasks; rotation calibrated + size-gated, skillMap honestly-partial, ei-mapping gate-aware.
Activation enum + GW2EI cast math delegated to cited verification. No placeholders.
