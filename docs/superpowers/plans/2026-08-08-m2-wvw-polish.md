# axilog M2 — WvW Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Close the deferred M1 gaps: real profession/elite-spec names, real WvW team/map tables,
CC metrics from CROWD_CONTROL events, enemy dedupe + pet-attribution hardening, and a PII-safe
committed fixture so golden parity runs in CI.

**Architecture:** No new crates. Changes land in `axilog-core` (model, wvw, analysis), `axilog-ei`,
plus a new small anonymizer used to produce a committed fixture.

## Global Constraints

- Golden parity must remain EXACT after every task: duration 49285, squad damage 2,138,414,
  friendly 42 (±2 of 41), timeline-sum == player-damage-sum. Run
  `cargo test --workspace` after each task; the local fixture `fixtures/local/wvw-small.zevtc`
  is present on this machine.
- EI golden reference (per-player values incl. professions and CC): the anonymized dps.report EI
  JSON at `/var/home/mstephens/Documents/GitHub/axibridge/test-fixtures/boon/20260117-181030.json`
  (READ-ONLY; never copy it wholesale into this repo — extract only what a test needs into
  `fixtures/wvw-small.ei.json`).
- CC ground truth (squad sums from that EI JSON): appliedCrowdControl=34,
  appliedCrowdControlDuration=50460.
- Map ground truth: MAP_ID statechange `src_agent`=95 ⇒ "Green Alpine Borderlands".
- Friendly team id in fixture: 2767 (from TEAM_CHANGE `value` field).
- Privacy: never commit bytes containing real account/character names. The anonymized fixture must
  be byte-scanned for every original player string before commit.
- No new external runtime crates without necessity; MIT; edition 2021; keep builds warning-free.

---

### Task 1: Profession & elite-spec name tables

**Files:** Modify `crates/axilog-core/src/model/mod.rs` (extend `profession_name`); modify
`crates/axilog-ei/src/lib.rs` (EI naming); test additions in both.

**Requirements:**
- Full base-profession table (prof 1..=9: Guardian, Warrior, Engineer, Ranger, Thief,
  Elementalist, Mesmer, Necromancer, Revenant).
- Elite-spec table keyed by `is_elite` (GW2 API specialization ids). Cover ALL current elite specs
  (HoT, PoF, EoD, SotO era): e.g. 5 Druid, 7 Daredevil, 18 Berserker, 27 Dragonhunter, 34 Reaper,
  40 Chronomancer, 43 Scrapper, 48 Tempest, 52 Herald, 55 Soulbeast, 56 Weaver, 57 Holosmith,
  58 Deadeye, 59 Mirage, 60 Scourge, 61 Spellbreaker, 62 Firebrand, 63 Renegade, 64 Harbinger,
  65 Willbender, 66 Virtuoso, 67 Catalyst, 68 Bladesworn, 69 Vindicator, 70 Mechanist, 71 Specter,
  72 Untamed. VERIFY this table against the EI golden JSON: for each friendly player in
  `fixtures/local/wvw-small.zevtc`, the mapped display name (elite spec name if any, else base)
  must match the EI JSON's `profession` for the same account. Fix any wrong id from that evidence
  (the fixture has 41 players across many specs — strong coverage). Unknown ids still fall back to
  the numeric string.
- Native schema: `profession` = base name, `elite_spec` = spec name ("" if core).
- EI adapter: emit `profession` = elite-spec name when present else base (EI convention), keep
  `elite_spec` field too.
- **Calibration test** (skip-when-absent pattern, name `professions_match_ei_golden`): decode the
  local fixture, join to `fixtures/wvw-small.ei.json` players by account, assert every joined
  player's EI-style profession equals the EI JSON's `profession` value. Extend
  `fixtures/wvw-small.ei.json` with a `profession` per player (extract from the axibridge EI JSON;
  it is already anonymized).

### Task 2: Real WvW map & team tables

**Files:** Modify `crates/axilog-core/src/wvw/mod.rs`; modify `crates/axilog-ei/src/lib.rs`.

