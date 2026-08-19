# `blocks.self_effects` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a squad-side `blocks.self_effects` carrying uptime and fused
stack timelines for the 14 conditions plus Stun (872) and Daze (833), so a
consumer can finally answer "what was on *me*".

**Architecture:** A third instantiation of the existing buff pipeline. A new
`axilog_core::analysis::self_effects` pass groups extracted buff events by
`(squad representative addr, buff id)`, runs the SAME `buffs::simulator::run`
that `simulate_boons` uses, and reduces each timeline twice — through
`buffs::uptime::compute` for the summary numbers and through
`buffs::states::to_ei_states` (with the duration clamp) for the graph. A new
`axilog_schema::v1::blocks::self_effects` reprojects that onto entity ids.
Gated by the existing `timeseries` option, and by nothing new.

**Tech Stack:** Rust (workspace crates `axilog-core`, `axilog-schema`,
`axilog-api`, `axilog-cli`, `axilog-node`, `axilog-py`), `serde`/`serde_json`,
`cargo test`.

**Spec:** `docs/superpowers/specs/2026-08-19-axilog-self-effects-design.md`

## Global Constraints

- **Sixteen buff ids, exactly.** The fourteen of
  `condition_catalog::CONDITION_BUFFS` plus Stun `872` and Daze `833`. The
  instantaneous control effects (Knockdown, Launch, Pull, Knockback, Float,
  Sink) are OUT — they are not duration buffs and are already in `blocks.cc`.
- **Stun and Daze are duration-stacking with ctor capacity 1**
  (`is_intensity = false`, `ctor_capacity = 1`). Measured off the fixture's
  own `sc::BUFF_INFO` rows: both report max stacks 1, category 1, arcdps
  stack type 5 = `BuffStackType::Force`. Elite Insights' own `buffMap`
  agrees independently (`b872`/`b833`: `"stacking": false`,
  `"Max Stack(s) 1"`).
- **One-gate block.** `blocks.self_effects` is produced entirely by one
  gated pass, so `coverage.self_effects` answers the whole question and
  `SelfEffectRow::states` is NOT an `Option`. Do not copy `blocks.boons`'
  two-gate shape.
- **No `per_source`.** Deliberately excluded (YAGNI; additive later, a
  breaking removal if it ships unused).
- **No human-readable name in any block.** All 16 ids go through
  `cats.reference_buff(id)` and resolve via `catalogs.buffs`.
- **`avg_stacks` follows the `BoonRow` convention exactly:** present for
  intensity-stacking effects, OMITTED (never a meaningless zero) for
  duration ones.
- **No simulator behaviour change.** `run_duration`'s `capacity == 1` arm
  already implements `ForceOverrideLogic` semantics. Only its stale comment
  changes.
- **Commits:** use `git commit --no-gpg-sign` (the signing helper fails in
  this environment). Do NOT push, merge, tag or publish.
- **Never `cat`/`head` `fixtures/local/wvw-postrework.ei.json`** — it is a
  57 MB single-line JSON. Read it from Rust or from `python3 -c`, printing
  small values only.

---

## File Structure

**Created:**

- `crates/axilog-core/src/analysis/control_catalog.rs` — the two-entry
  `CONTROL_EFFECTS` table, in the same `(id, name, is_intensity,
  ctor_capacity)` shape `CONDITION_BUFFS` uses. Its own module, sibling to
  `condition_catalog`, so `buffs::name`/`buffs::stacking` can compose it
  without depending on the pass.
- `crates/axilog-core/src/analysis/self_effects.rs` — the pass.
- `crates/axilog-schema/src/v1/blocks/self_effects.rs` — `SelfEffectsBlock`,
  `SelfEffectRow`, `build_self_effects`.
- `crates/axilog-schema/tests/v1_self_effects.rs` — committed-fixture tests:
  gating, catalog resolution, row shape.
- `crates/axilog-core/tests/self_effects_golden.rs` — the Elite Insights
  equality oracle, against the gitignored local capture.

**Modified:**

- `crates/axilog-core/src/analysis/mod.rs` — two `pub mod` lines.
- `crates/axilog-core/src/analysis/buffs/mod.rs` — `name()` and `stacking()`
  gain the third table.
- `crates/axilog-core/src/analysis/buffs/simulator.rs:196-197` — the stale
  "never hit" comment on the `capacity == 1` arm.
- `crates/axilog-schema/src/v1/blocks/mod.rs` — one `pub mod` line.
- `crates/axilog-schema/src/v1/envelope.rs` — `BlockName::SelfEffects`,
  `ALL: [BlockName; 16]`, the `as_str` arm, and two `15` literals in tests.
- `crates/axilog-schema/src/v1/mod.rs` — `Passes::self_effects`,
  `Blocks::self_effects`, and the build wiring.
- The ten `v1::Passes { .. }` literals that do NOT use
  `..Default::default()` and therefore break when the struct gains a field:
  `crates/axilog-api/src/lib.rs:146`, `crates/axilog-cli/src/main.rs:480`,
  `crates/axilog-node/src/lib.rs:312`, `crates/axilog-py/src/lib.rs:160`,
  `crates/axilog-py/src/lib.rs:318`,
  `crates/axilog-schema/tests/common/mod.rs:120`,
  `crates/axilog-schema/tests/v1_shape.rs:90`,
  `crates/axilog-schema/tests/v1_size.rs:213`,
  `crates/axilog-schema/tests/v1_equivalence.rs:86`,
  `crates/axilog-ei/tests/mstream_streaming_identity.rs:128`.
- `crates/axilog-schema/tests/v1-keyset.golden.txt` — regenerated.
- `crates/axilog-node/types.d.ts`, `crates/axilog-py/axilog.pyi` — the stubs
  `v1_sdk_stubs.rs` gates.
- `docs/NATIVE-FORMAT.md`, `docs/CHANGELOG.md`.

---

## Task 1: The control-effect catalog, and the two lookups that compose it

