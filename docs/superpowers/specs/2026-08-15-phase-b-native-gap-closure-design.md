# Phase B — gaps native can close that ei-json can't

Status: design approved 2026-08-15, awaiting implementation plan.

## Context

The native-format program's destination is axibridge running entirely off
axilog's native 1.0 document, with `to_ei_json` kept as a thin, permanent
compat path for other consumers. Phase A absorbed ei-json's private inputs;
`to_ei_json(report, replay)` now renders from the native report alone.

Phase B closes the gaps that remain — the ones native can close *because* it
is not constrained by EI's shape. `axibridge:docs/axilog-cutover-report.md`
is the authoritative inventory of what axibridge actually reads, and its
follow-up list is the source of every item below.

### Scope correction

`docs/ROADMAP.md` lists four Phase B items. Two of them were closed by the
1.0 container after that line was written and need no work:

- **Enemy class as a field.** `EntityOut` already carries `profession` and
  `elite_spec` as separate fields on every player role, `EnemyPlayer`
  included (`v1/entities.rs:57,61`). Merging them into EI's single string is
  an ei-json artifact; native is already ahead.
- **Replay join keys.** `ReplayTracks.by_entity` is keyed by entity id, and
  `down`/`dead` intervals are exported on both `ReplayIntervals` and
  `ReplayTrack` (`v1/blocks/activity.rs:254-333`). The genuine remainder is
  `dc`, which is item 2 below.

A third listed item — the zone/map split and `encounterDuration` — is also
already native: `EncounterOut` carries `kind`, `map`, and `duration_ms` as
separate fields. axibridge re-splits them only because ei-json glues them
back together. There is no axilog-side work; the derivation disappears when
axibridge reads native.

That leaves four real items.

## Item 1 — widen `PerTargetDetail` from 7 fields to 22

### Problem

EI's `statsTargets[i][0]` has 38 fields. Native's `PerTargetDetail`
(`v1/blocks/damage.rs:134`) carries 7. axibridge fills 8 more from a
whole-fight `statsAll[0]` fallback (`OFFENSE_METRICS_STATS_ALL_FALLBACK` in
`packages/bridge-metrics/src/statsMetrics.ts`) and leaves 7 blank.

The fallback is not merely a stand-in — it is **wrong today**. `statsAll`
counts every hit the player landed, including NPCs, guards and siege, while
the per-target roster counts only the enumerated targets. On the real EI
payload measured in the cutover report that is 136 hits versus 63. Every
per-target crit rate, flank rate and glance rate axibridge renders is
computed over a denominator roughly twice as large as it should be. Nobody
has noticed because the numbers are plausible.

The report also records the fallback's boundary condition: "if a per-target
or per-enemy filter is ever introduced over these columns, or over the
`offenseTotals` rollup, the fallback must not be applied inside it — it
would report whole-fight figures under a filtered heading." Closing the
fields deletes both the wrong numbers and the trap.

### Decision

Widen `PerTargetDetail` to the 15 additional fields axibridge reads. Not the
full 38: the remaining 23 appear nowhere in the cutover report's read
surface, and inventing them is speculative work.

Existing 7 (unchanged): `connected_hits`, `connected_damage`,
`against_downed_count`, `downed`, `killed`, `interrupts`,
`downs_contribution_damage`.

New 15:

| Field | Source |
|---|---|
| `direct_damage` | per-target accumulator |
| `direct_count` | per-target accumulator |
| `crit_count` | per-target accumulator |
| `crit_damage` | per-target accumulator |
| `flank_count` | per-target accumulator |
| `glance_count` | per-target accumulator |
| `critable_direct_count` | per-target accumulator |
| `against_downed_damage` | per-target accumulator |
| `applied_crowd_control` | per-target CC accumulator |
| `applied_crowd_control_duration_ms` | per-target CC accumulator |
| `applied_crowd_control_downs_contribution` | per-target CC accumulator |
| `applied_crowd_control_duration_downs_contribution_ms` | per-target CC accumulator |
| `missed` | `dist_outcomes`, rolled up per target |
| `evaded` | `dist_outcomes`, rolled up per target |
| `blocked` | `dist_outcomes`, rolled up per target |
| `invulned` | `dist_outcomes`, rolled up per target |

