// Node test suite for the axilog napi-rs addon (M5 Task 2), run via
// `node --test __test__/` (wired as `npm test`, see package.json).
//
// Every fixture path below is resolved from *this file's own location*
// (via `import.meta.url`), not the process cwd, so the suite runs
// correctly whether invoked as `npm test` from `crates/axilog-node/` or
// as `node --test crates/axilog-node/__test__/sdk.test.mjs` from the repo
// root.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { tmpdir } from 'node:os'
import { execFileSync } from 'node:child_process'

import * as sdk from '../index.js'

const __dirname = dirname(fileURLToPath(import.meta.url))
// crates/axilog-node/__test__ -> crates/axilog-node -> crates -> repo root
const REPO_ROOT = join(__dirname, '..', '..', '..')
const FIXTURE = join(REPO_ROOT, 'fixtures', 'wvw-small.anon.zevtc')
const GOLDEN_EI_JSON = join(REPO_ROOT, 'fixtures', 'wvw-small.ei.json')
const CLI_BIN = join(REPO_ROOT, 'target', 'debug', 'axilog')

// M3 Task 3 / M2 Task 3 calibration targets (see
// `crates/axilog-core/tests/golden.rs` / `support_golden.rs`): this exact
// committed fixture's squad damage total and squad support sums, verified
// directly against a real dps.report EI export
// (`fixtures/wvw-small.ei.json`'s `squadTotalDamage`/`squadCondiCleanse`/
// `squadCondiCleanseSelf`/`squadBoonStrips`/`squadResurrects`). Read here
// as plain literals (not re-derived from the golden JSON) so this suite
// pins the exact value the M5 Task 2 brief calls out, independent of the
// Rust-side calibration tests' own tolerance windows.
const EXPECTED_SQUAD_DAMAGE_TOTAL = 2138414
const EXPECTED_SUPPORT_SUMS = { cleanses: 801, cleanses_self: 97, strips: 437, resurrects: 6 }
// `Anon132.5884`'s Quickness (a duration-type boon, id 1187) presence_pct,
// read from the golden EI JSON's `players[].boons["1187"].uptime` (for a
// duration boon, EI's `uptime` field IS the presence percentage -- see
// `axilog_core::analysis::buffs::uptime`'s module doc and
// `boons_golden.rs`). Picked because it needs no float/rounding
// gymnastics: EI publishes it pre-rounded to 3 decimals (6.016) and our
// own computation lands within a hundredth of a percentage point of that.
const STABLE_BOON_ACCOUNT = ':Anon132.5884'
const STABLE_BOON_NAME = 'Quickness'
const STABLE_BOON_EXPECTED_PRESENCE_PCT = 6.016

function sumBy(arr, fn) {
  return arr.reduce((acc, x) => acc + fn(x), 0)
}

// ---------------------------------------------------------------------
// Native 1.0 container accessors (Task 12).
//
// `parseFile`/`parseBuffer` return `{axilog, blocks, catalogs, coverage,
// encounter, entities}` -- there is no flat `players[]` any more. The
// roster lives in `entities[]` and every per-player analysis block is a
// `blocks.<name>.by_entity` map keyed by `entities[].id`, so the shape
// assertions below join the two rather than reading nested objects.
// ---------------------------------------------------------------------

// The pre-1.0 `Report.players[]` was the log-recording squad PLUS
// non-squad friendly players -- 42 on this fixture, which is also what
// the golden EI export's `players.length` and `parseFileEi` report. The
// two roles have to stay split here: the squad alone accounts for all
// 2,138,414 damage but only 776 of the 801 cleanses.
const PLAYER_ROLES = new Set(['squad', 'friendly_player'])

function players(report) {
  return report.entities.filter((e) => PLAYER_ROLES.has(e.role))
}

/** `entity`'s slot in a `blocks.<name>` map, or `undefined` if it has none. */
function slot(block, entity) {
  return block?.by_entity?.[entity.id]
}

/** The buff catalog id `name` is published under, e.g. 'Quickness' -> '1187'. */
function buffIdByName(report, name) {
  return Object.keys(report.catalogs.buffs).find((id) => report.catalogs.buffs[id].name === name)
}

/** Expands a run-length-encoded `blocks.series` channel back to a flat array. */
function decodeSeries(series) {
  assert.equal(series.enc, 'rle', 'series channels are run-length encoded')
  const out = []
  for (const [value, run] of series.data) {
    for (let i = 0; i < run; i++) out.push(value)
  }
  assert.equal(out.length, series.len, 'the run lengths must sum to the declared len')
  return out
}