**Files:**
- Create: `crates/axilog-core/src/analysis/control_catalog.rs`
- Modify: `crates/axilog-core/src/analysis/mod.rs`
- Modify: `crates/axilog-core/src/analysis/buffs/mod.rs` (`name`, `stacking`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `axilog_core::analysis::control_catalog::STUN: u32` (= 872),
    `::DAZE: u32` (= 833)
  - `axilog_core::analysis::control_catalog::CONTROL_EFFECTS:
    [(u32, &'static str, bool, u32); 2]`
  - `axilog_core::analysis::buffs::name(id: u32) -> Option<&'static str>`
    now resolves 872/833
  - `axilog_core::analysis::buffs::stacking(id: u32) -> (bool, Option<u32>)`
    now returns `(false, Some(1))` for 872/833

- [ ] **Step 1: Write the failing tests**

Create `crates/axilog-core/src/analysis/control_catalog.rs` containing ONLY
its test module for now, so the tests fail to compile against a missing
table:

```rust
//! The duration-stacking CONTROL-effect catalog -- Stun and Daze.
//!
//! ## Why these two ids need a table of their own
//!
//! Elite Insights classifies both as `BuffClassification.Other`, not
//! `Condition`, so neither appears in `CommonBuffs.Conditions` and neither
//! is in this project's [`crate::analysis::condition_catalog::CONDITION_BUFFS`].
//! They are also not boons. Before this table there was no id table in this
//! repo that carried them at all, which is exactly why the squad-side CC
//! lanes downstream were permanently empty.
//!
//! The remaining control effects -- Knockdown, Launch, Pull, Knockback,
//! Float, Sink -- are deliberately ABSENT. They are instantaneous, not
//! duration buffs: they produce no apply/remove pair and so no stack
//! timeline exists to build. `analysis::cc` already counts them, which is
//! the correct shape for an instantaneous effect.
//!
//! ## The two values, measured rather than guessed
//!
//! `is_intensity = false` and `ctor_capacity = 1` for both, read off
//! `sc::BUFF_INFO` in `fixtures/wvw-small.anon.zevtc` (build 20260114) and
//! calibrated against ids whose classification is already known: every
//! known intensity id reports arcdps stack type 4 or 0, every known
//! duration id reports 1, and these two report 5 --
//! [`crate::analysis::buffs::BuffStackType::Force`], whose
//! `is_intensity()` is false. arcdps reports a max-stacks of 1 for both,
//! so the table and the log agree and the fallback can never contradict
//! the log. Elite Insights' own `buffMap` agrees independently: `b872` and
//! `b833` are `"stacking": false` with `"Max Stack(s) 1"`.

pub const STUN: u32 = 872;
pub const DAZE: u32 = 833;

/// `(skill id, display name, is_intensity, ctor capacity)` -- the same
/// four-tuple shape [`crate::analysis::condition_catalog::CONDITION_BUFFS`]
/// carries, so one lookup can scan both tables.
pub const CONTROL_EFFECTS: [(u32, &str, bool, u32); 2] =
    [(DAZE, "Daze", false, 1), (STUN, "Stun", false, 1)];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::buffs::BOON_IDS;
    use crate::analysis::condition_catalog::CONDITION_BUFFS;

    #[test]
    fn the_table_is_sorted_deduplicated_and_well_formed() {
        let ids: Vec<u32> = CONTROL_EFFECTS.iter().map(|&(id, _, _, _)| id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ids, "CONTROL_EFFECTS must be ascending");
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no duplicate ids");
        for &(id, name, _, cap) in CONTROL_EFFECTS.iter() {
            assert!(!name.is_empty(), "{id} needs a display name");
            assert!(cap > 0, "{id} needs a positive capacity");
        }
    }

    /// Both entries are duration-stacking with capacity 1 -- the measured
    /// values this table exists to record. A silent flip to intensity would
    /// make every Stun timeline report a raw stack count where Elite
    /// Insights reports 0/1.
    #[test]
    fn stun_and_daze_are_duration_stacking_with_capacity_one() {
        for &(id, _, is_intensity, cap) in CONTROL_EFFECTS.iter() {
            assert!(!is_intensity, "{id} is duration-stacking (BuffStackType::Force)");
            assert_eq!(cap, 1, "{id} has ctor capacity 1");
        }
    }

    /// The three id tables must stay pairwise disjoint. A duplicate would
    /// make a composed lookup's answer depend on scan order.
    #[test]
    fn the_control_table_is_disjoint_from_the_boon_and_condition_tables() {
        for &(id, _, _, _) in CONTROL_EFFECTS.iter() {
            assert!(
                !CONDITION_BUFFS.iter().any(|&(cid, _, _, _)| cid == id),
                "control effect {id} must not be in the condition catalog"
            );
            assert!(
                !BOON_IDS.iter().any(|&(bid, _, _)| bid == id),
                "control effect {id} must not be in the boon table"
            );
        }
    }
}
```

Then add the failing lookup tests to
`crates/axilog-core/src/analysis/buffs/mod.rs`, inside the EXISTING
`mod stacking_tests` and `mod name_tests` blocks:

```rust
    // -> in `mod stacking_tests`
    #[test]
    fn a_control_effect_resolves_through_the_control_table() {
        // Neither id is in the condition catalog (EI classifies both
        // `Other`) nor in the damage-mod stack table, so before the third
        // table both silently defaulted to `(false, None)` -- duration with
        // NO capacity, which is the shape of an unknown id, not of a
        // measured one.
        assert_eq!(stacking(crate::analysis::control_catalog::STUN), (false, Some(1)));
        assert_eq!(stacking(crate::analysis::control_catalog::DAZE), (false, Some(1)));
    }

    // -> in `mod name_tests`
    #[test]
    fn resolves_a_control_effect_name() {
        assert_eq!(name(crate::analysis::control_catalog::STUN), Some("Stun"));
        assert_eq!(name(crate::analysis::control_catalog::DAZE), Some("Daze"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p axilog-core control_catalog 2>&1 | tail -20`
Expected: FAIL — `error[E0433]: failed to resolve: could not find
control_catalog in analysis` (the module is not declared yet).

- [ ] **Step 3: Declare the module**

In `crates/axilog-core/src/analysis/mod.rs`, add the declaration in
alphabetical position among the existing `pub mod` lines (it sorts between
`contribution` and `damage`):

```rust
pub mod control_catalog;
```

- [ ] **Step 4: Extend `buffs::name` to compose the third table**

In `crates/axilog-core/src/analysis/buffs/mod.rs`, replace the body of
`name` (currently a two-table `.or_else` chain) with a three-table one, and
update its doc comment's "whichever of the two tracked name tables" wording:

```rust
/// Display name for a buff id, from whichever of the three tracked name
/// tables carries it: the 12 [`BOON_IDS`], the 14
/// [`crate::analysis::condition_catalog::CONDITION_BUFFS`], or the 2
/// [`crate::analysis::control_catalog::CONTROL_EFFECTS`].
///
/// Added for the native-format-1.0 buff catalog (NFCAT Task 4), which needs
/// a name for an arbitrary referenced buff id without owning a second copy
/// of any table -- this project has no crate-wide buff catalog (see
/// `damage_mods::catalog::buff_stack`'s module doc), so composing the
/// existing name-carrying tables is the calibration-safe option: no table's
/// data is duplicated, only looked up.
pub fn name(id: u32) -> Option<&'static str> {
    BOON_IDS
        .iter()
        .find(|&&(i, _, _)| i == id)
        .map(|&(_, n, _)| n)
        .or_else(|| {
            crate::analysis::condition_catalog::CONDITION_BUFFS
                .iter()
                .find(|&&(i, _, _, _)| i == id)
                .map(|&(_, n, _, _)| n)
        })
        .or_else(|| {
            crate::analysis::control_catalog::CONTROL_EFFECTS
                .iter()
                .find(|&&(i, _, _, _)| i == id)
                .map(|&(_, n, _, _)| n)
        })
}
```

- [ ] **Step 5: Extend `buffs::stacking` to consult the third table**

In the same file, insert a second lookup into `stacking`, immediately after
the `CONDITION_BUFFS` block and before the `stack_info` fallback, and extend
that function's numbered doc comment with the new step:

```rust
    if let Some(&(_, _, is_intensity, capacity)) =
        crate::analysis::control_catalog::CONTROL_EFFECTS.iter().find(|&&(i, _, _, _)| i == id)
    {
        return (is_intensity, Some(capacity));
    }
```

The doc comment's numbered list becomes:

```rust
/// 1. [`crate::analysis::condition_catalog::CONDITION_BUFFS`] -- ... (unchanged text)
/// 2. [`crate::analysis::control_catalog::CONTROL_EFFECTS`] -- Stun and
///    Daze, which appear in NO other table in this repo: EI classifies both
///    `Other`, and the damage-modifier stack table does not carry them. A
///    miss here fell through to step 4's `(false, None)`, which is the
///    answer for an UNKNOWN id -- indistinguishable from a measured one.
/// 3. [`stack_type_for`] (boons, auras, forms, ...) -- (was step 2)
/// 4. Otherwise `(false, None)` -- (was step 3)
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p axilog-core --lib control_catalog stacking_tests name_tests 2>&1 | tail -20`
Expected: PASS, 7 tests (3 catalog + 5 stacking + 4 name, minus overlaps —
the exact count is whatever the filter selects; every selected test passes).

- [ ] **Step 7: Run the whole core suite for regressions**

Run: `cargo test -p axilog-core 2>&1 | tail -30`
Expected: every test passes. `stacking` and `name` are consumed by the
native buff catalog, so a regression surfaces here.

- [ ] **Step 8: Commit**

```bash
git add crates/axilog-core/src/analysis/control_catalog.rs \
        crates/axilog-core/src/analysis/mod.rs \
        crates/axilog-core/src/analysis/buffs/mod.rs
git commit --no-gpg-sign -m "feat(core): catalog Stun and Daze as duration effects with capacity 1"
```

---

## Task 2: The `self_effects` pass

**Files:**
- Create: `crates/axilog-core/src/analysis/self_effects.rs`
- Modify: `crates/axilog-core/src/analysis/mod.rs`
- Modify: `crates/axilog-core/src/analysis/buffs/simulator.rs:196-197`

**Interfaces:**
- Consumes: `control_catalog::CONTROL_EFFECTS` (Task 1).
- Produces:
  - `axilog_core::analysis::self_effects::SelfEffects` with
    `pub uptime: BTreeMap<(u64, u32), BoonUptime>` and
    `pub states: BTreeMap<(u64, u32), StateTimeline>`
  - `axilog_core::analysis::self_effects::build(raw: &RawLog, enc: &Encounter) -> SelfEffects`
  - `axilog_core::analysis::self_effects::build_with_registry(raw: &RawLog, registry: &InstidRegistry, enc: &Encounter) -> SelfEffects`
  - `axilog_core::analysis::self_effects::effect_kind(id: u32) -> Option<(bool, u32)>`
    — `(is_intensity, ctor_capacity)`, the ONE lookup spanning both tables
  - `axilog_core::analysis::self_effects::effect_ids() -> BTreeSet<u32>` — the 16

- [ ] **Step 1: Write the failing tests**

Create `crates/axilog-core/src/analysis/self_effects.rs` with the module doc,
a `use` block, and the test module below — no implementation yet:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::condition_catalog::{BLEEDING, CHILLED};
    use crate::analysis::control_catalog::{DAZE, STUN};
    use crate::evtc::decode_raw;
    use crate::model::resolve;

    #[test]
    fn the_id_set_is_exactly_sixteen_and_spans_both_tables() {
        let ids = effect_ids();
        assert_eq!(ids.len(), 16, "14 conditions + Stun + Daze");
        assert!(ids.contains(&STUN));
        assert!(ids.contains(&DAZE));
        assert!(ids.contains(&BLEEDING));
        // A boon must never be swept in: Might (740) sits numerically
        // between Vulnerability (738) and Weakness (742), the exact id any
        // range-based shortcut would catch.
        assert!(!ids.contains(&740), "Might is not a self effect");
    }

    #[test]
    fn every_tracked_id_resolves_to_a_kind_and_capacity() {
        for id in effect_ids() {
            assert!(effect_kind(id).is_some(), "{id} must resolve in one of the two tables");
        }
        assert_eq!(effect_kind(STUN), Some((false, 1)));
        assert_eq!(effect_kind(DAZE), Some((false, 1)));
        assert_eq!(effect_kind(BLEEDING), Some((true, 1500)));
        assert_eq!(effect_kind(CHILLED), Some((false, 5)));
        assert_eq!(effect_kind(740), None, "a boon is not a self effect");
    }

    /// arcdps's own reported capacity wins over the table's ctor value,
    /// the same preference order `target_conditions::capacity_and_kind`
    /// and `simulate_boons_with_inputs` both use.
    #[test]
    fn capacity_prefers_the_arcdps_reported_value_over_the_table() {
        let none: BTreeMap<u32, u32> = BTreeMap::new();
        assert_eq!(capacity_and_kind(&none, BLEEDING), (1500, true));
        let reported: BTreeMap<u32, u32> = [(BLEEDING, 99u32)].into_iter().collect();
        assert_eq!(capacity_and_kind(&reported, BLEEDING), (99, true));
    }

    fn fixture() -> SelfEffects {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
                .expect("read committed fixture");
        let raw = decode_raw(&bytes).expect("decode fixture");
        let enc = resolve(&raw);
        build(&raw, &enc)
    }

    /// The committed fixture carries all 16 ids on squad players --
    /// measured with a throwaway probe before this plan was written: Stun
    /// on 3 players, Daze on 8, Taunt on 2 (the thinnest three). A pass
    /// that silently emitted nothing for the two control effects -- the
    /// whole reason this block exists -- cannot go green.
    #[test]
    fn the_committed_fixture_produces_rows_for_every_tracked_id_including_stun_and_daze() {
        let out = fixture();
        assert!(!out.states.is_empty(), "the fixture must produce timelines");
        let ids: BTreeSet<u32> = out.states.keys().map(|&(_, id)| id).collect();
        for id in effect_ids() {
            assert!(ids.contains(&id), "no row for tracked id {id}");
        }
    }

    /// `uptime` and `states` are two reductions of ONE simulation, so they
    /// must have exactly the same keys. A divergence would mean a consumer
    /// could read a timeline with no uptime, or an uptime with no graph.
    #[test]
    fn uptime_and_states_carry_identical_keys() {
        let out = fixture();
        let u: BTreeSet<(u64, u32)> = out.uptime.keys().copied().collect();
        let s: BTreeSet<(u64, u32)> = out.states.keys().copied().collect();
        assert_eq!(u, s);
    }

    /// Duration effects are clamped to 0/1 so the graph means what Elite
    /// Insights' means; intensity effects keep their real stack count.
    /// Most of these 16 ids are duration-stacking, so getting this wrong
    /// would be visible everywhere.
    #[test]
    fn duration_effects_are_clamped_to_zero_or_one_and_intensity_ones_are_not() {
        let out = fixture();
        let mut saw_intensity_above_one = false;
        for (&(_, id), timeline) in &out.states {
            let (is_intensity, _) = effect_kind(id).expect("tracked id resolves");
            for &(_, stacks) in timeline {
                if is_intensity {
                    saw_intensity_above_one |= stacks > 1;
                } else {
                    assert!(stacks <= 1, "duration effect {id} reported {stacks} stacks");
                }
            }
        }
        assert!(
            saw_intensity_above_one,
            "the fixture must exercise the unclamped intensity branch too"
        );
    }

    /// Every timeline opens with the mandatory leading `[0, 0]` pair and
    /// carries at least one real transition -- a timeline that never
    /// leaves 0 carries no information and is dropped rather than emitted,
    /// the same rule `target_conditions` applies.
    #[test]
    fn every_emitted_timeline_is_nontrivial_and_starts_at_zero() {
        let out = fixture();
        for (key, timeline) in &out.states {
            assert_eq!(timeline.first(), Some(&(0u64, 0u32)), "{key:?} must open with [0, 0]");
            assert!(timeline.len() >= 2, "{key:?} must carry a real transition");
        }
    }

    /// Timelines are keyed by the account's REPRESENTATIVE address, so a
    /// relogged player stays one row rather than splitting across the
    /// addresses each login produced.
    #[test]
    fn rows_are_keyed_by_representative_addresses_only() {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
                .expect("read committed fixture");
        let raw = decode_raw(&bytes).expect("decode fixture");
        let enc = resolve(&raw);
        let out = build(&raw, &enc);
        let reps: BTreeSet<u64> = enc.players.iter().map(|p| p.agent_addr).collect();
        for &(addr, _) in out.states.keys() {
            assert!(reps.contains(&addr), "{addr} is not a squad representative address");
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p axilog-core self_effects 2>&1 | tail -20`
Expected: FAIL — the module is not declared, so nothing compiles
(`error[E0583]: file not found for module` once declared, or the filter
matches nothing until it is).

- [ ] **Step 3: Declare the module**

In `crates/axilog-core/src/analysis/mod.rs`, add in alphabetical position
(between `rotation` and `skill_damage`):

```rust
pub mod self_effects;
```

- [ ] **Step 4: Write the pass**

Prepend this above the test module in
`crates/axilog-core/src/analysis/self_effects.rs`:

```rust
//! Squad-side condition and control-effect uptime + stack timelines --
//! `blocks.self_effects`.
//!
//! ## The gap this closes
//!
//! Elite Insights carries every buff a player HELD, boons and conditions
//! alike, in one `buffUptimes` array per player. This project splits them by
//! direction and by family, and until now the split left a hole:
//!
//! | | who holds it | which ids | timelines |
//! |---|---|---|---|
//! | `buffs::states` | squad players | the 12 `BOON_IDS` | yes |
//! | `target_conditions` | ENEMIES | the 14 `CONDITION_BUFFS` | `per_source` only |
//! | here | **squad players** | **conditions + Stun + Daze** | **yes** |
//!
//! Nothing answered "what was on ME". `analysis::cc` is not a substitute:
//! it tests `result == CROWD_CONTROL` and yields event COUNTS and summed
//! durations, a genuinely different measurement that cannot be turned into
//! a stack timeline.
//!
//! ## Mechanics: the boon machinery, re-pointed again
//!
//! `target_conditions`'s module doc describes this pipeline with two
//! substitutions; this is the third instantiation, with the same two:
//!
//! | | boons | target_conditions | here |
//! |---|---|---|---|
//! | id table | `buffs::BOON_IDS` (12) | `CONDITION_BUFFS` (14) | conditions + control (16) |
//! | owner scope | squad `Player::agent_addr` | enemy `Enemy::id` | squad `Player::agent_addr` |
//!
//! Event extraction (`buffs::events::extract_buff_events_with_registry`),
//! capacity extraction (`events::extract_buff_capacities`), the stack
//! machine (`buffs::simulator::run`), the uptime integral
//! (`buffs::uptime::compute`) and the EI reshaping
//! (`buffs::states::to_ei_states`) are all the SAME code, called with
//! different inputs -- so a condition timeline on a player and a boon
//! timeline on the same player can never disagree about simulator
//! semantics.
//!
//! This is the [`buffs::simulate_boons`] path, NOT the
//! `generation::run_segments` path `target_conditions` uses: this block
//! emits a FUSED total and no `per_source`, and the fused total is exactly
//! what the stack machine produces. `buffs::states::build` itself cannot be
//! reused, because it consumes `metrics.boons` -- already-simulated
//! timelines that exist only for the 12 boons.
//!
//! **Capacity 1 is new here.** Stun and Daze are the first buffs this
//! project simulates whose capacity is 1, which reaches an arm of
//! `simulator::run_duration` no boon and no condition ever did (the boons'
//! minimum real capacity is 5, the conditions' 3). That arm already
//! implements the right semantics -- see its comment.
//!
//! **Standalone, NOT wired into `analyze()`** -- opt-in like
//! `buffs::states` and `target_conditions`, and gated by every caller on
//! `--timeseries`.

use crate::analysis::buffs::events::BuffEvent;
use crate::analysis::buffs::states::{self, StateTimeline};
use crate::analysis::buffs::{events, simulator, uptime, BoonTimeline, BoonUptime};
use crate::analysis::condition_catalog::CONDITION_BUFFS;
use crate::analysis::control_catalog::CONTROL_EFFECTS;
use crate::analysis::damage::InstidRegistry;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// One log's squad-side effect uptime and stack timelines, both keyed by
/// `(player representative addr, buff id)`.
///
/// Two reductions of ONE simulation, deliberately: the summary numbers and
/// the graph must describe the same held stacks, and the only way to
/// guarantee that is to derive both from the same timeline rather than run
/// the machine twice.
#[derive(Debug, Clone, Default)]
pub struct SelfEffects {
    /// The fight-long uptime summary, `buffUptimes[].buffData[0]`'s two
    /// numbers.
    pub uptime: BTreeMap<(u64, u32), BoonUptime>,
    /// The fused stack timeline, `buffUptimes[].states`. Duration effects
    /// are clamped to 0/1 (see [`build_with_registry`]).
    pub states: BTreeMap<(u64, u32), StateTimeline>,
}

/// `(is_intensity, ctor capacity)` for a tracked effect id, from whichever
/// of the two tables carries it, or `None` for an id this pass does not
/// track.
///
/// This is the ONE lookup spanning both tables, and it is `pub` so the
/// schema reprojection can ask the same question rather than re-deriving
/// the answer from a third place. It deliberately does NOT reuse
/// `target_conditions::capacity_and_kind`, whose `.expect` panics on any id
/// outside `CONDITION_BUFFS`, nor `simulator::capacity_for`, whose `_ => 5`
/// arm is documented as unreachable for the ids it knows and would silently
/// give Stun a capacity of 5.
pub fn effect_kind(id: u32) -> Option<(bool, u32)> {
    CONDITION_BUFFS
        .iter()
        .chain(CONTROL_EFFECTS.iter())
        .find(|&&(i, _, _, _)| i == id)
        .map(|&(_, _, is_intensity, capacity)| (is_intensity, capacity))
}

/// The 16 buff ids this pass tracks, ascending.
pub fn effect_ids() -> BTreeSet<u32> {
    CONDITION_BUFFS
        .iter()
        .chain(CONTROL_EFFECTS.iter())
        .map(|&(id, _, _, _)| id)
        .collect()
}

/// `(arcdps-reported-or-table capacity, is_intensity)` for one tracked id.
/// arcdps's own `sc::BUFF_INFO` row wins where the log carries one -- the
/// same preference order `simulate_boons_with_inputs` and
/// `target_conditions::capacity_and_kind` both use, for the reason
/// MBUFFSIM measured: several real capacities sit far above the static
/// tables' values.
fn capacity_and_kind(capacities: &BTreeMap<u32, u32>, id: u32) -> (u32, bool) {
    let (is_intensity, ctor_capacity) =
        effect_kind(id).expect("capacity_and_kind is only called with an id from `effect_ids`");
    (capacities.get(&id).copied().unwrap_or(ctor_capacity), is_intensity)
}

/// Build every squad player's effect uptime and stack timelines.
pub fn build(raw: &RawLog, enc: &Encounter) -> SelfEffects {
    build_with_registry(raw, &InstidRegistry::build(raw), enc)
}

/// [`build`] against a caller-supplied, already-built [`InstidRegistry`] --
/// the standard threading convention (see
/// [`crate::analysis::damage::accumulate_pet_credit_with_registry`]).
pub fn build_with_registry(
    raw: &RawLog,
    registry: &InstidRegistry,
    enc: &Encounter,
) -> SelfEffects {
    let ids = effect_ids();
    let all = events::extract_buff_events_with_registry(raw, registry, &ids);
    let capacities = events::extract_buff_capacities(raw, &ids);

    // Every login's address folds onto the account's representative, so a
    // relogged player stays one row -- the same fold
    // `simulate_boons_with_inputs` applies, for the same reason.
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();

    let mut grouped: BTreeMap<(u64, u32), Vec<BuffEvent>> = BTreeMap::new();
    for &e in &all {
        // Squad-only scope: an event whose OWNER is not a known squad addr
        // (an enemy holding the same condition, an NPC, an addr absent from
        // the agent table) is dropped here. The enemy side is
        // `target_conditions`' job.
        let Some(&rep) = addr_to_rep.get(&e.owner) else { continue };
        grouped.entry((rep, e.buff_id)).or_default().push(e);
    }

    let log_start = raw.log_start_ms();
    let log_end = raw.events.last().map(|e| e.time).unwrap_or(0);

    let mut out = SelfEffects::default();
    for (key, evs) in grouped {
        let (capacity, is_intensity) = capacity_and_kind(&capacities, key.1);
        let timeline = BoonTimeline { states: simulator::run(evs, capacity, is_intensity, log_end) };
        // Clamped for the GRAPH only, never for the uptime integral: the
        // clamp is a presentation rule GW2EI applies to duration buffs'
        // step function, while `presence_pct` is already a 0/1 measure and
        // `avg_stacks` is meaningless for a duration buff (the schema omits
        // it). `buffs::states::build` applies the same clamp at the same
        // point, driven by the same `is_intensity`.
        let clamp = !is_intensity;
        let steps = timeline.states.iter().map(move |&(t, v)| (t, if clamp { v.min(1) } else { v }));
        let ei_states = states::to_ei_states(steps, log_start);
        // A timeline that never leaves 0 is the mandatory leading pair and
        // nothing else -- no information, so it is dropped rather than
        // emitted, the same rule `target_conditions` applies. Both maps are
        // written together so their key sets cannot diverge.
        if ei_states.len() < 2 {
            continue;
        }
        out.uptime.insert(key, uptime::compute(&timeline, log_start, log_end));
        out.states.insert(key, ei_states);
    }
    out
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p axilog-core self_effects 2>&1 | tail -25`
Expected: PASS, 8 tests.

- [ ] **Step 6: Correct the now-stale simulator comment**

`crates/axilog-core/src/analysis/buffs/simulator.rs`, in `run_duration`'s
at-capacity branch, currently reads:

```rust
                } else {
                    // capacity == 1 edge case; never hit for the 12 tracked
                    // boons (minimum real capacity is 5).
                    stack[0] = duration_ms;
                    0
                };
```

Replace those two comment lines with:

```rust
                } else {
                    // capacity == 1: unreachable for the 12 tracked boons
                    // (minimum real capacity 5) and for the 14 conditions
                    // (minimum 3), but LIVE since
                    // `analysis::self_effects` -- Stun and Daze are
                    // capacity 1, both by table and by arcdps's own
                    // `sc::BUFF_INFO` row.
                    //
                    // The unconditional overwrite is correct for them and
                    // not merely a fallback: both are
                    // `BuffStackType::Force`, whose `ForceOverrideLogic`
                    // has a new application REPLACE the active stack
                    // instead of being compared against it, and whose
                    // `IsFull => stacks.Count == 1` caps it at one stack
                    // regardless of the catalogued capacity. So the
                    // capacity-1 arm and Force semantics coincide exactly,
                    // and `run_segments`/`run_duration` need no notion of
                    // stack type to get Stun right.
                    stack[0] = duration_ms;
                    0
                };
```

- [ ] **Step 7: Run the whole core suite**

Run: `cargo test -p axilog-core 2>&1 | tail -30`
Expected: every test passes (the comment change is inert; this confirms the
new module broke nothing).

- [ ] **Step 8: Commit**

```bash
git add crates/axilog-core/src/analysis/self_effects.rs \
        crates/axilog-core/src/analysis/mod.rs \
        crates/axilog-core/src/analysis/buffs/simulator.rs
git commit --no-gpg-sign -m "feat(core): add the squad-side self-effects pass"
```

---

## Task 3: The `blocks.self_effects` schema surface and every caller

This task is one atomic compile unit: adding a field to `Passes` breaks
every struct literal that does not use `..Default::default()`, so the block,
the envelope entry, the wiring and all ten literal sites land together.

**Files:**
- Create: `crates/axilog-schema/src/v1/blocks/self_effects.rs`
- Modify: `crates/axilog-schema/src/v1/blocks/mod.rs`
- Modify: `crates/axilog-schema/src/v1/envelope.rs`
- Modify: `crates/axilog-schema/src/v1/mod.rs`
- Modify: `crates/axilog-api/src/lib.rs:135-160`
- Modify: `crates/axilog-cli/src/main.rs:480`
- Modify: `crates/axilog-node/src/lib.rs:312`
- Modify: `crates/axilog-py/src/lib.rs:160`, `:318`
- Modify: `crates/axilog-schema/tests/common/mod.rs:120`
- Modify: `crates/axilog-schema/tests/v1_shape.rs:90`
- Modify: `crates/axilog-schema/tests/v1_size.rs:213`
- Modify: `crates/axilog-schema/tests/v1_equivalence.rs:86`
- Modify: `crates/axilog-ei/tests/mstream_streaming_identity.rs:128`
- Test: `crates/axilog-schema/tests/v1_self_effects.rs` (create)

**Interfaces:**
- Consumes: `axilog_core::analysis::self_effects::{SelfEffects, build, effect_kind}` (Task 2).
- Produces:
  - `axilog_schema::v1::blocks::self_effects::{SelfEffectsBlock, SelfEffectRow, build_self_effects}`
  - `axilog_schema::v1::envelope::BlockName::SelfEffects` (`as_str() == "self_effects"`),
    `BlockName::ALL: [BlockName; 16]`
  - `axilog_schema::v1::Passes::self_effects: Option<&'a SelfEffects>`
  - `axilog_schema::v1::Blocks::self_effects: Option<SelfEffectsBlock>`

- [ ] **Step 1: Write the failing tests**

Create `crates/axilog-schema/tests/v1_self_effects.rs`:

```rust
//! `blocks.self_effects` -- the squad-side condition and control-effect
//! block.
//!
//! A ONE-gate block, unlike `blocks.boons`: uptime and timelines are
//! produced by the same gated pass and arrive together, so
//! `coverage.self_effects` settles the whole question and `states` is not
//! optional. These tests pin exactly that, plus the two conventions every
//! block here shares (ids resolve through `catalogs.buffs`, and an absent
//! `avg_stacks` means "duration-stacking", never zero).

mod common;

use axilog_schema::v1::envelope::{BlockName, CoverageState};

#[test]
fn the_gate_being_off_reports_not_computed_and_omits_the_block() {
    let (_e, _m, _l, v1) = common::fixture_report_no_gates();
    assert_eq!(
        v1.coverage.get(BlockName::SelfEffects.as_str()),
        Some(CoverageState::NotComputed),
        "the pass did not run, which is not the same as running and finding nothing"
    );
    assert!(v1.blocks.self_effects.is_none(), "a not_computed block is omitted, never empty");
}

#[test]
fn the_gate_being_on_reports_present_with_rows() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    assert_eq!(v1.coverage.get(BlockName::SelfEffects.as_str()), Some(CoverageState::Present));
    let block = v1.blocks.self_effects.as_ref().expect("block is carried when computed");
    assert!(!block.by_entity.is_empty(), "the committed fixture has squad conditions");
}

/// The two ids this block exists for. Measured on the committed fixture
/// before this plan was written: Stun (872) reaches 3 squad players and
/// Daze (833) reaches 8. A pass that silently emitted nothing for the
/// control effects would still light up every condition lane, so this is
/// the assertion that actually guards the change.
#[test]
fn stun_and_daze_reach_squad_entities() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    let mut stun = 0usize;
    let mut daze = 0usize;
    for (_id, rows) in block.by_entity.iter() {
        stun += usize::from(rows.contains_key(&872));
        daze += usize::from(rows.contains_key(&833));
    }
    assert!(stun > 0, "no entity carries Stun (872)");
    assert!(daze > 0, "no entity carries Daze (833)");
}

/// No block carries a human-readable name; every id must resolve through
/// `catalogs.buffs`, which is what makes the ids joinable at all.
#[test]
fn every_emitted_buff_id_resolves_in_the_buff_catalog() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for id in rows.keys() {
            let entry = v1
                .catalogs
                .buffs
                .get(id)
                .unwrap_or_else(|| panic!("buff {id} on entity {entity} is not in the catalog"));
            assert!(!entry.name.is_empty(), "buff {id} resolves to an empty name");
        }
    }
}

/// `avg_stacks` follows the `BoonRow` convention: present for
/// intensity-stacking effects, OMITTED for duration ones rather than
/// carrying a meaningless zero.
#[test]
fn avg_stacks_is_present_exactly_for_intensity_effects() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    let mut saw_intensity = false;
    let mut saw_duration = false;
    for (_entity, rows) in block.by_entity.iter() {
        for (&id, row) in rows {
            let (is_intensity, _) =
                axilog_core::analysis::self_effects::effect_kind(id).expect("tracked id");
            assert_eq!(
                row.avg_stacks.is_some(),
                is_intensity,
                "buff {id}: avg_stacks presence must follow the stacking kind"
            );
            saw_intensity |= is_intensity;
            saw_duration |= !is_intensity;
        }
    }
    assert!(saw_intensity && saw_duration, "the fixture must exercise both branches");
}

/// `states` is NOT optional here, and every emitted timeline is a real one
/// -- the one-gate argument, asserted rather than asserted-in-prose.
#[test]
fn every_row_carries_a_nontrivial_timeline() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for (id, row) in rows {
            assert_eq!(
                row.states.first(),
                Some(&(0u64, 0u32)),
                "entity {entity} buff {id} must open with [0, 0]"
            );
            assert!(row.states.len() >= 2, "entity {entity} buff {id} carries no transition");
        }
    }
}

/// Uptime is a percentage of the fight, so it cannot leave `[0, 100]`.
#[test]
fn uptime_percentages_stay_in_range() {
    let (_e, _m, _l, v1) = common::fixture_report_all_gates();
    let block = v1.blocks.self_effects.as_ref().expect("block");
    for (entity, rows) in block.by_entity.iter() {
        for (id, row) in rows {
            assert!(
                (0.0..=100.0).contains(&row.uptime_pct),
                "entity {entity} buff {id}: uptime_pct {} out of range",
                row.uptime_pct
            );
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p axilog-schema --test v1_self_effects 2>&1 | tail -20`
Expected: FAIL to compile — `no variant named SelfEffects found for enum
BlockName`.

- [ ] **Step 3: Write the block type and its builder**

Create `crates/axilog-schema/src/v1/blocks/self_effects.rs`:

```rust
//! `blocks.self_effects` -- what was on a SQUAD player: the 14 conditions
//! plus Stun and Daze, with uptime and a fused stack timeline each.
//!
//! The squad-side counterpart to `blocks.conditions` (which is enemy-side)
//! and the missing half of `blocks.boons` (which is squad-side but covers
//! only the 12 boons). `blocks.cc` is not a substitute: it counts
//! crowd-control EVENTS, a different measurement that carries no timeline.
//!
//! ## One gate, unlike `blocks.boons`
//!
//! `blocks.boons` is a two-gate block because its uptime half is computed
//! on every parse by `build_boons` while `attach_boon_states` only enriches
//! existing rows -- so `coverage.boons` answers the uptime question and
//! says nothing about the timelines. Here, uptime and states come out of
//! one gated pass and arrive together, so `coverage.self_effects` answers
//! the whole question and [`SelfEffectRow::states`] is not an `Option`.
//!
//! ## No `per_source`
//!
//! The machinery could produce it and "which enemy chained that stun" is a
//! real question, but nothing asks it today and it roughly doubles the
//! block. Additive to add later; a breaking removal if it ships unused.

use super::{ByEntity, StateTimeline};
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use axilog_core::analysis::self_effects::{effect_kind, SelfEffects};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SelfEffectsBlock {
    /// squad entity id -> buff id -> row. Two levels of real ids, like
    /// every other block -- the buff id resolves through `catalogs.buffs`.
    pub by_entity: ByEntity<BTreeMap<u32, SelfEffectRow>>,
}

impl SelfEffectsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SelfEffectRow {
    /// Percent of the fight this entity held at least one stack.
    pub uptime_pct: f64,
    /// Time-weighted mean stack count -- present for intensity-stacking
    /// effects (the 6 damaging conditions), omitted for duration ones
    /// rather than reported as a meaningless zero. The same convention
    /// [`super::support::BoonRow::avg_stacks`] follows, for the same
    /// reason: Elite Insights never populates it for a duration buff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    /// The fused stack timeline. Unconditional, per the one-gate argument
    /// in this module's doc: if the block is here at all, the pass ran.
    /// Duration effects are clamped to 0/1 upstream so the graph means what
    /// Elite Insights' means; the intensity ones carry their real count.
    pub states: StateTimeline,
}

/// Reproject the pass onto entity ids.
///
/// The pass keys `(player representative addr, buff id)`; the address joins
/// through [`EntityIndex::by_agent_addr`]. A player whose representative
/// address resolves to no entity is skipped rather than given a fabricated
/// id -- the same rule [`super::support::build_boons`] applies to the same
/// join.
///
/// `uptime` and `states` are written by the pass under identical keys, so a
/// missing uptime is a contract violation rather than a normal absence;
/// this walks `states` and skips a key with no uptime rather than emitting
/// a row with a fabricated zero.
pub fn build_self_effects(
    effects: &SelfEffects,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> SelfEffectsBlock {
    let mut by_entity: BTreeMap<u32, BTreeMap<u32, SelfEffectRow>> = BTreeMap::new();
    for (&(addr, buff_id), timeline) in &effects.states {
        let Some(entity_id) = index.by_agent_addr(addr) else { continue };
        let Some(uptime) = effects.uptime.get(&(addr, buff_id)) else { continue };
        // The same lookup the pass itself used -- asked of `axilog-core`
        // rather than re-derived here, so the omission rule for
        // `avg_stacks` cannot drift from the clamping rule for `states`.
        let Some((is_intensity, _)) = effect_kind(buff_id) else { continue };
        cats.reference_buff(buff_id);
        by_entity.entry(entity_id).or_default().insert(
            buff_id,
            SelfEffectRow {
                uptime_pct: uptime.presence_pct,
                avg_stacks: is_intensity.then_some(uptime.avg_stacks),
                states: timeline.clone(),
            },
        );
    }
    SelfEffectsBlock { by_entity: ByEntity(by_entity) }
}
```

In `crates/axilog-schema/src/v1/blocks/mod.rs`, add the declaration in
alphabetical position (after `pub mod minions;`):

```rust
pub mod self_effects;
```

- [ ] **Step 4: Add the block name to the envelope**

In `crates/axilog-schema/src/v1/envelope.rs`, four edits. Add the variant in
alphabetical position (after `Rotation`, before `Series`):

```rust
    Rotation,
    SelfEffects,
    Series,
```

Widen `ALL` and add the entry in the same position:

```rust
    pub const ALL: [BlockName; 16] = [
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
        BlockName::SelfEffects,
        BlockName::Series,
        BlockName::Support,
    ];
```

Add the `as_str` arm:

```rust
            BlockName::SelfEffects => "self_effects",
```

And update the two literals in
`block_name_enum_makes_typos_compile_errors_and_strings_stay_unique`:

```rust
        assert_eq!(BlockName::ALL.len(), 16, "all 16 known blocks are enumerated");

        let strings: BTreeSet<&'static str> = BlockName::ALL.iter().map(|b| b.as_str()).collect();
        assert_eq!(strings.len(), 16, "all block names serialize to unique strings; no duplicates allowed");
```

- [ ] **Step 5: Wire the block into `build_report_v1`**

In `crates/axilog-schema/src/v1/mod.rs`. First, extend the block import at
line 12 so the new module is in scope alongside the others:

```rust
use crate::v1::blocks::{activity, conditions, damage, defense, minions, self_effects, support};
```

Then:

(a) Add the field to `Blocks`, after `conditions`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_effects: Option<self_effects::SelfEffectsBlock>,
```

(b) Add the field to `Passes<'a>`, after `target_conditions`:

```rust
    /// `--timeseries`: per-(squad player, effect) uptime and fused stack
    /// timelines for the 14 conditions plus Stun and Daze. Lands on
    /// `blocks.self_effects`, the squad-side counterpart to
    /// `blocks.conditions`. ONE gate, unlike `boon_states`: this pass
    /// produces the whole block, so `coverage.self_effects` settles the
    /// question by itself.
    pub self_effects: Option<&'a axilog_core::analysis::self_effects::SelfEffects>,
```

(c) Add the build, immediately after the `conditions_block` stanza and
mirroring it exactly:

```rust
    // The squad-side counterpart to `conditions` above, and gated the same
    // way: the pass runs only under `--timeseries`, so its presence IS the
    // gate signal. Unlike `boons`, there is no always-on half here, which
    // is why `coverage.self_effects` answers the whole question.
    let self_effects_block = passes.self_effects.map(|effects| {
        let block = self_effects::build_self_effects(effects, &index, &mut cats);
        coverage.set(BlockName::SelfEffects, computed(block.is_empty()));
        block
    });
    if self_effects_block.is_none() {
        coverage.set(BlockName::SelfEffects, CoverageState::NotComputed);
    }
```

(d) Add it to the `Blocks { .. }` literal, after `conditions: conditions_block,`:

```rust
            self_effects: self_effects_block,
```


- [ ] **Step 6: Update the four production wiring sites**

Each runs the pass under the same `want_timeseries`/`timeseries` flag that
already gates `target_conditions`, and passes it by reference.

`crates/axilog-api/src/lib.rs` — after the `target_conditions` binding:

```rust
    let self_effects =
        want_timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
```

and in the `Passes { .. }` literal, after `target_conditions`:

```rust
            self_effects: self_effects.as_ref(),
```

`crates/axilog-node/src/lib.rs` and `crates/axilog-py/src/lib.rs` (BOTH
sites, `:160` and `:318`) take the identical two edits, with the same
`want_timeseries` flag those files already use.

`crates/axilog-cli/src/main.rs` — the flag there is named `timeseries`:

```rust
            let self_effects =
                timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
```

with `self_effects: self_effects.as_ref(),` in the literal. Place the
binding beside the existing `boon_states`/`target_conditions` bindings, and
make sure it is in scope at the `build_report_v1` call.

- [ ] **Step 7: Update the six test/example literal sites**

- `crates/axilog-schema/tests/common/mod.rs` — add
  `let self_effects = all_gates.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));`
  beside the existing `target_conditions` binding, and
  `self_effects: self_effects.as_ref(),` to the literal. This is what makes
  the new `v1_self_effects.rs` tests see a populated block under
  `fixture_report_all_gates` and an omitted one under
  `fixture_report_no_gates`.
- `crates/axilog-schema/tests/v1_shape.rs` — every gate is ON in this
  builder, so bind unconditionally:
  `let self_effects = axilog_core::analysis::self_effects::build(&raw, &enc);`
  and pass `self_effects: Some(&self_effects),`. This is the builder the
  key-set golden is generated from, so the block MUST be present here.
- `crates/axilog-schema/tests/v1_size.rs:213` (the per-block report, not the
  ratio test) — same unconditional binding and `Some(&self_effects)`, so the
  block's real byte cost gets printed.
- `crates/axilog-schema/tests/v1_equivalence.rs:86` — same unconditional
  binding and `Some(&self_effects)`.
- `crates/axilog-ei/tests/mstream_streaming_identity.rs:128` — mirror its
  `target_conditions` line:
  `let self_effects = flags.timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));`
  and `self_effects: self_effects.as_ref(),`.
- `crates/axilog-schema/tests/v1_size.rs:64` (the RATIO test) needs NO edit:
  it uses `..Default::default()`, and it deliberately excludes every
  absorbed pass so the legacy-vs-1.0 comparison stays honest. Leave it
  excluded, and do not widen its 0.88 bound.

- [ ] **Step 8: Compile the whole workspace**

Run: `cargo check --workspace --all-targets 2>&1 | tail -30`
Expected: clean. Any remaining `missing field self_effects` error names a
`Passes` literal the list above missed — fix it the same way.

- [ ] **Step 9: Run the new tests and the schema suite**

Run: `cargo test -p axilog-schema 2>&1 | tail -40`
Expected: `v1_self_effects` passes all 7 tests; `v1_coverage_states`,
`v1_shape`'s `coverage.len()` check and `envelope`'s
`coverage_starts_with_every_known_block_not_computed` all still pass (they
count blocks and are automatically extended by `ALL`). The key-set golden
test `the_full_key_set_matches_the_committed_golden` is EXPECTED TO FAIL
here — it is regenerated in Task 4. Note its failure and continue.

- [ ] **Step 10: Commit**

```bash
git add crates/axilog-schema/src/v1/blocks/self_effects.rs \
        crates/axilog-schema/src/v1/blocks/mod.rs \
        crates/axilog-schema/src/v1/envelope.rs \
        crates/axilog-schema/src/v1/mod.rs \
        crates/axilog-schema/tests/v1_self_effects.rs \
        crates/axilog-schema/tests/common/mod.rs \
        crates/axilog-schema/tests/v1_shape.rs \
        crates/axilog-schema/tests/v1_size.rs \
        crates/axilog-schema/tests/v1_equivalence.rs \
        crates/axilog-ei/tests/mstream_streaming_identity.rs \
        crates/axilog-api/src/lib.rs crates/axilog-cli/src/main.rs \
        crates/axilog-node/src/lib.rs crates/axilog-py/src/lib.rs
git commit --no-gpg-sign -m "feat(schema): add blocks.self_effects and wire it to the timeseries gate"
```

---

## Task 4: The key-set golden and the two SDK stubs

`v1_sdk_stubs.rs` reads the key-set golden and asserts every wire field name
appears in BOTH stubs, so these three files move together or CI fails.

**Files:**
- Modify: `crates/axilog-schema/tests/v1-keyset.golden.txt` (regenerated)
- Modify: `crates/axilog-node/types.d.ts`
- Modify: `crates/axilog-py/axilog.pyi`

**Interfaces:**
- Consumes: `blocks.self_effects` as serialized by Task 3.
- Produces: nothing further tasks consume.

- [ ] **Step 1: Regenerate the key-set golden**

Run:
```bash
UPDATE_GOLDEN=1 cargo test -p axilog-schema --test v1_shape the_full_key_set_matches_the_committed_golden
```

- [ ] **Step 2: Review the diff, which must be additive only**

Run: `git diff --stat crates/axilog-schema/tests/v1-keyset.golden.txt && git diff crates/axilog-schema/tests/v1-keyset.golden.txt`
Expected: exactly four ADDED lines and no removed or renamed line —
`blocks.self_effects`, `blocks.self_effects.by_entity`,
`blocks.self_effects.by_entity.<id>` and
`blocks.self_effects.by_entity.<id>.<id>` — plus the leaf rows
`...avg_stacks`, `...states` and `...uptime_pct` under that last path.
A REMOVED line is a breaking change and must be investigated, not accepted.

- [ ] **Step 3: Run the stub test to verify it fails**

Run: `cargo test -p axilog-schema --test v1_sdk_stubs 2>&1 | tail -20`
Expected: FAIL — `self_effects` is a wire field name declared in neither
stub.

- [ ] **Step 4: Add the TypeScript stub**

In `crates/axilog-node/types.d.ts`, insert after the `ConditionsBlock`
interface:

```ts
/**
 * One condition or control effect held BY a squad player: how long it was
 * up, and when.
 *
 * The squad-side counterpart to `ConditionRow` (enemy-side) and the missing
 * half of `BoonRow` (squad-side, but only the 12 boons). `CcBlock` is not a
 * substitute — it counts crowd-control events, which carries no timeline.
 */
export interface SelfEffectRow {
  /** Percent of the fight with at least one stack held. */
  uptime_pct: number
  /**
   * Time-weighted mean stack count. Present for intensity-stacking effects
   * (the 6 damaging conditions), omitted for duration ones rather than
   * reported as a meaningless zero — the same rule `BoonRow.avg_stacks`
   * follows.
   */
  avg_stacks?: number
  /**
   * The fused stack timeline. NOT optional, unlike `BoonRow.states`: this
   * whole block rides one gate, so if the block is here the timeline is.
   */
  states: StateTimeline
}

/**
 * squad entity id -> buff id -> row, for the 14 conditions plus Stun (872)
 * and Daze (833). Wholly gated on `{ timeseries: true }`, so unlike `boons`
 * its `coverage` entry does settle the question.
 */
export interface SelfEffectsBlock {
  by_entity: ByEntity<Record<string, SelfEffectRow>>
}
```

and add the field to `interface Blocks`, after `conditions?: ConditionsBlock`:

```ts
  self_effects?: SelfEffectsBlock
```

Also extend the `Coverage` doc comment's parenthesised block-name list to
include `` `"self_effects"` ``.

- [ ] **Step 5: Add the Python stub**

In `crates/axilog-py/axilog.pyi`, insert after `ConditionsBlock`:

```python
class _SelfEffectRowRequired(TypedDict):
    uptime_pct: float
    states: StateTimeline

class SelfEffectRow(_SelfEffectRowRequired, total=False):
    """One condition or control effect held BY a squad player.

    The squad-side counterpart to `ConditionRow` (enemy-side) and the
    missing half of `BoonRow` (squad-side, but only the 12 boons).
    `CcBlock` is not a substitute -- it counts crowd-control events, which
    carries no timeline.

    `avg_stacks` is present for intensity-stacking effects (the 6 damaging
    conditions) and omitted for duration ones, the same rule
    `BoonRow.avg_stacks` follows. `states` is REQUIRED, unlike
    `BoonRow.states`: this whole block rides one gate, so if the block is
    here the timeline is."""

    avg_stacks: float

class SelfEffectsBlock(TypedDict):
    """squad entity id -> buff id -> row, for the 14 conditions plus Stun
    (872) and Daze (833). Wholly gated on `timeseries=True`."""

    by_entity: Dict[str, Dict[str, SelfEffectRow]]
```

Add `self_effects: SelfEffectsBlock` to `class Blocks`, after `conditions`,
and add `"SelfEffectRow"` and `"SelfEffectsBlock"` to the `__all__` list
beside the existing `"ConditionRow"`/`"ConditionsBlock"` entries.

- [ ] **Step 6: Run the stub test to verify it passes**

Run: `cargo test -p axilog-schema --test v1_sdk_stubs 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 7: Run the whole schema suite**

Run: `cargo test -p axilog-schema 2>&1 | tail -30`
Expected: every test passes, including
`the_full_key_set_matches_the_committed_golden`.

- [ ] **Step 8: Commit**

```bash
git add crates/axilog-schema/tests/v1-keyset.golden.txt \
        crates/axilog-node/types.d.ts crates/axilog-py/axilog.pyi
git commit --no-gpg-sign -m "feat(sdk): declare self_effects in the key-set golden and both stubs"
```

---

## Task 5: The Elite Insights equality oracle

Every value computed twice and asserted to agree. Elite Insights already
carries this data in each player's `buffUptimes` — which is exactly what
makes the oracle possible — so this compares the new pass against a frozen
export of the same capture.

The reference is the gitignored local capture
`fixtures/local/wvw-postrework.{zevtc,ei.json}`. Verified present and
usable before this plan was written: 48 players, and every one of the 16
ids appears with `states` — Chilled on 44 players (372 state pairs), Stun
on 11 (37 pairs), Daze on 11 (37 pairs), Slow on 10 (28 pairs). The test
skips cleanly when the fixture is absent, the same
`AXILOG_LOCAL_FIXTURES` pattern every `*_ei_golden.rs` file uses.

**Files:**
- Create: `crates/axilog-core/tests/self_effects_golden.rs`

**Interfaces:**
- Consumes: `axilog_core::analysis::self_effects::{build, effect_ids, effect_kind}` (Task 2).
- Produces: nothing further tasks consume.

- [ ] **Step 1: Write the calibration in REPORT-ONLY mode**

The tolerances are set from measurement, not guessed — the convention
`boons_golden.rs`'s `PRESENCE_TOLERANCE_PP` doc comment establishes at
length. So the first version prints the worst observed divergence and
asserts only that the comparison is non-vacuous.

Create `crates/axilog-core/tests/self_effects_golden.rs`:

```rust
//! `analysis::self_effects` calibration against a real Elite Insights
//! export.
//!
//! Elite Insights carries every buff a player HELD in one `buffUptimes`
//! array per player -- boons, conditions and control effects alike -- with
//! `buffData[0].uptime` and a `states` step timeline per entry. That is the
//! same measurement this pass produces, split out by family, so every value
//! here is computed twice and asserted to agree.
//!
//! Reference: `fixtures/local/wvw-postrework.{zevtc,ei.json}`, gitignored
//! real capture data. Skips cleanly when absent, honouring
//! `AXILOG_LOCAL_FIXTURES` -- the same pattern every `*_ei_golden.rs` file
//! uses.
//!
//! Field mapping, from `analysis::buffs::uptime`'s module doc (verified
//! against GW2EI source there, not guessed): for a DURATION buff EI's
//! `buffData[0].uptime` is the percentage of the phase with the buff
//! active, which is this pass's `presence_pct`, and EI's `presence` field
//! is never populated. For an INTENSITY buff `uptime` is a time-weighted
//! mean stack count -- this pass's `avg_stacks` -- and `presence` is the
//! percentage. Every one of the 6 damaging conditions is intensity; the
//! other 10 tracked ids are duration.

use axilog_core::analysis::self_effects::{self, effect_ids, effect_kind};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use serde_json::Value;
use std::collections::BTreeMap;

fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

/// `(our SelfEffects, EI players[] by account key)`, or `None` when the
/// local capture is absent.
fn calibration() -> Option<(self_effects::SelfEffects, BTreeMap<u64, String>, Value)> {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let json = local_fixture("wvw-postrework.ei.json");
    let bytes = match std::fs::read(&zevtc) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {zevtc} absent (self-effects EI calibration)");
            return None;
        }
    };
    let text = match std::fs::read_to_string(&json) {
        Ok(s) => s,
        Err(_) => {
            println!("skip: {json} absent (self-effects EI calibration)");
            return None;
        }
    };
    let golden: Value = serde_json::from_str(&text).expect("parse EI export");
    let raw = decode_raw(&bytes).expect("decode capture");
    let enc = resolve(&raw);
    let ours = self_effects::build(&raw, &enc);
    // Representative agent address -> account key, the join this
    // calibration runs on. Both sides name the same accounts; EI's are
    // written without arcdps's leading colon.
    let accounts: BTreeMap<u64, String> =
        enc.players.iter().map(|p| (p.agent_addr, account_key(&p.account).to_string())).collect();
    Some((ours, accounts, golden))
}