`HitStatsEntity` (`v1/blocks/defense.rs`) already computes the whole-fight
version of the first eight, and `CcEntity` the CC pair, which is the
evidence that the per-target versions are a matter of keying an existing
accumulation by target rather than new analysis.

The field names above are the *quantities*, not the final spellings. Each
new field must take the name its whole-fight counterpart already uses, so
the per-target and whole-fight versions of one quantity are recognizably the
same thing: `HitStatsEntity`'s `crit_count`/`crit_damage`/`flank_count`/
`glance_count`/`direct_count`/`direct_damage`/`critable_direct_count`/
`against_downed_damage`, and `CcEntity`'s `applied_total` and
`applied_duration_ms` (whose down-contribution variants have no whole-fight
counterpart and take the `_downs_contribution` suffix `PerTargetDetail`
already uses on `downs_contribution_damage`).

`direct_damage` is deliberately *not* native's existing
`connected_direct_dmg`. The cutover report flags these as different
quantities; the new field must match EI's `directDmg` definition, and the
plan must state that definition explicitly before the field is added.

### Gating

**Keep the existing `--skill-damage` gate. Do not promote to always-on.**

Two facts settle this:

1. The per-target *pass* is already unconditional. `analyze()` computes
   `PlayerMetrics::per_target` on every parse, one shared scan
   (`axilog-schema/src/lib.rs:406`). The gate is a **serialization gate
   only**, so all 22 fields cost the same to compute as the current 7:
   nothing.
2. Always-on was measured and rejected. At **8** fields per pair, on the
   committed 41-player WvW fixture, the always-on variant grew the rendered
   HTML report from 260,520 to 407,826 bytes — **+56.5%**, far past the
   ~30% guideline every other block in this schema was measured against
   (`axilog-schema/src/lib.rs:395-404`). At 22 fields that payload is
   roughly 2.75× worse.

The four `dist_outcomes` fields are gated on the same flag, so the gates
already align — no new presence signal is needed, and `PerTarget.detail`'s
single `Option` remains the one unambiguous gate for the whole group.

Consequence, stated so it is not discovered later: a default parse
(`--format json`, no flags) still carries no per-target stats. That is
unchanged from today. axibridge already sets the flag, since it reads these
fields now. Closing the default-parse case is a separate conversation about
the HTML size budget, not Phase B.

## Item 2 — `dc` intervals on the replay block

### Problem

Native exports `down` and `dead` intervals but not `dc` — the
disconnected/not-yet-spawned segments GW2EI brackets with sentinel
positions. `analysis/replay.rs:358-381` notes that mid-fight despawn and
respawn `dc` segments were left out of scope for the cheap tier-1 pass.

`dc` is not cosmetic. It is a term in the active-position filter that every
EI distance metric depends on: a position is *active* only when the actor is
neither down, dead, nor disconnected. Item 3 cannot be computed correctly
without it. It is also one of the three reasons `axilog_ei::EiReplayInput`
had to survive Phase A as a named exception.

### Decision

Add `dc: Vec<(u64, u64)>` to `ReplayIntervals` and
`dc_intervals: Vec<(u64, u64)>` to `ReplayTrack`, matching the existing
`down`/`dead` naming on each struct.

Semantics to pin in `NATIVE-FORMAT.md`: a `dc` interval is `[start, end)` in
log-relative milliseconds, covering both the pre-spawn window before an
entity's first appearance and any mid-fight despawn/respawn gap. Endpoints
are exclusive at the end, unlike GW2EI's inclusive sentinel bracket — the
cutover report measured that difference at 6 of 6,894 samples (0.087%) and
attributes a small share of axibridge's current error to it. Choosing the
half-open convention here and documenting it is the fix.

