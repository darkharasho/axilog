# MNAME — Skill and Buff Name Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop axilog emitting unnamed skills — no id that any emitted array references may resolve to `Skill <id>` or to an empty name — and add a test that keeps it that way.

**Architecture:** One name-resolution chain, in one function, reachable from
both places that resolve names. `Metrics` carries the log's own skill table
so `CatalogBuilder::finish` can consult it, which is the rung it has always
been missing. A generated `skill_name_overrides` catalog supplies the ids
ArenaNet's API does not list. Indirect healing rows additionally register as
buffs, matching GW2EI's routing. A golden leak test asserts the invariant
over every id-bearing array so the next gap fails a build instead of
appearing in a screenshot.

**Tech Stack:** Rust (workspace: `axilog-core`, `axilog-schema`,
`axilog-ei`), Python 3 stdlib for catalog generation, `cargo test` goldens.

**Spec:** `docs/superpowers/specs/2026-08-26-mname-skill-name-resolution-design.md`
— **read its "Amendments" section first.** The plan implements the amended
design: spec change 1 is dropped, and its two effects are absorbed into
Tasks 1 and 2 here.

## Global Constraints

- Repository: `/var/home/mstephens/Documents/GitHub/axilog`, branch `main`.
  This is NOT the axibridge repo. Do not edit anything under
  `../axibridge/`.
- **AxiBridge needs no code change.** `computeHealEffectivenessData.ts`'s
  `resolveSkillMeta` already reads `skillMap` first and falls back to
  `buffMap`. The fix ships as an axilog version bump. Do not "also fix" the
  consumer.
- Commit signing is on (`commit.gpgsign=true`, `gpg.format=ssh`, 1Password
  agent). If a commit fails with `1Password: failed to fill whole buffer`,
  stop and report it — do NOT retry with `--no-gpg-sign`.
- GW2EI C# source is checked out at `/var/tmp/gw2ei`. Generators take the
  checkout root as `argv[1]`; the default is `/tmp/gw2ei`, so always pass
  `/var/tmp/gw2ei` explicitly.
- Generated catalogs (`crates/axilog-core/src/analysis/skill_icons.rs`,
  `skill_icon_overrides.rs`, `buff_icons.rs`, and the new
  `skill_name_overrides.rs`) are **never hand-edited**. Change the generator
  and re-run it.
- Every generator must satisfy the accounting identity
  `considered == transcribed + repeats + skipped`, with each skip carrying a
  reason string, and must emit rows sorted by id (the consumers
  binary-search).
- Cut release tags from `main` only.
- Run tests with `cargo test`; there is no build cache in this checkout, so
  the first build takes several minutes. Do not interpret a slow first
  `cargo test` as a hang.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/axilog-core/src/analysis/skill_map.rs` | `resolve_name` becomes the single public chain; gains the override rung | 1, 4 |
| `crates/axilog-core/src/analysis/mod.rs` | `Metrics::log_skill_names`, populated in `analyze` | 1 |
| `crates/axilog-core/src/analysis/hit_stats.rs` | `can_crit` visibility `pub(crate)` → `pub` | 2 |
| `crates/axilog-schema/src/v1/catalogs.rs` | `finish` delegates both name chains; computes pure-id flags for uncovered ids | 1, 2, 5 |
| `crates/axilog-schema/src/v1/blocks/support.rs` | indirect heal rows also `reference_buff` | 5 |
| `scripts/gen_skill_name_override_catalog.py` | **new** — extracts `OverridenSkillNames` | 3 |
| `crates/axilog-core/src/analysis/skill_name_overrides.rs` | **new, generated** — 332-entry name table | 3 |
| `crates/axilog-ei/tests/name_leak_golden.rs` | **new** — the systemic guard | 6 |

Tasks 1→2→3→4→5 are ordered by dependency. Task 6 (the leak test) comes
last because it is the acceptance test for all of them.

---

### Task 1: Give `CatalogBuilder::finish` the log's own skill table

This is the task that fixes ten of the eleven reported ids. Everything else
is narrowing the remaining gap and locking it shut.

Today there are two name chains that disagree
(`skill_map::resolve_name` consults the log table and `pseudo_name`;
`catalogs.rs:250-255` consults neither). This task makes `resolve_name`
public, gives `Metrics` the log's skill table, and points `finish` at both.

**Files:**
- Modify: `crates/axilog-core/src/analysis/skill_map.rs:435` (`resolve_name`
  — make `pub`, extend doc comment)
- Modify: `crates/axilog-core/src/analysis/mod.rs:305` (`Metrics` struct) and
  `:818` (the struct literal at the end of `analyze`)
- Modify: `crates/axilog-schema/src/v1/catalogs.rs:250-255` (`SkillEntry::name`)
- Test: `crates/axilog-schema/src/v1/catalogs.rs` (existing `mod tests`,
  which already has a `metrics_with_skills()` helper at `:372`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub fn axilog_core::analysis::skill_map::resolve_name(id: u32, raw_name: Option<&str>) -> String`
  - `Metrics.log_skill_names: std::collections::BTreeMap<u32, String>`

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block at the bottom of
`crates/axilog-schema/src/v1/catalogs.rs`:

```rust
    /// The bug from the MNAME report, reduced: an id that `blocks.healing`
    /// referenced but `skill_map`'s damage/rotation scope never covered.
    /// Before this task `finish` had no way to reach the log's own name for
    /// it and emitted the `Skill <id>` placeholder — the literal string the
    /// reporter saw rendered in AxiBridge's healing table.
    #[test]
    fn finish_names_a_referenced_id_from_the_logs_own_skill_table() {
        let mut metrics = metrics_with_skills();
        metrics.log_skill_names.insert(13721, "Restorative Mantras".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(13721);
        let catalogs = cats.finish(&metrics, None);

        assert_eq!(
            catalogs.skills[&13721].name, "Restorative Mantras",
            "an id outside skill_map's scope must still resolve through the log table"
        );
    }

    /// The log table is rung ONE, not a fallback: an id `skill_map` did
    /// cover keeps the name that pass already resolved, so this change can
    /// never move a name that resolved before it.
    #[test]
    fn finish_prefers_the_skill_map_entry_over_the_log_table() {
        let mut metrics = metrics_with_skills();
        let covered = *metrics.skill_map.keys().next().expect("helper seeds at least one skill");
        let expected = metrics.skill_map[&covered].name.clone();
        metrics.log_skill_names.insert(covered, "SHOULD NOT WIN".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(covered);
        let catalogs = cats.finish(&metrics, None);

        assert_eq!(catalogs.skills[&covered].name, expected);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd /var/home/mstephens/Documents/GitHub/axilog
cargo test -p axilog-schema finish_names_a_referenced_id 2>&1 | tail -20
```

Expected: FAIL to **compile**, with `no field 'log_skill_names' on type
'Metrics'`. A compile failure is the correct red state here — the field does
not exist yet.

- [ ] **Step 3: Add `log_skill_names` to `Metrics`**

In `crates/axilog-core/src/analysis/mod.rs`, add this field to the `Metrics`
struct (declared at `:305`), next to `skill_map`:

```rust
    /// The log's OWN decoded `cbtskill` name table, whole — id to the raw
    /// string arcdps wrote, untrimmed and unfiltered.
    ///
    /// `skill_map` is REFERENCE-scoped by design (see its module doc): it
    /// covers only ids some squad player's damage or rotation touched. Any
    /// other block may legitimately reference an id outside that scope —
    /// `blocks.healing` does, for every heal-only skill — and
    /// `CatalogBuilder::finish` is where those ids get named. Before this
    /// field existed `finish` had no access to the log's own table at all,
    /// so it skipped straight to the GW2 API catalog and emitted `"Skill
    /// <id>"` for anything the API had never heard of. That is the MNAME
    /// bug.
    ///
    /// Deliberately UNGATED and unscoped. A name is a property of the log,
    /// not of which passes a caller asked for; scoping this to the current
    /// reference set would make the same log name skills differently
    /// depending on its flags. It costs a few hundred short strings — the
    /// committed WvW fixture's table is ~508 rows.
    pub log_skill_names: BTreeMap<u32, String>,