/// One compared cell.
struct Cell {
    account: String,
    buff_id: u32,
    is_intensity: bool,
    ours: f64,
    theirs: f64,
}

/// Every (player, tracked id) EI reports, paired with our value for the
/// same key. A key EI has and we do not yields `ours = 0.0`, which is the
/// divergence the tolerance has to catch rather than skip.
fn cells(
    ours: &self_effects::SelfEffects,
    accounts: &BTreeMap<u64, String>,
    golden: &Value,
) -> Vec<Cell> {
    let ids = effect_ids();
    let by_account: BTreeMap<&str, u64> =
        accounts.iter().map(|(&addr, acc)| (acc.as_str(), addr)).collect();
    let mut out = Vec::new();
    for p in golden["players"].as_array().expect("players") {
        let account = account_key(p["account"].as_str().expect("account"));
        let Some(&addr) = by_account.get(account) else { continue };
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            let id = b["id"].as_u64().expect("buff id") as u32;
            if !ids.contains(&id) {
                continue;
            }
            let (is_intensity, _) = effect_kind(id).expect("tracked id");
            let theirs = b["buffData"][0]["uptime"].as_f64().unwrap_or(0.0);
            let mine = ours.uptime.get(&(addr, id));
            let ours_value = match (mine, is_intensity) {
                (Some(u), true) => u.avg_stacks,
                (Some(u), false) => u.presence_pct,
                (None, _) => 0.0,
            };
            out.push(Cell {
                account: account.to_string(),
                buff_id: id,
                is_intensity,
                ours: ours_value,
                theirs,
            });
        }
    }
    out
}

