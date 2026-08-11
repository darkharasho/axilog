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
  ]
}
```

`recorded_by` is an **entity id** (`22` here — an integer, joins into
`entities[]`), not the raw account string. `markers[]` entries carry both
`agent_addr` (always present) and `entity_id` (present only when the
marker's agent resolved to a tracked entity — `arcdps` does not restrict
`CBTS_MARKER` to squad members, so a marker on untracked friendly siege is
ordinary and still needs to survive with `agent_addr` alone).

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

Absent fields are omitted, never emitted as `null` or `""` — a player entity
has no `name`, an NPC has no `account`/`character`/`profession`. `id` is a
dense array index, stable **within this report only**; see "Joining across
reports" below.

### The `role` table

| `role` | Meaning |
|---|---|
| `squad` | In the recording squad |
| `friendly_player` | Non-squad player on the squad's team. **Currently unreachable in practice** — `Player::in_squad` is hardcoded `true` upstream, so no real log emits this role yet. Populating it accurately is required follow-up work, not done here |
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
(`/tmp/v1-full.json`) instead, so this is a composite of two real documents,
not one single one:

```json
{
  "skills": {
    "736": { "name": "Bleeding", "is_swap": false, "can_crit": true }
  },
  "buffs": {
    "717": { "name": "Protection", "kind": "boon", "stacking": "duration", "max_stacks": 5 },
    "740": { "name": "Might", "kind": "boon", "stacking": "intensity", "max_stacks": 25 }
  }
}
```

`damage_mods` is omitted entirely (not `{}`) when nothing referenced it, per
the omit-when-absent convention — the default `--format json` run above
never populates it, since damage-modifier attribution needs `--modifiers`.
`skills`/`buffs` are always present, even when empty, since they are plain
`BTreeMap` fields with no `skip_serializing_if`.

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
(as a decimal string, since JSON object keys are always strings). Real
excerpt — entity `22`, the fixture's top damage dealer:

```json
{
  "damage": {
    "squad": { "total": 2138414, "dps": 43388.73896723141 },
    "by_entity": {
      "22": {
        "total": 205612,
        "dps": 4171.898143451354,
        "taken": 25518,
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
- `by_skill` rows (when `--skill-damage` is on) carry `crit_hits`/
  `flank_hits` hit counts alongside `total`/`hits`/`min`/`max`, mirroring
  the legacy per-skill row field for field.

### `coverage` — what a block's status means, and what to do about it

Real example (default flags — no `--replay`/`--rotation`/`--modifiers`):

```json
{
  "boons": "present", "cc": "present", "conditions": "not_computed",
  "contribution": "present", "damage": "present", "damage_mods": "not_computed",
  "defenses": "present", "healing": "present", "hit_stats": "present",
  "minions": "not_computed", "missiles": "not_computed", "replay": "not_computed",
  "rotation": "not_computed", "series": "present", "support": "present"
}
```

| Value | Meaning | What a consumer should do |
|---|---|---|
| `present` | Computed, and `blocks` carries it | Read `blocks.<name>` directly |
| `not_computed` | The compute gate for it was off (e.g. `--replay` wasn't passed) | Do not treat this as "empty" — it is a missing flag, not a fact about the log. Re-parse with the flag on if you need it |
| `empty` | Computed, and there was genuinely nothing to report | Safe to treat as "zero rows", not an error |
| `unsupported` | This log's era or encounter kind cannot produce this block | Do not retry with a flag — the log itself cannot answer this. See `docs/EI-PARITY.md` for era-gated surfaces (pre/post `ResultEnumRework`, pre/post `AnimationAsStateChanges`) |

`not_computed` vs `empty` is the whole point of this map: without it, a
consumer parsing without `--rotation` and one parsing a log with zero casts
would both see an absent `rotation` block, with no way to tell "you forgot
a flag" from "this log really had nothing".

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

## Combat replay — NOT series-encoded

`blocks.replay` (present only with `--replay`) is the one exception to the
series envelope. Each entity's track is raw `(t_ms, x, y)` triples:

```json
{
  "poll_ms": 300,
  "bounds": { "min_x": -23880.6, "min_y": -31541.4, "max_x": -197.0, "max_y": 15974.8 },
  "by_entity": {
    "0": {
      "samples": [[300, -11097.6, -23619.4], [600, -11156.3, -23711.1], "..."],
      "down_intervals": [],
      "dead_intervals": []
    }
  }
}
```

This is deliberate, not an oversight: `SeriesOut` assumes a dense array
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
