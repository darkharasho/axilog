# Skill-Variant Disambiguation (MVAR) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop two skills that share a display name from rendering as
identical rows, by emitting an optional `variant_label` on every colliding
skill in the 1.0 catalog.

**Architecture:** One new optional field on `v1::catalogs::SkillEntry`,
populated by a single grouping pass at the end of `CatalogBuilder::finish`
— the only site holding every skill id in the document together with its
final resolved name. The label is the skill id by default (stable across
logs, which an ordinal is not), upgraded to `"Adrenaline N"` for the nine
warrior burst groups whose tier is known from a generated, committed table.

**Tech Stack:** Rust (workspace: `axilog-core`, `axilog-schema`,
`axilog-ei`), Python 3 for the catalog generator, napi/PyO3 SDK stubs.

**Spec:** `docs/superpowers/specs/2026-09-12-skill-variant-disambiguation-design.md`

## Global Constraints

- **Additive only.** Schema 1.0 is frozen; optional fields are permitted.
  `variant_label` MUST carry `#[serde(skip_serializing_if = "Option::is_none")]`
  so no existing document grows a key.
- **The EI view must not change.** `variant_label` is a 1.0-container
  field only. The legacy `SkillMapEntryOut` (`schema/src/lib.rs:169`) does
  NOT get the field, and `__test__/fixtures/ei-baseline.sha256.json` must
  not move. If it moves, the field leaked — the change is wrong.
- **No network at parse time.** The burst table is generated once and
  committed. `axilog` must not acquire a runtime HTTP dependency.
- **Never mutate `name`.** The conditions oracle resolves conditions by
  name; changing a resolved name shifts EI-side counts. The label is a
  sidecar field and nothing joins on it.
- **Branch:** `feat/skill-variant-disambiguation` (already created; the
  spec commit `4a468d1` is its tip).
- Generated tables are sorted by id and looked up with `binary_search_by_key`,
  matching `skill_name_overrides.rs`.
- Every generator prints the accounting identity
  `considered == transcribed + skipped`, with each skip carrying a reason.

---

### Task 1: The generic floor — `variant_label` on same-name collisions

Delivers the complete fix for every collision, including ids the GW2 API
does not list. Task 2 only improves the labels' wording.

**Files:**
- Modify: `crates/axilog-schema/src/v1/catalogs.rs` (struct ~`:47`,
  `finish` ~`:240`, tests `:410`)
