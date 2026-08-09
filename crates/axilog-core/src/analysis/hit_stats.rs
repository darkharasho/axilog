//! Outgoing hit-quality stats (M13 Task 1) -- EI's `statsAll[0]`
//! (`GW2EIJSON.JsonStatistics.JsonGameplayStatsAll`, built by
//! `GW2EIEvtcParser.EIData.OffensiveStatistics`). Pure classification of
//! already-decoded outgoing damage events by `result` byte + `is_flanking`/
//! `is_moving`/`is_ninety`/`is_offcycle` flags -- no new wire, same
//! `is_statechange == 0 && is_activation == 0 && is_buffremove == 0 &&
//! src in squad && dst in enemies` event family every other outgoing-damage
//! pass in this codebase already scans.
//!
//! ## `result` byte enum -- verified (see `evtc::event::result`'s own doc
//! comments for the full arcdps README hand-count + GW2EI `DamageResult`
//! cross-check)
//!
//! Every `buff == 0` (direct/strike) row, on EVERY arcdps build era, and
//! every post-`ResultEnumRework` (>= build 20260501, `RawHeader::
//! is_post_buff_rework`) `buff == 1` row, decodes `result` through the SAME
//! unified `DamageResult`-shaped enum (`evtc::event::result`'s constants).
//! Pre-era `buff == 1` rows instead decode `result` through the separate,
//! narrower, now-retired `ConditionResult` enum (values 0-4 only; GW2EI:
//! `ExpectedToHit=0, InvulByBuff=1, InvulByPlayerSkill1=2,
//! InvulByPlayerSkill2=3, InvulByPlayerSkill3=4, Unknown=5+`) -- this is the
//! SAME pre/post-era split `cc::is_cc` already era-gates on; see that
//! function's doc comment for the full citation trail. `classify` below
//! reuses `RawHeader::is_post_buff_rework` the same way.
//!
//! ## "HasHit" (connected) -- per GW2EI's `DirectHealthDamageEvent`/
//! `NonDirectHealthDamageEvent` ctors (`GW2EIEvtcParser/ParsedData/
//! CombatEvents/DamageEvents/{Direct,NonDirect}HealthDamageEvent.cs`)
//!
//! - `buff == 0`: hit iff `result` is `NORMAL`/`CRIT`/`GLANCE` (0/1/2).
//!   `BLOCK`/`EVADE`/`ABSORB`/`BLIND`/`INVERT` (3/4/6/7/13) are NOT hits
//!   (zero effective damage, do not connect) but still exist as a
//!   damage-shaped row -- irrelevant here since `analysis::hit_stats` only
//!   ever counts HITS, so both "not hit" and "not even a damage-shaped row"
//!   collapse to the same outcome (`classify` returns `None`).
//!   `INTERRUPT`/`KILLING_BLOW`/`DOWNED`/`BREAKBAR_DAMAGE`/`ACTIVATION`/
//!   `CROWD_CONTROL` (5/8/9/10/11/12) are marker rows GW2EI routes to a
//!   `NoDamageHealthDamageEvent` (`IsNotADamageEvent = true`) -- excluded
//!   from every `OffensiveStatistics` counter this module mirrors.
//! - `buff == 1`, post-era (`DamageResult`): hit iff `result` is
//!   `BUFF_CYCLE`/`BUFF_NOT_CYCLE`/`BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT`
//!   (14/15/17) OR one of the two life-leech values (16/18, see below --
//!   `HasHit = ... || IsLifeLeech`). `ABSORB`/`INVERT` (6/13) are NOT hits
//!   (absorbed). Any other value (including every `buff == 0`-only DIRECT
//!   value 0-4/7, and the shared marker values 5/8/9/10/11/12) is DROPPED
//!   entirely by GW2EI's `AddBuffDamageDamageEvent` post-era switch
//!   (`default: break` -- no event object created at all) -- `classify`
//!   returns `None`.
//! - `buff == 1`, pre-era (`ConditionResult`): hit iff `result == 0`
//!   (`ExpectedToHit`). `result` 1-4 (invuln variants) are NOT hits but
//!   still a row (absorbed); `result >= 5` decodes as `Unknown` and is
//!   silently DROPPED by GW2EI's pre-era switch (`case ConditionResult.
//!   Unknown: break;` -- no event at all, exactly like `cc::is_cc`'s own
//!   documented pre-era `buff == 1` / byte-12 collision). Both "not hit"
//!   outcomes (1-4 and >=5) are indistinguishable for this module's
//!   purposes (`classify` returns `None` either way, since only `result ==
//!   0` counts as a hit).
//!
//! Unlike `analysis::damage::accumulate`'s own predicate (which additionally
//! skips `dmg == 0` events, since it only cares about the SUM), `classify`
//! deliberately does NOT skip zero-damage hits -- GW2EI's `HasHit`/
//! `ConnectedDamageCount` counts a hit regardless of whether `HealthDamage`
//! happens to be 0 (verified directly in `OffensiveStatistics`'s ctor: the
//! `DamageCount++`/`Damage += dl.HealthDamage` pair are two separate
//! statements, the count always increments on `HasHit`, the sum just adds
//! whatever `HealthDamage` is, including 0). A documented, deliberate
//! divergence from the established damage-sum predicate, needed for
//! COUNT-exactness against EI's own counters.
//!
//! ## Direct vs Condition vs Life-leech
//!
//! GW2EI's `OffensiveStatistics` ctor further splits every buff==1 HIT into
//! **life-leech** (`NonDirectHealthDamageEvent.IsLifeLeech`: pre-era, the
//! (repurposed-for-this-row-shape) `is_offcycle` byte decoded as the
//! retired `BuffCycle` enum equals `NotCycle_DamageToTargetOnHit` (3) or
//! `NotCycle_DamageToTargetOnStackRemove` (5); post-era, `result` itself is
//! `BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT` (16) or
//! `BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE` (18)) vs **condition**
//! (anything else classified `ConditionDamageBased` -- GW2EI checks the
//! skill id against its own Condition-category buff catalog). This project
//! has no equivalent skill-id condition-category catalog, so `classify`
//! instead treats every buff==1 hit that ISN'T life-leech as "condition" --
//! **verified to reproduce the calibration fixture EXACTLY**: cross-checked
//! all 41 players in `axibridge/test-fixtures/boon/20260117-181030.json`,
//! `connectedConditionCount + connectedLifeLeechCount + connectedDirectDamageCount
//! == connectedDamageCount` holds EXACTLY for every row (zero residual "buff==1,
//! neither condition nor life-leech" rows observed anywhere in this fixture)
//! -- i.e. there is no third bucket to misclassify on this log. A
//! genuinely different buff==1 damage-dealing skill outside GW2EI's
//! Condition catalog (a rare arcdps edge case, if any exists at all) would
//! be misclassified as "condition" by this simplification; documented, not
//! observed.
//!
//! ## `against_downed` -- a WIRE FLAG, not down-interval/health-state
//! tracking
//!
//! The arcdps EVTC reference itself documents this directly on the
//! `CBTS_COMBAT` payload block (`curl
//! https://www.deltaconnected.com/arcdps/evtc/README.txt`, 2026-08-09):
//! **"`is_offcycle: dst was downed at time of event`"** -- i.e. this is a
//! plain per-event flag arcdps itself computes, not something this project
//! needs to derive from `analysis::downs`/`analysis::replay`'s down-interval
//! tracking or `analysis::health::HealthTracker` (the M13 plan brief's
//! original guess). Cross-checked against GW2EI: `DirectHealthDamageEvent`
//! (buff==0, any era) and the post-era `NonDirectHealthDamageEvent` ctor
//! (buff==1) both read `AgainstDowned = evtcItem.IsOffcycle == 1` -- the
//! SAME already-decoded `RawEvent::is_offcycle` byte (offset 59) this
//! project already reads for the buff-EXTENSION-apply-row meaning elsewhere
//! (`RawEvent::is_offcycle`'s own doc comment) -- arcdps reuses this byte's
//! bit differently per event SHAPE, not a collision in practice (an
//! ordinary damage-dealing combat row is never also a buff-apply row).
//! **Pre-era `buff == 1` rows are the one exception**: GW2EI's pre-era
//! `NonDirectHealthDamageEvent(ConditionResult)` ctor instead reads
//! `AgainstDowned = evtcItem.Pad1 == 1`, where `Pad1` is the LOW byte of the
//! 4-byte `pad` field (`RawEvent::pad`, offset 60-63) -- i.e. `(pad &
//! 0xFF) == 1` -- NOT `is_offcycle` (which on a pre-era condition-tick row
//! instead carries the `BuffCycle` life-leech classification, see above).
//! This pre-era-only wire quirk is not documented in the arcdps README
//! text at all; GW2EI is the arbiter here per this project's established
//! verification policy (`event.rs`'s own doc comments already establish
//! this convention for other GW2EI-only-documented fields).
//!
//! ## `above90_power`/`above90_condition` -- the SOURCE's own health, NOT
//! the target's
//!
//! The arcdps README documents `is_ninety` on `CBTS_COMBAT` as **"src is
//! above 90% health"** -- the damage SOURCE's (attacker's) OWN health at
//! the moment of the hit, not the target's. Cross-checked against GW2EI:
//! `SkillEvent.IsOverNinety = evtcItem.IsNinety > 0`
//! (`GW2EIEvtcParser/ParsedData/CombatEvents/SkillEvent.cs:36`), consumed
//! directly by `OffensiveStatistics`'s `PowerDamageAbove90HPCount`/
//! `ConditionDamageAbove90HPCount` -- and independently corroborated by
//! GW2EI's own damage-modifier catalog, e.g. Guardian's "Unscathed
//! Contender" / Thief's "Twin Fangs" / Revenant's "Rising Tide" traits
//! (`EIData/ProfHelpers/*.cs`, `"7% if hp >=90%"`), all gated on
//! `x.IsOverNinety` and all balanced around the CASTER's own health, not
//! the target's. This directly supersedes the M13 plan brief's original
//! text ("target health >= 90% via the M11 `HealthTracker::percent_at`") --
//! `HealthTracker` is NOT used anywhere in this module; `is_ninety` is a
//! plain, already-arcdps-computed per-event flag on the SOURCE, decoded
//! from `RawEvent::is_ninety` (offset 53, newly decoded this task -- see
//! that field's own doc comment). "Power" here means "not condition"
//! (direct hits AND life-leech hits both count, matching GW2EI's own
//! `PowerDamageAbove90HPCount` increment site sitting OUTSIDE the
//! direct-vs-life-leech inner branch).
//!
//! ## Pet/minion damage is EXCLUDED here -- unlike `damage_total`/
//! `skill_damage`
//!
//! Every other outgoing-damage pass in this codebase
//! (`damage::accumulate_pet_credit`, `skill_damage::
//! accumulate_outgoing_pet_credit`, `cc::pet_credit_cc_events`) folds
//! friendly pet/minion damage onto the owning squad player, because EI's
//! `dpsAll[0].damage` (== this project's `damage_total`) does too. **EI's
//! `statsAll[0]` does NOT** -- `OffensiveStatistics`'s per-hit loop gates
//! every counter behind `dl.From.Is(actor.AgentItem)` (exact agent
//! identity, NOT `CreditedFrom`/`GetFinalMaster()`'s pet-to-owner fold),
//! matching `totalDamageDist`'s already-documented actor-only scope
//! (`skill_damage`'s own module doc). Verified directly against the
//! calibration fixture: 4 of 41 players have an active pet/minion
//! (`dpsAll[0].damage > statsAll[0].connectedDmg`, by exactly the SAME gap
//! `skill_damage_golden.rs` already documents between `dpsAll[0].damage`
//! and `sum(totalDamageDist)`) -- e.g. `HarborJet.5544`: `dpsAll=100089`
//! vs `statsAll.connectedDmg=85037` vs `totalDamageDist_sum=85037` (the
//! latter two EXACTLY equal, `dpsAll` strictly larger). So `hit_stats::
//! build` intentionally does NOT use `damage::InstidRegistry`/pet-credit at
//! all -- only a direct `src in squad && dst in enemies` scan, folded
//! across an account's own relog addrs via `addr_to_rep` (the same fold
//! `GetFinalMaster()` performs for a relogged account, whose raw addrs all
//! resolve to the same logical `AgentItem` identity in GW2EI's own model
//! too -- NOT the pet-to-owner fold, a materially different kind of
//! folding this module deliberately skips).
//!
//! ## `critable_direct_count` -- the real (small, stable) `NonCritableSkills`
//! table
//!
//! GW2EI gates `CriticalDamageCount`/`CritableDirectDamageCount` (but NOT
//! `DirectDamageCount`/`FlankingCount`/`GlancingCount`) behind
//! `SkillItem.CanCrit(skillId, gw2Build) = !NonCritableSkills.TryGetValue(
//! id, out var sinceBuild) || gw2Build < sinceBuild`
//! (`GW2EIEvtcParser/ParsedData/Skills/SkillItem.cs:128-134`). **First
//! attempted this project's earlier established simplification
//! (critable_direct_count == direct_count) -- calibration caught it**: 9 of
//! 37 joined accounts on the golden fixture (mostly Reaper/Spellbreaker/
//! Firebrand builds) have a nonzero `direct - critable` gap, up to 65 on
//! one account, nowhere near negligible. `NonCritableSkills`
//! (`GW2EIEvtcParser/ParsedData/Skills/SkillItemOverrides.cs:1525-1546`) is
//! a small (20-entry), STABLE, hardcoded skill-id table (each entry
//! effective from a specific patch build, e.g. Reaper's `ChillingNova` id
//! `29604` since `GW2Builds.December2018Balance` = build `94051`) -- every
//! threshold in the table is from a 2015-2018 patch, all far below any
//! build this project's WvW fixtures use (`>= 193778`), so on any
//! realistic modern log every one of these 20 ids is UNCONDITIONALLY
//! non-critable; `NON_CRITABLE_SKILLS` below reproduces the id set
//! directly (no build gating needed at this project's operating range --
//! documented, not silently assumed). Verified EXACT against the golden
//! fixture once applied.