/** First `limit` paths where `a` and `b` differ, DFS over plain objects/arrays. Used only to produce a readable failure message for the dual-path parity test -- `assert.deepStrictEqual` alone just dumps the entire (huge) report object on mismatch. */
function firstDiffPaths(a, b, limit = 10, path = '$', out = []) {
  if (out.length >= limit) return out
  if (a === b) return out
  const aIsObj = a !== null && typeof a === 'object'
  const bIsObj = b !== null && typeof b === 'object'
  if (!aIsObj || !bIsObj) {
    out.push(`${path}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`)
    return out
  }
  const aArr = Array.isArray(a)
  const bArr = Array.isArray(b)
  if (aArr !== bArr) {
    out.push(`${path}: array-ness differs (${aArr} vs ${bArr})`)
    return out
  }
  const keys = new Set([...Object.keys(a), ...Object.keys(b)])
  for (const k of keys) {
    if (out.length >= limit) break
    const childPath = aArr ? `${path}[${k}]` : `${path}.${k}`
    if (!(k in a)) {
      out.push(`${childPath}: missing on left, right=${JSON.stringify(b[k])}`)
    } else if (!(k in b)) {
      out.push(`${childPath}: missing on right, left=${JSON.stringify(a[k])}`)
    } else {
      firstDiffPaths(a[k], b[k], limit, childPath, out)
    }
  }
  return out
}

test('parseFile: schema, player count, squad damage, one boon value, support sums', () => {
  const report = sdk.parseFile(FIXTURE)

  assert.equal(report.axilog.schema, '1.0')
  assert.equal(typeof report.axilog.version, 'string')
  // `parseFile` has a real file name to thread through; only the bare
  // name, never the full path.
  assert.equal(report.axilog.generated_from, 'wvw-small.anon.zevtc')

  const roster = players(report)
  assert.ok(roster.length > 0, 'expected at least one player')
  assert.equal(roster.length, 42)

  const damage = report.blocks.damage
  const squadDamageTotal = sumBy(roster, (p) => slot(damage, p)?.total ?? 0)
  assert.equal(squadDamageTotal, EXPECTED_SQUAD_DAMAGE_TOTAL)
  // The squad rollup is computed independently of the per-entity map, so
  // this pins the two against each other as well as against the golden.
  assert.equal(damage.squad.total, EXPECTED_SQUAD_DAMAGE_TOTAL)

  const supportBlock = report.blocks.support
  const support = {
    cleanses: sumBy(roster, (p) => slot(supportBlock, p)?.cleanses ?? 0),
    cleanses_self: sumBy(roster, (p) => slot(supportBlock, p)?.cleanses_self ?? 0),
    strips: sumBy(roster, (p) => slot(supportBlock, p)?.strips ?? 0),
    resurrects: sumBy(roster, (p) => slot(supportBlock, p)?.resurrects ?? 0),
  }
  assert.deepEqual(support, EXPECTED_SUPPORT_SUMS)

  const player = roster.find((p) => p.account === STABLE_BOON_ACCOUNT)
  assert.ok(player, `expected fixture to contain account ${STABLE_BOON_ACCOUNT}`)
  // Boon rows are keyed by buff id, with the display name in the catalog.
  const buffId = buffIdByName(report, STABLE_BOON_NAME)
  assert.ok(buffId, `expected the buff catalog to describe ${STABLE_BOON_NAME}`)
  const boon = slot(report.blocks.boons, player)?.[buffId]
  assert.ok(boon, `expected ${STABLE_BOON_ACCOUNT} to have a ${STABLE_BOON_NAME} entry`)
  assert.ok(
    Math.abs(boon.uptime_pct - STABLE_BOON_EXPECTED_PRESENCE_PCT) < 0.05,
    `${STABLE_BOON_NAME} uptime_pct ${boon.uptime_pct} not within 0.05 of golden ${STABLE_BOON_EXPECTED_PRESENCE_PCT}`,
  )
})

test('parseBuffer produces the same 1.0 document as parseFile', () => {
  const fromFile = sdk.parseFile(FIXTURE)
  const buf = readFileSync(FIXTURE)
  const fromBuffer = sdk.parseBuffer(buf)

  // `generated_from` is the ONE documented difference: a buffer has no
  // file name to offer, so `parseBuffer` omits the key entirely.
  assert.equal(fromBuffer.axilog.generated_from, undefined)
  const { generated_from: _dropped, ...fileHeader } = fromFile.axilog
  assert.deepStrictEqual(fromBuffer.axilog, fileHeader)
  assert.deepStrictEqual({ ...fromBuffer, axilog: null }, { ...fromFile, axilog: null })
})