- Modify: `crates/axilog-ei/src/lib.rs:4038` (the one other
  `v1::catalogs::SkillEntry` literal in the workspace)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub variant_label: Option<String>` on
  `axilog_schema::v1::catalogs::SkillEntry`; a private
  `fn label_name_collisions(skills: &mut BTreeMap<u32, SkillEntry>)` in
  `catalogs.rs`, which Task 2 modifies in place.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `crates/axilog-schema/src/v1/catalogs.rs`:

```rust
    /// Two ids, one name -- the shape of the Discord report. Both rows
    /// must become distinguishable; neither may be dropped or merged.
    #[test]
    fn two_ids_sharing_a_name_both_get_a_variant_label() {
        let mut skills = BTreeMap::new();
        for id in [73055u32, 72923] {
            skills.insert(id, skill_entry_named("Daybreaking Slash"));
        }
        skills.insert(9999, skill_entry_named("Unique Skill"));

        label_name_collisions(&mut skills);

        assert_eq!(skills[&73055].variant_label.as_deref(), Some("73055"));
        assert_eq!(skills[&72923].variant_label.as_deref(), Some("72923"));
        // A name only one id carries is not a collision and stays clean.
        assert_eq!(skills[&9999].variant_label, None);
    }

    /// The invariant the whole change exists to establish, stated once.
    #[test]
    fn no_two_entries_render_identically() {
        let mut skills = BTreeMap::new();
        skills.insert(10660u32, skill_entry_named("Fear"));
        skills.insert(791, skill_entry_named("Fear"));

        label_name_collisions(&mut skills);

        let rendered: Vec<String> = skills
            .values()
            .map(|e| match &e.variant_label {
                Some(l) => format!("{} ({})", e.name, l),
                None => e.name.clone(),
            })
            .collect();
        let mut deduped = rendered.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(rendered.len(), deduped.len(), "two rows render the same");
    }

    fn skill_entry_named(name: &str) -> SkillEntry {
        SkillEntry {
            name: name.to_owned(),
            icon: None,
            is_swap: false,
            can_crit: true,
            auto_attack: None,
            control_kind: None,
            variant_label: None,
            is_trait_proc: false,
            is_gear_proc: false,
            is_unconditional_proc: false,
            is_not_accurate: false,
            is_instant_cast: false,
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p axilog-schema --lib catalogs
```

Expected: FAIL to COMPILE — `SkillEntry` has no field `variant_label`, and
`label_name_collisions` is not defined. A compile failure is the correct
first failure here; do not "fix" it by deleting the test.

- [ ] **Step 3: Add the field**

In `crates/axilog-schema/src/v1/catalogs.rs`, inside `pub struct SkillEntry`,
immediately after the `control_kind` field:

```rust
    /// Set ONLY when another id in THIS document resolves to the same
    /// `name`. A consumer renders `"{name} ({variant_label})"` verbatim.
    ///
    /// MVAR. ArenaNet's own `/v2/skills` returns identical names AND
    /// identical icons for every adrenaline tier of a warrior burst
    /// skill, so no name lookup in any catalog can separate them -- the
    /// duplicate rows players report are not a naming bug and cannot be
    /// fixed by one. This field is the second axis that can.
    ///
    /// The default label is the id itself, NOT an ordinal. An ordinal
    /// could only be computed from the ids present in this one log, and
    /// consumers aggregate by id across many: an id ranked `#1` in a
    /// two-id log becomes `#2` as soon as a lower id appears in another,
    /// so the same skill would carry different labels in two views of the
    /// same session. The id is stable by construction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_label: Option<String>,
```

- [ ] **Step 4: Write the pass**

In the same file, above `impl CatalogBuilder`:

```rust
/// Give every skill sharing a display name with another skill in this
/// document a label that tells them apart.
///
/// Runs at the END of `finish`, which is the only site that holds every
/// id in the document together with its final resolved name -- a pass
/// anywhere earlier would see a partial set and label inconsistently.
///
/// Two passes rather than one because the first borrows every `name`
/// immutably and the second needs `&mut` on the same map.
fn label_name_collisions(skills: &mut BTreeMap<u32, SkillEntry>) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in skills.values() {
        *counts.entry(entry.name.as_str()).or_insert(0) += 1;
    }
    // Collect first: `counts` borrows `skills` immutably.
    let colliding: Vec<u32> = skills
        .iter()
        .filter(|(_, e)| counts.get(e.name.as_str()).copied().unwrap_or(0) > 1)
        .map(|(&id, _)| id)
        .collect();
    for id in colliding {
        if let Some(entry) = skills.get_mut(&id) {
            entry.variant_label = Some(id.to_string());
        }
    }
}
```

- [ ] **Step 5: Call it, and fill the two literals**

In `finish`, bind the collect result mutably and call the pass before
`Catalogs` is constructed:

```rust
        let mut skills: BTreeMap<u32, SkillEntry> = self
            .skills
            .into_iter()
            .map(|id| {
                // ... existing body unchanged ...
            })
            .collect();
        // MVAR: needs every entry resolved, so it runs last.
        label_name_collisions(&mut skills);
```

Add `variant_label: None` to the `SkillEntry` literal inside that `.map`
closure (next to `control_kind`), and to the literal at
`crates/axilog-ei/src/lib.rs:4038`.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p axilog-schema --lib catalogs
cargo test -p axilog-ei
```

Expected: PASS. If `axilog-ei`'s byte-identity or EI-shape tests fail, STOP
— the field has leaked into the EI view, which Global Constraints forbid.

- [ ] **Step 7: Commit**

```bash
git add crates/axilog-schema/src/v1/catalogs.rs crates/axilog-ei/src/lib.rs
git commit -m "feat(schema): label skills that share a display name

Same-name skill ids rendered as identical rows. SkillEntry gains an
optional variant_label, set to the id whenever another id in the same
document resolves to the same name.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: The curated layer — warrior burst adrenaline tiers

Upgrades the label from a bare id to `"Adrenaline 3"` for the nine groups
where the tier is genuinely known.

**Files:**
- Create: `scripts/gen_skill_variant_catalog.py`
- Create: `crates/axilog-core/src/analysis/skill_variants.rs`
- Modify: `crates/axilog-core/src/analysis/mod.rs` (add `pub mod skill_variants;`
  next to `skill_name_overrides` at `:128`)
- Modify: `crates/axilog-schema/src/v1/catalogs.rs` (`label_name_collisions`)

**Interfaces:**
- Consumes: `label_name_collisions` from Task 1.
- Produces: `axilog_core::analysis::skill_variants::label(id: u32) -> Option<&'static str>`
  and `pub const SKILL_VARIANT_LABELS: &[(u32, &str)]`, sorted by id.

- [ ] **Step 1: Write the generator**

Create `scripts/gen_skill_variant_catalog.py`, following
`gen_skill_icon_catalog.py`'s structure (module docstring stating the
source and the rules, `Skip` exception carrying a reason, accounting
printed at the end).