use crate::evtc::{result, RawEvent, RawLog};
use std::collections::{BTreeMap, BTreeSet};

/// GW2EI's `SkillItemOverrides.NonCritableSkills` table (skill ids that can
/// never crit, each unconditionally in effect at this project's operating
/// build range -- see this module's `critable_direct_count` doc section for
/// the full citation/build-threshold writeup): `LightningStrike_SigilOfAir`
/// (9292), `FlameBlast_SigilOfFire` (9284), `FireAttunementSkill` (5492),
/// `Mug` (13014), `PulmonaryImpactSkill` (30770), `ConjuredSlashPlayer`
/// (52370), `LightningJolt` (31686), `Sunspot` (56883), `EarthenBlast`
/// (56885), `ChillingNova` (29604), `LesserBanishEnchantment` (28388),
/// `LesserEnfeeble` (13907), `SpitefulSpirit` (29560), `LesserSpinalShivers`
/// (13906), `PowerBlockDamage` (13752), `ShatteredAegis` (22499),
/// `GlacialHeart` (21795), `ThermalReleaseValve` (43630), `LossAversion`
/// (45534), `Epidemic` (10606).
const NON_CRITABLE_SKILLS: &[u32] = &[
    9292, 9284, 5492, 13014, 30770, 52370, 31686, 56883, 56885, 29604, 28388, 13907, 29560, 13906,
    13752, 22499, 21795, 43630, 45534, 10606,
];