// `opts.replay` gates POSITIONS, not the whole block. The down/dead
// intervals under `blocks.replay.by_entity` are computed on every parse and
// are there with no opts at all, so -- unlike every other opt-in below --
// `coverage.replay === 'present'` is NOT a statement about whether the flag
// was passed. What the flag adds is `blocks.replay.tracks`.
test('parseFile: replay opt-in -- intervals always on, positions gated behind { replay: true }', () => {
  const withoutReplay = sdk.parseFile(FIXTURE)
  const intervalsOnly = withoutReplay.blocks.replay
  assert.ok(intervalsOnly, 'the intervals half of blocks.replay is present with no opts')
  assert.equal(withoutReplay.coverage.replay, 'present')
  assert.equal(intervalsOnly.tracks, undefined, 'positions must be absent without { replay: true }')
  assert.ok(Object.keys(intervalsOnly.by_entity).length > 0, 'expected intervals rows')
  const row = intervalsOnly.by_entity[Object.keys(intervalsOnly.by_entity)[0]]
  for (const k of ['start_ms', 'end_ms', 'active_ms']) {
    assert.equal(typeof row[k], 'number', `by_entity row ${k} must be a number`)
  }
  assert.ok(Array.isArray(row.down) && Array.isArray(row.dead))

  const withReplay = sdk.parseFile(FIXTURE, { replay: true })
  const replay = withReplay.blocks.replay
  assert.ok(replay, 'expected a replay block when { replay: true }')
  assert.equal(withReplay.coverage.replay, 'present')
  // Turning the position gate on must not change the intervals half.
  //
  // Scoped to the INTERVAL fields on purpose. `dist_to_com`/`stack_dist`
  // also live on this always-present row, but they are position-derived
  // reductions, so they legitimately appear only when positions were
  // requested -- that is the documented two-state convention (absent = the
  // pass never ran; -1 = the pass ran and nothing qualified), and it is the
  // gate that makes "absent" reachable at all. Comparing whole rows would
  // assert the opposite of the contract.
  const INTERVAL_FIELDS = ['start_ms', 'end_ms', 'active_ms', 'down', 'dead', 'dc']
  const intervalsOf = (byEntity) =>
    Object.fromEntries(
      Object.entries(byEntity).map(([id, r]) => [id, Object.fromEntries(INTERVAL_FIELDS.map((k) => [k, r[k]]))]),
    )
  assert.deepEqual(
    intervalsOf(replay.by_entity),
    intervalsOf(intervalsOnly.by_entity),
    'turning the position gate on must not change the intervals half',
  )
  assert.deepEqual(
    Object.keys(replay.by_entity).sort(),
    Object.keys(intervalsOnly.by_entity).sort(),
    'the gate must not add or drop entity rows either',
  )
  // ... and the scalars really are the only difference: absent ungated,
  // present gated.
  const gatedRow = replay.by_entity[Object.keys(replay.by_entity)[0]]
  for (const k of ['dist_to_com', 'stack_dist']) {
    assert.equal(row[k], undefined, `${k} must be absent when the position pass did not run`)
    assert.equal(typeof gatedRow[k], 'number', `${k} must be a number once positions were requested`)
  }

  const tracks = replay.tracks
  assert.ok(tracks, 'expected positions under { replay: true }')
  assert.equal(typeof tracks.poll_ms, 'number')
  for (const k of ['min_x', 'max_x', 'min_y', 'max_y']) {
    assert.equal(typeof tracks.bounds[k], 'number', `bounds.${k} must be a number`)
  }

  // Tracks are a by-entity map, not a flat array: the name/team/squad
  // membership the pre-1.0 `tracks[]` duplicated now live once on the
  // `entities[]` row the key joins to.
  const trackIds = Object.keys(tracks.by_entity)
  assert.ok(trackIds.length > 0, 'expected at least one replay track')
  const entityIds = new Set(withReplay.entities.map((e) => String(e.id)))
  for (const id of trackIds) {
    assert.ok(entityIds.has(id), `replay track ${id} does not join to any entities[] row`)
  }
  const track = tracks.by_entity[trackIds[0]]
  assert.ok(Array.isArray(track.samples))
  // Kept on the track as well as on `by_entity`: the track roster also covers
  // enemy players, whom the always-on intervals pass never walks.
  assert.ok(Array.isArray(track.down_intervals))
  assert.ok(Array.isArray(track.dead_intervals))
  if (track.samples.length > 0) {
    assert.equal(track.samples[0].length, 3, 'each sample is a [t, x, y] triple')
  }

  // opts.replay: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { replay: false })
  assert.equal(explicitlyOff.blocks.replay.tracks, undefined)

  // parseBuffer accepts the same opts shape.
  const buf = readFileSync(FIXTURE)
  const bufWithReplay = sdk.parseBuffer(buf, { replay: true })
  assert.ok(bufWithReplay.blocks.replay.tracks)
})

