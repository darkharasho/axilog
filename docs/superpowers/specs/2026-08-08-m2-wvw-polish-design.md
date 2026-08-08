# axilog — M2: WvW Polish (deferred M1 items)

**Status:** Approved (autonomous continuation authorized by user 2026-08-08)
**Scope:** Close the deferred M1 gaps so the WvW report is complete and CI-verifiable. No new
subsystems; every item refines the existing pipeline.

## Items

1. **Profession & elite-spec names.** Map `prof`/`is_elite` codes to real names (GW2 API
   specialization ids). Native schema keeps both `profession` (base) and `elite_spec` (name).
   EI adapter emits EI-style naming. Calibrate against the EI golden fixture's `profession`
   values (EI reports the elite-spec name as the profession when elite is active).
2. **Real WvW team/map tables.** Map name from `MAP_ID` statechange (`src_agent` carries the map
   id; fixture: 95 = Green Alpine Borderlands, matching EI fightName). Team-id→color from EI's
   own published mapping (GW2EI source), replacing the wrong 883/882/881 placeholders (fixture
   friendly team id = 2767). `wvWMapData` in ei-json built from detected ids.
3. **CC metrics.** `cc_applied` and `cc_duration_ms` computed from `result==CROWD_CONTROL` events
   (value carries CC duration in ms). Calibration targets from EI golden: squad totals
   34 applications, 50,460 ms. Replace the overstack-based `is_cc` heuristic.
4. **Dedupe/attribution hardening.** Enemy players deduped across relogs (like friendly).
   Pet→owner attribution made time-aware (instid registrations can be reused); keep golden exact.
5. **PII-safe committed fixture.** An anonymizer that rewrites player name buffers in a raw
   `.zevtc` (account/character → anonymized, subgroup preserved) producing a committable
   `fixtures/wvw-small.anon.zevtc`; golden tests run against it in CI (no more silent skips).
   Verified to contain zero original account/character strings.

## Non-goals
Boons/support (M3), healing, rotations, SDKs, HTML.

## Correctness gates
Golden parity must stay exact (duration 49285, squad damage 2,138,414, friendly 42±) and gain:
CC totals within 2% of EI (34 / 50460), map name "Green Alpine Borderlands", all friendly players
mapped to non-numeric profession/elite names matching EI's profession set.
