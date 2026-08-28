# Changelog

Version-grouped from the milestone log that used to live in the README. Grouping is by git-tag
ancestry (`git log --oneline vA..vB`), so each milestone appears under the first release that
contains its merge commit. The forward-looking view — what is queued, parked and in flight — is
[`docs/ROADMAP.md`](ROADMAP.md).

Milestones are the unit of work in this project: spec → plan → subagent-driven execution in an
isolated worktree → adversarial review per task → whole-branch review → merge. Every entry below
kept the cross-cutting invariants green (existing calibration exact, no PII committed, deterministic
output, all suites passing).

## v1.8.0 — 2026-08-28

### Added
- **Per-second CC and boon-strip series.** `SquadSeries` gains a required
  `strips` lane, and `EntitySeries` gains `cc_applied`, `strips` and
  `strips_taken` — the last three gated on `timeseries: true` like the
  existing healing lanes. Every lane is folded from the primitive that
  already produces its scalar counterpart (`is_cc`,
  `support::outgoing_boon_strips`, `defenses::incoming_boon_strips`), and
  sum-invariant tests pin each series against that scalar. The equality
  holds within the encounter window: buckets are sized from
  `duration_ms`, so an event timestamped past the encounter end is dropped
  from the lane while still counting toward the scalar. This matches the
  pre-existing squad `cc_applied` lane, and the field docs on all three
  surfaces say so.
- `analysis::entity_series`, a new module building the per-entity lanes,
  indexed positionally over the `players` slice in the same order as
  `healing_detail`. `build_from(&enc, &raw, &metrics)` is the constructor
  callers should use; `build` remains available for callers that already
  hold the resolved roster.

### Changed
- `support::outgoing_boon_strips` and `defenses::incoming_boon_strips` now
  return `(time_ms, skillid, duration_ms)` rather than `(skillid,
  duration_ms)`. The added field is the **raw** event time, not
  log-relative — consumers must subtract `RawLog::log_start_ms()`. Both
  existing folds ignore it, so `strips`, `strips_duration_ms` and
  `boon_strips_taken` are unchanged for every fixture.

## v1.7.2 — 2026-08-27

### Fixed
- **Buff names no longer render as `"Skill <id>"`.** `skill_map::resolve_name`
  walked five rungs — the log's own skill table, `pseudo_name`, the GW2 API
  catalog, GW2EI's name overrides, then its `SkillIDs.cs` symbols — and not
  one of them is a buff table. A boon, condition, sigil, relic or food buff
  whose name the capturing client had not cached therefore fell all the way
  to the `"Skill <id>"` placeholder, even where this crate's own buff
  catalog named it in the same document. Most visibly **Resolution**, which
  a damage distribution could render as `"Skill 873"` while the buff catalog
  beside it said Resolution.

  `analysis::buffs::name` — which already composes the 12 boons, the 14
  conditions, the 2 control effects and GW2EI's 2,267-entry `BUFF_META` for
  the native buff catalog — is now a rung, placed above the symbol rung and
  below everything that carries a real skill name. Measured over a 407-log
  sample of a ~4,000-log WvW corpus: of the 29 ids still hitting the
  placeholder, it names 873.

  The ranking is the larger half. `BUFF_META` and `SKILL_SYMBOL_NAMES` both
  cover 2,260 ids and disagree on 1,430, and on those the buff table carries
  GW2's real display string where the symbol rung carries a de-camel-cased
  C# identifier — `"Clove and Veggie Flatbread"` over
  `"Clove And Veggie Flatbread"`, `"Marked (Blue Keep)"` over
  `"Marked Keep Blue"`. This rung is therefore deliberately not
  additive-by-construction, unlike its neighbours: it displaces a symbol
  1,430 times and can still never touch a name from the log table,
  `pseudo_name`, the API catalog or the override table. Both halves are
  pinned by `crates/axilog-core/tests/buff_name_rung.rs`.

## v1.7.1 — 2026-08-26

### Added
- **`personalDamageMods` — which damage modifiers are a player's OWN.** The
  EI-shaped export gained Elite Insights' top-level `personalDamageMods`
  (`JsonLogBuilder.cs:315`), and the native container its counterpart
  `blocks.damage_mods.personal`: a map from spec name to the signed
  modifier ids that belong to that spec rather than to the shared pool —
  relics, food, squad buffs — whose damage gain is credited to every player
  who benefited rather than to whoever provided it.

  **The omission was not cosmetic.** This is the ONLY field that draws that
  line, and a consumer holding an empty set has to choose between reading it
  as "unclassified" and as "nothing is personal". AxiBridge read it the
  second way and filtered every modifier out of its Damage Modifiers panel —
  blank for every natively parsed log, reported 2026-08-26. The data was
  fully computed the whole time; only the classification was missing. The
  parity table's old entry called the field "a pure re-index of data
  `damageModMap` and the per-player arrays already carry", which was wrong
  on both counts, and the reason it gave for skipping it — that the spec
  spelling was not reproducible here — was wrong too: `players[].profession`
  has carried exactly GW2EI's `Spec.ToString()` all along.

  GW2EI's rule is reproduced exactly, including the `SpecSpecificShared`
  subtlety that a shallower reading would miss: EI tests the descriptor's
  `Srcs`, not the source bucket `DamageModifiersContainer` filed the
  modifier under, so a `.UsingSpecSpecificShared()` modifier is OFFERED to
  everyone while counting as personal only for its own spec.

  Calibrated against the reference export (`fixtures/local/
  wvw-postrework.ei.json`, EI 3.27.0.0): the **same 14 spec keys**, and
  across all **69 ids both engines emit, zero classification disagreements
  in either direction**. Six ids the export calls personal are absent here
  because this engine's catalog does not emit those modifiers at all — a
  pre-existing coverage gap, asserted as such by the test rather than
  tolerated as slack. Rides `damageModMap`'s existing `--modifiers` gate:
  the two describe the same id space, so emitting one without the other
  would hand a consumer a partition of a table it cannot see.

### Fixed
- The native-json baseline digest had been red on `main` since the v1.7.0
  release, which bumped `axilog.version` without re-digesting. `"1.6.2"` and
  `"1.7.0"` are the same byte length, so the guard reported the confusing
  half of its failure — length matched, digest did not. Proven to be the
  version string and nothing else before regenerating: the pre-change
  capture contains exactly one occurrence of `"1.7.0"`, and substituting
  `"1.6.2"` back for it reproduces the old digest byte for byte.

## v1.7.0 — 2026-08-26

### Added
- **Cleanses and strips counted the way the in-game arcdps meter counts
  them.** Six new `SupportMetrics` fields — `cleanses_arcdps`,
  `strips_arcdps`, and a `_by_minion` / `_on_minion` bucket for each — filled
  by a new `analysis::arcdps_parity` pass. It is a transcription of the
  meter's own counting code, supplied by deltaconnected (arcdps' author), not
  an adjustment layered on Elite Insights' number: it drops single-stack
  stability removals, drops self-consumed blinds, folds pets into their
  master on both the holder and remover side, and subtracts the self-removal
  burst a player produces on going down. EI-shaped keys are
  `condiCleanseArcdps*` / `boonStripsArcdps*`.

  **Three buckets, not one, and that is the load-bearing design decision.**
  There is no single "arcdps number" — the meter's displayed total depends on
  that window's "vs npcs" / "from npcs" inclusion toggles, which the reader
  sets. On `fixtures/wvw-small.anon.zevtc` the buckets are 900 / 46 / 157, so
  the toggle choice moves the total by 26%; a hardcoded single number would
  be wrong for most readers. Sum rule: base alone = both toggles off,
  `+ _on_minion` = "vs npcs", `+ _by_minion` = "from npcs".

  Three independent, untuned measurements corroborate the transcription. The
  base bucket reads **900 against EI's 898** — the same population, 0.2%
  apart, a prediction that could easily have failed. `_on_minion` = 46,
  *exactly* what the independently-derived `cleanses_minions` counter finds
  by a completely different route, which identifies the long-reported +3-4%
  field gap as the "vs npcs" bucket. And the EI-minus-arcdps strip residue is
  precisely 14, the stability single-stack population.

  The pass is purely additive: it writes no existing field, and the
  native-json leaf diff is 252 added, 0 removed, **0 changed**, so every
  EI-calibrated golden is untouched.