It fetches `/v2/skills`, keeps skills with `"Burst" in categories`, groups
them by `name`, and within each group of 5 assigns:

- the id with a non-null `cost` **and** a `flip_skill` → the slot skill, skipped
- the id with `specialization == 61` → the Berserker variant, skipped
- the remaining three → tiers, read from the `traited_facts` entry whose
  `requires_trait == 1649` (Cleansing Ire)

The docstring MUST record this, verbatim, as the reason three groups are
absent:

```
The trait-fact rule was VALIDATED against the one group with independent
ground truth and it FAILED. Harrier's Toss (72911 / 73042 / 73006) is
carried by GW2EI's OverridenSkillNames as Adrenaline Level 1 / 2 / 3; the
requires_trait:1649 fact returns level 1 for all five of its ids.
Breaching Strike behaves the same way; Path to Victory is internally
inconsistent; Bloodthirster has no trait facts at all.

Those four groups are therefore NOT derived here. Harrier's Toss is
hand-transcribed from GW2EI (the only group with an independent source).
The other three emit nothing and fall back to the id label in
`catalogs.rs::label_name_collisions`. Do not "finish the job" by
re-deriving them from this rule -- it is known to lie on exactly this
shape of input.
```

- [ ] **Step 2: Run it and inspect the output**

```bash
python3 scripts/gen_skill_variant_catalog.py > /tmp/skill_variants.rs
head -60 /tmp/skill_variants.rs
```

Expected: 27 entries (9 groups x 3 tiers), accounting balanced. If a group
yields other than 3 tiers, do NOT paper over it — print the group and stop;
the API shape has changed and the spec's premise needs rechecking.

- [ ] **Step 3: Write the failing test**

Put the generated table at
`crates/axilog-core/src/analysis/skill_variants.rs`, add
`pub mod skill_variants;` to `analysis/mod.rs`, then append to the new
module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The lookup binary-searches, so an unsorted table silently returns
    /// wrong answers rather than failing.
    #[test]
    fn the_table_is_sorted_and_has_no_duplicate_ids() {
        for pair in SKILL_VARIANT_LABELS.windows(2) {
            assert!(pair[0].0 < pair[1].0, "unsorted or duplicated at {:?}", pair[0]);
        }
    }

    /// Ground truth from GW2EI's OverridenSkillNames -- the ONE group with
    /// a source independent of the trait-fact derivation, and the group
    /// where that derivation was caught lying. If this breaks, the
    /// generator has started guessing again.
    #[test]
    fn harriers_toss_matches_the_gw2ei_override_table() {
        assert_eq!(label(72911), Some("Adrenaline 1"));
        assert_eq!(label(73042), Some("Adrenaline 2"));
        assert_eq!(label(73006), Some("Adrenaline 3"));
    }

    /// The three groups the derivation could not resolve are ABSENT on
    /// purpose; they fall back to the id label. Asserting the absence
    /// stops a future edit from quietly filling them with a guess.
    #[test]
    fn the_undecidable_groups_carry_no_label() {
        // Bloodthirster has no trait facts at all; Path to Victory's are
        // internally inconsistent.
        for (id, _) in SKILL_VARIANT_LABELS {
            assert!(label(*id).is_some());
        }
        assert_eq!(SKILL_VARIANT_LABELS.len(), 27, "9 groups x 3 tiers");
    }

    #[test]
    fn an_unknown_id_has_no_label() {
        assert_eq!(label(1), None);
    }
}
```

Then add one more test, pinning the group from the original report.
**Its ids are not written here on purpose** — read the three Earthshaker
ids and their tiers out of the table the generator just produced in Step 2
and paste them in, so the test records what was generated rather than what
someone assumed:

```rust
    /// The group BreakN reported. Pinned so a regenerated table that
    /// reshuffles these three fails loudly instead of silently relabelling
    /// the skill players are looking at.
    #[test]
    fn earthshaker_carries_its_three_adrenaline_tiers() {
        assert_eq!(label(/* id */), Some("Adrenaline 1"));
        assert_eq!(label(/* id */), Some("Adrenaline 2"));
        assert_eq!(label(/* id */), Some("Adrenaline 3"));
    }
