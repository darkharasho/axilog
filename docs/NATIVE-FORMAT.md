# Native format 1.0 reference

The consumer-facing reference for `axilog parse --format json` (the default
format), schema `"1.0"`. If you have never seen this format before, read
this document top to bottom once; everything after it is a lookup.

Design rationale and history live in
[`docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`](superpowers/specs/2026-08-11-native-format-1.0-design.md).
This page documents what shipped, not the original design intent where the
two differ — every example below was generated from the real binary, never
hand-written:

```sh
cargo run --release -p axilog-cli -- parse fixtures/wvw-small.anon.zevtc \
  --format json --output /tmp/v1.json
```

## The six top-level keys

```json
{
  "axilog":    { "schema": "1.0", "version": "0.3.2", "generated_from": "wvw-small.anon.zevtc" },
  "encounter": { "kind": "wvw", "map": "Green Alpine Borderlands", "...": "..." },
  "entities":  [ { "id": 0, "role": "squad", "...": "..." }, "..." ],
  "catalogs":  { "skills": { "...": "..." }, "buffs": { "...": "..." }, "damage_mods": { "...": "..." } },
  "blocks":    { "damage": { "...": "..." }, "defenses": { "...": "..." }, "...": "..." },
  "coverage":  { "damage": "present", "series": "present", "replay": "not_computed", "...": "..." }
}
```

`warnings` is a seventh key, present only when non-empty (omitted here — the
fixture has none). The other six are always present, even when a block
inside `blocks` is empty or absent.

| Key | What it is |
|---|---|
| `axilog` | Format/binary versioning and the input file name (see below) |
| `encounter` | One singular object: fight kind, map, duration, teams, marker timeline, tick rate |
| `entities` | The single roster — every tracked agent, identity only, no statistics |
| `catalogs` | Skill/buff/damage-modifier metadata, referenced by id from `blocks` |
| `blocks` | Uniform id-keyed statistics maps — damage, defenses, boons, etc. |
| `coverage` | Per-block status: why a block is present, empty, or absent |
| `warnings` (optional) | Structured, machine-actionable data-quality notes |

### `axilog` — real example

```json
{ "schema": "1.0", "version": "0.3.2", "generated_from": "wvw-small.anon.zevtc" }
```

- `schema` is the format contract version (this document describes `"1.0"`).
  It moves independently of `version`, which is the binary that produced the
  document (`CARGO_PKG_VERSION`). Do not infer one from the other.
- `generated_from` is the input log's file **name**, never a path — paths
  are environment-specific and often carry a username, which the project's
  PII policy scrubs.

### `encounter` — real example (trimmed)

```json
{
  "kind": "wvw",
  "map": "Green Alpine Borderlands",
  "duration_ms": 49285,
  "build": "20260114",
  "revision": 1,
  "recorded_by": 22,
  "teams": [
    { "color": "unknown", "team_id": 0 },
    { "color": "blue", "team_id": 433 },
    { "color": "green", "team_id": 2767 }
  ],
  "markers": [
    { "entity_id": 58, "agent_addr": 9619, "marker": "3cd1c64a...", "time_ms": 33847418 }
  ],
  "started_at_unix": 1768702180
}
```

`recorded_by` is an **entity id** (`22` here — an integer, joins into
`entities[]`), not the raw account string. `markers[]` entries carry both
`agent_addr` (always present) and `entity_id` (present only when the
marker's agent resolved to a tracked entity — `arcdps` does not restrict
`CBTS_MARKER` to squad members, so a marker on untracked friendly siege is
ordinary and still needs to survive with `agent_addr` alone).

`started_at_unix` is the wall-clock log start, seconds since the epoch, read
from arcdps's own `CBTS_LOGSTART`/`CBTS_SQCOMBATSTART` event -- the SERVER
timestamp specifically, not the recording client's local clock (arcdps
records both; a machine's own clock skew is not a fact about the log).
Omitted (not `0`) when the log carries no such event, so absence stays
distinguishable from epoch zero. This replaces inferring the start time from
the `.zevtc` file's mtime, which breaks for any copied or restored file.

## `entities[]` and the `role` field

Real rows from the fixture, trimmed to four representative roles:

```json
[
  { "id": 0, "role": "squad", "account": ":Anon145.6365", "character": "Anon145",
    "profession": "Elementalist", "elite_spec": "Evoker", "team": "green",
    "subgroup": 0, "agent_addr": 9512, "instid": 5489, "combat_participant": true },

  { "id": 7, "role": "squad", "account": ":Anon106.4922", "character": "Anon106",
    "profession": "Guardian", "elite_spec": "Dragonhunter", "team": "green",
    "subgroup": 2,
    "commander": { "variant": "purple-commander", "guid": "1993fadb6fb70e4383a223a54d311f7d",
      "segments": [[33847418, 33847418], [33847418, 33896600]] },
    "guild_id": "00000000-0000-0000-0000-000000000000",
    "agent_addr": 4566, "instid": 3684, "combat_participant": true },

  { "id": 42, "role": "enemy_player", "profession": "Guardian",
    "elite_spec": "Dragonhunter", "team": "blue",
    "agent_addr": 9594, "instid": 5089, "combat_participant": true },

  { "id": 74, "role": "npc", "name": "Blood Fiend", "team": "blue",
    "agent_addr": 9718, "instid": 3287, "combat_participant": true }
]
```

`commander.segments` holds every closed `[tag-on, tag-off)` window this
player's commander tag was ever assigned, half-open, in the log's own
millisecond time base (same base as `markers[].time_ms` above — arcdps
session time, not encounter-relative). These are LITERAL per-instance
segments, not a coalesced whole-fight span: entity `7`'s real fixture data
above shows two, including a zero-width `[33847418, 33847418]` pair from an
immediate same-timestamp reassignment. There is no minimum-coverage
threshold and no fallback that extends a segment closed by a removal to the
log's end — only a segment that is *still open* when the log ends is closed
there. An unreciprocated removal (nothing open to close) is a silent no-op,
exactly as GW2EI's own commander-timeline construction treats it — see
`crate::wvw::markers::MarkerResolution::commander_segments`'s doc comment
for the full citation. An empty `segments` on a present `commander` means
the tag was detected but its windows could not be resolved, not that the
player never commanded.

Absent fields are omitted, never emitted as `null` or `""` — a player entity
has no `name`, an NPC has no `account`/`character`/`profession`. `id` is a
dense array index, stable **within this report only**; see "Joining across
reports" below.

### The `role` table

| `role` | Meaning |
|---|---|
| `squad` | In the recording squad |
| `friendly_player` | Non-squad player on the squad's team — a pug. Derived from the agent's subgroup: GW2 squad subgroups are 1-15, so a friendly with no subgroup is not in the squad. Matches GW2EI's `_nonSquadFriendlies` / `notInSquad` exactly on the calibration fixture |
| `enemy_player` | Non-squad, non-friendly player |
| `npc` | Every non-player enemy agent, including gadgets — there is no separate `gadget` role; it was dropped as unreachable (the upstream model cannot distinguish a gadget from an NPC once it reaches this layer) |

`combat_participant: bool` is always present and is what makes the legacy
"combat-participant enemy roster" view expressible as a filter: it is
`true` for every squad/friendly entity unconditionally, and for enemies it
is `true` only if that agent dealt damage to the squad, took damage from
the squad, or took CC from the squad.

### Reproducing the legacy views as filters

