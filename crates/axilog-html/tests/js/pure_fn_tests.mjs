// Node-based unit tests for report.js's PURE functions (Task 2, M7 plan).
//
// report.js is written as a UMD module specifically so it is `require`-
// (and, via CJS/ESM default-export interop, `import`-) able from Node
// without a browser/DOM -- see the module header comment in
// crates/axilog-html/assets/report.js. This file is invoked by
// crates/axilog-html/tests/js_units.rs (`node <this file>`); it exits
// non-zero (via a thrown assertion) on any failure, which the Rust test
// treats as a failed `cargo test`. If `node` itself isn't on PATH, the
// Rust test skips this file entirely rather than failing the build.
//
// ESM (not CJS) so plain `import` works without a package.json at this
// path declaring "type": "module" for a sibling .js file.

import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const reportJsPath = path.join(here, "..", "..", "assets", "report.js");
const AxilogReport = (await import(reportJsPath)).default;

let ran = 0;
function test(name, fn) {
  ran++;
  try {
    fn();
  } catch (err) {
    console.error(`FAIL: ${name}`);
    throw err;
  }
}

// ---- formatting --------------------------------------------------------

test("formatThousands: groups triples", () => {
  assert.equal(AxilogReport.formatThousands(1234567), "1,234,567");
  assert.equal(AxilogReport.formatThousands(0), "0");
  assert.equal(AxilogReport.formatThousands(999), "999");
  assert.equal(AxilogReport.formatThousands(1000), "1,000");
});

test("formatThousands: rounds and handles negatives", () => {
  assert.equal(AxilogReport.formatThousands(1234.6), "1,235");
  assert.equal(AxilogReport.formatThousands(-42000), "-42,000");
});

test("formatPercent: one decimal place with a trailing %", () => {
  assert.equal(AxilogReport.formatPercent(33.333), "33.3%");
  assert.equal(AxilogReport.formatPercent(100), "100.0%");
  assert.equal(AxilogReport.formatPercent(0), "0.0%");
});

test("formatStacks: two decimal places", () => {
  assert.equal(AxilogReport.formatStacks(12.3456), "12.35");
  assert.equal(AxilogReport.formatStacks(0), "0.00");
});

test("formatFixed: thousands-grouped integer part, arbitrary decimals", () => {
  assert.equal(AxilogReport.formatFixed(1234567.891, 1), "1,234,567.9");
  assert.equal(AxilogReport.formatFixed(4.5, 1), "4.5");
});

test("formatSecondsFromMs: ms -> one-decimal seconds", () => {
  assert.equal(AxilogReport.formatSecondsFromMs(4500), "4.5");
  assert.equal(AxilogReport.formatSecondsFromMs(0), "0.0");
});

test("formatDuration: mm:ss, seconds floored", () => {
  assert.equal(AxilogReport.formatDuration(125_000), "02:05");
  assert.equal(AxilogReport.formatDuration(59_999), "00:59");
});

// ---- sorting: direction + stability -------------------------------------

test("sortRows: ascending by numeric key", () => {
  const rows = [{ k: 3 }, { k: 1 }, { k: 2 }];
  const sorted = AxilogReport.sortRows(rows, "k", "asc");
  assert.deepEqual(sorted.map((r) => r.k), [1, 2, 3]);
});

test("sortRows: descending by numeric key", () => {
  const rows = [{ k: 3 }, { k: 1 }, { k: 2 }];
  const sorted = AxilogReport.sortRows(rows, "k", "desc");
  assert.deepEqual(sorted.map((r) => r.k), [3, 2, 1]);
});

test("sortRows: stable on ties (ascending) -- original relative order kept", () => {
  const rows = [
    { k: 1, tag: "a" },
    { k: 2, tag: "b" },
    { k: 1, tag: "c" },
    { k: 1, tag: "d" },
  ];
  const sorted = AxilogReport.sortRows(rows, "k", "asc");
  assert.deepEqual(
    sorted.map((r) => r.tag),
    ["a", "c", "d", "b"]
  );
});

test("sortRows: stable on ties (descending) -- original relative order kept", () => {
  const rows = [
    { k: 1, tag: "a" },
    { k: 2, tag: "b" },
    { k: 1, tag: "c" },
    { k: 2, tag: "d" },
  ];
  const sorted = AxilogReport.sortRows(rows, "k", "desc");
  assert.deepEqual(
    sorted.map((r) => r.tag),
    ["b", "d", "a", "c"]
  );
});

