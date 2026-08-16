# Phase B — Native Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four gaps that keep axibridge deriving data client-side — widen the gated per-target offensive split from 7 to 23 fields, export replay `dc` intervals, emit commander segments plus engine-computed distance scalars, and extract a real log-start wall clock.

**Architecture:** Every new number rides an existing scan. `per_target::build` already calls `hit_stats::classify` on exactly the events the 16 new offensive fields need, so Tasks 1-3 add counters to a loop that already runs. `wvw::markers::resolve_markers_and_guilds` already decodes every commander-tag assignment and removal but discards closed instances; Task 6 retains them. Nothing here adds a pass over the event stream.

**Tech Stack:** Rust (MSRV 1.74), workspace crates `axilog-core` (analysis), `axilog-schema` (native 1.0 + legacy `Report`), `axilog-ei` (compat adapter), `axilog-cli`.

**Spec:** `docs/superpowers/specs/2026-08-15-phase-b-native-gap-closure-design.md`

## Global Constraints

- **MSRV 1.74.** No newer language or std features.
- **Never run `cargo fmt --all`.** This repo is hand-formatted. Match the
  surrounding style by hand: 4-space indent, ~100 column soft wrap, doc
  comments on every new public field explaining *why* it exists, not just
  what it is.
- **Never read `fixtures/local/`.** Real logs with real account names. Use
  the committed fixtures under `crates/*/tests/`.
- **Every commit must be signed.** Prefix every `git commit` with
  `SSH_AUTH_SOCK="$HOME/.1password/agent.sock"`. Never use `--no-gpg-sign`.
  To verify a commit is signed, use `git cat-file -p HEAD | grep -c gpgsig`
  — `git log --format=%G?` returns `N` here because
  `gpg.ssh.allowedSignersFile` is unset, which means "cannot verify
  locally", not "unsigned".
- **Scope test runs** with `cargo test -p <crate> -q` while iterating. Run
  the full workspace suite once before finishing.
- **ei-json goldens must not move** except in Task 4, which is the one task
  that intentionally adds keys to the EI payload. Any other golden movement
  is a bug — investigate, do not re-bless.
- **Record every native-format change** in `docs/NATIVE-FORMAT.md` under
  §"1.x compatibility rules". Native 1.0 is malleable while its only reader
  is the in-tree adapter, so these land without a major version bump, but
  each needs an entry so a bisect can explain the key-set golden diff.
- **New arcdps state-change ordinals require a citation**, not an
  assumption. This repo's methodology is to cite the curl'd
  `arcdps/evtc/README.txt` and cross-check against GW2EI's
  `ArcDPSEnums.StateChange`. See `analysis/health`'s module doc for the
  established form of that trail.

---

### Task 1: Per-target hit-quality counters

Adds the eight hit-quality fields to the per-target offensive split. These
are the fields axibridge currently fakes from `statsAll[0]`, over a
denominator roughly twice too large.