**Requirements:**
- Map name from `MAP_ID` statechange (`src_agent` = map id): 38 "Eternal Battlegrounds",
  95 "Green Alpine Borderlands", 96 "Blue Alpine Borderlands", 1099 "Red Desert Borderlands",
  968 "Edge of the Mists"; unknown → "World vs World". Set `enc.map` in `wvw::apply`. Assert the
  fixture resolves to "Green Alpine Borderlands" (calibration test, skip-when-absent).
- Team-id→color: research GW2EI's mapping (fetch
  https://raw.githubusercontent.com/baaron4/GW2-Elite-Insights-Parser/master/GW2EIEvtcParser/ArcDPSEnums.cs
  or search that repo for `TeamID`/`2763`/`2767` — WebFetch is available to you). Embed the id
  sets for Red/Blue/Green. The fixture's friendly team 2767 must map to a color; enemy team ids in
  the log must map to different colors. Replace the 883/882/881 placeholder in `team_color` AND in
  `axilog-ei`'s `color_to_team_id` (emit a representative real id per color; wvWMapData should use
  the log's actual detected team ids where available, falling back to representative ids).
- `fightName` in ei-json becomes `"Detailed WvW - <map name>"`.

### Task 2b: Dynamic team ids via CBTS_WVWTEAMS (arcdps-dev guidance)

**Files:** Modify `crates/axilog-core/src/evtc/event.rs` (sc const), `crates/axilog-core/src/wvw/mod.rs`, `crates/axilog-ei/src/lib.rs`.

**Requirements (from arcdps dev via user, 2026-08-08):** prefer the `CBTS_WVWTEAMS` statechange
event to configure the log's actual red/blue/green team ids rather than hardcoding. Verify the
enum value from the arcdps EVTC reference (https://www.deltaconnected.com/arcdps/evtc/README.txt)
and how the three ids are packed in the event (src_agent/dst_agent/value fields). When the event
is present, team_color resolves from it; the static table from Task 2 remains the fallback for
older logs (the calibration fixture predates the event and must keep passing via fallback).
`wvWMapData` in ei-json uses the real ids when present. Unit test with a synthetic WVWTEAMS event.

**CBTS_IDTOGUID (arcdps-dev guidance):** content-local ids are session-local; arcdps emits
`CBTS_IDTOGUID` statechange events mapping them to stable GUIDs, with a content-type from
`n_contentlocal` { EFFECT=0, MARKER=1, SKILL=2, SPECIES_NOT_GADGET=3, TEAM=4, EMOTE=5,
TRANSFORMATION=6 }. Verify the statechange value + payload layout from the arcdps EVTC reference.
Decode IDTOGUID events into a `guid_map` on the model (at minimum content-type TEAM now —
store team-id→GUID so team identity is stable across logs; expose in native schema
`encounter.teams[].guid` as optional). SKILL/SPECIES mappings: decode and retain in `RawLog`
(a `Vec<GuidMapping>`) for M3 (stable buff/skill identity) even if unused now. If the fixture
log predates these events, unit-test with synthetic events and leave fixture assertions
conditional.

### Task 3: CC metrics from CROWD_CONTROL events + CBTS_STUNBREAK

**Files:** Modify `crates/axilog-core/src/analysis/cc.rs` (and `analysis/mod.rs` if needed).

**Requirements:**
- `is_cc(e)` = `e.is_statechange==0 && e.result == result::CROWD_CONTROL` (drop the overstack
  heuristic). CC events' `value` field = CC duration in ms (these events are already excluded from
  damage).
- `apply_cc`: per squad player (aggregated across account addrs like other metrics),
  `cc_applied` += 1 and `cc_duration_ms` += `value.max(0)` for each CC event vs an enemy
  (src resolution incl. pet-credit — check whether EI credits pet CC to owner; calibrate).
- Timeline `cc_applied` buckets use the same predicate.
- **Calibration test** (skip-when-absent, `cc_matches_ei_golden`): squad totals within 2% of
  EI: sum(cc_applied)=34, sum(cc_duration_ms)=50460. If pet-credit inclusion/exclusion is what
  makes it match, document which. Extend `fixtures/wvw-small.ei.json` with the two squad totals.