#[test]
fn report_worst_divergence_against_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    assert!(cells.len() > 200, "the comparison must not be vacuous, got {} cells", cells.len());

    let mut worst_duration = (0.0f64, String::new());
    let mut worst_intensity = (0.0f64, String::new());
    for c in &cells {
        let label = format!("{} buff {} ours={} theirs={}", c.account, c.buff_id, c.ours, c.theirs);
        if c.is_intensity {
            // Relative error, `boons_golden.rs`'s intensity convention.
            let rel = (c.ours - c.theirs).abs() / c.theirs.abs().max(1.0);
            if rel > worst_intensity.0 {
                worst_intensity = (rel, label);
            }
        } else {
            // Percentage points, `boons_golden.rs`'s duration convention.
            let pp = (c.ours - c.theirs).abs();
            if pp > worst_duration.0 {
                worst_duration = (pp, label);
            }
        }
    }
    println!("CELLS {}", cells.len());
    println!("WORST duration {:.6}pp  {}", worst_duration.0, worst_duration.1);
    println!("WORST intensity {:.6}rel  {}", worst_intensity.0, worst_intensity.1);

    // Per-id breakdown, so a single bad id cannot hide behind a good mean.
    let mut per_id: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
    for c in &cells {
        let err = if c.is_intensity {
            (c.ours - c.theirs).abs() / c.theirs.abs().max(1.0)
        } else {
            (c.ours - c.theirs).abs()
        };
        let e = per_id.entry(c.buff_id).or_insert((0.0, 0));
        e.0 = e.0.max(err);
        e.1 += 1;
    }
    for (id, (worst, n)) in per_id {
        println!("ID {id} cells={n} worst={worst:.6}");
    }
}
```

- [ ] **Step 2: Run it and record the measurements**

Run: `cargo test -p axilog-core --test self_effects_golden -- --nocapture 2>&1 | tail -40`
Expected: PASS, printing `CELLS`, two `WORST` lines and one `ID` line per
tracked id. WRITE DOWN the two worst values and the per-id table — they are
the input to Step 3. If the run prints `skip:` instead, the local capture is
absent: say so in the task report and stop this task here, leaving Steps 3-6
undone rather than inventing bounds.

- [ ] **Step 3: Set the tolerances from the measurement, with margin**

Add two constants at the top of the file. Fill each doc comment with the
values Step 2 actually printed — the measured worst cell, which id it was
on, and the margin the chosen bound leaves. Follow
`boons_golden.rs`'s two rules exactly: a bound is set from the measurement
WITH margin (a bound equal to the worst observed value fails on the next
log), and Elite Insights rounds every emitted number through
`Math.Round(x, 3)`, so **0.0005 is a hard floor** no simulator work can go
below while the golden is a 3-decimal JSON export.

Choose each bound as roughly 5-10x the measured worst, rounded to one
significant figure, and never below the 0.0005 floor:

```rust
/// Duration-effect uptime, in percentage points.
///
/// MEASURED on `fixtures/local/wvw-postrework.ei.json`: worst cell
/// <VALUE>pp on buff <ID> (<ACCOUNT>), across <N> duration cells. The bound
/// below leaves ~<K>x margin over that.
///
/// The floor is 0.0005pp: Elite Insights rounds every emitted number
/// through `Math.Round(x, ParserHelper.BuffDigit)` with `BuffDigit = 3`
/// (`GW2EIEvtcParser/ParserHelpers/ParserHelper.cs:24`), whose maximum
/// representation error is exactly that. A tighter bound would be asserting
/// against precision the golden does not carry.
const DURATION_TOLERANCE_PP: f64 = /* from Step 2 */;