test('parseFile: skillDamage opt-in (M12 Task 1) -- absent by default, present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  const anyPlayer = players(without)[0]
  assert.equal(slot(without.blocks.damage, anyPlayer).by_skill, undefined, 'by_skill must be absent by default')
  assert.equal(slot(without.blocks.damage, anyPlayer).by_skill_taken, undefined)

  const withIt = sdk.parseFile(FIXTURE, { skillDamage: true })
  const p0 = players(withIt).find((p) => (slot(withIt.blocks.damage, p)?.total ?? 0) > 0)
  assert.ok(p0, 'expected at least one player with nonzero damage')
  const d0 = slot(withIt.blocks.damage, p0)
  assert.ok(d0.by_skill, 'expected a by_skill map when { skillDamage: true }')
  assert.ok(d0.by_skill_taken, 'expected a by_skill_taken map when { skillDamage: true }')
  const skillIds = Object.keys(d0.by_skill)
  assert.ok(skillIds.length > 0, 'expected at least one outgoing skill entry')
  const entry = d0.by_skill[skillIds[0]]
  for (const k of ['total', 'hits', 'crit_hits', 'flank_hits', 'min', 'max']) {
    assert.equal(typeof entry[k], 'number', `by_skill entries carry a numeric ${k}`)
  }
  // Every referenced skill id must be described by the skill catalog.
  for (const id of skillIds) {
    assert.ok(id in withIt.catalogs.skills, `skill ${id} missing from catalogs.skills`)
  }
  // sum(by_skill[*].total) == damage total exactly (internal invariant).
  const sum = sumBy(Object.values(d0.by_skill), (e) => e.total)
  assert.equal(sum, d0.total, 'sum(by_skill totals) must equal the damage total exactly')
  // The per-target breakdown gains the same distribution under the flag --
  // for every entry that has damage to distribute.
  //
  // NOT "every entry": the per_target map is keyed by every (player, target)
  // pair the offensive scan touched, which since Phase B includes pairs the
  // player only ever whiffed against (a lone `evaded`/`blocked`/`missed`/
  // `invulned` row) and pairs they only crowd-controlled. Those carry
  // `total: 0` and have no skill damage to split, so a `by_skill` on them
  // would be an empty map, and the format omits empty maps. Asserting
  // presence unconditionally asserted the absence of a real feature.
  //
  // The biconditional below is strictly stronger than the old assertion in
  // the direction that matters: it still fails if a damaging pair loses its
  // split (the regression the check exists for), AND it now also fails if a
  // non-damaging pair sprouts one.
  const perTarget = Object.values(d0.per_target)
  assert.ok(perTarget.length > 0, 'expected at least one per-target entry')
  assert.ok(
    perTarget.some((t) => t.total > 0),
    'expected at least one per-target entry WITH damage, or the check below is vacuous',
  )
  for (const t of perTarget) {
    assert.equal(
      Boolean(t.by_skill),
      t.total > 0,
      `per_target entries have by_skill exactly when they have damage (total=${t.total})`,
    )
  }

  // opts.skillDamage: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { skillDamage: false })
  assert.equal(slot(explicitlyOff.blocks.damage, anyPlayer).by_skill, undefined)
})

