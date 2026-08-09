# axilog M9 — Combat Replay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Position tracks decoded and calibrated vs EI's combatReplayData, opt-in replay data in
the native schema, and an animated Replay tab in the HTML report.

## Global Constraints

- Enum/payload verification protocol (project law): curl + hand-count the arcdps README enum
  (NEVER WebFetch it); cross-check GW2EI source (/tmp/gw2ei clone or GitHub raw —
  MovementEvent/AbstractMovementEvent + CombatEventFactory position handling; positions are
  packed floats in dst_agent/value fields — the EI ctor is the arbiter).
- Golden reference (READ-ONLY): `/var/home/mstephens/Documents/GitHub/axibridge/test-fixtures/boon/20260117-181030.json`
  players[].combatReplayData {positions, orientations, down, dead, dc; start/end},
  top-level combatReplayMetaData {inchToPixel 0.009, pollingRate 300, sizes}. Extract only what
  tests need into `fixtures/wvw-small.ei.json` (stay lean — a FEW players' full tracks, all
  players' first/last/count).
- Existing calibration stays EXACT (205 tests); HTML invariants hold (no external URLs, no
  literal `</script`, textContent-only, determinism); replay report ≤600KB; assets budget: the
  replay JS may push past 50KB combined — controller pre-authorizes raising to 60KB if needed
  (note it in the ledger when used).
- Visual verification: controller screenshots each HTML change (M7 process rule).
- MIT, warning-free, no new runtime crates.

---

### Task 1: Position decode + tracks + EI calibration

**Files:** Create `crates/axilog-core/src/analysis/replay.rs`; modify `evtc/event.rs` (sc consts),
`analysis/mod.rs` (opt-in compute — see interface), `fixtures/wvw-small.ei.json`;
test `crates/axilog-core/tests/replay_golden.rs`.

**Requirements:** verify CBTS_POSITION (and velocity/facing if adjacent) ordinal + payload
packing (floats in the 64-bit fields — GW2EI's ctor is authoritative; document byte layout).
Build `pub fn build_replay(raw, enc, poll_ms) -> Replay` — per squad player AND enemy-player
representative: samples [[t_ms, x, y]] downsampled to poll_ms (300 default), plus z retained
internally; down/dead intervals from existing event analysis. Calibrate: EI positions are in
map-pixel space — derive the exact transform from GW2EI source (inchToPixel etc.), then per
sampled timestamp compare our transformed (x,y) to EI's track for ≥3 players' full tracks +
all-players first/last-sample spot checks: ≥95% of samples within 1.0 px, document outliers.
`Metrics` does NOT grow by default: expose `build_replay` separately (CLI calls it only with
--replay; keep analyze() unchanged). Unit tests: packed-float decode probes (wrong-offset must
fail), downsampling determinism, interval correctness.

### Task 2: Schema + CLI flag + size gate

**Files:** `crates/axilog-schema/src/lib.rs` (`replay: Option<ReplayOut>` skip-none),
`crates/axilog-cli/src/main.rs` (`--replay` flag wiring for json/html), `axilog-node` + `axilog-py`
(optional `replay` param on parse fns — default false, additive, update stubs/types + one test
each), `crates/axilog-html/tests/golden_html.rs` (size gate).

**Requirements:** `ReplayOut { poll_ms, bounds {min_x,min_y,max_x,max_y}, tracks[] { name, team,
commander, is_squad, samples [[t,x,y] rounded 1dp], down_intervals, dead_intervals } }` (names via
the existing display fields; rounding keeps size down). `--replay` on parse: json embeds it; html
passes it through to the report data. SDKs: `parse_file(path, replay=False)` /
`parseFile(path, {replay?})` — verify napi/pyo3 optional-arg ergonomics, keep back-compat.
Size test: replay-enabled fixture report ≤600KB; determinism holds. All suites green
(cargo/node/python).

### Task 3: HTML Replay tab (animation)

**Files:** `crates/axilog-html/assets/report.js`, `report.css`, skeleton; tests (node pure-fn +
golden structural).

**Requirements:** Replay tab appears only when `data.replay` present. SVG stage: abstract dark
field with subtle grid, viewBox from bounds; team-colored dots (squad=green ring emphasis? use
team colors + a distinct squad/enemy shape or stroke), commander ring, hover tooltip (name, via
title/textContent). Controls: play/pause button, scrub slider (input range), time readout mm:ss,
playback at 4x default with speed toggle (1x/4x/8x). Animation via requestAnimationFrame
interpolating between samples (pure function `positionsAt(tracks, t)` — node-tested incl.
interpolation edges). Down markers pulse during down_intervals; dead dots fade. Keep the
zero-network/textContent/no-literal-script invariants (regression tests exist). Pure functions
node-tested (interp, bounds→viewBox, time formatting reuse). Structural goldens updated
(replay container conditional). Regenerate /tmp/axilog-report-replay.html WITH --replay for the
controller's visual pass; also confirm the non-replay report renders unchanged.

## Self-Review
Three tasks: decode+calibrate, plumb+budget, animate+verify. EI transform delegated to source-
reading, calibration quantified, size/XSS/network invariants restated, SDK back-compat explicit.
No placeholders.
