# axilog — MCONDCAT: Condition-skill-id classification catalog

**Status:** Approved (autonomous per docs/ROADMAP.md / [[axilog-autonomous-mandate]])
**Why:** The last EMPIRICALLY-CONFIRMED calibration gap. M13 documented (and the first real
post-era capture proved) that axilog's "every buff==1 non-life-leech hit is condition damage"
simplification diverges from GW2EI, which classifies by SKILL ID against its
`Buff.BuffClassification.Condition` catalog (`SkillEvent.cs:43-50` →
`log.Buffs.BuffsByIDs`). Real incoming-side divergence: ~2/3 of accounts, up to ~35% relative
on condition/power splits (outgoing side: 2/44 accounts). Every catalog-immune field is exact.
Fixing this makes the last report-only tolerance a hard EXACT assert.

## Scope

1. **The catalog** — transcribe GW2EI's complete set of buff ids registered with
   `BuffClassification.Condition` (find the authoritative definition site(s) in the GW2EI
   source — the Buff definition lists; cite file+lines per group). This is the M3-cleanse-set
   pattern: a complete static id set, machine-diffable against the GW2EI source.
2. **The fourth bucket** — rework the buff==1 classification in `hit_stats` (outgoing) and
   `defenses` (incoming): a buff==1 hit is Condition iff its skill id is in the catalog;
   life-leech stays as-is; a buff==1 hit outside the catalog that isn't life-leech becomes the
   fourth kind (power-but-not-strike — increments power counts only), exactly reproducing
   GW2EI's ctor logic already transcribed in `defenses.rs`'s module doc. Audit ALL other
   buff==1 classification consumers (skill_damage? timeseries? damage split docs) for the same
   simplification and align or document why unaffected.
3. **Gate flips** — the M13 golden tests' report-only tolerances on catalog-affected fields
   become hard EXACT asserts (post-era local calibration); the committed pre-era fixture stayed
   exact under the old simplification (zero fourth-bucket hits observed), so it must stay
   BYTE-IDENTICAL under the catalog too — assert explicitly. Update the long module docs in
   `defenses.rs`/`hit_stats.rs` (the simplification disclosures become "resolved by MCONDCAT"
   notes). README parity rows lose their catalog-gap caveats.

## Calibration

Post-era local (`fixtures/local/wvw-postrework.*`): the previously-divergent fields
(`condition_count/damage`, `power_count/damage`, above-90 condition splits, incoming
condition/power breakdown) become EXACT for every joined account — the divergent accounts are
the proof the catalog works. Committed pre-era fixture: byte-identical output across all
formats. All existing calibration exact. `power == strike + life_leech` equality is now
EXPECTED TO BREAK on rows with fourth-bucket hits — exactly as real GW2EI's does; update any
test asserting that identity to match the new (correct) semantics.

## Non-goals

Damage modifiers (M16), buff-simulation changes (the catalog is for damage-event
classification only), GW2-API-driven dynamic buff DBs (static transcription like M3).
