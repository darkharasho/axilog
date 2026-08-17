# axilog roadmap

Autonomous build loop. Each milestone: spec → plan → subagent-driven execution (isolated
worktree, per-task adversarial review, final opus whole-branch review) → merge to main + push.
Bar: first-class, performant, accurate-as-fuck. Calibrated numbers stay EI-exact or get a
documented+ruled exception. arcdps README is buggy — hand-count ordinals, GW2EI source is the
algorithm arbiter, dev-relayed arcdps methodology is authoritative.

## Done (merged to main)
- M1 WvW core · M2 polish · M3 boons/support · M4 post-rework era · M5 Node SDK · M6 Python SDK
- M7 HTML report · M8 release pipeline · M9 combat replay · M10 healing/missiles/polish
- **v0.1.0 RELEASED** — GitHub Release with 22 assets (5 CLI binaries+checksums, 6 npm tarballs, 4 wheels+sdist); publish steps gated no-ops (no tokens)
- M11 contribution family + axibridge tier-1
- M12 per-skill (totalDamageDist EXACT vs EI) + per-second (damage1S, --timeseries gate) + dpsTargets + ei-json mapping + SDK ei options
- M13 Hit-quality + defenses: outgoing statsAll hit-quality (crit/flank/glance/against-moving/
  connected/direct/condition/critable/against-downed/life-leech/above-90%-HP, EXACT vs EI) +
  incoming defenses (block/evade/dodge/miss/interrupt/invuln counts, strike/power/condition/
  life-leech/barrier/breakbar damage-taken breakdown, EXACT vs EI except a documented real
  GW2EI `lifeLeechDamageTakenCount` counting bug axilog deliberately doesn't reproduce) + ei-json
  mapping (`statsAll[0]`/`defenses[0]`, EXACT vs EI) + `--view defense`. Post-era classification
  now has REAL (not just synthetic) local calibration; first real capture confirmed the
  documented condition-skill-id-catalog simplification gap is real (not just theoretical) on the
  incoming side, see `analysis::defenses`'s module doc.
- CI: x86_64-apple-darwin cross-compiles on arm64 (macos-13 retired); ci concurrency-cancel
- M13 hit-quality (statsAll, 20 fields EXACT) + defenses (SURPASSES EI: true life-leech count EI's own bug zeroes) + --view defense
- M14 Rotation + skillMap: per-player cast tracking (`AnimatedCastEvent`-pipeline subset, opt-in
  `--rotation`, cast COUNT exact vs EI) + best-effort skillMap (log-table names, always-on,
  scoped to referenced skill ids only) + extended `is_swap` (WeaponSwap sentinel + elementalist
  attunement swaps + revenant legend swaps [5 variants] + necro shroud transforms [3 variants],
  Weaver's own combo-attunement table still out of scope) + ei-json mapping (`rotation[]`/
  `skillMap`, gate-respecting) + `--view rotation` (cast count + APM). Closes the last
  axibridge-flagged Tier-1 analysis gap — see README's EI-JSON parity section.
  Follow-up: py `.pyi` stub synced to the M14 surface (rotation/skillMap TypedDicts + params).

- MPERF Performance milestone (merged 1b5eb5b, reviewed SHIP): criterion bench harness + CI job;
  InstidRegistry built once (11 full-log builds → 1); shared BoonInputs; contribution
  time-ordered index; flat-Vec registry; healing gate hoist. Fixture pipeline 50.5→28.9ms;
  real 583k-event log 325.5→174.5ms (analyze 2.63×). Output verified byte-identical to
  pre-MPERF main across 30/30 surfaces. docs/BENCHMARKS.md has the full applied/declined record.
- MSTREAM Streaming ei-json serialization (merged 8977f74, shipped v0.3.0): stream the document
  instead of materializing the whole `serde_json::Value` tree. On the 583k-event log, matched
  ei-json surface: peak RSS 2,389 → 117.0 MiB (−95.1%, 20.4x) and wall 3.24 → 2.07 s (−36%);
  native JSON output untouched. Output byte-identical base → tip across 96/96 flag combinations.
  Note the baseline is `0a8cf25` (MEIGAP2's merge), not the v0.2.0 tag — MEIGAP/MEIGAP2 had
  roughly doubled the ei-json document (183 → 366 MB) and pushed the pre-MSTREAM peak from
  1,281 to 2,389 MiB first. This flipped the one column Elite Insights still won. Caveat kept
  from docs/BENCHMARKS.md: the EI side was NOT re-run, so those deltas are axilog-vs-axilog.
- **v0.1.1 RELEASED + npm LIVE**: packages renamed to the @axiapps scope and published —
  `npm install @axiapps/axilog` works from the public registry (install-smoke verified).
  M8-parked publish hardening landed (index.js version-literal guard in check-versions.sh,
  pypi-validate wheel install-smoke gate, npm/PyPI publish idempotency).

## Native format program (the axibridge cutover)

The destination: **axibridge runs entirely off axilog's native output, with no
ei-json shim.** The `to_ei_json` layer is PERMANENT regardless — it stays as a
thin compat path so other downstream consumers can migrate on their own
schedule. Absorption removed ei-json's *private data*, not ei-json.

- **Spec #1 / native container 1.0 — DONE**, merged 2026-08-11 (`747875b`).
  `--format json` emits the 1.0 document (`axilog`/`encounter`/`entities`/
  `catalogs`/`blocks`/`coverage`); `docs/NATIVE-FORMAT.md` is the reference.
  Was missing from this roadmap entirely until spec #2 Task 14.
- **Phase A / spec #2 side-channel absorption — DONE** (this branch, Tasks
  1–14; spec `docs/superpowers/specs/2026-08-13-side-channel-absorption-design.md`,
  plan `docs/superpowers/plans/2026-08-13-side-channel-absorption.md`).
  Absorbed the whole `EiInputs` side channel into native blocks, re-pointed the
  adapter at `ReportV1`, deleted `EiInputs` and the two-`Report` split, and
  added `--all` / `everything`. **The ei-json goldens never moved** — which is
  the proof the spec was built to produce: every field ei-json emits is now
  read from the native document, so the goldens attest to native's
  completeness. One named exception, the GW2EI combat-replay position surface
  (`axilog_ei::EiReplayInput`), which spec #1 decision 6 deliberately keeps out
  of the native shape.
- **Phase B — DONE**, merged 2026-08-16 (PR #5, `521abc9`; 16 commits, 39
  files, +3601/−151, CI green on all four targets). Closed the five gaps native
  could close that ei-json can't: the per-target split widened 7 → 23 fields,
  replay `dc` (despawn) intervals, commander-tag segments, engine-side
  `distToCom`/`stackDist` (`crates/axilog-core/src/analysis/distance.rs`, new),
  and `encounter.started_at_unix`. Absorbed what an older roadmap called
  spec #3. Two invariants worth not relearning: `CommanderTag::segments` and
  `markers[].time_ms` are in **arcdps session time**, not log-relative; and a
  distance scalar of `None` means the position pass never ran while `-1.0`
  means it ran and nothing qualified — never collapse the two.
- **Phase C — DONE (icons).** Two generated catalogs ship:
  `analysis::skill_icons` (GW2 `/v2/skills`, 4,656 entries, plus `autoAttack`
  from `slot == "Weapon_1"`) and `analysis::buff_icons` (GW2EI's `new Buff(...)`
  table, 2,267 entries). Fixture coverage 329/368; the 39 misses are internal
  damage-proc ids neither source has art for — a floor, not a gap. Both are
  wired into the NATIVE catalogs (`axilog-schema/src/v1/catalogs.rs:185-197`);
  ei-json's `skillMap` still omits `icon`/`autoAttack` by choice.
  Proc flags were split out of this phase as **MPROC** (see Queued) once a
  spike falsified the premise they were filed under — they are not a skill-database
  problem at all.
- **Phase D — the axibridge-side reader rewrite. NOT ours**: axilog-side
  readiness only unless the owner says otherwise.

Debt left parked by Phase B, in rough value order:
- ~~`dc` rename passes the SDK suites silently~~ — **CLOSED** (`c1ee0ec`). The
  guard that matters is in the schema: `ReplayIntervals` now asserts its exact
  serialized key set in both the gated and ungated states, which also pins
  `-1.0` as serialized rather than skipped. Both SDKs additionally assert
  presence and type per row. Note the original claim was half wrong: only Node
  passed silently; Python already failed, just as an opaque `KeyError`.
- ~~A `markers.rs` tag-colour-swap can emit two overlapping commander segments~~
  — CLOSED 2026-08-16. Root cause: post-`NewMarkerEventBehavior` (arcdps build
  20240418) a non-end marker only closes an open marker with the SAME id, and
  two tag colours are two ids, so both stayed open and both were closed at log
  end. The missing rule was GW2EI's per-player cutoff — `CalculateCommanderStates`
  `break`s at the first commander window with `EndNotSet` (`StatisticsHelper.cs:322-325`),
  so one player contributes at most one open-ended window and nothing after it.
  Ported as `markers::truncate_at_first_unclosed`. Note the pooled BETWEEN-player
  overlap rule was never missing — `distance::commander_positions` already
  mirrors "previous tag has priority" exactly.
- ~~`t0` is re-derived in `distance.rs:142`, duplicating `replay.rs:172`~~ —
  CLOSED 2026-08-16, and it was 8× larger than recorded: `events.first().time`
  was open-coded at **16** call sites across `analysis` and `wvw`, under four
  different local names (`t0`, `t0_ms`, `log_start`, `log_start_ms`), which is
  exactly why only one pair of them was ever noticed. Now `RawLog::log_start_ms()`
  (`evtc/mod.rs`), with the convention documented once and two unit tests
  pinning it (positional `first()`, NOT `min()`; empty log → 0). Two lookalikes
  in `buffs::generation`/`buffs::simulator` were deliberately left alone — they
  clock a filtered local slice, not `raw.events`.
- ~~`axilog.pyi` was swept for pre-1.0 staleness only at `PerTargetDetail`~~ —
  CLOSED 2026-08-16, and it was not just Python. Audited by binding a real
  `--all` document to each stub's declared types: **9** Python TypedDicts and
  **8** TypeScript interfaces were missing fields, including two whole blocks
  (`conditions`, `minions`) the Python stub still called "reserved for spec #2,
  always `not_computed`", and a stale `DamageModEntry.kind` that the schema no
  longer has. `minions` was self-documented as a known gap in `types.d.ts`.
  Both stubs now transcribe the full 1.0 surface, and
  `crates/axilog-schema/tests/v1_sdk_stubs.rs` keeps them honest: every field
  name in `v1-keyset.golden.txt` must appear in both, so a new field cannot
  land without the stubs following. The check is name-level, not type-level --
  it catches "nobody transcribed this" (the failure that actually happened),
  not a name put on the wrong type; that limit is documented in the test.
- ~~Windows CI `LNK1201`~~ — CLOSED, and the standing description of it was
  wrong. It was recorded as "the CLI bin and the Python cdylib are both named
  `axilog`, so their PDBs collide; the real fix is renaming one." Renaming
  either is impossible anyway (`axilog` is the published CLI command name AND
  the Python import name, which must match the `#[pymodule]` fn), but no rename
  is needed: `db34709` (2026-08-15) stopped the Windows leg emitting PDBs at
  all, via per-OS `CARGO_PROFILE_{DEV,TEST}_DEBUG` matrix keys. With no `/DEBUG`
  there is no PDB to contend over, so every candidate cause (disk, path,
  privilege, a Defender scan mid-write) is moot at once. Verified 2026-08-16:
  the last LNK1201 failure was run 31910071414 on `cbcf427`, which predates
  `db34709` by 9 minutes; all 10+ Windows runs since — including PR #5 and the
  `971b5c9` push — are green. Release builds were never exposed (cargo's release
  profile has `debug = false`). Residual cost, Windows-only and CI-only: a
  panicking test there gets an unsymbolized backtrace.

Two rules that hold across the whole program: the ei-json translation layer is
**permanent** (a thin translation over the native document — do not propose
sunsetting it), and native 1.0 is **malleable** — breaking changes land without
a major bump while the in-tree adapter is 1.0's only reader, each recorded in
`docs/NATIVE-FORMAT.md` §"1.x compatibility rules".

## In flight
(none)

## Done (publishing)
- MPUB COMPLETE: npm `@axiapps/axilog` (+5 platform packages) AND PyPI `axilog` both LIVE at
  0.1.1, install-smoke verified from the public registries. PyPI = trusted publishing (OIDC) via
  `.github/workflows/pypi-publish.yml` (environment `pypi`), dispatched by release.yml after each
  release; npm = NPM_TOKEN secret in release.yml. Both idempotent. Future releases publish to
  both registries automatically on tag push.

- M15 Combat-replay positions in EI shape (merged 549a518, reviewed SHIP): EI fixed-rate
  engine (positions/orientations/dc/start/end, 100% f32-TEXT-exact both eras — 37/37 + 44/44
  players, 50,999 samples); unified 5-map geometry table + 45-icon table (GW2EI machine-diffed
  exact); ei-json combatReplayData/combatReplayMetaData gated --replay; M11 always-on surface
  byte-identical. Fix waves: PlayerActor CR-trim, dst-side awareness, forcePolling squad-only,
  always-emit combatReplayData, PII scrub (incl. pre-existing _note names). wvWMapData
  (objective capture) = documented gap. Follow-up seed: to_ei_json options struct before a 4th arg.

- MCONDCAT Condition-skill-id classification catalog (merged c351e93, task-2 gate flips): reproduced
  `SkillEvent.ConditionDamageBased(log)` exactly via the complete 14-id `Buff.BuffClassification.
  Condition` catalog (`analysis::condition_catalog`, exhaustively scanned + machine-diffed against
  GW2EI source) instead of the old "buff==1 and not life-leech" approximation. Added the fourth
  `HitKind` bucket (buff==1, uncatalogued, not life-leech) both modules previously misclassified.
  The empirically-confirmed post-era gap (M13: up to 51.4% relative divergence on `power_count`,
  33/44 incoming + 2/44 outgoing accounts affected, pure reclassification — conserved total, never a
  dropped/extra event) is now closed: all previously report-only/tolerant golden checks in
  `hit_stats_golden.rs`/`defenses_golden.rs`/`ei_golden.rs` are hard-EXACT on all 44 joined accounts
  of a real post-era capture. Pre-era committed fixture output byte-identical across all 7 formats.

- MDOCS COMPLETE (arcdps-wiki c06aaf2): five-page axilog section live on the wiki (overview,
  quickstart with registry-verified commands, methodology, schema reference, accuracy story —
  all facts source-verified, zero dead links). axilog README de-staled (registries live,
  M11 down-contribution row, Later list).

- MBUFFSIM COMPLETE (merged 2f25110, reviewed SHIP): isolate-first diagnosis OVERTURNED the
  premise — simulator.rs was faithful; the real rules were event-pipeline (natural-expiry
  removal drop + conditional-loss RemovedDuration rewrite, both ported clause-exact). Stability
  allowlist 7→0 with teeth; avg-stack tolerance 0.05→0.005 (10x); modifier rows 682→779 exact;
  0 previously-exact cells disturbed. Deferred (ledgered in-repo): Regeneration/HealingLogic
  (4800x inside tolerance), PRESENCE_TOLERANCE_PP tighten (~0.05pp), boons_golden
  LOCAL_FIXTURE_PATH env-var migration, OffsetBuffExtensionEvents, duration-graph-value latent.

## Done (analysis milestones)
- M16 Damage modifiers COMPLETE (merged 1545930, reviewed SHIP): EiInputs refactor; GW2EI-cited
  engine (10 GainComputers, AlwaysMaster); 205-definition catalog regenerable from GW2EI source
  (scripts/gen_damage_mod_catalog.py, regen diff empty; 69/75 reference ids); emission behind
  --modifiers (native +44.2%, ei-json +441.5% incl. WvW Target variants; damageGain f64 text
  discipline). 30 ids hard-exact all-fields x 44 accounts; 39 bounded per-field (residual proven
  simulator-side -> MBUFFSIM); 69/69 damageModMap identical; 207/207 + 1,978/1,978 rows
  text-identical. NOTE for next MDOCS touch: /axilog/accuracy on the wiki needs a damage-modifier
  row (coverage + bounded-class statement).
- MATTRIB COMPLETE (merged 6edb3c0, reviewed SHIP): GW2EI's CompleteAgents orphan repair
  transcribed exactly (two-probe-point ±300ms, NOT a window) into decode_raw; M16's
  NonZeroAddrIndex retired byte-identically; 106/121 EI-comparable values toward EI (15 exact),
  0 previously-exact disturbed. BONUS: the M16 deficit ROOT-CAUSED AND FIXED — a self-damage
  filter on the incoming modifier pool GW2EI doesn't have (EI filters outgoing only); modifier
  rows 792/958 exact, ids 38/69, all denominator residuals 0.0. 36 local calibration tests now
  run from any worktree via AXILOG_LOCAL_FIXTURES.

## Done (axibridge gap closure)
- MEIGAP + MEIGAP2 COMPLETE (shipped v0.3.0): the axibridge cutover audit
  (axibridge:docs/axilog-cutover-report.md) found 30 of 118 read fields blank under axilog's
  ei-json. All closed — the per-player generation attribution arrays (selfBuffs/groupBuffs/
  squadBuffs), buffUptimes[].states/.statesPerSource, incoming CC/strips/boonStripsTime, the
  per-target offensive split in statsTargets, the enemy-side targets[] mirrors (buffs /
  totalDamageDist / damage1S / dpsAll[0]), powerDamageTaken1S, healing+barrier detail,
  minions[], guildID, plus MEIGAP2's six open-cheap rows (player-side dist outcome columns,
  healthPercents, instanceID, boonsStates, breakbarDamage). Per-field parity accounting lives
  in docs/EI-PARITY.md. axibridge flipping its parser default from elite-insights to axilog is
  now unblocked and user-gated (see Parked).
- MINSTID Enemy-player instid regroup (DONE): `wvw::dedupe_enemy_players` now keys on INSTID,
  GW2EI's own non-squad rule (`AgentManipulationHelper.cs:467-474`), instead of the ACCOUNT that
  WvW anonymization leaves empty. Native `enemies[]` 140 -> 125 rows (71 -> 56 enemy players),
  ei-json `targets[]` 71 -> 56 over EI's exact 56 instids, and the mitigation min-mean residual
  went 16/206 -> 0/206. Every merged row was verified to be the sum/union of its parts; the
  instid-joined calibrations widened from 43 targets to all 56, which exposed 3 pre-existing
  damage-CREDIT divergences (allowlisted + diagnosed in `meigap2_ei_golden`) as the one
  follow-up.

## Queued (the only open feature work)
- ~~MCAST: merge instant casts + weapon swaps into `rotation`~~ — **DONE 2026-08-16**.
  `analysis::rotation::build` now transcribes GW2EI's `SingleActor.InitCastEvents`
  (`SingleActor.cs:599-619`) whole: `animated ++ instant`, then the weapon-swap loop with
  its `ServerDelayConstant` replace-the-trailing-swap dedup. `AnimationStatus` gained the
  `Instant` variant (neutral to `aftercast_stats`, matching `GameplayStatistics`). The one
  expensive finder pass now runs once in `analyze` and is shared with `skill_map::build`,
  which takes the events rather than recomputing them.

  Three things this forced, each worth knowing:
  - **The committed ei-json golden had to move**, the one deliberate exception to "goldens
    don't move". `fixtures/wvw-small.ei.json`'s `rotation` was extracted PRE-FILTERED to the
    animated subset (1,222 entries, zero with `duration <= 1`) while the real export it came
    from has 1,732. It was regenerated VERBATIM from that export. The join was re-derived
    independently of the account mapping — match each old golden player against the source
    player whose `id >= 0 && duration > 1` subset reproduces it exactly (37 of 41 unique,
    all identity; the other 4 are the zero-cast `Non Squad Player N` rows). Details in the
    golden's own `_note`.
  - **Calibration is per cast FAMILY**, not one total. Animated: EXACT (unchanged, 1,222 for
    1,222, every field 0 residual). Weapon swaps: EXACT (134 for 134). Instant casts:
    BOUNDED — 340/364, **93.4% recovered**, because that family is only as complete as the
    finder catalog. Asserting exactness there would be asserting the catalog is complete.
  - **Negative pseudo ids now reach the surface.** They ride as `-2i32 as u32` natively and
    the ei-json adapter casts back (`ei_skill_id`), so `rotation[].id` is `-2` and `skillMap`
    keys `"s-2"`, as EI writes them. `skill_map::PSEUDO_SKILL_NAMES` ports the NEGATIVE-id
    subset of GW2EI's `SkillItemOverrides` so they name themselves ("Weapon Swap") instead
    of falling back to `"Skill 4294967294"`. The 12 Weaver dual-attunement ids still fall
    back — EI names those from its Buff table, a different subsystem.

  Found while calibrating: a real bug in `instant_cast`, not in the merge. The ext-healing
  finders never ported `HealingStatsExtensionHandler.SanitizeForSrc`, so every heal counted
  twice (healer's client + recipient's). One `src_is_peer` predicate at stream collection;
  three finders went from +35 over to exact.

  Behavioural consequences, both intended: `--view rotation` cast counts and APM now cover
  all actions, not just animations (the fixture's sample row went 55 → 63 casts, 67.0 → 76.7
  APM, and 63 is exactly what EI reports), and ei-json `rotation[]` gained the ~29% of
  entries it was missing.
- ~~MPROC skill proc/instant-cast flags~~ — **DONE 2026-08-16** (`c6dc134`, `324a1e5`,
  `a620c3c`). All five flags are computed and emitted in both formats. The scoping below
  held up in full: it is a subsystem port, not a catalog generator, and `isInstantCast`
  genuinely required running the finders. Shipped as
  `analysis::instant_cast` (model + one engine, mirroring `damage_mods`) plus
  `scripts/gen_instant_cast_catalog.py`, which extracts **571 of GW2EI's 649** finder
  constructions with a named reason for each of the 78 skips. (It was 429/220 as first
  shipped; the effect decode below closed the largest skip bucket.)

  Four corrections to the scoping below, all found by machine accounting rather than by
  reading:
  - The count is **649**, not 658. The earlier number came from a grep that also matched
    commented-out code; `GuardianHelper.cs:25-27` alone carries three dead finders in an
    obsolete 3-argument form.
  - `EffectCastFinderByDst` is a 60-construction subclass the first extraction regex
    (`\w*CastFinder`, no suffix) could not see at all — neither transcribed nor skipped,
    so the accounting balanced while under-counting the source by 10%.
  - `.UsingBeforeWeaponSwap()` is a FINDER method with 28 real call sites, not a
    damage-modifier one. Rather than skip those finders, `CBTS_WEAPSWAP` (statechange 11)
    is now decoded and the snap implemented — a one-directional clamp, `min(swap-1, time)`.
  - `MinionCommandBuff` is `59536`, not the value a first pass guessed.

  ~~Remaining gap: the 172 effect-keyed finders are not evaluated.~~ — **CLOSED
  2026-08-16.** `crates/axilog-core/src/evtc/effect.rs` decodes all three arcdps effect
  generations (`CBTS_EFFECT` 45, `CBTS_EFFECT` 51 with its end form, and the split
  ground/agent 60–63), folded into one `EffectEvent` the way GW2EI folds them. 136 of the
  175 effect finders are now transcribed; the rest fall into the same `.UsingChecker(lambda)`
  bucket every other subclass has. Effect on the committed fixture: distinct skills carrying
  `isInstantCast` went **9 → 84**, and the named results are correct GW2 mechanics (Deploy
  Jade Sphere, the chronomancer shatters, Relic of Fireworks, Tale of the Honorable Rogue).

  Three things that decode is load-bearing for and that are worth not re-deriving:
  - **Effect ids are session-local.** A row names its effect in `skillid`; the stable
    16-byte GUID arrives separately as a `CBTS_IDTOGUID` row of content type EFFECT. A
    finder therefore resolves GUID → local id per log. Below arcdps `20220709`
    (`FunctionalIDToGUIDEvents`) there is no usable GUID table at all, so no effect finder
    can fire — correct, not a bug.
  - **Non-static-platform effects are DROPPED**, reproducing GW2EI's release-build filter.
    An effect riding a moving platform has coordinates in the platform's frame. The drop is
    observable through `HasEffectData` and through every effect finder, so it must not be
    "fixed".
  - **`UsingDurationChecker` means two different things.** On a buff finder it is an
    epsilon band (`|applied - d| < eps`); on an effect finder it is exact equality, or an
    inclusive `[min, max]` range. They are separate `Check` variants for that reason.

  That gap's companion — **6 `UsingNoAnimatedCastChecker` finders** — is **CLOSED
  2026-08-16**. `Check::NoAnimatedCast` transcribes `CombatData.IsCasting` against the
  windows `rotation::animated` builds, so the catalog is now **571 of 649** (was 565).
  The apparent cycle (`instant_cast` needs cast windows, `rotation` needs instant casts)
  is broken by splitting `rotation::build` in two around the finder pass; `analyze` runs
  animated → finders → merge and builds the animated half exactly once. The checker is
  load-bearing, not decorative: on the committed fixture those six finders emit 6 squad
  casts of Lesser Symbol of Resolution without it and **1** with it, and EI reports 1.

  The **`rotation` fill** this entry used to leave open is **DONE 2026-08-16** — see MCAST
  below.

  Original scoping, retained because it is what made the work tractable:
- MPROC skill proc/instant-cast flags (`skillMap[].isTraitProc`/`isGearProc`/
  `isUnconditionalProc`/`isNotAccurate`/`isInstantCast`) — split out of Phase C 2026-08-16. Scoped by a
  source-triangulation spike, which OVERTURNED the "needs a GW2 skill database" premise:
  - No GW2 API involvement. The flags are a SIDE EFFECT of GW2EI's instant-cast detection
    subsystem. `CombatData.ComputeInstantCastEventsFromFinders` (`CombatData.cs:214-244`) walks
    every `InstantCastFinder` and, for each one whose `Available(this)` holds, adds its skill id
    to a `TraitProc`/`GearProc`/`UnconditionalProc` set by the finder's declared `CastOrigin`.
    `SkillData.cs:44-58` is then a bare `Contains`.
  - **The flag set is LOG-SPECIFIC, so a static id table cannot match EI.** `Available()`
    (`InstantCastFinder.cs:138-150`) gates on the finder's `_enableConditions` plus a GW2 build
    range plus an evtc build range. Build ranges alone would still be tabulatable per build pair,
    but `_enableConditions` are arbitrary `Func<CombatData,bool>` predicates over the parsed log
    (`UsingEnable`, `InstantCastFinder.cs:81-95` — e.g. `!combatData.HasEffectData`, spec checkers),
    with **187** `UsingChecker` call sites in `ProfHelpers`. So availability genuinely depends on
    log contents, not just on the build pair.
  - Scale (measured in `GW2EIEvtcParser/EIData/ProfHelpers`, 2026-08-16): **658** finder
    constructions across ~44 profession-helper files in **13** subclasses — `BuffGain` 190,
    `Effect` 175, `MinionCommand` 86, `Damage` 77, `BuffLoss` 33, `EXTHealing` 32, `BuffGive` 21,
    `MinionCast` 12, `Missile` 11, `MinionSpawn` 11, `EXTBarrier` 4, `BandTogether` 4,
    `BreakbarDamage` 2. Gating on top of that: **957** `.WithBuilds(`, 6 `.WithEvtcBuilds(`,
    187 `.UsingChecker(`. Only 148 `.UsingOrigin(` calls declare a non-default origin
    (85 `Gear`, 49 `Trait`, 17 `Unconditional`; the 2 `Skill` are the default and set no flag),
    and 19 `.UsingNotAccurate(`. (An earlier pass said "~616 finders / 153 origins" — same shape,
    superseded by these directory-scoped counts.)
  - `isGearProc` is the LARGEST bucket, so scope MPROC to the full family, not the three flags
    this roadmap used to name. `analysis/skill_map.rs` already had the complete list; only the
    roadmap summary was short. `isNotAccurate` comes from the same loop (`CombatData.cs:224`)
    and rides along for free.
  - RESOLVED (was open; `GW2EIBuilders` added to the sparse checkout 2026-08-16):
    `isInstantCast` is `log.CombatData.GetInstantCastData(skill.ID).Any()`
    (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs:23`), i.e. "did any `InstantCastEvent` for this
    skill id actually get emitted in THIS log" — `_instantCastDataByID` lookup,
    `CombatDataFetchers.cs:634`. **This is a strictly stronger requirement than the four proc
    flags.** Those are set from finder *availability*; `isInstantCast` needs the finder to have
    *fired*, so it cannot be shortcut at all — you must run `ComputeInstantCast` for real.
  - Therefore this is a SUBSYSTEM PORT, not a catalog generator — milestone-sized, and the
    roadmap's smallest-looking bullet was hiding that. Real adjacent value: instant-cast events
    would also fill M14's known `rotation` gap, which covers only the `AnimatedCastEvent` pipeline.
  - CORRECTION (2026-08-16): an earlier pass listed `canCrit`/`isSwap` here as cheap adjacent wins
    "not currently emitted by ei-json's `skillMap`". **They were already implemented and emitted** —
    `analysis::skill_map` since M14 Task 2/3, `axilog-ei/src/lib.rs:2526-2527`,
    `axilog-schema/src/v1/catalogs.rs:60-61`. Neither is MPROC work, and neither is open:
    - `canCrit` is exact against a real EI export (asserted, not printed, in
      `skill_map_golden.rs`). EI's `gw2Build < sinceBuild` gate (`SkillItem.cs:128`) is
      deliberately not reproduced: all 20 thresholds are 2015–2018 patches, far below any build
      this project parses, so every entry is unconditionally non-critable at the operating range.
      Documented in `hit_stats.rs:215-235`. Only a pre-2019 log would diverge.
    - `isSwap` had one real gap — Weaver's `_weaverAttunements` — **now closed**; it is a complete
      port of `SkillItem.IsSwap`. The gap was skipped in M14 as a "much larger 16-entry table";
      re-reading the source showed 12 of the 16 are EI-invented pseudo ids (`-5`..`-16`,
      `SkillIDs.cs:24-35`) and only 4 are real (41166/42264/43470/44857), so the stated reason for
      skipping did not hold.
  - The cheap alternative, considered and NOT taken: ship the 153 `(skill_id, origin)` pairs as a
    generated catalog and flag referenced ids unconditionally (~a day). It over-flags exactly where
    `Available()` says no, so it would need a documented+ruled exception rather than parity.
- ~~MOBJ wvWMapData objectives~~ — **CLOSED 2026-08-16.** Both formats now emit it:
  `encounter.objectives[]` + `encounter.teams[].shard_id` natively, and a complete
  `wvWMapData` (`redShardID`/`blueShardID`/`greenShardID` + `objectiveData`) in ei-json.
  Corrections to the entry this replaced, worth keeping:
  - The source is `CBTS_WVWOBJECTIVESTATUS` (sc=75), **not** GADGETCAPTURE. The
    GADGETCAPTURE family (sc=80-83) is replay outline geometry and is unrelated;
    EI's `WvWObjectiveStatusEvent` reads sc=75 only.
  - The shard ids were already in the sc=74 payload axilog parses, in the three
    uint32 slots it skipped as "unused by axilog today". No new event was needed.
  - An objective's TYPE is not in the log — it comes from a static
    `(map_id, objective_id)` catalog (`WvWHelper.cs:161-268`, 76 rows over 4 maps),
    and EI DROPS any status event the catalog can't type. An incomplete catalog
    silently shortens the output rather than emitting `"Unknown"`, so all 76 rows are
    transcribed and machine-diffed against the source, not just the mapped fixtures.
  - The committed fixture (Jan 2026) predates BOTH sc=74 and sc=75, so the key-set
    golden can't reach these fields; hand-built tests in `axilog-schema`/`axilog-ei`
    cover the wire shapes instead. Same gap class as `encounter.tick_rate`.

- ~~Pre-existing clippy warnings~~ — CLOSED 2026-08-16; `cargo clippy --workspace
  --all-targets` is now clean. 22 warnings: 13 `\0`-followed-by-a-digit escapes in
  arcdps name-field test literals (Rust has no octal escapes, so the bytes were
  always right, but they READ as octal — respelled `\x00`), 5 `len() > 0`,
  a `get().is_none()`, a descending `sort_by` → `sort_by_key(Reverse(..))`, and
  `credit_window`'s 8 arguments (fixed by collapsing `lo`/`hi` into the one
  window tuple they always were, not by an `#[allow]`). One deliberate `#[allow]`
  remains, on a `collapsible_match` whose collapse is safe but would orphan the
  comment explaining the branch it guards; the reason is recorded inline.
  Byproduct: the `len() > 0` lint drew the eye to
  `assert_eq!(raw.events.len(), raw.events.len())` in `decode_fixture.rs` — a
  value compared to ITSELF, under a comment claiming it checked layout-vs-decoded
  agreement. It could not fail, so it never had. Replaced with pinned decode
  counts (120,435 events / 173 agents / 969 skills), which is the check that was
  wanted: a wrong event stride moves them, and `EVENT_SIZE_REV1` has been wrong
  here before.

## Parked (user-gated — do NOT do autonomously)
- axibridge's actual cutover from EI CLI to the axilog SDK (both registries now live, so this is
  unblocked whenever the user wants it).
- ~~Replay eye-candy backlog (dev-notes #6/#8)~~ — **DONE 2026-08-16**, user-approved with the
  decoration layer explicitly in scope. Four new core modules (`analysis::agent_states`,
  `gadget_capture`, `decorations`, `replay_extras`), four new optional arrays on `blocks.replay`.
  Three findings worth keeping:
  - GLIDER (55) and TRANSFORMATION (73) are parsed-but-unconsumed in GW2EI, so this half is
    original output with no parity reference and nothing in ei-json.
  - GADGETCAPTURE (80–83) needs arcdps build `20260602`; the committed fixture is `20260114` and
    carries zero rows, so it is covered by hand-built wire tests, not a golden diff.
  - `analysis::decorations` is the first decoration container in the tree. It is deliberately
    narrow — fixed world anchors, three shape kinds, no metadata dedupe, no map projection — and
    `ei_replay::MapTransform`'s "decoration/viewpoint machinery is out of scope" note still holds
    for everything beyond it.

## Cross-cutting invariants (every milestone)
- All existing calibration exact; no PII committed (raw .zevtc gitignored; anon fixtures only);
  no-literal-`</script`/textContent-only/determinism for HTML; asset budget honored; warning-free;
  node+python+JS suites green on schema ripple; HTML changes get a controller browser pass.
