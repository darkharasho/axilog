# `blocks.self_effects` — squad-side condition and control timelines

**Status:** design, approved in chat 2026-08-19.

## The gap

Elite Insights carried every buff a player *held* — boons and conditions
alike — in one `buffUptimes` array per player. Native splits them by
direction and by family, and the split leaves a hole:

| block | who holds the buff | which ids | stack timelines |
|---|---|---|---|
| `blocks.boons` | squad players | the 12 `BOON_IDS` | yes, under `--timeseries` |
| `blocks.conditions` | **enemies** | the 14 `CONDITION_BUFFS` | `per_source` only |
| — | **squad players** | conditions, Stun, Daze | **nothing** |

Nothing answers "what was on *me*". `blocks.cc` counts crowd-control
*events* — it tests `result == CROWD_CONTROL` and yields counts and summed
durations, which is a genuinely different measurement from a stack
timeline and cannot be turned into one.

The consumer that motivated this is AxiPulse's timeline swimlanes, which
classify a held buff by id into offensive boons, defensive boons, hard CC
and soft CC. The boon lanes light up; the two CC lanes are permanently
empty, and AxiPulse currently ships a doc comment and a test pinning that
as a known coverage gap.

## Scope

Sixteen buff ids, held by squad entities:

- the fourteen of `condition_catalog::CONDITION_BUFFS` — Bleeding 736,
  Burning 737, Confusion 861, Poison 723, Torment 19426, Blind 720,
  Chilled 722, Crippled 721, Fear 791, Immobile 727, Slow 26766, Taunt
  27705, Weakness 742, Vulnerability 738
- **Stun 872 and Daze 833**, which are not conditions — Elite Insights
  classifies both as `Other` — and therefore appear in no existing table

Stun and Daze are in scope because leaving them out would have left the
hard-CC lane showing Fear alone. Measured on the frozen WvW fixture, Stun
is on 12 squad players and Daze on 5; against Fear's 7, dropping them
would discard most of the family the lane exists to show.

**Out of scope: the instantaneous control effects.** Knockdown, Launch,
Pull, Knockback, Float and Sink appear on no player in the fixture, because
they are not duration buffs at all — there is nothing to build a stack
timeline from. They are already represented in `blocks.cc`'s counts, which
is the correct shape for an instantaneous effect.

## Gate

The existing `timeseries` parse option, and nothing new. The pass runs only
when it is on, and the block's presence is the gate signal — the same
convention `blocks.conditions` uses.

This makes `self_effects` a **one-gate block**, unlike `blocks.boons`.
`blocks.boons` is two-gate because its uptime half is computed on every
parse by `build_boons` while `attach_boon_states` only enriches existing
rows, so `coverage.boons` answers the uptime question and says nothing
about whether timelines are present. Here, uptime and states are produced
by the same gated pass and arrive together, so `coverage.self_effects`
answers the whole question and `states` is not an `Option`.

## Core pass — `axilog_core::analysis::self_effects`

A third instantiation of the pipeline `target_conditions`'s module doc
already describes, with the same two substitutions and nothing else:

| | boons | target_conditions | **self_effects** |
|---|---|---|---|
| id table | `buffs::BOON_IDS` (12) | `CONDITION_BUFFS` (14) | **conditions + control (16)** |
| owner scope | squad `Player::agent_addr` | enemy `Enemy::id` | **squad `Player::agent_addr`** |

Event extraction (`buffs::events::extract_buff_events_with_registry`),
capacity extraction (`events::extract_buff_capacities`), the segment
simulator (`generation::run_segments`) and `uptime::compute` are all shared
and need no change. `uptime::compute` is already generic over any timeline.

**The new id table.** A `CONTROL_EFFECTS` table carrying Stun and Daze in
the same `(id, name, is_intensity, ctor_capacity)` shape as
`CONDITION_BUFFS`, so one lookup can span both. The pass resolves capacity
the way `target_conditions::capacity_and_kind` does — the arcdps-reported
capacity from `extract_buff_capacities` when present, the table's ctor
capacity otherwise — but must not call that function unchanged: its
`.expect("... only ever called with a catalogued condition id")` panics on
an id outside `CONDITION_BUFFS`, so the new pass needs its own lookup over
both tables. Nor may it fall through to `simulator::capacity_for`, whose
`_ => 5` arm is documented as unreachable for the ids it knows.

**Unresolved value, to be measured not invented.** The correct
`ctor_capacity` and `is_intensity` for Stun and Daze are not recorded
anywhere in this repo. Implementation must determine them from the
arcdps-reported capacities on the fixture and from Elite Insights' own buff
definitions, and record what was measured in the table's comment. The
fallback only binds on logs that report no capacity, but a wrong constant
there is silent.

**Output shape:**

```rust
pub struct SelfEffects {
    /// (squad agent addr, buff id) -> uptime
    pub uptime: BTreeMap<(u64, u32), BoonUptime>,
    /// (squad agent addr, buff id) -> fused stack timeline
    pub states: BTreeMap<(u64, u32), StateTimeline>,
}
```