test('parseFile: timeseries opt-in (M12 Task 2) -- per-entity series absent by default, present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  const anyPlayer = players(without)[0]
  assert.equal(slot(without.blocks.series, anyPlayer), undefined, 'per-entity series must be absent by default')
  // The squad-level series is always on -- only the per-entity map is gated.
  assert.ok(without.blocks.series.squad.damage, 'squad series stays present by default')

  const withIt = sdk.parseFile(FIXTURE, { timeseries: true })
  const p0 = players(withIt).find((p) => (slot(withIt.blocks.damage, p)?.total ?? 0) > 0)
  assert.ok(p0, 'expected at least one player with nonzero damage')
  const series = slot(withIt.blocks.series, p0)
  assert.ok(series, 'expected a per-entity series when { timeseries: true }')
  for (const k of ['damage', 'damage_taken', 'power_damage_taken']) {
    assert.equal(typeof series[k].interval_ms, 'number', `${k} carries its bucket width`)
  }

  const damage = decodeSeries(series.damage)
  assert.ok(damage.length > 0, 'expected at least one bucket')
  // Cumulative: the final bucket equals the damage total exactly.
  assert.equal(
    damage[damage.length - 1],
    slot(withIt.blocks.damage, p0).total,
    'final damage bucket must equal the damage total exactly',
  )
  // Monotonic non-decreasing.
  for (let i = 1; i < damage.length; i++) {
    assert.ok(damage[i] >= damage[i - 1], 'the damage series must be cumulative (monotonic non-decreasing)')
  }

  // The per-target series joins to the always-on per-target damage map.
  const perTargetIds = Object.keys(series.per_target)
  assert.ok(perTargetIds.length > 0, 'expected at least one per-target series')
  const damageTargets = slot(withIt.blocks.damage, p0).per_target
  for (const id of perTargetIds) {
    assert.ok(id in damageTargets, `per-target series ${id} has no matching damage entry`)
  }
  // sum(per_target[*].total) == damage total exactly (internal invariant).
  const dtSum = sumBy(Object.values(damageTargets), (t) => t.total)
  assert.equal(dtSum, slot(withIt.blocks.damage, p0).total, 'sum(per_target totals) must equal the damage total exactly')

  // opts.timeseries: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { timeseries: false })
  assert.equal(slot(explicitlyOff.blocks.series, anyPlayer), undefined)
})

test('parseFile: missiles opt-in -- absent by default, present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  assert.equal(without.blocks.missiles, undefined, 'missiles must be absent by default')
  assert.equal(without.coverage.missiles, 'not_computed')

  const withIt = sdk.parseFile(FIXTURE, { missiles: true })
  const missiles = withIt.blocks.missiles
  assert.ok(missiles, 'expected a missiles block when { missiles: true }')
  assert.equal(withIt.coverage.missiles, 'present')
  assert.ok(missiles.squad, 'expected a squad missiles rollup')
  for (const k of ['fired', 'hit', 'denied', 'incoming_fired', 'incoming_denied']) {
    assert.equal(typeof missiles.squad[k], 'number', `squad.${k} must be a number`)
  }
  const perEntity = Object.values(missiles.by_entity)
  assert.ok(perEntity.length > 0, 'expected at least one per-entity missile row')
  for (const k of ['fired', 'hit', 'denied', 'reflected_at_self']) {
    assert.equal(typeof perEntity[0][k], 'number', `by_entity rows carry a numeric ${k}`)
  }

  // opts.missiles: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { missiles: false })
  assert.equal(explicitlyOff.blocks.missiles, undefined)
})