/// `pub(crate)`: also reused by `analysis::skill_map` (M14, Task 2) for its
/// `SkillMapEntry::can_crit` field -- see that module's doc comment for the
/// citation trail (unchanged from this module's own, since it's the exact
/// same `NonCritableSkills` table).
pub(crate) fn can_crit(skillid: u32) -> bool {
    !NON_CRITABLE_SKILLS.contains(&skillid)
}

/// One squad player's outgoing hit-quality totals -- mirrors EI's
/// `statsAll[0]` field-for-field (see this module's doc comment for the
/// exact per-field derivation/citation). All-zero (`Default`) for a player
/// who dealt no outgoing damage to any enemy at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HitStats {
    pub crit_count: u32,
    pub crit_damage: u64,
    pub flank_count: u32,
    pub glance_count: u32,
    pub moving_count: u32,
    pub connected_count: u32,
    pub connected_damage: u64,
    pub direct_count: u32,
    pub direct_damage: u64,
    pub condition_count: u32,
    pub condition_damage: u64,
    pub critable_direct_count: u32,
    pub against_downed_count: u32,
    pub against_downed_damage: u64,
    pub life_leech_count: u32,
    pub life_leech_damage: u64,
    pub above90_power_count: u32,
    pub above90_power_damage: u64,
    pub above90_condition_count: u32,
    pub above90_condition_damage: u64,
}

