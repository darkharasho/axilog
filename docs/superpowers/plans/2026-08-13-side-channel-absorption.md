# Side-Channel Absorption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the nine analysis passes that `EiInputs` side-channels into the native 1.0 blocks, re-point the ei-json adapter to read `ReportV1` alone, and delete `EiInputs` — making native a provable superset of ei-json.

**Architecture:** Three movements, strictly ordered. First, give `ReportV1` the source-order join the adapter needs and build one explicit reconstruction helper (Tasks 1–2). Second, re-point `ei_doc` surface by surface onto `ReportV1` with **no new data**, so the goldens isolate re-point bugs from absorption bugs (Tasks 3–5). Third, absorb the passes one destination block at a time, each task deleting its own `EiInputs` field (Tasks 6–12), then delete the empty struct and land the consumer surface (Tasks 13–14).

**Tech Stack:** Rust 2021, `serde`/`serde_json`, `criterion` for benches, `BTreeMap` for every id-keyed map (determinism is a golden-diff requirement, not a preference).

**Spec:** [`docs/superpowers/specs/2026-08-13-side-channel-absorption-design.md`](../specs/2026-08-13-side-channel-absorption-design.md)

## Global Constraints

Every task's requirements implicitly include these. They come from the spec's "Testing" and "Done" sections and spec #1's cross-cutting invariants.

- **No golden may be re-blessed.** `git diff --stat main -- crates/axilog-ei/tests/ fixtures/` must be empty at the end of every task. A golden that needs updating means native lost something — stop and diagnose, do not regenerate.
- **`cargo test --workspace` green at every commit.** Run with `--maxWorkers`-equivalent restraint: this repo's suites are memory-heavy, so prefer `cargo test -p <crate>` while iterating and the full workspace before committing.
- **Warning-free.** `cargo build --workspace` emits no warnings.
- **No PII committed.** Raw `.zevtc` is gitignored; only anonymized fixtures. Names appear in `entities[]` and nowhere else in the 1.0 document.
- **Determinism.** Two consecutive parses of the same input produce byte-identical output. Every new map is a `BTreeMap`, never a `HashMap`.
- **Float text discipline.** Values whose text is golden-pinned (`damageGain`, the `*Rate` family) must reach serde through the same arithmetic path they do today. No intermediate `f32`, no reordered arithmetic.
- **Local calibrations.** The 36 tests behind `AXILOG_LOCAL_FIXTURES` must pass before the branch is declared done. They cover 56 real enemy instids the committed fixture does not.
- **Peak RSS ceiling.** On the 583k-event log with every gate on: no worse than today's ei-json peak + 10%.
- **Commit signing.** This environment needs `SSH_AUTH_SOCK="$HOME/.1password/agent.sock"` prefixed on `git commit`, or signing fails with `1Password: failed to fill whole buffer`. Never use `--no-gpg-sign`.

---

## File Structure

**New files:**

| Path | Responsibility |
|---|---|
| `crates/axilog-schema/src/v1/order.rs` | `SourceOrder` — the encounter-order join `ReportV1` carries for reprojection consumers |
| `crates/axilog-schema/src/v1/blocks/conditions.rs` | `blocks.conditions` — per-target condition state timelines, source-entity keyed |
| `crates/axilog-schema/src/v1/blocks/minions.rs` | `blocks.minions` — per-entity minion rollups |
| `crates/axilog-ei/src/join.rs` | `EiJoin` — reconstructs EI's positional orders and row lookups from `ReportV1` |
| `crates/axilog-ei/src/replay_derive.rs` | Derives the GW2EI fixed-rate track from `blocks.replay` (decision 6 escape hatch) |
| `crates/axilog-schema/tests/v1_coverage_states.rs` | Asserts `not_computed` / `unsupported` / `empty` are each reachable and correct |

**Modified files:**

| Path | Change |
|---|---|
| `crates/axilog-schema/src/v1/mod.rs` | `ReportV1` gains `source_order`; new blocks wired; coverage states extended |
| `crates/axilog-schema/src/v1/entities.rs` | `build_entities` returns source order alongside the index |
| `crates/axilog-schema/src/v1/blocks/damage.rs` | Enemy `by_entity` rows; `by_skill` outcome columns |
| `crates/axilog-schema/src/v1/blocks/support.rs` | Boon state timelines; healing detail arrays |
| `crates/axilog-schema/src/v1/blocks/activity.rs` | Enemy series rows; health percents; damage-mod per-target; replay interval/position split |
| `crates/axilog-ei/src/lib.rs` | `ei_doc`/`to_ei_json`/`write_ei_json` re-pointed; `EiInputs` deleted |
| `crates/axilog-cli/src/main.rs` | `format == Format::EiJson` compute conditions deleted; `--all` added |
| `crates/axilog-node/src/lib.rs`, `crates/axilog-py/src/lib.rs` | `everything` option; `EiInputs` construction removed |
| `crates/axilog-node/types.d.ts`, `crates/axilog-py/*.pyi` | Regenerated to the new surface |
| `docs/NATIVE-FORMAT.md`, `docs/EI-PARITY.md`, `docs/BENCHMARKS.md`, `docs/ROADMAP.md` | Final task |

**Why `order.rs` and `join.rs` are separate files:** the spec names ordering as the top risk and prescribes "one explicit, documented helper rather than ad-hoc per block." Splitting the *production* of source order (schema side) from its *consumption* (adapter side) keeps each testable alone — the schema side can be verified against the legacy `Report` without involving the adapter at all.

---

### Task 1: `SourceOrder` — the join the adapter is missing

`EntityOut` has `id`, `role`, `account`, `character`, `team`, `subgroup` — but no agent address and no encounter position. `build_entities` sorts by `(role, team, subgroup, account, character, agent_addr)` and drops `EntityIndex` when `build_report_v1` returns.

EI's `players[]` is positionally ordered by `enc.players` iteration, and `targets[]` by `report.ei_targets`. Neither is recoverable from a sorted roster. Without this task, Task 3 cannot begin.

`SourceOrder` is `#[serde(skip)]` — invisible on the wire, exactly like the precedent `PlayerOut::agent_addr` set in spec #1 Task 5. It is named for what it *is* (the encounter's original agent order), not for its first consumer, because a second reprojection would want the same thing.

**Files:**
- Create: `crates/axilog-schema/src/v1/order.rs`
- Modify: `crates/axilog-schema/src/v1/entities.rs` (`build_entities` return type)
- Modify: `crates/axilog-schema/src/v1/mod.rs` (`ReportV1` field, `build_report_v1` wiring)
- Test: `crates/axilog-schema/tests/v1_source_order.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct SourceOrder {
      players: Vec<u32>,
      targets: Vec<u32>,
  }
  impl SourceOrder {
      pub fn players(&self) -> &[u32];
      pub fn targets(&self) -> &[u32];
      pub fn player_position(&self, entity_id: u32) -> Option<usize>;
      pub fn target_position(&self, entity_id: u32) -> Option<usize>;
  }
  ```
  `ReportV1::source_order: SourceOrder`, `#[serde(skip)]`.
- Consumes: `EntityIndex::by_agent_addr`, `EntityIndex::by_enemy_id` (existing).

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/tests/v1_source_order.rs`:

```rust
//! `SourceOrder` must reproduce the legacy report's iteration orders
//! exactly, because ei-json's positional arrays are indexed by them.

mod common;
use common::fixture_report;

#[test]
fn player_order_matches_the_legacy_report() {
    let (enc, metrics, legacy, v1) = fixture_report();

    let by_source: Vec<u32> = v1.source_order.players().to_vec();
    assert_eq!(
        by_source.len(),
        legacy.players.len(),
        "every legacy player must appear exactly once in source order"
    );

    // Position i in source order must be the entity for legacy.players[i].
    for (i, p) in legacy.players.iter().enumerate() {
        let entity_id = by_source[i];
        let e = &v1.entities[entity_id as usize];
        assert_eq!(
            e.account.as_deref(),
            Some(p.account.as_str()),
            "source-order slot {i} resolves to the wrong entity"
        );
    }
    let _ = (&enc, &metrics);
}

#[test]
fn target_order_matches_the_legacy_ei_targets() {
    let (_enc, _metrics, legacy, v1) = fixture_report();

    let by_source: Vec<u32> = v1.source_order.targets().to_vec();
    assert_eq!(by_source.len(), legacy.ei_targets.len());

    for (i, t) in legacy.ei_targets.iter().enumerate() {
        let entity_id = by_source[i];
        let e = &v1.entities[entity_id as usize];
        let label = e.name.as_deref().or(e.character.as_deref());
        assert_eq!(label, Some(t.name.as_str()), "target slot {i} mismatched");
    }
}

