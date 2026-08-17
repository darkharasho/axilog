# @axiapps/axilog

Node bindings ([napi-rs](https://napi.rs)) for axilog's Rust GW2 arcdps WvW
parsing core. Every export is a thin wrapper around the same
decode → resolve → analyze → build_report pipeline the CLI
(`axilog-cli`) drives — see `src/lib.rs`'s module doc for the exact FFI
contract (no Rust panic ever crosses the boundary; every fallible step
becomes a `napi::Error` carrying the underlying Rust error text).

## Usage

```js
// CommonJS
const { parseFile, parseBuffer, parseFileEi, anonymizeFile } = require('@axiapps/axilog')
```

```ts
// ESM / TypeScript
import { parseFile, parseBuffer, parseFileEi, anonymizeFile } from '@axiapps/axilog'
import type { ReportV1 } from '@axiapps/axilog/types'
```

`parseFile`/`parseBuffer` return the **native 1.0 container** (`schema:
"1.0"`). Its six top-level keys are always present:

| Key | What it is |
| --- | --- |
| `axilog` | `{ schema, version, generated_from }` — format contract version, the binary that produced the document, and the input log's file *name* |
| `encounter` | One object: `kind`, `map`, `duration_ms`, `teams[]`, `markers[]`, `objectives[]`, `recorded_by`, … |
| `entities` | The single roster — every tracked agent, identity only, no statistics |
| `catalogs` | `skills`/`buffs`/`damage_mods`/`minions` metadata, referenced by id from `blocks` |
| `blocks` | Id-keyed statistics maps — `damage`, `defenses`, `boons`, `support`, … |
| `coverage` | Per-block status: `present`, `empty`, `not_computed` or `unsupported` |

A seventh key, `warnings`, appears only when non-empty.

There is **no top-level `players[]` and no `schema_version`**. The roster is
`entities[]`, and every per-entity statistic is a `blocks.<name>.by_entity`
map keyed by `entities[].id`. Those keys are JSON object keys, so they are
strings — `by_entity[entity.id]` happens to work in JS because the numeric
id coerces on lookup, but `Object.keys(by_entity)` gives you strings, so
compare accordingly.

```js
const { parseFile } = require('@axiapps/axilog')

const report = parseFile('./fight.zevtc')

console.log(report.axilog)
// { generated_from: 'wvw-small.anon.zevtc', schema: '1.0', version: '0.3.2' }

console.log(report.encounter.map, report.encounter.duration_ms)
// Green Alpine Borderlands 49285

// The roster is filtered by `role`, not by a separate list. WvW logs carry
// four: 'squad', 'friendly_player', 'enemy_player' and 'npc'.
const squad = report.entities.filter((e) => e.role === 'squad')
const damage = report.blocks.damage
console.log(squad.length, 'squad of', report.entities.length, '| squad damage', damage.squad.total)
// 38 squad of 122 | squad damage 2138414

const top = squad
  .map((e) => [e, damage.by_entity[e.id]])
  .sort((a, b) => b[1].total - a[1].total)[0]
console.log(top[0].account, top[0].profession, top[1].total, top[1].dps)
// :Anon104.4848 Engineer 205612 4171.898143451354

// Same pipeline, from an already-read Buffer.
const fs = require('fs')
const reportFromBuffer = parseBuffer(fs.readFileSync('./fight.zevtc'))
```

Buff and skill names live once in `catalogs`, never repeated per entity —
`blocks.boons.by_entity[id]` is keyed by buff id, and
`catalogs.buffs['1187'].name` resolves it to `Quickness`.

### Check `coverage` before reading a block

`coverage` is how a consumer tells "the option was off" apart from "the log
had nothing to say", which is otherwise indistinguishable — both look like
a missing block.

```js
console.log(report.coverage.rotation, report.coverage.damage_mods)
// empty not_computed
```

| Value | Meaning | What to do |
| --- | --- | --- |
| `present` | Computed, has rows | Read it |
| `empty` | Computed, genuinely nothing to report | Treat as zero rows, not an error. The block is still carried in `blocks` |
| `not_computed` | The gate for it was off | Re-parse with the option on. Do **not** report this as "the log had none" |
| `unsupported` | This log cannot produce it — its era, encounter kind, or recorder | Do not retry with an option; see `docs/EI-PARITY.md` |

### Opt-in options

`ParseOptions` is an optional second argument to `parseFile`, `parseBuffer`
and `parseFileEi`. Seven booleans, all `false` by default, in **camelCase**:

| Option | Adds |
| --- | --- |
| `skillDamage` | `blocks.damage.by_entity[].by_skill` / `.by_skill_taken` — per-skill outgoing and incoming splits |
| `timeseries` | `blocks.series` per-entity channels, and the buff stack timelines in `blocks.boons`/`blocks.conditions` |
| `rotation` | `blocks.rotation.by_entity[].{casts, aftercast}` |
| `replay` | `blocks.replay.tracks` — position samples. The down/dead intervals under `blocks.replay.by_entity` are computed on **every** parse; only the positions are gated |
| `missiles` | `blocks.missiles` — projectile fired/hit/denied counts |
| `modifiers` | `blocks.damage_mods` and `catalogs.damage_mods` |
| `everything` | Every analysis pass **this version** knows about |

```js
const full = parseFile('./fight.zevtc', { skillDamage: true, rotation: true })

console.log(Object.keys(full.blocks.damage.by_entity['1']).sort())
// [ 'breakbar_damage_dealt', 'by_skill', 'by_skill_taken', 'downs_dealt',
//   'dps', 'kills_dealt', 'per_target', 'taken', 'total' ]
```

Note the asymmetry: the *options* object is camelCase (`skillDamage`),
while the *report* keys are the schema's own snake_case (`skill_damage`,
`by_skill_taken`). The options are a napi-generated JS surface; the report
is the native JSON verbatim.

Prefer `everything: true` over enumerating options if you want complete
documents: it is defined as "everything that exists in this version", so a
pass added by a later release is included automatically. The first axibridge
cutover audit found 30 blank fields caused by exactly the opposite — a
consumer's option list drifting from the parser's. It is a union with the
individual options, never an override.

Options are not free, and they are not uniformly the same kind of cost.
`skillDamage`, `timeseries` and `rotation` only control whether an
already-computed result is serialized; `replay` and `modifiers` gate the
computation itself. Measured on the committed fixture
(`fixtures/wvw-small.anon.zevtc` — 38 squad entities, 49.3 s — release
build, compact JSON, 461,086-byte baseline at 0.07 s):

| Option | Bytes | Wall |
| --- | --- | --- |
| *(none)* | 461,086 | 0.07 s |
| `missiles` | 466,565 | 0.07 s |
| `rotation` | 708,367 | 0.07 s |
| `modifiers` | 616,591 | 0.08 s |
| `replay` | 1,679,284 | 0.12 s |
| `skillDamage` | 3,338,819 | 0.08 s |
| `timeseries` | 3,563,403 | 0.09 s |
| `everything` | 8,067,599 | 0.16 s |

That fixture is a 49-second skirmish. The per-skill and per-second blocks
are combinatorial — entity × target × skill and entity × target × second —
so on a real multi-minute zerg log they grow far faster than the table
above suggests. See `docs/BENCHMARKS.md` for real-log numbers.

### The other two functions

```js
// Elite Insights-compatibility JSON (axilog_ei::to_ei_json) — the shape
// axibridge-style consumers read (players[].account, dpsAll[0].damage,
// support[0].condiCleanse, buffUptimes[], targets[].enemyPlayer,
// wvWMapData.{red,blue,green}TeamID). Untyped (`any`) — see "Types" below.
// Unlike the native container this one does still have a flat `players[]`,
// because EI's shape does. It takes the same ParseOptions, except
// `missiles`, which has no EI equivalent and is ignored.
const ei = parseFileEi('./fight.zevtc')

// Rewrite every player's character/account name in a .zevtc to a
// deterministic Anon<N> placeholder (does not mutate metrics — safe for
// producing PII-safe fixtures). Returns the number of player agents
// rewritten.
const rewritten = anonymizeFile('./fight.zevtc', './fight.anon.zevtc')
```

All four functions throw a plain `Error` (never a raw Rust panic) on
failure — e.g. a missing file, a corrupt/unsupported `.evtc`.

### Further reference

`docs/NATIVE-FORMAT.md` in the repo root is the authoritative field-level
reference for the 1.0 container — the `entities[].role` rules, the catalog
join semantics, the series RLE envelope, the two replay halves, and the 1.x
compatibility rules a consumer can rely on. This README is the SDK surface;
that document is the format.

## Build

```sh
npm install
npm run build        # napi build --platform --release, then patches index.d.ts (see "Types")
npm run build:debug  # same, unoptimized (faster iteration)
npm test              # node --test __test__/*.test.mjs
```

`npm run build`/`npm run build:debug` compile the Rust crate
(`crates/axilog-node`) to a platform-specific `.node` addon (e.g.
`axilog.linux-x64-gnu.node`) and regenerate `index.js`/`index.d.ts` via
`@napi-rs/cli`.

## Tests

`__test__/sdk.test.mjs` (`node --test`, Node >= 18) covers, against the
committed PII-safe fixture `fixtures/wvw-small.anon.zevtc` (repo-root
relative, resolved from the test file's own location so it works from any
cwd):

- `parseFile`: the six-key container shape, player count, squad damage
  total, one boon uptime value, and the four support-stat squad sums — all
  pinned to this fixture's exact calibrated values (see
  `crates/axilog-core/tests/golden.rs`/`support_golden.rs` for how those
  numbers were derived against a real dps.report EI export).