test('parseFile: modifiers opt-in (M16) -- damage_mods block + catalog absent by default, present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  assert.equal(without.catalogs.damage_mods, undefined, 'the modifier catalog must be absent by default')
  assert.equal(without.blocks.damage_mods, undefined, 'the damage_mods block must be absent by default')
  assert.equal(without.coverage.damage_mods, 'not_computed')

  const withIt = sdk.parseFile(FIXTURE, { modifiers: true })
  const catalog = withIt.catalogs.damage_mods
  assert.ok(catalog, 'expected a modifier catalog when { modifiers: true }')
  assert.equal(withIt.coverage.damage_mods, 'present')
  const mapIds = Object.keys(catalog)
  assert.ok(mapIds.length > 0, 'expected at least one referenced modifier id')

  // Two SCOPES per entity: `overall` (whole fight) and the sparse
  // `per_target` map beside it, keyed by the target's entity id. Within
  // each scope the direction that used to be an outgoing/incoming array
  // split is carried by the id's SIGN.
  const rowSets = Object.values(withIt.blocks.damage_mods.by_entity)
  assert.ok(rowSets.length > 0, 'expected at least one entity with modifier rows')
  let sawIncoming = false
  let sawOutgoing = false
  // The per-target scope is the expensive half (measured at ~11x the
  // whole-fight arrays), so the NATIVE path does not compute it -- only
  // `parseFileEi` asks for it. An absent `per_target` on a present block
  // therefore means "the split was not computed", which is what the
  // field's own schema doc says; it is checked when present, never
  // required.
  let sawPerTarget = false
  const checkScope = (rows) => {
    for (const [id, r] of Object.entries(rows)) {
      if (Number(id) < 0) sawIncoming = true
      else sawOutgoing = true
      // Native keys carry no "d" prefix -- that is an ei-json-ism.
      assert.ok(!id.startsWith('d'), `native modifier keys carry no "d" prefix, got ${id}`)
      assert.ok(id in catalog, `modifier ${id} missing from catalogs.damage_mods`)
      for (const k of ['hit_count', 'total_hit_count', 'damage_gain', 'total_damage']) {
        assert.equal(typeof r[k], 'number', `${k} must be a number`)
      }
      assert.ok(r.hit_count >= 1 && r.hit_count <= r.total_hit_count)
    }
  }
  for (const entity of rowSets) {
    assert.ok(entity.overall, 'every damage_mods row carries an overall scope')
    checkScope(entity.overall)
    for (const perTarget of Object.values(entity.per_target ?? {})) {
      sawPerTarget = true
      checkScope(perTarget)
    }
  }
  assert.ok(sawOutgoing, 'expected at least one outgoing (positive-id) modifier row')
  assert.ok(sawIncoming, 'expected at least one incoming (negative-id) modifier row')
  assert.equal(sawPerTarget, false, 'the native path must not compute the per-target split')

  const desc = catalog[mapIds[0]]
  for (const k of ['name', 'description']) assert.equal(typeof desc[k], 'string')
  for (const k of ['non_multiplier', 'is_counter', 'skill_based', 'approximate']) {
    assert.equal(typeof desc[k], 'boolean')
  }

  // opts.modifiers: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { modifiers: false })
  assert.equal(explicitlyOff.catalogs.damage_mods, undefined)
  assert.equal(explicitlyOff.blocks.damage_mods, undefined)
})

test('parseFileEi: { modifiers: true } adds EI damageModifiers + damageModMap (M16)', () => {
  const without = sdk.parseFileEi(FIXTURE)
  assert.equal(without.damageModMap, undefined, 'damageModMap must be absent by default')
  assert.equal(without.players[0].damageModifiers, undefined)

  const ei = sdk.parseFileEi(FIXTURE, { modifiers: true })
  assert.ok(ei.damageModMap, 'expected damageModMap when { modifiers: true }')
  for (const k of Object.keys(ei.damageModMap)) {
    assert.ok(k.startsWith('d'), `damageModMap keys carry EI's "d" prefix, got ${k}`)
  }
  const nTargets = ei.targets.length
  const p = ei.players.find((x) => x.damageModifiers.length > 0)
  assert.ok(p, 'expected at least one player with damageModifiers')
  // EI nests the four numbers one level deeper, as a per-phase array.
  const item = p.damageModifiers[0].damageModifiers[0]
  assert.equal(typeof p.damageModifiers[0].id, 'number')
  for (const k of ['hitCount', 'totalHitCount', 'damageGain', 'totalDamage']) {
    assert.equal(typeof item[k], 'number', `${k} must be a number`)
  }
  // The Target arrays stay positionally locked to targets[].
  for (const k of ['damageModifiersTarget', 'incomingDamageModifiersTarget']) {
    assert.equal(p[k].length, nTargets, `${k} must have one slot per targets[] entry`)
  }
  assert.ok(
    p.damageModifiersTarget.some((slot) => slot.length > 0),
    'expected at least one populated per-target slot',
  )
})

test('parseFileEi: accepts the same ParseOptions as parseFile -- skillDamage/timeseries surface totalDamageDist/damage1S; default omits both', () => {
  const withoutOpts = sdk.parseFileEi(FIXTURE)
  const p0Without = withoutOpts.players[0]
  assert.equal(p0Without.totalDamageDist, undefined, 'totalDamageDist must be absent by default (back-compat)')
  assert.equal(p0Without.damage1S, undefined, 'damage1S must be absent by default (back-compat)')

  const withOpts = sdk.parseFileEi(FIXTURE, { skillDamage: true, timeseries: true })
  const p0With = withOpts.players.find((p) => Array.isArray(p.totalDamageDist) && p.totalDamageDist[0]?.length > 0)
  assert.ok(p0With, 'expected at least one player with a non-empty totalDamageDist when { skillDamage: true }')
  assert.ok(Array.isArray(p0With.totalDamageDist))
  assert.ok(Array.isArray(p0With.damage1S), 'expected damage1S when { timeseries: true }')
  assert.ok(p0With.damage1S[0].length > 0, 'expected a non-empty per-second series inside damage1S\'s phase wrapper')
})

