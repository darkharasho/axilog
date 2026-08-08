# axilog M3 — Boons & Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Buff state machine for the 12 core boons (uptimes + generation) and support stats
(cleanses/strips/resurrects), calibrated against the EI boon fixture for the committed golden log.

**Architecture:** New `axilog-core` module `analysis/buffs/` (event extraction, stack simulator,
uptime/generation rollups) + `analysis/support.rs`. Schema/EI adapter extensions. No new crates.

## Global Constraints

- Existing golden parity must stay EXACT after every task: duration 49285, squad damage 2,138,414,
  CC 34/50460, stunbreak 20/16907. `cargo test --workspace` after each task (uses the committed
  anon fixture; local raw at `fixtures/local/wvw-small.zevtc` optional).
- EI reference (READ-ONLY): `/var/home/mstephens/Documents/GitHub/axibridge/test-fixtures/boon/20260117-181030.json`
  — per-player `buffUptimes` (buffData[0].uptime/.presence, generated per source),
  `support[0]` (condiCleanse 801 squad / condiCleanseSelf 97 / boonStrips 437 / resurrects 6),
  `buffMap`. Extract needed values into `fixtures/wvw-small.ei.json`; join players by the
  established account mapping (anon accounts are index-derived — see golden.rs).
- Boon ids: Might 740, Fury 725, Regeneration 718, Vigor 726, Swiftness 719, Protection 717,
  Aegis 743, Resolution 873, Stability 1122, Quickness 1187, Resistance 26980, Alacrity 30328.
  Intensity-stacking: Might (25), Stability (25); all others duration-stacking.
- Tolerances: support squad sums EXACT (801/97/437/6) or precisely documented deviation; boon
  presence within 2pp per player; intensity avg stacks within 5% relative.
- arcdps buff-event semantics MUST be verified against the arcdps EVTC README via curl+hand-read
  (NOT WebFetch — fabricated content observed) and cross-checked against GW2EI source
  (raw.githubusercontent.com/baaron4/GW2-Elite-Insights-Parser/master/...) before implementing.
- MIT, edition 2021, warning-free builds, no new external runtime crates.

---

### Task 1: Buff event extraction + state machine

**Files:** Create `crates/axilog-core/src/analysis/buffs/mod.rs`, `events.rs`, `simulator.rs`.
Modify `crates/axilog-core/src/analysis/mod.rs` (wire in; add `BuffMetrics` to `Metrics`).

**Requirements:**
- Verify buff-event field semantics from arcdps README + GW2EI (BuffApplyEvent/BuffRemoveEvent):
  application (buff==1, is_buffremove==0, is_statechange==0, is_activation==0): value = applied
  duration ms, src = applier (master-resolve pets), dst = recipient; removal
  (is_buffremove != 0): ALL(1)/SINGLE(2)/MANUAL(3), value/buff_dmg carry removed duration,
  src = remover for strips/cleanses (verify which field is the remover vs owner — EI source is
  the arbiter). Also handle buff-initial events (statechange BUFFINITIAL — verify ordinal) for
  pre-log-start stacks.