#[test]
fn positions_round_trip() {
    let (_enc, _metrics, _legacy, v1) = fixture_report();
    for (i, &id) in v1.source_order.players().iter().enumerate() {
        assert_eq!(v1.source_order.player_position(id), Some(i));
    }
    for (i, &id) in v1.source_order.targets().iter().enumerate() {
        assert_eq!(v1.source_order.target_position(id), Some(i));
    }
}

#[test]
fn source_order_is_not_serialized() {
    let (_enc, _metrics, _legacy, v1) = fixture_report();
    let doc = serde_json::to_value(&v1).unwrap();
    assert!(
        doc.get("source_order").is_none(),
        "source_order is a reprojection aid, never wire data"
    );
    // And it must not have leaked into any other key either.
    let text = serde_json::to_string(&doc).unwrap();
    assert!(!text.contains("source_order"));
}
```

If `crates/axilog-schema/tests/common/mod.rs` does not already expose a
`fixture_report()` returning `(Encounter, Metrics, Report, ReportV1)`, add it
there — `v1_equivalence.rs` already builds all four, so lift that setup rather
than duplicating it.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --test v1_source_order`
Expected: FAIL to compile — `no field 'source_order' on type 'ReportV1'`.

- [ ] **Step 3: Write the implementation**

Create `crates/axilog-schema/src/v1/order.rs`:

```rust
//! The encounter's original agent order, preserved for reprojection.
//!
//! `entities[]` is sorted for human and diff legibility -- role, team,
//! subgroup, account, character, addr (see `build_entities`). That sort
//! deliberately discards the order agents appeared in the encounter.
//!
//! Some reprojections need that discarded order back. ei-json is the
//! motivating one: its `players[]` and `targets[]` are POSITIONAL arrays,
//! and `dpsTargets`/`statsTargets`/`targetDamageDist` are indexed by
//! position within them. Recomputing the order from `entities[]` is
//! impossible -- the information is gone, not merely rearranged.
//!
//! This is `#[serde(skip)]` and never reaches the wire. Consumers of the
//! 1.0 document join by `id`; ordering is not part of the contract. The
//! precedent is `PlayerOut::agent_addr`, a non-serialized join key added
//! for the same class of reason.

use serde::Serialize;
use std::collections::BTreeMap;

/// Entity ids in the encounter's original iteration order.
///
/// `players` mirrors `Encounter::players`; `targets` mirrors the sweep
/// `Report::ei_targets` uses (the `is_player` entries of
/// `Encounter::enemies`, in encounter order).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SourceOrder {
    players: Vec<u32>,
    targets: Vec<u32>,
    #[serde(skip)]
    player_pos: BTreeMap<u32, usize>,
    #[serde(skip)]
    target_pos: BTreeMap<u32, usize>,
}

impl SourceOrder {
    /// Build from the two id sequences, in encounter order.
    pub fn new(players: Vec<u32>, targets: Vec<u32>) -> Self {
        let player_pos = players.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let target_pos = targets.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        Self { players, targets, player_pos, target_pos }
    }

    /// Entity ids in `Encounter::players` order.
    pub fn players(&self) -> &[u32] {
        &self.players
    }

    /// Entity ids in `Report::ei_targets` order.
    pub fn targets(&self) -> &[u32] {
        &self.targets
    }

    /// This entity's slot in `players()`, if it is one.
    pub fn player_position(&self, entity_id: u32) -> Option<usize> {
        self.player_pos.get(&entity_id).copied()
    }

    /// This entity's slot in `targets()`, if it is one.
    pub fn target_position(&self, entity_id: u32) -> Option<usize> {
        self.target_pos.get(&entity_id).copied()
    }
}
```

In `crates/axilog-schema/src/v1/entities.rs`, change `build_entities` to also
return the source order. It already walks `enc.players` and `enc.enemies` to
build `Pending` rows, so capture the position as it goes rather than
re-walking:

```rust
pub fn build_entities(
    enc: &Encounter,
    metrics: &Metrics,
) -> (Vec<EntityOut>, EntityIndex, SourceOrder) {
    // ... existing body, unchanged, up to the point where `index` is built ...

    // Source order is derived AFTER ids are assigned, by re-walking the
    // encounter in its own order and resolving each agent through the
    // index. Deriving it from the index rather than tracking it through
    // the sort keeps the sort logic untouched -- and it means a lookup
    // miss is impossible by construction, since every encounter agent
    // produced a `Pending`.
    let players = enc
        .players
        .iter()
        .filter_map(|p| index.by_agent_addr(p.agent_addr))
        .collect::<Vec<_>>();
    debug_assert_eq!(
        players.len(),
        enc.players.len(),
        "every encounter player must resolve to an entity"
    );

    let targets = enc
        .enemies
        .iter()
        .filter(|e| e.is_player)
        .filter_map(|e| index.by_enemy_id(e.id))
        .collect::<Vec<_>>();

    (entities, index, SourceOrder::new(players, targets))
}
```

In `crates/axilog-schema/src/v1/mod.rs`, add the module, the field, and the
wiring:

```rust
pub mod order;
pub use order::SourceOrder;
```

On `ReportV1`, immediately after `entities`:

```rust
    /// The encounter's original agent order, for reprojections that need
    /// positional arrays. Never serialized -- see [`SourceOrder`].
    #[serde(skip)]
    pub source_order: SourceOrder,