### Fixed
- **The down-undo readback is a bounded chain walk, not a time window.**
  Corrected against further detail from deltaconnected: step back from the
  down over rows between the downed agent and itself, ending at the first
  entry that is not a buff removal, walking *through* buff damage
  (`statechange == CBTS_COMBAT && buff` — arcdps' chain locks around the
  condi sim loop and a log does not, so ticks land between the server's
  buffremove rows), any "determined" apply, and zero-source server rows.

  Two adaptations are forced by reading a log rather than arcdps' live
  buffer, and both were measured against all 25 downs in the fixture rather
  than assumed. Statechange rows are skipped rather than treated as chain
  entries — breaking on them truncated 4 of 25 bursts to nothing, every one
  on an `sc == 62` row that is not a buff event at all. And the walk stays
  bounded: arcdps' chain is a bounded live ring buffer, a log has no horizon,
  so unbounded one burst reached 23 rows against a true 10. Bounded and
  skipping statechanges, the chain agrees with the previous plain window on
  all 25 downs, so the calibration and the native-json digest are unchanged.

  Also: **"determined" is not one id.** The catalog carries three — 762, 785
  and 788 — all sharing one wiki icon. The reference's
  `SKILL_DETERMINED_PLAYER = 788` is correct for the training golem it was
  read off; WvW player downs carry 762, and 788 appears there on a non-player
  agent. All three are walked through.

- Node and Python stubs told consumers to sum
  `cleanses + cleanses_self + cleanses_minions` for arcdps parity. That is
  not how arcdps counts; they now point at the `cleanses_arcdps` family.

## v1.6.2 — 2026-08-26

### Fixed
- **The tail of `Skill <id>` placeholders, closed from one table instead of
  one row at a time.** v1.6.1 fixed the naming *chain* and was verified on
  the committed fixtures. Measured afterwards against a real ~4,000-log WvW
  corpus — the evidence the fixtures could not supply, because none of them
  contain these ids — **22 distinct ids still rendered as the literal
  placeholder**. `resolve_name` gains a fifth and final named rung,
  `skill_symbol_names`, generated from every `const long` in GW2EI's
  `SkillIDs.cs`: 5,568 positive ids, de-camel-cased, so
  `GladiatorsDefenseAnimation` becomes "Gladiators Defense Animation".
  Measured on the same 60-log sample, distinct placeholders fall from **38
  to 20**.

  A symbol is not a display name, and that is the point of ranking it last.
  `SkillIDs.cs` exists so GW2EI's own code can refer to an id, not so a
  player can read it, so the result is clumsy where a real name would not
  be — but the comparison at this rung is against `Skill 23288`, never
  against a name. The rung sits below the log's own skill table, the pseudo
  names, the GW2 API catalog and GW2EI's `OverridenSkillNames`, and
  immediately above the placeholder, so it is **additive by construction**:
  it can only displace the placeholder, never a name a higher-authority rung
  produced. That property is guarded directly rather than asserted —
  `symbol_rung_never_displaces_a_higher_rung` walks all 5,568 entries and
  checks both directions.

- **Weaver dual-attunement ids are named.** `41166`/`42264`/`43470`/`44857`
  were a ledgered known gap expected to need a bespoke `WeaverHelper.cs`
  port; they now resolve to "Dual Water/Air/Fire/Earth Attunement" from the
  same generated table, so the follow-up is closed rather than deferred
  again.

### Known limits
- **20 ids still render as `Skill <id>`, and cannot currently be named.**
  Verified individually: they are absent from the log's skill table, from
  `/v2/skills` (which does answer for real ids — `5491` returns Fireball),
  and from every file in GW2EI's parser sources. Nothing available names
  them, so they keep the placeholder deliberately: an honest "we do not
  know" beats inventing a label.

## v1.6.1 — 2026-08-26

### Fixed
- **Healing skills render their real names instead of `Skill <id>`.** A WvW
  player reported eleven skill ids showing as the literal placeholder
  `Skill 13721`, `Skill 1066` and so on in AxiBridge's Support & Healing
  tables. The cause was structural rather than a missing entry: the catalog
  that names ids for the v1 document could reach the GW2 API name table and
  the curated pseudo-id names, but **not the log's own skill table** — the
  one source that names an id no public catalog covers. Damage skills were
  unaffected because they are almost always in the API catalog; a skill that
  only ever heals frequently is not. So the gap presented as "healing skills
  have no names" while actually being "one naming rung was unreachable from
  this call site."

  `Metrics` now carries the log's skill-table text (`log_skill_names`),
  ungated — a name is a property of the log, not of which analysis passes
  were requested — and `skill_map::resolve_name` became the single shared
  chain both the skill and buff catalogs terminate in. Any future block that
  references an out-of-scope id is named for free, which was the point: the
  request was explicitly for a systemic fix rather than eleven special cases.

  The full ladder, in order: the log's own skill table (rejecting empty and
  all-digit strings, since arcdps writes a bare numeric placeholder for some
  ids) → curated pseudo-id names for negative synthetic ids → the GW2 API
  `skill_icons` table → GW2EI's `OverridenSkillNames` → `Skill <id>`.

- **Ids ArenaNet never published are named from GW2EI's override table.**
  Id `1066` (`Resurrect`) is the case that proves the rung is needed: it is
  absent from `/v2/skills` entirely, and arcdps writes its name into the log
  as the literal string `"1066"`, which the chain correctly rejects as a
  non-name. A new generated catalog, `skill_name_overrides` (293 positive
  entries transcribed from GW2EI's `OverridenSkillNames`, 332 considered /
  39 non-positive skipped), supplies it.

  **That table ranks LAST by measurement, not by convention.** Ranked first,
  it would have renamed 17 ids that an earlier rung had already resolved
  correctly, against only 2 ids it actually rescued from the placeholder — so
  it is demoted to a rung that can only ever displace `Skill <id>`. The
  shipped chain's zero-rename property (`MAX_RENAMES = 0`) and the
  `skill_icons`-beats-overrides ordering on all 35 double-covered ids are
  both pinned by `crates/axilog-core/tests/name_override_precedence.rs`.

- **Healing-over-time effects now resolve in both catalogs.** Following
  GW2EI's own routing, an indirect (buff-based) healing row's id is
  referenced into `catalogs.buffs` as well as `catalogs.skills`, so a report
  reader that looks in either map finds it. `BuffEntry::name` shares the same
  resolution chain, replacing the empty string those entries used to default
  to — 13 of the 15 new buff entries on the reference fixture were empty
  before. The EI-shaped `buffMap` is correspondingly no longer just the 12
  tracked boons.

- **Healing skills get correct `isSwap`/`canCrit` flags.** Ids the skill map
  never covered previously fell back to hardcoded defaults, which assumed
  every such skill could crit. Both flags are now computed from the id by the
  same pure functions the skill map itself uses, so covered and uncovered ids
  cannot disagree.

### Testing
- `crates/axilog-ei/tests/name_leak_golden.rs` walks every id-bearing row in
  the EI-shaped output — nine arrays, 6,520 rows with all gates on — and
  fails the build if any id renders as a placeholder or an empty name. Each
  array carries an independently-stated non-vacuity floor, so an array
  silently dropping out of the walk is itself a failure. This is the guard
  that stops the class of bug from returning rather than just this instance.

