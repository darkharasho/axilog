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
  --format json --output v1.json
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
  "objectives": [
    {
      "map_id": 96,
      "objective_id": 37,
      "objective_type": "Keep",
      "owners": [
        { "team_id": 433, "time_ms": 0 },
        { "team_id": 2767, "time_ms": 44684 }
      ]
    }
  ],
  "started_at_unix": 1768702180,
  "log_start_ms": 33847418
}
```

`recorded_by` is an **entity id** (`22` here — an integer, joins into
`entities[]`), not the raw account string. `markers[]` entries carry both
`agent_addr` (always present) and `entity_id` (present only when the
marker's agent resolved to a tracked entity — `arcdps` does not restrict
`CBTS_MARKER` to squad members, so a marker on untracked friendly siege is
ordinary and still needs to survive with `agent_addr` alone).

`objectives[]` is the WvW objective ownership record, from arcdps's
`CBTS_WVWOBJECTIVESTATUS`. One entry per objective the log mentions, each
carrying every ownership observation for it in log order — so a keep that
changed hands has the flip in its `owners` list, at `time_ms` relative to
the log start. `objective_type` is one of `Camp`, `Ruins`, `Tower`, `Keep`,
`Castle`; an objective whose `(map_id, objective_id)` pair is not in the
static catalog is **dropped**, never emitted with an `"Unknown"` type, which
matches GW2EI. Repeated identical owner entries are kept rather than
collapsed, also matching GW2EI — treat the list as an event log, not a
deduplicated history. The array is always present and is empty for non-WvW
logs and for logs predating the event.

`teams[].shard_id` (omitted when absent, in the same example above) is the
world/shard id from `CBTS_WVWTEAMS`. It is *not* the team id: `team_id`
identifies the colour side within this match, `shard_id` identifies the
server world playing it. A team can have a `color` and no `shard_id` — the
colour then came from the static fallback id table rather than from the
log's own event.

`started_at_unix` is the wall-clock log start, seconds since the epoch, read
from arcdps's own `CBTS_LOGSTART`/`CBTS_SQCOMBATSTART` event -- the SERVER
timestamp specifically, not the recording client's local clock (arcdps
records both; a machine's own clock skew is not a fact about the log).
Omitted (not `0`) when the log carries no such event, so absence stays
distinguishable from epoch zero. This replaces inferring the start time from
the `.zevtc` file's mtime, which breaks for any copied or restored file.

`log_start_ms` is the log's **`t0`**: the arcdps *session-time* millisecond
stamp of the log's first event. Every other time in this document is already
measured from it — with exactly two exceptions, both raw event times passed
through deliberately: `markers[].time_ms` and
`entities[].commander.segments`. Note the example above: the marker's
`time_ms` of `33847418` is not "9.4 hours into a 49-second fight", it is the
same instant as `t0`. Session time has no fixed origin, so those two fields
are uninterpretable on their own; subtract `log_start_ms` from either to get
an encounter-relative value comparable against `duration_ms`. The result can
be **negative** — a commander tag held before the log's first event is
ordinary — which is why the rebase is left to you rather than done here and
clamped. Always present; `0` for a log with no events.

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

`commander.segments` holds every terminated `[tag-on, tag-off)` window this
player's commander tag was ever assigned, half-open, in the log's own
millisecond time base (same base as `markers[].time_ms` above — arcdps
session time, not encounter-relative — subtract `encounter.log_start_ms` to
rebase). These are LITERAL per-instance
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

Real excerpt. `buffs` is from the default `v1.json` run — `catalogs.skills`
is `{}` there, because nothing referenced a skill id without `--skill-damage`
or `--rotation`; the `skills` entry below is from the `--skill-damage` run
(`v1-full.json`) instead, and `damage_mods` is from a `--modifiers` run
(`v1-modifiers.json`), so this is a composite of three real documents,
not one single one:

```json
{
  "skills": {
    "736": { "name": "Bleeding", "is_swap": false, "can_crit": true },
    "9284": { "name": "Flame Blast", "is_swap": false, "can_crit": false,
              "is_gear_proc": true, "is_not_accurate": true, "is_instant_cast": true }
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

`blocks.damage_mods` carries a second, non-entity-keyed field beside
`by_entity`: `personal`, a map from SPEC name to the signed modifier ids
that belong to that spec.

```json
"personal": { "Firebrand": [18, 107, 108, 109, 111, 313, 403], "Druid": [-132, 25, 334] }
```

The key is the same string an entity's `elite_spec` carries (its
`profession`, for a core build), so it joins to the roster without a
profession table of its own. Every id here is a key in
`catalogs.damage_mods`, so the two partition the referenced id space
between them. What is left over is the SHARED pool —
relics, food, squad buffs — whose damage gain is credited to every player
who benefited from it rather than to whoever provided it.

This is the only field that draws that line, and the reading of an EMPTY
map matters: it means the classification is UNAVAILABLE, never that
nothing is personal. A consumer that filters on the latter reading hides
every modifier there is, which is exactly what happened downstream while
this field did not exist. It rides `catalogs.damage_mods`' own `--modifiers`
gate, since a partition of a table the consumer cannot see would be no use
on its own.

A skill entry's five MPROC flags — `is_trait_proc`, `is_gear_proc`,
`is_unconditional_proc`, `is_not_accurate`, `is_instant_cast` — are
**omitted when `false`**, unlike their `is_swap`/`can_crit` neighbours.
Absence means `false`, not "unknown". They are sparse, and emitting
~370 × 5 literal `false`s cost 16% of the rendered report.

Two properties worth knowing before consuming them:

- **They are log-specific, not build-specific.** They come from GW2EI's
  `InstantCastFinder` availability, which is gated on predicates over the
  log's own contents as well as on build ranges. Two logs recorded at the
  same GW2 build can legitimately disagree.
- **`is_instant_cast` is strictly stronger than the other four.** Those
  four say a finder for this skill was AVAILABLE; `is_instant_cast` says
  one actually FIRED in this log. A log recorded WITHOUT effect events
  will therefore carry far fewer of them than the same fight recorded
  with — for many traits and sigils the spawned visual is the only trace
  of the proc. That is a property of the recording, not of the fight.

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

**`skill_id` can be a pseudo id.** The cast list holds all three families
Elite Insights merges — real animated casts, synthesized instant casts, and
weapon swaps — and EI numbers its synthetic skills NEGATIVELY (`-2` is a
weapon swap; the instant-cast catalog uses roughly three dozen more). Skill
ids are `u32` everywhere in this format, so those arrive as their two's
complement bit pattern: **`4294967294` is `-2`**, and in general
`id_signed = id as i32`. The ei-json adapter casts back, so an `ei-json`
export writes `-2` and keys `skillMap` as `"s-2"`, exactly as EI does.
`catalogs.skills` names them (`"Weapon Swap"`), so a consumer that resolves
ids through the catalog needs no special handling; one that formats the
number itself should apply the cast.

An instant cast or weapon swap always has `duration_ms == 0`,
`time_gained_ms == 0` and `quickness == 0.0` — which is also how to tell the
families apart after the fact: any cast with `duration_ms > 1` is a real
animated cast, and any other is not.

### `coverage` — what a block's status means, and what to do about it

Real example (default flags — no `--replay`/`--rotation`/`--modifiers`):

```json
{
  "boons": "present", "cc": "present", "conditions": "not_computed",
  "contribution": "present", "damage": "present", "damage_mods": "not_computed",
  "defenses": "present", "healing": "present", "hit_stats": "present",
  "minions": "not_computed", "missiles": "not_computed", "replay": "present",
  "rotation": "not_computed", "self_effects": "not_computed", "series": "present",
  "squad_buffs": "present", "support": "present"
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
the numbers above (`node decode.js v1.json`, using
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
(`python3 decode.py v1.json`):

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

Both decoders were executed against `v1.json` (default flags) and
`v1-full.json` (`--timeseries --skill-damage --rotation --modifiers
--missiles --replay`, which populates `series.by_entity` and exercises a
real RLE row) as part of writing this document; both round-tripped `len`
correctly and matched the raw/RLE encoding the binary actually chose.

## Buff stack timelines — `boons`' second gate, `conditions`, and `self_effects`

Three blocks carry per-buff **stack timelines**: when a buff was up, and at
what stack count. All three ride `--timeseries` (`timeseries: true` in the
SDKs).

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
        "736": { "uptime_pct": 41.9, "avg_stacks": 3.7, "states": [[0, 0], [1204, 2], "..."] },
        "872": { "uptime_pct": 0.232, "states": [[0, 0], [65670, 1], [66479, 0]] }
      }
    }
  }
}
```

`avg_stacks` is present exactly for the intensity-stacking effects (the six
`BuffStackType.Stacking` conditions, `CommonBuffs.cs:36-40` + `:49`) and
omitted for the rest, the same rule `boons` rows follow — an absent `avg_stacks` means "duration-stacking", never zero.

## `squad_buffs` — the rest of what a player held

Elite Insights keeps boons, conditions and everything else a player held in
one `buffUptimes` array. This format splits that population by family, and
`blocks.squad_buffs` is the third piece: every buff that is neither one of
the 12 boons nor a condition/control effect — **sigils, relics, food,
utilities, auras, signets, trait buffs**. A consumer rebuilding EI's single
array concatenates `boons` and this block; the three id sets are disjoint by
construction, so no id appears twice.

Unlike `conditions` and `self_effects` this block is **always-on**. It emits
uptime only — no `states` — which is the cost `boons`' own always-on uptime
half already carries, so no flag gates it and `coverage.squad_buffs` is
`present` on a default parse. Nothing plots a sigil's stack count over time;
a timeline per player per buff would multiply the block's payload by an
order of magnitude for a graph no consumer draws. Adding `states` later is
additive.

```json
{
  "squad_buffs": {
    "by_entity": {
      "0": {
        "9286": { "uptime_pct": 99.9655, "avg_stacks": 24.9913 },
        "10332": { "uptime_pct": 6.087 }
      }
    }
  }
}
```

`avg_stacks` follows the same rule as everywhere else in this format:
present exactly for intensity-stacking buffs, omitted — never zero — for
duration ones.

An id is admitted only when some catalog states its stack type; a buff
whose stack type is unknown cannot be simulated without guessing between
the duration and intensity machines, which produce different numbers, and
Elite Insights likewise tracks only the buffs its own container defines.
One deliberate deletion mirrors EI: a non-Weaver elementalist's log carries
the four Weaver dual-attunement ids alongside the plain ones, and EI drops
them for such a player (`ElementalistHelper.RemoveDualBuffs`), so this
block does too — otherwise every Tempest would carry a duplicate of its own
attunement row at a plausible uptime.

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

`tracks` — the downsampled position samples, and the `poll_ms`/`bounds`/
`arena` metadata that only describes them — rides `--replay`, because that is
the expensive half.

### Plotting positions — `tracks.arena`

Samples are raw **world (game-inch) coordinates**. That is the honest thing
to carry: it is what arcdps records, and it is independent of anybody's
canvas. It is also unplottable on its own — turning a world coordinate into
a map pixel needs the per-map world rect, which is static GW2 data.

Rather than make every consumer re-transcribe that table (axilog already
holds it in `axilog_core::wvw::maps`, and a second copy one repository out
is a second copy free to drift), the rect travels with the samples:

```json
"arena": {
  "image_width": 697,
  "image_height": 1000,
  "image_url": "https://i.imgur.com/nVu2ivF.png",
  "world_min_x": -30720.0,
  "world_min_y": -43008.0,
  "world_max_x": 30720.0,
  "world_max_y": 43008.0
}
```

World y grows northward and image y grows downward, so the y axis flips:

```js
const px = (x - a.world_min_x) / (a.world_max_x - a.world_min_x) * a.image_width
const py = (1 - (y - a.world_min_y) / (a.world_max_y - a.world_min_y)) * a.image_height
```

Scale both by `canvas / image_*` to render at any size. Doing exactly that
reproduces GW2EI's own combat-replay pixel for every map in the table, which
is asserted rather than asserted-in-prose (`arena_tests::
projection_reproduces_gw2eis_transform_on_every_map`).

Nothing in `arena` is pre-rounded or pre-rescaled. GW2EI's exported
`combatReplayMetaData` carries `sizes` already squeezed to a 750px maximum
dimension and an `inchToPixel` rounded to three decimals — both artifacts of
its renderer. Those are derivable from these numbers; these are not
recoverable from those.

`arena` is **omitted for a map id with no hand-authored arena image** (GW2EI
has none for Obsidian Sanctum or Armistice Bastion, and none for any non-WvW
id). A consumer then has only `bounds`, which is the union of the *observed*
positions rather than a fixed frame — so two logs on the same map do not
share a coordinate space, and `bounds` must not be used as if they did.

`encounter.map_id` carries the raw `CBTS_MAPID` value separately, for
consumers joining against their own per-map assets (tile sets, objective
catalogs, landmark tables). It is present with or without `--replay`; match
on it rather than on the `encounter.map` display string, which is prose.

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
  An agent that is still disconnected at log end gets **no interval at
  all** for that window — the still-open `dc` is dropped, not closed at the
  log's end and not emitted unclosed (`build_intervals` discards `dc_open`
  on scan end; pinned by
  `unclosed_dc_interval_at_log_end_is_dropped`). That is the honest
  half-open analogue of GW2EI's sentinel bracketing: GW2EI has no real
  closing bound there either, it just writes `[LastAware, MaxValue]`.
  Commander `segments`, by contrast, *are* closed at log end, because
  GW2EI's `CalculateCommanderStates` explicitly clamps with
  `Math.Min(markerEvent.EndTime, log.LogData.EvtcLogEnd)` — different
  upstream rule, deliberately different behaviour. **Consequence:** active
  time derived as `end_ms - start_ms` minus summed `dc` over-counts for
  every player who disconnects and never returns; use `active_ms`, or
  treat a missing trailing `dc` accordingly.

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
| absent | **The pass never ran.** `--replay` was not passed, so no positions were decoded and nothing was measured. |
| `-1` | **The pass ran and nothing qualified.** Positions were decoded; this actor had no poll that paired with a reference. GW2EI's own sentinel — it emits `-1`, not `null`, and EI-shaped readers already reject it by value. |
| `>= 0` | A real mean distance. `0` is reachable and correct: the commander's own `dist_to_com` is exactly `0`. |

