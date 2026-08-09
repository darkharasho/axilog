# MCONDCAT Task 1 — report

**Status: DONE.** The condition-skill-id catalog is implemented, `classify`/`record`
are reworked on both the outgoing (`hit_stats`) and incoming (`defenses`) sides, and
every previously-divergent field is now EXACT against the real post-rework
dps.report export for **every** joined account (44/44). The committed pre-era
fixture is byte-identical to base `1fdf20c` across all seven output variants.

---

## 1. Catalog provenance

### 1.1 What GW2EI actually asks

`GW2EIEvtcParser/ParsedData/CombatEvents/SkillEvent.cs:43-50`:

```csharp
public bool ConditionDamageBased(ParsedEvtcLog log)
{
    if (_isCondi == -1 && log.Buffs.BuffsByIDs.TryGetValue(SkillID, out var b))
    {
        _isCondi = b.Classification == Buff.BuffClassification.Condition ? 1 : 0;
    }
    return _isCondi == 1;
}
```

A pure per-**skill-id** set-membership test. The `_isCondi == -1` guard is a
per-event memoisation cache with no semantic effect (a `BuffsByIDs` miss leaves the
field at `-1` and returns `false`, re-probing next call), so a plain set lookup is a
faithful reproduction.

Two call sites matter here, and **both probe the catalog FIRST**, ahead of the
`is DirectHealthDamageEvent` / `is NonDirectHealthDamageEvent` type test:

- outgoing: `GW2EIEvtcParser/EIData/Statistics/OffensiveStatistics.cs:109`
- incoming: `GW2EIEvtcParser/EIData/Statistics/DefensePerTargetStatistics.cs`
  (ctor transcription already in `analysis::defenses`'s module doc)

### 1.2 The complete static id set — exactly one group

An exhaustive tree scan for `BuffClassification.Condition` at a `Buff` **construction**
site (as opposed to a comparison/lookup site) finds **14 sites, all contiguous in one
list**:

| group | citation | entries |
|---|---|---|
| `CommonBuffs.Conditions` | `GW2EIEvtcParser/EIData/Buffs/CommonBuffs.cs:34-52` (ctor lines **36-49**) | 14 |

| name | symbol | id | `SkillIDs.cs` line | `CommonBuffs.cs` line |
|---|---|---|---|---|
| Blind         | `Blind`         | 720   | 123  | 41 |
| Crippled      | `Crippled`      | 721   | 124  | 43 |
| Chilled       | `Chilled`       | 722   | 125  | 42 |
| Poison        | `Poison`        | 723   | 126  | 39 |
| Immobile      | `Immobile`      | 727   | 129  | 45 |
| Bleeding      | `Bleeding`      | 736   | 130  | 36 |
| Burning       | `Burning`       | 737   | 131  | 37 |
| Vulnerability | `Vulnerability` | 738   | 132  | 49 |
| Weakness      | `Weakness`      | 742   | 135  | 47 |
| Fear          | `Fear`          | 791   | 146  | 44 |
| Confusion     | `Confusion`     | 861   | 151  | 38 |
| Torment       | `Torment`       | 19426 | 1328 | 40 |
| Slow          | `Slow`          | 26766 | 1587 | 46 |
| Taunt         | `Taunt`         | 27705 | 1609 | 48 |

Deliberately **excluded**: the 15th entry of that same list,
`"Number of Conditions"` (`CommonBuffs.cs:51`), which is tagged
`BuffClassification.Other` despite living in a list named `Conditions`. Its id is
`SkillIDs.cs:21`'s synthetic negative pseudo-id `-4`, which is structurally
unrepresentable in the `u32` skill ids this project decodes from the wire.

**No second group exists.** Notably, **not one** profession/elite-spec helper
(`EIData/ProfHelpers/*.cs`) registers a Condition-classified buff — they use
`Offensive`/`Defensive`/`Support`/`Debuff`/`Other` exclusively. Every other
`BuffClassification.Condition` occurrence in the tree is a READ, never a
registration:

- `EIData/Statistics/SupportStatistics.cs:53`
- `EIData/Statistics/DefensePerTargetStatistics.cs:150`
- `EIData/Statistics/StatisticsHelper.cs:26`
- `EIData/Statistics/GameplayStatistics.cs:131`
- `EIData/Actors/ActorsHelper/SingleActorBuffsHelper.cs:499,969`
- `ParsedData/CombatEvents/SkillEvent.cs:47`
- `GW2EIBuilders/JsonModels/JsonLogBuilder.cs:47`
- `GW2EIBuilders/HtmlModels/HtmlStats/BuffData.cs:129,151`,
  `BuffVolumeData.cs:103,116`

Source revision: `baaron4/GW2-Elite-Insights-Parser`, `master`, merge commit
`7a6fe03`, read 2026-08-09.

### 1.3 Machine diff

Extractor: grep every `new Buff(...)` line tree-wide carrying
`BuffClassification.Condition`, resolve each id symbol through `SkillIDs.cs`, sort,
and diff against the Rust `CONDITION_SKILL_IDS` table (resolved through its own
`pub const`s).

```
ctor sites tagged Condition: 14   (all CommonBuffs.cs:36..49)
GW2EI set (sorted): [720, 721, 722, 723, 727, 736, 737, 738, 742, 791, 861, 19426, 26766, 27705]
axilog set (sorted): [720, 721, 722, 723, 727, 736, 737, 738, 742, 791, 861, 19426, 26766, 27705]
table stored in ascending order: True
MISSING from axilog: []
EXTRA in axilog:     []
IDENTICAL:           True
```

Cross-checks run alongside: no multi-line `new Buff(` construction carries the
classification on a separate line (every `BuffClassification.Condition` occurrence
outside the 14 ctor lines was individually classified as a read, list above); the
only two `CreateCustomBuff(...)` call sites use `Nourishment`/`Enhancement`.

---

## 2. Membership-rule finding — runtime `BuffsByIDs` == the static set, unconditionally

This was the open question in the brief ("if membership depends on the log's
buff-info rather than a static set, reproduce THAT rule"). Answer: **it does not, for
Condition specifically** — and that is provable, not merely observed.

`BuffsByIDs` is built in `GW2EIEvtcParser/EIData/Buffs/BuffsContainer.cs:21-149`:

```csharp
foreach (IReadOnlyList<Buff> buffs in AllBuffs)                              // :96-99
    currentBuffs.AddRange(buffs.Where(x => x.Available(combatData)));
... unknown-consumable synthesis ...                                          // :110-140
BuffsByIDs = currentBuffs.GroupBy(x => x.ID).ToDictionary(...);               // :142-149
```

so membership IS log-dependent in general, via exactly two mechanisms. Neither can
touch the Condition set:

1. **`Buff.Available(combatData)`** (`Buff.cs:259-270`) —
   `gw2Build ∈ [_minBuild, _maxBuild) && evtcBuild ∈ [_minEvtcBuild, _maxEvtcBuild)`.
   Those bounds default to `StartOfLife`/`EndOfLife` (`Buff.cs:58-61`) and are only
   narrowed by an explicit chained `.WithBuilds(...)` / `.WithEvtcBuilds(...)`
   (`Buff.cs:125-137`). **None of the 14 Condition entries carries either call**
   (verified line-by-line, `CommonBuffs.cs:36-49`). Contrast the sibling Boon list,
   where `"Retaliation"` (`CommonBuffs.cs:27`) *does* carry
   `.WithBuilds(GW2Builds.StartOfLife, GW2Builds.May2021Balance)` — so this is a
   mechanism GW2EI genuinely uses, just never for conditions. All 14 are therefore
   unconditionally available on every build, every era, every log.
2. **Synthesised unknown consumables** (`BuffsContainer.cs:110-140`) — ids present in
   the log's own buff-info table but absent from the static lists get a
   `CreateCustomBuff(...)` entry, but *only* with `BuffClassification.Nourishment`
   (`:121`) or `.Enhancement` (`:137`). This path cannot add a Condition.

Two further guarantees make the set exact rather than approximate:

- the `GroupBy(x => x.ID)` at `:142-149` **throws** `InvalidDataException` on a
  duplicate id (except two sentinels, `NoBuff`/`Unknown`), so no other list can
  shadow a Condition id with a differently-classified buff of the same id;
- the `#if DEBUG` reclassification at `Buff.cs:81-86` rewrites only
  `Hidden -> Other`, never touching `Condition`.

**Conclusion:** a hardcoded 14-id table is a faithful reproduction of the runtime
rule, not a simplification. No era gating and no per-log probing are required, and
the module doc says so explicitly rather than leaving it implied.

---

## 3. What changed in the code

**New:** `crates/axilog-core/src/analysis/condition_catalog.rs` — the 14 ids (ascending),
`is_condition_damage_based(u32) -> bool`, and the full provenance/membership writeup
above in the module doc. Registered in `analysis/mod.rs`.

**Deduplicated:** `analysis::support`'s pre-existing local 14-id `CONDITION_IDS` table
(used for condition-cleanse counting) is now a `pub use` re-export of the catalog.
This is a real correctness win beyond bookkeeping: GW2EI's cleanse counter
(`SupportStatistics.cs:53`) reads `BuffsByClassification[Condition]` — literally the
same set — so the two consumers could previously have drifted apart. The old doc
comment there also cited the wrong list (`CommonBuffs.Boons`); corrected.

**`hit_stats::record`** now mirrors `OffensiveStatistics`'s ctor statement for
statement: catalog probe first, then `is NonDirectHealthDamageEvent`, with the power
counters incrementing across the whole `else` arm (so the fourth bucket feeds
`above90_power_*`, which it previously did not).

**`defenses::classify`/`record`** — `HitKind` gained a fourth variant, `PowerOnly`:

| wire shape | catalogued? | life-leech? | buckets incremented |
|---|---|---|---|
| `buff==0` | no  | –   | `strike_*` + `power_*` |
| any       | yes | –   | `condition_*` only |
| `buff==1` | no  | yes | `life_leech_*` + `power_*` |
| `buff==1` | no  | no  | **`power_*` only** ← the fourth bucket |

Note the "any" row: the catalog probe is **not** gated on the `buff` byte, because
GW2EI's is not. A `buff==0` strike row with a catalogued skill id is a condition hit
in GW2EI and never reaches the crit/critable/flank/glance block. Reproduced
verbatim, with a unit test pinning it in each module.

**Broken identities, deliberately.** `power == strike + life_leech` (incoming) and
`connected == direct + condition + life_leech` (outgoing) held only by axilog's own
pre-catalog three-bucket construction; neither holds in GW2EI. Both in-module
assertions were replaced by the correct inequality plus an explicit
fourth-bucket-population assertion, so the checks can never silently degrade back
into three-bucket tests.

**Derived-reference fix (the golden-test derivation the brief asked to flag).**
`defenses_golden.rs` and `axilog-ei`'s `ei_golden.rs` recovered the true life-leech
reference as `powerDamageTakenCount - strikeDamageTakenCount`. Post-catalog that
difference is `life_leech + fourth_bucket`, not `life_leech` — it was correct on the
committed fixture only by accident (that fixture is fourth-bucket-free; the local
capture is not, on 33 of 48 players). Both now use a **fourth-bucket-immune**
derivation that exploits GW2EI's own double-increment bug instead:

```
golden.lifeLeechDamageTaken == [true sum] + [true count]
  ⇒ assert ours.life_leech_damage + ours.life_leech_count == golden.lifeLeechDamageTaken
```

The old derivation is retained on the committed fixture only, as a cross-check that
the fixture is *still* fourth-bucket-free (with a failure message saying exactly
that). A guard also fires if `golden.lifeLeechDamageTakenCount` ever stops being 0,
i.e. if a future GW2EI release fixes the bug the identity depends on.

**Test-path plumbing.** `hit_stats_golden.rs`/`defenses_golden.rs` hardcoded
`fixtures/local/...` paths, so their local-capture calibration silently skipped in a
worktree. Both migrated to `tests/common`'s `local_fixture()` helper, i.e. they now
honour `AXILOG_LOCAL_FIXTURES` (no PII copied anywhere; the env var points at the
primary checkout). Unset, behaviour is unchanged.

---

## 4. Calibration

### 4.1 Post-era local capture — previously-divergent fields now EXACT

`AXILOG_LOCAL_FIXTURES=<primary>/fixtures/local cargo test -p axilog-core --test defenses_golden --test hit_stats_golden -- --nocapture`

```
defenses_calibrated_against_local_postrework_ei_json: 44 accounts joined,
  all 9 reliable count fields within tolerance 2 on a REAL post-era capture;
  0 catalog-gap-field note(s)
  [power_count / condition_count / power_damage / condition_damage now hard-asserted EXACT,
   plus the life-leech bug identity — all pass for all 44]

hit_stats_calibrated_against_local_postrework_ei_json: 44 accounts joined;
  all 8 reliable count fields within tolerance 2 and all 8 catalog-classified
  fields EXACT on a REAL post-era capture
  [condition_count/damage, life_leech_count/damage, above90_power_count/damage,
   above90_condition_count/damage — promoted from report-only to hard-failed]

defenses_present_and_sane_on_local_postrework: fourth bucket populated for
  33 player(s), 840 hit(s) total
hit_stats_present_and_sane_on_local_postrework: fourth bucket populated for
  2 player(s), 39 hit(s) total
```

The 33-vs-2 asymmetry matches the M13 prediction recorded in the module docs:
incoming attackers span an entire opposing WvW roster's build diversity, the
recording squad's own outgoing skill set is far narrower.

### 4.2 Accounts that flipped divergent → exact

Baseline measured by building `1fdf20c` in a temp worktree and running the same
hooks against the same capture. **Account names are replaced by indices; no PII in
this file.**

**Incoming (`defenses[0]`) — 33 of 44 joined accounts flipped, all now exact.**
"before" is axilog at `1fdf20c`; "golden" is the dps.report export (which is also the
"after" value, exactly, in every row).

| idx | `power_count` before → golden | `condition_count` before → golden | rel. err (power) |
|---|---|---|---|
| A03 | 34 → 70 | 71 → 35 | 51.4% |
| A07 | 36 → 72 | 64 → 28 | 50.0% |
| A10 | 46 → 85 | 125 → 86 | 45.9% |
| A08 | 57 → 105 | 123 → 75 | 45.7% |
| A12 | 68 → 122 | 176 → 122 | 44.3% |
| A29 | 45 → 78 | 90 → 57 | 42.3% |
| A17 | 59 → 101 | 122 → 80 | 41.6% |
| A31 | 49 → 76 | 62 → 35 | 35.5% |
| A32 | 98 → 149 | 154 → 103 | 34.2% |
| A28 | 64 → 95 | 71 → 40 | 32.6% |
| A16 | 74 → 109 | 88 → 53 | 32.1% |
| A09 | 73 → 106 | 96 → 63 | 31.1% |
| A13 | 64 → 90 | 58 → 32 | 28.9% |
| A26 | 64 → 88 | 56 → 32 | 27.3% |
| A21 | 89 → 122 | 114 → 81 | 27.0% |
| A23 | 75 → 102 | 58 → 31 | 26.5% |
| A33 | 76 → 103 | 67 → 40 | 26.2% |
| A20 | 34 → 46 | 52 → 40 | 26.1% |
| A15 | 96 → 129 | 110 → 77 | 25.6% |
| A02 | 137 → 182 | 169 → 124 | 24.7% |
| A04 | 85 → 112 | 83 → 56 | 24.1% |
| A01 | 43 → 55 | 37 → 25 | 21.8% |
| A11 | 71 → 89 | 71 → 53 | 20.2% |
| A24 | 101 → 125 | 82 → 58 | 19.2% |
| A30 | 77 → 95 | 61 → 43 | 18.9% |
| A06 | 76 → 91 | 55 → 40 | 16.5% |
| A19 | 48 → 54 | 45 → 39 | 11.1% |
| A27 | 76 → 84 | 86 → 78 | 9.5% |
| A18 | 70 → 76 | 43 → 37 | 7.9% |
| A22 | 85 → 91 | 74 → 68 | 6.6% |
| A05 | 71 → 74 | 46 → 43 | 4.1% |
| A14 | 112 → 116 | 108 → 104 | 3.4% |
| A25 | 122 → 123 | 108 → 107 | 0.8% |

Worst-case relative error before: **51.4%** on `power_count` (A03) — noticeably worse
than the ~35% the M13 disclosure had recorded, because that figure was read off the
largest *absolute* diff rather than the largest relative one. The remaining 11 joined
accounts were already exact before and remain exact.

Note the conservation the M13 doc predicted holds exactly in every row: the
`power_count` shortfall equals the `condition_count` excess, hit for hit (A03: −36 /
+36; A12: −54 / +54; …). This was pure misclassification, never a dropped or extra
event — which is precisely the signature of a missing catalog.

**Outgoing (`statsAll[0]`) — 2 of 44 joined accounts flipped, both now exact.**

| idx | field | before → golden |
|---|---|---|
| O2 | `condition_count` | 177 → 139 |
| O2 | `above90_power_count` | 412 → 450 |
| O2 | `above90_condition_count` | 160 → 122 |
| O1 | `condition_count` | 77 → 76 |
| O1 | `above90_power_count` | 158 → 159 |
| O1 | `above90_condition_count` | 74 → 73 |

(The index spaces are independent: `A*` = incoming, `O*` = outgoing.)

### 4.3 Committed pre-era fixture — byte-identical vs base `1fdf20c`

Zero fourth-bucket rows and zero catalogued-id `buff==0` rows exist in
`fixtures/wvw-small.anon.zevtc`, so the rework is a provable no-op there. Verified by
`cmp` against outputs generated from the tree at `1fdf20c` before any edit:

```
IDENTICAL out.json              (--format json)
IDENTICAL out.ei-json           (--format ei-json)
IDENTICAL out.html              (--format html)
IDENTICAL out.csv               (--format csv)
IDENTICAL out.table             (--format table)
IDENTICAL out.table-def         (--format table --view defense)
IDENTICAL out.ei-replay.json    (--format ei-json --replay)
```

All seven `cmp`s exit 0. No committed golden numbers needed changing, so the
`DONE_WITH_CONCERNS` escape hatch in the brief was not triggered.

### 4.4 Determinism, tests, clippy

- Determinism: two consecutive `--format ei-json` runs are byte-identical on both the
  committed pre-era fixture and the local post-era capture.
- Tests: `cargo test --workspace` → **578 passed, 0 failed**, identical with and
  without `AXILOG_LOCAL_FIXTURES` set (baseline 563 + 15 new/split tests).
- Clippy: `cargo clippy --workspace --all-targets` → **29** warning lines, matching
  the established baseline exactly. None of the 29 point at any file touched by this
  task except one pre-existing `unnecessary_get_then_check` in a `hit_stats` test at
  a line this task did not modify.
- Real-log sanity: `--view defense` renders plausibly on the post-era capture; the
  pre-era fixture is byte-identical (§4.3).

---

## 5. Consumer audit — other `buff == 1` readers

Every remaining `buff == 1` site in `axilog-core` was inspected. **None needs the
catalog**, and one actively must *not* use it:

| site | what `buff == 1` does there | verdict |
|---|---|---|
| `damage.rs:143,256,278` | selects `buff_dmg` vs `value` as the amount field | non-applicable — amount selection only, no bucketing |
| `skill_damage.rs:126,179,212` | same amount selection | non-applicable — see below |
| `timeseries.rs:218` | same amount selection | non-applicable |
| `contribution.rs:450` | same amount selection | non-applicable |
| `analysis/mod.rs:342` | same amount selection | non-applicable |
| `cc.rs:88` | same amount selection (CC duration) | non-applicable |
| `cc.rs:166` | `post_era \|\| e.buff == 0` — era gate for the CC result-byte enum | non-applicable — era gating, not classification |
| `healing.rs` | arcdps healing-extension rows; no condition/power split exists in EI's healing stats | non-applicable |

`skill_damage` deserves the explicit note, because it looks like a candidate.
GW2EI's per-skill `totalDamageDist` entries carry an `indirectDamage` flag that
splits one skill id into two entries — but that flag is
`dmList.Exists(x => x is NonDirectHealthDamageEvent)`
(`GW2EIBuilders/JsonModels/JsonActorUtilities/JsonDamageDistBuilder.cs:17`), i.e. the
**wire shape**, *not* `ConditionDamageBased`. Applying the catalog there would be an
active regression. (axilog does not emit `indirectDamage` at all today —
`axilog-ei/src/lib.rs:143` documents that omission — so nothing changes either way.)

The one genuine alignment found and fixed is `support.rs`, covered in §3.

---

## 6. Concerns / follow-ups

1. **`support::CONDITION_IDS` is now an alias.** Behaviour is unchanged (same 14 ids,
   verified by the green `support_golden` suite), but the public constant is now a
   re-export. Harmless, worth knowing if anything downstream pattern-matches on
   module paths.
2. **The life-leech reference depends on a GW2EI bug.** The new derivation is
   strictly better than the old one (fourth-bucket-immune) but still leans on the
   `LifeLeechDamageTaken` double-increment. A `lifeLeechDamageTakenCount != 0` guard
   now reports loudly if upstream ever fixes it.
3. **Not exercised: a catalogued id on a `buff==0` row.** GW2EI's ordering says it
   would count as a condition hit; both modules reproduce that and both have a unit
   test for it, but no real capture in hand contains such a row, so it remains
   source-verified rather than fixture-verified.
4. **`ProfHelpers` re-scan on GW2EI upgrades.** The catalog is provably complete at
   `7a6fe03`, and it has been stable for years, but a future GW2EI release could
   classify a new buff as `Condition`. The machine-diff script in §1.3 is the check to
   re-run; the `catalog_is_sorted_and_deduplicated` test keeps the table diffable.