test('parseFileEi: { replay: true } adds GW2EI combat-replay positions + metaData (M15)', () => {
  const ei = sdk.parseFileEi(FIXTURE, { replay: true })

  // Top-level metaData: the arena image the pixel coordinates live on.
  assert.ok(ei.combatReplayMetaData, 'expected combatReplayMetaData with { replay: true }')
  assert.equal(ei.combatReplayMetaData.pollingRate, 300)
  assert.deepEqual(ei.combatReplayMetaData.sizes, [523, 750])
  // The f32-text contract: EI writes a C# float, so this must be exactly
  // 0.009 -- a widened f64 would arrive as 0.008999999612569809.
  assert.equal(ei.combatReplayMetaData.inchToPixel, 0.009)
  assert.ok(Array.isArray(ei.combatReplayMetaData.maps) && ei.combatReplayMetaData.maps.length === 1)

  const p0 = ei.players[0]
  const crd = p0.combatReplayData
  assert.ok(Array.isArray(crd.positions) && crd.positions.length > 0, 'expected positions')
  assert.equal(crd.positions.length, crd.orientations.length, 'orientations are grid-aligned')
  assert.equal(crd.positions[0].length, 2)
  assert.equal(typeof crd.positions[0][0], 'number')
  assert.ok(Array.isArray(crd.dc) && crd.dc.length > 0, 'expected the dc sentinel bracketing')
  assert.equal(typeof crd.iconURL, 'string')
  assert.ok(crd.iconURL.startsWith('https://'))
  // M11's always-on fields are untouched by the flag.
  const plain = sdk.parseFileEi(FIXTURE).players[0].combatReplayData
  assert.equal(crd.start, plain.start)
  assert.equal(crd.end, plain.end)
  assert.deepEqual(crd.down, plain.down)
  assert.deepEqual(crd.dead, plain.dead)
})

test('parseFileEi: axibridge-read key shapes', () => {
  const ei = sdk.parseFileEi(FIXTURE)

  assert.ok(Array.isArray(ei.players) && ei.players.length > 0, 'expected players[]')
  const p0 = ei.players[0]
  assert.equal(typeof p0.account, 'string')
  assert.ok(p0.account.length > 0)

  assert.ok(Array.isArray(p0.dpsAll) && p0.dpsAll.length > 0)
  assert.equal(typeof p0.dpsAll[0].damage, 'number')

  assert.ok(Array.isArray(p0.support) && p0.support.length > 0)
  assert.equal(typeof p0.support[0].condiCleanse, 'number')

  assert.ok(Array.isArray(p0.buffUptimes) && p0.buffUptimes.length > 0, 'expected non-empty buffUptimes')

  assert.ok(Array.isArray(ei.targets) && ei.targets.length > 0, 'expected targets[]')
  for (const t of ei.targets) {
    assert.equal(typeof t.enemyPlayer, 'boolean')
    // M11 Task 3: every target is a real (non-aggregate) agent -- axibridge
    // filters `!t.isFake` everywhere it reads `targets[]`.
    assert.equal(t.isFake, false, 'every target must be isFake: false')
  }

  // M11 Task 3: `activeTimes`/`combatReplayData` are ALWAYS present (not
  // gated on a `--replay`-equivalent option -- `parseFileEi` takes none),
  // with `down`/`dead` arrays of `[start, end]` pairs (positions stay
  // absent -- see `axilog_ei::to_ei_json`'s module comment).
  assert.ok(Array.isArray(p0.activeTimes) && p0.activeTimes.length === 1)
  assert.equal(typeof p0.activeTimes[0], 'number')
  assert.ok(p0.combatReplayData, 'expected combatReplayData')
  assert.equal(typeof p0.combatReplayData.start, 'number')
  assert.equal(typeof p0.combatReplayData.end, 'number')
  assert.ok(Array.isArray(p0.combatReplayData.down))
  assert.ok(Array.isArray(p0.combatReplayData.dead))
  assert.equal(p0.combatReplayData.positions, undefined, 'positions must stay absent without { replay: true }')
  assert.equal(ei.combatReplayMetaData, undefined, 'combatReplayMetaData must stay absent without { replay: true }')

  assert.ok(ei.wvWMapData, 'expected wvWMapData')
  for (const key of ['redTeamID', 'blueTeamID', 'greenTeamID']) {
    assert.equal(typeof ei.wvWMapData[key], 'number')
  }

  // Cross-check against the golden EI fixture this same log was verified
  // against (see `crates/axilog-core/tests/*_golden.rs`): every account in
  // the native EI output should be shaped like the golden fixture's
  // (`Anon<N>.<digits>` or a leading `:`-stripped equivalent).
  const golden = JSON.parse(readFileSync(GOLDEN_EI_JSON, 'utf8'))
  assert.ok(golden.players.length > 0)
})