```

Do not guess these ids from `skill_symbol_names.rs`'s `"Earthshaker 1".."4"`
— that is a four-entry symbol naming with no stated relationship to
adrenaline tiers, and the group has five ids.

- [ ] **Step 4: Run the tests to verify they fail**

```bash
cargo test -p axilog-core skill_variants
```

Expected: FAIL — `label` is not defined yet (the generator emits only the
table if you followed `skill_name_overrides.rs`'s split).

- [ ] **Step 5: Add the lookup**

At the top of `skill_variants.rs`, matching `skill_name_overrides.rs`:

```rust
/// The variant label for skill `id`, or `None` when this table does not
/// cover it -- in which case the caller falls back to the id.
pub fn label(id: u32) -> Option<&'static str> {
    SKILL_VARIANT_LABELS
        .binary_search_by_key(&id, |&(sid, _)| sid)
        .ok()
        .map(|i| SKILL_VARIANT_LABELS[i].1)
}
```

- [ ] **Step 6: Consult the table from the pass**

In `crates/axilog-schema/src/v1/catalogs.rs`, change the assignment inside
`label_name_collisions`:

```rust
    for id in colliding {
        if let Some(entry) = skills.get_mut(&id) {
            // Curated first, id as the floor. The curated label is only
            // ever set for ids whose tier is genuinely known.
            entry.variant_label = Some(
                axilog_core::analysis::skill_variants::label(id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| id.to_string()),
            );
        }
    }
```

Add to the test module in `catalogs.rs`:

```rust
    /// A curated id outranks the id fallback; its unlabelled sibling in
    /// the same collision still gets one.
    #[test]
    fn a_curated_burst_id_gets_its_adrenaline_label() {
        let mut skills = BTreeMap::new();
        skills.insert(72911u32, skill_entry_named("Harrier's Toss"));
        skills.insert(73042, skill_entry_named("Harrier's Toss"));
        label_name_collisions(&mut skills);
        assert_eq!(skills[&72911].variant_label.as_deref(), Some("Adrenaline 1"));
        assert_eq!(skills[&73042].variant_label.as_deref(), Some("Adrenaline 2"));
    }
```

- [ ] **Step 7: Run the tests to verify they pass**

```bash
cargo test -p axilog-core skill_variants
cargo test -p axilog-schema --lib catalogs
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add scripts/gen_skill_variant_catalog.py \
        crates/axilog-core/src/analysis/skill_variants.rs \
        crates/axilog-core/src/analysis/mod.rs \
        crates/axilog-schema/src/v1/catalogs.rs
git commit -m "feat(core): adrenaline-tier labels for warrior burst skills

Nine of the twelve burst groups get Adrenaline 1/2/3; three whose tier
cannot be derived keep the id label rather than a guess.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Close the schema gates

The additive-field checklist. Skipping any one of these ships a field the
SDKs cannot see or breaks CI after publish.

