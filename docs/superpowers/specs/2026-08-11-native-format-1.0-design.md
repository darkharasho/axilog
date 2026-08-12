# Native output format 1.0 — container, identity, catalogs, encoding

Status: implemented (`--format json` emits this shape as of Task 12 of the
implementation plan). This document has been reconciled against what
shipped (Task 14); divergences found during implementation are called out
inline as "Deviation" notes rather than silently rewritten, so the design
history stays legible.
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
3. **EI parity is a floor, not a ceiling.** If GW2EI computes something, so
   should axilog — a capability EI has and axilog lacks is a gap to close,
   not a scope decision. Where axilog is more correct than EI it stays more
   correct, but it never offers less. This is why unreachable-today roles and
   fields that mirror an EI capability are kept and made semantically correct
   now, rather than deleted as speculative.
4. **Native is our own design, not a transliteration of EI.** Where EI's shape
   is an artifact of its history, native picks the better shape. Where axilog
   is deliberately more correct than EI (down contribution per arcdps
   methodology, the true life-leech count EI's own bug zeroes), native says so
   in its own vocabulary.
5. **Id-first rules for everything new.** Stable ids, no positional joins, no
   arrays-of-one, catalogs referenced by id rather than inlined.
6. **`ei-json` becomes a pure function of the native report** —
   `to_ei_json(&Report) -> Value`, no side inputs. Enforced mechanically:
   delete the `EiInputs` struct and the compiler finds every violation.
   Escape hatch: a block that is a *pure reprojection* of data native already
   carries may be derived inside the adapter rather than stored twice. The
   fixed-rate `ei_replay` track is the motivating case — native has its own
   richer `ReplayOut`, and EI's resampled shape exists because GW2EI's replay
   engine wants it.
   (Landed by spec #2, not this one. Spec #1's implementation plan deferred
   the adapter re-point specifically — see "Program context" and Section 5
   below — because re-pointing `crates/axilog-ei/src/lib.rs` is the single
   riskiest change in the program and spec #2 already has to touch the
   adapter to delete `EiInputs`. Spec #1 instead proves the reshape lost
   nothing via `crates/axilog-schema/tests/v1_equivalence.rs`, which asserts
   the 1.0 blocks agree field-for-field with the legacy `Report` on the
   committed fixture.)
7. **Flags demote from payload gates to compute gates.** `--timeseries` comes
   to mean "spend the CPU", not "make the file legible". Anything cheap is
   default-on.

## Program context — the four specs

This spec is #1. It is deliberately scoped to the container because #2 and #3
both have to speak the vocabulary it defines; landing them first would mean
reshaping everything twice.

1. **Container (this spec)** — document shape, versioning, `entities[]`,
   catalogs, block layout, series encoding. Changes no numbers. The `ei-json`
   adapter re-point promised by decision 6 above is explicitly OUT of this
   spec's implementation — its plan defers that move to spec #2 (see Section
   5) and proves the reshape instead with an equivalence test against the
   still-untouched legacy adapter path.
2. **Absorb the side channel** — the eight EI-only passes get id-first native
   shapes in the block slots this spec reserves; ends with `EiInputs` deleted
   and decision 6 enforced, including the `ei-json` adapter re-point deferred
   from spec #1.
3. **Wart fixes** — replay track join keys, the `down`/`dead` gaps in
   `ei_replay`, and anything else the reshape surfaces.
4. **Consumers and docs** — HTML report, Node/Python SDKs (including SDK
   series hydration — decoding `enc`-tagged series into plain arrays so no
   SDK user sees the wire encoding, previously described in "Series encoding"
   below), axibridge migration guide, native-format reference page on the
   arcdps wiki.

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

`recorded_by` is `Option<u32>` — an entity id, resolved by matching the
legacy `Encounter::recorded_by` account string against the roster and
joining through that player's `agent_addr` — NOT the raw account string the
legacy `EncounterOut.recorded_by` still carries. This keeps the "account and
character names live in exactly one block" rule from "PII gets a real
boundary" below true for `encounter` too: a consumer who needs the
recorder's identity looks it up in `entities[]` by id, the same way every
other cross-reference in the document works. Absent (field omitted, not
`null`) when the recorder's account does not resolve to a tracked entity.