### Notes
- **The honest residual.** Of the eleven reported ids, five are confirmed
  against committed fixtures (`1066 → Resurrect`,
  `13721 → Restorative Mantras`, `53183 → Illusionary Inspiration`, plus
  `30301` and `77020` decoded from the reporter's own log). The remaining six
  — `14219`, `28313`, `45103`, `72365`, `76947`, `78971` — appear in no
  fixture available to this project. They have exactly one possible naming
  source left, the log's own skill row, so they are fixed **iff** arcdps
  wrote a real string for them in that log. If arcdps wrote a numeric
  placeholder instead, as it did for `1066`, they will still show as
  `Skill <id>` and will need an override-table entry.
- **The invariant is an allowlist, not zero.** On the committed fixture the
  output carries 508 skill and 205 buff catalog entries with **7 placeholders
  remaining**, all seven on a documented allowlist: four Weaver dual-attunement
  ids (`41166`, `42264`, `43470`, `44857`), nameable from GW2EI's buff table by
  a rung deliberately deferred to a later release, and four (`30060`, `31311`,
  `54960`, `69665`) that the live `/v2/skills` API reports as non-existent.
- **Size.** The `catalogs` block grows 146,068 → 148,645 bytes on the
  reference fixture (**+2,577, +1.76%**) — short name rows against a document
  whose bulk is replay data.
- AxiBridge picks this up with a dependency bump; no application-side change
  is required.

## v1.6.0 — 2026-08-23

### Added
- **`cleanses_minions` — the arcdps-parity cleanse bucket GW2EI throws
  away.** A player reported that AxiBridge showed fewer cleanses than the
  in-game arcdps meter for the same fight, same recorder, same start time.
  It is not a counting bug on either side. GW2EI's
  `SupportStatistics.ConditionCleanseCount` accumulates inside
  `foreach (Player p in log.PlayerList)` (`SupportStatistics.cs:61`), so a
  condition cleansed off a **ranger pet, necro minion, mesmer clone or
  revenant spirit is counted zero times**. The in-game meter has no
  `PlayerList` concept — it folds pets into their master — so it counts
  them. That single exclusion is the entire discrepancy.

  The fix is a new `SupportMetrics::cleanses_minions` counter, surfaced as
  `blocks.support.by_entity.<id>.cleanses_minions` on the v1 wire schema
  and as `condiCleanseMinions` on the EI-shaped output. When the cleanse
  recipient fails the `real_players` test, its `src_master_instid` is
  resolved through the pass's shared `InstidRegistry` (single-hop, the same
  resolution `minions::build` and `dist_outcomes` already use); if the
  master is a squad player, the removal lands in the new bucket. Consumers
  wanting arcdps parity sum `cleanses + cleanses_self + cleanses_minions`.

  **Scope was calibrated against a real log, not assumed.** The obvious
  reading — "arcdps just doesn't have EI's `PlayerList` guard, so drop it"
  — is wrong. Bucketing every buff-removal row on
  `testdata/20260128-190427.zevtc` (34 squad accounts) by recipient class
  gives `self 1009, squad player 3464, non-squad player 479, minion of
  squad 151, minion of other 12`. So:

  | definition | total | vs EI |
  |---|---|---|
  | EI-equivalent (`self` + squad) | 4473 | — |
  | **+ minions of squad** | **4624** | **+3.38%** |
  | + all minions | 4636 | +3.64% |
  | + all friendlies (drop the guard) | 5115 | +14.35% |

  Two independent field reports put the arcdps/EI gap at +3.3% and +4.1%.
  Only the minions-of-squad definition lands there; dropping the guard
  overshoots by 4x. arcdps tracks the squad and its pets, it does not adopt
  unrelated friendlies — so non-squad friendly *players* stay uncounted
  (they have no master, so `resolve_at` yields `None` and they fall
  through). `iff` was `Friend` on 100% of squad-remover rows, ruling out
  enemy contamination. Both `BuffRemoveSingle` (3768) and
  `BuffRemoveManual` (38739) remain ignored, as in EI: counting either
  would put the gap above 40%, not 4%.

  **Every existing number is bit-identical.** The new bucket is a separate
  field, never folded into `cleanses`, so `support_matches_ei_golden`
  passes unchanged. Verified structurally as well as by suite: the full
  native JSON for the fixture was diffed leaf-by-leaf against a build of
  the parent commit — 42 differences, every one an *added*
  `cleanses_minions` key, nothing removed and no existing value moved.
  On the AxiBridge fixture the field reads +3.85% squad-wide (+2.1% to
  +5.2% per player), and the per-player counts match an independent probe
  written directly against the raw EVTC bytes.

  Note for consumers: this is **not** a GW2EI field, and a document
  produced by any other parser will not have it. A missing key must be
  treated as "unavailable", not as zero — reading it as zero silently
  relabels an EI-scoped number as an arcdps-scoped one.

## v1.5.1 — 2026-08-23

### Fixed
- **WvW team resolution no longer splits the squad in half.** `wvw::resolve_teams`
  built its agent -> team map **last-write-wins**, on an explicitly stated
  assumption in the code that "every agent gets exactly one `TEAM_CHANGE`
  event". That assumption is false, and the extra events are not mid-fight
  noise: they are emitted at log *teardown*, as the recording player zones out
  of the map. Since `friendly_team` is the recorder's team, one trailing stamp
  decided friend/foe for the entire log.

  In the reference log (`20260822-192239.zevtc`, duration 82623 ms, last real
  combat event at t=68307 ms) the recorder emits
  `[(1282, t=66760), (433, t=68512), (2543, t=82335)]`. Last-write-wins picked
  **2543** — a team nobody fought on, stamped 14 seconds after combat ended.
  Only the 20 agents still in tracking range at that instant carried the
  matching stamp and stayed in `players[]`; the other 25 squad members fell to
  the `else` branch and were emitted as enemies, while the 40 real enemies
  dropped out of the report entirely. Per-agent histograms show the split
  cleanly — FIRST team `{886: 40, 1282: 45}`, LAST team
  `{707: 1, 886: 39, 1282: 25, 2543: 20}`.

  Resolution is now **first-write-wins**, with team `0` treated as a
  placeholder rather than a team: prefer any real id over it, but keep it when
  it is all an agent ever emits (a disjoint set of 45-of-243 agents in the
  reference log are `0` on every event they emit, never mixed with a real id).

  Healthy logs differed only in that the recorder's trailing event happened to
  still carry its real team, so this was a coin flip on whether a map
  transition landed inside the recording window — not a property of the fight
  or the map. Verified across all 12 EotM logs from the reporting session:
  every fight now resolves a consistent roster against red enemies (192239:
  45 squad / 40 enemy, was 20 / 25; 193429: 46 / 43, was 10 / 36), and the 10
  previously-healthy logs are unchanged.

  The M4 Keep Lord `iff` override's comment asserted last-write-wins and is
  corrected. The override is deliberately kept: the Lord now resolves hostile
  on its own, but `iff` remains the stronger signal and still covers an NPC
  whose first observed team is already friendly. Both new regression tests were
  flip-tested against the rules they pin.

  Note this does not retroactively correct anything already parsed and
  published — affected logs must be re-parsed with this build.

## v1.5.0 — 2026-08-21