```

Then populate it in `analyze`, immediately after the `skill_map::build` call
at `:808`:

```rust
    let skill_map = skill_map::build(raw, &players, &instants);
    // Ungated and whole, unlike `skill_map` above — see the field's doc
    // comment. Last-wins on a duplicate id, the same tie-break rule
    // `skill_map::build` documents for the same source.
    let log_skill_names: BTreeMap<u32, String> =
        raw.skills.iter().map(|s| (s.id, s.name.clone())).collect();
```

and add `log_skill_names` to the `Metrics { .. }` literal at `:818`.

- [ ] **Step 4: Make `resolve_name` public**

In `crates/axilog-core/src/analysis/skill_map.rs`, change the signature at
`:435` from `fn resolve_name` to `pub fn resolve_name`, and append this to
its existing doc comment (which already documents the four rungs):

```rust
/// # One chain, two callers
///
/// `CatalogBuilder::finish` resolves names too, for ids this module's
/// reference scope never covered, and used to do it with its own shorter
/// chain — no log table, no `pseudo_name`. Two chains that must agree and
/// silently did not. This function is now the only one; `finish` calls it
/// with `metrics.log_skill_names.get(&id)` as `raw_name`.
```

- [ ] **Step 5: Point `finish` at it**

In `crates/axilog-schema/src/v1/catalogs.rs`, replace the `name:` field
expression at `:250-255` (the `entry.map(...).or_else(...).unwrap_or_else(...)`
chain) with:

```rust
                        // One chain, shared with `skill_map::resolve_name`
                        // — see its "One chain, two callers" section. An
                        // id the map covered keeps the name that pass
                        // already resolved; anything else goes through the
                        // full chain from its first rung, the log's own
                        // table, which is the rung this site used to lack.
                        name: entry.map(|e| e.name.clone()).unwrap_or_else(|| {
                            axilog_core::analysis::skill_map::resolve_name(
                                id,
                                metrics.log_skill_names.get(&id).map(String::as_str),
                            )
                        }),
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p axilog-schema finish_ 2>&1 | tail -20
```
Expected: PASS, both tests.

- [ ] **Step 7: Run the full core and schema suites**

```bash
cargo test -p axilog-core -p axilog-schema 2>&1 | tail -40
```

Expected: PASS. If `skill_map_golden.rs` or a v1 golden fails on a name
that CHANGED (rather than one that stopped being a placeholder), stop and
report it — this task is designed to be purely additive, so a moved name is
a real finding, not a golden to refresh.

- [ ] **Step 8: Commit**

```bash
git add crates/axilog-core/src/analysis/mod.rs \
        crates/axilog-core/src/analysis/skill_map.rs \
        crates/axilog-schema/src/v1/catalogs.rs
git commit -m "fix(names): let the catalog resolve names from the log's own skill table

CatalogBuilder::finish names every id any block references, but had no
access to the log's own cbtskill table -- it skipped straight to the GW2
API catalog and emitted 'Skill <id>' for anything the API never listed.
blocks.healing references heal-only ids that skill_map's damage/rotation
scope does not cover, so that placeholder is what AxiBridge rendered.

Metrics now carries the log's whole name table, ungated, and finish
delegates to skill_map::resolve_name instead of running a second, shorter
chain that consulted neither the log table nor pseudo_name.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Resolve the pure-id flags for uncovered ids

`finish` defaults `can_crit` to `true` for an id `skill_map` never covered
(`catalogs.rs:258`). For a heal-only skill that is wrong, and Task 1 has
just made those ids far more visible. `is_swap` and `can_crit` are pure
functions of the id — the same functions `skill_map::build` itself calls —
so `finish` computes them rather than guessing. This is what the dropped
spec change 1 was really buying.

**Files:**
- Modify: `crates/axilog-core/src/analysis/hit_stats.rs:262` (visibility)
- Modify: `crates/axilog-schema/src/v1/catalogs.rs:257-258`
- Test: `crates/axilog-schema/src/v1/catalogs.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `Metrics.log_skill_names` (only so the tests can share
  the same helper shape; no API dependency).
- Produces: `pub fn axilog_core::analysis::hit_stats::can_crit(skillid: u32) -> bool`

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/axilog-schema/src/v1/catalogs.rs`:

```rust
    /// Weapon Swap (-2 as u32) is the clearest case: `is_swap` is true for
    /// it by definition and `can_crit` is false, but an id outside
    /// `skill_map`'s scope used to get `is_swap: false, can_crit: true` —
    /// both wrong, and both computable from the id with no log at all.
    #[test]
    fn finish_computes_pure_id_flags_for_an_id_the_skill_map_never_covered() {
        let metrics = metrics_with_skills();
        let weapon_swap = (-2i32) as u32;

        let mut cats = CatalogBuilder::default();
        cats.reference_skill(weapon_swap);
        let catalogs = cats.finish(&metrics, None);

        let entry = &catalogs.skills[&weapon_swap];
        assert!(entry.is_swap, "is_swap is a pure function of the id");
        assert!(!entry.can_crit, "a weapon swap cannot crit");
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p axilog-schema finish_computes_pure_id_flags 2>&1 | tail -20
```
Expected: FAIL — `assertion failed: entry.is_swap`.

- [ ] **Step 3: Widen `can_crit`'s visibility**

In `crates/axilog-core/src/analysis/hit_stats.rs:262`, change
`pub(crate) fn can_crit` to:

```rust
pub fn can_crit(skillid: u32) -> bool {
```

- [ ] **Step 4: Compute the flags in `finish`**

In `crates/axilog-schema/src/v1/catalogs.rs`, replace the `is_swap:` and
`can_crit:` lines at `:257-258` with:

```rust
                        // Pure functions of the id — the SAME two the skill
                        // map itself calls, so a covered and an uncovered id
                        // get the same answer. Defaulting them (`false` /
                        // `true`) let an uncovered heal skill claim it could
                        // crit.
                        is_swap: axilog_core::analysis::skill_map::is_swap(id),
                        can_crit: axilog_core::analysis::hit_stats::can_crit(id),
```

Note both lose their `entry.map(...)` wrapper entirely: reading the flag off
the entry and computing it are now the same answer by construction, so the
branch is dead weight.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p axilog-core -p axilog-schema 2>&1 | tail -40
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-core/src/analysis/hit_stats.rs \
        crates/axilog-schema/src/v1/catalogs.rs
git commit -m "fix(names): compute is_swap/can_crit for ids the skill map never covered

Both are pure functions of the skill id, and finish was defaulting them
(can_crit: true) for any referenced id outside skill_map's reference
scope -- which after the previous commit is mostly heal-only skills, for
which 'can crit' is simply wrong.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Generate the `skill_name_overrides` catalog

One reported id is still unnamed after Tasks 1-2: **1066**. arcdps writes
the literal string `"1066"` into the log table for it, which `resolve_name`
correctly rejects as a numeric placeholder, and `/v2/skills` has no record
of the id. GW2EI names it `Resurrect` from
`SkillItemOverrides.OverridenSkillNames`.

This task only GENERATES the table. Task 4 wires it in, because the
precedence question needs its own measurement and its own commit.