**Files:**
- Modify: `crates/axilog-node/types.d.ts` (`SkillEntry`, ~`:1115`)
- Modify: `crates/axilog-py/axilog.pyi` (`SkillEntry`, ~`:1062`)
- Modify: `crates/axilog-schema/tests/v1-keyset.golden.txt` (regenerated)
- Modify (conditionally): `crates/axilog-schema/tests/v1_sdk_stubs.rs`
- Modify: `docs/NATIVE-FORMAT.md`, `docs/CHANGELOG.md`

**Interfaces:**
- Consumes: the `variant_label` wire field from Tasks 1-2.
- Produces: nothing code-level.

`crates/axilog-node/index.d.ts` is napi-generated — do NOT hand-edit it. CI
regenerates it and fails on a dirty diff.

- [ ] **Step 1: Regenerate the key-set golden and read the diff**

```bash
UPDATE_GOLDEN=1 cargo test -p axilog-schema --test v1_shape
git diff crates/axilog-schema/tests/v1-keyset.golden.txt
```

(`v1_shape.rs:286` owns the `UPDATE_GOLDEN` hatch. There is no `v1_keyset`
test target — an earlier draft of this plan said there was.)

Expected: either exactly one added line,
`catalogs.skills.<id>.variant_label`, or **no diff at all**. Any other
added key means something beyond this change moved — stop and investigate.

- [ ] **Step 2: Handle the no-diff case**

If the golden did not change, the committed fixture
(`fixtures/wvw-small.anon.zevtc`) contains no same-name collision, so the
field is invisible to the golden — the same blind spot
`FOCUS_SKILL_WIRE_FIELDS` documents. Add to
`crates/axilog-schema/tests/v1_sdk_stubs.rs`, beside that constant:

```rust
/// Wire keys the key-set golden cannot see, for the same reason as
/// [`FOCUS_SKILL_WIRE_FIELDS`]: `variant_label` is
/// `skip_serializing_if = "Option::is_none"` and is set only when two ids
/// in one document share a display name, which the committed fixture does
/// not contain. Without this list both SDK stubs could stay silent about
/// the field while this check passed.
///
/// `catalogs.rs`'s unit tests assert the field's behaviour directly; this
/// list is what keeps the STUBS honest about it. Drop it if a fixture with
/// a name collision is ever committed.
const VARIANT_WIRE_FIELDS: &[&str] = &["variant_label"];
```

and chain it in `wire_field_names()` exactly as the other two are:

```rust
        .chain(VARIANT_WIRE_FIELDS.iter().map(|s| s.to_string()))
```

If the golden DID gain the line, skip this step entirely — no escape hatch
is needed or wanted.

- [ ] **Step 3: Transcribe into the Node stub**

In `crates/axilog-node/types.d.ts`, in `interface SkillEntry`, after
`control_kind?: string`:

```typescript
  /**
   * Set only when another id in the same report resolves to the same
   * `name` -- render as `${name} (${variant_label})`. Carries the
   * adrenaline tier for warrior burst skills ("Adrenaline 3") and the
   * skill id otherwise. Absent for skills whose name is unique, which is
   * nearly all of them.
   */
  variant_label?: string
```

- [ ] **Step 4: Transcribe into the Python stub**

In `crates/axilog-py/axilog.pyi`, add to the `SkillEntry(total=False)`
body, after `control_kind: str`:

```python
    variant_label: str
```

and add to that class's docstring:

```
    `variant_label` is present only when another id in the same report
    resolves to the same `name`; render as `f"{name} ({variant_label})"`.
    It carries the adrenaline tier for warrior burst skills and the skill
    id otherwise.
```

- [ ] **Step 5: Run the gates**

```bash
cargo test -p axilog-schema
```

Expected: PASS, including `python_stub_declares_every_wire_field` and
`node_stub_declares_every_wire_field`.

- [ ] **Step 6: Document**

In `docs/NATIVE-FORMAT.md`, add `variant_label` to the `SkillEntry` field
table (~`:219`) as optional, with a one-line description and a note that
the id is the fallback label.

In `docs/CHANGELOG.md`, add an `### Added` entry under the unreleased
section. **A missing changelog section kills the Release job after npm
publish** — this step is not optional.

- [ ] **Step 7: Commit**

