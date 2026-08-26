# MNAME — skill and buff name resolution for the healing arrays

**Status:** design, approved in chat 2026-08-26.

## The report

A WvW player reported that the Support & Healing tables in an AxiBridge web
report name skills wrongly ([Discord thread "Skill Name Bugs"][thread],
reference report `20260825-210941-afqp`). Eleven ids rendered as the literal
placeholder `Skill <id>`: 1066, 13721, 14219, 28313, 30301, 45103, 53183,
72365, 76947, 77020, 78971. Several of those rows also rendered with no
icon. Damage and offensive tables were reported correct.

[thread]: https://discord.com/channels/1466169948035481622/1542191505911971860

Two other things in the screenshots are **not** defects and are out of
scope:

- Names that read oddly for a healing context — `Journey` (71897),
  `Friendly Fire` (71892), `Nightmare Weapon` (76739), `Continuum Split`
  (29830), `Bandage` (1175 / 30142) — are the correct GW2 API names for
  those ids. They are relic and trait heal procs.
- Duplicate rows sharing a display name — two `Radiant Resolve` (78514 and
  78604), two `Signet of Restoration` (skill 5503 and buff 739) — are
  distinct ids that genuinely share a name in ArenaNet's own data. GW2EI
  disambiguates them through `OverridenSkillNames`, which this spec ports;
  whether the duplicates then merge or stay separate rows is an AxiBridge
  rendering question, not a naming one.

## Root cause

Not "the scope list forgot healing". **Healing never joined the mechanism.**

`axilog-schema/src/v1/blocks/` holds one module per native block — damage,
activity, minions, support, conditions, defense, self_effects,
squad_buffs — and each calls `cats.reference_skill(id)` or
`cats.reference_buff(id)` as it emits a row. `CatalogBuilder::finish` then
materialises exactly those ids into `catalogs.skills` / `catalogs.buffs`,
which the EI adapter serialises as `skillMap` / `buffMap`. The invariant
`catalogs.rs` states for itself is "every id any row references resolves to
an entry".

There is no healing block. `healing_detail::build`'s output reaches the wire
only through `axilog-ei`'s `heal_dist_json` (`axilog-ei/src/lib.rs:1970`),
which runs after `CatalogBuilder::finish` and registers nothing. The heal
and barrier dist arrays are the one id-bearing family in the document that
never enters the catalog discipline.

Two consequences, and they need different fixes:

1. **`analysis::skill_map`'s scope excludes heal ids.**
   `referenced_skill_ids` (`skill_map.rs:515`) unions each player's
   `skill_damage.outgoing` / `.taken` / `.per_target` and `rotation`, plus
   the 12 `BOON_IDS`. A skill that only ever healed appears in none of
   those, so it gets no `SkillMapEntry` — and therefore no log-table name
   and no icon, even when arcdps wrote its name into the log.
2. **Indirect healing ids resolve in neither map.** GW2EI's
   `BuildHealingDist` routes an `IndirectHealing` row's id into `buffMap`
   rather than `skillMap`. axilog's `buffMap` carries the 12 boons plus the
   14 conditions plus Stun/Daze, and a healing-over-time id is none of
   those. `axilog-ei/src/lib.rs:1956` already documents this as a known
   divergence.

A third cause is independent of healing and explains the most visible row:

3. **No name source for ids ArenaNet's API does not list.** `resolve_name`
   consults the log's own skill table, then `pseudo_name` (negative ids
   only), then the generated `skill_icons::name` catalog, then the
   placeholder. For skill 1066 arcdps writes the literal string `"1066"`,
   which `resolve_name` correctly rejects as a numeric placeholder, and
   `/v2/skills` has no record of the id. GW2EI names it `Resurrect` from
   `SkillItemOverrides.OverridenSkillNames`, of which axilog currently ports
   only the ~25 negative pseudo ids.

### Evidence

Decoding the `cbtskill` table straight out of two committed fixtures:

| id | log's own skill-table name | fixed by scope widening alone |
|---|---|---|
| 13721 | `Restorative Mantras` | yes |
| 30301 | `Leeching Bolts` | yes |
| 53183 | `Illusionary Inspiration` | yes |
| 77020 | `Restorative Glow` | yes |
| 1066 | `"1066"` (arcdps numeric placeholder) | **no** — needs the override table |
| 14219, 28313, 45103, 72365, 76947, 78971 | absent from these two fixtures | expected yes in the reporter's log |

