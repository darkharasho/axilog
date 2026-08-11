# Native Output Format 1.0 (Container) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the axilog native output format 1.0 container — a six-key document with a single `entities[]` roster, three referenced-ids-only catalogs, uniform id-keyed stat blocks, one RLE/raw series envelope, and an explicit `coverage` map — changing no computed number.

**Architecture:** The 1.0 shape is built as NEW modules in `axilog-schema` alongside the existing `Report`, from the same `(Encounter, Metrics, ...)` inputs. The legacy `Report` is demoted to an internal intermediate that `axilog-ei` keeps consuming unchanged, so every EI golden stays byte-identical by construction rather than by assertion. Consumers (CLI/HTML/SDKs) switch to the 1.0 document; the legacy shape stops being public output but stays in-tree for the adapter until spec #2 re-points it.

**Tech Stack:** Rust 2021 (workspace `rust-version = "1.74"`), `serde` + `serde_json`, `cargo test`, `criterion` (existing bench harness).

**Spec:** `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`

## Spec amendment (read before Task 1)

Spec Section 5 states the EI adapter reads from the new shape. This plan
**defers that** to spec #2. Rationale: re-pointing 3,331 lines of
`crates/axilog-ei/src/lib.rs` is the single riskiest change in the program,
and spec #1 is explicitly the one that changes no numbers. Spec #2 already
must touch the adapter to delete `EiInputs`, so the re-point lands there in
one motion.

The spec's actual guarantee is preserved and strengthened: `ei-json` output
is byte-identical because the adapter's inputs are literally untouched. The
reshape is instead proven by Task 10's equivalence test, which asserts the
1.0 blocks agree field-for-field with the legacy `Report` on the committed
fixture.

Cost, accepted: both shapes are built during a parse for one milestone.
Task 13 measures it.

Update the spec's Section 5 and its "Program context" list to match, as part
of Task 14.

## Global Constraints

- Schema version string is exactly `"1.0"`. Binary version stays
  `env!("CARGO_PKG_VERSION")` (workspace `Cargo.toml:6`, currently `0.3.2`).
  They are separate fields and never derived from each other.
- Absent optional fields are OMITTED from JSON, never serialized as `null`.
  Use `#[serde(skip_serializing_if = "Option::is_none")]` /
  `"Vec::is_empty"` / `"BTreeMap::is_empty"`. This matches the existing
  convention (`TickRateOut`, `TeamOut::guid`, `Report::replay`).
- No human-readable name appears outside `catalogs` or `entities`. Every
  block references `skill_id` / `buff_id` / `mod_id` as integers.
- Catalogs are scoped to REFERENCED ids only, both directions: every
  referenced id resolves, every catalog entry is referenced.
- All map keys in JSON are decimal strings with no prefix (no EI `"s"`/`"d"`).
- Every map is a `BTreeMap` (never `HashMap`) so serialization order is
  deterministic. Output must be byte-identical across runs of the same log.
- No number changes. If any existing test's expected value moves, STOP —
  that is a data-loss bug, not a golden to re-bless.
- Test runner: this machine limits parallelism. Use
  `cargo test -p <crate>` scoped to the crate under test; avoid
  `--workspace` except at the checkpoints that call for it.

## File Structure

`crates/axilog-schema/src/lib.rs` is 1,905 lines and gains substantially
more. It gets split by responsibility:

| File | Responsibility |
|---|---|
| `src/lib.rs` | Module declarations, re-exports, legacy `Report` + `build_report` (unchanged) |
| `src/v1/mod.rs` | `ReportV1` document struct + `build_report_v1` assembly |
| `src/v1/series.rs` | `SeriesOut` envelope + RLE/raw encoder |
| `src/v1/envelope.rs` | `AxilogMeta`, `Coverage`, `CoverageState`, `WarningOut` |
| `src/v1/entities.rs` | `Role`, `EntityOut`, deterministic sort, `EntityIndex` |
| `src/v1/catalogs.rs` | `Catalogs` + the three catalog entry types |
| `src/v1/blocks/mod.rs` | `Blocks` struct, `BlockName` enum, shared `ByEntity<T>` |
| `src/v1/blocks/damage.rs` | `damage` block |
| `src/v1/blocks/defense.rs` | `defenses`, `hit_stats`, `cc` blocks |
| `src/v1/blocks/support.rs` | `boons`, `support`, `contribution`, `healing` blocks |
| `src/v1/blocks/activity.rs` | `rotation`, `damage_mods`, `missiles`, `replay`, `series` blocks |

Tests:

| File | Responsibility |
|---|---|
| `crates/axilog-schema/src/v1/series.rs` (`mod tests`) | Series round-trip property tests |
| `crates/axilog-schema/tests/v1_shape.rs` | Full key-set golden, referential integrity, determinism |
| `crates/axilog-schema/tests/v1_equivalence.rs` | 1.0 blocks agree with legacy `Report` |
| `crates/axilog-core/tests/golden.rs` | PII structural assertion (extends existing file) |

---

### Task 1: Series encoding envelope

**Files:**
- Create: `crates/axilog-schema/src/v1/mod.rs`
- Create: `crates/axilog-schema/src/v1/series.rs`
- Modify: `crates/axilog-schema/src/lib.rs` (add `pub mod v1;` at the top, after the existing `use` block)

**Interfaces:**
- Consumes: nothing (first task, pure module).
- Produces: `axilog_schema::v1::series::SeriesOut`, with
  `SeriesOut::encode_u64(interval_ms: u64, values: &[u64]) -> SeriesOut`,
  `SeriesOut::encode_f64(interval_ms: u64, values: &[f64]) -> SeriesOut`,
  and `SeriesOut::decode_u64(&self) -> Vec<u64>` (test-facing, but public —
  the SDKs need a reference implementation to port).

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/series.rs` containing ONLY this test
module (no implementation yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_arbitrary_values() {
        // Deterministic pseudo-random cases -- no `rand` dependency, and a
        // fixed sequence keeps a failure reproducible.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for len in [0usize, 1, 2, 3, 17, 256, 1843] {
            for sparsity in [1u64, 4, 64, 4096] {
                let values: Vec<u64> =
                    (0..len).map(|_| if next() % sparsity == 0 { next() % 10_000 } else { 0 }).collect();
                let s = SeriesOut::encode_u64(1000, &values);
                assert_eq!(s.decode_u64(), values, "round-trip failed for len={len} sparsity={sparsity}");
                assert_eq!(s.len as usize, values.len(), "len must be the DECODED length");
            }
        }
    }

    #[test]
    fn picks_the_smaller_of_the_two_encodings() {
        // A long zero run must choose RLE.
        let zeros = vec![0u64; 400];
        let s = SeriesOut::encode_u64(1000, &zeros);
        assert_eq!(s.enc, "rle", "400 zeros must encode as a run");
        assert_eq!(s.data.len(), 1, "400 zeros is ONE run pair");

        // Alternating values make RLE strictly worse (every run is length 1,
        // costing a nested array per element), so raw must win.
        let alternating: Vec<u64> = (0..64).map(|i| i as u64).collect();
        let s = SeriesOut::encode_u64(1000, &alternating);
        assert_eq!(s.enc, "raw", "run-free data must encode as raw");
    }

    #[test]
    fn an_empty_series_is_raw_and_empty() {
        let s = SeriesOut::encode_u64(1000, &[]);
        assert_eq!(s.len, 0);
        assert_eq!(s.enc, "raw");
        assert!(s.data.is_empty());
        assert_eq!(s.decode_u64(), Vec::<u64>::new());
    }

    #[test]
    fn serializes_to_the_documented_json_shape() {
        // Ten zeros then a value: raw is 23 bytes, RLE is 14, so RLE wins
        // and we can pin the documented pair shape. (A shorter run like
        // [0,0,0,5] is 9 bytes raw vs 13 as RLE -- raw correctly wins there,
        // which is the encoder working, not a bug.)
        let s = SeriesOut::encode_u64(1000, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]);
        let v = serde_json::to_value(&s).expect("serializable");
        assert_eq!(v["interval_ms"], 1000);
        assert_eq!(v["len"], 11);
        assert_eq!(v["enc"], "rle");
        // RLE pairs are [value, run_length].
        assert_eq!(v["data"], serde_json::json!([[0, 10], [5, 1]]));
    }

    #[test]
    fn f64_values_round_trip_through_the_same_envelope() {
        let values = vec![0.0, 0.0, 1.5, 1.5, 1.5, 0.25];
        let s = SeriesOut::encode_f64(1000, &values);
        assert_eq!(s.len, 6);
        let decoded: Vec<f64> = s.data_f64();
        assert_eq!(decoded, values);
    }
}
```

Create `crates/axilog-schema/src/v1/mod.rs`:

```rust
//! The axilog native output format 1.0 container.
//!
//! Built alongside the legacy [`crate::Report`] from the same inputs. See
//! `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`.
pub mod series;
```

Add to the top of `crates/axilog-schema/src/lib.rs`, immediately after the
existing `use` statements:

```rust
pub mod v1;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::series`
Expected: FAIL — compile error, `cannot find type SeriesOut in this scope`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/axilog-schema/src/v1/series.rs`, above the test module:

```rust
use serde::Serialize;

/// One time series, in the format's single series envelope.
///
/// `enc` is `"raw"` (a plain array of values) or `"rle"` (an array of
/// `[value, run_length]` pairs), chosen per series by whichever serializes
/// smaller. `len` is the DECODED length in both cases, so a consumer can
/// allocate before decoding and validate after.
///
/// WvW per-second series are dominated by long zero runs -- a player idle
/// for 400 seconds is one pair rather than 400 characters. Base64 typed
/// arrays are deliberately NOT an option here: this format's calibration
/// workflow is diffing exports against GW2EI's, and opaque blobs destroy
/// that. A third `enc` value may be added later without breaking consumers
/// that already switch on the tag -- that is why the tag exists.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SeriesOut {
    pub interval_ms: u64,
    /// Decoded length, NOT `data.len()`.
    pub len: u64,
    pub enc: &'static str,
    pub data: Vec<serde_json::Value>,
}

impl SeriesOut {
    pub fn encode_u64(interval_ms: u64, values: &[u64]) -> Self {
        Self::encode(interval_ms, values, |v| serde_json::json!(v))
    }

    pub fn encode_f64(interval_ms: u64, values: &[f64]) -> Self {
        Self::encode(interval_ms, values, |v| serde_json::json!(v))
    }

    /// Shared encoder. `runs` are built structurally (equality on the
    /// serialized JSON value), so the same rule applies to integer and
    /// float series without duplicating the run detection.
    fn encode<T: Copy + PartialEq>(
        interval_ms: u64,
        values: &[T],
        to_json: fn(T) -> serde_json::Value,
    ) -> Self {
        let raw: Vec<serde_json::Value> = values.iter().copied().map(to_json).collect();

        let mut runs: Vec<serde_json::Value> = Vec::new();
        let mut i = 0usize;
        while i < values.len() {
            let mut j = i + 1;
            while j < values.len() && values[j] == values[i] {
                j += 1;
            }
            runs.push(serde_json::json!([to_json(values[i]), (j - i) as u64]));
            i = j;
        }

        // "Smaller" is measured on the actual serialized bytes -- the only
        // definition that matters for a wire format, and cheap at these
        // sizes. A tie goes to `raw`, which needs no decoder.
        let raw_len = serde_json::to_string(&raw).map(|s| s.len()).unwrap_or(usize::MAX);
        let rle_len = serde_json::to_string(&runs).map(|s| s.len()).unwrap_or(usize::MAX);
        if rle_len < raw_len {
            SeriesOut { interval_ms, len: values.len() as u64, enc: "rle", data: runs }
        } else {
            SeriesOut { interval_ms, len: values.len() as u64, enc: "raw", data: raw }
        }
    }

    /// Reference decoder for integer series. The SDKs port this; it is
    /// public so there is exactly one definition of the algorithm.
    pub fn decode_u64(&self) -> Vec<u64> {
        self.decode(|v| v.as_u64().unwrap_or_default())
    }

    /// Reference decoder for float series.
    pub fn data_f64(&self) -> Vec<f64> {
        self.decode(|v| v.as_f64().unwrap_or_default())
    }

    fn decode<T>(&self, from_json: fn(&serde_json::Value) -> T) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len as usize);
        match self.enc {
            "rle" => {
                for pair in &self.data {
                    let run = pair[1].as_u64().unwrap_or_default();
                    for _ in 0..run {
                        out.push(from_json(&pair[0]));
                    }
                }
            }
            _ => {
                for v in &self.data {
                    out.push(from_json(v));
                }
            }
        }
        out
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::series`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/v1/ crates/axilog-schema/src/lib.rs
git commit -m "feat(schema): add the 1.0 series envelope (raw/rle)"
```

---

### Task 2: Document envelope — meta, coverage, warnings

**Files:**
- Create: `crates/axilog-schema/src/v1/envelope.rs`
- Modify: `crates/axilog-schema/src/v1/mod.rs` (add `pub mod envelope;`)