test("sortRows: does not mutate the input array", () => {
  const rows = [{ k: 3 }, { k: 1 }];
  const before = rows.slice();
  AxilogReport.sortRows(rows, "k", "asc");
  assert.deepEqual(rows, before);
});

// ---- team chips: hide-zero rule + squad-vs-enemies label -----------------

test("buildTeamChips: labels the recording squad's own team 'squad'", () => {
  const encounter = { teams: [{ color: "green", team_id: 1 }] };
  const players = [{ team: "green" }, { team: "green" }];
  const chips = AxilogReport.buildTeamChips(encounter, players, []);
  assert.deepEqual(chips, [
    { color: "green", cssClass: "green", count: 2, kind: "squad", total: 2 },
  ]);
});

test("buildTeamChips: falls back to counting enemies for teams with no squad players", () => {
  const encounter = { teams: [{ color: "blue", team_id: 2 }] };
  const enemies = [{ team: "blue" }, { team: "blue" }, { team: "blue" }];
  const chips = AxilogReport.buildTeamChips(encounter, [], enemies);
  assert.deepEqual(chips, [
    { color: "blue", cssClass: "blue", count: 3, kind: "enemies", total: 3 },
  ]);
});

test("buildTeamChips: hides teams with zero players on both sides", () => {
  const encounter = {
    teams: [
      { color: "green", team_id: 1 },
      { color: "red", team_id: 3 },
    ],
  };
  const players = [{ team: "green" }];
  const chips = AxilogReport.buildTeamChips(encounter, players, []);
  assert.equal(chips.length, 1);
  assert.equal(chips[0].color, "green");
});

test("buildTeamChips: unknown-color team still renders (falls back to the 'unknown' CSS class)", () => {
  const encounter = { teams: [{ color: "unknown", team_id: 0 }] };
  const enemies = [{ team: "unknown" }];
  const chips = AxilogReport.buildTeamChips(encounter, [], enemies);
  assert.equal(chips[0].cssClass, "unknown");
  assert.equal(chips[0].kind, "enemies");
});

// ---- damage / support / boons row-building -------------------------------

function fixturePlayer(overrides) {
  return Object.assign(
    {
      account: ":A.1234",
      character: "Alpha",
      profession: "Guardian",
      elite_spec: "Firebrand",
      subgroup: 1,
      damage: { total: 1234567, dps: 8901.2, per_enemy: [] },
      downs_dealt: 3,
      kills_dealt: 1,
      downs_contribution: { damage: 42, cc: 0, strips: 0, movement_impairing: 0 },
      deaths: 0,
      damage_taken: 5000,
      cc: { applied_total: 0, applied_duration_ms: 0, stun_breaks: 2, removed_stun_duration_ms: 4500 },
      support: { cleanses: 10, cleanses_self: 2, strips: 3, resurrects: 1 },
      boons: [
        { id: 1, name: "Might", presence_pct: 100, avg_stacks: 18.5, generation: { self_pct: 90, group_pct: 60, squad_pct: 40 } },
        { id: 2, name: "Quickness", presence_pct: 55.5, generation: { self_pct: 80, group_pct: 30, squad_pct: 20 } },
      ],
    },
    overrides || {}
  );
}

test("professionDisplay: elite spec name when present", () => {
  assert.equal(AxilogReport.professionDisplay(fixturePlayer()), "Firebrand");
  assert.equal(
    AxilogReport.professionDisplay(fixturePlayer({ elite_spec: "" })),
    "Guardian"
  );
});

test("buildDamageRows: thousand-separator-ready raw numbers + nonSquad flag", () => {
  const rows = AxilogReport.buildDamageRows([
    fixturePlayer({ subgroup: 0 }),
  ]);
  assert.equal(rows[0].damage, 1234567);
  assert.equal(rows[0].nonSquad, true);
  assert.equal(rows[0].professionDisplay, "Firebrand");
});