### Added
- **PvE encounter identification.** Every log this project had ever parsed
  came out as a WvW log: `model::resolve` hardcoded `kind: "wvw"` and
  `map: "World vs World"`, and `axilog_ei` rendered `fightName` as
  `"Detailed WvW - {map}"` unconditionally. A raid, a strike and a fractal
  were therefore indistinguishable from each other and from a borderlands
  zerg — axibridge listed a night of Wing 1–4 raids as four "World vs World"
  fights, which is how this was found.

  arcdps records exactly one fact about the encounter: the trigger species
  id, in bytes 13–14 of the evtc header. The new `pve` module turns it into
  a name, a category and a success verdict:

  - **Name.** GW2EI's *default* rule (`LogLogic.GetLogicName`) is "the
    character name of the target whose species is the trigger id" — the
    boss's own agent name, already in the log. So the general case needs no
    table, and names encounters GW2EI has never heard of as readily as ones
    it has. `pve::encounters` is the correction layer: a generated
    transcription of GW2EI's 90 `LogData.DetectLogic` cases, supplying the
    category (which nothing in the log states) plus a fixed name for the 38
    fights that are not named after any one agent — "Twin Largos", "Bandit
    Trio", "Harvest Temple", "Siege the Stronghold".
  - **Category.** `Encounter::kind` now carries GW2EI's own `LogCategory`
    slug: `raid_wing`, `raid_encounter`, `fractal`, `golem`, `story`,
    `open_world`, `convergence`, `unknown_encounter`, or `wvw` as before.
    EI's vocabulary is kept verbatim rather than collapsed to
    "raid"/"strike", because `RaidEncounter` spans festival bosses, IBS/EoD
    strikes *and* the SotO and Visions of Eternity encounters.
  - **Success.** Only GW2EI's *generic* rule (`SetSuccessByDeath`): every
    agent of the trigger species died. Asymmetric on purpose and documented
    as such — `true` is reliable, `false` is not, because encounters GW2EI
    succeeds by reward chest or scripted event (Twisted Castle, River of
    Souls, the Hall of Chains statues) report `false` on a clean kill.

  New wire fields on `encounter`, all omitted for WvW so no existing
  document changes shape: `encounter_name`, `trigger_id`, `sub_category`,
  `success`. `map` is now an EMPTY STRING for a PvE log — `map_id` is still
  the real instance id, but there is no PvE map-name table here and the WvW
  fallback string was the lie being fixed.

  Thirteen anonymized fixtures are committed (`fixtures/pve/`, 18 MB) — the
  repo's first PvE goldens, and the reason this class of bug could survive a
  fully green suite for as long as it did. They are picked to exercise
  branches, not to be a pile of raid logs: four `LogCategory` kinds, eight
  sub-categories, a matched kill/wipe pair on the same boss (Keep
  Construct), the gadget-triggered Dragonvoid (which is also the one fight
  named from the catalog rather than from its own agent), a conditional
  `DetectLogic` case (Xera), and three golem benchmarks.

### Fixed
- **`targets[]` was empty on every PvE log, and would have been 265 rows
  long.** Target selection was written twice and the two copies disagreed
  the moment non-WvW encounters became possible: `build_report`'s
  `ei_targets` gated on `kind != "wvw"` (which would have kept *every*
  ambient NPC in a raid instance — 265 of them on the Gorseval fixture,
  including 46 "Spirit Energy" and 21 crows), while `v1::entities`'
  `SourceOrder` filtered on `is_player` with no gate at all. `axilog_ei`
  reads the row COUNT from one and the ROWS from the other, so the
  disagreement rendered as an empty `targets[]` rather than as any error.
  Both now call one predicate, `Encounter::is_ei_target`, and a PvE log
  reports its boss.
- **Damage modifiers ran in the wrong mode for PvE.** `ModeContext::
  from_encounter` mapped anything not `"wvw"` to `ParseMode::Unknown` +
  `SkillMode::PvE`, under a comment noting that no such encounter existed.
  It now transcribes GW2EI's per-category assignment, so raids, strikes,
  fractals and convergences get `ParseMode::Instanced` — several modifiers
  are WvW/sPvP-only or are dropped outside instanced content, so the old
  answer selected the wrong modifier set.