```

In `build_report_v1`, change the destructuring and the struct literal:

```rust
    let (entities, index, source_order) = build_entities(enc, metrics);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --test v1_source_order`
Expected: PASS, 4 tests.

- [ ] **Step 5: Verify nothing else moved**

Run: `cargo test --workspace`
Expected: PASS. `source_order` is `#[serde(skip)]`, so `v1_shape.rs`'s key-set
golden and `v1_size.rs`'s measurements must be unchanged. If either moved, the
skip attribute is wrong.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-schema/src/v1/order.rs crates/axilog-schema/src/v1/entities.rs \
        crates/axilog-schema/src/v1/mod.rs crates/axilog-schema/tests/v1_source_order.rs \
        crates/axilog-schema/tests/common/
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(schema): carry the encounter's source order for reprojections"
```

---

### Task 2: `EiJoin` — the adapter's single ordering helper

The spec's ordering mitigation: "build the order reconstruction as one explicit, documented helper rather than ad-hoc per block." This is that helper. Every subsequent adapter task goes through it; none re-derives an order inline.

**Files:**
- Create: `crates/axilog-ei/src/join.rs`
- Modify: `crates/axilog-ei/src/lib.rs` (add `mod join;`)
- Test: `crates/axilog-ei/tests/join_orders.rs`

**Interfaces:**
- Consumes: `SourceOrder::{players, targets, player_position, target_position}`, `ReportV1::{entities, blocks}` (Task 1).
- Produces:
  ```rust
  pub(crate) struct EiJoin<'a> {
      report: &'a ReportV1,
  }
  impl<'a> EiJoin<'a> {
      pub fn new(report: &'a ReportV1) -> Self;
      pub fn players(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)>;
      pub fn targets(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)>;
      pub fn target_slot(&self, entity_id: u32) -> Option<usize>;
      pub fn entity(&self, entity_id: u32) -> Option<&'a EntityOut>;
      pub fn display_name(&self, entity_id: u32) -> &'a str;
  }
  ```

`display_name` is load-bearing for Tasks 9 and 10: the absorbed timelines key
by source entity id natively, and EI wants a character-name key. One function
owns that resolution so the name→id redesign has exactly one inverse.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-ei/tests/join_orders.rs`:

```rust
//! `EiJoin` must hand back exactly the orders the legacy report iterates,
//! because the ei-json goldens are positional.

mod common;
use common::{fixture_legacy_and_v1};

#[test]
fn player_iteration_matches_legacy_order() {
    let (legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);

    let seen: Vec<String> = join
        .players()
        .map(|(_, _, e)| e.account.clone().unwrap_or_default())
        .collect();
    let want: Vec<String> = legacy.players.iter().map(|p| p.account.clone()).collect();
    assert_eq!(seen, want);
}

#[test]
fn target_iteration_matches_legacy_order() {
    let (legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);

    let seen: Vec<String> = join
        .targets()
        .map(|(_, id, _)| join.display_name(id).to_string())
        .collect();
    let want: Vec<String> = legacy.ei_targets.iter().map(|t| t.name.clone()).collect();
    assert_eq!(seen, want);
}

#[test]
fn target_slot_is_the_inverse_of_target_iteration() {
    let (_legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);
    for (slot, id, _) in join.targets() {
        assert_eq!(join.target_slot(id), Some(slot));
    }
}

#[test]
fn display_name_prefers_character_then_name() {
    let (_legacy, v1) = fixture_legacy_and_v1();
    let join = axilog_ei::test_support::join(&v1);
    for e in &v1.entities {
        let got = join.display_name(e.id);
        let want = e.character.as_deref().or(e.name.as_deref()).unwrap_or("");
        assert_eq!(got, want, "entity {} resolved the wrong label", e.id);
    }
}
```

Expose the helper to integration tests via a `test_support` module in
`crates/axilog-ei/src/lib.rs` rather than making `EiJoin` fully public — it is
an implementation detail of the adapter:

```rust
/// Test-only accessors. Not part of the supported surface.
#[doc(hidden)]
pub mod test_support {
    pub fn join(report: &axilog_schema::v1::ReportV1) -> crate::join::EiJoin<'_> {
        crate::join::EiJoin::new(report)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-ei --test join_orders`
Expected: FAIL to compile — `no module 'join'`.

- [ ] **Step 3: Write the implementation**

Create `crates/axilog-ei/src/join.rs`:

```rust
//! Reconstructs Elite Insights' positional joins from the native 1.0
//! document.
//!
//! EI's shape is positional everywhere native's is id-keyed: `players[]`
//! and `targets[]` are arrays whose INDEX is the identity, and
//! `dpsTargets`/`statsTargets`/`targetDamageDist` are indexed by position
//! within `targets[]`. Native has no positions at all -- it has
//! `entities[]` sorted for legibility and blocks keyed by entity id.
//!
//! Every order reconstruction in this crate goes through here. That is a
//! deliberate constraint, not a convenience: ordering is the single
//! highest-risk part of the adapter re-point (a wrong order diffs every
//! golden at once), so it gets exactly one implementation to audit rather
//! than one per block.

use axilog_schema::v1::{EntityOut, ReportV1};

pub(crate) struct EiJoin<'a> {
    report: &'a ReportV1,
}

impl<'a> EiJoin<'a> {
    pub fn new(report: &'a ReportV1) -> Self {
        Self { report }
    }

    /// `(ei_index, entity_id, entity)` in EI `players[]` order.
    pub fn players(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)> + '_ {
        let report = self.report;
        report
            .source_order
            .players()
            .iter()
            .enumerate()
            .filter_map(move |(i, &id)| report.entities.get(id as usize).map(|e| (i, id, e)))
    }

    /// `(ei_index, entity_id, entity)` in EI `targets[]` order.
    pub fn targets(&self) -> impl Iterator<Item = (usize, u32, &'a EntityOut)> + '_ {
        let report = self.report;
        report
            .source_order
            .targets()
            .iter()
            .enumerate()
            .filter_map(move |(i, &id)| report.entities.get(id as usize).map(|e| (i, id, e)))
    }

    /// This entity's index in EI `targets[]`, for the arrays keyed by it.
    pub fn target_slot(&self, entity_id: u32) -> Option<usize> {
        self.report.source_order.target_position(entity_id)
    }

    pub fn entity(&self, entity_id: u32) -> Option<&'a EntityOut> {
        self.report.entities.get(entity_id as usize)
    }

    /// The label EI uses for this entity: a player's character name, an
    /// NPC's name, or `""` when neither is recorded.
    ///
    /// This is the inverse of the native shape's source-entity-id keying
    /// (see the boon-state and condition blocks). Native keys timelines by
    /// id precisely so two players sharing a character name cannot
    /// collide; EI's own shape cannot express that, so the collision
    /// reappears on THIS side of the boundary, exactly where EI already
    /// had it. That is faithful reprojection, not a regression.
    pub fn display_name(&self, entity_id: u32) -> &'a str {
        self.entity(entity_id)
            .and_then(|e| e.character.as_deref().or(e.name.as_deref()))
            .unwrap_or("")
    }
}
```

Add `mod join;` and the `test_support` module to `crates/axilog-ei/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-ei --test join_orders`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-ei/src/join.rs crates/axilog-ei/src/lib.rs crates/axilog-ei/tests/join_orders.rs crates/axilog-ei/tests/common/
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(ei): add the single ordering helper for the adapter re-point"
```

---

### Task 3: Re-point the document scalars and maps

Smallest adapter surface first: everything on `EiDoc` that is not `players[]`
or `targets[]`. `EiInputs` stays intact and untouched.

**Files:**
- Modify: `crates/axilog-ei/src/lib.rs` (`ei_doc` signature and the scalar/map fields)
- Test: existing goldens

**Interfaces:**
- Consumes: `EiJoin` (Task 2).
- Produces: `fn ei_doc<'a>(report: &'a ReportV1, legacy: &'a Report, inputs: &EiInputs<'a>) -> EiDoc<'a>` — a **transitional** three-argument form. Tasks 4 and 5 drain `legacy`; Tasks 6–12 drain `inputs`; Task 13 deletes both parameters.

The transitional signature is what makes this safe: each task moves one surface
off `legacy` and onto `report`, and the goldens verify the move in isolation.

- [ ] **Step 1: Change the signature and thread both sources**

In `crates/axilog-ei/src/lib.rs`:

```rust
fn ei_doc<'a>(
    report: &'a ReportV1,
    legacy: &'a Report,
    inputs: &EiInputs<'a>,
) -> EiDoc<'a> {
```

and at both public entry points, taking both for now:

```rust
pub fn to_ei_json(report: &ReportV1, legacy: &Report, inputs: &EiInputs<'_>) -> Value
pub fn write_ei_json<W: std::io::Write>(
    report: &ReportV1,
    legacy: &Report,
    inputs: &EiInputs<'_>,
    w: W,
) -> serde_json::Result<()>
```

Update the three callers (`crates/axilog-cli/src/main.rs`,
`crates/axilog-node/src/lib.rs`, `crates/axilog-py/src/lib.rs`) to build a
`ReportV1` before rendering ei-json. In `main.rs` the `ReportV1` currently
builds only inside the `Format::Json` arm — hoist it above the
`if format == Format::EiJson` block so both arms share one.

- [ ] **Step 2: Run the goldens to confirm the plumbing is inert**

Run: `cargo test --workspace`
Expected: PASS, every golden byte-identical. Nothing has changed source yet —
this step only proves the new argument threads through without disturbing
output.

- [ ] **Step 3: Move the scalars and maps onto `report`**

Convert these `EiDoc` fields to read from `report` instead of `legacy`:
`fight_name`, `duration_ms`, `recorded_by`, `wvw_map_data` (from
`report.encounter`); `buff_map`, `skill_map`, `damage_mod_map` (from
`report.catalogs`).

The catalogs need EI's `b<id>` / `s<id>` / `d<id>` key spelling, which
`report.catalogs` stores as bare integer ids. Write that prefixing once:

```rust
/// EI prefixes catalog keys by kind (`b1187`, `s5491`, `d64`); native
/// stores bare ids. One helper so the three maps cannot disagree.
fn ei_catalog_key(prefix: char, id: u32) -> String {
    format!("{prefix}{id}")
}
```

- [ ] **Step 4: Run the goldens**