Marker assignments carry BOTH `agent_addr` (always present) and `entity_id`
(present only when the agent resolves to a tracked entity) rather than a
pure rekey from one to the other. `arcdps` does not restrict `CBTS_MARKER`
to squad members, and this project's WvW roster builder drops friendly-side
NPCs/gadgets (siege, pets, own-team guards) that never took a hostile hit
before they ever become tracked entities — so a squad marker placed on
friendly siege is an ordinary pattern with no resolvable entity id. A pure
rekey would silently discard that marker; carrying `agent_addr`
unconditionally keeps it, at the cost of one extra field per marker (see
`entities[]` below).

## `entities[]` — the single roster

Identity only. No statistics. Real, trimmed rows from
`fixtures/wvw-small.anon.zevtc` (`--format json`; see `docs/NATIVE-FORMAT.md`
for the full worked example):

```json
"entities": [
  { "id": 0, "role": "squad", "account": ":Anon145.6365", "character": "Anon145",
    "profession": "Elementalist", "elite_spec": "Evoker", "team": "green",
    "subgroup": 0, "agent_addr": 9512, "instid": 5489, "combat_participant": true },

  { "id": 7, "role": "squad", "account": ":Anon106.4922", "character": "Anon106",
    "profession": "Guardian", "elite_spec": "Dragonhunter", "team": "green",
    "subgroup": 2,
    "commander": { "variant": "purple-commander", "guid": "1993fadb6fb70e4383a223a54d311f7d" },
    "guild_id": "00000000-0000-0000-0000-000000000000",
    "agent_addr": 4566, "instid": 3684, "combat_participant": true },

  { "id": 42, "role": "enemy_player", "profession": "Guardian",
    "elite_spec": "Dragonhunter", "team": "blue",
    "agent_addr": 9594, "instid": 5089, "combat_participant": true },

  { "id": 74, "role": "npc", "name": "Blood Fiend", "team": "blue",
    "agent_addr": 9718, "instid": 3287, "combat_participant": true }
]
```

Player entities carry `account` + `character`; non-player entities (`npc`)
carry `name` instead, since they have neither. Absent fields are omitted
rather than emitted as empty strings, per the omit-when-absent convention.
`profession`/`elite_spec` are present exactly for player roles, preserving
the MENEMYPROF property that presence is itself the "is this a real player"
signal.

**Deviation from the original draft, found during implementation:** there is
no `team_id` field (only the `team` string) and no `first_aware_ms`/
`last_aware_ms` on `EntityOut` — see "What lives here and why" below for
why those two were dropped rather than added. `combat_participant: bool`
was added instead (not in the original draft) — see "`role` replaces three
overlapping notions" below.

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

**Deviation, decided before Task 1 (recorded in the plan's pre-flight
rulings):** the fifth role sketched here, `gadget`, was dropped as
unreachable. `axilog_core::model::agent_kind` can distinguish gadgets from
NPCs, but `model::Enemy` does not retain that distinction once an agent
reaches this layer, so a `Role::Gadget` arm could never actually be
constructed — every non-player enemy agent is `role: "npc"`. Adding a real
`Gadget` role later, if `model::Enemy` grows the distinction, is additive
under the 1.x rules.

Two stored rosters collapse into one, and both existing views become
documented filters:

- EI's curated `targets[]` roster (the MROSTER rule, `WvWLogic.cs:325-375`) is
  `role == "enemy_player"`. The whole `ei_targets` field and its long doc
  comment become a query.
- Native's combat-participant `enemies[]` is `role != "squad"` intersected
  with the nonzero-interaction criterion, which stays exactly as
  `Metrics::combat_participant_enemies` defines it today. This criterion now
  has its own field, `EntityOut::combat_participant: bool` (not in the
  original draft), populated for every non-squad entity so the legacy
  combat-participant `enemies[]` view stays expressible as a plain filter
  (`role != "squad" && combat_participant`) rather than requiring a consumer
  to re-derive the predicate.