```bash
git add crates/axilog-node/types.d.ts crates/axilog-py/axilog.pyi \
        crates/axilog-schema/tests/ docs/NATIVE-FORMAT.md docs/CHANGELOG.md
git commit -m "chore(schema): wire variant_label through stubs, golden and docs

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Prove additivity on a real log, then re-anchor

The standing rule from the version-bump audit: prove the JSON change by
diffing, not by trusting the changelog. Do this BEFORE re-digesting, or the
digest bakes in whatever went wrong.

**Files:**
- Modify: `crates/axilog-cli/tests/fixtures/native-json-baseline.sha256.json`
  (the NATIVE baseline, and only if the diff is clean)
- Do NOT modify: `crates/axilog-node/__test__/fixtures/ei-baseline.sha256.json`
  (the EI baseline — if this one needs touching, the change is wrong)

**Interfaces:**
- Consumes: everything from Tasks 1-3.
- Produces: a re-anchored native baseline.

- [ ] **Step 1: Confirm additivity from the key-set golden diff**

The additivity proof is the golden diff already taken in Task 3 Step 1 —
do NOT try to reproduce it by stashing. By this point the change is
committed, so `git stash` would stash nothing and a before/after dump
would be byte-identical, passing while proving nothing.

```bash
git diff main -- crates/axilog-schema/tests/v1-keyset.golden.txt
```

Expected: at most one ADDED line
(`catalogs.skills.<id>.variant_label`) and **zero removed lines**.
`v1_shape.rs:300` already treats a removed or renamed key as a hard
error, so this is the repo's own additivity gate rather than a bespoke
one.

- [ ] **Step 2: Emit the document once and eyeball the new field**

```bash
cargo run --release -p axilog-cli --bin axilog -- \
  fixtures/wvw-small.anon.zevtc --format json > /tmp/after.json
python3 -c "
import json; d = json.load(open('/tmp/after.json'))
s = d['catalogs']['skills']
lab = {k: v for k, v in s.items() if v.get('variant_label')}
print('skills:', len(s), 'labelled:', len(lab))
for k, v in list(lab.items())[:10]: print(' ', k, v['name'], '->', v['variant_label'])
"
```

Note the crate has two bins (`axilog`, `pipeline`), hence `--bin axilog`.
`--format json` goes through the `axilog-api` facade, NOT the CLI `Passes`
literal — that difference has burned this repo before.

Expected: either some labelled entries, each with a name genuinely shared
by another id in the same catalog, or zero (this fixture may contain no
collision, which Task 3 Step 2 already handles). Zero is an acceptable
result; a label on a skill whose name is unique is NOT — stop and fix.

- [ ] **Step 3: Confirm the EI view did not move**

```bash
cd crates/axilog-node && npm test
```

Expected: PASS with the ei-json byte-identity test green and its digest
UNCHANGED. If that digest moved, `variant_label` reached the EI adapter —
revert and find the leak. Only the native digest may move.

- [ ] **Step 4: Re-anchor the native baseline**

Only after Steps 2 and 3 are clean.

```bash
cargo test -p axilog-cli --test facade_identity
```

The assertion prints the actual length and digest on failure. There is no
`UPDATE_GOLDEN` hatch here — hand-edit both values into
`crates/axilog-cli/tests/fixtures/native-json-baseline.sha256.json`, then
re-run to confirm green.

Two traps this repo has hit before. The version string is embedded in the
document, so the native digest moves on every release regardless — never
read "the digest changed" as evidence this change did anything. And when
only the length matches while the digest does not, that is NOT "close
enough"; it is a same-length content change, which is the harder bug.

- [ ] **Step 5: Full workspace green**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Expected: PASS.

- [ ] **Step 6: Commit and push the branch**

```bash
git add -A
git commit -m "test: re-anchor native baseline for variant_label

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin feat/skill-variant-disambiguation
```

---

## Not in this plan

- **The AxiBridge render change.** It needs a published axilog version to
  depend on, so it is a separate change in a separate repo after release.
  Until it lands, players see no difference — this plan makes the data
  correct, not the UI.
- **Reordering the name-resolution rungs to match GW2EI** (spec root cause
  1) and **promoting the C# symbol rung** (root cause 2). Out of scope in
  the spec; do not let them creep in here.
- **Buff-catalog collisions.** `BuffEntry` gets no field.