**Files:**
- Create: `scripts/gen_skill_name_override_catalog.py`
- Create: `crates/axilog-core/src/analysis/skill_name_overrides.rs` (generated)
- Modify: `crates/axilog-core/src/analysis/mod.rs` (add `pub mod`, alphabetically
  adjacent to `skill_icon_overrides` at `:113`)
- Test: `crates/axilog-core/tests/skill_name_overrides_catalog.rs` (new)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub fn axilog_core::analysis::skill_name_overrides::name(id: u32) -> Option<&'static str>`
  - `pub const axilog_core::analysis::skill_name_overrides::SKILL_NAME_OVERRIDES: &[(u32, &str)]`

- [ ] **Step 1: Write the generator**

`scripts/gen_skill_name_override_catalog.py` is a near-clone of its icon
sibling. Everything up to `main()` is shared logic, so import it rather than
copying: the sibling is a module in the same directory and its helpers are
already at module scope.

```python
#!/usr/bin/env python3
"""Regenerate `analysis::skill_name_overrides` from the GW2EI C# sources.

`SkillItemOverrides.cs` holds two sibling dictionaries.
`gen_skill_icon_override_catalog.py` extracts `OverridenSkillIcons`; this
script extracts `OverridenSkillNames`, which has the identical shape —
`{ IdSymbol, "Display Name" }` — differing only in that the value is a
string literal rather than a symbol, so there is no symbol table to
resolve and no ambiguity to skip on.

Why the names matter separately from the icons. ArenaNet's `/v2/skills`
does not list every id an arcdps log carries, and for those ids arcdps
often writes a bare numeric placeholder into the log's own skill table —
skill 1066 arrives named literally `"1066"`. `skill_map::resolve_name`
rejects a numeric name (correctly: it is not a name), the API catalog has
never heard of the id, and the result is the `"Skill 1066"` placeholder a
WvW player reported seeing in a rendered healing table. GW2EI names that
id `Resurrect`, from this table.

Scope. Only the POSITIVE ids. The negative pseudo ids are already hand-
transcribed as `skill_map::PSEUDO_SKILL_NAMES`, with a documented gap
(EI's twelve Weaver dual-attunement ids are named from its BUFF table, not
from this one); duplicating them here would create a second source of
truth for the same 25 entries.

Usage:

    python3 scripts/gen_skill_name_override_catalog.py [/path/to/GW2EI/checkout]

then `git diff`: a clean tree means the committed catalog is exactly what
the current GW2EI source produces. Standard library only.
"""

import collections
import os
import re
import sys

from gen_skill_icon_override_catalog import (
    PARSER, TABLE, Skip, consts, entries, resolve_id, strip_comments, table_body,
)

OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "crates", "axilog-core", "src", "analysis", "skill_name_overrides.rs",
    )
)


def load_ids():
    # Both visibilities, for the same reason the icon generator gives: the
    # arcdps-synthetic ids are declared `internal`.
    return {
        k: int(v)
        for k, v in consts(
            os.path.join(PARSER, "ParserHelpers/IDs/SkillIDs.cs"),
            r"(?:public|internal) const long (\w+)\s*=\s*(-?\d+);",
        ).items()
    }


def resolve_name(sym):
    if sym.startswith('"') and sym.endswith('"') and len(sym) >= 2:
        text = sym[1:-1]
        if not text.strip():
            raise Skip("name literal is empty")
        return text
    raise Skip("name argument is not a string literal")


def main():
    ids = load_ids()
    body = table_body(
        strip_comments(open(TABLE, encoding="utf-8-sig").read()),
        "OverridenSkillNames",
    )

    considered = 0
    skipped = collections.Counter()
    by_id = collections.defaultdict(set)
    declarations = collections.Counter()

    for id_sym, name_sym in entries(body):
        considered += 1
        try:
            # Resolve BOTH before touching `by_id` — it is a defaultdict, so
            # indexing first would leave an empty entry behind on a raise.
            skill_id = resolve_id(id_sym, ids)
            display = resolve_name(name_sym)
        except Skip as e:
            skipped[str(e)] += 1
            continue
        by_id[skill_id].add(display)
        declarations[skill_id] += 1

    rows, repeats = [], 0
    for skill_id, names in sorted(by_id.items()):
        if len(names) > 1:
            # Two different names for one id is a contradiction we cannot
            # break, exactly as with two icons: no entry rather than a coin
            # flip, so the id keeps whatever the earlier rungs resolved.
            skipped["id declared with more than one name"] += declarations[skill_id]
            continue
        rows.append((skill_id, next(iter(names))))
        repeats += declarations[skill_id] - 1

    total_skipped = sum(skipped.values())
    with open(OUT, "w") as f:
        f.write(HEADER.format(
            count=len(rows),
            considered=considered,
            repeats=repeats,
            skipped=total_skipped,
            # No leading indent: rustdoc reads a 4-space-indented block in a
            # doc comment as a Rust code sample and tries to compile it.
            skip_table="\n".join(
                f"//! - {n} {reason}" for reason, n in skipped.most_common()
            ) or "//! - (none)",
        ))
        for skill_id, display in rows:
            escaped = display.replace("\\", "\\\\").replace('"', '\\"')
            f.write(f'    ({skill_id}, "{escaped}"),\n')
        f.write("];\n")

    print(f"considered {considered} = transcribed {len(rows)} "
          f"+ repeat declarations {repeats} + skipped {total_skipped}")
    for reason, n in skipped.most_common():
        print(f"  skipped {n}: {reason}")
    assert considered == len(rows) + repeats + total_skipped, "accounting must balance"


HEADER = '''//! Skill NAME overrides from GW2EI's C# sources.
//!
//! GENERATED by `scripts/gen_skill_name_override_catalog.py` -- do not
//! hand-edit. Re-run it and `git diff` to verify this table against
//! GW2EI's source.
//!
//! The sibling of [`super::skill_icon_overrides`], from the sibling
//! dictionary in the same file, closing the same gap for names that one
//! closes for art: ArenaNet's `/v2/skills` does not list every id an
//! arcdps log carries, and for the ids it omits arcdps itself often writes
//! a bare numeric placeholder into the log's own skill table (id 1066
//! arrives named literally `"1066"`). With no log name, no pseudo name and
//! no API entry, such an id used to render as `"Skill 1066"`.
//!
//! POSITIVE ids only. The negative pseudo ids are hand-transcribed as
//! [`super::skill_map::PSEUDO_SKILL_NAMES`], which documents its own gap;
//! two sources of truth for the same 25 entries would be worse than one.
//!
//! The generator's accounting for this table:
//!
//! considered {considered} = transcribed {count} + repeat declarations
//! {repeats} + skipped {skipped}
//!
//! A repeat is the same id re-stated elsewhere in the table agreeing on the
//! name -- `Resurrect` and `Resurrect2` are two ids, not a repeat. The rest
//! are skipped:
//!
{skip_table}
//!
//! Entries are sorted by id so lookups can binary-search.

/// GW2EI's overriding display name for skill `id`, or `None` when it has
/// none.
pub fn name(id: u32) -> Option<&'static str> {{
    SKILL_NAME_OVERRIDES
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_NAME_OVERRIDES[i].1)
}}

/// `(skill id, display name)`, sorted by id.
pub const SKILL_NAME_OVERRIDES: &[(u32, &str)] = &[
'''


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run the generator**

```bash
cd /var/home/mstephens/Documents/GitHub/axilog
python3 scripts/gen_skill_name_override_catalog.py /var/tmp/gw2ei
```

Expected: a line reading `considered 332 = transcribed <N> + repeat
declarations <R> + skipped <S>`, with the assertion at the end not firing.
`considered` should be 332; if it is not, GW2EI's table has moved since this
plan was written — report the new number rather than adjusting anything.

Sanity-check the headline id:

```bash
grep -n '^    (1066, ' crates/axilog-core/src/analysis/skill_name_overrides.rs
```
Expected: `(1066, "Resurrect"),`

- [ ] **Step 3: Register the module**

In `crates/axilog-core/src/analysis/mod.rs`, add next to the existing
`pub mod skill_icon_overrides;` at `:113`:

```rust
pub mod skill_name_overrides;
```

- [ ] **Step 4: Write the catalog test**

Create `crates/axilog-core/tests/skill_name_overrides_catalog.rs`:

```rust
//! Structural guards on the generated `skill_name_overrides` table.
//!
//! Deliberately NOT a transcription of the table's contents: the generator
//! and `git diff` are what verify it against GW2EI's source. These check
//! the two properties the CONSUMER depends on and a generator bug could
//! silently break.