/// Intensity-effect average stacks, relative error. Same convention
/// `boons_golden.rs`'s `INTENSITY_STACK_RELATIVE_TOLERANCE` uses.
///
/// MEASURED: worst cell <VALUE> on buff <ID> (<ACCOUNT>), across <N>
/// intensity cells. The bound leaves ~<K>x margin.
const INTENSITY_TOLERANCE_REL: f64 = /* from Step 2 */;
```

- [ ] **Step 4: Add the asserting tests**

Append to the same file:

```rust
#[test]
fn every_cell_agrees_with_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    assert!(cells.len() > 200, "the comparison must not be vacuous, got {} cells", cells.len());
    let mut failures: Vec<String> = Vec::new();
    for c in &cells {
        let ok = if c.is_intensity {
            (c.ours - c.theirs).abs() <= INTENSITY_TOLERANCE_REL * c.theirs.abs().max(1.0)
        } else {
            (c.ours - c.theirs).abs() <= DURATION_TOLERANCE_PP
        };
        if !ok {
            failures.push(format!(
                "{} buff {}: ours {} vs EI {}",
                c.account, c.buff_id, c.ours, c.theirs
            ));
        }
    }
    assert!(failures.is_empty(), "{} cells diverge:\n{}", failures.len(), failures.join("\n"));
}

