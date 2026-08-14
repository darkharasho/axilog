# Spec #2 — Absorb the EI side channel

Spec #2 of the native-format program. Spec #1 (`2026-08-11-native-format-1.0-design.md`)
built the 1.0 container and reserved block slots for this work. Read its
"Decisions taken before this spec (program-level)" section first — all seven
decisions bind here, and decision 6 is what this spec discharges.

## Why

The program's goal, restated by the repo owner on 2026-08-13, is concrete:
**axibridge should run entirely on axilog's native format, with no ei-json
shim.** Everything below is scoped to axilog-side readiness for that cutover.
The axibridge-side reader rewrite is the owner's, not this spec's.

axibridge is already capability-complete on `ei-json` —
`axibridge:docs/axilog-cutover-report.md` audits 83 read-surface rows and finds
5 residual gaps, none of which produces a wrong number. So this is not a
missing-analysis problem. It is a *location* problem: the data axibridge needs
is computed only when `format == Format::EiJson` and handed to the adapter
through `EiInputs`, a side channel that never touches the native report.

Absorbing that side channel is what makes native a superset of ei-json, which
is the precondition for the cutover.

## Scope

In scope: the `EiInputs` side channel, its native destinations, the adapter
re-point that enforces the result, and the gating/coverage vocabulary that
follows.

Out of scope, deferred to later phases agreed with the owner:

- **Phase B** — read-surface gaps native can close but ei-json structurally
  cannot: the `statsTargets` field-subset residual, replay track join keys and
  the `down`/`dead` export gap, enemy class as a field rather than a substring
  of `name`, and the six values axibridge currently *derives* (`zone`/`map`
  split out of `fightName`, `encounterDuration` spelling, `timeStart` from file
  mtime, `distToCom`/`stackDist` reconstruction). This absorbs what was
  previously called spec #3.
- **Phase C** — skill/buff metadata: `skillMap[].icon`/`autoAttack`/
  `isTraitProc`, `buffMap[].icon`. A genuinely new capability needing a GW2
  skill database, not a reshape. axibridge degrades gracefully without it.
- **Phase D** — the axibridge cutover itself. Owner-driven.

## Decisions taken in brainstorming

1. **The adapter reads `ReportV1`, not the legacy `Report`.** Forced by the
   goal, not merely preferred: the ei-json goldens prove *native* completeness
   only if the native document is the adapter's sole input. Pointed at the
   legacy `Report` they would prove the legacy struct complete, which is the
   wrong thing. The legacy `Report` demotes to a `table`/`csv`/`html`-only
   intermediate.