Collapsing absent into `-1` (or `-1` into absent) destroys the distinction.
Treat any negative value as "no answer" and never as a distance.

These two scalars are the **one** part of `by_entity` that depends on the
`--replay` gate; every other field on that row (`start_ms`, `end_ms`,
`active_ms`, `down`, `dead`, `dc`) is computed on every parse and is
byte-identical with and without the gate. If you assert "turning positions
on must not change the intervals half", scope that assertion to those
interval fields — the distance scalars are position-derived, so their
appearing only under `--replay` is the contract, not a violation of it.

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

**The rules above are in force as of v0.3.5.** They were suspended while 1.0
had no external consumer — the ei-json adapter was its only reader, and it
is in-tree, so a shape that turned out wrong got fixed rather than carried.
That licence was explicitly written to end the moment an outside consumer
appeared. It has: **axibridge** reads `axilog`, `encounter`, `entities` and
`coverage` off the native document in production as of its unit-2 cutover,
with more blocks landing per unit. 1.0 is therefore **frozen**: from here a
rename, a removal, a retype or a meaning change needs a major bump, and the
key-set golden (`crates/axilog-schema/tests/v1-keyset.golden.txt`) is the
gate that catches one.

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

- Phase B final-fix round: `axilog_core::analysis::per_target::PerTargetOffense`
  lost its four `applied_*` crowd-control fields. They were never written
  anywhere — the CC per-target accumulation has always lived on
  `PlayerMetrics::cc_per_target`/`cc_downs_contribution_per_target` — so a
  core-API consumer reading them got a silent `0`. **No native-format key
  moved**: `blocks.damage.by_entity.<id>.per_target.<id>.detail` still
  carries all four, filled by the schema-layer join, and the key-set golden
  is unchanged. This bullet records a *core Rust API* break, not a wire
  break.