Run: `cargo test --workspace`
Expected: PASS, byte-identical. A diff here means a catalog key, a scalar
spelling, or the `wvWMapData` shape moved — diagnose, do not re-bless.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-ei/src/lib.rs crates/axilog-cli/src/main.rs \
        crates/axilog-node/src/lib.rs crates/axilog-py/src/lib.rs
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "refactor(ei): read document scalars and catalogs from the 1.0 report"
```

---

### Task 4: Re-point `players[]`

The largest single surface — roughly 48 read-surface rows per the cutover
audit. Still no new data: every field here already exists in `ReportV1`,
because spec #1's `v1_equivalence.rs` proved the blocks agree with `legacy`
field-for-field.

**Files:**
- Modify: `crates/axilog-ei/src/lib.rs` (the player row builder)
- Test: existing goldens, `mstream_streaming_identity.rs`

**Interfaces:**
- Consumes: `EiJoin::players`, `EiJoin::target_slot`, `ReportV1::blocks`.

- [ ] **Step 1: Rewrite the player row builder over `EiJoin::players`**

Replace the `legacy.players.iter()` walk with `join.players()`, taking each
field from its block:

| EI field | Native source |
|---|---|
| `account`, `profession`, `elite_spec`, `group`, `teamID`, `notInSquad`, `hasCommanderTag`, `character_name` | `entities[id]` |
| `dpsAll[0].*` | `blocks.damage.by_entity[id]` |
| `statsAll[0].*` hit-quality family | `blocks.hit_stats.by_entity[id]` |
| `defenses[0].*` | `blocks.defenses.by_entity[id]` |
| `support[0].*` | `blocks.support.by_entity[id]` |
| `statsTargets[slot][0]` | `blocks.damage.by_entity[id].per_target[target_id]`, placed at `join.target_slot(target_id)` |
| `buffUptimes[]` | `blocks.boons.by_entity[id]` |
| `totalDamageDist[0][]` | `blocks.damage.by_entity[id].by_skill` |
| `rotation[]` | `blocks.rotation.by_entity[id]` |
| `damageModifiers[]`, `incomingDamageModifiers[]` | `blocks.damage_mods.by_entity[id]` |
| `extHealingStats.*`, `extBarrierStats.*` | `blocks.healing.by_entity[id]` |
| `damage1S[0]`, `damageTaken1S[0]`, `dpsTargets` | `blocks.series.by_entity[id]` |

**The `statsTargets` placement is the trap.** Native stores `per_target` as a
map keyed by target entity id; EI needs a dense array indexed by
`targets[]` position, with an empty row for targets this player never hit.
Build the dense vector explicitly:

```rust
// EI's `statsTargets` is DENSE over `targets[]` -- a player who never
// touched a target still has a (zeroed) row at that target's slot.
// Native's `per_target` is sparse, so the gaps must be materialized here
// rather than skipped, or every downstream index shifts.
let mut stats_targets = vec![ei_empty_target_stats(); join.targets().count()];
for (target_id, row) in &damage_row.per_target {
    if let Some(slot) = join.target_slot(*target_id) {
        stats_targets[slot] = ei_target_stats(row);
    }
}
```

- [ ] **Step 2: Run the goldens**

Run: `cargo test -p axilog-ei`
Expected: PASS, byte-identical.

Diagnostic guide if it fails:
- **Every golden diffs at the same array index** → ordering. Check `EiJoin::players` against Task 2's test.
- **One numeric field diffs by a last digit** → float text. Find the arithmetic that changed shape between the legacy struct and the block builder.
- **`statsTargets` shifts** → sparse/dense mismatch; the loop above is wrong or `target_slot` returned `None`.

- [ ] **Step 3: Confirm streaming still holds**

Run: `cargo test -p axilog-ei --test mstream_streaming_identity`
Expected: PASS. If this fails while the others pass, a block was materialized
into an owned `Value` where it used to be borrowed — find it and restore the
borrow.

- [ ] **Step 4: Run the full suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-ei/src/lib.rs
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "refactor(ei): read players[] from the 1.0 blocks"
```

---

### Task 5: Re-point `targets[]`

The spec's third named risk. Native's enemy rows are first-class entities;
EI's `targets[]` is a filtered, differently-ordered view, and MINSTID's instid
regroup means the mapping is not naive.

**Files:**
- Modify: `crates/axilog-ei/src/lib.rs` (the target row builder)
- Test: existing goldens **plus** the local-fixture calibrations

**Interfaces:**
- Consumes: `EiJoin::targets`, `ReportV1::blocks`.

- [ ] **Step 1: Rewrite the target row builder over `EiJoin::targets`**

| EI field | Native source |
|---|---|
| `id`, `name`, `teamID`, `enemyPlayer`, `isFake`, `instanceID`, `profession` | `entities[id]` |
| `dpsAll[0].damage` | `blocks.damage.by_entity[id]` |

**Two corrections found in execution.** First, `blocks.damage` had no enemy
rows at all — `build_damage` only walked `report.players`, so `dpsAll[0]` had
no native source and `EnemyOut::damage_out` had to be absorbed onto the block
first (same shape as Task 4's `breakbar_damage_dealt`). Second, the row that
said `combatReplayData ← blocks.replay.by_entity[id]` is struck: it contradicts
Task 13 Step 9, which derives `ei_replay` inside the adapter and never absorbs
it. The two are different structures — `blocks.replay` is the NATIVE replay
pass and carries no `end`/`orientations`/`dc`. `combatReplayData` stays on
`inputs` until Task 13.

`totalDamageDist`, `damage1S`/`powerDamage1S` and `buffs[]` stay on `inputs`
for now — they are Tasks 6, 8 and 10.

- [ ] **Step 2: Run the goldens on the committed fixture**

Run: `cargo test -p axilog-ei`
Expected: PASS, byte-identical.

- [ ] **Step 3: Run the local calibrations — mandatory for this task**

Run: `AXILOG_LOCAL_FIXTURES=<primary-checkout>/fixtures/local cargo test --workspace`
Expected: PASS, all 36.

**Not `AXILOG_LOCAL_FIXTURES=1`.** The variable is a PATH, not a boolean
(`crates/axilog-core/tests/common/mod.rs::local_fixture`) — `=1` resolves every
capture to `1/<name>`, every calibration skips, and the run goes green having
checked nothing. A worktree's own `fixtures/local/` is empty (the captures are
gitignored and never copied, so PII is not duplicated), so the var must point
at the primary checkout. Confirm it actually ran: the calibrating binaries take
seconds, not `0.00s`.

This is the task where the committed fixture is *not* sufficient evidence. It
has 32 enemy targets; the local capture has 56 real enemy instids, and the
enemy join is precisely what MINSTID made non-obvious. Do not commit this task
on the committed fixture alone.

- [ ] **Step 4: Commit**

```bash
git add crates/axilog-ei/src/lib.rs
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "refactor(ei): read targets[] from the 1.0 blocks"
```

---

## Absorption tasks (6–12)

Tasks 3–5 left `ei_doc` reading `report` for everything that already existed
natively, and `inputs` for the nine passes. Each task below moves one
destination block's worth of passes into `ReportV1` and deletes the
corresponding `EiInputs` field.

**Every absorption task has the same five-step spine.** It is written out in
full for Task 6; later tasks state only what differs, and the executor should
follow Task 6's spine for the rest.

1. Delete the `&& format == Format::EiJson` clause from the pass's compute
   condition in `crates/axilog-cli/src/main.rs`, and mirror it in the Node and
   Python callers.
2. Write the block builder in `crates/axilog-schema/src/v1/blocks/`, taking the
   pass output and the `EntityIndex`.
3. Wire it into `build_report_v1` with its coverage state.
4. Re-point the adapter's consumers of that data onto `report`, delete the
   `EiInputs` field, and confirm the goldens are byte-identical.
5. Extend `v1_size.rs` with the new block's measured payload, and commit.

---

### Task 6: `minions` and `health_percents`

Smallest and most self-contained. `MinionRollups = Vec<Vec<MinionGroup>>` is
positionally joined to `report.players`; `health_percents` is
`BTreeMap<u64, Vec<(u64, f64)>>` keyed by agent address.

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/minions.rs`
- Modify: `crates/axilog-schema/src/v1/blocks/activity.rs` (health percents into `series`)
- Modify: `crates/axilog-schema/src/v1/mod.rs`, `crates/axilog-cli/src/main.rs`, `crates/axilog-ei/src/lib.rs`
- Test: `crates/axilog-schema/tests/v1_shape.rs`, existing goldens

**Interfaces:**
- Produces:
  ```rust
  pub struct MinionsBlock { pub by_entity: BTreeMap<u32, Vec<MinionRow>> }
  pub struct MinionRow {
      pub species_id: u32,
      pub name: String,
      pub taken: Vec<MinionSkillTakenRow>,
  }
  pub fn build_minions(rollups: &MinionRollups, order: &SourceOrder, index: &EntityIndex) -> MinionsBlock;
  ```

- [ ] **Step 1: Un-gate the passes**

In `crates/axilog-cli/src/main.rs`, delete the format clause from both:

```rust
let minion_rollups = skill_damage
    .then(|| axilog_core::analysis::minions::build(&raw, &enc));
let health_percents = timeseries
    .then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
```

- [ ] **Step 2: Write the failing block test**

In `crates/axilog-schema/tests/v1_shape.rs`, add:

```rust
#[test]
fn minions_block_is_entity_keyed_and_matches_the_pass() {
    let (enc, metrics, legacy, v1) = fixture_report_all_gates();

    let block = v1.blocks.minions.as_ref().expect("--skill-damage was on");
    // The pass is positional over enc.players; the block is entity-keyed.
    // Every non-empty positional slot must appear under the right entity.
    let rollups = fixture_minion_rollups(&enc);
    for (i, groups) in rollups.iter().enumerate() {
        if groups.is_empty() {
            continue;
        }
        let entity_id = v1.source_order.players()[i];
        let rows = block
            .by_entity
            .get(&entity_id)
            .unwrap_or_else(|| panic!("player {i} lost its minions"));
        assert_eq!(rows.len(), groups.len());
        assert_eq!(rows[0].species_id, groups[0].species_id);
    }
    let _ = (&metrics, &legacy);
}