use axilog_core::analysis::skill_name_overrides::{name, SKILL_NAME_OVERRIDES};

#[test]
fn table_is_sorted_by_id_so_the_binary_search_is_valid() {
    let ids: Vec<u32> = SKILL_NAME_OVERRIDES.iter().map(|&(id, _)| id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted, "entries must be sorted by id and unique");
}

#[test]
fn no_entry_is_empty_or_numeric() {
    // A numeric "name" is exactly the arcdps placeholder `resolve_name`
    // rejects — an override that reinstated one would be worse than none.
    for &(id, n) in SKILL_NAME_OVERRIDES {
        assert!(!n.trim().is_empty(), "skill {id} has an empty override name");
        assert!(
            !n.chars().all(|c| c.is_ascii_digit()),
            "skill {id}'s override name {n:?} is numeric"
        );
    }
}

#[test]
fn resurrect_1066_is_the_reported_id_and_resolves() {
    // The MNAME report's headline offender: arcdps writes the literal
    // string "1066" for this id and /v2/skills has never listed it, so
    // this table is its only name source.
    assert_eq!(name(1066), Some("Resurrect"));
}

#[test]
fn only_positive_ids_are_transcribed() {
    // The negative pseudo ids live in `skill_map::PSEUDO_SKILL_NAMES`.
    for &(id, n) in SKILL_NAME_OVERRIDES {
        assert!((id as i32) > 0, "synthetic id {} ({n}) must not be here", id as i32);
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p axilog-core --test skill_name_overrides_catalog 2>&1 | tail -20
```
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add scripts/gen_skill_name_override_catalog.py \
        crates/axilog-core/src/analysis/skill_name_overrides.rs \
        crates/axilog-core/src/analysis/mod.rs \
        crates/axilog-core/tests/skill_name_overrides_catalog.rs
git commit -m "feat(names): generate skill_name_overrides from GW2EI's OverridenSkillNames

The sibling dictionary to OverridenSkillIcons, in the same file, closing
the same gap for names that the icon table closes for art: ids /v2/skills
does not list, for which arcdps often writes a bare numeric placeholder
into the log's own skill table (1066 arrives named literally '1066').

Generated only -- not yet consulted by any resolver. Wiring it in needs
its own precedence measurement.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Wire the overrides in, at a MEASURED precedence

The spec is explicit that this rung's position is decided by measurement,
not by symmetry with `resolve_icon`. Unlike an icon, an override can
**rename** an id that already resolves — GW2EI's table carries its
disambiguations (`Flame Blast (Superior Sigil of Fire)` where the log says
`Flame Blast`). Override-first matches GW2EI's `SkillItem.cs`; last-resort
can only ever displace the placeholder.

Implement it first, measure, then decide — and write the number down either
way.

**Files:**
- Modify: `crates/axilog-core/src/analysis/skill_map.rs:435` (`resolve_name`)
- Test: `crates/axilog-core/src/analysis/skill_map.rs` (`mod tests`)
- Create: `crates/axilog-core/tests/name_override_precedence.rs` (the
  measurement, kept as a permanent record)

**Interfaces:**
- Consumes: Task 3's `skill_name_overrides::name`, Task 1's public
  `resolve_name`.
- Produces: no new signature — `resolve_name`'s behaviour changes only.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/axilog-core/src/analysis/skill_map.rs`:

```rust
    /// The MNAME report's headline id. arcdps writes the literal string
    /// "1066" into the log's skill table for it — a numeric placeholder,
    /// which rung 1 rejects — and /v2/skills has never listed the id, so
    /// rung 3 misses too. Without the override table it renders as
    /// "Skill 1066".
    #[test]
    fn resolve_name_uses_the_override_table_for_an_id_no_other_rung_knows() {
        assert_eq!(resolve_name(1066, Some("1066")), "Resurrect");
        assert_eq!(resolve_name(1066, None), "Resurrect");
    }

    /// The override table must never reinstate the placeholder shape it
    /// exists to remove.
    #[test]
    fn resolve_name_never_returns_a_numeric_or_empty_name() {
        for &(id, _) in super::super::skill_name_overrides::SKILL_NAME_OVERRIDES {
            let resolved = resolve_name(id, Some("  "));
            assert!(!resolved.trim().is_empty());
            assert!(!resolved.chars().all(|c| c.is_ascii_digit()));
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p axilog-core --lib resolve_name_uses_the_override_table 2>&1 | tail -20
```
Expected: FAIL — `assertion \`left == right\` failed: left: "Skill 1066",
right: "Resurrect"`.

- [ ] **Step 3: Add the rung, override-FIRST for now**

In `crates/axilog-core/src/analysis/skill_map.rs`, replace the body of
`resolve_name` with:

```rust
pub fn resolve_name(id: u32, raw_name: Option<&str>) -> String {
    if let Some(n) = super::skill_name_overrides::name(id) {
        return n.to_string();
    }
    let trimmed = raw_name.map(str::trim).unwrap_or("");
    let numeric_or_empty = trimmed.is_empty() || trimmed.chars().all(|c| c.is_ascii_digit());
    if !numeric_or_empty {
        return trimmed.to_string();
    }
    if let Some(n) = pseudo_name(id) {
        return n.to_string();
    }
    match super::skill_icons::name(id) {
        Some(n) => n.to_string(),
        None => format!("Skill {id}"),
    }
}
```

- [ ] **Step 4: Run them to verify they pass**

```bash
cargo test -p axilog-core --lib resolve_name 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 5: Write the measurement**

Create `crates/axilog-core/tests/name_override_precedence.rs`:

```rust
//! How much does the override table MOVE, and how much does it FIX?
//!
//! `skill_name_overrides` is unlike `skill_icon_overrides` in one way that
//! decides where it belongs in the chain: an icon override can only change
//! art nobody was reading, but a NAME override can rename an id the log
//! already named perfectly well, because GW2EI's table carries its own
//! disambiguations ("Flame Blast (Superior Sigil of Fire)" where the log
//! says "Flame Blast").
//!
//! Ranked FIRST it matches GW2EI's own `SkillItem.cs`. Ranked LAST it can
//! only ever displace the `"Skill <id>"` placeholder, which is the same
//! justification `skill_map`'s doc comment gives for ranking the API
//! catalog third.
//!
//! This test does not assert a policy. It PRINTS the two counts on a real
//! log and fails only if the moved count exceeds the threshold the module
//! doc commits to — so the decision is a recorded number rather than a
//! preference, and a future GW2EI sync that widens the table trips a build
//! instead of quietly renaming a squad's skills.

mod common;

use axilog_core::analysis::{analyze, skill_name_overrides};
use axilog_core::encounter::resolve;
use axilog_core::evtc::decode_raw;

/// Ranked first, the override table may RENAME at most this many ids that
/// some other rung already resolved on one log. Chosen from the measured
/// value — see this test's output and `skill_map::resolve_name`'s doc.
const MAX_RENAMES: usize = 0; // ← set from step 6's measurement

#[test]
fn override_table_renames_few_and_fixes_many() {
    let Some(bytes) = common::read_bytes_or_skip(
        &common::local_fixture("wvw-postrework.zevtc"),
        "name-override precedence measurement",
    ) else {
        return;
    };
    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let metrics = analyze(&enc, &raw);

    let mut renamed = Vec::new();
    let mut fixed = Vec::new();
    for (&id, log_name) in &metrics.log_skill_names {
        let Some(over) = skill_name_overrides::name(id) else { continue };
        let trimmed = log_name.trim();
        let usable = !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit());
        if usable {
            if trimmed != over {
                renamed.push((id, trimmed.to_string(), over));
            }
        } else if axilog_core::analysis::skill_icons::name(id).is_none() {
            fixed.push((id, over));
        }
    }

    println!(
        "name-override precedence on wvw-postrework: {} renamed, {} placeholder-fixes",
        renamed.len(),
        fixed.len()
    );
    for (id, was, now) in renamed.iter().take(20) {
        println!("  RENAME {id}: {was:?} -> {now:?}");
    }
    for (id, now) in fixed.iter().take(20) {
        println!("  FIX    {id}: \"Skill {id}\" -> {now:?}");
    }

    assert!(
        renamed.len() <= MAX_RENAMES,
        "the override table renamed {} ids that already resolved (cap {MAX_RENAMES}). \
         Either GW2EI's table grew, or override-first is the wrong precedence — \
         see this module's doc comment before changing the cap.",
        renamed.len()
    );
}
```

- [ ] **Step 6: Run the measurement and record the number**

```bash
cargo test -p axilog-core --test name_override_precedence -- --nocapture 2>&1 | tail -40
```

The test skips with a printed note if `fixtures/local/wvw-postrework.zevtc`
is absent (it is gitignored). If it skips, fall back to the committed
fixture by changing the path to `"fixtures/wvw-small.anon.zevtc"` via
`common::read_bytes_or_skip` and say so in the module doc.

Now **decide**, using the printed `renamed` count:

- **`renamed` is 0-5** → keep override-first. Set `MAX_RENAMES` to the
  measured count, and append to `resolve_name`'s doc comment:

  ```rust
  /// # Why the override table ranks FIRST
  ///
  /// The order GW2EI itself uses (`SkillItem.cs` consults
  /// `OverridenSkillNames` before the API name): an entry there is a
  /// deliberate correction, so deferring to the log would reinstate
  /// exactly the name GW2EI overrode. Ranking it first is safe here
  /// because it was MEASURED: on `fixtures/local/wvw-postrework.zevtc` it
  /// renames <N> ids that already resolved and fixes <M> placeholders --
  /// see `tests/name_override_precedence.rs`, which fails if that ratio
  /// ever inverts.
  ```

- **`renamed` is more than 5** → move the override rung BELOW
  `skill_icons::name`, i.e. delete the leading `if let` block added in step
  3 and change the final `match` to:

  ```rust
      match super::skill_icons::name(id).or_else(|| super::skill_name_overrides::name(id)) {
          Some(n) => n.to_string(),
          None => format!("Skill {id}"),
      }
  ```

  Set `MAX_RENAMES = 0` (last-resort can rename nothing by construction),
  invert the assertion's message accordingly, and document the demotion in
  `resolve_name`'s doc comment with the measured count and the same
  reasoning the API catalog's third-place ranking already carries.

Either way, **the measured number goes in the doc comment.** Re-run the
step-4 tests after the change; `resolve_name(1066, Some("1066"))` must still
be `"Resurrect"` under both orderings, because no other rung can name 1066.

- [ ] **Step 7: Run the full suites**

```bash
cargo test -p axilog-core -p axilog-schema 2>&1 | tail -40
```
Expected: PASS. `skill_map_golden.rs` compares names against a real
dps.report export but does NOT hard-fail on a name mismatch (see its module
doc), so it should report movement rather than fail. If it fails, read what
it printed before touching it.

- [ ] **Step 8: Commit**

```bash
git add crates/axilog-core/src/analysis/skill_map.rs \
        crates/axilog-core/tests/name_override_precedence.rs
git commit -m "fix(names): consult GW2EI's name overrides, at a measured precedence

Closes the last of the eleven reported ids: 1066, for which arcdps writes
the numeric placeholder '1066' and /v2/skills has no entry at all, so the
override table is its only name source.

Precedence is measured rather than assumed -- unlike an icon override, a
name override can rename an id that already resolved. The count is in
resolve_name's doc comment and the test that produced it is committed, so
a future GW2EI sync that widens the table trips a build.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: Route indirect healing rows to `buffMap` as well

GW2EI's `BuildHealingDist` puts an `IndirectHealing` row's id in `buffMap`,
not `skillMap` — verified against `fixtures/local/wvw-postrework.ei.json`,
where 13721 and 77020 are in `buffMap` while 1066 and 53183 are in
`skillMap`. `blocks.healing` currently calls `reference_skill` for every row
regardless (`blocks/support.rs:567`).

`BuffEntry::name` is `buffs::name(id).unwrap_or_default()` — the **empty
string** for anything outside the boon, condition and control tables — so
registering a heal id as a buff without fixing that would emit a nameless
row. Both halves ship together.

**Files:**
- Modify: `crates/axilog-schema/src/v1/blocks/support.rs:560-575`
- Modify: `crates/axilog-schema/src/v1/catalogs.rs:306`
- Test: `crates/axilog-schema/src/v1/catalogs.rs` (`mod tests`) and
  `crates/axilog-schema/tests/v1_healing_detail.rs`

**Interfaces:**
- Consumes: Task 1's `Metrics.log_skill_names` and public `resolve_name`.
- Produces: no new signature.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/axilog-schema/src/v1/catalogs.rs`:

```rust
    /// A healing-over-time id is neither boon nor condition nor control
    /// effect, so `buffs::name` misses and the entry used to be emitted
    /// with an EMPTY name — worse than a placeholder, because a consumer
    /// cannot even tell it failed.
    #[test]
    fn finish_names_a_buff_that_is_not_a_boon_or_condition() {
        let mut metrics = metrics_with_skills();
        metrics.log_skill_names.insert(13721, "Restorative Mantras".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_buff(13721);
        let catalogs = cats.finish(&metrics, None);

        let entry = &catalogs.buffs[&13721];
        assert_eq!(entry.name, "Restorative Mantras");
        assert_eq!(entry.kind, "effect", "not a boon and not a condition");
    }

    /// The boon and condition tables stay authoritative for the ids they
    /// cover — this fallback is purely additive.
    #[test]
    fn finish_still_prefers_the_boon_table_for_a_boon() {
        let mut metrics = metrics_with_skills();
        metrics.log_skill_names.insert(740, "SHOULD NOT WIN".to_string());

        let mut cats = CatalogBuilder::default();
        cats.reference_buff(740);
        let catalogs = cats.finish(&metrics, None);

        assert_eq!(catalogs.buffs[&740].name, "Might");
        assert_eq!(catalogs.buffs[&740].kind, "boon");
    }
```

Add to `crates/axilog-schema/tests/v1_healing_detail.rs`:

```rust
/// GW2EI's `BuildHealingDist` routes an indirect (healing-over-time) row's
/// id into `buffMap` and a direct row's into `skillMap` — checked against
/// `fixtures/local/wvw-postrework.ei.json`, where 13721 and 77020 are
/// buffs while 1066 and 53183 are skills. The block registered every row
/// as a skill regardless.
#[test]
fn indirect_heal_rows_register_as_buffs_and_direct_rows_do_not() {
    let (report_v1, detail) = healing_report_and_detail();
    let mut indirect = std::collections::BTreeSet::new();
    let mut direct = std::collections::BTreeSet::new();
    for player in &detail {
        for e in player.healing_dist.iter().chain(player.barrier_dist.iter()) {
            if e.indirect { indirect.insert(e.skill_id); } else { direct.insert(e.skill_id); }
        }
    }
    assert!(!indirect.is_empty(), "fixture must carry at least one indirect heal row");

    for id in &indirect {
        assert!(
            report_v1.catalogs.buffs.contains_key(id),
            "indirect heal id {id} must resolve in the buff catalog"
        );
    }
    for id in direct.difference(&indirect) {
        assert!(
            !report_v1.catalogs.buffs.contains_key(id)
                || axilog_core::analysis::buffs::name(*id).is_some(),
            "a purely-direct heal id {id} must not be invented as a buff"
        );
    }
}
```

`healing_report_and_detail()` does not exist yet. Add it to the same file,
modelled on the existing setup at `v1_healing_detail.rs:329`:

```rust
/// The committed WvW fixture parsed to a v1 report with the healing gate
/// on, alongside the raw `HealingDetail` the block was built from.
fn healing_report_and_detail() -> (
    axilog_schema::v1::ReportV1,
    axilog_core::analysis::healing_detail::HealingDetail,
) {
    let bytes = std::fs::read("../../fixtures/wvw-small.anon.zevtc")
        .expect("committed WvW fixture must be readable");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::encounter::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let detail = axilog_core::analysis::healing_detail::build(&raw, &enc)
        .expect("fixture carries the healing extension");
    let legacy = axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), None, None, true, true, true, None,
    );
    let passes = axilog_schema::v1::Passes {
        healing_detail: Some(&detail),
        healing_series: Some(&detail),
        ..Default::default()
    };
    let v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &legacy, env!("CARGO_PKG_VERSION"), None, &passes,
    );
    (v1, detail)
}
```

If `Passes` does not implement `Default`, or `build_report`'s arity differs,
copy the exact construction from `crates/axilog-schema/tests/common/mod.rs:89`
rather than guessing — that helper already builds this shape.

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test -p axilog-schema finish_names_a_buff indirect_heal_rows 2>&1 | tail -30
```
Expected: FAIL — `assertion \`left == right\` failed: left: "", right:
"Restorative Mantras"` for the first, and a missing buff-catalog key for the
second.