### Known gaps
- **No challenge-mote detection.** A CM Skorvald is named `"Skorvald"`,
  where GW2EI says `"Skorvald CM"`. GW2EI decides this per encounter, with
  45 bespoke detectors keyed on boss health pools, specific skill casts and
  game-build gates; none of that is transcribed. A few encounters get it
  free, because ArenaNet gave the challenge version its own species id
  (`MinisterLiCM`, `DecimaCM`, the Old Lion's Court prototypes).
- **No per-encounter success rules.** The fixtures do cover the negative
  (five of the thirteen are failures, including a matched Keep Construct
  kill/wipe pair), but the rule itself is still only GW2EI's generic one.
  Two measured near-misses worth knowing:
  - GW2EI's `Golem.CheckSuccess` counts a benchmark as a success if the
    golem died **or** ended below 2% health. The death half is implemented;
    the two aborted golem fixtures end at 97% and 60%, so they agree with
    GW2EI by a wide margin — but a benchmark ending at, say, 1.5% would
    diverge. `golem_success_agrees_with_gw2ei_2_percent_rule` measures this
    rather than assuming it, and is where such a capture would surface.
  - Kanaxai's trigger id IS the challenge-mode species, and the fixture
    still reads `"Kanaxai, Scythe of House Aurkus"` with no suffix — which
    is EI-exact, because its `GetLogMode` returns `Mode.CMNoName` and
    `CompleteLogName` only appends for `CM`/`LegendaryCM`. Correct here by
    coincidence rather than by implementing anything.
- **No PvE key-set golden.** `v1-keyset.golden.txt` is built from the WvW
  fixture, so the four new `encounter` keys are absent from it by
  construction and `v1_sdk_stubs` cannot see them; they are listed
  explicitly in `PVE_ONLY_WIRE_FIELDS` as a stopgap.
- **No PvE target analysis beyond the trigger species.** GW2EI promotes
  split phases, adds friendly NPCs and names sub-targets per encounter;
  `targets[]` here is the boss and nothing else.

## v1.4.1 — 2026-08-21

### Fixed
- **Post-SotO elite specs 77/78/79 are named.** `profession_name`'s table had
  holes at three elite-spec ids, so Antiquary (Thief), Galeshot (Ranger) and
  Conduit (Revenant) all resolved to an empty `elite_spec`. Downstream that
  does not read as "unknown" — it reads as *core build*, so a Conduit
  rendered as a plain "Revenant" and a Galeshot as a plain "Ranger". Silent
  by construction: the output looked like valid data.

  The three ids were placed by elimination against the arcdps agent table's
  raw `prof` column in two independent fixtures — `wvw-small.anon.zevtc`
  (Thief/77, Revenant/79) and a WvW capture carrying Ranger/78 — leaving
  Thief, Ranger and Revenant as the only professions still missing a
  post-SotO spec and those three as the only unplaced names. `icons.rs`
  corroborates independently: its catalog is grouped by profession and had
  already filed the three names under exactly those professions.

  Two pins moved with the fix. `unmapped_elite_spec_falls_back_to_the_base_
  profession` used id 79 as its stand-in for "unnamed", so once Conduit
  shipped in game that test had quietly become a regression test *for* this
  bug; it now uses id 255. `no_agent_reports_a_numeric_elite_spec` pinned the
  known-unnamed population at 5 agents and is now empty, which is the outcome
  it existed to detect. Machine-diffed against the previous baseline: 206,050
  leaves both sides, nothing added or removed, exactly 5 leaves changed, every
  one an `elite_spec` blank → name.

## v1.4.0 — 2026-08-20

### Added
- **GW2 API skill names.** `skill_icons` gained a second generated table,
  `SKILL_NAMES` (4,610 of the API's 4,702 skills), and `skill_map::
  resolve_name` consults it before falling back to the `"Skill <id>"`
  placeholder. arcdps writes no display name for a skill the client had not
  cached, so a rotation entry could render as `"Skill 14404"` where the
  skill is Signet of Might. Ranked BELOW the log's own table on purpose: it
  can only displace the placeholder, never rename something the log already
  named. `catalogs::finish` gained the same rung for referenced-but-unmapped
  ids, which had their own copy of the placeholder. Machine-diffed against
  the previous baseline: exactly 61 leaves moved, every one a
  `catalogs.skills.*.name`, every one placeholder → real name.

- **`blocks.squad_buffs`** — squad-side uptime for every buff that is
  neither one of the 12 boons nor a condition/control effect: sigils,
  relics, food, utilities, auras, signets, trait buffs. `uptime_pct` and an
  optional `avg_stacks` per (squad player, buff).

  This is the third and last piece of the population Elite Insights keeps in
  one `buffUptimes` array. `blocks.boons` owned the 12 boons and
  `blocks.self_effects` the 16 conditions and control effects; the long tail
  had no home at all, so an EI-shaped consumer reading it got nothing.
  Measured on one real WvW log: EI emitted 36 `buffUptimes` rows for its
  first player where axilog emitted 12.

  ALWAYS-ON, unlike its two siblings: it emits uptime only, at the cost
  `blocks.boons`' own always-on half already carries. The three id sets are
  disjoint by construction, which is what lets the EI adapter concatenate
  `blocks.boons` and this block back into EI's single array.

  Calibrated against a real Elite Insights export: **1,196 cells, agreeing
  to 0.0005pp / 0.00055rel — the floor EI's own `Math.Round(x, 3)`
  serialization imposes** — outside 11 enumerated ids whose residual is
  locked by its own test rather than hidden under a loose tolerance. Two
  GW2EI rules are ported with it: `BuffClassification.Hidden` buffs are
  dropped from the array (`JsonPlayerBuilder.cs:266-269`), and a non-Weaver
  elementalist's four Weaver dual-attunement ids are deleted
  (`ElementalistHelper.RemoveDualBuffs`) rather than reported as duplicates
  of that player's plain attunement rows.

### Fixed
- `buffs::stack_type_for` consulted only `damage_mods::catalog::buff_stack`,
  whose own module doc says it is not a general buff catalog. Every
  conditional-loss buff outside it silently skipped the `RemovedDuration`
  band aid and simulated with the wrong stack count. It now falls back to
  the generated GW2EI catalog, which took Unblockable from 0.293 relative
  error against EI to the serialization floor.

## v1.3.0 — 2026-08-19

### Added
- `blocks.self_effects` — what was on a SQUAD player. The 14 conditions plus
  Stun (872) and Daze (833), each with `uptime_pct`, an optional
  `avg_stacks`, and a fused `states` stack timeline. Rides the existing
  `--timeseries` gate; one gate, so `coverage.self_effects` settles the
  whole question.

  This closes a real hole rather than adding a convenience. `blocks.boons`
  covered squad players but only the 12 boons; `blocks.conditions` covered
  the 14 conditions but only on enemies; nothing answered "what was on me".
  `blocks.cc` is not a substitute — it counts crowd-control events, a
  different measurement that carries no timeline — so a consumer rendering
  hard- and soft-CC timelines had nothing to read and rendered empty lanes.

  Stun and Daze needed a new id table (`analysis::control_catalog`): Elite
  Insights classifies both `Other`, so neither appeared in any table here.
  Both are duration-stacking with capacity 1, measured off the log's own
  `sc::BUFF_INFO` rows and agreeing with EI's `buffMap`. They are also the
  first buffs this project simulates at capacity 1, which reaches an arm of
  `run_duration` that no boon (minimum capacity 5) and no condition (minimum
  3) ever did; that arm already implemented the correct `ForceOverrideLogic`
  semantics, and only its comment needed correcting.

  Calibrated against a real Elite Insights export's per-player
  `buffUptimes`: values and key set both.

## v1.2.0 — 2026-08-18

### Added
- The arcdps healing-extension ROSTER — which players' own addon reported,
  GW2EI's `RunningExtension`. `blocks.healing.by_entity[].runs_extension`
  carries it per player, `blocks.healing.extension` carries the addon's
  registration descriptor (version/revision/signature), and `--format
  ei-json` reproduces GW2EI's `usedExtensions[]` entry from both.

  This was the one healing fact the native format could not answer, and its
  absence was not cosmetic: a consumer marking healing numbers "complete"
  vs. "partial" had nothing to key on, so an EI-shaped consumer that read
  `usedExtensions` flagged EVERY player partial on an axilog-parsed log.
  Nonzero healing is not a substitute — a peer's addon relays heals on other
  players' behalf, so 46 players had healing numbers on one real 55-player
  log where only 26 were running the addon.

  Roster verified EXACT against Elite Insights on 8 real WvW logs (zero set
  differences); see `docs/EI-PARITY.md`.

## v1.1.0 — 2026-08-18

### Added
- `axilog-api`: a shared `parse_report_v1` entry point. The native paths of
  `axilog-node` (`parseFile`, `parseBuffer`) and `axilog-cli` (`--format
  json`) now call it instead of each hand-rolling the same ~90-line analysis
  orchestration. The ei-json paths (`parseFileEi`, `--format ei-json`) and
  `axilog-py` still hand-roll their own: ei-json needs a per-target
  `damage_mods` split and the replay inputs that the facade does not
  compute.
- `blocks.series.by_entity[].healing_received_1s` and
  `.barrier_received_1s`: cumulative per-receiver healing and barrier, on
  the same 1s grid as `healing_1s` and behind the same `timeseries` gate.

### Changed
- `parse --format json` no longer filters `recorded_by_unresolved` out of
  stderr. Every `ReportV1` warning now reaches stderr unfiltered, so a log
  that trips this diagnostic emits a `warning:` line 1.0.0 suppressed.
  Stdout is unaffected.

### Unchanged
- ei-json output is byte-identical to 1.0.0. The compat projection gains
  nothing from this release.

## v1.0.0 — 2026-08-18

**1.0 is a promise, not a feature.** Nothing in this release changes an output — the native
format has been `"1.0"` since v0.3.3 and every metric is still asserted against a real reference
export in CI. What changes is what the version number commits to. From here:

- **Native format 1.0 is frozen.** Fields are added, never removed or re-meaninged, inside the
  `1.0` schema string. The one breaking change of the 0.3 line — `down_contribution` splitting
  into `downs_contribution`/`downed_by` — is the last of its kind before a 2.0.
- **The Node and Python surfaces are stable.** `@axiapps/axilog` and `axilog` on PyPI ship the
  same core as the CLI, as native extension modules rather than subprocess wrappers, and their
  exported names are covered by semver from this tag on.
- **The CLI's flags and formats are stable.** `axilog parse --help` stays the measured reference
  for what each flag computes and costs.

This entry also covers v0.3.7 through v0.3.12, which shipped without their own changelog sections:

- **Icons for every buff, condition, skill and marker.** `catalogs.buffs` entries carry icons
  (v0.3.7), GW2EI's skill-icon override catalog is applied (v0.3.9), boon and condition icons are
  traced from the GW2 wiki art rather than hotlinked (v0.3.11), and every icon the format emits is
  mirrored onto a domain we control (v0.3.12) — so a consumer rendering an axilog report is not
  one upstream deletion away from a page of broken images.
- **Squad markers and commander tags decode.** Marker and commander-tag GUIDs resolve to names
  (v0.3.10), and ground-placed squad markers come through with their positions (v0.3.11), which
  is what lets a consumer draw them on a replay map at all.
- **Account names lost a leading colon.** arcdps prefixes account names with `:`; axilog now
  strips it, so `":Player.1234"` no longer has to be cleaned up by every consumer independently
  (v0.3.7).
- **The release pipeline checks every version site before publishing, not after.** A stale
  `index.js` literal shipped to npm in v0.3.12 and was reported after it was unfixable; the
  pre-publish gate exists so that specific failure cannot recur.

## v0.3.6 — 2026-08-17

**The native container now carries the log's `t0` as `encounter.log_start_ms`.** Two fields in
the 1.0 document are deliberately NOT log-relative — `markers[].time_ms` and
`entities[].commander.segments` — because both are raw arcdps event times and clipping them into
the fight window would destroy real information. Their doc comments told a consumer to "rebase by
the log's `t0`", but nothing in the container was that `t0`, so from JSON alone the rebase was
impossible: arcdps session time has no fixed origin, which left both fields incomparable against
`duration_ms`, against each other across logs, and against anything else. On the committed fixture
the commander's segments read `[[33847418, 33847418], [33847418, 33896600]]` against a
`duration_ms` of `49285` — nothing in those numbers says which base they are in.

`encounter.log_start_ms` is that origin, sourced from `RawLog::log_start_ms` via a new
`Encounter::log_start_ms`. Purely additive, so 1.0 stays frozen and no existing field changes
meaning. The rebase is still left to the consumer rather than done at the serialization boundary,
because a commander tag held before the log's first event rebases negative and `u64` cannot carry
that — clamping here would silently lose data the consumer can represent correctly.
`v1_shape::the_two_session_time_fields_rebase_into_the_fight_by_log_start_ms` asserts the round
trip closes on the real fixture rather than arguing it in prose.

## v0.3.5 — 2026-08-17

**Native map geometry — `encounter.map_id` and `blocks.replay.tracks.arena`.** The native
format carried replay positions as raw world (game-inch) coordinates and nothing to plot them
with: no map id, no projection. The only place the per-map world rect existed on the way out was
`analysis::ei_replay::MapTransform`, i.e. inside the EI compatibility adapter — so a consumer
reading native could not draw a map without also parsing ei-json, or without re-transcribing
`wvw::maps::WVW_MAPS` into its own codebase and owning the drift.

Both are now emitted natively. `encounter.map_id` is the raw `CBTS_MAPID` value the `map`
display name is derived from, present with or without `--replay`, for consumers joining against
their own map assets. `blocks.replay.tracks.arena` carries the arena image's native size and URL
plus the world rect it covers, so world → pixel is a four-line formula the consumer can apply at
any canvas size. Nothing is pre-rounded or pre-rescaled: GW2EI's `combatReplayMetaData` squeezes
`sizes` to a 750px maximum dimension and rounds `inchToPixel` to three decimals, both artifacts of
its renderer, and both derivable from `arena` rather than the reverse. Scaling `arena` to EI's own
canvas reproduces EI's pixel on all five mapped ids, asserted against `MapTransform` itself
(`arena_tests::projection_reproduces_gw2eis_transform_on_every_map`) rather than argued in prose.

`arena` is omitted for a map id with no hand-authored arena image; the consumer then has only
`bounds`, which is the union of the observed positions rather than a fixed frame and is therefore
not comparable across logs.

**1.0 is frozen.** `docs/NATIVE-FORMAT.md`'s compatibility rules were explicitly suspended while
the format had no external consumer, with the suspension written to end "the moment one does". It
has: axibridge reads the native document in production as of its unit-2 cutover. Renames,
removals, retypes and meaning changes now require a major bump, gated by the key-set golden. This
release's additions are additive — nine new keys, none removed.

## v0.3.4 — 2026-08-16

**MSDELAY — the instant-cast server delay constant.** `SERVER_DELAY` was 150 ms; arcdps' own
figure is 10 ms. The two casts the wider window had been inventing were both spurious, so the
per-skill absolute error against the rotation golden went 54 → 52 and skills 56873 and 78358 went
from one cast over the golden to exact. Instant-cast coverage reads 338/364 rather than 340/364
for the same reason: fewer casts, none of them real losses.

Also in this release: the Windows `LNK1201` link failure (the CLI binary and the Python cdylib were
both named `axilog`, so their PDBs collided), and the 0.3.3 lockfile refresh.

## v0.3.3 — 2026-08-16

The native output format program, end to end. **Container 1.0** — six top-level keys, an
`entities[]` roster with roles and deterministic ids, referenced-ids-only `catalogs`, gated
`blocks`, honest `coverage` states and a raw/RLE series envelope — emitted from `--format json` and
both SDKs, with the legacy report proven to reproject losslessly out of it. **Phase A** absorbed the
EI side channel into those blocks (`dist_outcomes`, `healing_detail`, `boon_states`,
`target_conditions`, enemy per-skill damage and outgoing series, minions, health percents), which
let `ei-json` be rendered from the native report alone and `EiInputs` be deleted; `--all` and the
consumer-parse-cost benchmark landed with it. **Phase B** closed the gaps only native can close: the
23-field per-target split (hit quality, missed/evaded/blocked/invulned, applied CC), `distToCom` and
`stackDist` computed engine-side, commander-tag segments, `dc` despawn intervals, and the log-start
anchor centralised across its 16 call sites. **Phase C** shipped two generated icon catalogs —
4,656 skills from the GW2 API (plus `autoAttack` from `slot == "Weapon_1"`) and 2,267 buffs from
GW2EI's own table.

**MOBJ — WvW objective ownership.** Objective ownership timelines and shard ids parsed off the
encounter and emitted in both formats.

**MPROC / MCAST — instant casts.** GW2EI's `InstantCastFinder` machinery ported, a
565-of-649 catalog machine-extracted from the C# sources, the five `skillMap` proc/instant flags
emitted, arcdps effect events decoded across all three generations to feed the effect-keyed
finders, and instant casts merged with weapon swaps into the rotation per `InitCastEvents`.
Calibration: animated casts exact (1,222/1,222), weapon swaps exact (134/134), instant casts
bounded at 92.9%.

**Replay extras.** Glider, transformation and gadget capture. Both SDK type stubs were re-synced
with the 1.0 container and are now guarded in CI against the key-set golden. 1,069 tests.

## v0.3.2 — 2026-08-10

Enemy `profession`/`elite_spec` carried through to the enemy roster and declared in the Node types;
three CI failures fixed; the README rewritten from 778 lines to 232, with the deep reference moved
into `docs/`.

## v0.3.1 — 2026-08-10

**MSMALL — the deferred accuracy tail.** `HealingLogic` for Regeneration (GW2EI's only
`HealingLogic` buff) plus boon-generation `wasted` in `selfBuffs`/`groupBuffs`/`squadBuffs`,
including waste-only sources — this closed the last half-open axibridge audit row. `statsAll[0].
saved`/`timeSaved`/`wasted`/`timeWasted`, exact on 44/44 accounts, which turned out not to need the
`InstantCastEvent` pipeline; the ties-to-even fix behind it also took per-cast `timeGained` to
exactly 0 delta across all 10,878 local-capture casts. Presence tolerance tightened 2.0pp → 0.05pp
with the export's 3-decimal floor named. Breakbar damage excluded from down contribution (the
participation carve-out kept). `LazySeq` given a real take-once `debug_assert`; the Node
`index.d.ts` made regen-clean and guarded in CI.

**MROSTER — curate the `ei-json` `targets[]` roster.** `targets[]` was every enemy agent the log
enumerated (624 on the real capture) against the 57 GW2EI's WvW logic exposes. Curated to GW2EI's
own rule — enemy players only (`WvWLogic.cs:325-375`) — leaving 71. A correctness fix and a large
free performance win, because nine per-player arrays are positionally joined to `targets[]`:
`ei-json` flagless −71.1%, `--timeseries` −86.2%, matched axibridge surface −82.6% and 2.60 s →
1.70 s on the real log. Native `--format json` and `--format html` byte-identical before and after.

**MINSTID — regroup enemy players by instid.** `wvw::dedupe_enemy_players` now keys on `InstID`,
GW2EI's own non-squad rule (`AgentManipulationHelper.cs:467-474`), instead of the account that WvW
anonymization leaves empty. Native `enemies[]` 140 → 125 rows (71 → 56 enemy players), `ei-json`
`targets[]` 71 → 56 over EI's exact 56 instids, and the damage-mitigation `min` mean-of-minima
residual 16/206 → 0/206. Every merged row was verified to be the sum/union of its parts; the
instid-joined calibrations widened from 43 targets to all 56, which exposed 3 pre-existing
damage-credit divergences (allowlisted and diagnosed in `meigap2_ei_golden`).

## v0.3.0 — 2026-08-10

**MEIGAP — `ei-json` adapter gap closure for axibridge.** The axibridge cutover audit found 30 of
118 read fields blank under `ei-json`. Closed the per-player generation-attribution arrays
(`selfBuffs`/`groupBuffs`/`squadBuffs`), `buffUptimes[].states`/`.statesPerSource`, incoming
CC/strips/`boonStripsTime`, the per-target offensive split in `statsTargets`, the enemy-side
`targets[]` mirrors, the healing/barrier detail, `minions[]` and `guildID`.

**MEIGAP2 — the six open-cheap audit rows.** Player-side distribution outcome columns,
`healthPercents`, `instanceID`, `boonsStates`/`boonsAppliedCount`, `targets[].dpsAll[0].damage` and
`dpsAll[0].breakbarDamage`.

**MSTREAM — streaming `ei-json` serialization.** Stream the document instead of materializing the
whole `serde_json::Value` tree: peak RSS −95% (20× lower), output verified byte-identical across 96
flag/output combinations. This flipped the one column Elite Insights still won.

## v0.2.0 — 2026-08-09

**M15 — combat-replay positions in EI's shape.** A second, independent fixed-rate engine
(`analysis::ei_replay`) emitting `combatReplayData.{positions, orientations, dc, iconURL}` and the
top-level `combatReplayMetaData`, 100% f32-*text*-exact on both eras (37/37 and 44/44 players,
50,999 samples); unified 5-map geometry table and 45-icon table, machine-diffed against GW2EI.
`wvWMapData` objective capture left as a documented gap.

**MCONDCAT — condition-skill-id classification catalog.** Reproduced
`SkillEvent.ConditionDamageBased(log)` exactly via the complete 14-id `Buff.BuffClassification.
Condition` catalog instead of the old "buff == 1 and not life-leech" approximation, adding the
fourth `HitKind` bucket both modules had misclassified. Closed the empirically-confirmed post-era
gap (up to 51.4% relative divergence on `powerDamageTakenCount`); every previously report-only
golden check is now hard-exact on all 44 accounts of a real post-era capture.

**M16 — damage modifiers.** A GW2EI-cited attribution engine (10 gain computers) over a
205-definition catalog regenerable from GW2EI source, emitted behind `--modifiers`; 69 of the
reference export's 75 ids covered, `damageModMap` 69/69 character-identical.

**MBUFFSIM — buff-simulator stacking fidelity.** Isolate-first diagnosis overturned the premise:
the simulator was faithful; the real defects were two missing event-pipeline rules
(`BuffRemoveSingleEvent.OverstackOrNaturalEnd` and the `StackingConditionalLoss` `RemovedDuration`
band aid). Stability allowlist 7 → 0, average-stack tolerance 0.05 → 0.005, damage-modifier rows
682 → 779 exact, 0 previously-exact cells disturbed.

**MATTRIB — orphaned-instid attribution repair.** GW2EI's `CompleteAgents` repair transcribed
exactly (two probe points at ±300 ms, *not* a widened window) into `decode_raw`. Also root-caused
and fixed M16's residual: a self-damage filter on the incoming modifier pool GW2EI does not have.
Modifier rows 792/958 exact, every denominator residual pinned at 0.0.

## v0.1.1 — 2026-08-09

**M13 — hit quality and defenses.** Outgoing `statsAll` hit-quality (20 fields, exact vs EI) and
incoming defenses (block/evade/dodge/miss/interrupt/invuln counts plus the
strike/power/condition/life-leech/barrier/breakbar damage-taken breakdown), `ei-json` mapping and
`--view defense`. Surpasses EI on one field: the true life-leech count, which EI's own verified bug
zeroes.

**M14 — rotation and `skillMap`.** Per-player cast tracking (an `AnimatedCastEvent`-pipeline subset,
opt-in `--rotation`, cast count exact vs EI), a best-effort always-on `skillMap` scoped to
referenced skill ids, extended `is_swap`, `ei-json` mapping and `--view rotation`. Closed the last
axibridge-flagged Tier-1 analysis gap.

**MPERF — performance.** Criterion bench harness and CI job; `InstidRegistry` built once (11 full-log
builds → 1); shared `BoonInputs`; time-ordered contribution index; flat-`Vec` registry; healing gate
hoist. Fixture pipeline 50.5 → 28.9 ms, real 583k-event log 325.5 → 174.5 ms (`analyze` 2.63×).
Output verified byte-identical to pre-MPERF `main` across 30/30 surfaces.

**MPUB — registry publishing.** npm `@axiapps/axilog` (plus 5 platform packages) and PyPI `axilog`
both live, install-smoke verified from the public registries. PyPI uses trusted publishing (OIDC);
npm uses an `NPM_TOKEN` secret in `release.yml`. Both idempotent; future releases publish to both
automatically on tag push.

## v0.1.0 — 2026-08-09

The first release: 22 assets (5 CLI binaries + checksums, 6 npm tarballs, 4 wheels + an sdist).

**M11 (done):** the arcdps-methodology contribution family (`downs_contribution`/`downed_by`,
schema 0.1 → 0.2) — a health-anchored attribution window, max(last-≥99%-health − 2000 ms, log start,
prev-down + 2100 ms reset), with four stats (damage/CC/strips/movement-impairing) in both
directions, replacing the retired M1-era 10 s-window approximation. Plus the axibridge Tier-1
surface (`activeTimes`, `combatReplayData.{start,end,down,dead}`).

**M12 (done):** per-skill damage distribution (`totalDamageDist` exact vs EI) behind
`--skill-damage`, per-second cumulative series (`damage1S`/`targetDamage1S`/`damageTaken1S`) and
`dpsTargets` behind `--timeseries`, the `ei-json` mapping for all of it, and matching SDK options.

**M1 (done):** EVTC/zevtc decode, agent/skill resolution, WvW team/friend-foe resolution, damage +
DPS, downs/kills/deaths/down-contribution, CC + per-second timeline, native JSON schema, CLI
(`parse` with `json`/`table`/`csv`/`ei-json`), EI-compat adapter, golden parity test + CI.

**M2 (done):** real elite-spec profession naming, real team IDs from the log itself (`CBTS_WVWTEAMS`)
with a static fallback table, `CBTS_IDTOGUID` content-GUID decoding (teams now, skill/species
retained for M3), CC/stun-break metrics from real `CROWD_CONTROL`/`CBTS_STUNBREAK` events, enemy
relog dedupe, time-aware pet-damage/CC attribution across instid reuse, `axilog anonymize` +
PII-safe committed golden fixture (CI now runs real parity checks, not skip-and-pass), EI adapter
`statsAll` CC fields, squad markers (`CBTS_MARKER`) + commander-tag colour/variant + tick-rate
telemetry (`CBTS_TICK`) — native-schema-only.

**M3 (done):** the 12 tracked boons' stack-count timelines, uptime/presence/average-stacks, and
self/group/squad generation attribution (calibrated exact-to-near-exact vs. EI, see
[`docs/EI-PARITY.md`](EI-PARITY.md)); condi-cleanse/boon-strip/resurrect support stats
(calibrated exact vs. EI, no allowlist); exposed in the native schema (`players[].boons[]`,
`players[].support`), the EI adapter
(`buffMap`, `buffUptimes[]`, extended `support[0]`), and two new CLI table views (`--view
support`/`--view boons`).

**M4 (done):** post-`20260501` (buff-statechange-rework) log support — era-gated boon/support/CC
extraction (dedicated `BUFF_APPLY`/`BUFF_CHANGE`/`BUFF_REMOVE_SINGLE`/`BUFF_REMOVE_ALL`
statechanges, `ANIMATION_START`-gated resurrect detection, `buff == 1` CC rows), verified by
construction against GW2EI source + synthetic era-equivalence tests (no real post-rework capture
existed yet); downgraded the M3-era unconditional post-rework warning to fire only on genuinely
zero extracted buff events; added `tests/postrework_golden.rs`, a real-capture calibration hook
that activates automatically the moment a `fixtures/local/wvw-postrework.zevtc` fixture exists —
see [`docs/EI-PARITY.md`](EI-PARITY.md#supported-log-eras).

**M5 (done):** Node SDK (`crates/axilog-node`, `@axiapps/axilog`) — napi-rs native addon exporting
`parseFile`/`parseBuffer`/`parseFileEi`/`anonymizeFile` over the same decode → resolve → analyze →
build_report pipeline the CLI drives (no reimplementation, no JSON-over-subprocess); hand-maintained
TypeScript types (`types.d.ts`) for the native schema, patched into the generated `index.d.ts`; a
`node --test` suite covering all four exports plus a dual-path parity test against the CLI's own
`--format json` output; CI builds the addon on Linux/Windows/macOS and runs the node test suite on
Linux (see `.github/workflows/ci.yml`). npm publishing was deferred to MPUB.

**M6 (done):** Python SDK (`crates/axilog-py`, package `axilog`) — PyO3 native extension module
(`abi3-py39`) exporting `parse_file`/`parse_bytes`/`parse_file_ei`/`anonymize_file` over the same
decode → resolve → analyze → build_report pipeline the CLI and Node SDK drive (no
reimplementation); hand-maintained typed stubs (`axilog.pyi` + `py.typed`) for the native schema,
auto-bundled into the wheel by maturin; a stdlib `unittest` suite covering all four exports plus a
CLI-parity test against the CLI's own `--format json` output; CI builds the extension on
Linux/Windows/macOS (`maturin build`) and runs `maturin develop` + the unittest suite on Linux (see
`.github/workflows/ci.yml`). PyPI publishing was deferred to MPUB.

**M7 (done):** self-contained HTML report (`crates/axilog-html`, `axilog parse --format html`) —
dark-theme (light-mode toggle) single-file document, zero external requests, built from
`include_str!`-inlined `report.css`/`report.js` assets around the embedded `Report` JSON; header
(map/duration/recorder/commander/warnings/team chips); sortable Damage/Support/Boons tabs (keyboard-
accessible tab bar and column-sort buttons, `aria-sort` on the `<th>` per WAI-ARIA, squad-totals
footer row, muted non-squad rows, boon generation-mode self/group/squad toggle); an inline responsive
SVG damage timeline (squad-damage area/line, downs-on-the-line markers, normalized CC-applied bar
overlay, mm:ss/k-format axes) built by pure, node-tested path-generation functions
(`buildTimelinePaths`); an XSS contract (log-derived strings via `textContent` only, JSON escaped
`<`-safe for inline `<script>` embedding) with regression tests for both halves; a golden structural
test against the real committed fixture (calibrated squad-damage/support sums, all view/timeline
containers, byte-for-byte determinism, size budgets: <250KB total report, <50KB combined raw CSS+JS).
`-o/--output FILE` (any `--format`, not just html) added to the CLI alongside it.

**M8 (done):** tag-triggered release pipeline (`.github/workflows/release.yml`, `v*` tags) — CLI
binaries for all 5 targets, `@axiapps/axilog` npm main + platform packages (all 5), and `axilog`
Python wheels (abi3, 4 platforms) + sdist, all attached to one GitHub Release with a consolidated
`SHA256SUMS`; a version single-source guard (`scripts/check-versions.sh`, wired into `ci.yml`)
keeps `Cargo.toml`/`package.json`/npm platform packages/`pyproject.toml` from drifting apart, plus
a tag==Cargo-version guard (`scripts/check-tag-version.sh`) before every release; `npm publish`/
`twine upload` are wired in but gated on `NPM_TOKEN`/`PYPI_TOKEN` repository secrets being
configured (log-skip otherwise — the Release itself, with every artifact attached, is created
either way) and on the triggering event being a real tag push, never a `workflow_dispatch` dry
run. See [`RELEASING.md`](../RELEASING.md) for the full flow.

**M9 (done):** animated combat replay — `axilog_core::analysis::replay::build_replay` decodes
`CBTS_POSITION`/`CBTS_VELOCITY`/`CBTS_FACING` packed-float payloads (ordinals/layout verified
against the arcdps README and GW2EI's `MovementEvent` source) into per-squad-player and
per-enemy-player-representative position tracks, downsampled to a 300ms grid (matching GW2EI's own
combat-replay polling) with linear interpolation between bracketing samples, plus down/dead
intervals from existing event analysis — calibrated to ≥95% of samples within 1.0 map-pixel of
GW2EI's exported `combatReplayData` on both golden fixtures (in practice 99.77–100%, see
`crates/axilog-core/tests/replay_golden.rs`); an opt-in `replay: Option<ReplayOut>` schema block
(`axilog-schema`) wired through `axilog parse --replay` (json/html) and both SDKs
(`replay`/`replay=False` params, additive/back-compat); the HTML report's animated **Replay tab**
— SVG stage, play/pause/scrub/speed controls, pure node-tested
interpolation (`positionsAt`/`replayViewBox`/`isDownAt`/`isDeadAt`), rendered only when `--replay`
data is present. Size gates: replay-enabled reports <600KB, combined raw CSS+JS <60KB (raised from
M7's 50KB — controller-authorized, the animated stage/controls needed the extra headroom).

**M10 (done):** arcdps healing-extension stats (`players[].healing`: `healing_out_total`/
`healing_out_allies`/`healing_out_self`/`barrier_out`/`downed_healing_out`), calibrated exact
(`healing_out_self`/`downed_healing_out`, 41/41 accounts) to near-exact (`healing_out_total`/
`healing_out_allies` within 0.68%/0.71% squad-wide) against EI — except `barrier_out`, held to an
explicitly authorized wider 8.0% squad-wide tolerance (a single repeating-skill peer-report
cluster this project's byte-level replication of GW2EI's `SanitizeForSrc` rule can't perfectly
reconstruct without also replicating GW2EI's internal per-agent-lifetime identity tracking — see
`axilog_core::analysis::healing`'s module doc for the full trace); omitted entirely (not a
`null`/all-zero block) when the log carries no healing-extension data, a real "no data" signal
surfaced via a `Report.warnings` entry; new CLI `--view healing`. Opt-in missile (projectile)
analytics (`--missiles`, native-only): per-squad-player `fired`/`hit`/`denied`/
`reflected_at_self` plus a squad-wide `incoming_fired`/`incoming_denied` defensive rollup —
deliberately honest about its scope: the arcdps wire format carries no blocked/reflected/
destroyed reason code, so `denied` is one undifferentiated bucket, and there is no per-player
"who denied this" credit anywhere (only the aggregate `squad.incoming_denied`); `missiles.
players[]` entries carry `account` so they join back to `players[]` without needing `--replay`
too. A combat-participant enemy filter (`Report.enemies`, and the HTML team chips that read it)
now excludes NPC/gadget agents the squad never interacted with (no damage/CC either direction) —
a real WvW log enumerates every nearby lootable/tactivator/chest as an "enemy", most of which are
never actually part of the fight. (`ei-json`'s `targets[]`/`statsTargets[]` were left on the full
unfiltered roster at the time; MROSTER later curated them to GW2EI's own WvW rule instead — the two
surfaces are now independent filters over the same enemy list, see `Report::ei_targets`.) Team ids
(`TeamOut.team_id` and the model/analysis layers feeding it) widened `u16` → `u32`, removing a
truncating cast on dynamic `CBTS_WVWTEAMS` ids (future-proofing; no real fixture currently has an
id large enough for the truncation to have mattered).

---

For what is queued, parked or in flight after v0.3.1 — PvE encounter logic, `wvWMapData` objectives,
HTML report extras — see [`docs/ROADMAP.md`](ROADMAP.md).