The `isFake` accounting stays as documented in `docs/EI-PARITY.md`: axilog
does not synthesize GW2EI's `Dummy PvP Agent` aggregate, so every emitted
entity is a real tracked agent.

`friendly_player` is a role for data axilog currently drops upstream:
`Player::in_squad` is hardcoded `true` in `model::resolve`, so no non-squad
friendly is ever produced and the role is unreachable **today**.

It is kept rather than deleted because GW2EI computes this split
(`_nonSquadFriendlies`), and by decision 3 a capability EI has is a gap to
close. Populating it is REQUIRED follow-up work, not optional — tracked for a
later spec, since it is new analysis in `axilog-core` rather than a container
change.

What this spec does do is make every aggregate semantically correct in
advance: `squad` aggregates filter on `Role::Squad` rather than summing the
whole roster, so filling the upstream gap is pure addition instead of a
silent meaning change across every block.

### Id assignment and determinism

`entities[]` is sorted deterministically — by `role` (in the table order
above), then `team` (the string, e.g. `"red"`/`"green"`/`"blue"` — there is
no separate `team_id` field on `entities[]`; the numeric WvW team id lives
only on `encounter.teams[]`), then `subgroup`, then account, then character,
then `agent_addr` as the final tiebreak — and `id` is the array index, dense
from 0.

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

`commander`, `guild_id`, and the current `marker` are attributes of who
someone is. The marker *timeline* stays in `encounter.markers[]`.

