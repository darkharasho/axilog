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
      down_contribution: 42,
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
    fixturePlayer({ damage: { total: 100, dps: 10, per_enemy: [] }, downs_dealt: 1, kills_dealt: 1, deaths: 1, down_contribution: 1, damage_taken: 1 }),
    fixturePlayer({ damage: { total: 200, dps: 20, per_enemy: [] }, downs_dealt: 2, kills_dealt: 2, deaths: 2, down_contribution: 2, damage_taken: 2 }),
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

console.log(`ok - ${ran} pure-function tests passed`);