The bottom row is an artifact of the probe, not a finding: the skill table
is per-log and carries only ids that occurred. Those skills fired in the
reporter's log, so arcdps will have written rows for them.

Cross-checked against a real dps.report export
(`fixtures/local/wvw-postrework.ei.json`): 1066 and 53183 are in its
`skillMap` (`Resurrect`, `Illusionary Inspiration`); 13721 and 77020 are in
its `buffMap` (`Restorative Mantras`, `Restorative Glow`) — which is the
direct/indirect routing of cause 2, observed rather than inferred.

All eleven ids are absent from both generated catalogs: `SKILL_NAMES`
(4610 entries, from `/v2/skills`) and `buff_icons` (2267, from GW2EI's buff
table).

## Design

Four changes. Each has one job; none subsumes another.

### 1. Widen `analysis::skill_map`'s referenced-id scope

`referenced_skill_ids` gains every `skill_id` in each player's
`healing_dist` and `barrier_dist`. `build` already takes `raw`, so those ids
resolve through the existing `resolve_name` chain and pick up the log's own
name. Icons follow for free: `CatalogBuilder::finish` calls `resolve_icon`
for every catalog id, and the id now has an entry to attach one to.

`healing_detail` is not on `PlayerMetrics` — it is built separately and
handed to the schema builder as `BuildInputs::healing_detail`
(`v1/mod.rs:349`). `skill_map::build` is called from `analyze()`, which does
not have it. So the widening happens by passing the already-built
`HealingDetail` into `build` as a fourth argument, `Option<&HealingDetail>`,
threaded from the same call site that already computes it. `None` on a log
with no healing extension, which is the existing gate.

### 2. Register heal ids on the catalog, direct and indirect apart

Where the v1 builder drives `CatalogBuilder`, walk
`BuildInputs::healing_detail`'s `healing_dist` and `barrier_dist` and
register each row:

- `entry.indirect == false` → `cats.reference_skill(entry.skill_id)`
- `entry.indirect == true` → `cats.reference_buff(entry.skill_id)`

This mirrors `BuildHealingDist` exactly, and it is what puts an HoT id into
`buffMap` where a real EI export has it. `HealDistEntry::indirect` already
carries the flag.

`BuffEntry` requires `kind` and `stacking`. A healing-over-time id is
neither boon nor condition, so it takes `kind: "effect"` — the bucket
`catalogs.rs` already documents for "auras, forms, and other non-boon
non-condition buffs" — and whatever `buffs::stacking(id)` returns, which for
an untracked id is duration with no capacity.

**`BuffEntry::name` depends on change 1, and that dependency is the whole
reason both changes are needed.** `finish` has no access to the log's own
skill table, so resolving 13721 there in isolation would consult the API
catalog and `buff_icons`, miss in both, and emit `Skill 13721` into
`buffMap` — the same placeholder, relocated. What makes it resolve is that
change 1 has already put 13721 into `Metrics::skill_map` with its log-table
name, and `finish` already takes `&Metrics`. So `BuffEntry::name` reads
`metrics.skill_map` first, exactly as `SkillEntry::name` does, before
falling back through the shared chain of change 3. Change 2 without change 1
fixes nothing.

**Indirect ids will therefore appear in both maps.** Change 1 widens the
skill map over the whole of `healing_dist`, indirect rows included, because
that is where the log-table name comes from; change 2 additionally registers
those ids as buffs. Real EI puts an indirect id in `buffMap` only. This is a
deliberate superset, on the same reasoning `catalogs.rs` already records for
Stun and Daze joining `buffMap` without appearing in any emitted uptime
array: a consumer only ever looks up ids it already holds, so the extra
entry is inert. AxiBridge's `resolveSkillMeta` prefers `skillMap` and falls
back to `buffMap`, so it reads the log-table name either way. Do not narrow
it.

### 3. Port `OverridenSkillNames`, and collapse the two resolvers into one

`scripts/gen_skill_icon_override_catalog.py` already resolves `SkillIDs`
symbols to ids out of
`GW2EIEvtcParser/ParsedData/Skills/SkillItemOverrides.cs` for the
`OverridenSkillIcons` half of that file. `OverridenSkillNames` is the
sibling dictionary in the same file with the same shape — `{ Resurrect,
"Resurrect" }` — differing only in that the value is a string literal rather
than an icon symbol. A sibling generator, `gen_skill_name_override_catalog.py`,
emits `analysis::skill_name_overrides`, with the same accounting discipline
(`considered == transcribed + repeats + skipped`, skips carrying a reason).

