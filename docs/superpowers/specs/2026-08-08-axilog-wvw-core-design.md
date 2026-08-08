# axilog — WvW Core Design (Milestone 1)

**Status:** Approved design, pending spec review
**Date:** 2026-08-08
**Scope of this spec:** The first vertical slice — a working WvW `.zevtc`/`.evtc` parser that
produces a native JSON stat report plus a partial EI-compatible adapter. Later milestones
(boons/uptime, healing, rotations, combat replay, SDKs, HTML) are named but out of scope here.

---

## 1. Context & goal

axilog is a cross-platform, CLI-first reimplementation of Elite Insights (EI) for parsing GW2
arcdps combat logs, part of the axi suite. It has a reusable Rust parsing core with planned
Python/Node SDKs, matches standard EI functionality, and follows the arcdps spec more closely —
notably **down contribution** and **CCs over time (full timeline support)**.

**The concrete driver:** the sibling app **axibridge** (and downstream axipulse /
arcdps-axipulse) currently never parses `.evtc` itself — it shells out to the **Elite Insights
CLI** and consumes EI's `DPSReportJSON` output. The long-term goal is to drop EI entirely and
have axilog produce the data those apps need. So axilog's output is what axibridge will one day
ingest, and EI's JSON is the correctness baseline we can measure against today.

**First focus: WvW.** WvW logs are open-world PvP "combat mode" — no scripted bosses, no CM
logic, no per-encounter mechanic tables. That removes EI's single largest source of complexity
and lets the first milestone be a genuinely useful, fully-correct slice rather than a broad,
shallow one.

### Non-goals (this milestone)
- Boss/instanced-PvE encounters and mechanic definitions
- Boon/buff uptime tables, healing/barrier extension stats, skill rotations, combat-replay positions
- Full EI-field parity in the `ei-json` adapter (only Milestone-1 fields are mapped)
- Python/Node SDKs, HTML/web report

---

## 2. Key decisions (from brainstorming)

1. **WvW-first vertical slice** through the whole pipeline (read → decode → model → analyze → emit).
2. **JSON is the default output**; `--format table`, `--format csv`, and later `html` are opt-in flags.
3. **Native schema is the source of truth (option B)**; an `--format ei-json` adapter provides
   equivalent EI compatibility without binding our data model to EI's shape. Down-contribution
   and CC-over-time are first-class in the native schema.
4. **Golden-file validation against EI.** We diff axilog's numbers against EI JSON produced from
   the same logs (fixtures available from axibridge), within numeric tolerance.

### Resolved open questions
- **License:** MIT. (axibridge itself is GPLv3; axilog is a clean reimplementation with no EI or
  axibridge code copied, so MIT is unencumbered here.)
- **`ei-json` target:** the latest EI release schema (the version axibridge currently expects,
  `DetailledWvW=True`). Pin the exact version at implementation time from axibridge's `ei-cli.conf`.
- **Fixtures:** commit 1–2 small WvW logs + trimmed EI JSON; gitignore the large fixture sets and
  point to them via an env var.
- **Release targets:** all of them, prioritized — Linux x86_64 first (dev machine), then Windows
  x86_64, macOS aarch64 + x86_64, Linux aarch64.

---

## 3. Architecture — Cargo workspace

Engine cleanly separated from I/O so the CLI and future SDKs share one core.

```
axilog/
├── crates/
│   ├── axilog-core/     # the engine (library, no I/O beyond taking &[u8])
│   │   ├── evtc/        # binary format decode: header, agents, skills, combat events (revision-aware)
│   │   ├── model/       # resolved domain: Encounter, Agent, Player, Target, Event, Phase
│   │   ├── analysis/    # dps, damage distribution, downs/kills, down contribution, CC timeline
│   │   └── wvw/         # squad↔enemy resolution, team detection, map/area detection
│   ├── axilog-schema/   # native versioned serde output types + serialization (source of truth)
│   ├── axilog-ei/       # compatibility adapter: native model → EI DPSReportJSON shape
│   └── axilog-cli/      # the `axilog` binary (clap): input, --format flags, orchestration
└── docs/ tests/ fixtures/ ...
```