- [ ] **Step 3: Give `BuffEntry::name` the shared chain**

In `crates/axilog-schema/src/v1/catalogs.rs`, replace the `name:` line at
`:306`:

```rust
                        // The boon/condition/control tables first — they
                        // are authoritative for the ids they cover. Beyond
                        // them this catalog now carries ids that are none
                        // of the three (a healing-over-time skill routed
                        // here by `blocks.healing`, per GW2EI's own
                        // BuildHealingDist), and `unwrap_or_default` gave
                        // those an EMPTY name: a consumer could not even
                        // tell the lookup had failed. Same chain as the
                        // skill half, for the same reason.
                        name: buffs::name(id).map(str::to_owned).unwrap_or_else(|| {
                            metrics
                                .skill_map
                                .get(&id)
                                .map(|e| e.name.clone())
                                .unwrap_or_else(|| {
                                    axilog_core::analysis::skill_map::resolve_name(
                                        id,
                                        metrics.log_skill_names.get(&id).map(String::as_str),
                                    )
                                })
                        }),
```

- [ ] **Step 4: Split the registration in `blocks/support.rs`**

In `crates/axilog-schema/src/v1/blocks/support.rs`, inside the `dist`
closure (`:560-575`), replace the single `cats.reference_skill(e.skill_id);`
at `:567` with:

```rust
                // Every id this block joins on has to resolve in the
                // catalog, or the row is a dangling reference — the same
                // hole Task 9 found on the damage side.
                //
                // WHICH catalog follows GW2EI's `BuildHealingDist`: an
                // indirect (healing-over-time) row's id goes to `buffMap`,
                // a direct row's to `skillMap`. Checked against
                // `fixtures/local/wvw-postrework.ei.json`, where 13721 and
                // 77020 are buffs while 1066 and 53183 are skills.
                //
                // An indirect id lands in BOTH, deliberately. It stays a
                // skill reference so it keeps a `SkillEntry` with the log's
                // own name and art, and gains a buff reference so a
                // consumer following EI's routing finds it. Same superset
                // this catalog already carries for Stun and Daze, and for
                // the same reason: a consumer only ever looks up ids it
                // already holds, so the extra entry is inert. Do not
                // narrow it.
                cats.reference_skill(e.skill_id);
                if e.indirect {
                    cats.reference_buff(e.skill_id);
                }
```

- [ ] **Step 5: Run them to verify they pass**

```bash
cargo test -p axilog-schema 2>&1 | tail -40
```
Expected: PASS. `buffMap`/`catalogs.buffs` counts move, so a size or
count-asserting golden may fail — for example
`buff_map_covers_the_12_tracked_boons_with_computed_fields_only`
(`axilog-ei/src/lib.rs:3183`) asserts exactly 12 entries, but it builds from
a synthetic report with no healing rows, so it should be unaffected. If a
golden fails on a COUNT, refresh it and state the delta in the commit
message. If one fails on a NAME that changed, stop and report.

- [ ] **Step 6: Commit**

```bash
git add crates/axilog-schema/src/v1/blocks/support.rs \
        crates/axilog-schema/src/v1/catalogs.rs \
        crates/axilog-schema/tests/v1_healing_detail.rs
git commit -m "fix(names): route indirect heal rows to the buff catalog, and name them

GW2EI's BuildHealingDist puts a healing-over-time row's id in buffMap and
a direct row's in skillMap; blocks.healing registered every row as a skill
regardless. Indirect ids now land in both -- a deliberate superset, on the
same reasoning this catalog already records for Stun and Daze.

BuffEntry::name was buffs::name(id).unwrap_or_default(), so any id outside
the boon/condition/control tables was emitted with an EMPTY name. It now
falls through the same chain as the skill half.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: The leak test

This is what makes the fix systemic rather than one more patch. Tasks 1-5
close the healing gap; this stops the next array from opening a new one
silently.

**Files:**
- Create: `crates/axilog-ei/tests/name_leak_golden.rs`

**Interfaces:**
- Consumes: everything above. No new API.

- [ ] **Step 1: Write the test**

Create `crates/axilog-ei/tests/name_leak_golden.rs`:

```rust
//! No emitted id may go unnamed. MNAME's standing guard.
//!
//! A WvW player reported eleven skills rendering as the literal string
//! `"Skill <id>"` in AxiBridge's healing tables. The cause was structural
//! rather than a missing entry: `blocks.healing` referenced ids that
//! `analysis::skill_map`'s damage-and-rotation scope never covered, and
//! `CatalogBuilder::finish` had no access to the log's own skill table to
//! name them with. Any block could have had the same hole; healing is
//! simply the one someone noticed.
//!
//! So this test does not check healing. It walks EVERY id-bearing array in
//! the emitted EI JSON and asserts each id resolves, in `skillMap` or
//! `buffMap`, to a name that is neither the `Skill <id>` placeholder nor
//! empty. The empty case matters as much as the placeholder: `BuffEntry`'s
//! name used to default to `""`, which a consumer cannot even detect as a
//! failure.
//!
//! Ids no source can name get an explicit allowlist below, each with a
//! reason. That is the point: the allowlist growing is a visible diff in
//! review, which a placeholder appearing in a Discord screenshot is not.