- `simulator.rs`: per (agent, buff_id) stack machine — apply pushes a stack with duration;
  intensity buffs cap at 25 stacks (all ticking), duration buffs tick one stack with the rest
  queued (cap: verify EI's queue cap, typically 5 for duration boons); SINGLE removal pops the
  matched stack, ALL clears, natural expiry decrements. Output: step timeline of
  (time, stack_count) per agent per boon.
- Extract per-boon-per-agent event streams for ONLY the 12 boon ids (skip everything else for
  now — YAGNI; the machine is generic but instantiated for boons).
- Unit tests: synthetic apply/expire/remove sequences for duration + intensity semantics,
  including stack cap and SINGLE-removal-of-queued-stack.

**Produces:** `pub struct BoonTimeline { pub states: Vec<(u64, u32)> }`,
`pub fn simulate_boons(raw: &RawLog, enc: &Encounter) -> BTreeMap<(u64 /*agent rep*/, u32 /*buff*/), BoonTimeline>`
(fold agent addrs by account representative like all other metrics).

### Task 2: Boon uptimes + calibration

**Files:** Create `crates/axilog-core/src/analysis/buffs/uptime.rs`; extend
`fixtures/wvw-small.ei.json`; test `crates/axilog-core/tests/boons_golden.rs`.

**Requirements:**
- From BoonTimeline: presence % (time ≥1 stack / fight duration) and average stacks
  (time-weighted mean) per squad player per boon.
- Extract per-player EI values (buffData[0].presence for duration boons — note EI reports
  `uptime` = avg stacks and `presence` = %>0 for intensity boons; for duration boons `uptime` IS
  the presence % — mirror EI's meaning faithfully; document the mapping) into
  `fixtures/wvw-small.ei.json` for all 12 boons × 41 players.
- Calibration test `boon_uptimes_match_ei_golden`: presence within 2pp per player per boon;
  intensity avg stacks within 5% relative. ITERATE the simulator until green (expect to tune:
  duration-queue cap, extension handling, initial-stack events). Document every simulator rule
  you had to add, with the GW2EI source citation.
- Golden gate: existing calibration stays exact.

### Task 3: Support stats — cleanses, strips, resurrects

**Files:** Create `crates/axilog-core/src/analysis/support.rs`; extend `PlayerMetrics` or add
`SupportMetrics`; extend `fixtures/wvw-small.ei.json`; test in `boons_golden.rs`.

**Requirements:**
- Cleanses: removal events of CONDITION buffs (build the condition id set — EI buffMap
  classification or a curated id list from GW2EI) where remover ∈ squad and recipient ∈ squad;
  self-cleanse when remover == recipient. Strips: removal of BOONS where remover ∈ squad and
  recipient ∈ enemies. Resurrects: verify EI's definition (successful resurrect skill casts —
  check GW2EI source; skill id 1066) and mirror it.
- Verify remover-attribution field semantics against GW2EI (this is the subtle part: in arcdps
  buffremove events the src/dst roles differ from damage events — EI source is the arbiter).
- Calibration: squad sums EXACT vs 801 / 97 / 437 / 6 (document precisely any unavoidable
  deviation with root cause). Per-player values into the fixture JSON where they join cleanly.
- Fold by account representative; pet-sourced cleanses credit owners (calibrate).

### Task 4: Generation attribution (self/group/squad)

**Files:** Create `crates/axilog-core/src/analysis/buffs/generation.rs`; extend fixture JSON;
tests in `boons_golden.rs`.

**Requirements:**
- Per boon application, attribute generated boon-time to the applier: rollups per player —
  self (to self), group (to own subgroup, excl. self), squad (all squad, excl. self) generation
  as % of fight duration (mirror EI's generation semantics — buffData[0].generated per source;
  verify EI's exact denominator from source before implementing).
- Calibration: per-player squad-generation for Might/Quickness/Alacrity/Stability within 2pp of
  EI's summed per-source generated values for that player as source. Document divergences.

### Task 5: Schema, EI adapter, table view, README

**Files:** Modify `crates/axilog-schema/src/lib.rs`, `crates/axilog-ei/src/lib.rs`,
`crates/axilog-cli/src/main.rs`, `README.md`.

**Requirements:**
- Native schema: `players[].boons[]` `{ id, name, presence_pct, avg_stacks?, generation { self_pct, group_pct, squad_pct } }`;
  `players[].support { cleanses, cleanses_self, strips, resurrects }` (stun fields stay in cc block).
- EI adapter: `buffMap` subset (the 12 boons: name, stacking type), `buffUptimes[]` per player
  (id, buffData[0] with uptime/presence/generated in EI's exact meaning), extend `support[0]`
  with condiCleanse/condiCleanseSelf/boonStrips/resurrects. Only computed fields; unit tests.
- CLI: `--view support` for the table format (account, profession, cleanses, strips, resurrects,
  stunbreaks) and `--view boons` (presence % for the key boons: Might avg stacks, Quickness,
  Alacrity, Stability, Protection). Default view unchanged.
- README: M3 section, parity table rows for boons/support with measured-vs-EI status.

## Self-Review
Five tasks cover spec items 1-5. Calibration targets embedded (ids, 801/97/437/6, tolerances).
Verification protocol repeated. Buff simulator unknowns (queue cap, extensions, initial events,
remover-field semantics) are called out as verify-against-GW2EI rather than guessed. Type names
for cross-task interfaces stated in Task 1. No placeholders.