**Interfaces:**
- Consumes: nothing.
- Produces: `AxilogMeta { schema: &'static str, version: String, generated_from: Option<String> }`;
  `CoverageState` enum with `Present | NotComputed | Empty | Unsupported`;
  `BlockName` enum (one variant per block, snake_case) with
  `BlockName::ALL` and `BlockName::as_str()`;
  `Coverage` with `Coverage::new() -> Coverage` (every `BlockName::ALL`
  entry `NotComputed`) and
  `Coverage::set(&mut self, block: BlockName, state: CoverageState)`;
  `WarningOut { code: String, severity: Severity, message: String, entity_id: Option<u32> }`;
  `Severity` enum with `Info | Warn | Error`.
  `BlockName::ALL` is the reserved block-name list — the single source of
  truth. There is no parallel `&'static str` array to drift from it.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/envelope.rs` with ONLY this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_starts_with_every_known_block_not_computed() {
        let c = Coverage::new();
        let v = serde_json::to_value(&c).expect("serializable");
        let obj = v.as_object().expect("coverage is an object");
        assert_eq!(obj.len(), BlockName::ALL.len(), "coverage must name every block");
        for block in BlockName::ALL {
            let name = block.as_str();
            assert_eq!(obj[name], "not_computed", "block {name} must default to not_computed");
        }
    }

    #[test]
    fn coverage_states_serialize_as_documented_snake_case() {
        let mut c = Coverage::new();
        c.set(BlockName::Damage, CoverageState::Present);
        c.set(BlockName::Series, CoverageState::Empty);
        c.set(BlockName::Replay, CoverageState::Unsupported);
        let v = serde_json::to_value(&c).expect("serializable");
        assert_eq!(v["damage"], "present");
        assert_eq!(v["series"], "empty");
        assert_eq!(v["replay"], "unsupported");
        assert_eq!(v["boons"], "not_computed");
    }

    #[test]
    fn meta_omits_generated_from_when_absent() {
        let m = AxilogMeta { schema: "1.0", version: "0.3.2".into(), generated_from: None };
        let v = serde_json::to_value(&m).expect("serializable");
        assert_eq!(v["schema"], "1.0");
        assert_eq!(v["version"], "0.3.2");
        assert!(v.get("generated_from").is_none(), "absent optional fields are omitted, never null");
    }

    #[test]
    fn a_warning_carries_a_machine_readable_code_and_optional_entity() {
        let w = WarningOut {
            code: "blank_account_agent".into(),
            severity: Severity::Info,
            message: "one agent has a blank account".into(),
            entity_id: Some(37),
        };
        let v = serde_json::to_value(&w).expect("serializable");
        assert_eq!(v["code"], "blank_account_agent");
        assert_eq!(v["severity"], "info");
        assert_eq!(v["entity_id"], 37);

        let w = WarningOut { entity_id: None, ..w };
        let v = serde_json::to_value(&w).expect("serializable");
        assert!(v.get("entity_id").is_none(), "entity_id is omitted when the warning is not per-entity");
    }
}
```

Add to `crates/axilog-schema/src/v1/mod.rs`:

```rust
pub mod envelope;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::envelope`
Expected: FAIL — compile error, `cannot find type Coverage in this scope`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/axilog-schema/src/v1/envelope.rs`:

```rust
use serde::Serialize;
use std::collections::BTreeMap;

/// Every block name the 1.0 schema defines. Fixed by spec #1 so spec #2
/// fills reserved slots rather than renegotiating the container; adding a
/// name is additive under the 1.x rules, renaming one is a major bump.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BlockName {
    Boons,
    Cc,
    Conditions,
    Contribution,
    Damage,
    DamageMods,
    Defenses,
    Healing,
    HitStats,
    Minions,
    Missiles,
    Replay,
    Rotation,
    Series,
    Support,
}

impl BlockName {
    /// The reserved block-name list -- the SINGLE source of truth. There is
    /// deliberately no parallel `&'static str` array: one would be free to
    /// drift from this, and a stringly-typed `Coverage::set` would let a
    /// typo insert an unknown key that only a debug assertion would catch
    /// (and this workspace does not enable debug assertions in release).
    pub const ALL: [BlockName; 15] = [
        BlockName::Boons,
        BlockName::Cc,
        BlockName::Conditions,
        BlockName::Contribution,
        BlockName::Damage,
        BlockName::DamageMods,
        BlockName::Defenses,
        BlockName::Healing,
        BlockName::HitStats,
        BlockName::Minions,
        BlockName::Missiles,
        BlockName::Replay,
        BlockName::Rotation,
        BlockName::Series,
        BlockName::Support,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BlockName::Boons => "boons",
            BlockName::Cc => "cc",
            BlockName::Conditions => "conditions",
            BlockName::Contribution => "contribution",
            BlockName::Damage => "damage",
            BlockName::DamageMods => "damage_mods",
            BlockName::Defenses => "defenses",
            BlockName::Healing => "healing",
            BlockName::HitStats => "hit_stats",
            BlockName::Minions => "minions",
            BlockName::Missiles => "missiles",
            BlockName::Replay => "replay",
            BlockName::Rotation => "rotation",
            BlockName::Series => "series",
            BlockName::Support => "support",
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AxilogMeta {
    /// The FORMAT contract version. Moves independently of `version`.
    pub schema: &'static str,
    /// The binary that produced this document (`CARGO_PKG_VERSION`).
    pub version: String,
    /// The input log's file NAME. Never a path -- paths are
    /// environment-specific and routinely carry a user name, which the PII
    /// policy scrubs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_from: Option<String>,
}

/// Why a block is or is not in `blocks`.
///
/// Without this a consumer cannot distinguish "absent because the compute
/// gate was off" from "absent because the log had nothing" -- an ambiguity
/// that turns a missing flag into silently-reported zeros.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// Computed, and `blocks` carries it.
    Present,
    /// The compute gate for it was off.
    NotComputed,
    /// Computed, and there was genuinely nothing to report.
    Empty,
    /// This log's era or encounter kind cannot produce it.
    Unsupported,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Coverage(BTreeMap<&'static str, CoverageState>);

impl Coverage {
    pub fn new() -> Self {
        Coverage(BlockName::ALL.iter().map(|b| (b.as_str(), CoverageState::NotComputed)).collect())
    }

    /// Takes a `BlockName`, not a string: a mistyped block is then a
    /// COMPILE error rather than a silent extra key.
    pub fn set(&mut self, block: BlockName, state: CoverageState) {
        self.0.insert(block.as_str(), state);
    }

    pub fn get(&self, block: &str) -> Option<CoverageState> {
        self.0.get(block).copied()
    }
}

impl Default for Coverage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// A structured, user-facing analysis warning.
///
/// The legacy `Report::warnings` is `Vec<String>`, which no consumer can act
/// on programmatically. `code` is a closed, documented set: adding a code is
/// additive, changing one's meaning is a break.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct WarningOut {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// The entity this warning is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::envelope`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/v1/
git commit -m "feat(schema): add the 1.0 document envelope (meta, coverage, warnings)"
```

---

### Task 3: `entities[]` roster

**Files:**
- Create: `crates/axilog-schema/src/v1/entities.rs`
- Modify: `crates/axilog-schema/src/v1/mod.rs` (add `pub mod entities;`)

**Interfaces:**
- Consumes: `axilog_core::model::{Encounter, Player, Enemy}`,
  `axilog_core::analysis::Metrics` (for `metrics.instance_ids: BTreeMap<u64, u16>`).
- Produces:
  `Role` enum `Squad | FriendlyPlayer | EnemyPlayer | Npc` (serialized snake_case);
  `EntityOut` (fields below);
  `build_entities(enc: &Encounter, metrics: &Metrics) -> (Vec<EntityOut>, EntityIndex)`;
  `EntityIndex` with `by_agent_addr(&self, addr: u64) -> Option<u32>` and
  `by_enemy_id(&self, enemy_id: u64) -> Option<u32>`.

Note for the implementer: `enc.players` are the squad/friendly players and
`enc.enemies` the rest, already friend/foe split by `axilog_core::wvw::apply`.
`Enemy::is_player` distinguishes enemy players from NPCs/gadgets.
`Enemy::profession.is_some()` is the "this is a real player" signal
(MENEMYPROF). `Player::in_squad` distinguishes `Squad` from `FriendlyPlayer`.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/entities.rs` with ONLY this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::model::{CommanderTag, Encounter, Enemy, Player};
    use axilog_core::analysis::Metrics;

    fn player(addr: u64, account: &str, in_squad: bool, subgroup: u8) -> Player {
        Player {
            agent_addr: addr,
            account: account.into(),
            character: format!("Char{addr}"),
            profession: "Guardian".into(),
            elite_spec: "Firebrand".into(),
            team: "red".into(),
            subgroup,
            in_squad,
            commander: false,
            marker: None,
            commander_tag: None,
            guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    fn enemy(id: u64, name: &str, is_player: bool, profession: Option<&str>) -> Enemy {
        Enemy {
            id,
            instid: id as u16,
            name: name.into(),
            team: "green".into(),
            is_player,
            marker: None,
            profession: profession.map(|s| s.into()),
            elite_spec: profession.map(|_| String::new()),
            agent_addrs: vec![id],
        }
    }

    fn encounter(players: Vec<Player>, enemies: Vec<Enemy>) -> Encounter {
        Encounter {
            kind: "wvw".into(),
            map: "Green Alpine Borderlands".into(),
            duration_ms: 1000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players,
            enemies,
            markers: vec![],
            tick_rate: None,
        }
    }

    #[test]
    fn assigns_dense_ids_in_deterministic_role_then_team_then_subgroup_order() {
        // Deliberately inserted out of order: an enemy first, then squad
        // players with subgroups descending. Ids must not depend on input
        // order, because they are the join key for every block and the
        // goldens are byte-exact diffs.
        let enc = encounter(
            vec![player(20, ":Bea.2", true, 3), player(10, ":Al.1", true, 1)],
            vec![enemy(90, "Gold Invader", true, Some("Reaper"))],
        );
        let (entities, _) = build_entities(&enc, &Metrics::default());

        let ids: Vec<u32> = entities.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![0, 1, 2], "ids are dense array indices from 0");

        let accounts: Vec<&str> = entities.iter().map(|e| e.account.as_deref().unwrap_or("")).collect();
        assert_eq!(accounts, vec![":Al.1", ":Bea.2", ""], "squad sorts before enemy, subgroup ascending");
        assert_eq!(entities[2].role, Role::EnemyPlayer);
    }

    #[test]
    fn role_separates_squad_from_non_squad_friendly_players() {
        let enc = encounter(
            vec![player(10, ":Al.1", true, 1), player(11, ":Pug.9", false, 0)],
            vec![],
        );
        let (entities, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities[0].role, Role::Squad);
        assert_eq!(entities[1].role, Role::FriendlyPlayer);
    }

    #[test]
    fn npcs_carry_a_name_and_no_account_or_profession() {
        let enc = encounter(vec![], vec![enemy(90, "Footman", false, None)]);
        let (entities, _) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities[0].role, Role::Npc);
        assert_eq!(entities[0].name.as_deref(), Some("Footman"));
        assert!(entities[0].account.is_none(), "an NPC has no account");
        assert!(entities[0].profession.is_none(), "an NPC has no profession");

        let v = serde_json::to_value(&entities[0]).expect("serializable");
        assert!(v.get("account").is_none(), "absent fields are omitted, never null");
        assert!(v.get("character").is_none());
    }

    #[test]
    fn player_entities_carry_account_and_character_not_name() {
        let enc = encounter(vec![player(10, ":Al.1", true, 1)], vec![]);
        let (entities, _) = build_entities(&enc, &Metrics::default());
        let v = serde_json::to_value(&entities[0]).expect("serializable");
        assert_eq!(v["account"], ":Al.1");
        assert_eq!(v["character"], "Char10");
        assert!(v.get("name").is_none(), "players use account/character, not name");
    }

    #[test]
    fn the_index_joins_both_agent_addrs_and_enemy_ids_to_entity_ids() {
        let enc = encounter(
            vec![player(10, ":Al.1", true, 1)],
            vec![enemy(90, "Gold Invader", true, Some("Reaper"))],
        );
        let (entities, index) = build_entities(&enc, &Metrics::default());
        assert_eq!(index.by_agent_addr(10), Some(entities[0].id));
        assert_eq!(index.by_enemy_id(90), Some(entities[1].id));
        assert_eq!(index.by_agent_addr(9999), None);
    }

    #[test]
    fn every_agent_addr_of_a_relogged_player_resolves_to_one_entity() {
        // arcdps issues a new addr per relog; `agent_addrs` holds them all
        // and `agent_addr` is the representative. A block keyed by any of
        // them must land on the same entity.
        let mut p = player(10, ":Al.1", true, 1);
        p.agent_addrs = vec![10, 11, 12];
        let enc = encounter(vec![p], vec![]);
        let (entities, index) = build_entities(&enc, &Metrics::default());
        assert_eq!(entities.len(), 1, "relogs are one person, not three");
        for addr in [10, 11, 12] {
            assert_eq!(index.by_agent_addr(addr), Some(0), "addr {addr} must resolve");
        }
    }
}
```

Add to `crates/axilog-schema/src/v1/mod.rs`:

```rust
pub mod entities;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::entities`
Expected: FAIL — compile error, `cannot find function build_entities in this scope`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/axilog-schema/src/v1/entities.rs`:

```rust
use axilog_core::analysis::Metrics;
use axilog_core::model::Encounter;
use serde::Serialize;
use std::collections::BTreeMap;

/// What an entity IS, replacing three overlapping signals the legacy shape
/// carried separately (`in_squad`, `is_player`, and membership in
/// `enemies[]` vs the `#[serde(skip)]` `ei_targets[]`).
///
/// Declaration order is the SORT order -- see `build_entities`.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Squad,
    /// Non-squad player on the squad's team -- GW2EI's
    /// `_nonSquadFriendlies`, which the legacy shape discarded entirely.
    FriendlyPlayer,
    EnemyPlayer,
    /// Every non-player enemy agent. `axilog_core::model::agent_kind`
    /// distinguishes gadgets from NPCs, but `model::Enemy` does not retain
    /// that, so a separate `Gadget` role would be unreachable. Adding one
    /// later is additive under the 1.x rules; see the spec's known
    /// simplifications.
    Npc,
}

/// One agent's IDENTITY. No statistics -- those live in `blocks`, keyed by
/// `id`.
///
/// This is the single place account and character names appear, which makes
/// the PII scrub a single pass rather than a hunt through nested structures.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct EntityOut {
    /// Dense index into `entities[]`, from 0. Stable WITHIN a report, not
    /// across reports -- join across logs on `account`.
    pub id: u32,
    pub role: Role,
    /// Players only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Players only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Non-player entities only -- they have neither account nor character.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Present exactly for player roles, preserving MENEMYPROF's property
    /// that presence is itself the "is this a real player" signal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    /// Empty string when the agent has no elite spec, or one this project
    /// cannot name. Never a numeric spec id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elite_spec: Option<String>,
    pub team: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subgroup: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander: Option<CommanderOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// The arcdps agent address. A documented attribute, not a secret --
    /// a consumer correlating against raw arcdps or another tool needs it,
    /// and hiding it is what forced the legacy EI side channel to exist.
    pub agent_addr: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instid: Option<u16>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct CommanderOut {
    pub variant: String,
    pub guid: String,
}

/// Join tables from the two id spaces the analysis layer uses onto entity
/// ids. Built once with the roster so no block has to re-derive it.
#[derive(Debug, Default, Clone)]
pub struct EntityIndex {
    by_addr: BTreeMap<u64, u32>,
    by_enemy: BTreeMap<u64, u32>,
}

impl EntityIndex {
    pub fn by_agent_addr(&self, addr: u64) -> Option<u32> {
        self.by_addr.get(&addr).copied()
    }
    pub fn by_enemy_id(&self, enemy_id: u64) -> Option<u32> {
        self.by_enemy.get(&enemy_id).copied()
    }
}

/// Build the roster and its join index.
///
/// The sort key is FULLY specified rather than left to encounter order,
/// because `id` is the join key for every block and the goldens are
/// byte-exact diffs: role, then team, then subgroup, then account, then
/// character/name, then `agent_addr` as the final tiebreak.
pub fn build_entities(enc: &Encounter, metrics: &Metrics) -> (Vec<EntityOut>, EntityIndex) {
    // (sort key, entity-without-id, every addr that should resolve to it,
    //  enemy id when it came from `enc.enemies`)
    struct Pending {
        key: (Role, String, u8, String, String, u64),
        entity: EntityOut,
        addrs: Vec<u64>,
        enemy_id: Option<u64>,
    }

    let mut pending: Vec<Pending> = Vec::with_capacity(enc.players.len() + enc.enemies.len());

    for p in &enc.players {
        let role = if p.in_squad { Role::Squad } else { Role::FriendlyPlayer };
        pending.push(Pending {
            key: (
                role,
                p.team.clone(),
                p.subgroup,
                p.account.clone(),
                p.character.clone(),
                p.agent_addr,
            ),
            entity: EntityOut {
                id: 0,
                role,
                account: Some(p.account.clone()),
                character: Some(p.character.clone()),
                name: None,
                profession: Some(p.profession.clone()),
                elite_spec: Some(p.elite_spec.clone()),
                team: p.team.clone(),
                subgroup: Some(p.subgroup),
                commander: p.commander_tag.as_ref().map(|c| CommanderOut {
                    variant: c.variant.clone(),
                    guid: c.guid.clone(),
                }),
                guild_id: p.guild_id.clone(),
                marker: p.marker.clone(),
                agent_addr: p.agent_addr,
                instid: metrics.instance_ids.get(&p.agent_addr).copied(),
            },
            addrs: p.agent_addrs.clone(),
            enemy_id: None,
        });
    }

    for e in &enc.enemies {
        // `is_player` is the friend/foe-split roster's player flag;
        // `profession.is_some()` (MENEMYPROF) agrees with it on every real
        // log and is the signal consumers use.
        let role = if e.is_player { Role::EnemyPlayer } else { Role::Npc };
        let is_player_role = matches!(role, Role::EnemyPlayer);
        pending.push(Pending {
            key: (role, e.team.clone(), 0, String::new(), e.name.clone(), e.id),
            entity: EntityOut {
                id: 0,
                role,
                account: None,
                character: None,
                name: (!is_player_role).then(|| e.name.clone()),
                profession: e.profession.clone(),
                elite_spec: e.elite_spec.clone(),
                team: e.team.clone(),
                subgroup: None,
                commander: None,
                guild_id: None,
                marker: e.marker.clone(),
                agent_addr: e.id,
                instid: metrics.instance_ids.get(&e.id).copied(),
            },
            addrs: e.agent_addrs.clone(),
            enemy_id: Some(e.id),
        });
    }

    pending.sort_by(|a, b| a.key.cmp(&b.key));

    let mut index = EntityIndex::default();
    let mut entities = Vec::with_capacity(pending.len());
    for (i, mut p) in pending.into_iter().enumerate() {
        let id = i as u32;
        p.entity.id = id;
        for addr in p.addrs {
            index.by_addr.insert(addr, id);
        }
        index.by_addr.insert(p.entity.agent_addr, id);
        if let Some(enemy_id) = p.enemy_id {
            index.by_enemy.insert(enemy_id, id);
        }
        entities.push(p.entity);
    }

    (entities, index)
}
```

Note: an enemy player keeps `name: None` because its `name` is the WvW rank
title, not an identity worth charting — see the `resolveEnemyClassLabel`
episode in axibridge. Its identity is `profession`/`elite_spec` plus
`instid`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::entities`
Expected: PASS, 6 tests.

If `Metrics::default()` does not exist, add `#[derive(Default)]` to
`axilog_core::analysis::Metrics` in `crates/axilog-core/src/analysis/mod.rs`
— every field is already `Default`-able (collections and numbers). Run
`cargo test -p axilog-core --lib` afterwards to confirm nothing regressed.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/v1/ crates/axilog-core/src/analysis/mod.rs
git commit -m "feat(schema): add the 1.0 entities roster with deterministic ids"
```

---

### Task 4: Catalogs

**Files:**
- Create: `crates/axilog-schema/src/v1/catalogs.rs`
- Modify: `crates/axilog-schema/src/v1/mod.rs` (add `pub mod catalogs;`)

**Interfaces:**
- Consumes: `axilog_core::analysis::Metrics` (`metrics.skill_map`),
  `axilog_core::analysis::buffs` (boon id/name/stacking tables),
  `axilog_core::analysis::damage_mods::DamageModifierResults`.
- Produces:
  `Catalogs { skills: BTreeMap<u32, SkillEntry>, buffs: BTreeMap<u32, BuffEntry>, damage_mods: BTreeMap<i32, DamageModEntry> }`;
  `CatalogBuilder` with `reference_skill(&mut self, id: u32)`,
  `reference_buff(&mut self, id: u32)`, `reference_damage_mod(&mut self, id: i32)`,
  and `finish(self, metrics: &Metrics, mods: Option<&DamageModifierResults>) -> Catalogs`.

The builder pattern is deliberate: blocks call `reference_*` as they emit
ids, and `finish` materializes exactly the referenced subset. That is what
makes the both-directions referential-integrity invariant true by
construction rather than by discipline.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/catalogs.rs` with ONLY this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axilog_core::analysis::Metrics;