#[test]
fn minions_absent_when_the_gate_is_off() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_gates();
    assert!(v1.blocks.minions.is_none());
    assert_eq!(v1.coverage.get(BlockName::Minions), CoverageState::NotComputed);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p axilog-schema --test v1_shape minions`
Expected: FAIL — `blocks.minions` is `None` / field missing.

- [ ] **Step 4: Write the block and wire it**

Create `crates/axilog-schema/src/v1/blocks/minions.rs` with `build_minions` as
specified above, converting the positional outer `Vec` to an entity-keyed
`BTreeMap` via `order.players()[i]`. In `build_report_v1`, add the parameter,
build the block when the pass is `Some`, and set coverage with the existing
`computed(block.is_empty())` helper.

Health percents go into the existing `SeriesBlock` as a new per-entity field,
keyed through `index.by_agent_addr`.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p axilog-schema`
Expected: PASS.

- [ ] **Step 6: Re-point the adapter and delete the fields**

In `crates/axilog-ei/src/lib.rs`, read `minions[]` and `players[].healthPercents`
from `report.blocks`, then delete `minions` and `health_percents` from
`EiInputs` and from all three callers.

- [ ] **Step 7: Run the goldens**

Run: `cargo test --workspace`
Expected: PASS, byte-identical.

- [ ] **Step 8: Record the payload and commit**

Extend `crates/axilog-schema/tests/v1_size.rs` with the two new measurements.

```bash
git add crates/axilog-schema/ crates/axilog-ei/ crates/axilog-cli/ crates/axilog-node/ crates/axilog-py/
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(schema): absorb minions and health percents into native blocks"
```

**Execution notes (Task 6, done).** Five deviations from the steps above.
Tasks 7-12 follow this task's spine, so they inherit all five.

1. **`build_report_v1` takes a `Passes<'a>` struct, not more positional
   parameters.** Step 4 says "add the parameter". `damage_mods` was already
   a seventh positional parameter and this task adds two more; carried
   through Task 12 that is a fourteen-argument function whose call sites
   are a row of bare `None`s, rewritten across ~40 sites seven times. The
   struct absorbs `damage_mods` too, so those sites moved once, here.
   **Later tasks add a field and touch no call site.**

2. **Minion identity lives in `catalogs.minions`, not on the block's rows.**
   The interface sketch puts `species_id`/`name` on `MinionRow`, which trips
   `v1_shape.rs::no_block_inlines_a_human_readable_name` -- this format
   keeps names in `catalogs` and `entities[]` only. Minions are not tracked
   entities, so a new catalog keyed by a synthetic id is the only home that
   satisfies the invariant. Check any absorbed surface carrying a name
   against that test BEFORE writing the block.

3. **`series[].health_percents` is `Option<Vec<_>>`, not `Vec<_>`.** The
   pass keys its map off `HEALTH_UPDATE` events, so a player who emitted
   none is ABSENT from it, and ei-json omits `healthPercents` for that
   player rather than writing `[]`. A plain `Vec` collapses those two into
   an empty list and silently adds the key to every player. Caught by the
   byte comparison, not by any test.

4. **ei-json output is NOT byte-identical: `skillMap` gains 18 entries on
   the committed fixture (16 on the local capture).** Every added id was
   already referenced by `minions[].totalDamageTakenDist` and did NOT
   resolve in `skillMap` before, so this fixes dangling references rather
   than changing data -- verified additive, zero changed or removed values,
   across five gate combinations on both fixtures. It follows from the
   block registering its skill ids in the catalog, which the native format
   requires; **expect the same from any later task whose block references
   catalog ids.**

5. **The `v1_size.rs` ratio test now excludes `Passes`-supplied data.** It
   is a ratio against the legacy document, which has nowhere to put an
   absorbed pass, so including one compares a document carrying data
   against one that structurally cannot -- the ratio moved 0.800 -> 0.885
   with no encoding regression whatsoever. Excluding them keeps it honest.
   The measured ratio is nonetheless 0.837 against a 0.85 bound, because
   Tasks 4/5 absorbed onto ALWAYS-ON blocks and cannot be excluded. **When
   a later task trips this, do not widen the bound** -- see the comment
   there.

---

### Task 7: `enemy_dist` — enemy rows on `blocks.damage`

Follow Task 6's spine. What differs:

**This is not a new block.** `enemy_dist` is `BTreeMap<u64, Vec<SkillEntry>>`
keyed by enemy id, and its destination is `blocks.damage.by_entity[enemy_id].by_skill`
— the same field a player's own skill distribution already uses. The enemy is
just an entity.

**Files:** `crates/axilog-schema/src/v1/blocks/damage.rs`, plus the standard
`mod.rs` / `main.rs` / `lib.rs` trio.

**Gate:** `--skill-damage`. Delete `&& format == Format::EiJson` from the
`enemy_dist` binding in `main.rs`.

**Test to add** in `v1_shape.rs`:

```rust
#[test]
fn enemy_damage_rows_land_on_the_same_block_as_players() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let damage = &v1.blocks.damage;

    // At least one entity with an enemy role must carry by_skill rows,
    // in the SAME map players use -- not a parallel enemy structure.
    let enemy_with_skills = v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, Role::Enemy | Role::EnemyPlayer))
        .filter_map(|e| damage.by_entity.get(&e.id))
        .filter(|row| !row.by_skill.is_empty())
        .count();
    assert!(enemy_with_skills > 0, "enemy skill distributions did not land");
}
```

**Adapter:** `targets[].totalDamageDist[0][]` now reads
`report.blocks.damage.by_entity[id].by_skill`. Delete `EiInputs::enemy_dist`.

**Execution notes (Task 7, done).** Four things Tasks 8-12 inherit.

1. **`SkillRow` needed a SECOND hit-count field, not a shared one.** The
   sketch says the enemy rows go in the same `by_skill` map, which is right,
   but `hits` does not mean the same thing on both sides: the player pass
   counts CONTRIBUTING (`dmg > 0`) rows, and `build_enemy_dist` counts
   `HasHit` rows -- a superset including zero-damage connecting hits. The
   ei-json adapter already emits them under different keys (`hits` vs
   `connectedHits`), so folding both into one field would have forced the
   adapter to reinterpret it by the row's ROLE, which is exactly the
   coupling this container exists to remove. `SkillRow::hits` became
   `Option<u32>` (absent on enemy rows -- a `0` there is a denominator a
   consumer would divide `total` by) and `connected_hits: Option<u32>` was
   added. **Task 9 fills `connected_hits` for player rows** from
   `dist_outcomes`; it does not need to add the field.

2. **The DATA absorbed; the GATE did not.** An empty `by_skill` cannot
   distinguish "the flag was off" from "this enemy landed nothing", and the
   flagless render must omit `totalDamageDist` rather than emit `[[]]`. The
   adapter therefore reads the same `PlayerOut::skill_damage` presence its
   own player-side branch already uses -- deliberately the SAME signal, so
   the two sides cannot diverge. **Every remaining task hits this**: absorbed
   data does not carry its own gate, and the document has no native gate
   record. Task 13 has to add one (or accept that `legacy` outlives the
   other reads) -- do not solve it per-task.

3. **The enemy row set is a UNION of two sources.** `report.enemies` is the
   combat-participant roster ("dealt nonzero damage"); `enemy_dist` keys off
   any actor that produced a `HealthDamageEvent` and deliberately keeps
   legitimate all-zero rows. Iterating only the former drops a dist key's
   whole breakdown. The adapter's own `ei_targets` is a third, differently
   filtered roster -- so a block built from one list and read through
   another loses rows silently.