- Phase B final-fix round: `ReplayTrackOut::dc_intervals` gained
  `#[serde(skip)]`, so the **legacy** embedded HTML-report JSON no longer
  carries it (matching `agent_addr`/`dist_to_com`/`stack_dist` on the same
  struct). `report.js` never read it. Again **no native-format key moved** —
  `blocks.replay.tracks.by_entity.<id>.dc_intervals` and
  `blocks.replay.by_entity.<id>.dc` are both untouched.

  The 16 new native fields on `PerTargetDetail` come from **two** core
  sources, joined side by side by `PerTargetStatsOut` at the schema layer.
  Twelve mirror `axilog_core::analysis::per_target::PerTargetOffense`
  field-for-field — `direct_count`, `direct_damage`, `crit_count`,
  `crit_damage`, `flank_count`, `glance_count`, `critable_direct_count`,
  `against_downed_damage`, `missed`, `evaded`, `blocked`, `invulned` — and
  the four `applied_*` crowd-control fields (`applied_total`,
  `applied_duration_ms`, `applied_downs_contribution`,
  `applied_duration_downs_contribution_ms`) come instead from
  `PlayerMetrics::cc_per_target` and
  `PlayerMetrics::cc_downs_contribution_per_target`, because CC rows are
  dispatched by a different predicate (`cc::is_cc`) than the damage scan.
  `PerTargetOffense` deliberately carries no `applied_*` fields; do the same
  join if you read the core model directly. Of those, `critable_direct_count`
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

