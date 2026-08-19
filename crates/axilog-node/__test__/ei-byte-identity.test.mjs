// The axibridge guard: axibridge consumes ei-json exclusively through
// parseFileEi, so the axilog-api facade refactor (M17 Task 2) must not
// move a single byte of it. This compares the current build's output
// against a committed digest of the pre-refactor baseline -- not a
// /tmp file, so it reproduces identically on any machine and in CI.
//
// Regeneration procedure (ONLY legitimate when the ei-json format is
// being intentionally changed -- e.g. a new opt-in field, a schema bump
// -- never to make a red run here go green without understanding why):
//
//   1. Identify the last commit BEFORE the change you're validating
//      against (for the original baseline this was 6dd5953, the last
//      commit before the axilog-api facade refactor touched
//      axilog-node). Check that commit out into a throwaway worktree so
//      your working tree's in-progress change can't leak in:
//
//        git worktree add /tmp/axilog-baseline-wt <commit-ish>
//        cd /tmp/axilog-baseline-wt/crates/axilog-node
//        npm install && npm run build
//
//   2. Capture the digest TWICE and confirm determinism before trusting
//      it -- if the two runs disagree (e.g. non-deterministic map
//      iteration order, float formatting), STOP: that is a finding
//      about the parser, not something this test can paper over.
//
//        node -e "
//          const crypto = require('node:crypto');
//          const { parseFileEi } = require('./index.js');
//          const json = JSON.stringify(
//            parseFileEi('../../fixtures/wvw-small.anon.zevtc', { everything: true })
//          );
//          console.log(json.length, crypto.createHash('sha256').update(json, 'utf8').digest('hex'));
//        "
//
//   3. Update fixtures/ei-baseline.sha256.json's `jsonStringLength` and
//      `sha256` to the (matching, both-runs-agreed) values, and its
//      `sourceCommit` to the commit you captured from.
//
//   4. Clean up: `git worktree remove /tmp/axilog-baseline-wt`.
//
// `jsonStringLength` is `String.prototype.length` (UTF-16 code units),
// the same thing `assert.equal` below compares -- NOT a byte count.
// It differs from `Buffer.byteLength(json, 'utf8')` (what `wc -c` on a
// captured file reports) whenever the JSON contains any character
// outside the BMP's single-UTF-16-unit range or a UTF-8 multi-byte
// sequence that doesn't map 1:1 to UTF-16 code units -- the committed
// baseline for this fixture is 3,571,194 code units vs 3,571,196 UTF-8
// bytes, a real and expected 2-unit gap between those two metrics, not
// drift between runs.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const { parseFileEi } = require('../index.js')

// crates/axilog-node/__test__ -> crates/axilog-node -> crates -> repo root
const FIXTURE = new URL('../../../fixtures/wvw-small.anon.zevtc', import.meta.url).pathname
const BASELINE_PATH = new URL('./fixtures/ei-baseline.sha256.json', import.meta.url)

const baseline = JSON.parse(readFileSync(BASELINE_PATH, 'utf8'))

test('ei-json output matches the committed pre-refactor baseline digest', () => {
  const actual = JSON.stringify(parseFileEi(FIXTURE, baseline.parseFileEiOpts))
  const actualDigest = createHash('sha256').update(actual, 'utf8').digest('hex')

  assert.equal(
    actual.length,
    baseline.jsonStringLength,
    'ei-json length changed vs the committed baseline (fixtures/ei-baseline.sha256.json) -- ' +
      'see this file\'s header comment for the regeneration procedure before touching the baseline',
  )
  assert.equal(
    actualDigest,
    baseline.sha256,
    'ei-json content changed vs the committed baseline (fixtures/ei-baseline.sha256.json) -- ' +
      'length matched but the digest did not, so bytes moved within the document; ' +
      'see this file\'s header comment for the regeneration procedure before touching the baseline',
  )
  console.log('ei-json byte-identical to committed baseline:', actual.length, 'chars, sha256', actualDigest)
})
