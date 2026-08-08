# axilog — M4: Post-Rework Log Support (arcdps ≥ 20260501)

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Why now:** every freshly captured log uses the post-20260501 wire format. axilog currently
parses damage on those logs but emits zero boons/support (with a warning). For axibridge/axipulse
to run on axilog with live logs, the current era must be first-class.

## What changed at 20260501 (per GW2EI's version gates — verify each at implementation)

1. **Buff applies/removes became dedicated statechanges** (`BuffAppliesAndRemovesAsStateChanges`,
   statechange ordinals ~69-72 — verify by hand-count + GW2EI) instead of `is_statechange==0`
   combat events with `buff==1`. Payload layouts differ — GW2EI's event factory is the arbiter.
2. **Result enum rework** (`ResultEnumRework`): buff events route through the shared
   `DamageResult` enum; the retired `ConditionResult` is gone. Our CC predicate already carries a
   TODO for this (cc.rs).

## Scope

1. **Era-gated buff event extraction:** `extract_buff_events`/`extract_buff_capacities` dispatch
   on `is_post_buff_rework`: pre-era path unchanged (calibrated, frozen); post-era path decodes
   the new statechange events into the SAME `BuffEvent` model (the simulator and everything
   downstream is era-agnostic). Same for support (cleanses/strips) removal-event sourcing.
3. **CC predicate era-gating:** post-rework, verify whether real CC arrives with `buff==1`
   (shared enum) and extend `is_cc` accordingly per GW2EI's post-rework factory.
4. **Damage predicate audit:** confirm buff-damage (`buff==1` value/buff_dmg) semantics are
   unchanged post-rework for strike/condi damage accounting (GW2EI source).
5. **Synthetic-era test suite:** golden-style unit tests constructing post-era buff statechanges
   (apply/remove/initial/capacity) asserting identical simulator outcomes to equivalent pre-era
   sequences (era-equivalence tests — the strongest available check without a real fixture).
6. **Warning downgrade:** once post-era extraction exists, the M3 warning changes to fire only
   when a post-era log yields zero buff events AND zero buff statechanges (genuinely absent data).
7. **Real-log validation hook:** a `tests/postrework_golden.rs` scaffold that activates when
   `fixtures/local/wvw-postrework.zevtc` (+ optional EI JSON) appears — so the moment a fresh
   capture is provided, calibration is one file-drop away. Document in README how to provide it.

## Correctness gates
- All existing (pre-era) calibration stays EXACT — the pre-era path must be untouched.
- Era-equivalence: every synthetic pre-era sequence has a post-era twin producing identical
  timelines/uptimes/support counts.
- GW2EI source citations for every post-era payload field.

## Non-goals
Healing ext, SDKs, HTML, missile analytics (next milestones; SDK milestone follows — it is the
axibridge adoption path).