- `parseBuffer` deep-equals `parseFile`.
- `replay`, `skillDamage`, `timeseries`, `missiles` and `modifiers`: absent
  by default, present and correctly shaped when requested, and absent again
  when passed explicitly `false`. `replay`'s test additionally pins the
  split halves — intervals always on, `tracks` gated. **`rotation` has no
  such test** — in either SDK. Its shape and size are guarded schema-side
  (`crates/axilog-schema/tests/v1_shape.rs`'s keyset golden and
  `v1_size.rs`, both of which drive every gate), and the option is
  exercised here through `{ everything: true }`, but there is no
  absent-by-default/present-when-requested assertion for it at the SDK
  boundary. Known gap.
- `{ everything: true }`: every gate computes, leaving nothing
  `not_computed` in `coverage`.
- `parseFileEi`: the specific keys axibridge-style consumers read
  (`players[].account`, `dpsAll[0].damage`, `support[0].condiCleanse`,
  non-empty `buffUptimes`, `targets[].enemyPlayer` booleans,
  `wvWMapData`'s three team ids).
- `anonymizeFile` round-trip: writes to a tmpdir, re-parses, checks entity
  count/metrics are unchanged (anonymization only rewrites names).
- Missing-file error path: throws a real `Error` with a non-empty message.
- **Dual-path parity**: builds nothing itself, but shells out to the
  already-built CLI (`cargo build -p axilog-cli`; run that first if
  `target/debug/axilog` is missing) and asserts
  `axilog parse <fixture> --format json` parsed, deep-equals `parseFile`'s
  return value — with a small first-N-differing-paths diff helper so a
  future divergence prints something readable instead of two giant JSON
  blobs.

## Types

`index.d.ts` is generated by `@napi-rs/cli` from `src/lib.rs`'s exported
function signatures. Because every export returns a plain
`serde_json::Value` (not a `#[napi(object)]`-derived struct — see
`src/lib.rs`'s module doc for why), napi's typegen has no visibility into
the actual JSON shape and can only emit `any` for `parseFile`/
`parseBuffer`'s return type.

To close that gap without hand-editing a generated file (which `napi
build` would silently overwrite), this crate adds:

- **`types.d.ts`** — a hand-maintained transcription of the serialized
  shape. The 1.0 container's interfaces (`ReportV1`, `AxilogMeta`,
  `EncounterOutV1`, `EntityOut`, `Catalogs`, `Coverage`, `SeriesOut` and
  friends) are transcribed from `crates/axilog-schema/src/v1/`, which is the
  source of truth — keep this file in sync with it if the schema changes.
  It mirrors `crates/axilog-py/axilog.pyi`'s equivalent transcription for
  the Python SDK.
- **`scripts/patch-dts.mjs`** — a small idempotent postbuild step (wired
  into both `npm run build` and `npm run build:debug`) that patches the
  freshly-regenerated `index.d.ts` to import the report type from
  `./types` and retypes `parseFile`/`parseBuffer`'s return type from
  `any`. Because it runs as part of the build script itself (not a one-off
  manual edit), the reference survives every future `napi build`
  regeneration instead of needing to be reapplied by hand.

`parseFileEi`'s return value is a materially different, larger EI-JSON
shape (not a serialized native container) and is intentionally left `any` —
typing it faithfully is out of scope (see `crates/axilog-ei/src/lib.rs`'s
doc comments for what fields it does/doesn't carry).