2. **Absorbed passes get freshly designed id-first shapes**, not transliterated
   EI ones (spec #1 decisions 4 and 5).
3. **Consumers that want everything ask once.** A CLI `--all` and SDK
   `everything: true` set every compute gate this binary knows about, defined
   as "every pass that exists in this version" rather than an enumerated list.
   Individual flags stay for consumers who want a small document. The default
   is *not* inverted — a bare `axilog parse` stays cheap and small.
4. **The legacy reprojection direction is preserved for now.** `build_report_v1`
   keeps taking `legacy: &Report`. Inverting it (V1 primary, legacy derived) is
   the right end state but belongs to a spec that can also move
   `html`/`table`/`csv`; doing it here would mean re-pointing the adapter *and*
   reversing the reprojection in one branch, against an oracle that can only
   catch one of them.

## Block map

Thirteen `EiInputs` fields, each with a destination. The "slot" column
references spec #1's reserved-names table.

| `EiInputs` field | Destination | Slot |
|---|---|---|
| `boon_states.total` | `blocks.boons` — per-entity, per-buff stack timeline | reserved (#2) |
| `boon_states.per_source` | `blocks.boons` — same, keyed by **source entity id** | reserved (#2) |
| `target_conditions` | `blocks.conditions` | reserved, `NotComputed` today |
| `minions` | `blocks.minions` | reserved, `NotComputed` today |
| `healing_detail` | `blocks.healing` — detail arrays | reserved (#2) |
| `health_percents` | `blocks.series` | reserved (#2) |
| `enemy_series` | `blocks.series` — rows keyed by enemy entity id | existing block, new rows |
| `enemy_dist` | `blocks.damage` — `by_entity` rows for enemy entities | existing block, new rows |
| `dist_outcomes` | `blocks.damage` — outcome columns on `by_skill` rows | existing rows, new fields |
| `modifiers.per_target` | `blocks.damage_mods` — per-target split | reserved (#2) |
| `activity` | `blocks.replay` — intervals, **always on** (see below) | existing block |
| `healing_series`, `healing_dist` | deleted — they only mirror flag state, which block presence and `coverage` now carry | — |
| `replay` (`EiReplay`) | **not stored.** Derived inside the adapter | — |

### The three that are not relocations

`enemy_dist`, `enemy_series` and `dist_outcomes` are not new blocks. They are
rows and columns native's existing blocks should always have carried, and did
not only because enemy statistics were `#[serde(skip)]` in the legacy shape.
Spec #1's unified `entities[]` dissolved that — an enemy is just an entity id —
so these become ordinary `by_entity` rows rather than a parallel `targets[]`
universe. This is a simplification, not a relocation, and it is the clearest
evidence spec #1 drew the container correctly.

### `activity` is always-on, and splits from the `--replay` gate

`activity` (`ActivityIntervals`: down/dead intervals plus first/last-aware
bounds) is computed by every caller today regardless of format, because it is
cheap. It powers `activeTimes[0]` and `combatReplayData.{start,end,down,dead}`
— all of which the cutover audit marks **E** (always emitted), while
`combatReplayData.{positions,orientations,iconURL}` are marked **F** (gated on
`--replay`).

Native must mirror that split: `blocks.replay` carries intervals
unconditionally and positions only under the gate. Today the whole block is
gated on `legacy.replay.is_some()`, so intervals disappear when positions do.
Fixing this is required for the cutover, since axibridge reads down/dead
without setting a replay flag on that path.

### `ei_replay` stays out, under decision 6's escape hatch

The fixed-rate `EiReplay` track is a pure resampling of `blocks.replay` onto
GW2EI's 300 ms pixel grid. Spec #1 decision 6 explicitly allows a pure
reprojection to be derived inside the adapter rather than stored twice, and
names this as the motivating case. Storing it would put ~1 MB of GW2EI-pixel
data into a native document that already carries the same track in world units.

The adapter therefore gains a derivation step it does not have today. This is
the one place where "the adapter reads only native" is satisfied by computing
rather than by reading, and it must be tested as such: `write_ei_json` on a
document with `blocks.replay` populated must reproduce today's byte-identical
`combatReplayData`.

### Two shapes need redesign, not relocation

Both key by **character name**:

```rust
pub type PerSourceTimelines = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;
pub type TargetConditionStates = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;
```

Names as join keys is the EI wart spec #1's PII boundary exists to kill — names
live in `entities[]` and nowhere else. Native keys both by **source entity id**;
the adapter resolves id → name when it renders EI.

This also fixes a latent correctness bug rather than only a style one: two
players sharing a character name currently collide into one map key, and the
`UNKNOWN_SOURCE` sentinel exists to paper over sources that are not recorded
players. An entity id has neither problem — a non-player source is an entity
too.

`MinionRollups = Vec<Vec<MinionGroup>>` is positionally joined to
`report.players` and becomes entity-id-keyed for the same reason.

## The adapter re-point

`ei_doc` builds an `EiDoc` of borrows that serde streams; nothing materializes
a `Value` on the CLI path (MSTREAM). Re-pointing preserves that property:

```rust
pub fn to_ei_json(report: &ReportV1) -> Value;
pub fn write_ei_json<W: Write>(report: &ReportV1, w: W) -> serde_json::Result<()>;
```

No side inputs. `EiInputs` is deleted, and the compiler enumerates every
violation — the mechanical enforcement decision 6 asks for.

### The memory trap

`build_report_v1` reprojects from the legacy `Report`, so at peak **both
reports are resident**, and after this spec both are larger. Today the ei-json
path builds no `ReportV1` at all. On the 583k-event log that MSTREAM brought
down from ~1.28 GB RSS, this is the regression risk in this spec — memory, not
CPU.

It is smaller than it first appears: `EiInputs`' side buffers are live across
the whole render today, and after absorption they are *owned by the V1 blocks*
— moved, not copied. The delta is the legacy `Report`, not the passes.

**Budget (hard exit criterion):** peak RSS on the 583k-event log with every
gate on must be no worse than today's ei-json peak plus 10%. A failure is
evidence for inverting the reprojection (decision 4's deferred end state), and
the measurement is what would justify scheduling it.

## Gating and coverage

Flags stay compute gates; no flag changes meaning. Absorbed passes attach to
the flag that already gates their EI counterpart, which `crates/axilog-cli/src/main.rs`
already encodes correctly. The only edit per pass is deleting
`&& format == Format::EiJson` from its condition.

| Pass | Gate |
|---|---|
| `boon_states`, `target_conditions`, `health_percents`, `enemy_series` | `--timeseries` |
| `enemy_dist`, `minions`, `dist_outcomes` | `--skill-damage` |
| `healing_detail` | `--skill-damage` or `--timeseries` (its families split across both) |
| `modifiers.per_target` | `--modifiers` |
| `activity` | none — always on |

`--all` / `everything: true` sets every gate this binary knows about. A pass
added in a later milestone is then covered by consumers who already pass it,
which is the point: the first cutover audit found 30 blank fields caused
precisely by a consumer's option list drifting from the parser's.

### `CoverageState::Unsupported` becomes reachable

Spec #1 documented `Unsupported` as reserved vocabulary and told consumers no
current binary emits it, because nothing in that container was era- or
capability-gated. That changes here: `healing_detail` self-gates to `None` on a
log with no healing extension, and boon-state families degrade on post-rework
builds. Those are honestly `unsupported`, not `empty`.

The distinction is load-bearing for consumers. Three states, three actions:

| State | Means | Consumer should |
|---|---|---|
| `not_computed` | the gate was off | pass the flag |
| `unsupported` | this log cannot answer it | stop asking; hide the column |
| `empty` | it ran, the answer is genuinely zero | render a zero |

axibridge's read surface currently cannot tell these apart — it inspects
whether an array came back empty and guesses. `docs/NATIVE-FORMAT.md`'s
"unreachable today" note must be replaced with the real state table.

## Testing

### The oracle is nearly free

Once the adapter's only input is the native document, **"ei-json is
byte-identical to its goldens" and "native carries everything ei-json carries"
are the same statement.** The adapter cannot render a field it cannot read.

So the existing suite — `ei_golden.rs`, the four `meigap*` goldens,
`damage_mods_ei_golden.rs`, `msmall_waste_ei_golden.rs`, and the 36
local-fixture calibrations behind `AXILOG_LOCAL_FIXTURES` — becomes a
completeness proof for native with no new assertions. This is what makes the
`EiInputs` deletion load-bearing rather than cosmetic.

**Re-blessing a golden is forbidden in this spec.** A re-blessed golden is the
sound of native having lost something. `git diff --stat main -- crates/axilog-ei/tests/ fixtures/`
must be empty at the end.

`mstream_streaming_identity.rs` must also keep passing, pinning the
borrow-and-stream property through the re-point.

### What the oracle cannot cover

`v1_equivalence.rs` asserts V1 agrees field-for-field with the legacy `Report`.
The absorbed passes have **no legacy counterpart to agree with** — that is the
definition of a side channel. For the new data specifically, the ei-json
goldens are the *only* oracle.

This drives the sequencing: a block written before its adapter re-point is
unverified code, so no task may write a block without re-pointing its consumer
in the same task.

### New tests

- **PII boundary, extended** over the new blocks: spec #1's assertions (no
  unscrubbed identity anywhere; names only in `entities[]`) must hold. This is
  the enforcement for the name → id redesign, not a spot check — a shape that
  slipped through keyed by string fails here.
- **Coverage state honesty**: a fixture per state. No-healing-extension log
  reports `healing: "unsupported"`; gate-off reports `not_computed`; ran-with-no-rows
  reports `empty`.
- **Key-set golden and determinism**: `v1_shape.rs`'s golden is generated with
  every gate on, so `--all` grows it substantially. Two consecutive parses must
  be byte-identical.
- **Size budgets**: extend `v1_size.rs` per new block. Per-target damage
  modifiers alone measured 854,077 bytes against the whole-fight arrays' 76,611
  — an 11× multiplier.
- **Speed and memory**: criterion bench plus the RSS ceiling above.

## Sequencing

**Task 1 — re-point the adapter with no new data.** `ei_doc` reads `ReportV1`
for everything spec #1 already reprojected. `EiInputs` stays intact, still
carrying all thirteen fields. Goldens byte-identical.

This isolates the single riskiest change in the program — rewriting every EI
block against id-keyed maps instead of positional player arrays — from the
new-data work, so a golden diff has exactly one possible cause.

**Tasks 2–N — absorb the passes**, each task deleting the `EiInputs` field(s)
it absorbs. Grouped and ordered by ascending risk; a group sharing a
destination block is one task, since they touch the same builder:

1. `minions`, `health_percents` — small, self-contained
2. `enemy_dist`, `enemy_series`, `dist_outcomes` — new rows/columns on existing blocks
3. `healing_detail` — split gating
4. `activity` — the always-on/gated split in `blocks.replay`
5. `boon_states`, `target_conditions` — the name → id redesign
6. `modifiers.per_target` — the payload monster

Each task ends with goldens byte-identical and one fewer side input.

**Final task** — delete the empty `EiInputs`; add `--all`/`everything`; make
coverage states honest; regenerate the key-set golden; update
`docs/NATIVE-FORMAT.md`, `docs/EI-PARITY.md`, `docs/BENCHMARKS.md` and
`docs/ROADMAP.md`.

## Risks

All three concentrate in Task 1.

**Ordering.** EI arrays are positionally indexed by player, and
`dpsTargets`/`statsTargets`/`targetDamageDist` are indexed by position in
`targets[]`. V1 blocks are `BTreeMap`s keyed by entity id, assigned in a
different sweep than `enc.players` iteration. The adapter must reconstruct EI's
exact orders from ids. Getting it wrong diffs every golden at once — a loud
failure, which is the good kind. **Mitigation:** build the order reconstruction
as one explicit, documented helper rather than ad-hoc per block.

**Float text.** `damageGain` and the `*Rate` family are f64s whose *text* is
pinned by the goldens. Any change in how a value reaches serde — an
intermediate `f32`, a different arithmetic order inside a block builder — moves
a digit. M16 fought this once already.

**Enemy identity.** Native's enemy rows are first-class entities; EI's
`targets[]` is a filtered, differently-ordered view, and MINSTID's instid
regroup means the mapping is not the naive one. This is the join most likely to
be subtly wrong in a way the small committed fixture does not catch — so the
local-fixture calibrations (56 real enemy instids) must run before the branch
is called done, not only the committed fixture.

## Done

- Every golden byte-identical, none re-blessed;
  `git diff --stat main -- crates/axilog-ei/tests/ fixtures/` empty
- `EiInputs` deleted; `to_ei_json(&ReportV1) -> Value` takes no side inputs
- The 36 `AXILOG_LOCAL_FIXTURES` calibrations green
- Peak RSS within the stated ceiling; criterion bench recorded in
  `docs/BENCHMARKS.md`
- `--all` produces a document whose `coverage` map contains no `not_computed`
- `docs/NATIVE-FORMAT.md` documents the new blocks and the real coverage-state
  table