test("buildDamageTotals: sums every numeric column across rows", () => {
  const rows = AxilogReport.buildDamageRows([
    fixturePlayer({ damage: { total: 100, dps: 10, per_enemy: [] }, downs_dealt: 1, kills_dealt: 1, deaths: 1, downs_contribution: { damage: 1, cc: 0, strips: 0, movement_impairing: 0 }, damage_taken: 1 }),
    fixturePlayer({ damage: { total: 200, dps: 20, per_enemy: [] }, downs_dealt: 2, kills_dealt: 2, deaths: 2, downs_contribution: { damage: 2, cc: 0, strips: 0, movement_impairing: 0 }, damage_taken: 2 }),
  ]);
  const totals = AxilogReport.buildDamageTotals(rows);
  assert.equal(totals.damage, 300);
  assert.equal(totals.dps, 30);
  assert.equal(totals.downs, 3);
  assert.equal(totals.kills, 3);
  assert.equal(totals.deaths, 3);
  assert.equal(totals.downContribution, 3);
  assert.equal(totals.damageTaken, 3);
});

test("buildSupportRows: removed-stun ms converted to seconds", () => {
  const rows = AxilogReport.buildSupportRows([fixturePlayer()]);
  assert.equal(rows[0].cleanses, 10);
  assert.equal(rows[0].cleansesSelf, 2);
  assert.equal(rows[0].stunBreaks, 2);
  assert.equal(rows[0].removedStunSeconds, 4.5);
});

test("buildBoonRows: Might avg stacks + presence % + default (group) generation", () => {
  const rows = AxilogReport.buildBoonRows([fixturePlayer()], undefined);
  assert.equal(rows[0].mightAvg, 18.5);
  assert.equal(rows[0].presenceQuickness, 55.5);
  assert.equal(rows[0].presenceAlacrity, 0); // no Alacrity boon entry in fixture -> 0
  assert.equal(rows[0].genMight, 60); // default mode is "group"
  assert.equal(rows[0].genQuickness, 30);
});

test("buildBoonRows: generation mode toggle swaps self/group/squad values", () => {
  const [self_] = AxilogReport.buildBoonRows([fixturePlayer()], "self");
  const [group_] = AxilogReport.buildBoonRows([fixturePlayer()], "group");
  const [squad_] = AxilogReport.buildBoonRows([fixturePlayer()], "squad");
  assert.equal(self_.genMight, 90);
  assert.equal(group_.genMight, 60);
  assert.equal(squad_.genMight, 40);
});

// ---- timeline: formatK + buildTimelinePaths -------------------------------

test("formatK: k-format for damage y-axis ticks", () => {
  assert.equal(AxilogReport.formatK(45000), "45k");
  assert.equal(AxilogReport.formatK(45231), "45.2k");
  assert.equal(AxilogReport.formatK(999), "999");
  assert.equal(AxilogReport.formatK(0), "0");
  assert.equal(AxilogReport.formatK(-2500), "-2.5k");
});

test("buildTimelinePaths: empty timeline (Task 3 required case) returns empty paths/markers/ticks", () => {
  const result = AxilogReport.buildTimelinePaths(
    { squad_damage: [], cc_applied: [], downs: [] },
    960,
    260
  );
  assert.equal(result.areaPath, "");
  assert.equal(result.linePath, "");
  assert.deepEqual(result.downMarkers, []);
  assert.deepEqual(result.ccBars, []);
  assert.deepEqual(result.xTicks, []);
  assert.deepEqual(result.yTicks, []);
});

test("buildTimelinePaths: missing per_second is treated the same as empty", () => {
  const result = AxilogReport.buildTimelinePaths(undefined, 960, 260);
  assert.equal(result.areaPath, "");
  assert.deepEqual(result.xTicks, []);
});

test("buildTimelinePaths: single-bucket timeline (Task 3 required case) centers the point", () => {
  const result = AxilogReport.buildTimelinePaths(
    { squad_damage: [5000], cc_applied: [2], downs: [1] },
    960,
    260
  );
  // A lone point has no line segment to draw (no "L" in the path) but the
  // area still closes into a (degenerate, zero-width) polygon.
  assert.ok(result.linePath.startsWith("M"));
  assert.ok(!result.linePath.includes("L"));
  assert.ok(result.areaPath.endsWith("Z"));
  assert.equal(result.xTicks.length, 1);
  assert.equal(result.xTicks[0].label, "00:00");
  assert.equal(result.downMarkers.length, 1);
  assert.equal(result.downMarkers[0].second, 0);
  assert.equal(result.ccBars.length, 1);
  assert.equal(result.ccBars[0].second, 0);
});