Deferred crates (named now, built later): `axilog-py` (PyO3), `axilog-node` (napi-rs).

**Boundaries / testability:**
- `axilog-core` takes bytes, returns a domain model + computed metrics. No filesystem, no CLI concerns.
- `axilog-schema` depends on `axilog-core`'s model; owns the public JSON contract and its version.
- `axilog-ei` depends on the model; owns the EI mapping. Isolated so EI's quirks never leak into the native schema.
- `axilog-cli` is the only crate that touches the filesystem, stdin/stdout, and argument parsing.

---

## 4. Parsing pipeline (data flow)

1. **Input & decompress.** Accept a path to `.zevtc` (zip/deflate-wrapped) or raw `.evtc`.
   Detect the container and inflate to raw EVTC bytes.
2. **Decode EVTC binary** (revision-aware): 16-byte header (`"EVTC"` + build date + revision byte
   + boss/instance id), then agent table, skill table, and the combat-event array (rev 0 vs rev 1
   event layouts). Built against the arcdps EVTC reference.
3. **Build domain model.** Resolve agents (players, NPCs, gadgets), link master↔minion, identify
   players and elite specs, identify enemy targets, and establish the fight time bounds. For WvW,
   phases start as a single continuous "full fight" — the timeline is the primary structure.
4. **Analysis passes** over combat events → per-player and per-target metrics plus per-second
   timeline arrays (damage, downs, CC).
5. **WvW resolution.** Squad vs enemy via account/team; dedupe agent churn (arcdps emits a new
   agent on relog / build swap / subgroup change — sums iterate all entries, counts dedupe by
   account then character name); team colors from the WvW-team statechange event; map/area name
   from the fight name.
6. **Emit.** Native JSON (default) → serde_json; or `table` / `csv` / `ei-json` via adapters.

The EVTC binary decode (steps 2–3) is the hard-correctness core and where TDD focus goes first.

---

## 5. Native output schema (source of truth, versioned)

Top level carries `schema_version` and `axilog_version`. Cleaner naming than EI; the
differentiators are first-class rather than bolted on.

```jsonc
{
  "schema_version": "0.1",
  "axilog_version": "0.1.0",
  "encounter": {
    "kind": "wvw",
    "map": "Eternal Battlegrounds",      // from fight name
    "duration_ms": 0,
    "start_time": "…", "recorded_by": "…",
    "evtc": { "revision": 1, "arc_build": "…" },
    "teams": [ { "color": "red",  "team_id": 0 }, … ]   // from WvW-team statechange
  },
  "players": [
    {
      "account": "…", "character": "…", "profession": "…", "elite_spec": "…",
      "team": "red", "subgroup": 1, "in_squad": true, "commander": false,
      "damage": { "total": 0, "dps": 0, "per_enemy": [ { "enemy_id": 0, "total": 0 } ] },
      "downs_dealt": 0, "kills_dealt": 0,
      "down_contribution": 0,               // first-class
      "downs_taken": 0, "deaths": 0, "damage_taken": 0,
      "cc": { "applied_total": 0, "applied_duration_ms": 0 }
    }
  ],
  "enemies": [ { "id": 0, "name": "…", "team": "green", "is_player": true } ],
  "timeline": {
    "resolution_ms": 1000,
    "per_second": {
      "squad_damage": [ … ],
      "cc_applied": [ … ],                  // CCs over time — first-class
      "downs": [ … ]
    }
  }
}
```

Exact field names finalize during implementation; the shape above is the contract intent.

---

## 6. EI-compatibility adapter (`--format ei-json`)

