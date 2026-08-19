// The axibridge guard: axibridge consumes ei-json exclusively through
// parseFileEi, so the facade refactor must not move a single byte of it.
//
// Named `*.test.mjs` (not the brief's literal `ei-byte-identity.mjs`) to
// match this package's `npm test` glob (`node --test __test__/*.test.mjs`,
// see package.json) so this guard actually runs in CI, not just when
// invoked by hand.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { parseFileEi } = require('../index.js')

// crates/axilog-node/__test__ -> crates/axilog-node -> crates -> repo root
const FIXTURE = new URL('../../../fixtures/wvw-small.anon.zevtc', import.meta.url).pathname
const BASELINE = '/tmp/ei-baseline.json'

test('ei-json output is byte-identical to the pre-refactor baseline', () => {
  const actual = JSON.stringify(parseFileEi(FIXTURE, { everything: true }))
  const expected = readFileSync(BASELINE, 'utf8')

  assert.equal(actual.length, expected.length, 'ei-json byte length changed')
  assert.equal(actual, expected, 'ei-json content changed')
  console.log('ei-json byte-identical:', actual.length, 'bytes')
})