impl HitStats {
    fn merge(&mut self, o: &HitStats) {
        self.crit_count += o.crit_count;
        self.crit_damage += o.crit_damage;
        self.flank_count += o.flank_count;
        self.glance_count += o.glance_count;
        self.moving_count += o.moving_count;
        self.connected_count += o.connected_count;
        self.connected_damage += o.connected_damage;
        self.direct_count += o.direct_count;
        self.direct_damage += o.direct_damage;
        self.condition_count += o.condition_count;
        self.condition_damage += o.condition_damage;
        self.critable_direct_count += o.critable_direct_count;
        self.against_downed_count += o.against_downed_count;
        self.against_downed_damage += o.against_downed_damage;
        self.life_leech_count += o.life_leech_count;
        self.life_leech_damage += o.life_leech_damage;
        self.above90_power_count += o.above90_power_count;
        self.above90_power_damage += o.above90_power_damage;
        self.above90_condition_count += o.above90_condition_count;
        self.above90_condition_damage += o.above90_condition_damage;
    }
}

/// One connected (hit) event's classification -- see this module's doc
/// comment for the full per-field derivation.
struct Classified {
    dmg: u64,
    is_direct_hit: bool,
    is_crit: bool,
    is_glance: bool,
    is_life_leech_hit: bool,
    is_against_downed: bool,
    is_against_moving: bool,
    is_above_ninety: bool,
}