/// Stun and Daze are the two ids this whole block exists for, and they are
/// the two with the fewest cells -- so they are exactly what a mean-based
/// check would hide. Measured on this capture: 11 players each, 37 state
/// pairs each. A pass that emitted nothing for them cannot go green here.
#[test]
fn stun_and_daze_are_covered_and_agree() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    for id in [872u32, 833] {
        let mine: Vec<&Cell> = cells.iter().filter(|c| c.buff_id == id).collect();
        assert!(mine.len() >= 5, "buff {id} has only {} cells to compare", mine.len());
        assert!(
            mine.iter().any(|c| c.theirs > 0.0),
            "buff {id}: the EI export reports no uptime at all, so this proves nothing"
        );
        for c in mine {
            assert!(
                (c.ours - c.theirs).abs() <= DURATION_TOLERANCE_PP,
                "buff {id} on {}: ours {} vs EI {}",
                c.account,
                c.ours,
                c.theirs
            );
        }
    }
}

/// The KEY SET, not just the values: every (player, id) Elite Insights
/// reports with real uptime must exist on our side too. A pass that
/// produced correct numbers for the keys it emitted while silently dropping
/// whole players would pass the value check above.
#[test]
fn the_key_set_matches_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    let by_account: BTreeMap<&str, u64> =
        accounts.iter().map(|(&addr, acc)| (acc.as_str(), addr)).collect();
    let mut missing: Vec<String> = Vec::new();
    for c in &cells {
        if c.theirs <= 0.0 {
            continue;
        }
        let addr = by_account[c.account.as_str()];
        if !ours.states.contains_key(&(addr, c.buff_id)) {
            missing.push(format!("{} buff {} (EI uptime {})", c.account, c.buff_id, c.theirs));
        }
    }
    assert!(missing.is_empty(), "{} EI keys have no timeline:\n{}", missing.len(), missing.join("\n"));
}
```

- [ ] **Step 5: Run the full calibration**

Run: `cargo test -p axilog-core --test self_effects_golden -- --nocapture 2>&1 | tail -40`
Expected: PASS, 4 tests. If a cell diverges beyond the bound chosen in Step
3, do NOT widen the bound to make it pass — that is the failure mode
`boons_golden.rs`'s comments document at length. Investigate the diverging
id first and report what you find.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-core/tests/self_effects_golden.rs
git commit --no-gpg-sign -m "test(core): calibrate self_effects against the Elite Insights export"
```