There are today **two** name-resolution chains that must agree and already
do not:

| | `skill_map::resolve_name` | `CatalogBuilder::finish` |
|---|---|---|
| log's own skill table | yes | no (has no access) |
| `pseudo_name` | yes | **no** |
| `skill_icons::name` | yes | yes |
| placeholder | yes | yes |

`finish` reaches its chain only for a referenced id the skill map never
covered — which, after change 1, is a narrower set, but not an empty one.
Both collapse into a single `skill_map::resolve_name(id, log_name:
Option<&str>)`, with `finish` passing `None`. One function, one order, no
drift. The buff half gets the same treatment: `BuffEntry::name` falls back
through the shared chain instead of resolving in isolation.

**Precedence of the override table is measured, not assumed.** Placing it
before `skill_icons::name` matches `resolve_icon`'s established order and
GW2EI's own `SkillItem.cs`. But unlike the icon case it can *rename* ids
that already resolve, because the overrides carry EI's disambiguations
(`Flame Blast (Superior Sigil of Fire)` where the log says `Flame Blast`).
So: implement it override-first, count the resulting name changes on
`fixtures/local/wvw-postrework.zevtc`, and record the count in the module
doc. If it moves more than a handful of ids, rank the table **below**
`skill_icons::name` instead, where it can only ever displace `Skill <id>` —
the same justification `skill_map.rs`'s doc comment already gives for
ranking the API catalog third. Either outcome is acceptable; the count
decides, and the decision is written down.

### 4. The leak test

A golden over the emitted EI JSON that walks every id-bearing array —
`totalDamageDist`, `totalHealingDist`, `totalBarrierDist`,
`targetDamageDist`, `rotation`, `buffUptimes`, `targets[].buffs`,
`damageModifiers` — and asserts each id resolves in `skillMap` or `buffMap`
to a name that is not `Skill <id>`.

This is the part that makes the fix systemic rather than one more patch. The
three changes above close the healing gap; the test is what stops the next
array from opening a new one silently. Ids no source can name get an
explicit allowlist in the test, each with a comment saying why — so the
allowlist growing is a visible diff, which a placeholder appearing in a
Discord screenshot is not.

Run it under both gate configurations the adapter supports, since arrays
appear and disappear with the flags.

## Testing

- Unit: `referenced_skill_ids` includes heal and barrier dist ids; direct
  and indirect rows route to `reference_skill` / `reference_buff`
  respectively; the shared resolver returns the same answer for an id
  reachable through both call paths.
- Generator: the accounting identity holds, and the emitted table is sorted
  by id so the binary search in the consumer is valid.
- Golden: the leak test above. Plus the existing `skill_map_golden.rs` and
  `ei_golden.rs` re-run — the skill map and buff map both gain rows, so
  their counts move and the goldens need refreshing with the deltas
  explained in the commit.
- Regression against the report: parse the reporter's log, or the nearest
  committed fixture with the healing extension, and assert 1066 resolves to
  `Resurrect` and 13721 to `Restorative Mantras` in `buffMap`.

## Consumer impact

AxiBridge needs no code change. `computeHealEffectivenessData.ts`'s
`resolveSkillMeta` already consults `skillMap` first and falls back to
`buffMap` (`:52`), which is exactly the routing this spec makes true on the
producer side. The fix ships as an axilog version bump.

Per the standing rule from the version-bump audit: diff `parseFileEi` output
on the fixture across the bump rather than trusting the changelog, because
the shapes of `skillMap` and `buffMap` both change here.

## Size

`skillMap` gains the heal-only ids; `buffMap` gains the indirect heal ids.
Both are small maps of short rows, on the order of tens to low hundreds of
entries on a WvW log, against a `report.json` whose bulk is `replayFights`.
Measure and record it, but no gate is anticipated.

## Out of scope

- A native `blocks.healing` block. Architecturally where this ends up —
  heal dist as a first-class native block that registers its ids like every
  other — but it carries its own schema surface, goldens and size budget,
  and the naming bug does not need it.
- Adopting `OverridenSkillNames` as the *primary* name source over the log's
  own table. That is the database-backed naming gap `skill_map.rs`'s doc
  comment declares out of scope, and it would move thousands of names for a
  readability gain.