test("buildTimelinePaths: multi-bucket timeline builds one line segment per second + down/cc markers", () => {
  const squad = [0, 1000, 5000, 2000, 8000, 3000, 100, 500];
  const cc = [0, 1, 0, 3, 0, 0, 2, 0];
  const downs = [0, 0, 1, 0, 0, 2, 0, 0];
  const result = AxilogReport.buildTimelinePaths(
    { squad_damage: squad, cc_applied: cc, downs: downs },
    960,
    260
  );
  assert.equal((result.linePath.match(/L/g) || []).length, squad.length - 1);
  assert.equal(result.downMarkers.length, 2);
  assert.deepEqual(result.downMarkers.map((mrk) => mrk.second), [2, 5]);
  assert.equal(result.ccBars.length, 3);
  assert.deepEqual(result.ccBars.map((bar) => bar.second), [1, 3, 6]);
  assert.ok(result.xTicks.length > 0 && result.xTicks.length <= 6);
  assert.equal(result.yTicks.length, 5);
  assert.equal(result.yTicks[0].label, "0");
  assert.equal(result.yTicks[4].label, "8k"); // yMax (8000) k-formatted
});

test("buildTimelinePaths: down markers sit ON the damage line (same y as that second's damage point)", () => {
  const squad = [1000, 9000, 500];
  const result = AxilogReport.buildTimelinePaths(
    { squad_damage: squad, cc_applied: [0, 0, 0], downs: [0, 1, 0] },
    960,
    260
  );
  assert.equal(result.downMarkers.length, 1);
  const lineYAtSecond1 = result.linePath
    .split("L")[1] // "M x0,y0" then "L x1,y1" ...
    .split(",")[1];
  assert.equal(result.downMarkers[0].y, Number(lineYAtSecond1));
});

// ---- replay: hasReplay / replayViewBox / positionsAt / isDownAt/isDeadAt --

test("hasReplay: false for absent/empty, true for at least one track", () => {
  assert.equal(AxilogReport.hasReplay(null), false);
  assert.equal(AxilogReport.hasReplay({}), false);
  assert.equal(AxilogReport.hasReplay({ replay: { tracks: [] } }), false);
  assert.equal(AxilogReport.hasReplay({ replay: { tracks: [{}] } }), true);
});

// M10 Task 3: `replayHasSamples` distinguishes "tracks[] is non-empty" (what
// `hasReplay` checks -- gates the tab button) from "at least one track
// actually carries position samples" (what decides whether the Replay tab
// shows the animated stage or a "no position data" message).
test("replayHasSamples: false for absent/empty tracks, and for tracks with zero samples each", () => {
  assert.equal(AxilogReport.replayHasSamples(null), false);
  assert.equal(AxilogReport.replayHasSamples({}), false);
  assert.equal(AxilogReport.replayHasSamples({ tracks: [] }), false);
  // Every track present (as `build_replay` always produces one per squad/
  // enemy-player representative) but none carry any samples -- a log with
  // no CBTS_POSITION telemetry at all.
  assert.equal(
    AxilogReport.replayHasSamples({ tracks: [{ samples: [] }, { samples: [] }] }),
    false
  );
});

test("replayHasSamples: true when at least one track has a nonzero samples array", () => {
  assert.equal(
    AxilogReport.replayHasSamples({ tracks: [{ samples: [] }, { samples: [[0, 1, 2]] }] }),
    true
  );
});

test("replayViewBox: pads bounds 5% on every side", () => {
  const vb = AxilogReport.replayViewBox({ min_x: 0, min_y: 0, max_x: 100, max_y: 50 });
  assert.deepEqual(vb, { x: -5, y: -2.5, width: 110, height: 55 });
});

test("replayViewBox: degenerate (zero-area) bounds fall back to a 1x1 span before padding", () => {
  const vb = AxilogReport.replayViewBox({ min_x: 5, min_y: 5, max_x: 5, max_y: 5 });
  assert.equal(vb.width, 1.1);
  assert.equal(vb.height, 1.1);
});