---

## Task 6: Documentation

**Files:**
- Modify: `docs/NATIVE-FORMAT.md`
- Modify: `docs/CHANGELOG.md`

**Interfaces:**
- Consumes: the shipped block from Tasks 3-4.
- Produces: nothing.

- [ ] **Step 1: Retitle and extend the buff-timelines section**

In `docs/NATIVE-FORMAT.md`, the section currently headed

```markdown
## Buff stack timelines — `boons`' second gate, and `conditions`
```

becomes

```markdown
## Buff stack timelines — `boons`' second gate, `conditions`, and `self_effects`
```

and gains this text after the existing `blocks.conditions` paragraph (the
one ending "...overlap rather than stack."):

```markdown
`blocks.self_effects` is the squad-side counterpart: the same 14 conditions
plus **Stun (`872`) and Daze (`833`)**, held BY a squad player rather than
put onto an enemy. It is wholly gated too, and unlike `boons` it carries
both halves — `uptime_pct`, an optional `avg_stacks`, and an unconditional
`states` — so `coverage.self_effects` settles the whole question. It has no
`per_source`.

The two control effects are here and not in `conditions` because Elite
Insights classifies them `Other`, not `Condition`; they are in this block
because a consumer asking "what crowd control landed on me, and when" needs
a timeline, and `blocks.cc` answers a different question — it counts
crowd-control *events*, with no notion of stacks over time. The
instantaneous control effects (Knockdown, Launch, Pull, Knockback, Float,
Sink) are deliberately absent from `self_effects`: they produce no
apply/remove pair, so no timeline exists to carry. `blocks.cc` counts those,
which is the right shape for them.

```json
{
  "self_effects": {
    "by_entity": {
      "22": {
        "872": { "uptime_pct": 0.232, "states": [[0, 0], [65670, 1], [66479, 0]] },
        "736": { "uptime_pct": 41.9, "avg_stacks": 3.7, "states": [[0, 0], [1204, 2], "..."] }
      }
    }
  }
}
```

`avg_stacks` is present exactly for the intensity-stacking effects (the six
damaging conditions) and omitted for the rest, the same rule `boons` rows
follow — an absent `avg_stacks` means "duration-stacking", never zero.
```