mod common;

use std::collections::BTreeSet;

/// Ids that legitimately have no name in any source we carry.
///
/// EVERY entry needs a reason. If you are adding one to make a build pass,
/// the question to answer first is why no catalog knows the id -- an entry
/// here is a permanent admission, not a silencer.
const UNNAMEABLE: &[(u32, &str)] = &[
    // GW2EI's twelve Weaver dual-attunement pseudo ids (-5..-16) are named
    // from EI's BUFF table, not from OverridenSkillNames -- see
    // `skill_map::PSEUDO_SKILL_NAMES`'s "The one gap" section.
];

fn placeholder(name: &str, id: u32) -> bool {
    name.trim().is_empty() || name == format!("Skill {id}")
}

/// Every id-bearing array in the emitted document, by the path a reader
/// would use to find it. Extend this list when a new array ships -- an
/// array not listed here is not guarded.
fn collect_ids(v: &serde_json::Value) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut push = |path: &str, val: &serde_json::Value| {
        if let Some(id) = val.get("id").and_then(|i| i.as_i64()) {
            out.push((path.to_string(), id as u32));
        }
    };

    let players = v["players"].as_array().cloned().unwrap_or_default();
    for (i, p) in players.iter().enumerate() {
        for key in ["totalDamageDist", "totalHealingDist", "totalBarrierDist", "totalDamageTaken"] {
            // Phase-then-row nesting: `[phase][row]`.
            if let Some(phases) = p[key].as_array() {
                for rows in phases {
                    for row in rows.as_array().into_iter().flatten() {
                        push(&format!("players[{i}].{key}"), row);
                    }
                }
            }
        }
        // `targetDamageDist` adds a target level: `[target][phase][row]`.
        for targets in p["targetDamageDist"].as_array().into_iter().flatten() {
            for phases in targets.as_array().into_iter().flatten() {
                for row in phases.as_array().into_iter().flatten() {
                    push(&format!("players[{i}].targetDamageDist"), row);
                }
            }
        }
        for phases in p["rotation"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].rotation"), phases);
        }
        for row in p["buffUptimes"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].buffUptimes"), row);
        }
        for row in p["damageModifiers"].as_array().into_iter().flatten() {
            push(&format!("players[{i}].damageModifiers"), row);
        }
    }
    for (i, t) in v["targets"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        for row in t["buffs"].as_array().into_iter().flatten() {
            push(&format!("targets[{i}].buffs"), row);
        }
    }
    out
}

/// The floor that stops this test passing vacuously.
///
/// `collect_ids` hardcodes each array's nesting depth, and EI's shapes are
/// not uniform -- `totalDamageDist` is `[phase][row]`, `targetDamageDist`
/// is `[target][phase][row]`, `rotation` is its own thing again. Get one
/// wrong and the walker silently yields nothing for that array, so the
/// invariant holds over an empty set and the guard guards nothing. This
/// floor is per-array, not a total, because a single fat array could
/// otherwise mask three empty ones.
fn assert_walker_reaches_every_array(found: &[(String, u32)], label: &str) {
    let arrays = ["totalDamageDist", "totalHealingDist", "totalDamageTaken",
                  "targetDamageDist", "rotation", "buffUptimes"];
    for want in arrays {
        assert!(
            found.iter().any(|(path, _)| path.contains(want)),
            "{label}: the walker found ZERO ids in {want}. Either the fixture \
             genuinely has none, or `collect_ids` has the wrong nesting depth \
             for it -- check against the emitted JSON before relaxing this."
        );
    }
}

fn check_no_leaks(v: &serde_json::Value, label: &str) {
    let allowed: BTreeSet<u32> = UNNAMEABLE.iter().map(|&(id, _)| id).collect();
    let skills = v["skillMap"].as_object().cloned().unwrap_or_default();
    let buffs = v["buffMap"].as_object().cloned().unwrap_or_default();

    let found = collect_ids(v);
    assert_walker_reaches_every_array(&found, label);

    let mut leaks: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for (path, id) in found {
        if allowed.contains(&id) || !seen.insert(id) {
            continue;
        }
        let skill = skills.get(&format!("s{id}")).and_then(|e| e["name"].as_str());
        let buff = buffs.get(&format!("b{id}")).and_then(|e| e["name"].as_str());
        let resolved = match (skill, buff) {
            (Some(n), _) if !placeholder(n, id) => continue,
            (_, Some(n)) if !placeholder(n, id) => continue,
            (Some(n), _) | (_, Some(n)) => format!("{n:?}"),
            (None, None) => "absent from BOTH maps".to_string(),
        };
        leaks.push(format!("  {path}: id {id} -> {resolved}"));
    }

    assert!(
        leaks.is_empty(),
        "{label}: {} id(s) emitted with no usable name:\n{}\n\
         Either a name source is missing the id, or the id belongs in \
         UNNAMEABLE with a reason. Do not add it without one.",
        leaks.len(),
        leaks.join("\n")
    );
}
```

- [ ] **Step 2: Add the two gate configurations**

Append to the same file. Read
`crates/axilog-ei/tests/meigap3_ei_golden.rs:80-100` first and mirror its
exact parse-and-emit construction — the adapter's entry point and `EiInputs`
shape are not guessable from this plan, and using the wrong one silently
tests a document with fewer arrays than production emits.

```rust
/// Arrays appear and disappear with the gates, so the invariant is checked
/// under both configurations the adapter supports. A flagless parse emits
/// the smaller document; the all-gates parse is where `targetDamageDist`,
/// `totalHealingDist` and `damageModifiers` actually exist.
#[test]
fn no_emitted_id_goes_unnamed_with_all_gates_on() {
    let Some(v) = ei_json_for_fixture(/* all gates */ true) else { return };
    check_no_leaks(&v, "all gates on");
}

#[test]
fn no_emitted_id_goes_unnamed_with_default_gates() {
    let Some(v) = ei_json_for_fixture(/* all gates */ false) else { return };
    check_no_leaks(&v, "default gates");
}
```

Write `ei_json_for_fixture(all_gates: bool) -> Option<serde_json::Value>`
against `fixtures/wvw-small.anon.zevtc` (committed, and it carries the
healing extension — `healing_golden.rs:344` relies on that), returning
`None` via `common::read_bytes_or_skip` if it is unreadable.

- [ ] **Step 3: Run it — expect it to PASS**

```bash
cargo test -p axilog-ei --test name_leak_golden -- --nocapture 2>&1 | tail -40
```

Expected: PASS. Tasks 1-5 are what make it pass; this step is confirming
them, not driving new code.

**If it fails on `assert_walker_reaches_every_array`**, the walker's nesting
depth is wrong for that array, not the fix. Dump the emitted JSON and read
the actual shape:

```bash
cargo run -p axilog-cli -- --format json fixtures/wvw-small.anon.zevtc \
  | python3 -c "import json,sys; p=json.load(sys.stdin)['players'][0]; \
    [print(k, json.dumps(p.get(k))[:200]) for k in \
     ('totalDamageDist','totalHealingDist','targetDamageDist','rotation','buffUptimes')]"
