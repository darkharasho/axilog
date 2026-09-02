# Enemy cast-animation rows and the enemy-event filter

Note for the arcdps developer, 2026-09-02. Measured with axilog v1.11.0 over a
private corpus of **4,143 real WvW logs** (one squad's own recordings, builds
spanning the May-2026 rework). Reproduction instructions at the end.

**Summary.** In post-rework logs, `CBTS_ANIMATIONSTART` rows from enemy players
survive the enemy-event filter, and they survive with `dst_agent` populated. The
surviving set looks like a *census* of enemy casts aimed at the recording squad
rather than a sample. In pre-rework logs the same rows are absent entirely. And
`CBTS_ANIMATIONSTOP` never survives for those casters — not rarely, but exactly
zero times in 1.19M start rows. I would like to know whether this is intended
and whether it is safe to depend on, before I build further on it.

## 1. What survives, post-rework

Restricted to casters visible *only* through the enemy filter (see the method
note in §4), across 1,809 post-rework WvW logs, 1,799 of which carry at least
one enemy cast row:

| enemy `CBTS_ANIMATIONSTART` rows | count | share |
|---|---:|---:|
| `dst_agent` = a squad player | 1,087,485 | 91.7% |
| `dst_agent` = a squad player's pet/minion | 97,978 | 8.3% |
| `dst_agent` = anything else | 2 | ~0% |
| `dst_agent` = 0 | 0 | 0% |
| **enemy `CBTS_ANIMATIONSTOP` rows** | **0** | **0%** |

For scale, the same logs carry 5,485,690 squad `ANIMATIONSTART` rows and
6,299,928 enemy→squad strike-damage rows.

Two rows out of 1.19M had a `dst_agent` I could not tie to the squad. Every
other row is squad-directed, which is why "census" rather than "sample" seems
like the right description: the filter appears to be **dst-driven** for this
statechange — the row survives when the thing being aimed at is squad-adjacent.

The pet/minion share is worth noting on its own: `dst_master_instid` on those
rows resolves to a squad member, so ranger pets, necro minions and mesmer clones
are drawing a real 8% of aimed enemy casts. (Top targets by volume across the
corpus: Juvenile Brown Bear, Juvenile Polar Bear, Blood Fiend.)

## 2. The pre-rework logs show none of this

Same corpus, 2,334 pre-rework logs, cast rows identified the pre-rework way
(ordinary combat events with `is_activation` in `1..=2` for start, `3..=6` for
end):

| | count |
|---|---:|
| enemy cast START | **0** |
| enemy cast END | **0** |
| squad cast START (contrast) | 7,352,123 |
| enemy→squad strike-damage rows | 7,338,951 |

So this is not a case of the enemies being quiet or absent — 7.3M enemy damage
rows land on the squad in those same logs. Enemy cast rows simply are not there.
The census appears to have arrived *with* the move to the dedicated
`CBTS_ANIMATIONSTART`/`STOP` statechanges.

I want to be careful about the boundary claim: I split the corpus on build
`>= 20260501`, which is my own conservative proxy for the build where cast
animations became statechanges, not a boundary I have narrowed empirically. What
I can state precisely is that no log below that build in this corpus has a
single enemy cast row, and 1,799 of 1,809 above it do.

## 3. START without STOP

The zero in the STOP row of the §1 table is the part I would most like to ask
about. It is not a rounding artifact — across 1,809 logs and 1.19M surviving
start rows, the enemy-filtered casters produce no stop rows at all, while the
squad's own casters produce 5,489,118 of them in the same logs.

This reads as consistent with the dst-driven theory: a STOP row names the caster
in `src` and has nothing squad-side in `dst`, so a dst-driven filter would drop
every one. If that is what is happening, it is a coherent design rather than an
oversight.

The practical cost is that the surviving data is start-times only. No cast
durations, no completion-vs-cancel, and therefore no interrupt detection against
enemy casters. If retaining STOP rows for casters whose START was already
retained were cheap, it would turn a stream of "what is being aimed at us" into
"what was aimed at us and whether it landed or got interrupted" — but I have no
sense of what that costs on your side, and the volume argument against it is
obvious.

## 4. Method note — the apparent exceptions

An earlier pass of this measurement showed 235 STOP rows and 53 null-`dst` start
rows, which would have made the rule leaky. They have a common explanation, and
excluding them is what produces the exact zeros above.

arcdps emits `CBTS_BUFFINITIAL` only for agents it is tracking at full fidelity;
enemy-filtered agents never get it. In a handful of logs, an agent that arcdps
was tracking fully ended up in my *enemy* roster, because the only
`CBTS_TEAM_CHANGE` it emitted reported team `0`, which my team resolution treats
as an enemy. Those agents behave like squad members in the log (they carry
`BUFFINITIAL` and `ENTER_COMBAT` rows that no genuinely enemy-filtered agent
has), so their cast rows are not evidence about the enemy filter at all.

All 235 STOP rows and all 53 null-`dst` rows come from such agents — 0 from
enemy-filtered ones. They account for 8,373 of 1.19M start rows (0.7%). The
misclassification is mine, not arcdps's; I mention it only because it is what
stands between "approximately zero" and "exactly zero", and because anyone
repeating this measurement will hit it.

## 5. Why I care

I use the surviving rows to compute a per-player "focus index": share of aimed
enemy casts divided by an even `1/N` share. Validated against the commander as a
known-focused player, on three disjoint slices of ~1,400 logs, the commander
draws 1.92x / 1.86x / 1.47x versus a median squad member's 0.56x / 0.53x /
0.64x. It is a stable signal and there is no other way I know of to measure
enemy *intent* — as distinct from what landed, which the damage rows already
cover.

One negative result, in case it saves anyone the trouble: weighting each cast by
how hard its skill hits looked much better offline (2.05x vs 1.50x commander
separation in the 3s before a down, on 300 logs) and did not survive holdout on
unseen logs — it was never better than counting casts unweighted. I deleted it.

## 6. Questions

1. Is the dst-driven survival of `CBTS_ANIMATIONSTART` for enemy agents
   intentional, or a side effect of how the new statechange interacts with the
   enemy filter?
2. Is it stable enough to build on, or should I treat it as incidental and
   liable to change?
3. Is retaining `CBTS_ANIMATIONSTOP` for casters whose START already survived
   feasible, or does the filter's shape make that awkward?

I am not asking for a change — mainly I want to know whether I am reading the
mechanism correctly and whether depending on it is unwise.

## 7. Reproduction

`crates/axilog-core/examples/enemy_cast_census.rs` in the axilog tree produces
every number above:

```
cargo build --release --example enemy_cast_census -p axilog-core
find <arcdps.cbtlogs> -name '*.zevtc' > /tmp/logs.txt
./target/release/examples/enemy_cast_census /tmp/logs.txt
```

It takes a list file rather than argv so the totals are not split across `xargs`
batches, separates enemy-filtered from fully-tracked casters as described in §4,
and classifies each surviving row's `dst`. The corpus is private (real player
names); the counts are reproducible on any sufficiently large WvW capture.