**Files:**
- Modify: `crates/axilog-core/src/analysis/per_target.rs:78-96` (struct), `:169-174` (accumulation)
- Test: `crates/axilog-core/src/analysis/per_target.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `hit_stats::classify(e, post_era) -> Option<Classified>` and
  `hit_stats::can_crit(skillid) -> bool`, both already `pub(crate)`.
  `Classified` carries `dmg: u64`, `is_direct_hit: bool`, `is_crit: bool`,
  `is_glance: bool`, `is_against_downed: bool`.
- Produces: `PerTargetOffense` gains `direct_count: u32`,
  `direct_damage: u64`, `crit_count: u32`, `crit_damage: u64`,
  `flank_count: u32`, `glance_count: u32`, `critable_direct_count: u32`,
  `against_downed_damage: u64`. Names match `hit_stats::HitStats`'s
  whole-fight counterparts exactly, so the two are recognizably the same
  quantity at different granularity.

- [ ] **Step 1: Write the failing test**

Add to the inline `mod tests` in `crates/axilog-core/src/analysis/per_target.rs`:

```rust
    /// The eight hit-quality counters must split by target using the same
    /// `hit_stats::classify` decision the whole-fight totals already use.
    /// A crit on one target must not inflate the other target's row --
    /// which is exactly the error axibridge's `statsAll` fallback makes.
    #[test]
    fn splits_hit_quality_by_target() {
        let mut crit9 = base(1, 9);
        crit9.result = result::CRIT;
        crit9.value = 300;
        crit9.is_flanking = 1;
        let mut glance10 = base(1, 10);
        glance10.result = result::GLANCE;
        glance10.value = 50;
        let out = run(vec![crit9, glance10]);

        let t9 = &out[&(1, 9)];
        assert_eq!(t9.direct_count, 1);
        assert_eq!(t9.direct_damage, 300);
        assert_eq!(t9.crit_count, 1);
        assert_eq!(t9.crit_damage, 300);
        assert_eq!(t9.critable_direct_count, 1);
        assert_eq!(t9.flank_count, 1);
        assert_eq!(t9.glance_count, 0);

        let t10 = &out[&(1, 10)];
        assert_eq!(t10.direct_count, 1);
        assert_eq!(t10.direct_damage, 50);
        assert_eq!(t10.crit_count, 0, "target 9's crit must not leak onto target 10");
        assert_eq!(t10.glance_count, 1);
        assert_eq!(t10.flank_count, 0);
    }

    /// `against_downed_damage` is the damage pair for the count this struct
    /// already carries; EI reports both.
    #[test]
    fn accumulates_against_downed_damage_per_target() {
        let mut hit = base(1, 9);
        hit.result = result::NORMAL;
        hit.value = 120;
        hit.is_offcycle = 1;
        let out = run(vec![hit]);
        let t = &out[&(1, 9)];
        assert_eq!(t.against_downed_count, 1);
        assert_eq!(t.against_downed_damage, 120);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p axilog-core -q per_target 2>&1 | tail -20
```

Expected: FAIL — `no field 'direct_count' on type 'PerTargetOffense'` (a
compile error, which is the correct failure here; the struct does not have
these fields yet).

- [ ] **Step 3: Add the eight fields to the struct**

In `crates/axilog-core/src/analysis/per_target.rs`, extend
`PerTargetOffense` (after `connected_damage`, before
`against_downed_count`, keeping the existing field order otherwise
untouched):

```rust
    /// EI's `directDmg` count pair. Named to match
    /// `hit_stats::HitStats::direct_count` so the per-target and
    /// whole-fight versions of one quantity are recognizably the same
    /// thing. NOT the same as the schema's `connected_direct_dmg`, which
    /// measures a different quantity -- see this plan's Task 4.
    pub direct_count: u32,
    /// EI's `directDmg`.
    pub direct_damage: u64,
    /// EI's `criticalRate` numerator. Gated behind `hit_stats::can_crit`
    /// exactly as the whole-fight counter is -- GW2EI gates
    /// `crit_count`/`critable_direct_count` behind `CanCrit` but NOT
    /// `direct_count`/`flank_count`/`glance_count`.
    pub crit_count: u32,
    /// EI's `criticalDmg`.
    pub crit_damage: u64,
    /// EI's `flankingRate` numerator.
    pub flank_count: u32,
    /// EI's `glanceRate` numerator.
    pub glance_count: u32,
    /// EI's `criticalRate` DENOMINATOR -- not `direct_count`. See
    /// `hit_stats`'s module doc, `critable_direct_count` section.
    pub critable_direct_count: u32,
    /// EI's `againstDownedDamage` -- the damage pair for the
    /// `against_downed_count` this struct already carried.
    pub against_downed_damage: u64,
```

- [ ] **Step 4: Accumulate them**

In the same file, replace the accumulation block at the end of `build`'s
loop (currently the five lines from `s.connected_hits += 1;` through the
`is_against_downed` block) with:

```rust
        s.connected_hits += 1;
        s.connected_damage += c.dmg;
        // Mirrors `hit_stats::accumulate`'s direct-hit branch byte for
        // byte, minus the condition/life-leech/above-90 buckets EI does not
        // split per target. Keeping the same order and the same `can_crit`
        // gate is what makes the per-target rows sum to the whole-fight
        // totals for every enumerated target.
        if c.is_direct_hit {
            if can_crit(e.skillid) {
                s.critable_direct_count += 1;
                if c.is_crit {
                    s.crit_count += 1;
                    s.crit_damage += c.dmg;
                }
            }
            s.direct_count += 1;
            s.direct_damage += c.dmg;
            if e.is_flanking != 0 {
                s.flank_count += 1;
            }
            if c.is_glance {
                s.glance_count += 1;
            }
        }
        if c.is_against_downed {
            s.against_downed_count += 1;
            s.against_downed_damage += c.dmg;
        }
```

Add `can_crit` to the existing import at the top of the file:

```rust
use crate::analysis::hit_stats::{can_crit, classify};
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p axilog-core -q per_target 2>&1 | tail -20
```

Expected: PASS, including the pre-existing
`splits_hits_downs_kills_and_interrupts_by_target`.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-core/src/analysis/per_target.rs
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(per-target): split the eight hit-quality counters by target

Mirrors hit_stats::accumulate's direct-hit branch, including the can_crit
gate that separates critable_direct_count from direct_count. These are the
fields axibridge fakes from statsAll[0] today, over a whole-fight
denominator that counts NPCs and siege the per-target roster excludes.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Per-target mitigation outcomes

Adds the four outcome counters. `hit_stats::classify` returns `None` for
these — they are exactly the non-hit results — so they need
`defenses::classify_outcome`, which the module doc describes as
"`hit_stats::classify`'s exact era/result-byte semantics, just widened to
also surface the non-hit outcomes."

**Files:**
- Modify: `crates/axilog-core/src/analysis/per_target.rs` (struct + loop)
- Modify: `crates/axilog-core/src/analysis/defenses.rs:506` (visibility, if needed)
- Test: `crates/axilog-core/src/analysis/per_target.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `defenses::classify_outcome(e, post_era) -> Option<Outcome>`,
  already `pub(crate)`. `Outcome` has variants `Hit { .. }`, `Blocked`,
  `Evaded`, `Interrupted`, `Invulned`, `Missed`.
- Produces: `PerTargetOffense` gains `missed: u32`, `evaded: u32`,
  `blocked: u32`, `invulned: u32`.

- [ ] **Step 1: Write the failing test**

```rust
    /// The four mitigation outcomes are the rows `hit_stats::classify`
    /// returns `None` for, so they need the widened `defenses` classifier.
    /// GW2EI reports them per target (`totalDamage: 0, hits: n` rows), and
    /// without them a target a player only ever whiffed against produces no
    /// row at all.
    #[test]
    fn splits_mitigation_outcomes_by_target() {
        let mut blocked = base(1, 9);
        blocked.result = result::BLOCK;
        let mut evaded = base(1, 9);
        evaded.result = result::EVADE;
        let mut blind = base(1, 10);
        blind.result = result::BLIND;
        let mut absorb = base(1, 10);
        absorb.result = result::ABSORB;
        let out = run(vec![blocked, evaded, blind, absorb]);

        let t9 = &out[&(1, 9)];
        assert_eq!(t9.blocked, 1);
        assert_eq!(t9.evaded, 1);
        assert_eq!(t9.missed, 0);
        assert_eq!(t9.invulned, 0);
        assert_eq!(t9.connected_hits, 0, "a mitigated attempt is not a connected hit");

        let t10 = &out[&(1, 10)];
        assert_eq!(t10.missed, 1, "BLIND is EI's `missed`");
        assert_eq!(t10.invulned, 1, "ABSORB is EI's `invulned`");
    }

    /// A pair the player only ever whiffed against must still produce a
    /// row. `is_empty` drives `retain`, so it has to see the new counters.
    #[test]
    fn mitigation_only_pair_still_produces_a_row() {
        let mut blocked = base(1, 9);
        blocked.result = result::BLOCK;
        let out = run(vec![blocked]);
        assert!(out.contains_key(&(1, 9)), "a blocked-only pair must not be retained away");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p axilog-core -q per_target 2>&1 | tail -20
```

Expected: FAIL — `no field 'blocked' on type 'PerTargetOffense'`.

- [ ] **Step 3: Add the four fields**

Append to `PerTargetOffense`, after `interrupts`:

```rust
    /// EI's `missed` -- arcdps `BLIND`. From `defenses::classify_outcome`,
    /// not `hit_stats::classify`: the latter returns `None` for every
    /// non-hit outcome by design, so these four are invisible to it.
    pub missed: u32,
    /// EI's `evaded` -- arcdps `EVADE`.
    pub evaded: u32,
    /// EI's `blocked` -- arcdps `BLOCK`.
    pub blocked: u32,
    /// EI's `invulned` -- arcdps `ABSORB`/`INVERT`.
    pub invulned: u32,
```

- [ ] **Step 4: Accumulate them**

In `build`, the `let Some(c) = classify(e, post_era) else { continue };`
line currently discards every mitigated row. Replace that line and the
`let s = ...` that follows it with:

```rust
        // A mitigated attempt is not a hit, so `classify` drops it -- but
        // GW2EI counts it per target, and a pair the player only ever
        // whiffed against has no other way to produce a row. Dispatch the
        // outcome first, then fall through to the hit path.
        let s = out.entry((src, dst)).or_default();
        match classify_outcome(e, post_era) {
            Some(Outcome::Blocked) => {
                s.blocked += 1;
                continue;
            }
            Some(Outcome::Evaded) => {
                s.evaded += 1;
                continue;
            }
            Some(Outcome::Missed) => {
                s.missed += 1;
                continue;
            }
            Some(Outcome::Invulned) => {
                s.invulned += 1;
                continue;
            }
            _ => {}
        }
        let Some(c) = classify(e, post_era) else { continue };
```

Note the reordering: `out.entry(...)` now runs before the classification,
so `s` is in scope for both branches. This can insert an all-zero row for
an event both classifiers reject; the existing
`out.retain(|_, v| !v.is_empty())` at the end of `build` removes it, which
is why that line must stay.

Add the import:

```rust
use crate::analysis::defenses::{classify_outcome, Outcome};
```

If `Outcome` is not already `pub(crate)` in `defenses.rs`, make it so —
`classify_outcome` is public to the crate but returns it, so it almost
certainly already is. Check before editing:

```bash
grep -n "enum Outcome" crates/axilog-core/src/analysis/defenses.rs
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p axilog-core -q per_target 2>&1 | tail -20
```

Expected: PASS, all four per-target tests.

- [ ] **Step 6: Run the wider core suite for regressions**

```bash
cargo test -p axilog-core -q 2>&1 | tail -20
```

Expected: PASS. The `out.entry` reordering touches the shape of the output
map, so any test asserting on the pair set would catch a mistake here.

- [ ] **Step 7: Commit**

```bash
git add crates/axilog-core/src/analysis/per_target.rs crates/axilog-core/src/analysis/defenses.rs
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(per-target): count missed/evaded/blocked/invulned by target

hit_stats::classify returns None for every non-hit outcome by design, so
these four come from defenses::classify_outcome -- the same era and
result-byte semantics, widened. Without them a pair the player only ever
whiffed against produced no row at all.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Per-target applied crowd control

Adds the four CC fields. Two are a straightforward per-target key on the
existing CC scan; two require the down-contribution window to start
tracking CC *duration*, which it does not today.

**Files:**
- Modify: `crates/axilog-core/src/analysis/cc.rs:193` (visibility), `:240-287`
- Modify: `crates/axilog-core/src/analysis/contribution.rs:97-119` (struct + merge), `:329-346` (per-target fold)
- Modify: `crates/axilog-core/src/analysis/mod.rs:213` (`PlayerMetrics`)
- Modify: `crates/axilog-core/src/analysis/per_target.rs` (struct)
- Test: `crates/axilog-core/src/analysis/cc.rs`, `crates/axilog-core/src/analysis/contribution.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `cc::is_cc(e, post_era) -> bool` (already `pub(crate)`) and
  `cc::pet_credit_cc_events(..) -> impl Iterator<Item = (u64, u64, u64)>`
  yielding `(owner_addr, dst_addr, duration_ms)` — currently private, must
  become `pub(crate)`.
- Produces:
  - `PerTargetOffense` gains `applied_total: u32` and
    `applied_duration_ms: u64`, matching `CcEntity`'s existing names for
    the whole-fight versions.
  - `ContributionMetrics` gains `cc_duration_ms: u64` alongside its
    existing `cc: u32`.
  - `PlayerMetrics` gains
    `cc_per_target: BTreeMap<u64, (u32, u64)>` keyed by enemy
    representative id, holding `(count, duration_ms)`, and
    `cc_downs_contribution_per_target: BTreeMap<u64, (u32, u64)>` in the
    same shape.

- [ ] **Step 1: Write the failing test for per-target CC**

Add to the inline `mod tests` in `crates/axilog-core/src/analysis/cc.rs`:

```rust
    /// EI's `appliedCrowdControl`/`appliedCrowdControlDuration` split by
    /// target. The whole-fight versions already exist on `CcEntity`; this
    /// is the same accumulation additionally keyed by the enemy.
    #[test]
    fn splits_applied_cc_by_target() {
        // Two CC applications on enemy 9 (300ms, 200ms) and one on 10 (450ms).
        let mut players = two_enemy_player_fixture();
        let raw = raw_with_cc_on(&[(9, 300), (9, 200), (10, 450)]);
        let registry = InstidRegistry::build(&raw);
        let squad: BTreeSet<u64> = [1u64].into_iter().collect();
        let enemies: BTreeSet<u64> = [9u64, 10].into_iter().collect();
        apply_cc_with_registry(
            &mut players, &raw, &registry, &squad, &enemies, &BTreeMap::new(),
        );

        assert_eq!(players[0].cc_applied, 3, "whole-fight total is unchanged");
        assert_eq!(players[0].cc_duration_ms, 950);
        assert_eq!(players[0].cc_per_target[&9], (2, 500));
        assert_eq!(players[0].cc_per_target[&10], (1, 450));
    }
```

Write `two_enemy_player_fixture()` and `raw_with_cc_on()` as local helpers
in that `mod tests`, following the existing helpers there — read the
neighbouring CC tests (around `cc.rs:384-470`) for the exact `RawEvent`
shape a CC row needs on this era, and reuse their construction rather than
inventing one.

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p axilog-core -q splits_applied_cc_by_target 2>&1 | tail -20
```

Expected: FAIL — no field `cc_per_target` on `PlayerMetrics`.

- [ ] **Step 3: Add the field and accumulate it**

In `crates/axilog-core/src/analysis/mod.rs`, beside
`downs_contribution_per_target`:

```rust
    /// Applied crowd control split by the enemy it landed on, keyed by
    /// enemy representative id -- EI's `appliedCrowdControl` and
    /// `appliedCrowdControlDuration` in `statsTargets`. `(count,
    /// duration_ms)`, the same pair `CcEntity` carries whole-fight.
    pub cc_per_target: BTreeMap<u64, (u32, u64)>,
```

In `cc::apply_cc_with_registry`, extend both accumulation sites. The
direct loop becomes:

```rust
    for e in &raw.events {
        if is_cc(e, post_era) && squad.contains(&e.src_agent) && enemies.contains(&e.dst_agent) {
            if let Some(&i) = idx.get(&rep(e.src_agent)) {
                let dur = e.value.max(0) as u64;
                players[i].cc_applied += 1;
                players[i].cc_duration_ms += dur;
                let t = players[i].cc_per_target.entry(e.dst_agent).or_default();
                t.0 += 1;
                t.1 += dur;
            }
        }
    }
```

and the pet-credit loop stops discarding its destination:

```rust
    for (owner, dst, duration_ms) in
        pet_credit_cc_events(raw, registry, squad, enemies, friendly_team, &agent_team)
    {
        if let Some(&i) = idx.get(&rep(owner)) {
            players[i].cc_applied += 1;
            players[i].cc_duration_ms += duration_ms;
            let t = players[i].cc_per_target.entry(dst).or_default();
            t.0 += 1;
            t.1 += duration_ms;
        }
    }
```

**Enemy representative folding:** `cc_per_target` must be keyed by the
enemy *representative* id, the way `downs_contribution_per_target` is
(`contribution.rs:337`), so it lines up with `PerTargetOffense`'s key. The
CC pass does not currently receive `enemy_addr_to_rep`. Add it as a
parameter to `apply_cc_with_registry` and apply
`enemy_addr_to_rep.get(&dst).copied().unwrap_or(dst)` at both sites, then
update the call site in `analysis/mod.rs`. Do not skip this — an unfolded
key silently produces a second row for a relogged enemy.

- [ ] **Step 4: Run it to verify it passes**

```bash
cargo test -p axilog-core -q splits_applied_cc_by_target 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 5: Write the failing test for CC duration in the down-contribution window**

`ContributionMetrics` counts in-window CC applications (`cc: u32`) but not
their duration, so `appliedCrowdControlDurationDownContribution` has no
source at all. Add to `contribution.rs`'s inline `mod tests`:

```rust
    /// EI reports both a COUNT and a DURATION of crowd control credited in
    /// a down's contribution window. Only the count existed before; the
    /// duration field is new, so this is the first test that pins it.
    #[test]
    fn credits_cc_duration_in_the_down_window() {
        let m = run_window_with_cc(&[300, 200]);
        assert_eq!(m.cc, 2);
        assert_eq!(m.cc_duration_ms, 500);
    }
```

Write `run_window_with_cc()` following the existing contribution-window
test helpers in that module.

- [ ] **Step 6: Run it to verify it fails**

```bash
cargo test -p axilog-core -q credits_cc_duration_in_the_down_window 2>&1 | tail -20
```

Expected: FAIL — no field `cc_duration_ms` on `ContributionMetrics`.

- [ ] **Step 7: Add the field**

In `contribution.rs`, beside `cc`:

```rust
    /// Sum of in-window `cc::is_cc` application DURATIONS credited, in ms
    /// -- EI's `appliedCrowdControlDurationDownContribution`. The count
    /// pair (`cc` above) existed from M11; the duration did not, which is
    /// why that EI field had no source before Phase B.
    pub cc_duration_ms: u64,
```

Add `self.cc_duration_ms += other.cc_duration_ms;` to
`ContributionMetrics::merge`, and accumulate the duration wherever
`credit_window` increments `cc` — use `e.value.max(0) as u64`, matching
`cc::apply_cc_with_registry`'s own duration read exactly.

- [ ] **Step 8: Add the per-target CC contribution map**

In `analysis/mod.rs`, beside `cc_per_target`:

```rust
    /// The CC half of the down-contribution split, keyed by the DOWNED
    /// enemy's representative id -- EI's
    /// `appliedCrowdControlDownContribution` and its duration pair.
    /// `(count, duration_ms)`. The damage half is
    /// `downs_contribution_per_target` above.
    pub cc_downs_contribution_per_target: BTreeMap<u64, (u32, u64)>,
```

In `contribution.rs`'s `Direction::Outgoing` arm, extend the existing
per-target fold — it already has `target_rep` in scope:

```rust
                for (contributor, c) in credits {
                    if let Some(&i) = idx.get(&rep(contributor)) {
                        players[i].downs_contribution.merge(c);
                        *players[i]
                            .downs_contribution_per_target
                            .entry(target_rep)
                            .or_default() += c.damage;
                        let t = players[i]
                            .cc_downs_contribution_per_target
                            .entry(target_rep)
                            .or_default();
                        t.0 += c.cc;
                        t.1 += c.cc_duration_ms;
                    }
                }
```

`c` is `Copy`/cheap here already since `merge` takes it by value — if the
borrow checker objects to reading `c.cc` after `merge(c)`, read the two
values into locals before the `merge` call rather than cloning.

- [ ] **Step 9: Add the four fields to `PerTargetOffense`**

```rust
    /// EI's `appliedCrowdControl` for this target. Filled from
    /// `PlayerMetrics::cc_per_target` at the schema layer, not by this
    /// module's scan -- CC rows are dispatched by `cc::is_cc`, a different
    /// predicate from the damage classifiers above.
    pub applied_total: u32,
    /// EI's `appliedCrowdControlDuration`, ms.
    pub applied_duration_ms: u64,
    /// EI's `appliedCrowdControlDownContribution` -- the CC subset credited
    /// inside this target's down-contribution windows.
    pub applied_downs_contribution: u32,
    /// EI's `appliedCrowdControlDurationDownContribution`, ms.
    pub applied_duration_downs_contribution_ms: u64,
```

Leave them at zero in `per_target::build`; they are joined in at the schema
layer in Task 4, from the two maps this task added. Note this in a comment
at the top of `build` so a reader does not go looking for the accumulation.

- [ ] **Step 10: Run the core suite**

```bash
cargo test -p axilog-core -q 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/axilog-core/src/analysis/
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(per-target): split applied crowd control by target

Two of the four fields are the existing CC scan keyed by enemy
representative. The other two needed ContributionMetrics to start tracking
CC duration, not just count -- which is why EI's
appliedCrowdControlDurationDownContribution had no source at all before.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Surface the 16 fields through the schema and the adapter

Carries the new numbers out through the legacy `Report`, the native 1.0
document, and `to_ei_json`'s `statsTargets` split. This is the one task in
the plan that intentionally moves ei-json goldens.

**Files:**
- Modify: `crates/axilog-schema/src/lib.rs:412` (`PerTargetStatsOut`), `:1268` (build site)
- Modify: `crates/axilog-schema/src/v1/blocks/damage.rs:134` (`PerTargetDetail`) and its build site
- Modify: `crates/axilog-ei/src/lib.rs` (the `statsTargets` split — grep for `statsTargets`)
- Modify: `docs/NATIVE-FORMAT.md` (§"1.x compatibility rules")
- Test: `crates/axilog-schema/tests/v1_equivalence.rs`

**Interfaces:**
- Consumes: everything Tasks 1-3 produced —
  `PerTargetOffense`'s 22 fields (18 accumulated in that struct, 4 left
  zero), plus `PlayerMetrics::cc_per_target` and
  `PlayerMetrics::cc_downs_contribution_per_target` for the CC join.
- Produces: `PerTargetDetail` with 23 fields (`PerTargetOffense`'s 22 plus
  `downs_contribution_damage`, which comes from `PlayerMetrics` rather than
  that struct); the ei-json `statsTargets` entry filled for **15**
  previously-absent or fallback-filled keys. The 16th new native field,
  `critable_direct_count`, has no EI counterpart — it is EI's crit-rate
  DENOMINATOR, which EI itself never publishes per target. Do not invent a
  `statsTargets` key for it.

- [ ] **Step 1: Write the failing equivalence test**

Add to `crates/axilog-schema/tests/v1_equivalence.rs`:

```rust
/// Phase B item 1: the per-target split must carry the full 23-field set.
/// The inequality below is the load-bearing assertion -- summing a
/// player's per-target connected hits across enumerated targets must be
/// LESS THAN their whole-fight connected count on any log containing
/// non-enumerated targets (NPCs, guards, siege). That gap is exactly the
/// error axibridge's `statsAll[0]` fallback makes, so pinning it here is a
/// regression guard against ever reintroducing the fallback's semantics.
#[test]
fn per_target_detail_carries_the_full_field_set_and_undercounts_whole_fight() {
    let doc = build_v1_from_fixture_with_skill_damage();
    let mut checked = 0usize;
    for (id, dmg) in doc.blocks.damage.by_entity.iter() {
        let Some(hit) = doc.blocks.defense.hit_stats.by_entity.get(id) else { continue };
        let mut summed = 0u32;
        let mut saw_detail = false;
        for (_target, per) in dmg.per_target.iter() {
            let Some(d) = per.detail.as_ref() else { continue };
            saw_detail = true;
            summed += d.connected_hits;
            // Every new field must be reachable -- a field that never
            // compiles into the struct would pass a value assertion by
            // being absent, so touch each one.
            let _ = (d.direct_count, d.direct_damage, d.crit_count, d.crit_damage);
            let _ = (d.flank_count, d.glance_count, d.critable_direct_count);
            let _ = (d.against_downed_damage, d.missed, d.evaded, d.blocked, d.invulned);
            let _ = (d.applied_total, d.applied_duration_ms);
            let _ = (d.applied_downs_contribution, d.applied_duration_downs_contribution_ms);
        }
        if !saw_detail {
            continue;
        }
        checked += 1;
        assert!(
            summed <= hit.connected_count,
            "entity {id}: per-target hits {summed} exceeded whole-fight {}",
            hit.connected_count
        );
    }
    assert!(checked > 0, "fixture produced no per-target detail -- is --skill-damage on?");
}
```

Write `build_v1_from_fixture_with_skill_damage()` following the existing
fixture helpers in that file. It must set the `--skill-damage` equivalent
option, since `PerTarget.detail` is `None` otherwise. Read the top of
`v1_equivalence.rs` for how the existing helpers construct a document
before writing this one.

The exact field paths (`doc.blocks.damage.by_entity`,
`doc.blocks.defense.hit_stats.by_entity`) are the plan's best reading of
the block layout — verify them against `v1/blocks/damage.rs` and
`v1/blocks/defense.rs` and correct the test if the accessors differ. The
*assertion* is what matters, not the spelling of the path.

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p axilog-schema -q per_target_detail_carries 2>&1 | tail -20
```

Expected: FAIL — the new fields do not exist on `PerTargetDetail`.

- [ ] **Step 3: Widen `PerTargetStatsOut`**

In `crates/axilog-schema/src/lib.rs`, add the 16 fields to
`PerTargetStatsOut` using the same names as `PerTargetOffense`, and fill
them at the build site (`:1268`) from the `PerTargetOffense` value plus the
two `PlayerMetrics` CC maps, looked up by the same enemy representative key
the row is already keyed on.

Update the struct's doc comment: it currently says "~50 enemies x 8 fields
per player is simply 5x the shape of `DamageOut::per_enemy`". That number
is now 23. Restate it rather than leaving a stale figure — the gate's
justification depends on it.

- [ ] **Step 4: Widen `PerTargetDetail`**

In `crates/axilog-schema/src/v1/blocks/damage.rs`, add the same 16 fields
to `PerTargetDetail` and fill them at its build site. Update the struct's
doc comment, which currently says "the seven fields below"; it is 23 now.

Add a note to `PerTarget::detail`'s doc comment recording that the gate is
unchanged and why: the per-target *pass* is unconditional, so this is a
serialization gate only, and always-on was measured at +56.5% on the
rendered HTML report with 8 fields per pair — worse at 23.

- [ ] **Step 5: Run the schema tests**

```bash
cargo test -p axilog-schema -q 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 6: Fill the ei-json `statsTargets` split**

```bash
grep -n "statsTargets" crates/axilog-ei/src/lib.rs
```

Fill the 15 EI keys from the widened `PerTargetStatsOut` — 15, not 16:
`critable_direct_count` stays native-only, since EI has no `statsTargets`
key for it. Keep the adapter
thin — this is a field-for-field copy, no arithmetic. `criticalRate`,
`flankingRate` and `glanceRate` in EI's payload are *counts*, not rates,
despite the names; confirm against the neighbouring `statsAll` mapping in
the same file before assigning, and follow whatever convention that code
already uses.

`directDmg` is the one field needing care: it is **not** native's
`connected_direct_dmg`, which the cutover report flags as a different
quantity. It is `direct_damage` from Task 1 — the damage sum over
`is_direct_hit` rows. Write that distinction into a comment at the
assignment site.

- [ ] **Step 7: Run the ei-json goldens and inspect the diff**

```bash
cargo test -p axilog-ei -q 2>&1 | tail -30
```

Expected: FAIL, with the golden diff limited to `statsTargets` entries
gaining keys. **Read the diff before re-blessing.** Any change outside
`statsTargets` — a moved value, a changed `statsAll` number, a different
roster — is a bug in Tasks 1-3, not a golden that needs updating. If the
diff is clean, re-bless the goldens using whatever mechanism that crate's
tests document.

- [ ] **Step 8: Record the format change**

Add an entry to `docs/NATIVE-FORMAT.md` §"1.x compatibility rules"
recording that `PerTargetDetail` went from 7 to 23 fields in Phase B,
additively, within the existing `--skill-damage` gate.

- [ ] **Step 9: Run the full workspace suite**

```bash
cargo test -q --workspace 2>&1 | tail -30
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/ docs/NATIVE-FORMAT.md
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(schema): widen the per-target split to 23 fields

Carries Tasks 1-3's counters through PerTargetStatsOut, PerTargetDetail
and to_ei_json's statsTargets. The gate is unchanged: the per-target pass
is unconditional, so --skill-damage is a serialization gate only, and
always-on was measured at +56.5% on the HTML report with 8 fields per pair.

This is the one Phase B commit that intentionally moves ei-json goldens,
and only by adding statsTargets keys.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: Replay `dc` intervals

Exports the disconnected/not-yet-spawned segments. Item 3's active-position
filter depends on these, so this task precedes it.

**Files:**
- Modify: `crates/axilog-core/src/evtc/event.rs` (add `SPAWN`/`DESPAWN` constants)
- Modify: `crates/axilog-core/src/analysis/replay.rs:136-137`, `:349-350`, `:430-472`
- Modify: `crates/axilog-schema/src/v1/blocks/activity.rs:254`, `:319`, `:760-800`
- Modify: `docs/NATIVE-FORMAT.md`
- Test: `crates/axilog-core/src/analysis/replay.rs` (inline `mod tests`)

**Interfaces:**
- Produces: `analysis::replay::Track` and `ActivityIntervals` each gain
  `dc_intervals: Vec<Interval>`; `build_intervals` returns a 3-tuple
  `(down, dead, dc)`. Schema-side, `ReplayIntervals` gains
  `dc: Vec<(u64, u64)>` and `ReplayTrack` gains
  `dc_intervals: Vec<(u64, u64)>`, matching each struct's existing naming.

- [ ] **Step 1: Verify and add the state-change ordinals**

`sc::SPAWN` and `sc::DESPAWN` are not defined. `analysis/health`'s module
doc already cites `CBTS_DESPAWN` as index 7 from the curl'd
`arcdps/evtc/README.txt`, which is a usable in-repo citation for DESPAWN;
SPAWN needs the same treatment. Confirm both against the reference text and
GW2EI's `ArcDPSEnums.StateChange` before writing them, and reproduce the
citation in the doc comment the way `health` does. Do not guess.

```rust
    /// Agent spawned / became trackable. arcdps reference:
    /// `CBTS_SPAWN` is index 6, between `CBTS_CHANGEDOWN` (5) and
    /// `CBTS_DESPAWN` (7); cross-checked against GW2EI's
    /// `ArcDPSEnums.StateChange.Spawn = 6`.
    pub const SPAWN: u8 = 6;
    /// Agent despawned / left tracking. Index 7, per the same source --
    /// already cited in `crate::analysis::health`'s module doc.
    pub const DESPAWN: u8 = 7;
```

- [ ] **Step 2: Write the failing test**

```rust
    /// A `dc` interval covers the window an agent was not trackable --
    /// both the pre-spawn head of the log and any mid-fight despawn gap.
    /// EI's distance metrics null out positions across exactly these
    /// windows, which is why the replay block has to export them.
    #[test]
    fn build_intervals_reports_despawn_gaps_as_dc() {
        let events = vec![
            state_event(1000, 1, sc::DESPAWN),
            state_event(4000, 1, sc::SPAWN),
        ];
        let raw = raw_from(events);
        let addrs: BTreeSet<u64> = [1u64].into_iter().collect();
        let (down, dead, dc) = build_intervals(&raw, 0, &addrs);
        assert!(down.is_empty());
        assert!(dead.is_empty());
        assert_eq!(dc, vec![Interval { start_ms: 1000, end_ms: 4000 }]);
    }

    /// `dc` must not overlap `down` or `dead` -- a consumer intersecting
    /// the three to find "active" polls depends on them being disjoint.
    #[test]
    fn dc_does_not_overlap_down_or_dead() {
        let events = vec![
            state_event(1000, 1, sc::CHANGE_DOWN),
            state_event(2000, 1, sc::CHANGE_UP),
            state_event(3000, 1, sc::DESPAWN),
            state_event(5000, 1, sc::SPAWN),
        ];
        let raw = raw_from(events);
        let addrs: BTreeSet<u64> = [1u64].into_iter().collect();
        let (down, _dead, dc) = build_intervals(&raw, 0, &addrs);
        assert_eq!(down, vec![Interval { start_ms: 1000, end_ms: 2000 }]);
        assert_eq!(dc, vec![Interval { start_ms: 3000, end_ms: 5000 }]);
        for d in &down {
            for c in &dc {
                assert!(d.end_ms <= c.start_ms || c.end_ms <= d.start_ms, "overlap");
            }
        }
    }
```

Write `state_event(time, src, is_statechange)` as a local helper following
the existing `pos_event` helper in that `mod tests`.

- [ ] **Step 3: Run it to verify it fails**

```bash
cargo test -p axilog-core -q build_intervals_reports_despawn 2>&1 | tail -20
```

Expected: FAIL — `build_intervals` returns a 2-tuple.

- [ ] **Step 4: Extend `build_intervals`**

Change the signature to return
`(Vec<Interval>, Vec<Interval>, Vec<Interval>)` and track `dc` as a third
kind. `dc` is a **separate open slot** from the down/dead state machine,
not a third `Kind` in the same slot — an agent can despawn while dead, and
collapsing them would lose one. Widen the event filter to include
`sc::SPAWN | sc::DESPAWN`, keep a separate `dc_open: Option<u64>`, and:

- `DESPAWN` sets `dc_open = Some(t)` if not already open.
- `SPAWN` closes it, pushing `Interval { start_ms, end_ms: t }`.

Half-open `[start, end)` per the spec. Leave a still-open `dc` at the end
of the loop unclosed rather than clamping it to the log end — the caller
knows the log duration and a clamped interval is indistinguishable from a
real one that happened to end there. Document that choice at the return
site.

Update all three call sites (`:244`, `:419`, and any other the compiler
finds) and add `dc_intervals` to `Track` and `ActivityIntervals`.

- [ ] **Step 5: Run it to verify it passes**

```bash
cargo test -p axilog-core -q replay 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 6: Export through the schema**

Add `dc: Vec<(u64, u64)>` to `ReplayIntervals` and
`dc_intervals: Vec<(u64, u64)>` to `ReplayTrack`
(`crates/axilog-schema/src/v1/blocks/activity.rs`), filling both at
`build_replay` (`:760-800`) from the new core fields. Match each struct's
existing naming — `ReplayIntervals` uses bare `down`/`dead`, `ReplayTrack`
uses `down_intervals`/`dead_intervals`.

Document the half-open convention on both fields, including the divergence
from GW2EI's inclusive sentinel bracket and why: the cutover report
measured that difference at 6 of 6,894 samples (0.087%) of axibridge's
current distance error.

- [ ] **Step 7: Record the format change and run the suites**

Add the `docs/NATIVE-FORMAT.md` entry, then:

```bash
cargo test -q --workspace 2>&1 | tail -30
```

Expected: PASS. Native goldens gain `dc` keys; ei-json goldens must not
move at all — `to_ei_json` has no destination for these.

- [ ] **Step 8: Commit**

```bash
git add crates/ docs/NATIVE-FORMAT.md
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(replay): export dc (despawn) intervals

Half-open [start, end), tracked in a slot separate from the down/dead state
machine since an agent can despawn while dead. Diverges deliberately from
GW2EI's inclusive sentinel bracket, which the cutover report measured as
0.087% of axibridge's distance error.

Prerequisite for the distance scalars: EI's active-position filter nulls
positions across down, dead AND disconnected windows.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: Commander segments

Retains the commander-tag windows that marker resolution currently
discards.

**Files:**
- Modify: `crates/axilog-core/src/wvw/markers.rs:164`, `:216-273`
- Modify: `crates/axilog-core/src/model/mod.rs:84` (`CommanderTag`)
- Modify: `crates/axilog-core/src/wvw/mod.rs:437`
- Modify: `crates/axilog-schema/src/v1/entities.rs:112`, `:183`
- Modify: `docs/NATIVE-FORMAT.md`
- Test: `crates/axilog-core/src/wvw/markers.rs` (inline `mod tests`)

**Interfaces:**
- Produces: `MarkerResolution` gains
  `commander_segments: BTreeMap<u64, Vec<(u64, u64)>>` keyed by agent addr;
  `model::CommanderTag` gains `segments: Vec<(u64, u64)>`; schema
  `CommanderOut` gains `segments: Vec<(u64, u64)>`.

- [ ] **Step 1: Resolve the open calibration question FIRST**

`markers.rs:150-164` records a real-log finding that shapes this task: a
commander whose only commander-tag activity is a burst of assign/remove
events in the first ~350ms of a ~5m48s log, ending in an unreciprocated
removal, with no marker activity for the remaining ~99.9% of the fight.
`ever_commander` exists because `open` alone reported zero commanders for
that log.

Naive segments give that commander a single ~350ms window, which would make
`distToCom` a mean over ~350ms of polls — far worse than axibridge's
current whole-track approximation. **Determine what GW2EI does with this
case before writing the segment logic**, by reading its commander-timeline
construction in the checked-out GW2EI source (see the
`gw2ei-checkout-for-generators` note for the sparse clone at
`/var/tmp/gw2ei`).

Record the finding as a doc comment on `commander_segments`, and pick one
of these explicitly rather than by accident:

- **(a)** A commander-tag instance closed by an *unreciprocated* removal
  inside the first N ms extends to the end of the log, on the reading that
  arcdps is replaying pre-existing marker state at recording start rather
  than reporting a real un-tag.
- **(b)** Segments are literal, and Task 7's distance computation falls
  back to the whole track when a commander's total segment coverage is
  below a threshold.

(a) is the better guess — it matches why `ever_commander` exists at all —
but it must be confirmed against GW2EI, not assumed. If GW2EI turns out to
do neither, implement what it does and say so in the commit message.

- [ ] **Step 2: Write the failing test**

```rust
    /// A commander tag opened and later closed by a removal produces one
    /// closed segment. Marker resolution discards closed instances today
    /// ("nothing downstream needs point-in-time history"), which is exactly
    /// what this changes.
    #[test]
    fn commander_segments_capture_a_closed_tag_window() {
        let raw = raw_from(vec![
            marker_event(1000, 1, COMMANDER_LOCAL_ID, /* buff */ 1),
            marker_event(5000, 1, 0, 0), // value == 0: removal
        ]);
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 5000)]);
    }

    /// A tag still open at the end of the log runs to the log's end, not
    /// to its own start -- a commander who never un-tagged commanded the
    /// whole fight.
    #[test]
    fn commander_segments_close_an_open_tag_at_log_end() {
        let raw = raw_from(vec![
            marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
            marker_event(9000, 2, OVERHEAD_LOCAL_ID, 0),
        ]);
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 9000)]);
    }

    /// An overhead marker on the same agent must not close the commander
    /// tag -- the arcdps rule mirrored at `resolve_markers_and_guilds`:
    /// a non-removal assignment only closes an open instance of the SAME
    /// marker id.
    #[test]
    fn overhead_marker_does_not_end_a_commander_segment() {
        let raw = raw_from(vec![
            marker_event(1000, 1, COMMANDER_LOCAL_ID, 1),
            marker_event(2000, 1, OVERHEAD_LOCAL_ID, 0),
            marker_event(6000, 1, 0, 0),
        ]);
        let res = resolve_markers(&raw);
        assert_eq!(res.commander_segments[&1], vec![(1000, 6000)]);
    }
```

Write `marker_event(time, src, local_id, buff)` and the two id constants as
local helpers, following the existing marker tests in that module for the
exact `RawEvent` field layout a `CBTS_MARKER` row needs.

- [ ] **Step 3: Run it to verify it fails**

```bash
cargo test -p axilog-core -q commander_segments 2>&1 | tail -20
```

Expected: FAIL — no field `commander_segments` on `MarkerResolution`.

- [ ] **Step 4: Collect the segments**

Add to `MarkerResolution`:

```rust
    /// Closed `[tag-on, tag-off)` windows per agent, in ms, for
    /// commander-tag instances only. `open` deliberately drops closed
    /// instances -- it only ever needed final state -- so this is a
    /// parallel collection rather than something derivable from it.
    ///
    /// Multiple simultaneous commanders are normal in WvW; this map holds
    /// every one of them and does NOT pick a reference. That choice belongs
    /// to the consumer (see the distance scalars), which resolves it by
    /// squad membership the way GW2EI does, not by who tagged first.
    pub(crate) commander_segments: BTreeMap<u64, Vec<(u64, u64)>>,
```

In `resolve_markers_and_guilds`, push a segment whenever a commander
instance closes:

- On `value == 0` (removal): before `open.remove(&agent)`, drain the
  agent's open instances and push `(start_ms, e.time)` for each
  `is_commander` one.
- On the same-id replacement path (`slot.retain(...)`): push
  `(start_ms, e.time)` for any retained-away instance that `is_commander`.

At the end of the function, close every still-open commander instance at
the log's last event time. `resolve_markers_and_guilds` does not currently
know the log end — take it as `raw.events.last().map(|e| e.time)`, and note
in a comment that this is the raw stream's last timestamp, matching the
`t0`-relative convention only if callers rebase it (they do not here; these
are absolute times, and the caller in `wvw/mod.rs` rebases if needed).

Then apply the Step 1 decision for the unreciprocated-removal case.

- [ ] **Step 5: Run it to verify it passes**

```bash
cargo test -p axilog-core -q markers 2>&1 | tail -20
```

Expected: PASS, including the existing marker tests.

- [ ] **Step 6: Carry segments onto `CommanderTag` and `CommanderOut`**

Add `pub segments: Vec<(u64, u64)>` to `model::CommanderTag`, filled at
`wvw/mod.rs:437` from `commander_segments` for that player's agent addrs
(a player can have several — merge and sort them). Then add
`pub segments: Vec<(u64, u64)>` to schema `CommanderOut`
(`v1/entities.rs:112`) and fill it at `:183`.

Document on `CommanderOut::segments` that windows are half-open, in
log-relative ms, and that an empty vec on a present `CommanderOut` means
the tag was detected but its windows could not be resolved — not that the
player never commanded.

- [ ] **Step 7: Record the format change and run the suites**

```bash
cargo test -q --workspace 2>&1 | tail -30
```

Expected: PASS. Native goldens gain `segments`; ei-json goldens must not
move — EI's payload has no per-player commander timeline in this shape, and
per the spec the adapter must not acquire one.

- [ ] **Step 8: Commit**

```bash
git add crates/ docs/NATIVE-FORMAT.md
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(markers): retain commander-tag segments

Marker resolution dropped closed instances because nothing downstream
needed point-in-time history. The distance scalars do: axibridge's 3.7%/4.3%
error is dominated by approximating EI's per-segment commander timeline as
one player's whole track.

Multiple simultaneous commanders are normal in WvW, so this map holds every
one and leaves reference selection to the consumer.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: Distance scalars

Computes `distToCom` and `stackDist` engine-side, consuming Tasks 5 and 6.
Deletes `deriveDistanceScalars` from axibridge (that deletion is the
owner's Phase D work; this task makes it possible).

**Files:**
- Create: `crates/axilog-core/src/analysis/distance.rs`
- Modify: `crates/axilog-core/src/analysis/mod.rs` (module declaration)
- Modify: `crates/axilog-schema/src/v1/blocks/activity.rs` (replay block fields + build)
- Modify: `docs/NATIVE-FORMAT.md`
- Test: `crates/axilog-core/src/analysis/distance.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `analysis::replay::Replay` (tracks with `samples: Vec<(u64, f32,
  f32, f32)>` or equivalent — read `replay::Track` for the exact sample
  shape before writing against it), `Track::down_intervals`,
  `dead_intervals`, `dc_intervals` from Task 5, and commander segments from
  Task 6.
- Produces:
  `pub fn build(replay: &Replay, enc: &Encounter) -> BTreeMap<u64, DistanceScalars>`
  keyed by agent addr, where
  `pub struct DistanceScalars { pub dist_to_com: f64, pub stack_dist: f64 }`.
  Schema-side, two `Option<f64>` fields on the replay block's per-entity row.

- [ ] **Step 1: Write the failing tests, one per semantic**

Each of the four semantics from the spec gets its own test, because a
single end-to-end test cannot tell you *which* one is wrong.

```rust
    /// EI iterates the actor's ACTIVE polls only -- positions are nulled
    /// while down, dead, or disconnected. A poll inside any of those
    /// windows must not contribute to the mean.
    #[test]
    fn excludes_polls_inside_down_dead_and_dc_windows() {
        // Actor at distance 100 for two polls, then down for a poll at
        // distance 1000. Mean must be 100, not 400.
        let scalars = run_two_actor_case(
            /* actor */ &[(0, 100.0), (1000, 100.0), (2000, 1000.0)],
            /* reference */ &[(0, 0.0), (1000, 0.0), (2000, 0.0)],
            /* actor_down */ &[(1500, 2500)],
        );
        assert_eq!(scalars.stack_dist, 100.0);
    }

    /// Distance is XY-plane only; Z is discarded. A pure vertical
    /// separation is zero distance to EI.
    #[test]
    fn discards_the_z_axis() {
        let scalars = run_vertical_separation_case(/* dz */ 5000.0);
        assert_eq!(scalars.stack_dist, 0.0);
    }

    /// Polls pair by TIMESTAMP, not by index. A reference missing a poll
    /// the actor has must drop that pair rather than shift the pairing.
    #[test]
    fn pairs_by_timestamp_not_index() {
        let scalars = run_two_actor_case(
            &[(0, 100.0), (1000, 100.0), (2000, 100.0)],
            &[(0, 0.0), (2000, 0.0)], // reference missing t=1000
            &[],
        );
        assert_eq!(scalars.stack_dist, 100.0, "the unmatched poll drops, it does not shift");
    }

    /// EI's sentinel for "nothing qualified" is -1, NOT zero and NOT
    /// absent. Absence means the replay pass never ran; -1 means it ran
    /// and this actor had no qualifying poll. Collapsing the two loses the
    /// distinction the gate-record idiom exists to preserve.
    #[test]
    fn reports_minus_one_when_no_poll_qualifies() {
        let scalars = run_two_actor_case(
            &[(0, 100.0)],
            &[(5000, 0.0)], // no overlapping timestamp at all
            &[],
        );
        assert_eq!(scalars.stack_dist, -1.0);
        assert_eq!(scalars.dist_to_com, -1.0);
    }

    /// The commander reference uses the commanding player's RAW positions
    /// during their segments -- NOT active-filtered. The squad centre uses
    /// every player's ACTIVE position. GW2EI's two references differ this
    /// way on purpose, and matching the asymmetry is what takes the error
    /// to zero rather than merely reducing it.
    #[test]
    fn commander_reference_is_raw_but_squad_centre_is_active() {
        let scalars = run_downed_commander_case();
        assert_ne!(
            scalars.dist_to_com, -1.0,
            "a downed commander still provides a reference position"
        );
    }
```

Write the three `run_*_case` helpers as local constructors that build a
minimal `Replay` directly rather than parsing a log — these are unit tests
of the reduction, not of the parser.

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p axilog-core -q distance 2>&1 | tail -20
```

Expected: FAIL — module does not exist.

- [ ] **Step 3: Write the module**

Create `crates/axilog-core/src/analysis/distance.rs` with a module doc
recording the full semantics and their provenance — this repo's convention
is that a module encoding GW2EI behaviour cites where that behaviour was
read from. The five rules, all verified against GW2EI's
`GetDistanceToTarget` per the cutover report §5:

1. Iterate the actor's **active** polled positions — excluded while down,
   dead, or disconnected.
2. Pair with the reference at the **same poll timestamp**; unmatched polls
   drop.
3. **XY-plane** length; Z discarded.
4. Arithmetic mean over qualifying pairs.
5. **`-1`** when none qualified.

Two references:
- **Commander:** the commanding player's **raw** (not active-filtered)
  positions during their commander segments. Resolve *which* player by
  squad membership, matching GW2EI — not by who tagged first.
- **Squad centre:** the per-poll arithmetic mean of every player's
  **active** position.

Declare the module in `analysis/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p axilog-core -q distance 2>&1 | tail -20
```

Expected: PASS, all five.

- [ ] **Step 5: Export through the schema**

Add to the replay block's per-entity row:

```rust
    /// EI's `distToCom` -- mean distance to the commander over this
    /// actor's active polls. `-1.0` when no poll qualified. `None` when
    /// the replay pass did not run: these are TWO DISTINCT STATES and must
    /// not be collapsed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dist_to_com: Option<f64>,
    /// EI's `stackDist` -- the same reduction against the squad centre.
    /// Same two-state convention as `dist_to_com`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_dist: Option<f64>,
```

- [ ] **Step 6: Validate against the cutover report's measured EI values**

This is the task's real acceptance gate. The cutover report §5 records
`deriveDistanceScalars`' output at 3.7% / 4.3% mean error against EI's own
values on a specific payload. Reproduce that comparison against the
committed fixture.

**The bar is exact agreement, not a tolerance.** Landing inside the old
3.7% band is a FAILURE, not a pass: it means one of the five semantics
above is wrong and the implementation has merely reproduced axibridge's
approximation by a different route. If that happens, do not widen the
assertion — find which semantic is wrong. The most likely culprits, in
order: the commander reference being active-filtered when it should be raw,
the squad centre being taken over the wrong roster, and `dc` windows not
being excluded.

Small floating-point differences from summation order are acceptable;
compare with an epsilon around 1e-9, not a percentage.

- [ ] **Step 7: Record the format change and run the full suite**

```bash
cargo test -q --workspace 2>&1 | tail -30
```

Expected: PASS. ei-json goldens must not move.

- [ ] **Step 8: Commit**

```bash
git add crates/ docs/NATIVE-FORMAT.md
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(distance): compute distToCom and stackDist engine-side

Five semantics verified against GW2EI's GetDistanceToTarget: active-only
actor polls, timestamp pairing, XY-plane length, arithmetic mean, -1 when
nothing qualified. The commander reference is raw and the squad centre is
active-filtered -- GW2EI's asymmetry, and matching it is what takes the
error to zero rather than merely reducing it.

Absent and -1.0 are distinct: absent means the replay pass never ran.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 8: Log-start wall clock

Replaces axibridge's `.zevtc`-mtime inference with the timestamp arcdps
already records and axilog has never read.

**Files:**
- Modify: `crates/axilog-core/src/evtc/event.rs:23` (doc the payload)
- Modify: `crates/axilog-core/src/model/mod.rs` (`Encounter`)
- Modify: `crates/axilog-core/src/wvw/mod.rs` or wherever `Encounter` is built (grep for `duration_ms:` to find it)
- Modify: `crates/axilog-schema/src/v1/mod.rs:28` (`EncounterOut`), `:477` (build site)
- Modify: `docs/NATIVE-FORMAT.md`
- Test: `crates/axilog-core` (inline, beside the extraction)

**Interfaces:**
- Produces: `model::Encounter` gains `started_at_unix: Option<u64>`;
  `EncounterOut` gains `started_at_unix: Option<u64>` with
  `skip_serializing_if = "Option::is_none"`.

- [ ] **Step 1: Establish the payload slot with a citation**

`sc::LOG_START = 9` is defined and never read. `RawEvent` exposes `time`,
`src_agent`, `dst_agent`, `value`, `buff_dmg`. arcdps documents `LOG_START`
as carrying a server timestamp and a local timestamp, but **which field
holds which is not to be guessed.**

Confirm against the curl'd `arcdps/evtc/README.txt` and cross-check with
GW2EI's `LogStartEvent` / `CombatEventFactory`. Reproduce the citation trail
in the doc comment on `sc::LOG_START`, following the form
`analysis::health`'s module doc uses for `HEALTHPCTUPDATE`.

**Emit server time, not client time**, when both are present. A recording
client's clock is a fact about that machine, not about the log.

- [ ] **Step 2: Write the failing test**

```rust
    /// arcdps records a wall clock at log start. axilog defined the
    /// ordinal and never read it, which is why axibridge infers the start
    /// time from the .zevtc file's mtime -- wrong for any copied or
    /// restored file.
    #[test]
    fn extracts_the_log_start_wall_clock() {
        let mut ev = base_state_event(0, sc::LOG_START);
        set_log_start_server_time(&mut ev, 1_760_000_000);
        let enc = resolve_encounter_from(vec![ev]);
        assert_eq!(enc.started_at_unix, Some(1_760_000_000));
    }

    /// Absence is a real state -- a truncated or synthetic log may carry no
    /// LOG_START at all, and that must stay distinguishable from epoch
    /// zero.
    #[test]
    fn reports_absence_not_zero_without_a_log_start() {
        let enc = resolve_encounter_from(vec![]);
        assert_eq!(enc.started_at_unix, None);
    }
```

`set_log_start_server_time` writes whichever `RawEvent` field Step 1
established — the helper exists so that field appears in exactly one place
in the tests.

- [ ] **Step 3: Run it to verify it fails**

```bash
cargo test -p axilog-core -q log_start 2>&1 | tail -20
```

Expected: FAIL — no field `started_at_unix` on `Encounter`.

- [ ] **Step 4: Implement the extraction**

Add `pub started_at_unix: Option<u64>` to `model::Encounter` and fill it by
scanning for the first `sc::LOG_START` event. Fold it into an existing
whole-stream pass rather than adding a dedicated one — `markers.rs:224-229`
records that a separate pass measured +12% on `model::resolve` for a single
`u8` compare per event, which is the precedent for not adding one here.

Every existing `Encounter` literal in tests will need the new field; the
compiler will list them.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p axilog-core -q 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 6: Export through the schema**

Add to `EncounterOut`:

```rust
    /// Wall-clock log start, seconds since the epoch, from arcdps's
    /// `CBTS_LOGSTART`. `None` when the log carries no such event --
    /// absence must stay distinguishable from epoch zero, which is why
    /// this is not a bare `u64` defaulting to 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_unix: Option<u64>,
```

Fill it at `v1/mod.rs:477` from `legacy.encounter.started_at_unix`.

- [ ] **Step 7: Record the format change and run the full suite**

```bash
cargo test -q --workspace 2>&1 | tail -30
```

Expected: PASS. Native goldens gain `started_at_unix` on the committed
fixture; ei-json goldens must not move.

- [ ] **Step 8: Commit**

```bash
git add crates/ docs/NATIVE-FORMAT.md
SSH_AUTH_SOCK="$HOME/.1password/agent.sock" git commit -m "feat(encounter): extract the log-start wall clock

sc::LOG_START was defined and never read, so axibridge infers timeStart
from the .zevtc file's mtime -- wrong for any copied or restored file.
Server time, not client time: a recording machine's clock is not a fact
about the log. Option, so absence stays distinguishable from epoch zero.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Finishing

After Task 8, run the full suite once more plus the SDK checks, since the
schema changed and both SDKs mirror it:

```bash
cargo test -q --workspace 2>&1 | tail -30
cd crates/axilog-node && npm run build && npm test
cd ../.. && .venv/bin/maturin develop --release && .venv/bin/python -m unittest discover -s tests
```

If `crates/axilog-node/types.d.ts` needs the new fields, add them — it is a
hand-maintained mirror.

Then use superpowers:finishing-a-development-branch.

## Known Risks

- **Task 6's calibration question is genuinely open.** The
  unreciprocated-removal case is documented in `markers.rs` as a real
  finding on a real log, and the segment policy for it must come from GW2EI,
  not from this plan. If Step 1 cannot resolve it, stop and raise it rather
  than guessing — Task 7's accuracy depends on the answer.
- **Task 8's payload slot is unverified.** Five candidate `RawEvent` fields,
  no citation yet. Establish it before implementing.
- **Windows CI is intermittently red for an unrelated reason** — `axilog-cli`'s
  bin and `axilog-py`'s cdylib are both named `axilog`, so both write
  `axilog.pdb` and the two `link.exe` invocations race (`LNK1201`). A rerun
  usually goes green. Do not chase it as a Phase B regression, and do not
  spend CI round-trips on the debuginfo workaround without first explaining
  why the env var is ignored there.