- **CBTS_STUNBREAK (arcdps-dev guidance):** verify the statechange enum value from the arcdps
  EVTC reference; track per-player `stun_breaks` (count) and, if the event carries it, removed
  stun duration. Native schema: add to the player cc block (`stun_breaks`,
  `removed_stun_duration_ms`). EI adapter: `defenses[0].stunBreak` (+`removedStunDuration` if
  computed) matching EI v3.24+ placement. Unit test with synthetic stunbreak events; if the
  fixture log contains any, sanity-check counts are nonzero and plausible.

### Task 4: Enemy dedupe + pet-attribution hardening

**Files:** Modify `crates/axilog-core/src/wvw/mod.rs`, `crates/axilog-core/src/analysis/damage.rs`.

**Requirements:**
- Dedupe enemy players across relogs by (account, else character) like `dedupe_players`,
  aggregating their `agent_addrs` into a representative Enemy (add `agent_addrs: Vec<u64>` to
  `Enemy`); the enemy id set used by analysis must still include ALL addrs; per_enemy maps fold to
  the representative enemy id. NPCs are NOT deduped (distinct spawns are distinct).
- Pet attribution: replace the log-wide last-write-wins `instid → owner addr` map with a
  time-aware resolution (track instid registrations in event order; when crediting an event at
  time t, use the registration active at t). Where the owner cannot be resolved, do not credit.
- Golden must remain exact (2,138,414). Add a unit test for time-aware instid reuse: two agents
  claim the same instid at different times; damage in each era credits the era's owner.

### Task 5: PII-safe committed fixture + CI-run golden

**Files:** Create `crates/axilog-core/tests/support/anonymize.rs` helper or a
`crates/axilog-cli` hidden subcommand `anonymize` (choose the simpler; a test-support binary
`cargo run -p axilog-cli -- anonymize <in> <out>` is acceptable and user-useful). Create
`fixtures/wvw-small.anon.zevtc` (committed). Modify `crates/axilog-core/tests/golden.rs`,
`wvw_partition.rs`, `crates/axilog-cli/tests/cli.rs`, `crates/axilog-core/tests/decode_fixture.rs`
to prefer the committed anonymized fixture (no skip needed in CI anymore); keep the local-raw
path as an optional extra check when present. Modify `.github/workflows/ci.yml` only if needed.

**Requirements:**
- Anonymizer: read a `.zevtc`, decode agent table, for each PLAYER agent rewrite the 64-byte name
  buffer: character → `Anon<N>`, account → `:Anon<N>.<4digits>` (stable by index), subgroup
  preserved; all other bytes byte-identical; re-zip (stored or deflate) so the file parses
  identically. Metrics must be IDENTICAL to the original (damage/duration/counts) — assert in a
  test when the local original is present.
- Anonymize the local fixture → `fixtures/wvw-small.anon.zevtc`; VERIFY by byte-scan that none of
  the original account/character strings appear; commit it. Golden tests now run everywhere,
  including CI. Update `fixtures/wvw-small.ei.json` `players[].account` to the anonymized
  accounts so the profession-join test still works (deterministic mapping by agent-table order).
- Keep `/fixtures/local/` gitignored.

### Task 6: EI adapter refresh + docs

**Files:** Modify `crates/axilog-ei/src/lib.rs`, `README.md`.

**Requirements:**
- ei-json: real teamIDs (from Task 2), elite-style profession naming (Task 1), CC fields
  (`statsAll`-subset or extend statsTargets entry with `appliedCrowdControl`,
  `appliedCrowdControlDuration` — match EI field names), map-derived fightName.
- README: usage section (parse, formats incl. table example), fixture policy (PII note),
  current parity status table (what matches EI, what's approximate), M2 feature list.
- Verify `cargo test --workspace` green; golden + new calibration tests pass.

## Self-Review
Covered all five spec items (names, tables, CC, dedupe/pets, fixture) + adapter/docs. Calibration
targets embedded verbatim (34/50460, map 95, team 2767, damage 2,138,414). No placeholders; each
task names files, method, and test. Type impacts (Enemy.agent_addrs) called out.