    fn metrics_with_skills() -> Metrics {
        let mut m = Metrics::default();
        m.skill_map.insert(
            5491,
            axilog_core::analysis::skill_map::SkillMapEntry {
                name: "Symbol of Protection".into(),
                auto_attack: None,
                is_swap: false,
                can_crit: true,
            },
        );
        m.skill_map.insert(
            9999,
            axilog_core::analysis::skill_map::SkillMapEntry {
                name: "Never Referenced".into(),
                auto_attack: None,
                is_swap: false,
                can_crit: true,
            },
        );
        m
    }

    #[test]
    fn a_catalog_holds_only_referenced_ids() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        assert!(c.skills.contains_key(&5491), "referenced id must resolve");
        assert!(!c.skills.contains_key(&9999), "an unreferenced definition must not appear");
    }

    #[test]
    fn referencing_the_same_id_twice_yields_one_entry() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        assert_eq!(c.skills.len(), 1);
    }

    #[test]
    fn skill_keys_serialize_as_bare_decimal_strings_without_an_ei_prefix() {
        let mut b = CatalogBuilder::default();
        b.reference_skill(5491);
        let c = b.finish(&metrics_with_skills(), None);
        let v = serde_json::to_value(&c).expect("serializable");
        assert!(v["skills"].get("5491").is_some(), "keys are bare decimal ids");
        assert!(v["skills"].get("s5491").is_none(), "no EI 's' prefix");
        assert_eq!(v["skills"]["5491"]["name"], "Symbol of Protection");
        assert_eq!(v["skills"]["5491"]["can_crit"], true);
    }

    #[test]
    fn a_referenced_id_with_no_definition_still_resolves_to_an_entry() {
        // The referential-integrity invariant is "every referenced id
        // resolves". A skill the log table never named must therefore still
        // produce an entry -- with an honest placeholder name -- rather than
        // a dangling reference.
        let mut b = CatalogBuilder::default();
        b.reference_skill(424242);
        let c = b.finish(&metrics_with_skills(), None);
        let e = c.skills.get(&424242).expect("referenced id must resolve");
        assert_eq!(e.name, "Skill 424242");
    }

    #[test]
    fn buffs_carry_the_stacking_metadata_the_legacy_shape_had_nowhere_to_put() {
        let mut b = CatalogBuilder::default();
        b.reference_buff(740); // Might
        b.reference_buff(717); // Protection
        let c = b.finish(&Metrics::default(), None);

        let might = c.buffs.get(&740).expect("Might resolves");
        assert_eq!(might.name, "Might");
        assert_eq!(might.stacking, "intensity");
        assert_eq!(might.kind, "boon");

        let prot = c.buffs.get(&717).expect("Protection resolves");
        assert_eq!(prot.stacking, "duration");
    }
}
```

Add to `crates/axilog-schema/src/v1/mod.rs`:

```rust
pub mod catalogs;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::catalogs`
Expected: FAIL — compile error, `cannot find type CatalogBuilder in this scope`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/axilog-schema/src/v1/catalogs.rs`:

```rust
use axilog_core::analysis::buffs;
use axilog_core::analysis::damage_mods::DamageModifierResults;
use axilog_core::analysis::Metrics;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Definition metadata for every id any block references.
///
/// The rule that makes this pay: no human-readable name appears outside
/// `catalogs` or `entities`. Every block references integers, so a skill
/// name appears once per document instead of once per player per target per
/// distribution row.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct Catalogs {
    pub skills: BTreeMap<u32, SkillEntry>,
    pub buffs: BTreeMap<u32, BuffEntry>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub damage_mods: BTreeMap<i32, DamageModEntry>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SkillEntry {
    pub name: String,
    pub is_swap: bool,
    pub can_crit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_attack: Option<bool>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct BuffEntry {
    pub name: String,
    /// `"boon"`, `"condition"`, or `"effect"` -- GW2's own taxonomy.
    /// arcdps does not distinguish them structurally, so this field carries
    /// the distinction and one catalog serves all three.
    ///
    /// Membership in `condition_catalog::CONDITION_BUFFS` decides
    /// `"condition"`, NOT whether the condition deals damage: eight of the
    /// fourteen (Blind, Crippled, Chilled, Immobile, Weakness, Fear, Slow,
    /// Taunt) are non-damaging and are still conditions. Auras and forms
    /// (Frost Aura, Death Shroud) are `"effect"`; calling them boons would
    /// simply be false.
    pub kind: &'static str,
    /// `"intensity"` or `"duration"`. Sourced from
    /// `condition_catalog::CONDITION_BUFFS` for conditions -- the
    /// damage-modifier catalog's `buff_stack` table is a 91-entry SUBSET
    /// scoped to that catalog's needs and holds only one condition, so
    /// reading stacking from it silently mislabels five common conditions.
    pub stacking: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stacks: Option<u32>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DamageModEntry {
    pub name: String,
    pub kind: String,
    pub approximate: bool,
}

/// Accumulates referenced ids as blocks emit them, then materializes
/// exactly that subset. This is what makes "every catalog entry is
/// referenced" true by construction.
#[derive(Debug, Default, Clone)]
pub struct CatalogBuilder {
    skills: BTreeSet<u32>,
    buffs: BTreeSet<u32>,
    damage_mods: BTreeSet<i32>,
}

impl CatalogBuilder {
    pub fn reference_skill(&mut self, id: u32) {
        self.skills.insert(id);
    }
    pub fn reference_buff(&mut self, id: u32) {
        self.buffs.insert(id);
    }
    pub fn reference_damage_mod(&mut self, id: i32) {
        self.damage_mods.insert(id);
    }

    pub fn finish(self, metrics: &Metrics, mods: Option<&DamageModifierResults>) -> Catalogs {
        let skills = self
            .skills
            .into_iter()
            .map(|id| {
                let entry = metrics.skill_map.get(&id);
                (
                    id,
                    SkillEntry {
                        // A referenced id ALWAYS resolves, even when the log
                        // table never named it -- a dangling reference would
                        // break the invariant the integrity test asserts.
                        name: entry
                            .map(|e| e.name.clone())
                            .unwrap_or_else(|| format!("Skill {id}")),
                        is_swap: entry.map(|e| e.is_swap).unwrap_or(false),
                        can_crit: entry.map(|e| e.can_crit).unwrap_or(true),
                        auto_attack: entry.and_then(|e| e.auto_attack),
                    },
                )
            })
            .collect();

        let buffs = self
            .buffs
            .into_iter()
            .map(|id| {
                let is_intensity = buffs::is_intensity_stacking(id);
                (
                    id,
                    BuffEntry {
                        name: buffs::buff_name(id).unwrap_or_default().to_string(),
                        kind: if buffs::is_condition(id) { "condition" } else { "boon" },
                        stacking: if is_intensity { "intensity" } else { "duration" },
                        max_stacks: buffs::max_stacks(id),
                    },
                )
            })
            .collect();

        let damage_mods = match mods {
            None => BTreeMap::new(),
            Some(m) => self
                .damage_mods
                .into_iter()
                .filter_map(|id| {
                    // A referenced id ALWAYS resolves, mirroring the skills
                    // path above. GW2EI builds `damageModMap` from inside the
                    // same loop that writes the rows, so a dangling reference
                    // is unrepresentable there; match that guarantee rather
                    // than silently dropping the row.
                    Some(match m.descriptor(id) {
                        Some(d) => (
                            id,
                            DamageModEntry {
                                name: d.name.clone(),
                                kind: d.kind.clone(),
                                approximate: d.approximate,
                            },
                        ),
                        None => (
                            id,
                            DamageModEntry {
                                name: format!("Damage modifier {id}"),
                                kind: "unknown".into(),
                                approximate: false,
                            },
                        ),
                    })
                })
                .collect(),
        };

        Catalogs { skills, buffs, damage_mods }
    }
}
```

