# MVAR — disambiguating skills that share a display name

**Status:** design, approved in chat 2026-09-12.

## The report

WvW players reported that skills appear twice or more in the same list in
AxiBridge's Skill Usage / Skill Totals section and in the player damage
breakdown ([Discord thread][thread]). Two rows, identical name, identical
icon, different numbers.

[thread]: https://discord.com/channels/1466169948035481622/1494033179533901975/1548292441075875852

BreakN diagnosed the visible cases as warrior burst skills at different
adrenaline levels — Earthshaker, Arcing Slice, Eviscerate — and asked for
the levels to be **told apart**, not merged:

> i wanna see the difference and tell which level is which

That framing is the requirement. A merge would destroy the information the
reporter asked to see.

This is the question [MNAME][mname] explicitly deferred. That spec recorded
duplicate display names as real ("distinct ids that genuinely share a name
in ArenaNet's own data") and listed "whether AxiBridge merges two rows that
share a display name" under Out of scope. This spec answers it, and answers
it on the producer side.

[mname]: 2026-08-26-mname-skill-name-resolution-design.md

## Root cause

Three distinct problems wear this one costume. Only the third is this spec's.

Measured on a committed fixture, the axilog-parsed document has **71 groups
of ids sharing a display name**, against GW2EI's 48 on the same log.

1. **Ordering divergence from EI (13 groups).** axilog ranks the log's own
   embedded name table above the curated `SKILL_NAME_OVERRIDES` table; EI
   ranks the override on top. So EI shows `Rush (Hit)`,
   `Ring of Earth (Superior Sigil of Geomancy)` and
   `Harrier's Toss (Adrenaline Level 1)` where axilog shows the bare log
   name. MNAME's change 3 made this ordering a deliberate, measured choice.
   Revisiting it is its own change with its own blast radius — thousands of
   names move — and is **not** this spec.

2. **The symbol rung is a trap.** Promoting rung 5 (`SKILL_SYMBOL_NAMES`,
   from C# `SkillIDs.cs`) above the log table "fixes" 55 of the 71 groups
   and emits internal identifiers to players: `Signet Of Renewal Skill`,
   `Quicksand Stone Spirit NPC`, `Black Powder Wv W`, `Storm Of Swords 7`.
   `skill_map.rs`'s test `symbol_rung_never_displaces_a_higher_rung` pins
   the current order on purpose. Rejected.

3. **Names that are genuinely identical at the source.** ArenaNet's own
   `/v2/skills` returns the same `name` **and the same `icon` URL** for
   every adrenaline tier of a burst skill. No lookup in any catalog — the
   API, GW2EI's overrides, the C# symbols — can separate them, because
   every catalog agrees they are called the same thing. **This is the
   reported bug, and it cannot be fixed by naming.** It needs a second
   field.

### The ceiling, measured

Some of the reported ids are not in any catalog at all:

| id | name | in `/v2/skills`? |
|---|---|---|
| 73055 | Daybreaking Slash | yes, `slot: Weapon_1`, no categories |
| 72923 | (reported as Daybreaking Slash) | **404** |
| 9284 | (reported as Flame Blast) | **404** |
| 62813 / 62940 | Deploy Jade Sphere | both yes, both `specialization: 67`, otherwise identical |

Of the 71 groups, **45 get their display name from the log's own embedded
table** — arcdps knows a name for them that no external catalog carries.

Two consequences that shape the whole design:

- A curated-name approach has a **permanent ceiling** well short of "no
  duplicate rows". An id the GW2 API 404s can never be named distinctly
  from an external source.
- The reported cases split. BreakN's warrior bursts are curatable.
  SilentTaz's `Daybreaking Slash` and `Shield of Courage` are **not burst
  skills at all** (`Shield of Courage` is a Guardian Virtue, `spec: 27`),
  and one of the Daybreaking ids does not exist upstream.

So a warrior-burst-only feature would close the diagnosis and leave the
original screenshot exactly as broken. The design therefore has a
**generic floor** that covers every collision including the 404 ids, and a
**curated layer** that adds meaning where meaning is genuinely known.

### Merging is wrong, independently

`Fear` resolves for both id 10660 and id 791, and those two ids have
**different icons**. Any collapse-by-name rule would have to pick one icon
and would be silently wrong. Grouping is by id everywhere in AxiBridge
today (`computeSkillUsageData.ts:84`, `computePlayerAggregation.ts:1337`) —
there is no name-keyed dedup anywhere in that codebase to remove. The
defect is purely that two correctly-separate rows render identically.

## Design

One new optional field, computed in one place, populated from two sources.

### 1. The field: `SkillEntry::variant_label`

`crates/axilog-schema/src/v1/catalogs.rs`, on `SkillEntry`:

```rust
/// Set only when another id in THIS log resolves to the same `name`.
/// A consumer renders `"{name} ({variant_label})"` verbatim.
#[serde(skip_serializing_if = "Option::is_none")]
pub variant_label: Option<String>,
```

**One field, not two.** A separate `adrenaline_tier: u8` alongside a
generic ordinal would double the additive-field cost — each new schema
field is seven edit sites, and the SDK stubs, golden keyset and facade
digest are the ones that get missed. A single pre-rendered string means the
consumer never branches on which source produced it.

**A sidecar field, not a changed `name`.** Baking the tier into the
resolved name would move the conditions oracle, which resolves conditions
*by name*, and silently shift EI-side counts. It would also fight MNAME's
rung ordering. `variant_label` cannot: nothing joins on it.

The field is absent for the ~99% of skills whose name is unique in the log.

### 2. Where it is computed

`CatalogBuilder::finish` (`catalogs.rs:240`), immediately after the
existing `skills` collect.

This is the only site in the codebase that holds every skill id in the
document *and* its final resolved name — MNAME's amendment A2 made it so,
by giving `finish` the log's own name table and the collapsed
`resolve_name`. A collision pass anywhere earlier would see a partial set.

The pass: group the collected entries by `name`; for every group of size
≥ 2, set `variant_label` on each member. Single-pass, no allocation beyond
the grouping map, over a map of hundreds of entries.

### 3. What the generic label says: the id

For a collision with no curated entry, the label is the id itself:

> `Daybreaking Slash (72923)`

The obvious alternative — an ordinal, `#1` / `#2` — reads better and is
**wrong**, because it is not stable across logs. The ordinal can only be
computed from the ids present in *this* log, and AxiBridge aggregates by id
across many logs: an id ranked `#1` in a two-id log becomes `#2` the moment
a lower id appears in another log in the same session. The same skill would
carry different labels in Skill Totals and in a single fight's breakdown —
a worse bug than the one being fixed, and a subtler one.

The id is stable by construction, needs no group-ranking, and makes the
next bug report self-diagnosing: a player can quote the id instead of
screenshotting. It is uglier. Stable-and-honest wins here.

### 4. The curated burst layer

A static, sorted table in `axilog-core`, id → label, consulted before the
generic fallback:

```rust
// analysis/skill_variants.rs
pub static SKILL_VARIANT_LABELS: &[(u32, &str)] = &[ /* ... */ ];
```

Coverage, from the 67 `categories: ['Burst']` skills in `/v2/skills`,
which form exactly **12 name groups of 5 ids each**. Within a group the
roles are derivable: the slot skill has a non-null `cost` and a
`flip_skill`; the Berserker variant has `specialization: 61`; the remaining
three are the adrenaline tiers.

- **8 groups** get `Adrenaline 1|2|3`, derived from the `traited_facts`
  entry gated on `requires_trait: 1649` (Cleansing Ire) — Arcing Slice,
  Earthshaker, Eviscerate, Skull Crack, Kill Shot, Whirling Strike,
  Combustive Shot, Forceful Shot.
- **Harrier's Toss** is transcribed from the existing
  `SKILL_NAME_OVERRIDES` (`skill_name_overrides.rs:291`), where GW2EI
  already carries `72911 → Level 1`, `73042 → Level 2`, `73006 → Level 3`.
- **3 groups get no entry** and fall through to the id label — Breaching
  Strike, Path to Victory, Bloodthirster.

That last bullet is a deliberate refusal, and the reason is worth
recording: **the trait heuristic was validated against the one group with
independent ground truth, and it failed.** Harrier's Toss returns
`level = 1` for all five of its ids, contradicting EI's known-correct
1/2/3. Breaching Strike does the same; Path to Victory is internally
incoherent; Bloodthirster has no trait facts at all. A heuristic that
demonstrably lies on this exact shape of input does not get to guess on
three more groups. The derivation script is committed alongside the table
with this failure recorded in its header, so nobody re-derives the missing
three from it later.

The table is generated once and committed — not fetched at runtime. axilog
has no network access at parse time and must not acquire one.

### 5. Scope: the skill catalog only

Buffs are out. `BuffEntry` has its own name chain and its own placeholder
semantics (MNAME amendment A3: the empty string, not `Skill <id>`), and no
reported case is a buff collision. Cross-map collisions — a skill and a
buff sharing a name, as with `Signet of Restoration` 5503/739 — are
likewise out: AxiBridge's `resolveSkillMeta` prefers `skillMap` and falls
back to `buffMap`, so those two never render as sibling rows in one list.

## Edit sites

The seven-site additive-field checklist, instantiated:

1. `axilog-schema/src/v1/catalogs.rs` — the `SkillEntry` field, the literal
   in `finish`, and the collision pass.
2. `axilog-core/src/analysis/skill_variants.rs` — new module, the table.
3. Test-only `SkillEntry` literals that must still compile —
   `catalogs.rs:454`, `catalogs.rs:468`, `schema/src/lib.rs:1874`,
   `lib.rs:1901`, `ei/src/lib.rs:4003`, `ei/src/lib.rs:4014`.
4. `axilog-node/types.d.ts` and `axilog-py/axilog.pyi` — hand-written
   stubs, gated by `tests/v1_sdk_stubs.rs`. `axilog-node/index.d.ts` is
   napi-generated; do not hand-edit it.
5. `tests/v1-keyset.golden.txt` — regenerate with `UPDATE_GOLDEN=1` and
   read the diff before committing it.
6. `tests/v1_size.rs` — the budget moves by the labelled entries only.
7. `docs/NATIVE-FORMAT.md` (the `SkillEntry` table, ~`:219`) and
   `docs/CHANGELOG.md` — a missing changelog section kills the Release job
   *after* npm publish.

The legacy `SkillMapEntryOut` (`schema/src/lib.rs:169`) does **not** get
the field. It is the frozen legacy surface; the EI view is unchanged.

## Testing

- Unit, the table: sorted by id (the consumer binary-searches), no
  duplicate ids, and every entry's id is one of the 60 burst ids.
- Unit, the pass: a synthetic catalog with two ids sharing a name gets
  labels on both; a unique name gets `None`; a curated id gets the
  adrenaline label rather than the id label.
- **The regression that encodes the report:** parse the committed fixture
  and assert that `(name, variant_label)` is unique across
  `catalogs.skills` — i.e. zero pairs of rows that would render
  identically. This is the invariant, stated once.
- Pin the Earthshaker group: 5 ids, 3 carrying `Adrenaline 1|2|3`.
- Pin Harrier's Toss against the EI override table, since it is the only
  group with independent ground truth and the place the heuristic broke.
- Additivity proof, per standing rule: worktree-diff the emitted JSON
  across the change and confirm the only new keys are `variant_label`,
  **before** re-digesting the native baseline. The ei-json digest should
  not move at all; if it does, the field leaked into the EI view and the
  change is wrong.

## Consumer impact

AxiBridge needs a small render change and an axilog version bump. Both
surfaces already key by id, so nothing regroups — the label is appended at
display time where `skillNameMap` is read
(`computeSkillUsageData.ts:88`, `StatsView.tsx:3052`,
`computePlayerAggregation.ts:1337`). That lands as its own change after the
axilog release, and is not part of this spec.

Per the version-bump audit rule: diff `parseFileEi` output on the fixture
across the bump rather than trusting the changelog.

## Out of scope

- Reordering the name-resolution rungs to match EI (root cause 1). Real,
  separate, thousands of names.
- Promoting the C# symbol rung (root cause 2). Rejected outright.
- Buff-catalog and cross-map collisions (§5).
- Tiers for the three groups where the heuristic failed. They render with
  the id label, which is correct if unlovely; a curated source would close
  them later without any schema change.
- Merging same-name rows. The report asked for the opposite.