**Deviation from the original draft:** `first_aware_ms` / `last_aware_ms`
were originally sketched as identity fields here — when this agent existed
— on the reasoning that they are already computed unconditionally for the
EI adapter's `combatReplayData.start`/`end`. They did not land on
`EntityOut` during implementation. The values remain available, but only
per replay track and only when `--replay` is on: each entity's track in
`blocks.replay.by_entity[id].samples` starts at that agent's own
first-aware time rounded up to the polling grid (see `blocks.replay` and
`ReplayTrack`'s doc comment), and `down_intervals`/`dead_intervals` on the
same track carry the down/dead windows. A consumer who needs first/last
aware unconditionally (without paying for `--replay`) cannot get it from
the 1.0 document today; this is a real gap relative to the original design,
not a documentation-only correction, and is left for a later spec rather
than papered over here.

## `catalogs` — names appear exactly once

```json
"catalogs": {
  "skills":      { "5491": { "name": "Symbol of Protection", "is_swap": false, "can_crit": true } },
  "buffs":       { "740":  { "name": "Might", "kind": "boon", "stacking": "intensity",
                             "max_stacks": 25, "icon": "..." } },
  "damage_mods": { "174":  { "name": "Empowered", "description": "1% per boon<br>...",
                             "non_multiplier": false, "is_counter": false,
                             "skill_based": false, "approximate": false } }
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

`buffs` absorbs boons, conditions, *and* effects — arcdps does not
distinguish them structurally, and `kind` carries the distinction using GW2's
own three-way taxonomy. This catalog holds the metadata EI's `buffMap`
carries and native currently has nowhere to put (stacking type, max stacks,
kind), which is a prerequisite for spec #2 absorbing the boon-states side
channel.

`kind` is decided by catalog membership, not by damage. Eight of GW2's
fourteen conditions — Blind, Crippled, Chilled, Immobile, Weakness, Fear,
Slow, Taunt — deal no damage and are still conditions, so a
"does it deal condition damage" test mislabels them. Auras and forms (Frost
Aura, Death Shroud) are `"effect"`; calling them boons would be false.

Stacking metadata comes from `condition_catalog::CONDITION_BUFFS` for
conditions. The damage-modifier catalog's `buff_stack` table is a 91-entry
subset scoped to that catalog's needs and contains exactly one condition, so
reading stacking from it silently reports five of the most common conditions
in the game as duration-stacking with no stack cap.

**Deviation, decided during Task 4's implementation review:** this section originally sketched `kind` as a binary condition-vs-boon field decided by whether the buff deals condition damage. Both halves of that were wrong, as the three paragraphs above now record.

**Deviation, decided after Task 4 landed:** `DamageModEntry` originally
folded `is_counter`/`skill_based`/`non_multiplier` into a single `kind`
string (`"counter"`/`"skill"`/`"flat"`/`"multiplier"`, plus `"unknown"` for
unresolved ids) via a priority chain. That is lossy — a modifier that is
BOTH skill-based and a multiplier collapses to `"skill"`, silently erasing
the multiplier axis — and folds two orthogonal properties into one label
for no reason `buffs.kind`'s three-way taxonomy doesn't already have (that
one IS a real single classification; this wasn't). `kind` was replaced with
the four independent booleans (`non_multiplier`, `is_counter`,
`skill_based`, `approximate`) plus `description`, mirroring GW2EI's own
`damageModMap` entry fields and `axilog_core::analysis::damage_mods::
DamageModifierMeta`, which already stored them this way. For an id with no
resolved definition, all five fields are omitted together (`Option`,
`skip_serializing_if`) rather than defaulted to `false`/`""` — absence is
the honest "no metadata" signal, not an assertion that every property is
false.

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
        "total": 1203311, "dps": 652.4, "taken": 88123,
        "per_target": { "41": { "total": 88123 } },
        "by_skill":   { "5491": { "total": 44012, "hits": 88, "min": 320, "max": 1204,
                                   "crit_hits": 31, "flank_hits": 9 } }
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

(Illustrative shape, hand-composed for readability here — see
`docs/NATIVE-FORMAT.md` for a real trimmed document. Corrections from the
earliest draft of this example: `by_skill` rows (`SkillRow`) carry
`crit_hits`/`flank_hits` hit *counts*, mirroring the legacy `SkillEntryOut`
field for field, which this sketch first omitted entirely.

A second "correction" recorded here during Task 5/8 was itself **wrong**
and is retracted by the final whole-branch review: it claimed `per_target`
rows carry only `total`, on the reasoning that "a target's hit/crit counts
live on that target's own `by_skill` rows the same way any entity's do".
They do not. `by_skill` is the *attacker's* per-skill totals across all
targets, so it cannot answer a per-`(attacker, target)` question — and
`PerTargetStatsOut`'s `interrupts` and `downs_contribution_damage` are not
reconstructible from any other block at all. Acting on that reasoning
dropped the whole struct. `per_target` rows now carry `total` plus an
optional `detail` (the full `PerTargetStatsOut`) and an optional
`by_skill` (the per-`(attacker, target, skill)` split); see
`docs/NATIVE-FORMAT.md`.)

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

### `enc` is a wire-format concern only

Decode is roughly five lines in any language, and the reference doc (spec
#4, `docs/NATIVE-FORMAT.md`) states the algorithm explicitly for consumers
not using an SDK. Whether the Node and Python SDKs hydrate series into plain
arrays so no SDK user ever sees the encoding tag is consumer-facing polish,
not a container concern — that promise belongs to spec #4's "Consumers and
docs" scope, not this one, which changes no consumer surface. This spec's
job stops at defining `enc` as a tagged field precisely so a future SDK (or
consumer) can make that choice without a format break.

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
free: the tests already exist. **Amendment, decided during implementation:**
the adapter does NOT re-point to the new shape in this spec, contrary to
decision 5 above. Re-pointing `crates/axilog-ei/src/lib.rs` (3,331 lines) was
judged the single riskiest change in the program, and spec #2 already has to
touch the adapter to delete `EiInputs`, so the re-point lands there in one
motion instead. The adapter keeps reading the legacy `Report` completely
unchanged, so `ei-json` output is byte-identical by construction rather than
by assertion — a strictly stronger guarantee than "the adapter reads from
the new shape and we assert its output does not move" would have been. The
reshape itself (that the 1.0 blocks lose or corrupt nothing relative to the
legacy `Report`) is instead proven by
`crates/axilog-schema/tests/v1_equivalence.rs`, which asserts the 1.0 blocks
agree field-for-field with the legacy `Report` on the committed fixture.
Implementation should treat any EI golden diff as a hard stop, not a golden
to re-bless.

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