Implementer note: `buffs::buff_name`, `buffs::is_intensity_stacking`,
`buffs::is_condition`, `buffs::max_stacks`, and
`DamageModifierResults::descriptor` may not exist under those exact names.
Before writing this file, run:

```bash
grep -n 'pub fn ' crates/axilog-core/src/analysis/buffs/mod.rs | head -40
grep -n 'BOON_IDS\|pub fn ' crates/axilog-core/src/analysis/damage_mods/mod.rs | head -40
```

and use the real accessors. If a lookup genuinely does not exist (e.g. no
`is_condition`), add it to `axilog-core` in this task with its own unit
test — do NOT inline a duplicate table in `axilog-schema`, which would give
the project two sources of truth for buff metadata.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::catalogs`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/v1/ crates/axilog-core/src/analysis/
git commit -m "feat(schema): add referenced-ids-only catalogs for the 1.0 shape"
```

---

### Task 5: Block scaffolding and the `damage` block

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/mod.rs`
- Create: `crates/axilog-schema/src/v1/blocks/damage.rs`
- Modify: `crates/axilog-schema/src/v1/mod.rs` (add `pub mod blocks;`)

**Interfaces:**
- Consumes: `EntityIndex` (Task 3), `CatalogBuilder` (Task 4),
  `axilog_core::analysis::Metrics`, legacy `crate::PlayerOut`.
- Produces:
  `ByEntity<T>(BTreeMap<u32, T>)` with `insert(&mut self, entity_id: u32, value: T)` and `is_empty(&self)`;
  `DamageBlock { squad: DamageSquad, by_entity: ByEntity<DamageEntity> }`;
  `build_damage(report: &crate::Report, index: &EntityIndex, cats: &mut CatalogBuilder) -> DamageBlock`.

The builder reads the LEGACY `crate::Report` rather than `Metrics` directly.
That is deliberate: it makes the 1.0 shape a pure reprojection of a
structure the EI goldens already pin, so Task 10's equivalence test is a
tight loop and any divergence is a reprojection bug rather than a
recomputation bug.

- [ ] **Step 1: Give `PlayerOut` a non-serialized `agent_addr`**

Every block builder joins on it, and `PlayerOut` currently carries only
`account`/`character`. In `crates/axilog-schema/src/lib.rs`, add to
`PlayerOut` (immediately above `pub damage: DamageOut`):

```rust
    /// The player's representative arcdps agent addr. `#[serde(skip)]`:
    /// not part of the legacy JSON (which would be a breaking change to a
    /// shape the EI goldens pin), but every 1.0 block builder joins on it.
    /// Promoted to a real serialized field on the 1.0 `EntityOut`.
    #[serde(skip)]
    pub agent_addr: u64,
```

and populate it in `build_report`'s player loop with `agent_addr: p.agent_addr,`.

Run: `cargo test -p axilog-core --test golden && cargo test -p axilog-ei`
Expected: PASS with no golden re-blessed. If any expected value moved, the
`#[serde(skip)]` is missing — fix it, do not update the golden.

- [ ] **Step 2: Write the failing test**

Create `crates/axilog-schema/src/v1/blocks/damage.rs` with ONLY this test
module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_per_target_by_entity_id_not_by_array_position() {
        // The identity/statistics split's payoff: an enemy player's own row
        // and the damage dealt TO them are the same integer. Impossible in
        // the legacy shape, where enemy statistics were `#[serde(skip)]`.
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);

        let squad_entity = index.by_agent_addr(1).expect("squad player resolves");
        let enemy_entity = index.by_enemy_id(9).expect("enemy resolves");

        let row = block.by_entity.get(squad_entity).expect("squad player has a damage row");
        assert!(
            row.per_target.contains_key(&enemy_entity),
            "per_target is keyed by ENTITY id, so it joins to the enemy's own row"
        );
    }

    #[test]
    fn squad_total_matches_the_legacy_report() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);
        let expected: u64 = report.players.iter().map(|p| p.damage.total).sum();
        assert_eq!(block.squad.total, expected, "no number may change in this spec");
    }

    #[test]
    fn skill_rows_reference_ids_and_register_them_in_the_catalog() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats);

        let squad_entity = index.by_agent_addr(1).expect("squad player resolves");
        let row = block.by_entity.get(squad_entity).expect("row");
        for skill_id in row.by_skill.keys() {
            // No name anywhere in the block -- names live in catalogs only.
            let v = serde_json::to_value(&row.by_skill[skill_id]).expect("serializable");
            assert!(v.get("name").is_none(), "a block must never inline a skill name");
        }

        let built = cats.finish(&Default::default(), None);
        for skill_id in row.by_skill.keys() {
            assert!(
                built.skills.contains_key(skill_id),
                "every referenced skill id must resolve in the catalog"
            );
        }
    }

    #[test]
    fn an_empty_block_serializes_as_an_empty_map_not_null() {
        let block = DamageBlock::default();
        let v = serde_json::to_value(&block).expect("serializable");
        assert_eq!(v["by_entity"], serde_json::json!({}));
    }
}
```

Create `crates/axilog-schema/src/v1/blocks/mod.rs`:

```rust
//! Statistic blocks. Every block is an aggregate slot plus an entity-keyed
//! map, so a consumer learns the access pattern once.
use serde::Serialize;
use std::collections::BTreeMap;

pub mod damage;

/// The uniform entity-keyed map every block uses.
///
/// Keys serialize as decimal strings (`serde_json`'s integer-key
/// stringification), matching the catalogs.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ByEntity<T>(pub BTreeMap<u32, T>);

impl<T> Default for ByEntity<T> {
    fn default() -> Self {
        ByEntity(BTreeMap::new())
    }
}