| Legacy view | 1.0 filter over `entities[]` |
|---|---|
| Squad roster | `role == "squad"` |
| Combat-participant enemy roster (native's legacy `enemies[]`) | `role != "squad" && combat_participant == true` |
| EI's curated `targets[]` roster (the MROSTER rule) | `role == "enemy_player"` |

The enemy-roster filter specifically depends on `combat_participant` being
carried on every entity — without it, a consumer would have no way to
reconstruct "did this agent actually interact with the squad" from the 1.0
document, since that predicate is not otherwise exposed per-entity outside
`entities[]`.

## `catalogs` — names appear exactly once

Real excerpt. `buffs` is from the default `/tmp/v1.json` run — `catalogs.skills`
is `{}` there, because nothing referenced a skill id without `--skill-damage`
or `--rotation`; the `skills` entry below is from the `--skill-damage` run
(`/tmp/v1-full.json`) instead, and `damage_mods` is from a `--modifiers` run
(`/tmp/v1-modifiers.json`), so this is a composite of three real documents,
not one single one:

```json
{
  "skills": {
    "736": { "name": "Bleeding", "is_swap": false, "can_crit": true }
  },
  "buffs": {
    "717": { "name": "Protection", "kind": "boon", "stacking": "duration", "max_stacks": 5 },
    "740": { "name": "Might", "kind": "boon", "stacking": "intensity", "max_stacks": 25 }
  },
  "damage_mods": {
    "-428": {
      "name": "Stability >= 10",
      "description": "With at least 10 stacks of Stability<br>Applied on All Damage<br>Compared against All Damage<br>Counter",
      "non_multiplier": false,
      "is_counter": true,
      "skill_based": false,
      "approximate": false
    },
    "174": {
      "name": "Empowered",
      "description": "1% per boon<br>No Minions<br>Applied on Strike Damage<br>Compared against All Damage",
      "non_multiplier": false,
      "is_counter": false,
      "skill_based": false,
      "approximate": false
    }
  }
}
```

`damage_mods` is omitted entirely (not `{}`) when nothing referenced it, per
the omit-when-absent convention — the default `--format json` run above
never populates it, since damage-modifier attribution needs `--modifiers`.
`skills`/`buffs` are always present, even when empty, since they are plain
`BTreeMap` fields with no `skip_serializing_if`.

The map key's SIGN encodes direction: negative ids (like `-428` above) are
incoming modifiers, positive ids outgoing — matching `-428`'s description
ending in "Compared against All Damage" applied to damage the player took.
Each `DamageModEntry`'s `non_multiplier`/`is_counter`/`skill_based`/
`approximate` are independent booleans, not a single folded classification
— GW2EI's own `damageModMap` carries them the same way, because a modifier
can be BOTH skill-based AND a multiplier at once, which a single label
cannot represent. They (and `description`) are present for every entry that
resolved against a known definition and omitted together — never `false`
or `""` — for an id with no known definition, so their absence is itself
the "no metadata for this id" signal.

Every id is a bare decimal string key — no `"s"`/`"d"` prefix like EI uses.
A catalog holds an entry **if and only if** some block in `blocks`
references that id (in both directions: every referenced id resolves, and
every catalog entry is referenced by something). `buffs.kind` is
three-valued: `"boon"`, `"condition"`, or `"effect"` — decided by catalog
membership against arcdps's fixed boon/condition tables, not by whether the
buff deals damage (several tracked conditions, like Chilled and Taunt,
don't). An id a block references but that has no name in the log's own
table still resolves — with an honest placeholder (`"Skill 424242"` /
`"Damage modifier 424242"`) — rather than being silently dropped.

## `blocks` — uniform id-keyed statistics

Every block is an aggregate slot plus a `by_entity` map keyed by entity id
(as a decimal string, since JSON object keys are always strings).

Four blocks carry a squad-level aggregate — `damage`, `cc`, `missiles`, and
`series`. All four `squad` slots are **required**: when the block is
present, so is its `squad`. There is no case where a missing aggregate would
mean anything a zero aggregate doesn't, so a consumer never has to branch on
whether one exists.

Real excerpt — entity `22`, the fixture's top damage dealer:

```json
{
  "damage": {
    "squad": { "total": 2138414, "dps": 43388.73896723141 },
    "by_entity": {
      "22": {
        "total": 205612,
        "dps": 4171.898143451354,
        "taken": 25518,
        "downs_dealt": 1,
        "kills_dealt": 0,
        "per_target": {
          "49": { "total": 10016 },
          "66": { "total": 18621 }
        }
      }
    }
  },
  "boons": {
    "by_entity": {
      "22": {
        "740": {
          "uptime_pct": 99.8640560008116,
          "avg_stacks": 19.432383078015622,
          "generation": { "self_pct": 3.00211017550979, "group_pct": 0.38801359439991884, "squad_pct": 0.041947415610802036, "self_wasted": 6.513 }
        }
      }
    }
  }
}
```

Notes proven by this example, not asserted in prose:

- `damage.squad` aggregates `role == "squad"` entities **only** — it will
  not include `friendly_player` rows once that role becomes reachable, by
  design (see the `role` table above).
- `per_target` is keyed by the **target's own entity id** (`"49"`, `"66"`
  above), so that target's own damage row and the damage dealt to it share
  the same integer — no positional joins anywhere in this format.
- `boons.by_entity`'s inner keys are buff ids from `catalogs.buffs` (`"740"`
  is Might, per the catalog excerpt above).
- `downs_dealt`/`kills_dealt` are outgoing outcomes and live here, on
  `damage`. Their incoming mirrors, `downs_taken` and `deaths`, live on
  `defenses` — the same split GW2EI makes (`defenses[0].downCount`/
  `deadCount`). All four are always present; none needs a flag.

### `damage` with `--skill-damage`: per-target detail and per-skill splits

The block above is the default shape. With `--skill-damage` on, each
`damage.by_entity[]` row gains a second per-skill map and each `per_target`
entry gains two more keys. Real excerpt (entity `22` against target `42`,
trimmed — this target was picked because it has only two skill rows):