- [ ] **Step 2: Add the block to the `coverage` example**

In the same file, the `coverage` example object gains `"self_effects":
"not_computed",` (the example is the default-flags parse, and this block
rides `--timeseries`). Keep the existing alphabetical-ish ordering: place it
between `"rotation"` and `"series"`.

- [ ] **Step 3: Add the changelog entry**

In `docs/CHANGELOG.md`, add a new `## Unreleased` section above
`## v1.2.0 — 2026-08-18` (or extend one if it already exists):

```markdown
## Unreleased

### Added
- `blocks.self_effects` — what was on a SQUAD player. The 14 conditions plus
  Stun (872) and Daze (833), each with `uptime_pct`, an optional
  `avg_stacks`, and a fused `states` stack timeline. Rides the existing
  `--timeseries` gate; one gate, so `coverage.self_effects` settles the
  whole question.

  This closes a real hole rather than adding a convenience. `blocks.boons`
  covered squad players but only the 12 boons; `blocks.conditions` covered
  the 14 conditions but only on enemies; nothing answered "what was on me".
  `blocks.cc` is not a substitute — it counts crowd-control events, a
  different measurement that carries no timeline — so a consumer rendering
  hard- and soft-CC timelines had nothing to read and rendered empty lanes.

  Stun and Daze needed a new id table (`analysis::control_catalog`): Elite
  Insights classifies both `Other`, so neither appeared in any table here.
  Both are duration-stacking with capacity 1, measured off the log's own
  `sc::BUFF_INFO` rows and agreeing with EI's `buffMap`. They are also the
  first buffs this project simulates at capacity 1, which reaches an arm of
  `run_duration` that no boon (minimum capacity 5) and no condition (minimum
  3) ever did; that arm already implemented the correct `ForceOverrideLogic`
  semantics, and only its comment needed correcting.

  Calibrated against a real Elite Insights export's per-player
  `buffUptimes`: values and key set both.
```

- [ ] **Step 4: Verify no doc claim is stale**

Run: `grep -n "fifteen\|15 known blocks\|13 blocks" docs/NATIVE-FORMAT.md docs/*.md`
Expected: no hit that now miscounts. Fix any that does — the count is 16.

- [ ] **Step 5: Run the full workspace suite**

Run: `cargo test --workspace 2>&1 | tail -40`
Expected: every test passes.

- [ ] **Step 6: Commit**

```bash
git add docs/NATIVE-FORMAT.md docs/CHANGELOG.md
git commit --no-gpg-sign -m "docs: describe blocks.self_effects"
```

---

## Out of scope

The AxiPulse consumer change — reading the CC lanes from
`blocks.self_effects`, deleting the `KNOWN COVERAGE GAP` doc comment in
`src/shared/extract/timeline.ts` and flipping the test in `timeline.test.ts`
that currently pins the gap so it pins the data — needs an axilog version
bump and a dependency bump, so it is its own plan in that repo.