4. **ei-json: `skillMap` gains 12 entries on the committed fixture, zero
   other diffs.** Same mechanism as Task 6's note 4, with one new wrinkle:
   only 6 of the 12 are referenced by `targets[].totalDamageDist`. The other
   6 belong to enemies present in the NATIVE enemy roster but absent from
   the curated `ei_targets` the adapter renders -- so they are required
   references natively (the container's own orphan test proves it) and
   merely unreferenced in the narrower ei-json view. Verified: every target
   distribution is byte-identical, and the flagless and `--timeseries`-only
   renders are byte-identical outright.

---

### Task 8: `enemy_series` — enemy rows on `blocks.series`

Follow Task 6's spine. What differs:

`EnemySeries { enemy_id, damage: Vec<u64>, power_damage: Vec<u64> }` becomes
per-entity rows on the existing `SeriesBlock`, keyed via
`index.by_enemy_id(enemy_id)`.

**Gate:** `--timeseries`.

**Series envelope:** these are long integer arrays and must go through spec
#1's series envelope (`enc: "raw"` or `enc: "rle"`), not raw JSON arrays — the
envelope is what keeps `--all` payloads tractable. Reuse
`crates/axilog-schema/src/v1/series.rs`'s existing encoder; do not hand-roll.

**Adapter:** `targets[].damage1S` / `targets[].powerDamage1S` read from
`report.blocks.series`, decoding the envelope. Delete `EiInputs::enemy_series`.

---

### Task 9: `dist_outcomes` — outcome columns on existing skill rows

Follow Task 6's spine. What differs:

**No new rows at all** — this adds *fields* to skill rows that already exist.
`DistOutcomes { outgoing: Vec<SkillOutcomes>, taken: Vec<SkillOutcomes> }` is
positional over `report.players`, and each `SkillOutcomes` carries
`skill_id`, so it joins to existing `by_skill` entries by
`(entity_id, skill_id)`.

**Gate:** `--skill-damage`.

**Risk:** a skill id present in `dist_outcomes` but absent from the existing
`by_skill` map (or vice versa) means the two passes disagree about which
skills exist. Assert it rather than silently dropping:

```rust
#[test]
fn every_outcome_row_joins_an_existing_skill_row() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    for (entity_id, row) in &v1.blocks.damage.by_entity {
        for (skill_id, skill) in &row.by_skill {
            if skill.connected_hits.is_some() {
                assert!(
                    skill.hits > 0,
                    "entity {entity_id} skill {skill_id} has outcomes but no hits"
                );
            }
        }
    }
}
```

**Adapter:** `totalDamageDist[][].{connectedHits,downContribution,indirectDamage,glance,missed,evaded,blocked,invulned,interrupted}`
read from the enriched skill rows. Delete `EiInputs::dist_outcomes`.

---

**Execution notes (Task 9, done).** Five things, one of which corrects
this task's own sketch.

1. **The sketch's assertion is FALSE, and the merge is a UNION.**
   `every_outcome_row_joins_an_existing_skill_row` cannot hold: a skill
   whose every attempt was blocked deals no damage, so it never reaches
   `skill_damage`'s `dmg > 0` accumulator, while `dist_outcomes` counts
   exactly those rows and GW2EI emits them (`totalDamage: 0, hits: n`).
   Those pure-mitigation rows ARE the payload — axibridge's damage-
   mitigation table is what the pass exists to feed. Asserting the row
   sets match would have failed on the first real log; intersecting them
   would have deleted the data silently. The ei-json adapter had emitted
   this union since MEIGAP2, inside `dist_rows_ei_json`; Task 9 moved the
   union down into `merge_outcomes` so both readers see the same rows
   instead of the native container carrying half the set. The committed
   fixture has real mitigation-only rows, so
   `the_row_set_is_a_union_not_an_intersection` proves it rather than
   asserting into a vacuum.

2. **There are THREE hit counts, not two.** Task 7 split `hits`
   (contributing, `dmg > 0`) from `connected_hits` (`HasHit`). The outcome
   pass brings a third, GW2EI's own attempt count — every non-marker row,
   a superset of both — which is what real EI exports mean by `hits`. It
   landed as `SkillOutcomeCols::attempt_hits` rather than being folded
   into either existing field, for exactly Task 7's reason: one field
   reinterpreted by context is how a consumer divides by the wrong
   denominator. The nesting `hits <= connected_hits <= attempt_hits`
   holds and is pinned. **`interrupted` is NOT bounded by `attempt_hits`**
   — GW2EI excludes its Interrupt/KillingBlow/Downed markers from the
   attempt count while still counting the interrupt as an outcome, so an
   interrupt-heavy skill legitimately reports more interrupts than
   attempts (fixture: skill 77357, 2 attempts, 3 interrupts). The first
   draft of that test asserted the tidier invariant and caught only its
   own wrong assumption.

3. **Eight columns behind ONE `Option`, not eight `Option`s.** The
   counters come from a single pass over a single event list, so no
   "this one measured, that one didn't" combination can arise.
   `outcomes: Option<SkillOutcomeCols>` states that in the type and gives
   the adapter one presence check for the branch that emits all eight
   keys together. `connected_hits` deliberately stays on `SkillRow`: the
   enemy pass (Task 7) measures the same quantity, and duplicating it
   would leave two fields for one number with nothing forcing agreement.

4. **The GATE absorbed for free — no Task 13 dependency.** Unlike Task
   7's, this gate needs no native gate record, because the outcome pass
   and the distributions it annotates ride the same `--skill-damage`
   flag: `by_skill`'s presence already answers both questions. The merge
   runs INSIDE the `p.skill_damage` guard so the two can never be built
   off different conditions, and
   `outcome_columns_are_absent_entirely_when_the_gate_is_off` pins it.
   Task 8's zero-fill rule decided the one genuinely ambiguous field: a
   union row gets `hits: Some(0)`, because absence from the damage pass
   is a measurement (zero contributing events), not a gap.

5. **ei-json: 15 `skillMap` entries added, and nothing else changes.**
   Verified byte-for-byte across all four gate combos — the flagless and
   `--timeseries`-only renders are identical outright, and both
   `--skill-damage` renders differ in `skillMap` alone (`players[]` and
   `targets[]` are byte-identical, existing `skillMap` values unchanged).
   All 15 were **already referenced by the OLD document's own
   distributions**, so this is a dangling-reference fix, not new data:
   `merge_outcomes` calls `reference_skill` for the union rows it
   creates, which the adapter's private union never could. The CLI also
   dropped its `&& format == Format::EiJson` condition (Task 6's
   precedent) — these columns are a native surface now, so gating them
   on the output format would make the native JSON depend on which
   writer was asked for; both SDKs' native paths run the pass too.

---

### Task 10: `healing_detail` — detail arrays on `blocks.healing`

Follow Task 6's spine. What differs:

**Split gating.** `HealingDetail = Vec<PlayerHealingDetail>` powers families
gated on *different* flags: `healing1S` on `--timeseries`, the ally matrices
and the two `*Dist` arrays on `--skill-damage`. The pass runs when
`skill_damage || timeseries`, and **self-gates to `None`** on a log with no
healing extension.

**This is where `CoverageState::Unsupported` becomes reachable.** A `None` from
the pass on a gate-on run means the log has no healing extension — that is
`unsupported`, not `empty`:

```rust
let healing_detail_state = if !(skill_damage || timeseries) {
    CoverageState::NotComputed
} else if healing_detail.is_none() {
    // The pass self-gated: this log carries no healing extension, so the
    // question is unanswerable rather than answered with zero.
    CoverageState::Unsupported
} else {
    computed(block.is_empty())
};
```

**Adapter:** `extHealingStats.{outgoingHealingAllies,totalHealingDist,healing1S}`
and `extBarrierStats.{outgoingBarrierAllies,totalBarrierDist}` read from
`report.blocks.healing`. Delete `EiInputs::healing_detail`, and delete
`healing_series` and `healing_dist` outright — they only mirror flag state,
which block presence and coverage now carry.

**Execution notes (Task 10, done).**

1. **`healing1S` did NOT go on `blocks.healing`.** The sketch above put all
   five families there. It landed on
   `blocks.series.by_entity[].healing_1s` instead, because what a field
   belongs to in this format is its GRID and its GATE, not its subject
   matter. `healing_detail` buckets on `timeseries::ei_grid` — the CEILING
   grid, one bucket longer than `timeline.resolution_ms`'s floor grid on a
   partial-second log — which is exactly the grid the per-entity `damage`
   series beside it uses, and it rides `--timeseries` like every other
   per-second array. On `blocks.healing` it would have been the one field
   answering a different flag than its siblings, `coverage.healing` could
   not have described it, and nothing would have forced it onto either
   grid. `the_1s_graph_shares_the_grid_of_the_series_it_sits_beside` pins
   the length equality that now does.
2. **The split gate cost `Passes` two fields, not two `bool`s.**
   `healing_detail` and `healing_series` are both
   `Option<&HealingDetail>`, set from the same pass output by whichever
   flag the caller actually has. A borrow cannot be set without the data,
   which is the property the deleted `EiInputs` `bool`s lacked. This is
   the first pass in the program feeding two families under two flags, so
   it is also the first whose gate could NOT be answered by a single
   presence check — `each_family_rides_only_its_own_flag` walks all four
   combinations.
3. **`CoverageState::Unsupported` became reachable, but not the way the
   sketch derived it.** The sketch read it off `healing_detail.is_none()`
   on a gate-on run, which is correct but only available when a gate ran.
   `metrics.has_healing_extension` answers the same question on EVERY run,
   so the coverage line uses that and needs no gate branch at all. The
   `Empty` arm lost its only witness in the process (that WAS the
   no-extension case, per Finding 4), so `v1_equivalence.rs` gained a
   second witness — a zero-roster log — rather than letting `Empty` go
   back to being dead code.
4. **The ally matrix is keyed and sparse.** EI's `outgoingHealingAllies`
   is a dense N×N array of objects; `by_ally` is keyed by the ally's
   entity id, folds the barrier twin into the same row, and omits cells
   that are zero in all three quantities. The adapter re-densifies against
   `source_order.players()`. `the_ally_matrix_is_sparse_and_never_exceeds_
   the_scalar` fails if it ever stops being sparse, because that is when
   the payload argument behind the `--skill-damage` gate would evaporate.
5. **ei-json: 43 new `skillMap` entries, nothing else.** Flagless and
   `--timeseries`-only render byte-identical; both `--skill-damage` combos
   differ in `skillMap` alone — 43 added, 0 removed, 0 values changed, and
   all 43 were already referenced by the OLD document's own healing
   dists. Same dangling-reference fix Task 9 produced, same cause: the
   adapter's private side channel could not reach the catalog.
   `EiInputs` is now **5 fields, from the original 13**.

---

### Task 11: `activity` — the always-on/gated split in `blocks.replay`

Follow Task 6's spine. What differs, and it is the substantive part:

**`blocks.replay` is currently all-or-nothing**, gated on
`legacy.replay.is_some()`. `ActivityIntervals` (down/dead intervals plus
first/last-aware bounds) is computed by every caller unconditionally, and the
cutover audit marks `combatReplayData.{start,end,down,dead}` as always
emitted while `{positions,orientations,iconURL}` ride `--replay`.

Native must mirror that split:

```rust
pub struct ReplayBlock {
    /// Always present -- intervals are cheap and every caller computes
    /// them. A consumer reading down/dead must not have to pay for
    /// position tracks (see the cutover audit's E vs F columns).
    pub by_entity: BTreeMap<u32, ReplayIntervalsRow>,
    /// Present only under `--replay`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<BTreeMap<u32, ReplayTrackRow>>,
}
```

Coverage for `replay` becomes `Present` whenever intervals exist, regardless of
the position gate. Document that in the block's doc comment — a consumer
reading `coverage.replay == "present"` must not conclude positions are there.

**Test the split explicitly:**

```rust
#[test]
fn replay_intervals_survive_without_the_position_gate() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_no_replay_flag();
    let replay = &v1.blocks.replay;
    assert!(!replay.by_entity.is_empty(), "intervals are always-on");
    assert!(replay.tracks.is_none(), "positions ride --replay");
}
```

**Adapter:** `players[].activeTimes[0]` and `combatReplayData.{start,end,down,dead}`
read from `report.blocks.replay.by_entity`. Delete `EiInputs::activity`.

---

**Execution notes (Task 11, done):**

1. **`by_entity` is squad-only, and that is a roster fact, not a
   simplification.** `replay::build_replay` walks squad players AND
   enemy-player representatives; `build_activity_intervals` walks
   `enc.players` alone. So the sketch's `ReplayTrackRow` could not simply
   shed its intervals -- doing so would delete every enemy player's
   down/dead history. `ReplayTrack` keeps them; for a squad entity the two
   copies come from the same `build_intervals` call over the same folded
   addr set and a test asserts they agree. Extending the always-on pass
   over the enemy roster was the alternative and was rejected: it puts a
   per-enemy event scan on the path of every parse, for data nothing reads.

2. **`poll_ms`/`bounds` moved down into `tracks` with the samples.** Left at
   the block's top level they would serialize as a zero polling interval on
   every parse without `--replay` -- metadata describing tracks that are not
   there.

3. **`active_ms` is carried, not derived.** GW2EI subtracts dead time and
   NOT down time. A consumer deriving "active" from the other four fields
   under the intuitive reading under-reports every player who went down, so
   the field is emitted and `active_ms_subtracts_dead_time_but_not_down_time`
   pins it against a fixture that actually contains downs.

4. **This is the program's first key REMOVAL.** Nine keys moved under
   `tracks`; five new intervals keys appeared. The 1.x rules call a
   relocation breaking, so it is recorded explicitly in
   `docs/NATIVE-FORMAT.md`'s compatibility section rather than absorbed
   silently -- 1.0 has no external reader yet, which is what makes it
   payable now and not later.

5. **ei-json is byte-identical in all five gate combos** (none, `--replay`,
   `--skill-damage`, `--timeseries`, all three) -- the first task in the
   program with no ei-json delta at all. The native document, meanwhile,
   gains 42 interval rows and `coverage.replay: "present"` on a default
   parse that previously carried no replay block whatsoever.

---

### Task 12: `boon_states` and `target_conditions` — the name→id redesign

Follow Task 6's spine. This is the task the spec's PII section is about.

Both passes key by **character name**:

```rust
pub type PerSourceTimelines   = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;
pub type TargetConditionStates = BTreeMap<(u64, u32), BTreeMap<String, StateTimeline>>;
```

Native keys both by **source entity id**:

```rust
/// Per-source stack timelines, keyed by SOURCE ENTITY ID.
///
/// The pass keys by source character NAME, which has two defects native
/// will not carry: names are identity data that spec #1 confined to
/// `entities[]`, and two players sharing a character name collide into
/// one key. An entity id has neither problem -- and a non-player source
/// is an entity too, so the pass's `UNKNOWN_SOURCE` sentinel has no
/// native counterpart and is resolved at the boundary instead.
pub per_source: BTreeMap<u32, StateTimelineRow>,
```

**Gate:** `--timeseries` for both.

**The `UNKNOWN_SOURCE` sentinel needs a decision, not a translation.** The pass
emits it for a source that is not a recorded player. Natively, resolve the
source agent through `EntityIndex::by_agent_addr`; if it resolves, use that
entity id. If it genuinely does not resolve, drop the row and record a
structured warning — do **not** invent a sentinel entity id, which would put a
non-existent entity in a block keyed by `entities[]`.

- [ ] **PII assertion — the enforcement for this whole redesign**

Add to `crates/axilog-schema/tests/` (alongside spec #1's existing identity
tests):

```rust
#[test]
fn no_block_key_is_a_character_name() {
    let (_enc, _metrics, _legacy, v1) = fixture_report_all_gates();
    let doc = serde_json::to_value(&v1).unwrap();

    // Every character name in the roster.
    let names: Vec<String> = v1
        .entities
        .iter()
        .filter_map(|e| e.character.clone())
        .collect();
    assert!(!names.is_empty(), "fixture must have named players to be meaningful");

    // No name may appear as a KEY anywhere under `blocks`.
    fn walk(v: &serde_json::Value, names: &[String], path: &str) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, child) in m {
                    assert!(
                        !names.iter().any(|n| n == k),
                        "character name used as a key at {path}/{k}"
                    );
                    walk(child, names, &format!("{path}/{k}"));
                }
            }
            serde_json::Value::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    walk(child, names, &format!("{path}/{i}"));
                }
            }
            _ => {}
        }
    }
    walk(&doc["blocks"], &names, "blocks");
}
```

**Adapter:** `buffUptimes[].states`/`.statesPerSource` and
`targets[].buffs[].statesPerSource` re-key id→name through
`EiJoin::display_name` (Task 2). Delete `EiInputs::boon_states` and
`EiInputs::target_conditions`.

---

### Task 13: `modifiers.per_target` and the `EiInputs` funeral

Follow Task 6's spine for the absorption, then finish the job.

**The payload monster:** `DamageModifierResults::per_target` measured 854,077
bytes against the whole-fight arrays' 76,611 — an 11× multiplier. It rides
`--modifiers` and must go through the series envelope where its rows are
array-shaped.

**Files:** `crates/axilog-schema/src/v1/blocks/activity.rs` (damage-mods
block), plus the standard trio, plus deletions.

- [ ] **Step 1–8: absorb per Task 6's spine**

Adapter: `damageModifiersTarget` / `incomingDamageModifiersTarget` read from
`report.blocks.damage_mods`. Delete `EiInputs::modifiers`.

- [ ] **Step 9: Derive `ei_replay` inside the adapter**

`EiInputs::replay` is the last field and does **not** get absorbed — spec #1
decision 6's escape hatch keeps the GW2EI-pixel-grid resampling out of the
native document, since `blocks.replay` already carries the same track in world
units.

Create `crates/axilog-ei/src/replay_derive.rs` porting
`axilog_core::analysis::ei_replay::build_ei_replay_auto`'s resampling to read
`blocks.replay.tracks` instead of core's `Track`. The 300 ms grid, the
sentinel-bracketed `dc`, and the f32 text formatting must be preserved exactly
— the goldens pin all three.

- [ ] **Step 10: Delete `EiInputs`**

```rust
pub fn to_ei_json(report: &ReportV1) -> Value;
pub fn write_ei_json<W: std::io::Write>(report: &ReportV1, w: W) -> serde_json::Result<()>;
```

Delete the struct, the `legacy` parameter threaded in Task 3, and every
construction site. The compiler enumerates the rest.

- [ ] **Step 11: Prove the goldens still hold**

Run: `cargo test --workspace && AXILOG_LOCAL_FIXTURES=1 cargo test --workspace`
Expected: PASS. Then:

```bash
git diff --stat main -- crates/axilog-ei/tests/ fixtures/
```
Expected: **empty**. This is the moment the spec's central claim is proven —
ei-json is byte-identical to goldens calibrated against real EI exports, built
from nothing but the native document.

- [ ] **Step 12: Commit**

```bash
git add -A
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(ei): render ei-json from the native report alone; delete EiInputs"
```

---

### Task 14: `--all`, honest coverage, benchmarks, docs

**Files:**
- Modify: `crates/axilog-cli/src/main.rs` (`--all`), `crates/axilog-node/src/lib.rs`, `crates/axilog-py/src/lib.rs` (`everything`)
- Modify: `crates/axilog-node/types.d.ts`, `crates/axilog-py/*.pyi`
- Create: `crates/axilog-schema/tests/v1_coverage_states.rs`
- Modify: `docs/NATIVE-FORMAT.md`, `docs/EI-PARITY.md`, `docs/BENCHMARKS.md`, `docs/ROADMAP.md`

- [ ] **Step 1: Add `--all` / `everything`**

```rust
/// Compute every analysis pass this binary knows about.
///
/// Deliberately defined as "everything that exists in this version",
/// not as an enumerated flag list: a consumer that sets this keeps
/// getting complete documents as later milestones add passes. The first
/// axibridge cutover audit found 30 blank fields caused by exactly the
/// opposite -- a consumer's option list drifting from the parser's.
#[arg(long)]
all: bool,
```

Fold it in where the gates are read: `let skill_damage = skill_damage || all;`
and so on for each. Mirror as `everything: Option<bool>` in both SDKs.

- [ ] **Step 2: Write the coverage-state test**

Create `crates/axilog-schema/tests/v1_coverage_states.rs` asserting all three
states are reachable:

```rust
#[test]
fn gate_off_reports_not_computed() {
    let (_e, _m, _l, v1) = fixture_report_no_gates();
    assert_eq!(v1.coverage.get(BlockName::Minions), CoverageState::NotComputed);
}

#[test]
fn no_healing_extension_reports_unsupported() {
    // The committed fixture has no healing extension.
    let (_e, _m, _l, v1) = fixture_report_all_gates();
    assert_eq!(
        v1.coverage.get(BlockName::Healing),
        CoverageState::Unsupported,
        "a log without the healing extension cannot answer the question, \
         which is different from answering it with zero"
    );
}

#[test]
fn all_flag_leaves_nothing_not_computed() {
    let (_e, _m, _l, v1) = fixture_report_all_gates();
    for (block, state) in v1.coverage.iter() {
        assert_ne!(
            state, CoverageState::NotComputed,
            "--all must compute {block:?}"
        );
    }
}
```

If the committed fixture *does* carry a healing extension, the second test
needs a fixture that does not — check before writing, and if none exists, use
the smallest available log lacking the extension rather than synthesizing one.

- [ ] **Step 3: Measure and gate performance**

Run the criterion bench and capture peak RSS on the 583k-event log with `--all`:

```bash
cargo bench -p axilog-cli
/usr/bin/time -v cargo run --release -p axilog-cli -- parse <583k-log> --all --format json -o /tmp/all.json 2>&1 | grep "Maximum resident"
```

Compare against today's ei-json peak. **Ceiling: +10%.** If it fails, stop and
report — the spec names this as evidence for inverting the reprojection
direction, which is a separate spec, not a fix to improvise here.

- [ ] **Step 4: Update the docs**

- `docs/NATIVE-FORMAT.md` — document the new blocks; **replace the
  "`unsupported` is unreachable today" note** with the real three-state table
  (`not_computed` → pass the flag; `unsupported` → this log cannot answer it;
  `empty` → the answer is genuinely zero); document `--all`.
- `docs/EI-PARITY.md` — note that ei-json is now a pure projection of native.
- `docs/BENCHMARKS.md` — the measured `--all` timings, payload sizes per block,
  and the RSS result.
- `docs/ROADMAP.md` — record spec #1 as done (it is currently missing entirely),
  this spec as done, and the Phase B/C/D breakdown.

- [ ] **Step 5: Regenerate the key-set golden**

Run: `cargo test -p axilog-schema --test v1_shape`

The full key-set golden is generated with every gate on, so `--all` grows it
substantially. This is the **one** golden in this plan that legitimately
changes — it describes the native shape, not the EI projection. Review the diff
by eye: every added key must be one this plan introduced.

- [ ] **Step 6: Full verification**

```bash
cargo build --workspace 2>&1 | grep -c warning   # expect 0
cargo test --workspace
AXILOG_LOCAL_FIXTURES=1 cargo test --workspace
git diff --stat main -- crates/axilog-ei/tests/ fixtures/   # expect empty
```

- [ ] **Step 7: Commit**

```bash
git add -A
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat: add --all, honest coverage states, and the spec #2 docs"
```

---

## Self-review notes

**Spec coverage.** Each spec section maps to a task: block map → Tasks 6–13;
`activity` split → Task 11; name→id redesign → Task 12; `ei_replay` escape
hatch → Task 13 Step 9; adapter re-point → Tasks 3–5; memory trap → Task 14
Step 3; gating table → each absorption task's Step 1; `--all` → Task 14;
coverage states → Tasks 10 and 14; testing → the Global Constraints plus each
task's golden step; sequencing → task order; the three risks → Tasks 2, 4, 5.

**Known soft spots an executor should expect to resolve.** These are places
where the plan specifies the requirement precisely but the exact code depends
on structures too large to transcribe here (`crates/axilog-ei/src/lib.rs` is
3,300 lines; the block builders are eight files):

- Task 4's field-by-field mapping is given as a table, not as a full row
  builder. The executor must read the existing builder and move it field by
  field, running goldens between groups rather than in one jump.
- Tasks 7–12 state their spine by reference to Task 6. That is deliberate DRY,
  not omission — but it means Task 6 must be executed first and read carefully.
- The exact `SkillOutcomes` → `by_skill` field names in Task 9 depend on what
  spec #1's `SkillRow` already carries; check `blocks/damage.rs` before
  writing.

### Execution notes (Task 8, done)

Four notes Tasks 9–12 inherit.

1. **The gate CAN absorb — when the block zero-fills.** Task 7 had to leave the
   gate behind because an empty `by_skill` could not distinguish "flag off"
   from "this enemy landed nothing". Task 8 does not have that problem,
   because `build_series` gives EVERY `Enemy` a row — zero-filling the ones
   the pass skipped, which is the fill the ei-json adapter used to do itself.
   With every enemy filled, "no row" can only mean the flag was off, so the
   adapter branches on the native row's presence and reads no `EiInputs`
   `Option` at all. The general rule for the remaining tasks: **if a block
   can be made total over its roster, its gate absorbs with it; if absence is
   also a legitimate data value, the gate has to wait for Task 13.**

2. **The entity roster is broader than `report.enemies`.** 80 enemy-role
   entities against 49 `Enemy` records on the committed fixture — the extra
   rows are minions and gadgets promoted to entities. Zero-filling all 80
   would invent measurements for entities no pass ever considered, so the
   fill runs over `report.enemies` (which is also where `blocks.damage` draws
   the line). The adapter's gate inference stays sound because every rendered
   `source_order.targets()` entry is backed by an `Enemy` record — that is a
   stronger claim than "enemies are a superset", so it is pinned by its own
   test rather than left as reasoning.

3. **`EntitySeries` needed a field only one side populates**, exactly like
   Task 7's `SkillRow`. Enemies carry an outgoing power split
   (`powerDamage1S`); no pass computes one for players — `PlayerPerSecondOut`
   has `power_damage_taken` but no outgoing equivalent. So `power_damage` is
   `Option<SeriesOut>`, absent on player rows. Expect this shape again: the
   two sides of this format are measured by different passes, and a shared
   field with a zero default silently reports "measured zero" for "never
   measured".

4. **ei-json is byte-identical across all four gate combos** (flagless,
   `--timeseries`, `--skill-damage`, both) — unlike Tasks 6 and 7, which grew
   `skillMap`. Nothing grew here because a series references no catalog
   entry. A task whose absorbed data carries skill or buff ids should still
   expect additive catalog growth; one carrying only numbers should expect
   none, and a diff is then a real regression rather than a known effect.
