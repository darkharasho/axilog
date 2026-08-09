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

  assert.equal(report.schema_version, '0.2')
  assert.equal(typeof report.axilog_version, 'string')
  assert.ok(report.players.length > 0, 'expected at least one player')
  assert.equal(report.players.length, 42)

  const squadDamageTotal = sumBy(report.players, (p) => p.damage.total)
  assert.equal(squadDamageTotal, EXPECTED_SQUAD_DAMAGE_TOTAL)

  const support = {
    cleanses: sumBy(report.players, (p) => p.support.cleanses),
    cleanses_self: sumBy(report.players, (p) => p.support.cleanses_self),
    strips: sumBy(report.players, (p) => p.support.strips),
    resurrects: sumBy(report.players, (p) => p.support.resurrects),
  }
  assert.deepEqual(support, EXPECTED_SUPPORT_SUMS)

  const player = report.players.find((p) => p.account === STABLE_BOON_ACCOUNT)
  assert.ok(player, `expected fixture to contain account ${STABLE_BOON_ACCOUNT}`)
  const boon = player.boons.find((b) => b.name === STABLE_BOON_NAME)
  assert.ok(boon, `expected ${STABLE_BOON_ACCOUNT} to have a ${STABLE_BOON_NAME} entry`)
  assert.ok(
    Math.abs(boon.presence_pct - STABLE_BOON_EXPECTED_PRESENCE_PCT) < 0.05,
    `${STABLE_BOON_NAME} presence_pct ${boon.presence_pct} not within 0.05 of golden ${STABLE_BOON_EXPECTED_PRESENCE_PCT}`,
  )
})

test('parseBuffer produces the same Report as parseFile', () => {
  const fromFile = sdk.parseFile(FIXTURE)
  const buf = readFileSync(FIXTURE)
  const fromBuffer = sdk.parseBuffer(buf)
  assert.deepStrictEqual(fromBuffer, fromFile)
})

test('parseFile: replay opt-in (M9 Task 2) -- absent by default, present + shaped when requested', () => {
  const withoutReplay = sdk.parseFile(FIXTURE)
  assert.equal(withoutReplay.replay, undefined, 'replay must be absent by default')

  const withReplay = sdk.parseFile(FIXTURE, { replay: true })
  assert.ok(withReplay.replay, 'expected a replay block when { replay: true }')
  assert.equal(typeof withReplay.replay.poll_ms, 'number')
  assert.ok(withReplay.replay.tracks.length > 0, 'expected at least one replay track')
  const track = withReplay.replay.tracks[0]
  assert.equal(typeof track.name, 'string')
  assert.equal(typeof track.team, 'string')
  assert.equal(typeof track.is_squad, 'boolean')
  assert.ok(Array.isArray(track.samples))
  if (track.samples.length > 0) {
    assert.equal(track.samples[0].length, 3, 'each sample is a [t, x, y] triple')
  }

  // opts.replay: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { replay: false })
  assert.equal(explicitlyOff.replay, undefined)

  // parseBuffer accepts the same opts shape.
  const buf = readFileSync(FIXTURE)
  const bufWithReplay = sdk.parseBuffer(buf, { replay: true })
  assert.ok(bufWithReplay.replay)
})

test('parseFile: skillDamage opt-in (M12 Task 1) -- absent by default, present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  assert.equal(without.players[0].skill_damage, undefined, 'skill_damage must be absent by default')

  const withIt = sdk.parseFile(FIXTURE, { skillDamage: true })
  const p0 = withIt.players.find((p) => p.damage.total > 0)
  assert.ok(p0, 'expected at least one player with nonzero damage')
  assert.ok(p0.skill_damage, 'expected a skill_damage block when { skillDamage: true }')
  assert.ok(Array.isArray(p0.skill_damage.outgoing))
  assert.ok(Array.isArray(p0.skill_damage.taken))
  assert.ok(Array.isArray(p0.skill_damage.per_target))
  assert.ok(p0.skill_damage.outgoing.length > 0, 'expected at least one outgoing skill entry')
  const entry = p0.skill_damage.outgoing[0]
  assert.equal(typeof entry.skill_id, 'number')
  assert.equal(typeof entry.total, 'number')
  assert.equal(typeof entry.hits, 'number')
  // sum(outgoing[*].total) == damage.total exactly (internal invariant).
  const sum = sumBy(p0.skill_damage.outgoing, (e) => e.total)
  assert.equal(sum, p0.damage.total, 'sum(outgoing totals) must equal damage.total exactly')

  // opts.skillDamage: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { skillDamage: false })
  assert.equal(explicitlyOff.players[0].skill_damage, undefined)
})

test('parseFile: timeseries opt-in (M12 Task 2) -- per_second AND dps_targets both absent by default, both present + shaped when requested', () => {
  const without = sdk.parseFile(FIXTURE)
  assert.equal(without.players[0].per_second, undefined, 'per_second must be absent by default')
  assert.equal(without.players[0].dps_targets, undefined, 'dps_targets must be absent by default')

  const withIt = sdk.parseFile(FIXTURE, { timeseries: true })
  const p0 = withIt.players.find((p) => p.damage.total > 0)
  assert.ok(p0, 'expected at least one player with nonzero damage')
  assert.ok(p0.per_second, 'expected a per_second block when { timeseries: true }')
  assert.ok(Array.isArray(p0.per_second.damage))
  assert.ok(Array.isArray(p0.per_second.damage_taken))
  assert.ok(Array.isArray(p0.per_second.per_target))
  assert.ok(p0.per_second.damage.length > 0, 'expected at least one bucket')
  // Cumulative: the final bucket equals damage.total exactly (internal invariant).
  const finalDamage = p0.per_second.damage[p0.per_second.damage.length - 1]
  assert.equal(finalDamage, p0.damage.total, 'final per_second.damage bucket must equal damage.total exactly')
  // Monotonic non-decreasing.
  for (let i = 1; i < p0.per_second.damage.length; i++) {
    assert.ok(p0.per_second.damage[i] >= p0.per_second.damage[i - 1], 'per_second.damage must be cumulative (monotonic non-decreasing)')
  }

  assert.ok(Array.isArray(p0.dps_targets), 'expected a dps_targets array when { timeseries: true }')
  assert.ok(p0.dps_targets.length > 0, 'expected at least one dps_targets entry')
  const dt = p0.dps_targets[0]
  assert.equal(typeof dt.enemy_id, 'number')
  assert.equal(typeof dt.damage, 'number')
  assert.equal(typeof dt.dps, 'number')
  // sum(dps_targets[*].damage) == damage.total exactly (internal invariant).
  const dtSum = sumBy(p0.dps_targets, (d) => d.damage)
  assert.equal(dtSum, p0.damage.total, 'sum(dps_targets damage) must equal damage.total exactly')

  // opts.timeseries: false must behave the same as omitting opts entirely.
  const explicitlyOff = sdk.parseFile(FIXTURE, { timeseries: false })
  assert.equal(explicitlyOff.players[0].per_second, undefined)
  assert.equal(explicitlyOff.players[0].dps_targets, undefined)
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
  assert.equal(p0.combatReplayData.positions, undefined, 'positions must stay absent (deferred to M15)')

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

    assert.equal(roundTripped.players.length, original.players.length)

    const originalDamage = sumBy(original.players, (p) => p.damage.total)
    const roundTrippedDamage = sumBy(roundTripped.players, (p) => p.damage.total)
    assert.equal(roundTrippedDamage, originalDamage)

    assert.equal(roundTripped.encounter.duration_ms, original.encounter.duration_ms)
    assert.equal(roundTripped.enemies.length, original.enemies.length)
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