`axilog-ei` maps the native model onto EI's `DPSReportJSON` shape (the contract in axibridge's
`packages/bridge-metrics/src/dpsReportTypes.ts`). Milestone 1 maps only the fields the WvW slice
produces:

- `evtc{type,version,bossId}`, `fightName`, `durationMS`, `recordedBy`, `success`
- `players[]`: `account`, `character_name`, `profession`, `elite_spec`, `teamID`, `group`,
  `notInSquad`, `hasCommanderTag`, `dpsAll[]`, `statsTargets[]` (incl. `downContribution`,
  `killed`, `downed`), `defenses[]` (downCount/deadCount/damageTaken), CC fields
- `targets[]`: `id`, `name`, `enemyPlayer`, `teamID`, `dpsAll[]`
- `wvWMapData` (team ids/colors)

Fields axilog doesn't yet compute (boons, healing, rotations, replay) are omitted or emitted as
empty per EI's shape — never faked. The adapter targets the latest EI release; the exact version
is pinned from axibridge's `ei-cli.conf` at implementation time.

---

## 7. Other output formats

- **`--format table`** — human-readable terminal summary: top damage, DPS, downs, kills, deaths.
- **`--format csv`** — one row per player with the Milestone-1 stat columns.
- **`--format html`** — deferred to a later milestone.

---

## 8. Testing & validation strategy

- **TDD unit tests** per decoder and analysis pass, using small hand-built byte fixtures so the
  EVTC decode is pinned precisely (header, agent record, each combat-event revision).
- **Golden-file integration tests:** commit a curated small WvW `.zevtc` + its EI JSON (sourced
  from axibridge's `testdata/` and `test-fixtures/ei/`); assert axilog's native metrics match EI
  **within numeric tolerance** (floats differ by rounding), and that `ei-json` output field-diffs
  cleanly against the golden EI JSON for mapped fields.
- **Large fixtures** (23 MB+ EI JSON, 200 MB fixture sets) stay out of the repo, referenced via an
  env var (e.g. `AXILOG_FIXTURES`); a small trimmed set is committed for CI.
- Rust `cargo test`. (The `--maxWorkers` guidance in CLAUDE.md is JS/vitest-specific and N/A here.)

**Correctness baseline source:** axibridge's local EI CLI path — `dotnet GuildWars2EliteInsights-CLI.dll
-c <conf> <log>` with `SaveOutJSON=True, DetailledWvW=True, RawTimelineArrays=True, ParsePhases=True`
— reproduces the golden JSON for any new fixture log.

---

## 9. Milestone 1 — definition of done

Given a real WvW `.zevtc`, `axilog parse <log>` (default JSON) produces, correct within tolerance
vs EI:

- Encounter: map name, duration, teams, evtc revision, recorded-by
- Squad vs enemy resolution with agent-churn dedupe; per-player team/subgroup/profession/elite-spec
- Per player: damage out (total + per enemy), DPS, downs dealt, kills dealt, **down contribution**,
  downs taken, deaths, damage taken, **CC applied (total + over the timeline)**
- Timeline: per-second squad damage, CC applied, downs
- `--format table` summary and `--format csv`
- Partial `--format ei-json` covering the above fields
- One committed golden WvW integration test passing

### Later milestones (not now)
Boon/uptime tables · healing/barrier ext · rotations · combat-replay positions · full EI-field
parity · PvE/boss encounters · Python (PyO3) + Node (napi-rs) SDKs · HTML/web report ·
cross-platform release binaries for all targets.

---

## 10. Remaining risks / to confirm during implementation

- **EVTC revision coverage:** confirm which arc revisions appear in the sample logs; decode both rev 0 and rev 1 events.
- **zevtc container variance:** confirm whether logs are standard zip or bare deflate; handle both.
- **`ei-json` version pin:** read the exact EI version from axibridge's current `ei-cli.conf`.
- **Down-contribution definition:** match EI's `downContribution` semantics for parity, while the
  native schema is free to expose the closer-to-spec version.