impl<T> ByEntity<T> {
    pub fn insert(&mut self, entity_id: u32, value: T) {
        self.0.insert(entity_id, value);
    }
    pub fn get(&self, entity_id: u32) -> Option<&T> {
        self.0.get(&entity_id)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use crate::v1::entities::{build_entities, EntityIndex};

    /// The shared fixture every block test builds on: the committed
    /// anonymized WvW log, run through the real pipeline. Using the real
    /// fixture rather than hand-built structs is what makes these tests
    /// catch reprojection bugs on realistic shapes (sparse per-target maps,
    /// relogged players, NPC enemies).
    pub fn fixture_report() -> (crate::Report, EntityIndex) {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/wvw-small.anon.zevtc"
        ))
        .expect("read committed fixture");
        let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
        let enc = axilog_core::model::resolve(&raw);
        let metrics = axilog_core::analysis::analyze(&enc, &raw);
        let report =
            crate::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
        let (_, index) = build_entities(&enc, &metrics);
        (report, index)
    }
}
```

Add to `crates/axilog-schema/src/v1/mod.rs`:

```rust
pub mod blocks;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::blocks::damage`
Expected: FAIL — compile error, `cannot find function build_damage in this scope`.

- [ ] **Step 4: Write minimal implementation**

Prepend to `crates/axilog-schema/src/v1/blocks/damage.rs`:

```rust
use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageBlock {
    pub squad: DamageSquad,
    pub by_entity: ByEntity<DamageEntity>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageSquad {
    pub total: u64,
    pub dps: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageEntity {
    pub total: u64,
    pub dps: f64,
    pub taken: u64,
    /// Keyed by the TARGET's entity id -- so it joins directly to that
    /// entity's own row. Sparse; omitted when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, PerTarget>,
    /// Keyed by skill id. Present only when the per-skill compute gate was
    /// on; omitted otherwise.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill: BTreeMap<u32, SkillRow>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTarget {
    pub total: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SkillRow {
    pub total: u64,
    pub hits: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
}

pub fn build_damage(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> DamageBlock {
    let mut by_entity = ByEntity::default();
    let mut squad_total = 0u64;
    let mut squad_dps = 0f64;

    for p in &report.players {
        let Some(entity_id) = index.by_agent_addr(p.agent_addr) else {
            continue;
        };
        squad_total += p.damage.total;
        squad_dps += p.damage.dps;

        let per_target = p
            .damage
            .per_enemy
            .iter()
            .filter_map(|pe| index.by_enemy_id(pe.enemy_id).map(|tid| (tid, PerTarget { total: pe.total })))
            .collect();

        let mut by_skill = BTreeMap::new();
        if let Some(skills) = p.skills.as_ref() {
            for e in skills {
                cats.reference_skill(e.id);
                by_skill.insert(
                    e.id,
                    SkillRow { total: e.total, hits: e.hits, min: e.min, max: e.max },
                );
            }
        }

        by_entity.insert(
            entity_id,
            DamageEntity {
                total: p.damage.total,
                dps: p.damage.dps,
                taken: p.damage_taken,
                per_target,
                by_skill,
            },
        );
    }

    DamageBlock { squad: DamageSquad { total: squad_total, dps: squad_dps }, by_entity }
}
```

Implementer notes:

1. `PlayerOut`'s per-skill field may be named differently (check
   `grep -n 'pub skills\|SkillEntryOut' crates/axilog-schema/src/lib.rs`).
   Use the real field and the real `SkillEntryOut` field names for
   `total`/`hits`/`min`/`max`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::blocks::damage`
Expected: PASS, 4 tests.

- [ ] **Step 6: Verify the legacy surface did not move**

Run: `cargo test -p axilog-core --test golden && cargo test -p axilog-ei`
Expected: PASS, no golden re-blessed. If any expected value changed, the
`PlayerOut` field addition leaked into serialization — fix the
`#[serde(skip)]`, do not update the golden.

- [ ] **Step 7: Commit**

```bash
git add crates/axilog-schema/src/
git commit -m "feat(schema): add the 1.0 damage block and block scaffolding"
```

---

### Task 6: `defenses`, `hit_stats`, and `cc` blocks

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/defense.rs`
- Modify: `crates/axilog-schema/src/v1/blocks/mod.rs` (add `pub mod defense;`)

**Interfaces:**
- Consumes: `crate::Report` (legacy `DefensesOut`, `HitStatsOut`, `CcOut`), `EntityIndex`.
- Produces:
  `DefensesBlock { by_entity: ByEntity<DefensesEntity> }`,
  `HitStatsBlock { by_entity: ByEntity<HitStatsEntity> }`,
  `CcBlock { squad: CcSquad, by_entity: ByEntity<CcEntity> }`;
  `build_defenses(...)`, `build_hit_stats(...)`, `build_cc(...)`, each with the
  signature `(&crate::Report, &EntityIndex) -> <Block>`.

- [ ] **Step 1: Extract the two-player test helper**

Task 5 built its non-squad-filter test with a hand-constructed `Encounter`
inline in `damage.rs`. Tasks 6, 7 and 8 all need the same fixture, and every
fixture-based test is blind to this class of bug (every player in
`fixtures/wvw-small.anon.zevtc` is in-squad). Promote it to a shared helper
in `crates/axilog-schema/src/v1/blocks/mod.rs`'s `tests_support` module,
beside `fixture_report`:

```rust
    /// A minimal two-player report: `players[0]` is IN squad, `players[1]`
    /// is a non-squad friendly. Their agent addrs are 1 and 2.
    ///
    /// Every statistic starts at zero; a test sets only the fields it
    /// asserts on, then checks that a `squad` aggregate counted `players[0]`
    /// alone while `by_entity` kept both rows.
    ///
    /// The committed fixture CANNOT serve this purpose: every player in it
    /// is in-squad, so a fixture-based test passes whether or not the filter
    /// exists. That is exactly how the defect reached review in Task 5.
    pub fn two_player_report() -> (crate::Report, EntityIndex) {
```

Build it from the same `Encounter` -> `analyze` -> `build_report` path
`fixture_report` uses, with two hand-made `Player`s (`in_squad: true` and
`in_squad: false`). Rewrite Task 5's inline test in `damage.rs` to call it,
so there is ONE definition rather than four copies.

Run `cargo test -p axilog-schema --lib v1::blocks::damage` afterwards and
confirm Task 5's tests still pass unchanged — the helper must be a pure
refactor of the existing test, not a weakening of it.

- [ ] **Step 2: Write the failing test**

Create `crates/axilog-schema/src/v1/blocks/defense.rs` with ONLY this test
module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;

    #[test]
    fn every_squad_player_has_a_defenses_row_keyed_by_entity_id() {
        let (report, index) = fixture_report();
        let block = build_defenses(&report, &index);
        assert_eq!(
            block.by_entity.len(),
            report.players.len(),
            "one defenses row per player, no positional joins"
        );
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            assert!(block.by_entity.get(id).is_some(), "row for entity {id}");
        }
    }

    #[test]
    fn defenses_values_match_the_legacy_report_exactly() {
        let (report, index) = fixture_report();
        let block = build_defenses(&report, &index);
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            let row = block.by_entity.get(id).expect("row");
            assert_eq!(row.blocked, p.defenses.blocked, "no number may change");
            assert_eq!(row.evaded, p.defenses.evaded);
            assert_eq!(row.damage_taken, p.defenses.damage_taken);
        }
    }

    #[test]
    fn cc_squad_totals_sum_only_the_squad_rows() {
        // On the committed fixture every player is in-squad, so the aggregate
        // equals the sum of all rows. This pins the arithmetic; the filter
        // itself is pinned by the non-fixture test below, because a
        // fixture-only test cannot tell a working filter from a missing one.
        let (report, index) = fixture_report();
        let block = build_cc(&report, &index);
        let summed: u32 = block.by_entity.0.values().map(|r| r.applied_total).sum();
        assert_eq!(block.squad.applied_total, summed, "the aggregate must be the sum of its parts");
    }

    #[test]
    fn cc_squad_aggregate_excludes_non_squad_friendlies() {
        // Hand-built, NOT the fixture: every fixture player is in-squad.
        // Give the two players DIFFERENT cc totals so an unfiltered sum is
        // distinguishable from a filtered one.
        let (mut report, index) = crate::v1::blocks::tests_support::two_player_report();
        report.players[0].cc.applied_total = 7; // in-squad
        report.players[1].cc.applied_total = 5; // non-squad friendly
        let block = build_cc(&report, &index);
        assert_eq!(block.by_entity.len(), 2, "the roster keeps both players");
        assert_eq!(block.squad.applied_total, 7, "only the in-squad player counts");
    }

    #[test]
    fn hit_stats_rows_carry_no_names() {
        let (report, index) = fixture_report();
        let block = build_hit_stats(&report, &index);
        let v = serde_json::to_value(&block).expect("serializable");
        let text = serde_json::to_string(&v).expect("stringify");
        assert!(!text.contains("\"name\""), "no block inlines a name");
    }
}
```

Add to `crates/axilog-schema/src/v1/blocks/mod.rs`:

```rust
pub mod defense;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::blocks::defense`
Expected: FAIL — compile error, `cannot find function build_defenses in this scope`.

- [ ] **Step 4: Write minimal implementation**

Before writing, enumerate the real legacy field names:

```bash
sed -n '/pub struct DefensesOut/,/^}/p'  crates/axilog-schema/src/lib.rs
sed -n '/pub struct HitStatsOut/,/^}/p'  crates/axilog-schema/src/lib.rs
sed -n '/pub struct CcOut/,/^}/p'        crates/axilog-schema/src/lib.rs
```

Then prepend to `crates/axilog-schema/src/v1/blocks/defense.rs`:

```rust
use super::ByEntity;
use crate::v1::entities::EntityIndex;
use serde::Serialize;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DefensesBlock {
    pub by_entity: ByEntity<DefensesEntity>,
}

/// Incoming defenses. Mirrors the legacy `DefensesOut` field-for-field --
/// this spec reshapes, it does not recompute. Copy EVERY field from
/// `DefensesOut`; the list below is the shape, not an abridgement.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DefensesEntity {
    pub blocked: u32,
    pub evaded: u32,
    pub dodged: u32,
    pub missed: u32,
    pub interrupted: u32,
    pub invulned: u32,
    pub damage_taken: u64,
    pub strike_damage_taken: u64,
    pub condition_damage_taken: u64,
    pub life_leech_damage_taken: u64,
    pub barrier_damage_taken: u64,
    pub breakbar_damage_taken: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HitStatsBlock {
    pub by_entity: ByEntity<HitStatsEntity>,
}

/// Outgoing hit quality. Mirrors the legacy `HitStatsOut` field-for-field.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HitStatsEntity {
    pub connected_hits: u32,
    pub crit: u32,
    pub flank: u32,
    pub glance: u32,
    pub against_moving: u32,
    pub against_downed: u32,
    pub connected_direct: u32,
    pub connected_condition: u32,
    pub critable_direct: u32,
    pub life_leech: u32,
    pub above_90_hp: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcBlock {
    pub squad: CcSquad,
    pub by_entity: ByEntity<CcEntity>,
}

/// Aggregates `Role::Squad` entities ONLY. `by_entity` below carries the
/// full friendly roster, including `Role::FriendlyPlayer`. Filtering here
/// rather than summing the roster keeps this correct once the upstream
/// non-squad-friendly split is populated -- see the spec's decision 3.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcSquad {
    pub applied_total: u32,
    pub applied_duration_ms: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CcEntity {
    pub applied_total: u32,
    pub applied_duration_ms: u64,
    pub received_total: u32,
    pub received_duration_ms: u64,
    pub stun_breaks: u32,
    pub removed_stun_duration_ms: u64,
}

pub fn build_defenses(report: &crate::Report, index: &EntityIndex) -> DefensesBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let d = &p.defenses;
        by_entity.insert(
            id,
            DefensesEntity {
                blocked: d.blocked,
                evaded: d.evaded,
                dodged: d.dodged,
                missed: d.missed,
                interrupted: d.interrupted,
                invulned: d.invulned,
                damage_taken: d.damage_taken,
                strike_damage_taken: d.strike_damage_taken,
                condition_damage_taken: d.condition_damage_taken,
                life_leech_damage_taken: d.life_leech_damage_taken,
                barrier_damage_taken: d.barrier_damage_taken,
                breakbar_damage_taken: d.breakbar_damage_taken,
            },
        );
    }
    DefensesBlock { by_entity }
}

pub fn build_hit_stats(report: &crate::Report, index: &EntityIndex) -> HitStatsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let h = &p.hit_stats;
        by_entity.insert(
            id,
            HitStatsEntity {
                connected_hits: h.connected_hits,
                crit: h.crit,
                flank: h.flank,
                glance: h.glance,
                against_moving: h.against_moving,
                against_downed: h.against_downed,
                connected_direct: h.connected_direct,
                connected_condition: h.connected_condition,
                critable_direct: h.critable_direct,
                life_leech: h.life_leech,
                above_90_hp: h.above_90_hp,
            },
        );
    }
    HitStatsBlock { by_entity }
}

pub fn build_cc(report: &crate::Report, index: &EntityIndex) -> CcBlock {
    let mut by_entity = ByEntity::default();
    let mut squad = CcSquad::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let c = &p.cc;
        // Squad aggregate excludes non-squad friendlies; `by_entity` does not.
        if index.role_of(id) == Some(crate::v1::entities::Role::Squad) {
            squad.applied_total += c.applied_total;
            squad.applied_duration_ms += c.applied_duration_ms;
        }
        by_entity.insert(
            id,
            CcEntity {
                applied_total: c.applied_total,
                applied_duration_ms: c.applied_duration_ms,
                received_total: c.received_total,
                received_duration_ms: c.received_duration_ms,
                stun_breaks: c.stun_breaks,
                removed_stun_duration_ms: c.removed_stun_duration_ms,
            },
        );
    }
    CcBlock { squad, by_entity }
}
```

If a legacy field name differs from the mirror above, use the LEGACY name on
the right-hand side and keep the 1.0 name on the left; where the legacy name
is an EI artifact (e.g. abbreviations), prefer the clearer 1.0 name and note
the mapping in a comment.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::blocks`
Expected: PASS — the 5 defense-file tests plus Task 5's damage tests, which
the Step 1 refactor must leave green.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-schema/src/v1/
git commit -m "feat(schema): add the 1.0 defenses, hit_stats, and cc blocks"
```

---

### Task 7: `boons`, `support`, `contribution`, and `healing` blocks

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/support.rs`
- Modify: `crates/axilog-schema/src/v1/blocks/mod.rs` (add `pub mod support;`)

**Interfaces:**
- Consumes: `crate::Report` (legacy `BoonOut`, `GenerationOut`, `SupportOut`,
  `ContributionOut`, `HealingOut`), `EntityIndex`, `CatalogBuilder`.
- Produces:
  `BoonsBlock { by_entity: ByEntity<BTreeMap<u32, BoonRow>> }`,
  `SupportBlock { by_entity: ByEntity<SupportEntity> }`,
  `ContributionBlock { by_entity: ByEntity<ContributionEntity> }`,
  `HealingBlock { by_entity: ByEntity<HealingEntity> }`;
  `build_boons(&crate::Report, &EntityIndex, &mut CatalogBuilder) -> BoonsBlock`,
  `build_support(&crate::Report, &EntityIndex) -> SupportBlock`,
  `build_contribution(&crate::Report, &EntityIndex) -> ContributionBlock`,
  `build_healing(&crate::Report, &EntityIndex) -> HealingBlock`.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/blocks/support.rs` with ONLY this test
module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;
    use crate::v1::catalogs::CatalogBuilder;

    #[test]
    fn boons_are_keyed_by_buff_id_not_by_position_in_a_fixed_array() {
        // The legacy shape is `Vec<BoonOut>` in `buffs::BOON_IDS` order --
        // a positional join a consumer must know the table to read.
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);

        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("boon row");
        assert!(!row.is_empty(), "player carries per-boon rows");
        for buff_id in row.keys() {
            assert!(*buff_id > 0, "keys are real buff ids");
        }
    }

    #[test]
    fn every_referenced_buff_id_resolves_in_the_catalog() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        let built = cats.finish(&Default::default(), None);
        for row in block.by_entity.0.values() {
            for buff_id in row.keys() {
                assert!(built.buffs.contains_key(buff_id), "buff {buff_id} must resolve");
            }
        }
    }

    #[test]
    fn boon_uptime_matches_the_legacy_report_exactly() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            let row = block.by_entity.get(id).expect("row");
            for legacy in &p.boons {
                let got = row.get(&legacy.id).expect("boon present");
                assert_eq!(got.uptime_pct, legacy.presence_pct, "no number may change");
            }
        }
    }

    #[test]
    fn contribution_carries_both_directions() {
        let (report, index) = fixture_report();
        let block = build_contribution(&report, &index);
        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("row");
        assert_eq!(row.downs_contribution.damage, p.downs_contribution.damage);
        assert_eq!(row.downed_by.damage, p.downed_by.damage);
    }
}
```

Add to `crates/axilog-schema/src/v1/blocks/mod.rs`:

```rust
pub mod support;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::blocks::support`
Expected: FAIL — compile error, `cannot find function build_boons in this scope`.

- [ ] **Step 3: Write minimal implementation**

Before writing, enumerate the real legacy field names:

```bash
sed -n '/pub struct BoonOut/,/^}/p'         crates/axilog-schema/src/lib.rs
sed -n '/pub struct GenerationOut/,/^}/p'   crates/axilog-schema/src/lib.rs
sed -n '/pub struct SupportOut/,/^}/p'      crates/axilog-schema/src/lib.rs
sed -n '/pub struct ContributionOut/,/^}/p' crates/axilog-schema/src/lib.rs
sed -n '/pub struct HealingOut/,/^}/p'      crates/axilog-schema/src/lib.rs
```

Then prepend to `crates/axilog-schema/src/v1/blocks/support.rs`:

```rust
use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonsBlock {
    /// entity id -> buff id -> row. Two levels of real ids, no positional
    /// joins: the legacy shape was a `Vec` in `buffs::BOON_IDS` order, which
    /// a consumer could only read by knowing that table.
    pub by_entity: ByEntity<BTreeMap<u32, BoonRow>>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonRow {
    pub uptime_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationRow>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct GenerationRow {
    #[serde(rename = "self")]
    pub self_: f64,
    pub group: f64,
    pub squad: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportBlock {
    pub by_entity: ByEntity<SupportEntity>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportEntity {
    pub cleanses: u32,
    pub cleanses_self: u32,
    pub strips: u32,
    pub resurrects: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionBlock {
    pub by_entity: ByEntity<ContributionEntity>,
}

/// Both directions of the arcdps-methodology down contribution (M11).
/// GW2EI has no equivalent surface -- this follows arcdps itself.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionEntity {
    pub downs_contribution: ContributionRow,
    pub downed_by: ContributionRow,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionRow {
    pub damage: u64,
    pub cc: u32,
    pub strips: u32,
    pub movement_impairing: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingBlock {
    pub by_entity: ByEntity<HealingEntity>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingEntity {
    pub outgoing: u64,
    pub outgoing_barrier: u64,
    pub downed_healing: u64,
}

pub fn build_boons(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> BoonsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let mut rows = BTreeMap::new();
        for b in &p.boons {
            cats.reference_buff(b.id);
            rows.insert(
                b.id,
                BoonRow {
                    uptime_pct: b.presence_pct,
                    avg_stacks: b.avg_stacks,
                    generation: b.generation.as_ref().map(|g| GenerationRow {
                        self_: g.self_,
                        group: g.group,
                        squad: g.squad,
                    }),
                },
            );
        }
        by_entity.insert(id, rows);
    }
    BoonsBlock { by_entity }
}

pub fn build_support(report: &crate::Report, index: &EntityIndex) -> SupportBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            SupportEntity {
                cleanses: p.support.cleanses,
                cleanses_self: p.support.cleanses_self,
                strips: p.support.strips,
                resurrects: p.support.resurrects,
            },
        );
    }
    SupportBlock { by_entity }
}

pub fn build_contribution(report: &crate::Report, index: &EntityIndex) -> ContributionBlock {
    let row = |c: &crate::ContributionOut| ContributionRow {
        damage: c.damage,
        cc: c.cc,
        strips: c.strips,
        movement_impairing: c.movement_impairing,
    };
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            ContributionEntity {
                downs_contribution: row(&p.downs_contribution),
                downed_by: row(&p.downed_by),
            },
        );
    }
    ContributionBlock { by_entity }
}

pub fn build_healing(report: &crate::Report, index: &EntityIndex) -> HealingBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            HealingEntity {
                outgoing: p.healing.outgoing,
                outgoing_barrier: p.healing.outgoing_barrier,
                downed_healing: p.healing.downed_healing,
            },
        );
    }
    HealingBlock { by_entity }
}
```

Implementer note: `BoonOut` may not carry an `id` field (the legacy `Vec` is
positional over `buffs::BOON_IDS`). If it does not, add
`pub id: u32` to `BoonOut` populated from `buffs::BOON_IDS[i]` in
`build_report` — it is a genuine field, useful in the legacy shape too, and
it is what makes the positional join disappear. Adding a serialized field to
the legacy shape DOES change legacy output, so instead mark it
`#[serde(skip)]` for now and confirm
`cargo test -p axilog-core --test golden` stays green.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::blocks::support`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/
git commit -m "feat(schema): add the 1.0 boons, support, contribution, healing blocks"
```

---