## Item 3 — commander segments and engine-computed distance scalars

### Problem

axibridge derives `distToCom` and `stackDist` itself, in
`deriveDistanceScalars`, at a measured **3.7% / 4.3% mean error**. The
report calls that "the sum of the approximations" and names the dominant
one: "The commander reference is one player's whole track, not EI's
per-segment commander timeline… axilog's ei-json exposes only a boolean
`hasCommanderTag`." `CommanderOut` (`v1/entities.rs:112`) carries only
`variant` and `guid` — native is no better here than ei-json.

The remaining approximations are the squad centre being taken over
`players[]` rather than EI's `log.PlayerList`, pixel-grid rounding from the
resampled positions, and the inclusive `dc` bracket endpoints from item 2.

### Decision

Emit **both** the segments and the scalars.

**Segments.** `CommanderOut` grows `segments: Vec<(u64, u64)>` — the
half-open windows, in log-relative milliseconds, during which this entity
carried a commander tag.

Semantics to pin: a segment is `[tag-on, tag-off)`. Multiple simultaneous
commanders are possible in WvW; EI resolves the reference commander by squad
membership, not by who tagged first, and the plan must reproduce that rule
rather than assume a single commander.

**Scalars.** Two per-player fields in the replay block carrying EI's
`DistanceToCommander` and `DistanceToCenterOfSquad`, computed engine-side
under the exact semantics verified against GW2EI source in the report's §5:

- Iterate the actor's **active** polled positions — nulled while down, dead,
  or disconnected (which is why item 2 precedes this one).
- Pair each with the reference position at the **same poll timestamp**.
- Take the **XY-plane** length; Z is discarded.
- Arithmetic mean over the qualifying pairs.
- **`-1`** when nothing qualified. This is EI's sentinel and must be
  preserved, not translated to `null` — a consumer distinguishing "no
  overlap" from "zero distance" depends on it.
- The whole computation is gated on the replay pass having run, matching
  EI's `log.CanCombatReplay` gate. Use the format's established idiom: the
  scalars are `Option`, absent when the pass did not run.

These last two are **two distinct states and must not be collapsed**:
absent means the replay pass never ran, `-1` means it ran and this actor had
no qualifying poll. Emitting `-1` for the first case, or `None` for the
second, loses the distinction the gate-record idiom exists to preserve.

The commander reference is the commanding player's **raw** positions during
their commander segments — *not* active-filtered. The squad centre is the
per-poll mean of every player's **active** position. These two differ on
purpose in GW2EI; matching that asymmetry is what takes the error to zero.

### Why both

The segments are a prerequisite for computing the scalars correctly, so the
only question was whether to discard them afterward. Publishing them costs
one `Vec<(u64, u64)>` and makes the scalars auditable — a consumer can check
the arithmetic instead of trusting it — and it serves any consumer wanting a
distance to something we did not anticipate. The report's follow-up 4 asks
for the scalars "**or** a commander-segment timeline", where the *or* was a
concession to cost rather than a preference.

Emitting the scalars deletes `deriveDistanceScalars` from axibridge outright.

## Item 4 — log-start wall clock

### Problem

axibridge infers `timeStart` from the `.zevtc` file's mtime minus
`durationMS`. That is wrong for any copied, restored, or re-downloaded file.
The blast radius is small — the report notes it is "only ever consulted
after `uploadTime`" — but the fix is cheap and the inference is
indefensible once a real timestamp is available.

arcdps records one: `sc::LOG_START` is defined at
`axilog-core/src/evtc/event.rs:23` and **never read**. Grep finds two
references outside that file, both a doc comment and a test assertion.

### Decision

Extract the wall-clock timestamp from the `LOG_START` state-change event and
carry it on `EncounterOut` as `started_at_unix: Option<u64>`, seconds since
the epoch.

`Option` because a truncated or synthetic log may carry no `LOG_START` at
all, and absence must stay distinguishable from epoch zero.