```

**If it fails on a leak**, the failure message lists every leaking id and its path.
That is a genuine finding — one of Tasks 1-5 did not reach that array.
Diagnose it before considering the allowlist. The allowlist is for ids no
source *can* name, not for ids no source *does* name yet.

- [ ] **Step 4: Prove the test can fail**

A guard that cannot fail guards nothing. Temporarily revert Task 1's change
to `catalogs.rs` (restore the `.or_else(skill_icons::name)` chain), re-run,
and confirm the test now reports the heal-only ids by path. Then restore the
fix.

```bash
git stash list  # ensure a clean start
# edit catalogs.rs back to the pre-Task-1 name chain by hand
cargo test -p axilog-ei --test name_leak_golden 2>&1 | tail -30   # expect FAIL, listing ids
git checkout crates/axilog-schema/src/v1/catalogs.rs
cargo test -p axilog-ei --test name_leak_golden 2>&1 | tail -10   # expect PASS
```

Record the number of ids the reverted run reported in the test's module doc:

```rust
//! Verified to actually fail: reverting the `CatalogBuilder::finish` name
//! chain reintroduces <N> leaking ids on the committed WvW fixture.
```

- [ ] **Step 5: Commit**

```bash
git add crates/axilog-ei/tests/name_leak_golden.rs
git commit -m "test(names): assert no emitted id renders as a placeholder

Walks every id-bearing array in the emitted EI JSON -- damage, healing,
barrier, target damage, rotation, buff uptimes, target buffs, damage
modifiers -- under both gate configurations, and asserts each id resolves
in skillMap or buffMap to a name that is neither 'Skill <id>' nor empty.

Ids no source can name need an explicit allowlist entry with a reason, so
the allowlist growing is a visible diff in review -- which a placeholder
appearing in a Discord screenshot is not.

Verified to fail when the fix is reverted.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: Full validation, size check, and release

**Files:**
- Modify: `docs/CHANGELOG.md` (a missing section kills the Release job
  AFTER npm-publish — see the standing rule)
- Modify: workspace version files (via `scripts/workspace-version.sh`)

- [ ] **Step 1: Run the whole workspace suite**

```bash
cd /var/home/mstephens/Documents/GitHub/axilog
cargo test --workspace 2>&1 | tail -60
```
Expected: PASS. Any golden that fails on a COUNT (skill map / buff map sizes)
is expected movement — refresh it and state the delta. Any golden that fails
on a NAME that CHANGED is a finding: stop and report.

- [ ] **Step 2: Measure the size delta**

The spec commits to measuring it. `crates/axilog-schema/tests/v1_size.rs`
already builds a report from the fixture; run it and record the before/after
`catalogs` bytes:

```bash
cargo test -p axilog-schema --test v1_size -- --nocapture 2>&1 | tail -30
```

Record the number in the CHANGELOG entry. No gate is anticipated — both maps
are short rows against a `report.json` whose bulk is `replayFights` — but an
unmeasured "it's small" is not a measurement.

- [ ] **Step 3: Verify against a real EI export**

The standing version-bump rule: diff the parsed output on the fixture rather
than trusting the changelog, because `skillMap` and `buffMap` both change
shape here.

`cargo run -p axilog-cli -- --format json` emits the NATIVE container
(top-level `axilog`/`encounter`/`entities`/`catalogs`/`blocks`/`coverage`),
not EI's `skillMap`/`buffMap` keys — those live at `catalogs.skills` and
`catalogs.buffs` here. Run with `--all` so every gated block (rotation,
skill-damage, timeseries, modifiers) is present, the same coverage
`no_emitted_id_goes_unnamed_with_all_gates_on` exercises:

```bash
cargo run -p axilog-cli -- parse --format json --all fixtures/wvw-small.anon.zevtc > /tmp/mname-after.json
python3 - <<'PY'
import json
a = json.load(open('/tmp/mname-after.json'))
c = a.get('catalogs', {})
sm, bm = c.get('skills', {}), c.get('buffs', {})
UNNAMEABLE = {41166, 42264, 43470, 44857, 30060, 31311, 54960, 69665}  # name_leak_golden.rs
def placeholder(k, v):
    name = str(v.get('name', ''))
    if not name.strip():
        return True
    n = int(k)
    return name == f"Skill {n}" or name == f"Skill {n & 0xFFFFFFFF}"
bad = [(k, v.get('name')) for k, v in list(sm.items()) + list(bm.items()) if placeholder(k, v)]
unexpected = [(k, n) for k, n in bad if int(k) not in UNNAMEABLE]
print(f"skills {len(sm)} entries, buffs {len(bm)} entries, {len(bad)} unnamed")
for k, n in bad:
    print(' ', k, repr(n), "OK (UNNAMEABLE)" if int(k) in UNNAMEABLE else "UNEXPECTED")
print("FAIL:", unexpected) if unexpected else print("PASS: every unnamed id is on UNNAMEABLE")
PY
```

Note: `--format json` goes through the `axilog-api` facade
(`crates/axilog-cli/src/main.rs`'s M17 Task 3 comment), not the CLI's own
`Passes` literal — that is the intended path here, since it is what
AxiBridge consumes.

Expected: **7 unnamed** on the committed fixture, all seven ids on the
`UNNAMEABLE` allowlist in `crates/axilog-ei/tests/name_leak_golden.rs` —
"0 unnamed" is wrong, since four Weaver dual-attunement ids and four
unlisted internal ids are permanent, documented gaps, not a bug. The
assertion that matters is that no unnamed id falls OUTSIDE that allowlist.
An id that shows up as `UNEXPECTED` here, or that the leak test did not
catch, means an array is emitted that `collect_ids` does not walk — add it to
Task 6's collector.

- [ ] **Step 4: Write the CHANGELOG entry**

Add a section to `docs/CHANGELOG.md` for the new version. Cover: the
placeholder fix and its cause, the indirect-heal buffMap routing, the new
generated catalog, the measured override-precedence decision and its count,
the size delta, and the leak test. Say explicitly that AxiBridge needs no
code change.

- [ ] **Step 5: Bump the version and tag from `main`**

```bash
git status --porcelain   # must be clean
git branch --show-current  # must be main
./scripts/workspace-version.sh <new-version>
./scripts/check-versions.sh
cargo test --workspace 2>&1 | tail -20
git add -A && git commit -m "chore: release v<new-version>"
git tag v<new-version>
git show v<new-version>:Cargo.toml | grep '^version'   # must be the NEW version
```

The last line is the standing check from the double-tag incident: verify the
tag points at the commit carrying the new version, not one commit early. If
it is wrong, force-move the tag before pushing.

- [ ] **Step 6: Push and watch CI**

```bash
git push origin main && git push origin v<new-version>
```

Then show a live CI watcher card rather than pasting an Actions URL.

- [ ] **Step 7: Bump axilog in AxiBridge and verify end-to-end**

In `../axibridge`, bump the axilog dependency to the new version and re-run
the reporter's scenario. Confirm in the rendered Support & Healing table
that 1066 reads `Resurrect` and 13721 reads `Restorative Mantras`, and that
no row reads `Skill <id>`.

Remember `npm install --package-lock-only` if the lockfile needs repair, and
do NOT hand-write versionless platform stubs — pin exactly.

- [ ] **Step 8: Close the Discord thread**

Only after the user confirms the fix is live. Thread `1542191505911971860`;
post a summary comment, then PATCH `{'applied_tags':
['1494033603590488194'], 'archived': True}`. Pipe all JSON through
`python3 -c "...json.dumps(...)"` into `curl -d @-`.

In the summary, say what was NOT a bug as well as what was: `Journey`,
`Friendly Fire`, `Nightmare Weapon`, `Continuum Split` and `Bandage` are the
correct GW2 API names for those relic and trait heal procs, and the two
`Radiant Resolve` rows are two genuinely distinct ids (78514, 78604) that
share a name in ArenaNet's own data. The reporter took the trouble to list
them; they deserve an answer, not silence.