/// Classify one already-filtered (non-statechange/activation/buffremove,
/// squad->enemy) event. Returns `None` for every non-hit outcome (blocked/
/// evaded/absorbed/blind/invert/dropped-unknown) AND every non-damage
/// marker row (interrupt/killing-blow/downed/breakbar/activation/CC) -- see
/// module doc's "HasHit" section for the exact per-era/per-buff enumeration.
fn classify(e: &RawEvent, post_era: bool) -> Option<Classified> {
    let dmg = if e.buff == 1 { e.buff_dmg.max(0) as u64 } else { e.value.max(0) as u64 };
    let is_against_moving = (e.is_moving & 2) != 0;
    let is_above_ninety = e.is_ninety != 0;

    if e.buff == 0 {
        // Direct/strike -- unified `DamageResult`-shaped enum on every era.
        let is_hit = matches!(e.result, result::NORMAL | result::CRIT | result::GLANCE);
        if !is_hit {
            return None;
        }
        return Some(Classified {
            dmg,
            is_direct_hit: true,
            is_crit: e.result == result::CRIT,
            is_glance: e.result == result::GLANCE,
            is_life_leech_hit: false,
            is_against_downed: e.is_offcycle == 1,
            is_against_moving,
            is_above_ninety,
        });
    }

    // buff == 1 -- era-gated (see module doc + `cc::is_cc`'s doc comment).
    if post_era {
        let is_life_leech = matches!(
            e.result,
            result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT
                | result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE
        );
        let is_hit = is_life_leech
            || matches!(
                e.result,
                result::BUFF_CYCLE
                    | result::BUFF_NOT_CYCLE
                    | result::BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT
            );
        if !is_hit {
            return None;
        }
        Some(Classified {
            dmg,
            is_direct_hit: false,
            is_crit: false,
            is_glance: false,
            is_life_leech_hit: is_life_leech,
            is_against_downed: e.is_offcycle == 1,
            is_against_moving,
            is_above_ninety,
        })
    } else {
        // Pre-era `buff == 1` rows are AMBIGUOUS between two totally
        // different event shapes on the wire (verified against GW2EI's
        // `CombatItem.IsBuffDamageEvent()`, `CombatItem.cs:226-238`, pre-
        // `ResultEnumRework` branch): a genuine damage TICK (`Value == 0`,
        // `BuffDmg` carries the tick amount) vs a boon/condition STACK
        // APPLICATION (`Value > 0`, the applied duration in ms, `BuffDmg ==
        // 0` -- the SAME predicate `analysis::buffs::events::
        // extract_buff_events`'s own `IsBuffApplyEvent` branch already
        // checks, `buff_dmg == 0 && value > 0`). Without this `Value == 0`
        // gate, every ordinary boon/condition APPLICATION (Might, Fury,
        // Vulnerability, Bleeding, ...) -- vastly more frequent than actual
        // condition damage ticks in a real fight -- would be misread as a
        // zero-damage "hit" (its `result` byte defaults to 0/
        // `ExpectedToHit` on an apply-shaped row, since `result` has no
        // apply-time meaning), wildly inflating `condition_count`/
        // `connected_count`/`moving_count`/`above90_condition_count`.
        // Caught via the golden calibration fixture (M13 Task 1): before
        // this gate, `connected_count`/`condition_count` were 1.5-6x too
        // high on nearly every pre-era account.
        if e.value != 0 {
            return None;
        }
        // Pre-era `ConditionResult`: only `result == 0` (ExpectedToHit) is a
        // hit; 1-4 (invuln variants) and >=5 (Unknown, silently dropped by
        // GW2EI) both collapse to "not a hit" here.
        if e.result != 0 {
            return None;
        }
        // Pre-era life-leech: the (repurposed) `is_offcycle` byte decoded as
        // the retired `BuffCycle` enum -- `NotCycle_DamageToTargetOnHit` (3)
        // or `NotCycle_DamageToTargetOnStackRemove` (5).
        let is_life_leech = matches!(e.is_offcycle, 3 | 5);
        // Pre-era against-downed: `Pad1` (low byte of `pad`), NOT
        // `is_offcycle` -- see module doc.
        let is_against_downed = (e.pad & 0xFF) == 1;
        Some(Classified {
            dmg,
            is_direct_hit: false,
            is_crit: false,
            is_glance: false,
            is_life_leech_hit: is_life_leech,
            is_against_downed,
            is_against_moving,
            is_above_ninety,
        })
    }
}

/// Fold one classified hit into its source's running `HitStats`.
fn record(stats: &mut HitStats, e: &RawEvent, c: &Classified) {
    stats.connected_count += 1;
    stats.connected_damage += c.dmg;

    if c.is_direct_hit {
        stats.direct_count += 1;
        stats.direct_damage += c.dmg;
        // GW2EI gates crit_count/critable_direct_count (but NOT
        // direct_count/flank_count/glance_count) behind CanCrit -- see
        // module doc's `critable_direct_count` section + `NON_CRITABLE_SKILLS`.
        if can_crit(e.skillid) {
            stats.critable_direct_count += 1;
            if c.is_crit {
                stats.crit_count += 1;
                stats.crit_damage += c.dmg;
            }
        }
        if c.is_glance {
            stats.glance_count += 1;
        }
        if e.is_flanking != 0 {
            stats.flank_count += 1;
        }
        if c.is_above_ninety {
            stats.above90_power_count += 1;
            stats.above90_power_damage += c.dmg;
        }
    } else if c.is_life_leech_hit {
        stats.life_leech_count += 1;
        stats.life_leech_damage += c.dmg;
        if c.is_above_ninety {
            stats.above90_power_count += 1;
            stats.above90_power_damage += c.dmg;
        }
    } else {
        // buff==1, hit, not life-leech -- "condition" (see module doc).
        stats.condition_count += 1;
        stats.condition_damage += c.dmg;
        if c.is_above_ninety {
            stats.above90_condition_count += 1;
            stats.above90_condition_damage += c.dmg;
        }
    }

    if c.is_against_downed {
        stats.against_downed_count += 1;
        stats.against_downed_damage += c.dmg;
    }
    if c.is_against_moving {
        stats.moving_count += 1;
    }
}