- Replay eye candy (arcdps-dev-notes #6/#8): `blocks.replay` gained four
  optional arrays — `gliding`, `transformations`, `captures`,
  `decorations`. **Purely additive on the wire**; the key-set golden moved
  by five entries, all under `blocks.replay.gliding`, because the committed
  fixture is the only family it can exercise (see below). Each array is
  omitted rather than emitted empty.

  Three things a consumer should know about this group:

  1. **It rides neither replay gate.** The four arrays are computed on
     every parse, like `blocks.replay.by_entity` and unlike
     `blocks.replay.tracks`. `coverage.replay` still answers the intervals
     question and says nothing about these — check the arrays.
  2. **`gliding`/`transformations` are original axilog output, not
     parity.** GW2EI parses both families and has no consumer for either
     (`GetGliderEvents` / `GetTransformationEvents` are fetched by nothing),
     so there is no EI field these correspond to and the ei-json layer emits
     nothing for them.
  3. **`captures`/`decorations` need arcdps build `20260602` or newer.**
     `CBTS_GADGETCAPTURE*` does not exist before it, so both arrays are
     absent on every older log — including the committed fixture, which is
     why their shape is pinned by
     `crates/axilog-schema/tests/v1_replay_extras.rs` rather than by a
     golden diff.

  `decorations` is the renderable projection of `captures` and is carried
  alongside it rather than instead of it: a decoration has lost which wrbg
  owner index it came from (it carries an `rgba` string, and `unknown_<n>`
  folds to white in the palette), while a capture has no lifespan
  resolution, anchor, or anchor-relative geometry. Neither reconstructs the
  other.

  `DecorationOut.start_ms`/`end_ms` are **signed**, uniquely in this format:
  the capture-progress splitter synthesizes a sample at `time - 1`, which is
  `-1` for a transition landing on log-relative 0.

- Same change, **core Rust API only**: `axilog_schema::v1::Passes` gained a
  `replay_extras` field. `Passes` is `Default`-able, so `..Default::default()`
  call sites are unaffected; an exhaustive struct initializer needs the new
  field. **No native-format key moved** by this.

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
