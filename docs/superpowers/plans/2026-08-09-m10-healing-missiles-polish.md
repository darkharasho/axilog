# axilog M10 — Healing, Missiles, Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Healing/barrier stats calibrated vs the EI healing extension output; opt-in missile
analytics; the polish batch (combat-participant enemy counts, replay minors, u32 team ids).

## Global Constraints

- Verification protocol (project law): arcdps README via curl + hand-count ONLY (WebFetch has
  fabricated content 3x; a quick grep ALSO produced wrong missile ordinals — the enum extraction
  must be line-by-line inside `enum cbtstatechange`); GW2EI source is the payload arbiter
  (/tmp/gw2ei clone or GitHub raw). For the healing EXTENSION, the wire format arbiter is
  GW2EI's `EvtcParserExtensions`/HealingStatsExtensionHandler (+ the arcdps_healing_stats repo
  if needed — read-only research).
- Fixtures: committed `fixtures/wvw-small.anon.zevtc` + `fixtures/wvw-small.ei.json`; local-only
  `fixtures/local/wvw-postrework.{zevtc,ei.json}` (PII — nothing derived from them enters
  committed files); the user's Red Desert log at
  `/home/mstephens/Downloads/20260701-180601/20260701-174927.zevtc` may be READ for sanity
  checks only. Probe whether the committed fixture's source EI JSON
  (`/var/home/mstephens/Documents/GitHub/axibridge/test-fixtures/boon/20260117-181030.json`,
  READ-ONLY) carries extHealingStats — if yes, committed calibration; else local-only tests.
- ALL existing calibration stays exact: `cargo test --workspace` (233) after each task; node
  (7) + python (10) suites when their crates are touched.
- Budgets: assets ceiling 64KB (65,536B) — authorized this milestone (update the Rust gate with
  a citation comment); report total gates unchanged (250KB / 600KB replay).
- MIT, warning-free, no new runtime crates. HTML changes: regenerate /tmp artifacts for the
  controller's browser pass.

---

### Task 1: Healing & barrier stats (extension decode + calibration)

**Files:** Create `crates/axilog-core/src/evtc/ext_healing.rs` (extension event decode),
`crates/axilog-core/src/analysis/healing.rs`; modify `analysis/mod.rs` (Metrics.healing),
`axilog-schema` (`players[].healing`), `axilog-ei` (extHealingStats subset), CLI
`--view healing`; extend `fixtures/wvw-small.ei.json` IF the source EI JSON has healing;
test `crates/axilog-core/tests/healing_golden.rs`.

**Requirements:** Research the healing extension's event encoding first: arcdps extension
events appear in the combat stream with a signature mechanism (GW2EI's ExtensionHandler
dispatch — how are extension events marked? src_agent==0 && is_statechange==EXTENSION? The
GW2EI source is authoritative; cite files/lines). Decode healing/barrier events (healer, target,
amount, barrier flag, downed flag per the extension's format). Compute per squad player
(account-folded): healing_out_total, healing_out_allies, healing_out_self, barrier_out,
downed_healing_out. Mirror EI's extHealingStats definitions EXACTLY (their
outgoingHealingAllies excludes self? their per-target lists? read EXTHealing statistics source).
Calibration: EI squad sums within 1% (exact preferred) — committed fixture if available, else
local-only (skip-when-absent). Native schema block omitted when the log has no healing
extension data (+ a warnings[] note when absent so users know why). Table `--view healing`:
account, profession, healing out, barrier out, downed-ally healing. Unit tests with synthetic
extension events (wrong-field-mapping must fail).

### Task 2: Missile analytics (opt-in)

**Files:** Modify `crates/axilog-core/src/evtc/event.rs` (verified missile sc consts), create
`crates/axilog-core/src/analysis/missiles.rs`; schema `missiles: Option<...>`; CLI `--missiles`;
tests incl. `missiles` additions to postrework local test.

**Requirements:** Hand-count MISSILECREATE/MISSILELAUNCH/MISSILEREMOVE/MISSILEEFFECT ordinals
(line-by-line enum walk — document the count table in the report). Payloads from GW2EI
MissileEvent/MissileLaunchEvent/MissileRemoveEvent ctors (owner, skill, target/position, remove
reason — which field says blocked/reflected/destroyed and who did it; GW2EI is arbiter). Compute
per-player: missiles_fired; missiles_denied {blocked, reflected, destroyed} attributed to the
denying squad player where the events support it (document exactly what's attributable).
`build_missiles(raw, enc)` standalone (like replay); `--missiles` embeds `missiles` block in
json (html visualization NOT in scope). Sanity gates: real post-rework log (local test,
skip-when-absent): non-zero fired counts, denied ≤ fired, plausible magnitudes logged. Synthetic
unit tests for each remove-reason mapping. If EI's JSON exposes comparable numbers, calibrate;
if not, document that this is native-only (do NOT invent EI fields).

### Task 3: Polish batch

**Files:** `crates/axilog-core/src/wvw/mod.rs` + `analysis` (participant filter), `model`
(team_id u32), `axilog-html` assets (chips, replay minors, contrast), schema if needed; tests.

**Requirements:**
- **Combat participants only:** `enc.enemies` drops agents with zero damage dealt AND zero
  damage received AND zero CC interaction across the fight (loot bags, chests, tactivators);
  keep anything that participated (catapults that took damage stay). Verify counts on the
  committed fixture (80 → fewer; adjust affected tests/goldens deliberately, documenting each
  expected-count change — golden metric VALUES must not change, only entity-list lengths) and
  report the Red Desert log's before/after (456 → expected ~65+dozens). EI parity: enemies[]
  feeds ei targets[] — EI keeps junk targets; note the deliberate divergence in the adapter
  docs (or keep ei-json targets unfiltered from a preserved full list — choose the design that
  keeps ei-json faithful to EI: PRESERVE full agent list internally for the adapter, filter
  only native enemies[] and chips; state the choice).
- **Replay minors:** bounds finiteness (all four), empty-samples tab message, enemy dot
  contrast (+1 stroke weight / lighter fill), per-frame allocation trim if budget allows.
- **team_id u16→u32** end-to-end (model/schema/wvw/ei — WVWTEAMS ids are u32; adjust tests).
- Budgets: update asset gate to 65,536 with citation. Regenerate /tmp artifacts (fixture +
  Red Desert with replay) for controller browser pass.

## Self-Review
Three tasks; extension-format research delegated with arbiters named; missile ordinal trap
called out explicitly; enemy-filter design preserves EI-adapter faithfulness; budget raise
documented. No placeholders.