### Task 8: `rotation`, `damage_mods`, `missiles`, `replay`, and `series` blocks

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/activity.rs`
- Modify: `crates/axilog-schema/src/v1/blocks/mod.rs` (add `pub mod activity;`)

**Interfaces:**
- Consumes: `crate::Report` (legacy `CastOut`, `SkillRotationOut`,
  `DamageModEntryOut`, `MissilesOut`, `ReplayOut`, `TimelineOut`,
  `PlayerPerSecondOut`), `EntityIndex`, `CatalogBuilder`,
  `crate::v1::series::SeriesOut`.
- Produces:
  `RotationBlock`, `DamageModsBlock`, `MissilesBlock`, `ReplayBlock`, `SeriesBlock`;
  `build_rotation(&crate::Report, &EntityIndex, &mut CatalogBuilder) -> RotationBlock`,
  `build_damage_mods(&crate::Report, &EntityIndex, &mut CatalogBuilder) -> DamageModsBlock`,
  `build_missiles(&crate::Report, &EntityIndex) -> MissilesBlock`,
  `build_replay(&crate::Report, &EntityIndex) -> ReplayBlock`,
  `build_series(&crate::Report, &EntityIndex) -> SeriesBlock`.

This task closes the `ReplayTrackOut` join-key gap: the legacy replay track
has NO key tying a track to a player. In 1.0 it is keyed by entity id like
every other block.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/src/v1/blocks/activity.rs` with ONLY this test
module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;

    #[test]
    fn squad_series_use_the_shared_envelope_and_decode_to_the_legacy_arrays() {
        let (report, index) = fixture_report();
        let block = build_series(&report, &index);
        let squad = block.squad.as_ref().expect("squad series present");
        assert_eq!(
            squad.damage.decode_u64(),
            report.timeline.per_second.squad_damage,
            "the envelope must be lossless -- this spec changes no number"
        );
        assert_eq!(squad.damage.interval_ms, report.timeline.resolution_ms);
    }

    #[test]
    fn a_replay_track_is_keyed_by_entity_id() {
        // The legacy `ReplayTrackOut` carries NO join key at all, so a
        // consumer cannot tell whose track it is reading.
        let (report, index) = fixture_report();
        let block = build_replay(&report, &index);
        // The committed fixture is parsed without `--replay`, so the block
        // is empty -- the assertion that matters is the SHAPE.
        for entity_id in block.by_entity.0.keys() {
            assert!(
                index.by_agent_addr(u64::from(*entity_id)).is_some() || *entity_id < 1000,
                "keys are entity ids"
            );
        }
        let v = serde_json::to_value(&block).expect("serializable");
        assert!(v.get("by_entity").is_some(), "replay hangs off by_entity like every block");
    }

    #[test]
    fn rotation_casts_reference_skill_ids_and_register_them() {
        let (report, index) = fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_rotation(&report, &index, &mut cats);
        let built = cats.finish(&Default::default(), None);
        for row in block.by_entity.0.values() {
            for cast in &row.casts {
                assert!(built.skills.contains_key(&cast.skill_id), "cast skill must resolve");
            }
        }
    }

    #[test]
    fn an_ungated_block_is_empty_rather_than_absent_at_this_layer() {
        // Coverage (Task 9) decides absence. A builder always returns a
        // well-formed, possibly-empty block, so assembly has one rule.
        let (report, index) = fixture_report();
        let block = build_missiles(&report, &index);
        let _ = serde_json::to_value(&block).expect("an empty block still serializes");
    }
}
```

Add to `crates/axilog-schema/src/v1/blocks/mod.rs`:

```rust
pub mod activity;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --lib v1::blocks::activity`
Expected: FAIL — compile error, `cannot find function build_series in this scope`.

- [ ] **Step 3: Write minimal implementation**

Before writing, enumerate the real legacy shapes:

```bash
sed -n '/pub struct CastOut/,/^}/p'            crates/axilog-schema/src/lib.rs
sed -n '/pub struct SkillRotationOut/,/^}/p'   crates/axilog-schema/src/lib.rs
sed -n '/pub struct DamageModEntryOut/,/^}/p'  crates/axilog-schema/src/lib.rs
sed -n '/pub struct ReplayOut/,/^}/p'          crates/axilog-schema/src/lib.rs
sed -n '/pub struct ReplayTrackOut/,/^}/p'     crates/axilog-schema/src/lib.rs
sed -n '/pub struct PlayerPerSecondOut/,/^}/p' crates/axilog-schema/src/lib.rs
sed -n '/pub struct MissilesOut/,/^}/p'        crates/axilog-schema/src/lib.rs
```

Then prepend to `crates/axilog-schema/src/v1/blocks/activity.rs`:

```rust
use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use crate::v1::series::SeriesOut;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct RotationBlock {
    pub by_entity: ByEntity<RotationEntity>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct RotationEntity {
    pub cast_count: u32,
    pub casts: Vec<CastRow>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CastRow {
    pub skill_id: u32,
    pub time_ms: u64,
    pub duration_ms: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageModsBlock {
    pub by_entity: ByEntity<BTreeMap<i32, DamageModRow>>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageModRow {
    pub hit_count: u32,
    pub total_hit_count: u32,
    pub damage_gain: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squad: Option<MissilesSquad>,
    pub by_entity: ByEntity<MissilesEntity>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesSquad {
    pub incoming_denied: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesEntity {
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ReplayBounds>,
    /// Keyed by entity id. The legacy `ReplayTrackOut` carried no join key
    /// at all, so a consumer could not tell whose track it was reading.
    pub by_entity: ByEntity<ReplayTrack>,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayTrack {
    pub interval_ms: u64,
    pub x: SeriesOut,
    pub y: SeriesOut,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SeriesBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squad: Option<SquadSeries>,
    pub by_entity: ByEntity<EntitySeries>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SquadSeries {
    pub damage: SeriesOut,
    pub cc_applied: SeriesOut,
    pub downs: SeriesOut,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct EntitySeries {
    pub damage: SeriesOut,
}

pub fn build_series(report: &crate::Report, index: &EntityIndex) -> SeriesBlock {
    let res = report.timeline.resolution_ms;
    let ps = &report.timeline.per_second;
    let squad = Some(SquadSeries {
        damage: SeriesOut::encode_u64(res, &ps.squad_damage),
        cc_applied: SeriesOut::encode_u64(
            res,
            &ps.cc_applied.iter().map(|v| u64::from(*v)).collect::<Vec<_>>(),
        ),
        downs: SeriesOut::encode_u64(
            res,
            &ps.downs.iter().map(|v| u64::from(*v)).collect::<Vec<_>>(),
        ),
    });

    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(series) = p.per_second.as_ref() else { continue };
        by_entity.insert(id, EntitySeries { damage: SeriesOut::encode_u64(res, &series.damage) });
    }

    SeriesBlock { squad, by_entity }
}

pub fn build_rotation(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> RotationBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(rot) = p.rotation.as_ref() else { continue };
        let mut casts = Vec::new();
        for skill in rot {
            cats.reference_skill(skill.id);
            for c in &skill.casts {
                casts.push(CastRow {
                    skill_id: skill.id,
                    time_ms: c.time_ms,
                    duration_ms: c.duration_ms,
                });
            }
        }
        casts.sort_by_key(|c| (c.time_ms, c.skill_id));
        let cast_count = casts.len() as u32;
        by_entity.insert(id, RotationEntity { cast_count, casts });
    }
    RotationBlock { by_entity }
}

pub fn build_damage_mods(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> DamageModsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(mods) = p.damage_mods.as_ref() else { continue };
        let mut rows = BTreeMap::new();
        for m in mods {
            cats.reference_damage_mod(m.id);
            rows.insert(
                m.id,
                DamageModRow {
                    hit_count: m.hit_count,
                    total_hit_count: m.total_hit_count,
                    damage_gain: m.damage_gain,
                },
            );
        }
        by_entity.insert(id, rows);
    }
    DamageModsBlock { by_entity }
}

pub fn build_missiles(report: &crate::Report, index: &EntityIndex) -> MissilesBlock {
    let Some(m) = report.missiles.as_ref() else { return MissilesBlock::default() };
    let mut by_entity = ByEntity::default();
    // `PlayerMissilesOut` already carries its own `agent_addr` -- join on it
    // rather than on array position.
    for row in &m.players {
        let Some(id) = index.by_agent_addr(row.agent_addr) else { continue };
        by_entity.insert(
            id,
            MissilesEntity { fired: row.fired, hit: row.hit, denied: row.denied },
        );
    }
    MissilesBlock {
        squad: Some(MissilesSquad { incoming_denied: m.squad.incoming_denied }),
        by_entity,
    }
}

pub fn build_replay(report: &crate::Report, index: &EntityIndex) -> ReplayBlock {
    let Some(r) = report.replay.as_ref() else { return ReplayBlock::default() };
    let mut by_entity = ByEntity::default();
    // Requires Step 1 below: `ReplayTrackOut` must carry the `agent_addr`
    // that `analysis::replay::Track` already has.
    for track in &r.tracks {
        let Some(id) = index.by_agent_addr(track.agent_addr) else { continue };
        by_entity.insert(
            id,
            ReplayTrack {
                interval_ms: r.interval_ms,
                x: SeriesOut::encode_f64(r.interval_ms, &track.x),
                y: SeriesOut::encode_f64(r.interval_ms, &track.y),
            },
        );
    }
    ReplayBlock {
        bounds: Some(ReplayBounds {
            min_x: r.bounds.min_x,
            min_y: r.bounds.min_y,
            max_x: r.bounds.max_x,
            max_y: r.bounds.max_y,
        }),
        by_entity,
    }
}
```

Implementer notes:

1. `ReplayTrackOut` carries NO join key today (`name`, `team`, `commander`,
   `is_squad`, `samples`, intervals) even though its upstream
   `axilog_core::analysis::replay::Track` has `agent_addr`. That omission is
   the documented replay join-key gap. Before writing `build_replay`, add to
   `ReplayTrackOut` in `crates/axilog-schema/src/lib.rs`:

   ```rust
       /// The representative raw agent addr, carried from
       /// `analysis::replay::Track`. `#[serde(skip)]` so the legacy JSON
       /// stays byte-identical; promoted to the 1.0 `by_entity` key.
       #[serde(skip)]
       pub agent_addr: u64,
   ```

   and populate it from `t.agent_addr` where `build_report` builds the
   tracks. Run `cargo test -p axilog-core --test golden && cargo test -p axilog-ei`
   to confirm no golden moved.
2. APM is deliberately NOT a field here. It is `cast_count` over the
   entity's active time, both of which a consumer already has, and storing a
   derived rate in a wire format invites it to disagree with its own inputs.
   `docs/NATIVE-FORMAT.md` (Task 14) states the formula.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --lib v1::blocks::activity`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/src/v1/
git commit -m "feat(schema): add the 1.0 rotation, damage_mods, missiles, replay, series blocks"
```

---

### Task 9: `ReportV1` assembly and coverage wiring

**Files:**
- Modify: `crates/axilog-schema/src/v1/mod.rs`
- Test: `crates/axilog-schema/tests/v1_shape.rs` (create)

**Interfaces:**
- Consumes: everything from Tasks 1–8.
- Produces:
  `ReportV1 { axilog: AxilogMeta, encounter: EncounterOut, entities: Vec<EntityOut>, catalogs: Catalogs, blocks: Blocks, coverage: Coverage, warnings: Vec<WarningOut> }`;
  `build_report_v1(enc: &Encounter, metrics: &Metrics, legacy: &crate::Report, axilog_version: &str, generated_from: Option<&str>, damage_mods: Option<&DamageModifierResults>) -> ReportV1`.

`build_report_v1` takes the already-built legacy `Report` rather than
rebuilding from `Metrics`, which is what keeps the 1.0 shape a pure
reprojection and Task 10's equivalence test tight.

- [ ] **Step 1: Give warnings a code at the source**

The 1.0 `WarningOut` needs a machine-readable code, and inventing one at
serialization time would just be the `Vec<String>` problem with extra steps.
Enumerate the real producers first:

```bash
grep -rn 'warnings.push' crates/axilog-core/src/
```

In `crates/axilog-core/src/analysis/mod.rs`, replace
`pub warnings: Vec<String>` with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum WarningSeverity { Info, Warn, Error }

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    /// Closed, documented set. Adding a code is additive; changing one's
    /// meaning is a breaking change to the 1.0 format.
    pub code: &'static str,
    pub severity: WarningSeverity,
    pub message: String,
    /// The agent this warning is about, when it is about one. Mapped to an
    /// entity id at serialization.
    pub agent_addr: Option<u64>,
}

pub warnings: Vec<Warning>,
```

Give every `warnings.push` site a distinct `code` describing the condition
(e.g. `blank_account_agent`, `no_team_change_event`). Update the legacy
`Report::warnings` mapping in `build_report` to `w.message.clone()` so the
LEGACY JSON stays byte-identical.

Run: `cargo test -p axilog-core && cargo test -p axilog-ei`
Expected: PASS, no golden re-blessed.

- [ ] **Step 2: Write the failing test**

Create `crates/axilog-schema/tests/v1_shape.rs`:

```rust
//! Structural invariants of the 1.0 container.
use axilog_schema::v1::envelope::BlockName;

fn build() -> serde_json::Value {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        Some("wvw-small.anon.zevtc"),
    );
    serde_json::to_value(&v1).expect("serializable")
}

#[test]
fn the_document_has_exactly_the_six_top_level_keys() {
    let v = build();
    let obj = v.as_object().expect("object");
    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    // `warnings` is omitted when empty, so it is optional here.
    let expected_always = ["axilog", "blocks", "catalogs", "coverage", "encounter", "entities"];
    for k in expected_always {
        assert!(keys.contains(&k), "missing top-level key {k}");
    }
    for k in &keys {
        assert!(
            expected_always.contains(k) || *k == "warnings",
            "unexpected top-level key {k} -- the 1.0 container is closed"
        );
    }
}

#[test]
fn the_schema_version_is_one_point_oh_and_distinct_from_the_binary_version() {
    let v = build();
    assert_eq!(v["axilog"]["schema"], "1.0");
    assert_eq!(v["axilog"]["version"], "0.0.0-test");
    assert_eq!(v["axilog"]["generated_from"], "wvw-small.anon.zevtc");
}

#[test]
fn coverage_names_every_block_and_agrees_with_what_blocks_carries() {
    let v = build();
    let coverage = v["coverage"].as_object().expect("coverage object");
    assert_eq!(coverage.len(), BlockName::ALL.len());
    let blocks = v["blocks"].as_object().expect("blocks object");
    for block in BlockName::ALL {
        let name = block.as_str();
        let state = coverage[name].as_str().expect("state string");
        let present = blocks.contains_key(name);
        match state {
            "present" => assert!(present, "coverage says {name} is present but blocks lacks it"),
            "not_computed" | "unsupported" => {
                assert!(!present, "coverage says {name} is {state} but blocks carries it")
            }
            "empty" => {}
            other => panic!("unknown coverage state {other} for {name}"),
        }
    }
}

#[test]
fn every_referenced_id_resolves_and_every_catalog_entry_is_referenced() {
    let v = build();
    let text = serde_json::to_string(&v["blocks"]).expect("stringify blocks");

    for (catalog, key_prefix) in [("skills", "skill"), ("buffs", "buff"), ("damage_mods", "mod")] {
        let Some(entries) = v["catalogs"].get(catalog).and_then(|c| c.as_object()) else { continue };
        for id in entries.keys() {
            // Direction 2: no orphan definitions. A referenced id appears in
            // the blocks payload either as a map key or as a `*_id` value.
            assert!(
                text.contains(&format!("\"{id}\"")) || text.contains(&format!(":{id}")),
                "catalog {catalog} entry {id} ({key_prefix}) is never referenced by any block"
            );
        }
    }
}

#[test]
fn parsing_the_same_log_twice_is_byte_identical() {
    let a = serde_json::to_string(&build()).expect("stringify");
    let b = serde_json::to_string(&build()).expect("stringify");
    assert_eq!(a, b, "entity ids are indices into a sorted roster -- the sort must be total");
}

#[test]
fn no_block_inlines_a_human_readable_name() {
    let v = build();
    let text = serde_json::to_string(&v["blocks"]).expect("stringify blocks");
    assert!(!text.contains("\"name\""), "names live in catalogs and entities only");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p axilog-schema --test v1_shape`
Expected: FAIL — compile error, `cannot find function build_report_v1`.

- [ ] **Step 4: Write minimal implementation**

Append to `crates/axilog-schema/src/v1/mod.rs`:

```rust
use crate::v1::blocks::{activity, damage, defense, support};
use crate::v1::catalogs::{CatalogBuilder, Catalogs};
use crate::v1::entities::{build_entities, EntityOut};
use crate::v1::envelope::{AxilogMeta, BlockName, Coverage, CoverageState, Severity, WarningOut};
use axilog_core::analysis::Metrics;
use axilog_core::model::Encounter;
use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ReportV1 {
    pub axilog: AxilogMeta,
    pub encounter: crate::EncounterOut,
    pub entities: Vec<EntityOut>,
    pub catalogs: Catalogs,
    pub blocks: Blocks,
    pub coverage: Coverage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<WarningOut>,
}

/// A block is omitted entirely when `coverage` says `not_computed` or
/// `unsupported`; `empty` blocks are still carried, so a consumer can tell
/// "computed and there was nothing" from "never ran".
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct Blocks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<damage::DamageBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defenses: Option<defense::DefensesBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_stats: Option<defense::HitStatsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<defense::CcBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boons: Option<support::BoonsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<support::SupportBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution: Option<support::ContributionBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing: Option<support::HealingBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<activity::RotationBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage_mods: Option<activity::DamageModsBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missiles: Option<activity::MissilesBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<activity::ReplayBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<activity::SeriesBlock>,
}

pub fn build_report_v1(
    enc: &Encounter,
    metrics: &Metrics,
    legacy: &crate::Report,
    axilog_version: &str,
    generated_from: Option<&str>,
    damage_mods: Option<&axilog_core::analysis::damage_mods::DamageModifierResults>,
) -> ReportV1 {
    let (entities, index) = build_entities(enc, metrics);
    let mut cats = CatalogBuilder::default();
    let mut coverage = Coverage::new();

    // Always-on blocks.
    let damage = damage::build_damage(legacy, &index, &mut cats);
    coverage.set(BlockName::Damage, CoverageState::Present);
    let defenses = defense::build_defenses(legacy, &index);
    coverage.set(BlockName::Defenses, CoverageState::Present);
    let hit_stats = defense::build_hit_stats(legacy, &index);
    coverage.set(BlockName::HitStats, CoverageState::Present);
    let cc = defense::build_cc(legacy, &index);
    coverage.set(BlockName::Cc, CoverageState::Present);
    let boons = support::build_boons(legacy, &index, &mut cats);
    coverage.set(BlockName::Boons, CoverageState::Present);
    let support_block = support::build_support(legacy, &index);
    coverage.set(BlockName::Support, CoverageState::Present);
    let contribution = support::build_contribution(legacy, &index);
    coverage.set(BlockName::Contribution, CoverageState::Present);
    let healing = support::build_healing(legacy, &index);
    coverage.set(BlockName::Healing, CoverageState::Present);
    let series = activity::build_series(legacy, &index);
    coverage.set(BlockName::Series, CoverageState::Present);

    // Gated blocks: presence of the legacy `Option` IS the gate signal, the
    // same rule the legacy shape already uses.
    let rotation = legacy.players.iter().any(|p| p.rotation.is_some()).then(|| {
        coverage.set(BlockName::Rotation, CoverageState::Present);
        activity::build_rotation(legacy, &index, &mut cats)
    });
    let damage_mods = legacy.damage_mod_map.is_some().then(|| {
        coverage.set(BlockName::DamageMods, CoverageState::Present);
        activity::build_damage_mods(legacy, &index, &mut cats)
    });
    let missiles = legacy.missiles.is_some().then(|| {
        coverage.set(BlockName::Missiles, CoverageState::Present);
        activity::build_missiles(legacy, &index)
    });
    let replay = legacy.replay.is_some().then(|| {
        coverage.set(BlockName::Replay, CoverageState::Present);
        activity::build_replay(legacy, &index)
    });

    // Reserved for spec #2. Named here so the vocabulary is fixed.
    coverage.set(BlockName::Conditions, CoverageState::NotComputed);
    coverage.set(BlockName::Minions, CoverageState::NotComputed);

    // `Metrics::warnings` carries a code at the source as of this task --
    // see Step 1. A catch-all code would defeat the whole point of making
    // warnings structured, so there is no `_ =>` arm here.
    let warnings = metrics
        .warnings
        .iter()
        .map(|w| WarningOut {
            code: w.code.to_string(),
            severity: match w.severity {
                axilog_core::analysis::WarningSeverity::Info => Severity::Info,
                axilog_core::analysis::WarningSeverity::Warn => Severity::Warn,
                axilog_core::analysis::WarningSeverity::Error => Severity::Error,
            },
            message: w.message.clone(),
            entity_id: w.agent_addr.and_then(|a| index.by_agent_addr(a)),
        })
        .collect();

    ReportV1 {
        axilog: AxilogMeta {
            schema: "1.0",
            version: axilog_version.to_string(),
            generated_from: generated_from.map(|s| s.to_string()),
        },
        encounter: legacy.encounter.clone(),
        entities,
        catalogs: cats.finish(metrics, damage_mods),
        blocks: Blocks {
            damage: Some(damage),
            defenses: Some(defenses),
            hit_stats: Some(hit_stats),
            cc: Some(cc),
            boons: Some(boons),
            support: Some(support_block),
            contribution: Some(contribution),
            healing: Some(healing),
            rotation,
            damage_mods,
            missiles,
            replay,
            series: Some(series),
        },
        coverage,
        warnings,
    }
}
```

Implementer notes:

1. `crate::EncounterOut` is not `Clone` today. Derive `Clone` on it and its
   members (`TeamOut`, `MarkerAssignmentOut`, `TickRateOut`,
   `CommanderTagOut`) so `legacy.encounter.clone()` compiles. Also rekey
   `encounter.markers[]` from `agent_addr` to entity id — define a
   1.0-local marker type carrying `entity_id: u32` rather than mutating the
   legacy one, which the EI adapter still reads.
2. `enc` is unused in the body as written (everything routes through
   `legacy` and `metrics`) EXCEPT for `build_entities`. Keep the parameter;
   do not "simplify" it away.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p axilog-schema --test v1_shape`
Expected: PASS, 6 tests.

- [ ] **Step 6: Commit the assembly**

```bash
git add crates/
git commit -m "feat(schema): assemble the 1.0 report with coverage wiring"
```

- [ ] **Step 7: Add the committed full key-set golden**

The spec's compatibility rule ("additive-only within a major") is only real
if removing a key fails CI. Append to
`crates/axilog-schema/tests/v1_shape.rs`:

```rust
/// The COMPLETE 1.0 key set on the committed fixture, as a sorted list of
/// dotted paths. Removing or renaming a key fails; adding one is a reviewed
/// diff. This is the compatibility rule made executable -- the six-key test
/// above only guards the top level.
#[test]
fn the_full_key_set_matches_the_committed_golden() {
    fn walk(v: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, val) in m {
                    // Entity/skill/buff ids are DATA, not schema -- collapse
                    // them so the golden tracks shape, not fixture content.
                    let key = if k.chars().all(|c| c.is_ascii_digit() || c == '-') { "<id>" } else { k };
                    let path = if prefix.is_empty() { key.to_string() } else { format!("{prefix}.{key}") };
                    if !out.contains(&path) {
                        out.push(path.clone());
                    }
                    walk(val, &path, out);
                }
            }
            serde_json::Value::Array(items) => {
                if let Some(first) = items.first() {
                    walk(first, &format!("{prefix}[]"), out);
                }
            }
            _ => {}
        }
    }

    let v = build();
    let mut keys = Vec::new();
    walk(&v, "", &mut keys);
    keys.sort();

    let golden_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/v1-keyset.golden.txt");
    let actual = keys.join("\n") + "\n";
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(golden_path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(golden_path).unwrap_or_default();
    assert_eq!(
        actual, expected,
        "the 1.0 key set changed. Adding keys is additive and fine -- re-run with \
         UPDATE_GOLDEN=1 and review the diff. REMOVING or RENAMING a key is a \
         breaking change requiring a major bump."
    );
}
```

Generate it once and review every line before committing:

```bash
UPDATE_GOLDEN=1 cargo test -p axilog-schema --test v1_shape the_full_key_set
cat crates/axilog-schema/tests/v1-keyset.golden.txt
cargo test -p axilog-schema --test v1_shape
```

- [ ] **Step 8: Commit the golden**

```bash
git add crates/axilog-schema/tests/
git commit -m "test(schema): commit the full 1.0 key-set golden"
```

---

### Task 10: Equivalence test — the 1.0 shape loses nothing

**Files:**
- Create: `crates/axilog-schema/tests/v1_equivalence.rs`

**Interfaces:**
- Consumes: `build_report_v1` (Task 9), legacy `build_report`.
- Produces: nothing — this is the reshape's safety net, standing in for the
  spec's "adapter reads the new shape" guarantee (see the spec amendment at
  the top of this plan).

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/tests/v1_equivalence.rs`:

```rust
//! The 1.0 container is a pure REPROJECTION of the legacy report: every
//! number reachable in the legacy shape is reachable, and identical, in the
//! 1.0 shape.
//!
//! This is spec #1's safety net. The spec's Section 5 proposed proving the
//! reshape by re-pointing the EI adapter; this plan defers that to spec #2
//! (see the spec amendment in the plan header) and proves it here instead,
//! which is both tighter and cheaper.

fn build() -> (axilog_schema::Report, axilog_schema::v1::ReportV1) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None);
    (legacy, v1)
}

#[test]
fn every_legacy_player_has_exactly_one_entity() {
    let (legacy, v1) = build();
    let squad_entities = v1
        .entities
        .iter()
        .filter(|e| {
            matches!(
                e.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            )
        })
        .count();
    assert_eq!(squad_entities, legacy.players.len(), "no player lost or duplicated in the roster");
}

#[test]
fn every_legacy_enemy_has_exactly_one_entity() {
    let (legacy, v1) = build();
    let enemy_entities = v1
        .entities
        .iter()
        .filter(|e| {
            !matches!(
                e.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            )
        })
        .count();
    assert_eq!(enemy_entities, legacy.enemies.len(), "no enemy lost or duplicated in the roster");
}

#[test]
fn per_player_damage_totals_are_identical() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account: std::collections::BTreeMap<&str, &axilog_schema::PlayerOut> =
        legacy.players.iter().map(|p| (p.account.as_str(), p)).collect();

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");
        assert_eq!(row.total, p.damage.total, "{account} total damage");
        assert_eq!(row.dps, p.damage.dps, "{account} dps");
        assert_eq!(row.taken, p.damage_taken, "{account} damage taken");
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_boon_cell_survives_the_reshape() {
    let (legacy, v1) = build();
    let boons = v1.blocks.boons.as_ref().expect("boons block present");
    let by_account: std::collections::BTreeMap<&str, &axilog_schema::PlayerOut> =
        legacy.players.iter().map(|p| (p.account.as_str(), p)).collect();

    let mut cells = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = boons.by_entity.get(e.id).expect("boon row");
        assert_eq!(row.len(), p.boons.len(), "{account} boon count");
        for legacy_boon in &p.boons {
            let got = row.get(&legacy_boon.id).expect("boon id present");
            assert_eq!(got.uptime_pct, legacy_boon.presence_pct, "{account} boon {} uptime", legacy_boon.id);
            cells += 1;
        }
    }
    assert!(cells >= 300, "expected the full boon matrix, got {cells} cells");
}

#[test]
fn the_squad_damage_series_decodes_back_to_the_legacy_array() {
    let (legacy, v1) = build();
    let series = v1.blocks.series.as_ref().expect("series block");
    let squad = series.squad.as_ref().expect("squad series");
    assert_eq!(squad.damage.decode_u64(), legacy.timeline.per_second.squad_damage);
    assert_eq!(
        squad.downs.decode_u64(),
        legacy.timeline.per_second.downs.iter().map(|v| u64::from(*v)).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --test v1_equivalence`
Expected: FAIL. If it passes on the first run, the test is not exercising
anything — check that `build()` really produces populated blocks.

- [ ] **Step 3: Fix whatever it caught**

Any failure here is a reprojection bug in Tasks 5–9, not a test to relax.
Fix the builder. The only legitimate reason to change an assertion is if the
legacy field it reads genuinely has no 1.0 counterpart by design — in which
case document that in the spec's block table.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema`
Expected: PASS, all suites.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/tests/
git commit -m "test(schema): assert the 1.0 shape reprojects the legacy report losslessly"
```

---

### Task 11: PII boundary and its structural assertion

**Files:**
- Modify: `crates/axilog-core/tests/golden.rs`
- Modify: wherever the PII scrub lives (find with
  `grep -rn 'anon_account\|scrub' crates/axilog-core/src/ | head -20`)

**Interfaces:**
- Consumes: `build_report_v1`.
- Produces: no new public API — a test plus, if needed, a scrub that walks
  `entities[]` only.

- [ ] **Step 1: Write the failing test**

Append to `crates/axilog-core/tests/golden.rs`:

```rust
/// After scrubbing, no real account or character string appears ANYWHERE in
/// the serialized 1.0 document.
///
/// The 1.0 container makes this assertable for the first time: account and
/// character names live in `entities[]` and nowhere else, so the scrub is a
/// single pass rather than a hunt through nested structures. The M15 fix
/// waves found that hunt had already missed a `_note` field once.
#[test]
fn no_unscrubbed_identity_survives_in_the_v1_document() {
    let bytes = read_anon_fixture();
    let raw = decode_raw(&bytes).expect("decode WvW fixture");
    let enc = resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, true, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None);
    let text = serde_json::to_string(&v1).expect("serializable");

    // The committed fixture is already anonymized: every account is
    // `:Anon<N>.<4 digits>`. Any account-shaped string that is NOT an Anon
    // account means an unscrubbed identity leaked through some other field.
    let account_like = regex_lite_account_matches(&text);
    for a in &account_like {
        assert!(
            a.starts_with(":Anon"),
            "unscrubbed account-shaped string {a:?} in the v1 document"
        );
    }
    assert!(!account_like.is_empty(), "the scan found nothing -- it is not actually scanning");
}

/// Minimal account-shape scanner: `:Name.1234`. Avoids adding a `regex`
/// dependency to the test suite for one pattern.
fn regex_lite_account_matches(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b':' {
            continue;
        }
        let rest = &text[i..];
        let end = rest
            .char_indices()
            .position(|(_, c)| c == '"' || c == ',')
            .unwrap_or(rest.len());
        let candidate = &rest[..end];
        let dot = candidate.rfind('.');
        if let Some(d) = dot {
            let digits = &candidate[d + 1..];
            if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) {
                out.push(candidate.to_string());
            }
        }
    }
    out
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-core --test golden no_unscrubbed_identity`
Expected: FAIL — either a compile error (`axilog-schema` is not a dev-dependency
of `axilog-core`) or a real leak.

If it is the dependency error: `axilog-core` cannot depend on `axilog-schema`
(the edge runs the other way). Move this test to
`crates/axilog-schema/tests/v1_shape.rs` instead and re-run there. Note that
relocation in the commit message.

- [ ] **Step 3: Fix whatever it caught**

If a non-Anon account-shaped string appears, find which field carries it and
either route it through the scrub or stop emitting it.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema && cargo test -p axilog-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "test: assert no unscrubbed identity survives in the 1.0 document"
```

---

### Task 12: Emit 1.0 from the CLI and SDKs

**Files:**
- Modify: `crates/axilog-cli/src/main.rs`
- Modify: `crates/axilog-node/src/lib.rs`
- Modify: `crates/axilog-py/src/lib.rs`
- Modify: `crates/axilog-html/src/lib.rs`

**Interfaces:**
- Consumes: `build_report_v1`.
- Produces: `--format json` emits the 1.0 document. `--format ei-json` is
  UNCHANGED (still built from the legacy report). `--format html` and
  `--format csv`/`table` read whichever shape needs least churn.

- [ ] **Step 1: Write the failing test**

Add to `crates/axilog-cli/tests/` (create `cli_v1.rs` if the directory has no
suitable file):

```rust
//! The CLI's native format is the 1.0 container.
use std::process::Command;

#[test]
fn native_json_output_is_the_one_point_oh_container() {
    let exe = env!("CARGO_BIN_EXE_axilog");
    let out = Command::new(exe)
        .args(["parse", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"), "--format", "json"])
        .output()
        .expect("run axilog parse");
    assert!(out.status.success(), "parse failed: {}", String::from_utf8_lossy(&out.stderr));

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["axilog"]["schema"], "1.0");
    assert!(v.get("entities").is_some(), "1.0 emits entities[]");
    assert!(v.get("coverage").is_some(), "1.0 emits coverage");
    assert!(v.get("players").is_none(), "the legacy players[] is gone from native output");
}

#[test]
fn ei_json_output_is_untouched_by_the_native_reshape() {
    let exe = env!("CARGO_BIN_EXE_axilog");
    let out = Command::new(exe)
        .args(["parse", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"), "--format", "ei-json"])
        .output()
        .expect("run axilog parse");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(v.get("players").is_some(), "ei-json keeps EI's shape");
    assert!(v.get("entities").is_none(), "ei-json is not the native container");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-cli --test cli_v1`
Expected: FAIL — `assertion failed: v["axilog"]["schema"] == "1.0"`, because
`--format json` still emits the legacy shape.

- [ ] **Step 3: Write minimal implementation**

In `crates/axilog-cli/src/main.rs`, in the `Format::Json` arm (currently
`format!("{}\n", serde_json::to_string_pretty(&report)?)`), build and
serialize the 1.0 document instead:

```rust
Format::Json => {
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &report,
        env!("CARGO_PKG_VERSION"),
        path.file_name().and_then(|s| s.to_str()),
        damage_mods.as_ref(),
    );
    format!("{}\n", serde_json::to_string_pretty(&v1)?)
}
```

Leave the `Format::EiJson` streaming path and `report` construction exactly
as they are — the adapter still consumes the legacy shape.

For `axilog-node` and `axilog-py`: find the function that serializes the
native report (`grep -n 'build_report' crates/axilog-node/src/lib.rs
crates/axilog-py/src/lib.rs`) and make the native entry point return the 1.0
document, leaving the `parse_file_ei` entry points untouched. Update the
Python `.pyi` stub and the Node `index.d.ts` to the 1.0 shape.

For `axilog-html`: it reads `Report` directly. Keep it on the LEGACY report
for now — the HTML renderer is spec #4's problem, and changing it here would
mix concerns. Add a one-line comment at its `use axilog_schema::Report;`
noting it is pinned to the legacy shape pending spec #4.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-cli --test cli_v1`
Expected: PASS, 2 tests.

- [ ] **Step 5: Verify nothing else moved**

Run: `cargo test --workspace`
Expected: PASS. Every EI golden must still pass unchanged. If one moved, the
`Format::EiJson` path was disturbed — revert that part, do not re-bless.

- [ ] **Step 6: Commit**

```bash
git add crates/
git commit -m "feat: emit the 1.0 container from --format json and the SDKs"
```

---

### Task 13: Size regression measurement

**Files:**
- Create: `crates/axilog-schema/tests/v1_size.rs`
- Modify: `docs/BENCHMARKS.md`

**Interfaces:**
- Consumes: `build_report_v1`, legacy `build_report`.
- Produces: committed byte numbers per block.

- [ ] **Step 1: Write the failing test**

Create `crates/axilog-schema/tests/v1_size.rs`:

```rust
//! Bytes per block on the committed fixture.
//!
//! Reducing payload is part of this spec's point (catalog dedup + RLE);
//! unmeasured, it will regress. The bound is deliberately loose -- this
//! guards against a 2x blowup, not against normal drift.

fn build() -> (String, String) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None);
    (
        serde_json::to_string(&legacy).expect("legacy serializes"),
        serde_json::to_string(&v1).expect("v1 serializes"),
    )
}

#[test]
fn the_one_point_oh_document_is_not_larger_than_the_legacy_one() {
    let (legacy, v1) = build();
    // Catalog dedup and RLE should make 1.0 SMALLER on the same content.
    // It carries strictly more (enemy stats, friendly players), so parity is
    // the bar rather than a strict reduction.
    assert!(
        v1.len() <= legacy.len() * 12 / 10,
        "1.0 is {} bytes vs legacy {} -- more than 20% larger means the dedup is not working",
        v1.len(),
        legacy.len()
    );
    println!("SIZE legacy={} v1={} ratio={:.3}", legacy.len(), v1.len(), v1.len() as f64 / legacy.len() as f64);
}

#[test]
fn per_block_sizes_are_reported_for_the_benchmarks_doc() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let legacy =
        axilog_schema::build_report(&enc, &metrics, "0.0.0-test", None, None, true, false, false, None);
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None);
    let v = serde_json::to_value(&v1).expect("serializable");

    for (name, value) in v["blocks"].as_object().expect("blocks").iter() {
        let size = serde_json::to_string(value).expect("stringify").len();
        println!("BLOCK {name} {size}");
    }
    let cats = serde_json::to_string(&v["catalogs"]).expect("stringify").len();
    let ents = serde_json::to_string(&v["entities"]).expect("stringify").len();
    println!("BLOCK catalogs {cats}");
    println!("BLOCK entities {ents}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axilog-schema --test v1_size -- --nocapture`
Expected: Either FAIL on the size bound (dedup not wired correctly — fix it)
or PASS with the `SIZE`/`BLOCK` lines printed.

- [ ] **Step 3: Record the numbers**

Copy the printed `SIZE` and `BLOCK` lines into a new
`## Native format 1.0 payload` section in `docs/BENCHMARKS.md`, in the same
style as the existing MPERF entries: the numbers, the fixture they came
from, and the date.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axilog-schema --test v1_size`
Expected: PASS, 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-schema/tests/v1_size.rs docs/BENCHMARKS.md
git commit -m "test(schema): measure and record 1.0 payload sizes per block"
```

---

### Task 14: Documentation and spec reconciliation

**Files:**
- Modify: `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`
- Modify: `docs/EI-PARITY.md`
- Modify: `README.md`
- Create: `docs/NATIVE-FORMAT.md`

**Interfaces:**
- Consumes: the shipped implementation.
- Produces: the consumer-facing reference for the 1.0 format.

- [ ] **Step 1: Reconcile the spec with what shipped**

In `docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`:
- Section 5: replace "the adapter reads from the new shape; its output does
  not move" with the deferral described in this plan's header, and point at
  `crates/axilog-schema/tests/v1_equivalence.rs` as the actual safety net.
- "Program context": note that the adapter re-point moved into spec #2.
- Any block name that changed during implementation: update the block table.
- Resolve the spec's own internal inconsistency: "Series encoding / The SDKs
  hide it" promises SDK hydration, while "Non-goals" defers all consumer work
  to spec #4. Move the hydration promise to spec #4 and leave the wire-format
  statement here.

- [ ] **Step 2: Write the format reference**

Create `docs/NATIVE-FORMAT.md` covering, for a consumer who has never seen
the format:
- the six top-level keys, with a complete worked example document (use a
  trimmed real one from `cargo run -- parse fixtures/wvw-small.anon.zevtc
  --format json`, not a hand-written one);
- the `role` table and how to reproduce the "squad", "enemy roster", and
  "EI targets[]" views as filters;
- the `coverage` state table and what a consumer should DO about each;
- the series envelope with a five-line decoder in JavaScript and Python;
- the 1.x compatibility rules;
- an explicit "joining across reports" note: `id` is per-report, join on
  `account`.

- [ ] **Step 3: Update the parity and readme pages**

- `docs/EI-PARITY.md`: add a short header note that native 1.0 is now the
  primary surface and `ei-json` is a frozen compat adapter, linking to
  `docs/NATIVE-FORMAT.md`. Do not restructure the parity table.
- `README.md`: update any example native-JSON snippet to the 1.0 shape, and
  link the new reference.

- [ ] **Step 4: Verify the examples are real**

Run:
```bash
cargo run --release -p axilog-cli -- parse fixtures/wvw-small.anon.zevtc --format json --output /tmp/v1.json
python3 -c "import json; d=json.load(open('/tmp/v1.json')); print(sorted(d.keys())); print(d['axilog'])"
```
Expected: the key list and meta block match what `docs/NATIVE-FORMAT.md`
documents. Fix the doc, not the output.

- [ ] **Step 5: Commit**

```bash
git add docs/ README.md
git commit -m "docs: add the native format 1.0 reference and reconcile the spec"
```

---

## Final checkpoint

- [ ] Run the full suite: `cargo test --workspace`
- [ ] Confirm every EI golden passed WITHOUT being re-blessed:
      `git diff --stat main -- crates/axilog-ei/tests/ fixtures/` must be empty
- [ ] Confirm the 1.0 document is deterministic:
      `cargo run --release -p axilog-cli -- parse fixtures/wvw-small.anon.zevtc --format json --output /tmp/a.json && cargo run --release -p axilog-cli -- parse fixtures/wvw-small.anon.zevtc --format json --output /tmp/b.json && diff /tmp/a.json /tmp/b.json`
- [ ] Confirm `docs/BENCHMARKS.md` carries the recorded payload numbers
- [ ] Re-read the spec's non-goals: no EI-only pass was absorbed (that is
      spec #2), no sectioned container was built, `axilog-html` still reads
      the legacy shape