/// Per-raw-source-addr outgoing hit-quality totals, over `squad -> enemies`
/// events only (mirrors `damage::accumulate`'s squad/enemy membership
/// predicate, WITHOUT the pet-credit fold -- see module doc for why).
fn accumulate(
    events: &[RawEvent],
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    post_era: bool,
) -> BTreeMap<u64, HitStats> {
    let mut out: BTreeMap<u64, HitStats> = BTreeMap::new();
    for e in events {
        if e.is_statechange != 0 || e.is_activation != 0 || e.is_buffremove != 0 {
            continue;
        }
        if !squad.contains(&e.src_agent) || !enemies.contains(&e.dst_agent) {
            continue;
        }
        let Some(c) = classify(e, post_era) else { continue };
        let stats = out.entry(e.src_agent).or_default();
        record(stats, e, &c);
    }
    out
}

/// Compute per-squad-player outgoing hit-quality stats (M13 Task 1),
/// account-folded via `addr_to_rep` (relog fold ONLY -- deliberately no
/// pet/minion owner fold, see module doc).
pub fn build(
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
    addr_to_rep: &BTreeMap<u64, u64>,
) -> BTreeMap<u64, HitStats> {
    let post_era = raw.header.is_post_buff_rework();
    let by_addr = accumulate(&raw.events, squad, enemies, post_era);
    let mut by_rep: BTreeMap<u64, HitStats> = BTreeMap::new();
    for (addr, stats) in by_addr {
        let rep = addr_to_rep.get(&addr).copied().unwrap_or(addr);
        by_rep.entry(rep).or_default().merge(&stats);
    }
    by_rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawHeader, RawLog};

    fn base(src: u64, dst: u64) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: src,
            dst_agent: dst,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 1,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0,
            is_moving: 0,
            is_statechange: 0,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn direct(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = base(src, dst);
        e.result = result_;
        e.value = dmg;
        e
    }

    fn buff_dmg_event(src: u64, dst: u64, result_: u8, dmg: i32) -> RawEvent {
        let mut e = base(src, dst);
        e.buff = 1;
        e.result = result_;
        e.buff_dmg = dmg;
        e
    }

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn raw_post(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260501".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn squad1() -> BTreeSet<u64> {
        [1u64].into_iter().collect()
    }
    fn enemies9() -> BTreeSet<u64> {
        [9u64].into_iter().collect()
    }
    fn no_rep() -> BTreeMap<u64, u64> {
        BTreeMap::new()
    }

    // ---- direct (buff==0) result-byte classes ----

    #[test]
    fn direct_normal_is_connected_not_crit_not_glance() {
        let raw = raw_from(vec![direct(1, 9, result::NORMAL, 100)]);
        let s = build(&raw, &squad1(), &enemies9(), &no_rep());
        let h = s[&1];
        assert_eq!(h.connected_count, 1);
        assert_eq!(h.connected_damage, 100);
        assert_eq!(h.direct_count, 1);
        assert_eq!(h.direct_damage, 100);
        assert_eq!(h.critable_direct_count, 1);
        assert_eq!(h.crit_count, 0);
        assert_eq!(h.glance_count, 0);
    }

    #[test]
    fn direct_crit_counts_crit_and_direct_and_connected() {
        let raw = raw_from(vec![direct(1, 9, result::CRIT, 200)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.crit_count, 1);
        assert_eq!(h.crit_damage, 200);
        assert_eq!(h.direct_count, 1);
        assert_eq!(h.connected_count, 1);
    }

    #[test]
    fn direct_glance_counts_glance_and_direct_not_crit() {
        let raw = raw_from(vec![direct(1, 9, result::GLANCE, 50)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.glance_count, 1);
        assert_eq!(h.crit_count, 0);
        assert_eq!(h.direct_count, 1);
        assert_eq!(h.connected_count, 1);
    }

    #[test]
    fn direct_block_does_not_connect() {
        let raw = raw_from(vec![direct(1, 9, result::BLOCK, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default(), "blocked hit must not contribute anywhere");
    }

    #[test]
    fn direct_evade_does_not_connect() {
        let raw = raw_from(vec![direct(1, 9, result::EVADE, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn direct_absorb_does_not_connect() {
        let raw = raw_from(vec![direct(1, 9, result::ABSORB, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn direct_blind_does_not_connect() {
        let raw = raw_from(vec![direct(1, 9, result::BLIND, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn direct_invert_does_not_connect() {
        let raw = raw_from(vec![direct(1, 9, result::INVERT, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn interrupt_killing_blow_downed_breakbar_activation_cc_are_excluded() {
        let evs = vec![
            direct(1, 9, result::INTERRUPT, 999),
            direct(1, 9, result::KILLING_BLOW, 999),
            direct(1, 9, result::DOWNED, 999),
            direct(1, 9, result::BREAKBAR_DAMAGE, 999),
            direct(1, 9, result::ACTIVATION, 999),
            direct(1, 9, result::CROWD_CONTROL, 999),
        ];
        let raw = raw_from(evs);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default(), "marker/non-damage result rows must not contribute anywhere");
    }

    // ---- flags on a direct hit ----

    #[test]
    fn flanking_flag_counts_flank_only_on_direct_hits() {
        let mut e = direct(1, 9, result::NORMAL, 10);
        e.is_flanking = 1;
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.flank_count, 1);
    }

    #[test]
    fn against_moving_bit1_counts_moving_not_bit0() {
        let mut src_moving = direct(1, 9, result::NORMAL, 10);
        src_moving.is_moving = 1; // bit0 (src moving) -- must NOT count
        let mut dst_moving = direct(1, 9, result::NORMAL, 10);
        dst_moving.is_moving = 2; // bit1 (dst/target moving) -- must count
        let raw = raw_from(vec![src_moving, dst_moving]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.moving_count, 1, "only bit1 (against-moving) should count, not bit0 (src moving)");
    }

    #[test]
    fn is_ninety_flag_counts_above90_power_on_direct_hit() {
        let mut e = direct(1, 9, result::NORMAL, 40);
        e.is_ninety = 1;
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.above90_power_count, 1);
        assert_eq!(h.above90_power_damage, 40);
        assert_eq!(h.above90_condition_count, 0);
    }

    #[test]
    fn is_offcycle_flag_counts_against_downed_on_direct_hit() {
        let mut e = direct(1, 9, result::NORMAL, 77);
        e.is_offcycle = 1;
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.against_downed_count, 1);
        assert_eq!(h.against_downed_damage, 77);
    }

    // ---- pre-era buff==1 (ConditionResult) ----

    #[test]
    fn pre_era_condition_tick_expected_to_hit_counts_condition() {
        let raw = raw_from(vec![buff_dmg_event(1, 9, 0, 30)]); // ConditionResult::ExpectedToHit
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.condition_count, 1);
        assert_eq!(h.condition_damage, 30);
        assert_eq!(h.connected_count, 1);
        assert_eq!(h.direct_count, 0);
        assert_eq!(h.life_leech_count, 0);
    }

    #[test]
    fn pre_era_condition_invuln_variants_do_not_connect() {
        let evs = vec![
            buff_dmg_event(1, 9, 1, 999), // InvulByBuff
            buff_dmg_event(1, 9, 2, 999), // InvulByPlayerSkill1
            buff_dmg_event(1, 9, 3, 999), // InvulByPlayerSkill2
            buff_dmg_event(1, 9, 4, 999), // InvulByPlayerSkill3
        ];
        let raw = raw_from(evs);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn pre_era_unknown_result_byte_is_dropped_not_counted() {
        // result >= 5 is `Unknown` in the retired pre-era ConditionResult
        // enum (e.g. byte 12, which on a buff==0/post-era row would be
        // CROWD_CONTROL) -- must not be misread as a hit.
        let raw = raw_from(vec![buff_dmg_event(1, 9, result::CROWD_CONTROL, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn pre_era_life_leech_via_offcycle_buffcycle_value_3() {
        let mut e = buff_dmg_event(1, 9, 0, 25);
        e.is_offcycle = 3; // BuffCycle::NotCycle_DamageToTargetOnHit
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.life_leech_count, 1);
        assert_eq!(h.life_leech_damage, 25);
        assert_eq!(h.condition_count, 0);
    }

    #[test]
    fn pre_era_life_leech_via_offcycle_buffcycle_value_5() {
        let mut e = buff_dmg_event(1, 9, 0, 15);
        e.is_offcycle = 5; // BuffCycle::NotCycle_DamageToTargetOnStackRemove
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.life_leech_count, 1);
        assert_eq!(h.condition_count, 0);
    }

    #[test]
    fn pre_era_against_downed_via_pad1_low_byte_of_pad() {
        let mut e = buff_dmg_event(1, 9, 0, 60);
        e.pad = 1; // Pad1 = low byte of `pad` == 1
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.against_downed_count, 1);
        assert_eq!(h.against_downed_damage, 60);
    }

    #[test]
    fn pre_era_is_offcycle_alone_does_not_set_against_downed() {
        // Pre-era against-downed comes from `pad` (Pad1), NOT `is_offcycle`
        // -- is_offcycle==1 here is coincidentally also a valid BuffCycle
        // byte (NotCycle) but must not be misread as against-downed.
        let mut e = buff_dmg_event(1, 9, 0, 60);
        e.is_offcycle = 1;
        let raw = raw_from(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.against_downed_count, 0, "pre-era against-downed must come from pad/Pad1, not is_offcycle");
    }

    // ---- post-era buff==1 (DamageResult, unified) ----

    #[test]
    fn post_era_buff_cycle_counts_condition() {
        let raw = raw_post(vec![buff_dmg_event(1, 9, result::BUFF_CYCLE, 45)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.condition_count, 1);
        assert_eq!(h.condition_damage, 45);
    }

    #[test]
    fn post_era_buff_not_cycle_counts_condition() {
        let raw = raw_post(vec![buff_dmg_event(1, 9, result::BUFF_NOT_CYCLE, 20)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.condition_count, 1);
    }

    #[test]
    fn post_era_buff_not_cycle_dmg_to_source_on_hit_counts_condition_not_lifeleech() {
        let raw = raw_post(vec![buff_dmg_event(
            1,
            9,
            result::BUFF_NOT_CYCLE_DMG_TO_SOURCE_ON_HIT,
            12,
        )]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.condition_count, 1);
        assert_eq!(h.life_leech_count, 0);
    }

    #[test]
    fn post_era_life_leech_dmg_to_target_on_hit() {
        let raw = raw_post(vec![buff_dmg_event(
            1,
            9,
            result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT,
            33,
        )]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.life_leech_count, 1);
        assert_eq!(h.life_leech_damage, 33);
        assert_eq!(h.condition_count, 0);
    }

    #[test]
    fn post_era_life_leech_dmg_to_target_on_stack_remove() {
        let raw = raw_post(vec![buff_dmg_event(
            1,
            9,
            result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_STACK_REMOVE,
            18,
        )]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.life_leech_count, 1);
    }

    #[test]
    fn post_era_buff_absorb_and_invert_do_not_connect() {
        let raw = raw_post(vec![
            buff_dmg_event(1, 9, result::ABSORB, 999),
            buff_dmg_event(1, 9, result::INVERT, 999),
        ]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn post_era_direct_only_result_on_buff_row_is_dropped() {
        // A buff==1 row whose result byte is a DIRECT-only value (e.g.
        // NORMAL) is dropped entirely post-era (GW2EI's `default: break`).
        let raw = raw_post(vec![buff_dmg_event(1, 9, result::NORMAL, 999)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }

    #[test]
    fn post_era_against_downed_via_is_offcycle_not_pad() {
        let mut e = buff_dmg_event(1, 9, result::BUFF_CYCLE, 5);
        e.is_offcycle = 1;
        let raw = raw_post(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.against_downed_count, 1);
    }

    #[test]
    fn post_era_above90_condition_via_is_ninety() {
        let mut e = buff_dmg_event(1, 9, result::BUFF_CYCLE, 9);
        e.is_ninety = 1;
        let raw = raw_post(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.above90_condition_count, 1);
        assert_eq!(h.above90_condition_damage, 9);
        assert_eq!(h.above90_power_count, 0);
    }

    #[test]
    fn post_era_above90_power_via_is_ninety_on_life_leech() {
        let mut e = buff_dmg_event(1, 9, result::BUFF_NOT_CYCLE_DMG_TO_TARGET_ON_HIT, 9);
        e.is_ninety = 1;
        let raw = raw_post(vec![e]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.above90_power_count, 1, "life-leech counts as power, not condition");
        assert_eq!(h.above90_condition_count, 0);
    }

    // ---- misc plumbing ----

    #[test]
    fn zero_damage_hit_is_still_counted_as_connected() {
        // Deliberate divergence from `damage::accumulate`'s own `dmg == 0`
        // skip -- GW2EI counts HasHit regardless of the damage value.
        let raw = raw_from(vec![direct(1, 9, result::NORMAL, 0)]);
        let h = build(&raw, &squad1(), &enemies9(), &no_rep())[&1];
        assert_eq!(h.connected_count, 1);
        assert_eq!(h.connected_damage, 0);
    }

    #[test]
    fn pet_sourced_damage_is_not_folded_onto_owner() {
        // Unlike `damage::accumulate_pet_credit`, `hit_stats::build` must
        // NOT credit a non-squad source's damage to any squad player --
        // matches EI's own actor-only `statsAll` scope (see module doc).
        let raw = raw_from(vec![direct(300, 9, result::NORMAL, 500)]); // src 300 is not in squad
        let squad = squad1();
        let result_map = build(&raw, &squad, &enemies9(), &no_rep());
        assert!(result_map.get(&1).is_none() || result_map[&1] == HitStats::default());
        assert!(!result_map.contains_key(&300), "non-squad source must not get its own entry either");
    }

    #[test]
    fn relog_folds_across_account_addrs() {
        let squad: BTreeSet<u64> = [1u64, 2u64].into_iter().collect();
        let addr_to_rep: BTreeMap<u64, u64> = [(1u64, 1u64), (2u64, 1u64)].into_iter().collect();
        let raw = raw_from(vec![
            direct(1, 9, result::NORMAL, 10),
            direct(2, 9, result::CRIT, 20),
        ]);
        let h = build(&raw, &squad, &enemies9(), &addr_to_rep)[&1];
        assert_eq!(h.connected_count, 2);
        assert_eq!(h.connected_damage, 30);
        assert_eq!(h.crit_count, 1);
    }

    #[test]
    fn non_enemy_destination_is_ignored() {
        let raw = raw_from(vec![direct(1, 2, result::NORMAL, 999)]); // dst 2 is not an enemy
        let h = build(&raw, &squad1(), &enemies9(), &no_rep()).get(&1).copied().unwrap_or_default();
        assert_eq!(h, HitStats::default());
    }
}