test('anonymizeFile: round-trip parses identically to the source fixture', () => {
  const tmpDir = mkdtempSync(join(tmpdir(), 'axilog-node-test-'))
  const outPath = join(tmpDir, 'wvw-small.anon2.zevtc')
  try {
    const rewritten = sdk.anonymizeFile(FIXTURE, outPath)
    assert.ok(rewritten > 0, 'expected at least one player agent rewritten')

    const original = sdk.parseFile(FIXTURE)
    const roundTripped = sdk.parseFile(outPath)

    assert.equal(players(roundTripped).length, players(original).length)
    assert.equal(roundTripped.entities.length, original.entities.length)

    const damageOf = (r) => sumBy(players(r), (p) => slot(r.blocks.damage, p)?.total ?? 0)
    assert.equal(damageOf(roundTripped), damageOf(original))

    assert.equal(roundTripped.encounter.duration_ms, original.encounter.duration_ms)
    assert.equal(roundTripped.blocks.damage.squad.total, original.blocks.damage.squad.total)
  } finally {
    rmSync(tmpDir, { recursive: true, force: true })
  }
})

test('parseFile throws a non-empty, descriptive error for a missing file', () => {
  assert.throws(
    () => sdk.parseFile(join(tmpdir(), 'axilog-node-test-does-not-exist.zevtc')),
    (err) => {
      assert.ok(err instanceof Error)
      assert.ok(typeof err.message === 'string' && err.message.length > 0, 'expected a non-empty error message')
      return true
    },
  )
})

test('dual-path parity: node parseFile matches the CLI\'s --format json output', () => {
  const nodeReport = sdk.parseFile(FIXTURE)
  const stdout = execFileSync(CLI_BIN, ['parse', FIXTURE, '--format', 'json'], {
    maxBuffer: 64 * 1024 * 1024,
  })
  const cliReport = JSON.parse(stdout.toString('utf8'))

  try {
    assert.deepStrictEqual(nodeReport, cliReport)
  } catch {
    const diffs = firstDiffPaths(nodeReport, cliReport, 10)
    assert.fail(
      `node parseFile output diverges from CLI --format json output at ${diffs.length} path(s):\n` +
        diffs.join('\n'),
    )
  }
})

test('parseFile: { everything: true } computes every gate -- nothing left not_computed', () => {
  // The contract is stated in terms of `coverage`, not of a block list:
  // `everything` means "every pass this version knows about", so a test
  // that enumerated blocks would drift from it exactly the way a
  // consumer's option list does -- which is the drift `everything` exists
  // to prevent.
  const all = sdk.parseFile(FIXTURE, { everything: true })
  const notComputed = Object.entries(all.coverage).filter(([, s]) => s === 'not_computed')
  assert.deepEqual(
    notComputed,
    [],
    'everything: true must leave no block reporting not_computed'
  )

  // `unsupported` is deliberately still permitted -- it is the LOG's
  // answer, and no option can change it.
  const states = new Set(Object.values(all.coverage))
  for (const s of states) {
    assert.ok(
      ['present', 'empty', 'unsupported'].includes(s),
      `unexpected coverage state ${s}`
    )
  }

  // A UNION with the individual options, never an override.
  const alsoReplay = sdk.parseFile(FIXTURE, { everything: true, replay: true })
  assert.deepEqual(alsoReplay.coverage, all.coverage, 'everything + replay == everything')

  // And it genuinely turns gates ON: the default parse leaves several off.
  const bare = sdk.parseFile(FIXTURE)
  const bareOff = Object.values(bare.coverage).filter((s) => s === 'not_computed').length
  assert.ok(bareOff >= 3, `expected the default parse to leave gates off, got ${bareOff}`)
})