**Open item for the plan, not for this design:** the exact payload field.
`RawEvent` exposes `time`, `src_agent`, `dst_agent`, `value`, `buff_dmg`.
arcdps documents `LOG_START` as carrying a server timestamp and a local
timestamp in two of these, but this repo's standing methodology is to cite
the curl'd `arcdps/evtc/README.txt` and cross-check against GW2EI's parser
before committing to an ordinal or a payload slot — the same trail
`analysis/health`'s module doc records for `HEALTHPCTUPDATE`. The plan's
first task for this item is that verification, and it must produce a
citation, not an assumption. Server time is the one to emit if both are
present; a client clock is not a fact about the log.

`EncounterOut` currently has no wall-clock field of any kind, so this is
purely additive.

## Sequencing

One branch, five stages, ordered by dependency:

1. **Item 1** — widen `PerTargetDetail`. Independent; largest diff; goes
   first so it is not waiting behind anything.
2. **Item 2** — `dc` intervals. Independent of item 1, prerequisite for
   item 3.
3. **Item 3a** — commander segments on `CommanderOut`.
4. **Item 3b** — distance scalars, consuming 2 and 3a.
5. **Item 4** — log-start wall clock. Independent; last because it carries
   an open verification question that should not gate the rest.

Items 2, 3a and 3b are a single dependency chain and must not be split
across branches: the scalars' inputs are the other two.

## Compatibility

Every change here is additive to native 1.0 except the `PerTargetDetail`
widening, which is additive within an already-gated struct. Under the
standing ruling that 1.0 stays malleable while its only reader is the
in-tree adapter, each lands without a major bump, and each gets an entry in
`docs/NATIVE-FORMAT.md` §"1.x compatibility rules" so a bisect can explain
the key-set golden diff.

The `to_ei_json` adapter stays thin. It gains the `statsTargets` fields it
can now fill honestly and drops nothing; the distance scalars and commander
segments have no EI-json destination and should not acquire one, since EI's
own JSON does not carry them per-player in this shape.

## Testing

- **Golden key-set diffs** for each of the four items, one per stage, so a
  bisect attributes a key change to a single commit.
- **ei-json goldens must not move** except for the `statsTargets` fields in
  item 1, which is the one place this phase intentionally adds keys to the
  EI payload. Any other movement is a bug.
- **Item 1 correctness:** assert on the committed WvW fixture that summing
  a player's per-target `connected_hits` across targets is ≤ that player's
  whole-fight `HitStatsEntity::connected_count`, and strictly less whenever
  the log contains non-enumerated targets. That inequality is exactly the
  bug the `statsAll` fallback embodies, so a test that pins it is a
  regression guard against reintroducing the fallback's semantics.
- **Item 2:** a fixture-backed assertion that a player with a mid-fight
  despawn has a non-empty `dc` and that `dc`, `down` and `dead` do not
  overlap.
- **Item 3:** validate the scalars against the report's measured EI values
  on the fixture it used. The target is exact agreement, not 3.7% — if the
  implementation lands inside the old error band rather than on the value,
  one of the four semantics above is wrong and the plan should treat that
  as a failure, not a pass.
- **Item 4:** assert `started_at_unix` is present on the committed fixture
  and falls in a sane range; assert absence, not zero, on a synthetic log
  with no `LOG_START`.

Scoped runs (`cargo test -p <crate> -q`) while iterating; the full workspace
suite once before merging. `cargo fmt --all` must not be run — this repo is
hand-formatted.

## Out of scope

- The remaining 23 EI `statsTargets` fields.
- Promoting per-target stats to an always-on serialization.
- Widening the native replay shape into GW2EI's (spec #1 decision 6). Item 2
  adds `dc` because the distance semantics require it, not as a step toward
  adopting GW2EI's replay format; orientations remain out.
- Retiring `axilog_ei::EiReplayInput`. Item 2 removes one of its three
  justifications; the other two stand.
- The axibridge-side reader rewrite (Phase D), which is the owner's.