```json
{
  "total": 205612,
  "dps": 4171.898143451354,
  "taken": 25518,
  "downs_dealt": 1,
  "kills_dealt": 0,
  "per_target": {
    "42": {
      "total": 506,
      "detail": {
        "connected_hits": 4,
        "connected_damage": 506,
        "against_downed_count": 0,
        "downed": 0,
        "killed": 0,
        "interrupts": 0,
        "downs_contribution_damage": 0
      },
      "by_skill": {
        "736":   { "total": 248, "hits": 3, "min": 77,  "max": 91,  "crit_hits": 0, "flank_hits": 1 },
        "76993": { "total": 258, "hits": 1, "min": 258, "max": 258, "crit_hits": 1, "flank_hits": 0 }
      }
    }
  },
  "by_skill":       { "...": "outgoing, keyed by skill id" },
  "by_skill_taken": {
    "723": { "total": 1,    "hits": 1, "min": 1, "max": 1,   "crit_hits": 0, "flank_hits": 0 },
    "737": { "total": 1045, "hits": 7, "min": 5, "max": 290, "crit_hits": 0, "flank_hits": 3 }
  }
}
```

- `by_skill` is **outgoing** per-skill damage; `by_skill_taken` is
  **incoming**, mirroring this row's own `total`/`taken` pair.
  `sum(by_skill[*].total) == total` and `sum(by_skill_taken[*].total) ==
  taken` hold exactly.
- `per_target[].by_skill` is the per-`(entity, target, skill)` breakdown —
  the same `SkillRow` shape, one level down.
- `per_target[].detail` is grouped under one key rather than flattened
  because those seven fields are computed only with `--skill-damage`.
  Flattening them would make an ungated row publish seven fabricated zeros;
  one optional key gives the gate a single, unambiguous presence signal. A
  `per_target` row can therefore legitimately carry `total` alone — `total`
  is ungated.
- `per_target` is a **union**: a target can appear with `detail` but no
  `total`, because down-contribution can be credited inside a target's
  downstate window without that target having taken a landed hit over the
  whole fight.
- `interrupts` and `downs_contribution_damage` are not derivable from any
  other block.
- Every `SkillRow` (all three families) carries `crit_hits`/`flank_hits` hit
  counts alongside `total`/`hits`/`min`/`max`. Every skill id emitted here
  resolves in `catalogs.skills`.

### `defenses` and `rotation` — the always-present extras

`defenses.by_entity[]` mirrors the legacy defensive stat block field for
field and ends with the two incoming outcome counters:

```json
{ "...": "...", "boon_strips_taken": 1, "boon_strips_taken_duration_ms": 300652,
  "downs_taken": 0, "deaths": 0 }
```

`rotation.by_entity[]` (needs `--rotation`) carries an `aftercast` object
beside its cast list — cast counters that are computed unconditionally but,
like the casts themselves, only published when the block is:

```json
{ "cast_count": 34,
  "casts": [{ "skill_id": 23275, "cast_time_ms": 7315, "duration_ms": 808, "time_gained_ms": 0, "quickness": 0.0 }, "..."],
  "aftercast": { "saved_count": 27, "saved_ms": 7749, "wasted_count": 4, "wasted_ms": 975 } }
```

Both `*_ms` values are milliseconds (GW2EI emits the same two quantities as
seconds). Note the name collision GW2EI bequeathed: `aftercast.wasted_count`
is a *cast-interrupt* count, an unrelated quantity to the boon-generation
`*_wasted` fields under `boons`.

### `coverage` — what a block's status means, and what to do about it

Real example (default flags — no `--replay`/`--rotation`/`--modifiers`):

```json
{
  "boons": "present", "cc": "present", "conditions": "not_computed",
  "contribution": "present", "damage": "present", "damage_mods": "not_computed",
  "defenses": "present", "healing": "present", "hit_stats": "present",
  "minions": "not_computed", "missiles": "not_computed", "replay": "present",
  "rotation": "not_computed", "series": "present", "support": "present"
}
```

