# Native output format 1.0 — container, identity, catalogs, encoding

Status: approved design, not yet implemented.
Date: 2026-08-11.
Scope: the first of four specs in the EI-compat → native cutover program (see
"Program context" below). This spec covers ONLY the container: document shape,
versioning, the entity roster, catalogs, block layout, and series encoding.

## Why

axilog has spent its whole life so far chasing GW2 Elite Insights. `--format
ei-json` is now field-for-field calibrated (see `docs/EI-PARITY.md`), and that
work is worth keeping. But the native format has quietly become the weaker of
the two surfaces, for one structural reason: **eight analysis passes are
computed only when `format == Format::EiJson`** and are handed to the EI
adapter through a side channel that never touches the native report at all —
`boon_states`, `target_conditions`, `enemy_dist`, `healing_detail`,
`minion_rollups`, `dist_outcomes`, `health_percents`, and the per-target half
of the damage-modifier engine, plus `ei_replay`.

The consequence is that a consumer who wants everything axilog computes has to
ask for it in Elite Insights' shape, inheriting EI's warts (`dpsAll[0]`,
`statsAll[0]`, joins by array position, names as identity) and EI's omissions.
Native's `enemies[]` is a bare roster whose only two statistics — `instid` and
`damage_out` — are literally `#[serde(skip)]`, EI-adapter-only.

This program makes the native format the primary surface and the one axilog
would recommend to a new consumer, including consumers who would otherwise
reach for an EI export.

## Decisions taken before this spec (program-level)

These were settled in brainstorming and constrain every spec in the program.

1. **Cutover target: everyone.** Native must be a credible standalone
   replacement for anyone who would otherwise use an EI export — not just
   axibridge, and not just in-tree consumers.
2. **`ei-json` stays, frozen as a compat adapter.** It remains supported and
   CI-asserted indefinitely. What changes is the policy direction: native
   becomes the source of truth that EI is *projected from*, rather than EI
   being the thing native chases. New analyses land natively first and get an
   EI mapping only where EI has a shape for them. The EI goldens are also the
   program's oracle — they are how native numbers are proven correct — so
   retiring them would cost evidence, not just a format.