- `conversionBasedHealing` / `hybridHealing`, which genuinely need the
  external database.
- Whether AxiBridge merges two rows that share a display name.

---

## Amendments — 2026-08-26, discovered during planning

Four factual corrections found while reading the code to write the
implementation plan. The design's intent survives all four; two of its
stated mechanics do not. Recorded here rather than edited into the body
above, so what was approved stays legible next to what changed.

### A1. There IS a healing block, and it already registers its ids

`blocks.healing` exists — `v1/blocks/support.rs::build_healing`, which takes
`&mut CatalogBuilder` and calls `cats.reference_skill(e.skill_id)` for
**every** heal-dist and barrier-dist row (`support.rs:567`), direct and
indirect alike, with the comment "Every id this block joins on has to
resolve in the catalog, or the row is a dangling reference".

So the Root cause section's "There is no healing block ... registers
nothing" is wrong, and change 2's direct half is already shipped. This
correction *strengthens* the diagnosis rather than weakening it: the heal
ids are already in `catalogs.skills`, which is exactly why the reporter sees
the string `Skill 13721` in a rendered row rather than a row that is simply
missing. The remaining defect is purely one of **name resolution**, not of
registration.

What is left of change 2: routing indirect rows to `reference_buff` as well,
and giving `BuffEntry::name` a real chain.

### A2. Change 1 cannot be threaded as written, and does not need to be

`skill_map::build` is called from inside `analyze()`
(`analysis/mod.rs:808`). `healing_detail::build` is called by each consumer
*after* `analyze` returns, gated on `--skill-damage`/`--timeseries`
(`axilog-api/src/lib.rs:128`, `axilog-cli/src/main.rs:423`,
`axilog-node/src/lib.rs:237`, `axilog-py/src/lib.rs:133` and `:252`). **No
call site holds both.** The spec's "threaded from the same call site that
already computes it" describes a call site that does not exist.

Threading it anyway would mean editing six consumers — the whack-a-mole
shape this spec exists to avoid — and would make `Metrics::skill_map`
gate-dependent, so the same log would name skills differently depending on
which passes were requested.

The replacement is smaller and strictly more systemic. `CatalogBuilder::
finish` already unions `metrics.skill_map.keys()` into the skill catalog
(`catalogs.rs:231`) and already falls back for ids the map never covered;
its only real handicap is the one the comparison table in change 3 names —
**it has no access to the log's own skill table.** So give it one:

> `Metrics` gains `log_skill_names: BTreeMap<u32, String>`, populated in
> `analyze` directly from `raw.skills` — ungated, cheap (hundreds of
> entries), and a property of the log rather than of any flag. The collapsed
> `skill_map::resolve_name(id, log_name)` of change 3 is then called by
> `finish` with `metrics.log_skill_names.get(&id)`.

This closes the gap for **every** block that references an id outside
`skill_map`'s scope — present and future — instead of for healing alone.
That is the anti-whack-a-mole property the report asked for, and the change-4
leak test is what holds it.

Change 1's remaining value was that a heal-only id would get real
`can_crit`/`is_swap`/proc flags rather than `finish`'s defaults — notably
`can_crit: true`, which is wrong for a heal skill. That is better served
directly: `is_swap` and `hit_stats::can_crit` are pure functions of the id,
so `finish` computes them for uncovered ids instead of defaulting. The proc
and instant flags stay `false`, which is already what they mean there — no
finder claimed the id.

**Change 1 as specified is therefore dropped**, and its two effects are
absorbed into the amended changes 3 and 2 respectively. `analyze`'s
signature does not change and no consumer is touched.

### A3. The buff-side placeholder is the empty string, not `Skill <id>`

`BuffEntry::name` is `buffs::name(id).unwrap_or_default()`
(`catalogs.rs:306`), so an id outside the boon, condition and control tables
resolves to `""`, not to `Skill <id>`. The prose under change 2 predicted
the wrong placeholder.

The design is unaffected — a nameless row is no better than a placeholder
one — but the change-4 leak test must treat an empty or whitespace name as a
failure alongside the `Skill <id>` pattern, or the buff half of the
invariant goes unenforced.

### A4. `hit_stats::can_crit` is `pub(crate)`

It has to become `pub` for `catalogs.rs` (a different crate) to call it
under A2. A visibility widening on a pure id predicate, noted only so it is
not mistaken for scope creep in review.