function replayTrack(overrides) {
  return Object.assign(
    {
      name: "Alpha",
      team: "red",
      commander: false,
      is_squad: true,
      samples: [
        [0, 0, 0],
        [1000, 10, 10],
        [2000, 20, 0],
      ],
      down_intervals: [[500, 900]],
      dead_intervals: [],
    },
    overrides || {}
  );
}

test("positionsAt: exact-sample hit returns that sample's position at full opacity", () => {
  const [pos] = AxilogReport.positionsAt([replayTrack()], 1000);
  assert.deepEqual(pos, { x: 10, y: 10, opacity: 1 });
});

test("positionsAt: between-samples linearly interpolates", () => {
  const [pos] = AxilogReport.positionsAt([replayTrack()], 1500);
  assert.deepEqual(pos, { x: 15, y: 5, opacity: 1 });
});

test("positionsAt: before-first holds the first sample at full opacity", () => {
  const [pos] = AxilogReport.positionsAt([replayTrack()], -500);
  assert.deepEqual(pos, { x: 0, y: 0, opacity: 1 });
});

test("positionsAt: after-last holds the last sample and fades opacity over REPLAY_FADE_MS", () => {
  const half = AxilogReport.positionsAt([replayTrack()], 2000 + AxilogReport.REPLAY_FADE_MS / 2)[0];
  assert.equal(half.x, 20);
  assert.equal(half.y, 0);
  assert.equal(half.opacity, 0.5);

  const full = AxilogReport.positionsAt([replayTrack()], 2000 + AxilogReport.REPLAY_FADE_MS)[0];
  assert.equal(full.opacity, 0);
});

test("positionsAt: empty track (no samples) returns null, not a position", () => {
  const [pos] = AxilogReport.positionsAt([replayTrack({ samples: [] })], 500);
  assert.equal(pos, null);
});

test("isDownAt / isDeadAt: half-open [start,end) -- inclusive start, exclusive end", () => {
  const track = replayTrack({ down_intervals: [[500, 900]], dead_intervals: [[900, 1200]] });
  assert.equal(AxilogReport.isDownAt(track, 700), true);
  assert.equal(AxilogReport.isDownAt(track, 500), true); // inclusive start
  assert.equal(AxilogReport.isDownAt(track, 900), false); // exclusive end
  assert.equal(AxilogReport.isDownAt(track, 100), false);
  assert.equal(AxilogReport.isDeadAt(track, 1000), true);
  assert.equal(AxilogReport.isDeadAt(track, 700), false);
});

test("isDownAt / isDeadAt: down->dead sharing one timestamp -- exactly one state true at the transition ms", () => {
  // Matches the Rust `Interval` contract (half-open, == GW2EI): a
  // CHANGE_DEAD event both closes the down interval and opens the dead
  // interval at the same ms -- at that instant the agent reads as dead,
  // not down.
  const track = replayTrack({ down_intervals: [[500, 900]], dead_intervals: [[900, 1200]] });
  assert.equal(AxilogReport.isDownAt(track, 900), false);
  assert.equal(AxilogReport.isDeadAt(track, 900), true);
});

test("isDownAt / isDeadAt: empty intervals never match", () => {
  const track = replayTrack({ down_intervals: [], dead_intervals: [] });
  assert.equal(AxilogReport.isDownAt(track, 700), false);
  assert.equal(AxilogReport.isDeadAt(track, 700), false);
});

test("shouldScheduleNextTick: false when paused, regardless of remaining duration", () => {
  assert.equal(AxilogReport.shouldScheduleNextTick(false, 0, 49000), false);
  assert.equal(AxilogReport.shouldScheduleNextTick(false, 49000, 49000), false);
});

test("shouldScheduleNextTick: false once currentMs reaches/passes durationMs, even while playing", () => {
  assert.equal(AxilogReport.shouldScheduleNextTick(true, 49000, 49000), false);
  assert.equal(AxilogReport.shouldScheduleNextTick(true, 50000, 49000), false);
});

test("shouldScheduleNextTick: true only while playing AND before the fight's end", () => {
  assert.equal(AxilogReport.shouldScheduleNextTick(true, 0, 49000), true);
  assert.equal(AxilogReport.shouldScheduleNextTick(true, 48999, 49000), true);
});

console.log(`ok - ${ran} pure-function tests passed`);