3. **Native is our own design, not a transliteration of EI.** Where EI's shape
   is an artifact of its history, native picks the better shape. Where axilog
   is deliberately more correct than EI (down contribution per arcdps
   methodology, the true life-leech count EI's own bug zeroes), native says so
   in its own vocabulary.
4. **Id-first rules for everything new.** Stable ids, no positional joins, no
   arrays-of-one, catalogs referenced by id rather than inlined.
5. **`ei-json` becomes a pure function of the native report** —
   `to_ei_json(&Report) -> Value`, no side inputs. Enforced mechanically:
   delete the `EiInputs` struct and the compiler finds every violation.
   Escape hatch: a block that is a *pure reprojection* of data native already
   carries may be derived inside the adapter rather than stored twice. The
   fixed-rate `ei_replay` track is the motivating case — native has its own
   richer `ReplayOut`, and EI's resampled shape exists because GW2EI's replay
   engine wants it.
   (Landed by spec #2, not this one.)
6. **Flags demote from payload gates to compute gates.** `--timeseries` comes
   to mean "spend the CPU", not "make the file legible". Anything cheap is
   default-on.

## Program context — the four specs

This spec is #1. It is deliberately scoped to the container because #2 and #3
both have to speak the vocabulary it defines; landing them first would mean
reshaping everything twice.

1. **Container (this spec)** — document shape, versioning, `entities[]`,
   catalogs, block layout, series encoding. Changes no numbers.
2. **Absorb the side channel** — the eight EI-only passes get id-first native
   shapes in the block slots this spec reserves; ends with `EiInputs` deleted
   and decision 5 enforced.
3. **Wart fixes** — replay track join keys, the `down`/`dead` gaps in
   `ei_replay`, and anything else the reshape surfaces.
4. **Consumers and docs** — HTML report, Node/Python SDKs, axibridge migration
   guide, native-format reference page on the arcdps wiki.

Each gets its own spec → plan → execution cycle.

## Non-goals for this spec

- No new numbers, no new analyses. This spec relocates and re-encodes what
  already exists. The eight side-channel passes are named and their block
  slots reserved, but filling them is spec #2.
- No sectioned/multi-file container. A manifest-plus-sections package (a
  `.axilog` archive with independently addressable sections) is a plausible
  future, and the block layout here is drawn so it stays a packaging change
  rather than a redesign — each block is independently meaningful and
  id-joined, with no cross-block positional dependencies. But building it now
  would be speculative; we have no server serving slices.
- No consumer migration. `axilog-html`, the SDKs, and axibridge are spec #4.
- No binary/columnar encoding. See "Series encoding" for why base64 typed
  arrays are rejected *for now* and how they could land additively later.

## Document shape

Six top-level keys.

```json
{
  "axilog":   { "schema": "1.0", "version": "0.3.2", "generated_from": "wvw-small.zevtc" },
  "encounter": { ... },
  "entities":  [ ... ],
  "catalogs":  { "skills": {...}, "buffs": {...}, "damage_mods": {...} },
  "blocks":    { "damage": {...}, "defenses": {...}, "boons": {...}, ... },
  "coverage":  { "damage": "present", "series": "not_computed", "replay": "not_computed" },
  "warnings":  [ ... ]
}
```

`warnings` keeps its existing omit-when-empty behaviour (see "`warnings`
becomes structured" below); the other five are always present.

### `axilog` — schema vs binary version

`schema` is the format contract. `version` is the binary that produced the
document. They move independently.

The current native `schema_version` is `"0.2"`; this spec takes it to `"1.0"`,
which is also the honest signal that this is a break rather than a bump.

Schema `1.0` while the binary is `0.3.2` is deliberate. Schema stability is a
promise to consumers — additive-only, CI-enforced — and axibridge needs that
promise now, well before axilog itself claims 1.0 maturity. Tying the two
would mean either withholding format stability until the binary is 1.0, or
shipping a "0.x, anything can change" format to a consumer we are asking to
cut over.

`generated_from` is the input log's file name (not path — paths are
environment-specific and can carry a user name, which the PII policy scrubs).

### `coverage` — why a block is absent

A per-block status map, always present, with one entry per block name the
schema defines (including blocks absent from `blocks`).

| Value | Meaning |
|---|---|
| `present` | computed, and `blocks` carries it |
| `not_computed` | the compute gate for it was off |
| `empty` | computed, and there was genuinely nothing to report |
| `unsupported` | this log's era or encounter kind cannot produce it |

Today a consumer cannot distinguish "absent because you did not pass
`--timeseries`" from "absent because the log had nothing". That ambiguity is
the worst part of consuming a gated format — it turns a missing flag into
silently-reported zeros — and it costs one small object to eliminate.
axibridge can fail loudly on `not_computed`.

`unsupported` matters more than it looks: `docs/EI-PARITY.md` records several
era-gated surfaces (pre- vs post-`ResultEnumRework`, pre- vs post-
`AnimationAsStateChanges`) where the honest answer for some logs is "this
cannot be computed", not "this is zero".

### `encounter`

Stays a plain singular object: kind, map, duration, arcdps build/revision,
recorded-by, teams, marker assignment timeline, tick-rate telemetry. It is
genuinely singular and is deliberately NOT modelled as an entity.

Marker assignments rekey from `agent_addr` to entity `id` (see below).

## `entities[]` — the single roster

Identity only. No statistics.

```json
"entities": [
  { "id": 0, "role": "squad", "account": ":Bob.1234", "character": "Bobbo",
    "profession": "Guardian", "elite_spec": "Firebrand",
    "team": "red", "team_id": 705, "subgroup": 3,
    "commander": { "variant": "blue", "guid": "..." },
    "guild_id": "ABC-...", "marker": "arrow",
    "agent_addr": 2000123456789, "instid": 1042,
    "first_aware_ms": 0, "last_aware_ms": 184320 },

  { "id": 41, "role": "enemy_player", "profession": "Necromancer",
    "elite_spec": "Reaper", "team": "green", "team_id": 2202,
    "instid": 3311, "first_aware_ms": 12040, "last_aware_ms": 180221 },

  { "id": 98, "role": "npc", "name": "Keep Lord", "team": "green",
    "instid": 9001, "first_aware_ms": 0, "last_aware_ms": 184320 }
]
```

Player entities carry `account` + `character`; non-player entities (`npc`,
`gadget`) carry `name` instead, since they have neither. Absent fields are
omitted rather than emitted as empty strings, per the omit-when-absent
convention. `profession`/`elite_spec` are present exactly for player roles,
preserving the MENEMYPROF property that presence is itself the
"is this a real player" signal.

### `role` replaces three overlapping notions

Today squad-vs-not, player-vs-not, and membership in `enemies[]` vs the
`#[serde(skip)]` `ei_targets[]` are three separate signals that partially
encode the same fact. One field replaces them:

| `role` | Meaning |
|---|---|
| `squad` | in the recording squad |
| `friendly_player` | non-squad player on the squad's team (GW2EI's `_nonSquadFriendlies`, which axilog currently discards) |
| `enemy_player` | non-squad, non-friendly player |
| `npc` | non-player agent |
| `gadget` | gadget agent |

Two stored rosters collapse into one, and both existing views become
documented filters:

- EI's curated `targets[]` roster (the MROSTER rule, `WvWLogic.cs:325-375`) is
  `role == "enemy_player"`. The whole `ei_targets` field and its long doc
  comment become a query.
- Native's combat-participant `enemies[]` is `role != "squad"` intersected
  with the nonzero-interaction criterion, which stays exactly as
  `Metrics::combat_participant_enemies` defines it today.

The `isFake` accounting stays as documented in `docs/EI-PARITY.md`: axilog
does not synthesize GW2EI's `Dummy PvP Agent` aggregate, so every emitted
entity is a real tracked agent.

`friendly_player` is a new role for data axilog currently drops. Emitting the
roster row is in scope for this spec; computing statistics for those entities
is not (they will simply have no rows in the stat blocks until a later spec
decides they should).

### Id assignment and determinism

`entities[]` is sorted deterministically — by `role` (in the table order
above), then `team_id`, then `subgroup`, then account, then character, then
`agent_addr` as the final tiebreak — and `id` is the array index, dense from
0.

The full sort key is specified rather than left to "whatever order the
encounter produced", because ids are the join key for every block and the
goldens are byte-exact diffs. A determinism test (parse twice, assert
byte-identical) keeps this honest.

Rationale for a dense small integer over the alternatives:

- `agent_addr` is 64-bit and noisy in every join key and every map key.
- `instid` is not stable across relogs, and the enemy dedupe already keys on
  it (MINSTID) for a different purpose.
- `account` is PII, which the project scrubs.

Both `agent_addr` and `instid` are carried as entity attributes, so a consumer
correlating against raw arcdps or another tool has them. Their being
`#[serde(skip)]` secrets today is part of what forced the EI side channel to
exist.

`id` is stable *within a report*, not across reports. A consumer joining
across logs joins on `account` (or `instid` within an era, with the usual
caveats). This is stated in the reference docs.

### PII gets a real boundary

Account and character names now live in exactly one block. The scrub becomes a
single pass over `entities[]` rather than chasing name strings through nested
structures — which, per the M15 fix waves, already missed a `_note` field
once. This is a correctness property, not tidiness, and the scrub test should
assert it structurally: after scrubbing, no unscrubbed name appears anywhere
in the document.

### What lives here and why

`first_aware_ms` / `last_aware_ms` are identity — when this agent existed —
and are already computed unconditionally for the EI adapter's
`combatReplayData.start`/`end`. `commander`, `guild_id`, and the current
`marker` are attributes of who someone is. The marker *timeline* stays in
`encounter.markers[]`.

## `catalogs` — names appear exactly once

```json
"catalogs": {
  "skills":      { "5491": { "name": "Symbol of Protection", "is_swap": false, "can_crit": true } },
  "buffs":       { "740":  { "name": "Might", "kind": "boon", "stacking": "intensity",
                             "max_stacks": 25, "icon": "..." } },
  "damage_mods": { "174":  { "name": "Scholar Rune", "kind": "multiplier", "approximate": true } }
}
```

**The rule: no human-readable name appears outside `catalogs` or `entities`.**
Every block references `skill_id`, `buff_id`, `mod_id` — integers.

This is the largest byte win in the spec. Today a skill name is re-inlined per
player, per target, per distribution row; the per-skill blocks are where the
measured payload growth lives (`docs/BENCHMARKS.md` and the CLI flag docs
record `--timeseries` at +147.7% and one EI block at 854,077 bytes). The cost
to consumers is one dictionary lookup.

Keys are the id as a decimal string (plain `serde_json` object-key
stringification), with no EI-style `"s"`/`"d"` prefix — matching what
`skill_map` and `damage_mod_map` already do; the EI adapter adds the prefixes
back.

### Three catalogs, not more

`buffs` absorbs boons *and* conditions — arcdps does not distinguish them
structurally, and `kind` carries the distinction. This catalog holds the
metadata EI's `buffMap` carries and native currently has nowhere to put
(stacking type, max stacks, condition-vs-boon), which is a prerequisite for
spec #2 absorbing the boon-states side channel.

Professions and elite specs stay inline strings on entities. They are a closed
set of ~40 short values with no metadata worth hoisting, and hoisting them
would make entities unreadable for no gain.

### Scoped to referenced ids, both directions

A catalog entry exists if and only if some block references it — keeping the
precedent `skill_map` and `damage_mod_map` already set (GW2EI populates its
own `damageModMap` lazily for the same reason; measured 59 referenced ids out
of 205 definitions on the committed fixture).

This gives an invariant worth asserting in CI in **both** directions: every id
referenced by any block resolves in a catalog, and every catalog entry is
referenced by something. That catches an entire class of "block emitted,
catalog forgotten" bug that gating makes easy to write and that would
otherwise surface as an `undefined` in a consumer's UI months later.

## `blocks` — uniform id-keyed maps

Every block has the same shape: an aggregate slot plus an entity-keyed map, so
a consumer learns the access pattern once.

```json
"blocks": {
  "damage": {
    "squad": { "total": 41203311, "dps": 22345.1 },
    "by_entity": {
      "0": {
        "total": 1203311, "dps": 652.4,
        "per_target": { "41": { "total": 88123, "hits": 412, "crits": 190 } },
        "by_skill":   { "5491": { "total": 44012, "hits": 88, "min": 320, "max": 1204 } }
      }
    }
  },
  "defenses": { "by_entity": { ... } },
  "boons": {
    "by_entity": {
      "0": { "740": { "uptime_pct": 91.2, "avg_stacks": 18.4,
                      "generation": { "self": 12.1, "group": 40.2, "squad": 51.0 } } }
    }
  }
}
```

The identity/statistics split pays off directly here: `per_target` keys are
entity ids, so an enemy player's own damage row and the damage dealt *to* them
are keyed by the same integer. That correlation is impossible in native today,
because enemy statistics are `#[serde(skip)]`.

It also makes gating coherent. Dropping a block leaves the roster intact and
every remaining block joinable; today dropping a block leaves holes inside
player objects.

### Block names reserved by this spec

Fixing these now is what lets spec #2 fill reserved slots rather than
renegotiate the container. Blocks marked (#2) are named and reserved here but
not populated by this spec.

| Block | Source today |
|---|---|
| `damage` | `PlayerOut.damage`, `DamageOut`, `PerTargetStatsOut`, `SkillEntryOut` |
| `defenses` | `DefensesOut` |
| `hit_stats` | `HitStatsOut` |
| `support` | `SupportOut` |
| `boons` | `BoonOut`, `GenerationOut` |
| `contribution` | `ContributionOut` (both directions) |
| `healing` | `HealingOut`; detail arrays (#2) |
| `cc` | `CcOut` |
| `rotation` | `CastOut`, `SkillRotationOut`, `AftercastOut` |
| `damage_mods` | `DamageModEntryOut`; per-target split (#2) |
| `missiles` | `MissilesOut` |
| `replay` | `ReplayOut` (join keys are spec #3) |
| `series` | `TimelineOut`, `PlayerPerSecondOut`, `PlayerTargetSeriesOut`; `health_percents` and boon-state timelines (#2) |
| `conditions` | (#2) `target_conditions` |
| `minions` | (#2) `minion_rollups` |

## Series encoding

One envelope, used by every time series in the format — per-second damage,
health percents, boon stack states, breakbar, tick rate.

```json
{ "interval_ms": 1000, "len": 1843, "enc": "rle",
  "data": [[0, 412], [1830, 3], [0, 51], [2201, 1]] }
```

`enc` is `"raw"` (plain array of values) or `"rle"` (`[value, run_length]`
pairs), chosen per series by whichever serializes smaller. `len` is the
decoded length in both cases, so a consumer can allocate before decoding and
validate after.

WvW per-second series are dominated by long zero runs — a player idle for 400
seconds encodes as `[0, 400]` rather than 400 characters of `0,`. This is what
makes decision 6 (flags as compute gates, not payload gates) affordable.

### Base64 typed arrays are rejected, for now

Denser still, and deliberately not taken. This format's job includes being
debuggable: `jq` over a native report should tell you something, and the
project's entire calibration workflow is diffing exports against EI's. Opaque
blobs destroy that to save perhaps another 30% on a block RLE has already
shrunk by an order of magnitude.

Under the 1.x additive rule a third `enc` value can land later if profiling
justifies it, and consumers that already switch on `enc` will not break. That
is the whole reason `enc` is a tagged field rather than an implicit format.

### The SDKs hide it

`enc` is a wire-format concern only. The Node and Python SDKs hydrate series
into plain arrays, so no SDK user sees an encoding tag. Decode is roughly five
lines in any language and the reference doc (spec #4) states the algorithm
explicitly for consumers not using an SDK.

## `warnings` becomes structured

Today `Report.warnings` is `Vec<String>`, which no consumer can act on
programmatically. It becomes:

```json
"warnings": [ { "code": "blank_account_agent", "severity": "info",
                "message": "...", "entity_id": 37 } ]
```

Small change, and it is the difference between axibridge surfacing a real
data-quality caveat (the known relog straggler with a blank account, for
instance) and dropping it on the floor. Codes are a closed, documented set;
adding a code is additive, changing one is a break.

## Compatibility rules for 1.x

Stated in the reference docs and enforced by test:

- **Additive-only within a major.** New blocks, new catalog entries, new
  optional fields on existing records, new `enc` values, new `coverage`
  values, new `warnings` codes — all fine.
- **Breaking:** renaming a field, removing a field, retyping a field, changing
  a field's meaning, changing the `entities[]` sort key, or changing a
  `coverage`/`warnings` value's meaning. All require a major bump.
- Absent optional fields are omitted entirely rather than serialized as
  `null`, keeping the existing convention (`TickRateOut`, `TeamOut.guid`,
  `replay`, `missiles`).

## Testing

### The safety net: `ei-json` output stays byte-identical

This spec changes no number — it relocates and re-encodes. So every existing
EI golden (`ei_golden.rs`, the `meigap*` suites, `damage_mods_ei_golden`, the
`msmall`/`mstream` suites, and the local-fixture-gated post-rework tests) is
an unchanged assertion throughout the work, and any diff means the reshape
lost or corrupted data.

That is a far stronger guarantee than a new native golden alone, and it is
free: the tests already exist. The adapter reads from the new shape; its
output does not move. Implementation should treat any EI golden diff as a
hard stop, not a golden to re-bless.

### New tests

1. **Full key-set golden** — the complete 1.0 surface on the committed
   fixture (`fixtures/wvw-small.anon.zevtc`). Removing or renaming a key fails
   CI; adding one is a reviewed diff. This is the compatibility rule made
   executable.
2. **Catalog referential integrity, both directions** — every referenced id
   resolves; every catalog entry is referenced. Run across fixtures and across
   gate combinations, since gating is exactly what makes the failure mode
   easy to introduce.
3. **Series round-trip** — property test: `decode(encode(xs)) == xs` for
   arbitrary `xs`, plus the invariant that the chosen `enc` is genuinely the
   smaller of the two serializations.
4. **Determinism** — parse the same log twice, assert byte-identical output.
   Entity ids are indices into a sorted roster; this keeps the sort honest.
5. **Size regression** — bytes per block on the committed fixture, recorded as
   numbers in `docs/BENCHMARKS.md` alongside MPERF's. Reducing bytes is part
   of this spec's point; unmeasured, it will regress.
6. **PII structural assertion** — after scrubbing, no unscrubbed account or
   character string appears anywhere in the serialized document.

### Error handling

There is no new failure mode in this spec. Parse and analysis errors are
unchanged. The two data-quality channels are `coverage` (why a block is
absent) and `warnings` (what was odd about the data), both described above.

## Risks

- **Reshape scope.** `axilog-schema` is ~1,900 lines and every downstream
  crate reads it. Mitigated by the byte-identical EI-golden safety net, which
  converts an invisible data-loss risk into a loud test failure.
- **Consumers break at once.** By construction — `axilog-html` and the SDKs
  read `Report` directly. Spec #4 handles them; until it lands, the in-tree
  consumers must be kept compiling, which will mean a mechanical adaptation
  pass inside this spec's implementation even though the polish is #4's.
- **`friendly_player` roster growth.** Emitting non-squad friendlies adds rows
  a WvW log has plenty of. They carry identity only, and the catalog/RLE wins
  should more than pay for them, but the size-regression test is what will
  actually tell us.
- **Getting block names wrong.** Spec #2 has to live with them. Mitigated by
  reserving names from the existing `*Out` types, which have already survived
  sixteen milestones of use.