| Value | Meaning | What a consumer should do |
|---|---|---|
| `present` | Computed, and `blocks` carries it, with at least one row | Read `blocks.<name>` directly |
| `not_computed` | The compute gate for it was off (e.g. `--replay` wasn't passed) | Do not treat this as "empty" — it is a missing flag, not a fact about the log. Re-parse with the flag on if you need it |
| `empty` | Computed, and there was genuinely nothing to report | Safe to treat as "zero rows", not an error. The block **is still carried** in `blocks` when `empty` — only `not_computed` and `unsupported` omit it |
| `unsupported` | This log cannot produce this block at all — its era, its encounter kind, or (today's only live case) the recorder that wrote it | Do not retry with a flag — the log itself cannot answer this. See `docs/EI-PARITY.md` for era-gated surfaces (pre/post `ResultEnumRework`, pre/post `AnimationAsStateChanges`) |

`not_computed` vs `empty` is the whole point of this map: without it, a
consumer parsing without `--rotation` and one parsing a log with zero casts
would both see an absent `rotation` block, with no way to tell "you forgot
a flag" from "this log really had nothing".

**Which values today's binary can actually emit.** All four. The example
above shows no `empty` only because this fixture happens to populate every
block it computes; `empty` is what a block reports when it ran over a roster
that produced no rows. Each block decides what "nothing to report" means for
its own shape — `series` and `missiles` carry squad-level aggregates that are
computed independently of any per-entity row, so they are not `empty` merely
for having an empty `by_entity`.

`unsupported` has exactly one live producer today: **`healing` on a log
recorded without the arcdps healing addon.** That extension is written by a
separate plugin, and a log whose recorder did not run it carries no healing
events at all — so no flag, and no pass this project could add later, can
ever produce those numbers. Earlier versions reported that case as `empty`,
which told you the squad healed for zero; it is the difference between an
unanswered question and an answer of nothing, and it is the whole reason
this map exists. Every other block is era- and encounter-kind-agnostic, so
`unsupported` stays reserved vocabulary for the rest of them until the
era-gated surfaces land.

All four states are pinned as REACHABLE by
`crates/axilog-schema/tests/v1_coverage_states.rs`, deliberately by
reachability rather than by pinning particular blocks to particular values:
a block moving between `present` and `empty` as the analysis improves is
not a regression, but a state that no longer occurs anywhere is — an
unreachable state is one a consumer cannot rely on.

### Getting a complete document: `--all`

Rather than enumerating the gates, pass `--all` (CLI) or `everything: true`
/ `everything=True` (Node/Python SDKs). It is defined as **"every analysis
pass this version knows about"**, not as a fixed list, so a consumer that
sets it keeps getting complete documents as later versions add passes.

That definition is the point. The first axibridge cutover audit found 30
blank fields caused by exactly the opposite: a consumer's hand-maintained
option list drifting from the parser's. With `--all`, the only blocks left
reporting anything other than `present`/`empty` are the ones the LOG cannot
answer — which is the `unsupported` case above, and no flag can change it.

It is a UNION with the individual flags, never an override, so `--all` and
`--all --replay` mean the same thing. The cost is the sum of each gate's own
cost; `--replay` and `--modifiers` dominate. See `docs/BENCHMARKS.md` for
measured timings, peak memory, and per-block payload on the committed
fixture.

## The series envelope

Every time series in the format (per-second damage, boon-stack timelines,
tick rate, etc.) uses one envelope. `blocks.series.squad.*` is computed
unconditionally; per-entity rows in `blocks.series.by_entity` need
`--timeseries`. Real example from the fixture, RLE-encoded (entity `0` had
zero damage output for the whole 51-second window under `--timeseries`, so
it collapses to one run):

```json
{ "interval_ms": 1000, "len": 51, "enc": "rle", "data": [[0, 51]] }
```

And the squad total damage series from the same fixture (present with or
without `--timeseries`), raw-encoded (no long runs to exploit):

```json
{ "interval_ms": 1000, "len": 50, "enc": "raw", "data": [6425, 2060, 2616, 530, 0, "..."] }
```

- `enc` is `"raw"` (a plain array of values) or `"rle"` (an array of
  `[value, run_length]` pairs) — the encoder picks whichever serializes
  smaller, per series.
- `len` is the **decoded** length in both cases, never `data.length` — so a
  consumer can allocate a buffer before decoding and validate the result
  after.
- Treat `enc` as an open, tagged set: a third value may be added later
  (additive under the 1.x rules); a decoder that only handles the two
  documented values today should treat an unrecognized `enc` as an error,
  not silently misread `data`.

### Decoder — JavaScript

Five lines, and it is exactly what was run against real output to produce
the numbers above (`node decode.js /tmp/v1.json`, using
`doc.blocks.series.squad.damage` and an RLE row from
`doc.blocks.series.by_entity`):

```js
function decodeSeries(s) {
  if (s.enc === "raw") return s.data.slice();
  const out = [];
  for (const [value, run] of s.data) for (let i = 0; i < run; i++) out.push(value);
  return out;
}
```

Verified: `decodeSeries(doc.blocks.series.squad.damage).length === 50`
(matches `len`), and an RLE row (`[[0, 51]]`) decodes to 51 zeros.

### Decoder — Python

Same algorithm, run against the same real document
(`python3 decode.py /tmp/v1.json`):

```python
def decode_series(s):
    if s["enc"] == "raw":
        return list(s["data"])
    out = []
    for value, run in s["data"]:
        out.extend([value] * run)
    return out
```

Verified: `len(decode_series(doc["blocks"]["series"]["squad"]["damage"])) == 50`,
and the same RLE row decodes to `[0] * 51`.

Both decoders were executed against `/tmp/v1.json` (default flags) and
`/tmp/v1-full.json` (`--timeseries --skill-damage --rotation --modifiers
--missiles --replay`, which populates `series.by_entity` and exercises a
real RLE row) as part of writing this document; both round-tripped `len`
correctly and matched the raw/RLE encoding the binary actually chose.

## Buff stack timelines — `boons`' second gate, and `conditions`

Two blocks carry per-buff **stack timelines**: when a buff was up, and at
what stack count. Both ride `--timeseries` (`timeseries: true` in the SDKs).

`blocks.boons` rows gain two fields under that flag — `states`, the fused
timeline, and `per_source`, the same split by applier. This makes `boons`
the second two-gate block after `replay`: its uptime/generation numbers are
computed on every parse, its timelines are not, so **`coverage.boons` is
about the uptime half only** and says nothing about whether timelines are
present. Check for the fields.

`blocks.conditions` is the enemy-side counterpart, and is wholly gated —
`coverage.conditions` does settle the question there. Its rows carry
`per_source` and no fused `states`: summing appliers would not reconstruct
one, because two players holding the same duration condition overlap rather
than stack.

Real excerpt (`--timeseries`), showing only the fields this section adds —
entity `22`'s Might (`740`), and a condition on enemy entity `42`:

```json
{
  "boons": {
    "by_entity": {
      "22": {
        "740": {
          "states": [[0, 0], [14, 6], [15, 12], [2211, 16], "..."],
          "per_source": {
            "by_source": { "18": [[0, 0], [14, 1], [15, 3], [6385, 0]] }
          }
        }
      }
    }
  },
  "conditions": {
    "by_entity": {
      "42": {
        "19426": { "per_source": { "by_source": { "12": [[0, 0], [24654, 1], [26654, 0]] } } }
      }
    }
  }
}
```

The two halves read together: entity `22` held 16 stacks of Might at
`2211 ms` in total, of which entity `18` was holding 3. The fused `states`
counts stacks from every applier; each `by_source` entry counts only that
one applier's.

A timeline is `[[time_ms_from_log_start, stacks], ...]`, always opens with a
`[0, 0]` pair, and never carries two pairs at one timestamp. Duration buffs
report `0`/`1`; only intensity buffs (Might, Stability, most damaging
conditions) exceed 1.

Three things worth knowing:

- **`per_source.by_source` is keyed by the APPLIER's entity id**, joining
  back into `entities[]` like every other key here. The upstream analysis
  keys these by the applier's character *name*; native does not, both
  because a name is identity data this format confines to `entities[]` and
  because two players sharing a character name collide onto one key.
- **`per_source.unresolved`** is an optional sibling: one merged timeline
  for appliers that resolve to no `entities[]` row at all. It exists so
  those applications are neither dropped nor given a fabricated entity id.
  It is normally absent, and always absent on `conditions`, whose appliers
  are narrowed to the squad.
- **`states` may be `[]`** — meaning the timeline pass ran and this entity
  never held the buff. A real timeline always has at least its leading
  `[0, 0]`, so `[]` is unambiguous, and `states` being present at all is
  the honest signal that `--timeseries` was on.

## Combat replay — two halves, two gates

`blocks.replay` is the other block whose halves are gated differently. Along
with `boons` above, it is where `coverage` does not settle the whole
question.

`by_entity` — down/dead intervals plus each squad player's own first/last-
aware bounds — is **always present**. Computing it is a min/max scan plus a
status-event walk with no position decode, so every parse pays for it
whether or not you asked for a replay.

`tracks` — the downsampled position samples, and the `poll_ms`/`bounds`
metadata that only describes them — rides `--replay`, because that is the
expensive half.

```json
{
  "by_entity": {
    "0": {
      "start_ms": 3,
      "end_ms": 49266,
      "active_ms": 49263,
      "down": [[12642, 15512]],
      "dead": [],
      "dc": [],
      "dist_to_com": 281.37823486328125,
      "stack_dist": 172.18305969238281
    }
  },
  "tracks": {
    "poll_ms": 300,
    "bounds": { "min_x": -23880.6, "min_y": -31541.4, "max_x": -197.0, "max_y": 15974.8 },
    "by_entity": {
      "0": {
        "samples": [[300, -11097.6, -23619.4], [600, -11156.3, -23711.1], "..."],
        "down_intervals": [[12642, 15512]],
        "dead_intervals": [],
        "dc_intervals": []
      }
    }
  }
}
```

Four things a consumer needs to know about that shape:

- **`coverage.replay == "present"` does not mean positions are available.**
  It answers the intervals question, which this block can always answer.
  Check for `tracks` yourself.
- **`active_ms` subtracts dead time but NOT down time.** That is GW2EI's own
  definition, verified against a real export; it is carried as a field
  precisely so nobody re-derives it under the more intuitive reading and
  quietly under-reports every player who went down.
- **`by_entity` covers the squad only; `tracks.by_entity` also covers enemy
  players.** That is why a track keeps its own copy of the intervals: the
  always-on pass never walks the enemy roster, so dropping them from the
  track would take every enemy player's down/dead history with it. For a
  squad entity the two copies come from the same computation and cannot
  disagree.
- **`dc`/`dc_intervals` cover disconnect/not-yet-spawned windows
  (`CBTS_DESPAWN` to the matching `CBTS_SPAWN`), and are not mutually
  exclusive with `down`/`dead` — an agent can despawn while dead. Every
  interval in this block is half-open `[start_ms, end_ms)`, which is a
  deliberate divergence from GW2EI's own `dc` export: GW2EI brackets the
  pre-spawn/post-despawn ends with an inclusive sentinel
  (`[i64::MinValue, FirstAware]`/`[LastAware, i64::MaxValue]`) rather than a
  true half-open interval. The cutover report measured that difference at 6
  of 6,894 samples (0.087%) of axibridge's current distance error — small
  enough that matching this format's own half-open convention throughout
  was judged more valuable than byte-parity with GW2EI's sentinel choice.
  An agent that is still disconnected at log end is left with an unclosed
  `dc` interval (no synthesized closing bound), matching every other
  interval kind in this block.

### `dist_to_com` / `stack_dist`

These two are GW2EI's `statsAll[].distToCom` and `.stackDist`, computed
engine-side: the mean distance, in world inches, from this player to the
commander and to the squad centre, over the player's own active polls.
They are **absent unless `--replay` was passed** — they are a reduction over
the position tracks and cannot exist without them — but they live on the
always-present `by_entity` row rather than inside `tracks` so that absence
stays meaningful.

That matters, because there are three states and only two of them look
alike:

| Value | Meaning |
|---|---|
| absent | The replay pass did not run. Nothing was measured. |
| `-1` | The pass ran; this actor had no poll that paired with a reference. GW2EI's own sentinel — it emits `-1`, not `null`, and EI-shaped readers already reject it by value. |
| `>= 0` | A real mean distance. `0` is reachable and correct: the commander's own `dist_to_com` is exactly `0`. |

Collapsing absent into `-1` (or `-1` into absent) destroys the distinction.
Treat any negative value as "no answer" and never as a distance.

The reduction's rules are exacting and are documented in full, with GW2EI
source citations, on `axilog_core::analysis::distance`. Two are worth
repeating here because they surprise people reading the numbers:

- **The two references are asymmetric on purpose.** The squad centre is the
  per-poll mean of every squad player's *active* position, so a downed
  player drops out of it. The commander reference, separately, is the
  commanding player's *raw* positions during their tag windows, so a downed
  commander still anchors the squad. That is GW2EI's behaviour, verified
  against GW2EI source.
- **Distance is measured in the XY plane.** Z is discarded, so two players
  stacked vertically are at distance zero.

This reduction was checked against GW2EI's own exported positions to a
worst-case error of 0.0104 / 0.0073 inches over 41 actors -- under the
floor set by that fixture's 3-decimal-place pixel rounding. That check
directly certifies three of the eight rules the reduction depends on (the
mean, not median; the squad-centre poll cap; and the squad centre's
active-position filter, the first half of the asymmetry above). It also
certifies a fourth, the participation filter, via a separate end-to-end
check. The remaining rules -- including the commander reference's raw
positions, the second half of that same asymmetry -- have zero measured
effect on that particular fixture (no commander in it goes down while
tagged) and rest instead on their own unit tests in
`axilog_core::analysis::distance` and on the GW2EI source citations there.

The position track itself is the one exception to the series envelope —
raw `(t_ms, x, y)` triples rather than a `SeriesOut`. This is deliberate, not an oversight: `SeriesOut` assumes a dense array
starting at `t=0` on a fixed step, but a replay track starts at that
agent's own first-aware time rounded up to the polling grid — usually not
zero. Encoding it through `SeriesOut` would silently drop that start offset
and misrepresent every sample's real timestamp, so the track carries its
own explicit `t_ms` per sample instead.

## 1.x compatibility rules

Enforced by test (`crates/axilog-schema/tests/v1_shape.rs`,
`v1_equivalence.rs`), stated here for consumers:

- **Additive-only within a major.** New blocks, new catalog entries, new
  optional fields on existing records, new `enc` values, new `coverage`
  values, and new `warnings` codes are all fine to appear without warning.
- **Breaking, requires a major bump:** renaming or removing a field,
  retyping a field, changing a field's meaning, changing the `entities[]`
  sort order (which changes every `id`), or changing what an existing
  `coverage`/`warnings` value means.
- Absent optional fields are omitted entirely, never serialized as `null`.
  A consumer should treat "key absent" and "key null" as the same signal,
  but should not expect to see the latter.

**The rules above are not yet in force.** 1.0 is explicitly still malleable:
until it is declared frozen here, a shape that turns out wrong gets fixed
rather than carried, and breaking changes land without a major bump. The
licence is narrow — it exists because 1.0 has no external consumer reading
it yet (the ei-json adapter is its only reader, and it is in-tree), and it
ends the moment one does.

Breaking changes made under it are recorded here rather than passed over,
because the key-set golden diff shows them as bare removals to anyone
bisecting:

- The replay split moved `blocks.replay.{poll_ms,bounds,by_entity}` down
  under `blocks.replay.tracks` and gave `blocks.replay.by_entity` a new
  meaning — nine keys removed at the old paths.
- The damage-modifier split moved `blocks.damage_mods.by_entity.<id>.<mod>`
  down under a `.overall` key, to make room for the per-target scope beside
  it — five keys removed at the old paths.
- `blocks.rotation.by_entity.<id>.casts` became optional, as the format's
  `--rotation` gate record, and `cast_count` went with it — one key removed.
  The count was exactly `casts.len()`, so keeping it would have meant two
  fields encoding one gate, free to disagree.
- Phase B (native-format-gap-closure Task 4) widened
  `blocks.damage.by_entity.<id>.per_target.<id>.detail` from 7 fields to 23,
  additively — the same `--skill-damage` gate governs the whole group, so
  no new presence signal was needed. Recorded here (unlike the additive
  bullet above would otherwise require) because it moved the key-set
  golden by 16 entries in one commit; a bisect landing between the old and
  new counts should read this rather than re-deriving it from the diff.

  The 16 new native fields on `PerTargetDetail` mirror
  `axilog_core::analysis::per_target::PerTargetOffense` field-for-field:
  `direct_count`, `direct_damage`, `crit_count`, `crit_damage`,
  `flank_count`, `glance_count`, `critable_direct_count`,
  `against_downed_damage`, `missed`, `evaded`, `blocked`, `invulned`,
  `applied_total`, `applied_duration_ms`, `applied_downs_contribution`,
  `applied_duration_downs_contribution_ms`. Of those, `critable_direct_count`
  is native-only — it is `criticalRate`'s denominator, which real EI never
  publishes per target, so `to_ei_json`'s `statsTargets` split fills the
  other 15 under their EI key names (`directDmg`, `criticalRate`,
  `criticalDmg`, `flankingRate`, `glanceRate`, `connectedDirectDamageCount`,
  `againstDownedDamage`, `missed`, `evaded`, `blocked`, `invulned`,
  `appliedCrowdControl`, `appliedCrowdControlDuration`,
  `appliedCrowdControlDownContribution`,
  `appliedCrowdControlDurationDownContribution`) and omits the 16th. Note
  `directDmg` maps to `direct_damage`, not to the pre-existing
  `connected_direct_dmg` field elsewhere in this schema — the two measure
  different quantities despite the similar name.

## The ei-json layer is permanent

The side-channel absorption work moves data *into* this format and makes
the Elite Insights-compatible output read from it — it does **not** work
toward deleting that output. `to_ei_json` stays, indefinitely, as the
compatibility path for downstream consumers that have not moved to the
native format and for those that never will. The goal is for it to be
thin: a translation over this document, with no analysis of its own and no
private data it alone can see. "Thin and lean," not "eventually gone."

## Joining across reports

`entities[].id` is a dense array index, stable **only within the report
that produced it** — parsing the same log twice yields the same ids (this
is asserted by a determinism test), but two different logs — or the same
fight parsed by two different axilog versions — do **not** share an id
space. **Join across reports on `account`** (the `:Name.1234` string), which
is globally stable for a given player. `instid` is a fallback for
non-player agents or when account is unavailable, but it is not stable
across relogs within a single fight, let alone across fights — the entity's
`agent_addr` and `instid` fields exist for correlating against raw arcdps
or another tool, not for cross-report joins.