`build(raw: &RawLog, enc: &Encounter) -> SelfEffects`, with a
`build_with_registry` sibling matching `target_conditions`.

Note that `buffs::states::build` cannot be reused: it consumes
`metrics.boons`, a set of already-simulated timelines keyed by
`(agent_addr, buff_id)` that exists only for the 12 boons. The new pass
runs its own simulation over the 16 effect ids.

**Duration clamping.** `buffs::states` clamps duration boons to 0/1 so the
graph means what Elite Insights' means, using `is_intensity` over
`BOON_IDS`. The same clamp applies here, driven by `is_intensity` over the
two new tables — most of these ids are duration-stacking, so getting this
wrong would be visible everywhere.

## Schema surface — `blocks.self_effects`

```rust
pub struct SelfEffectsBlock {
    /// squad entity id -> buff id -> row.
    pub by_entity: ByEntity<BTreeMap<u32, SelfEffectRow>>,
}

pub struct SelfEffectRow {
    pub uptime_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    pub states: StateTimeline,
}
```

`avg_stacks` follows the `BoonRow` convention exactly: present for
intensity-stacking effects, omitted rather than a meaningless zero for
duration ones. `states` is unconditional, per the one-gate argument above.

**No `per_source`.** The machinery produces it and "which enemy chained
that stun" is a real question, but nothing asks it today, and it roughly
doubles the block. Additive to add later; a breaking removal if it ships
unused.

Also required:

- `BlockName::SelfEffects` added to the enum and to `ALL`, which becomes
  `[BlockName; 16]`; `envelope.rs`'s
  `coverage_starts_with_every_known_block_not_computed` test asserts the
  coverage map names every block, so this is checked automatically
- the snake_case name arm → `"self_effects"`
- in `build_report_v1`: `passes.self_effects.map(...)` setting
  `computed(block.is_empty())`, and `CoverageState::NotComputed` when the
  pass did not run — the exact shape `conditions_block` uses
- `cats.reference_buff(id)` for all 16 ids, so names and icons resolve
  through `catalogs.buffs` (no human-readable name appears in a block)
- `buffs::name()` extended to compose the third table, so Stun and Daze
  resolve rather than falling through

## Wiring

In `axilog-api`, beside the two existing timeseries passes:

```rust
let self_effects =
    want_timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
```

and a `pub self_effects: Option<&'a SelfEffects>` field on `Passes`.

Type declarations follow in the Node (`types.d.ts`) and Python SDK
surfaces. The EI-JSON adapter needs nothing: Elite Insights already carries
this data in `buffUptimes`, which is exactly what makes the oracle below
possible.

## Payload

Measured on the frozen WvW fixture: `blocks.conditions` is 78,714 bytes
across 56 enemy rows, `blocks.boons` is 331,865 bytes, and the whole report
is 3,287,078 bytes. A squad-side block over 16 ids and 46 players lands
well under 5% of the document — the payload argument that scoped
`target_conditions` narrowly does not bind here.

## Testing — an equality oracle against the frozen export

The same discipline the rest of this repo uses: every value computed twice
and asserted to agree. For each of the 16 ids and each of the 46 squad
players, the block's `states` and uptime are compared against Elite
Insights' `buffUptimes` entry for the same player and buff.

The fixture gives this real teeth rather than a vacuous pass. Measured
player counts and state-pair counts for the ids that matter most:

| id | name | players | state pairs |
|---|---|---|---|
| 722 | Chilled | 43 | 287 |
| 721 | Crippled | 42 | — |
| 720 | Blind | 24 | — |
| 727 | Immobile | 16 | 56 |
| 26766 | Slow | 16 | 52 |
| **872** | **Stun** | **12** | **38** |
| 791 | Fear | 7 | 27 |
| **833** | **Daze** | **5** | **21** |
| 27705 | Taunt | 1 | 3 |

A `—` in the pairs column means the player count was measured and the
pair count was not; the oracle covers those ids regardless, since it
compares against whatever Elite Insights emits rather than against a
hardcoded count.

Stun and Daze — the two ids that motivated the change — are covered by 38
and 21 pairs across 12 and 5 players, so a pass that silently emitted
nothing for them cannot go green.

Additionally: the coverage map must report `not_computed` without
`--timeseries` and `computed` with it, and every one of the 16 ids must
resolve in `catalogs.buffs`.

## Downstream, not in this plan

AxiPulse reads the CC lanes from `blocks.self_effects` instead of hoping
for them in the boons row, deletes the `KNOWN COVERAGE GAP` doc comment in
`src/shared/extract/timeline.ts`, and flips the test in `timeline.test.ts`
that currently pins the gap so it pins the data. Its extractor already
classifies by buff id, so the change is small — but it needs an axilog
version bump and a dependency bump, so it is its own plan.
